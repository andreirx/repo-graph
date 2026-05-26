//! Unix socket transport implementation.
//!
//! This is the primary transport for daemon communication in normal
//! (non-sandboxed) shell environments.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::connection::DaemonClientError;
use super::transport::Transport;

/// Default connection timeout in seconds.
const CONNECTION_TIMEOUT_SECS: u64 = 2;

/// Default read timeout in seconds.
///
/// RMAPD-PERF-1: 300s for long-running operations.
const READ_TIMEOUT_SECS: u64 = 300;

/// NDJSON request envelope.
#[derive(Debug, Serialize)]
struct Request<'a> {
    id: &'a str,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

/// NDJSON response envelope.
#[derive(Debug, Deserialize)]
struct Response {
    id: String,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<ErrorDetail>,
    #[serde(default)]
    progress: Option<serde_json::Value>,
}

/// Error detail from daemon response.
#[derive(Debug, Deserialize)]
struct ErrorDetail {
    code: String,
    message: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// Classify an I/O error from socket read.
fn classify_read_error(e: std::io::Error, timeout_secs: u64) -> DaemonClientError {
    match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
            DaemonClientError::Timeout { timeout_secs }
        }
        _ => DaemonClientError::ReadFailed(e.to_string()),
    }
}

/// Unix socket transport.
///
/// Connects to the daemon via Unix domain socket.
pub struct SocketTransport {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl SocketTransport {
    /// Connect to the daemon at the given socket path.
    pub fn connect(socket_path: &Path) -> Result<Self, DaemonClientError> {
        // Check if socket exists first for better error messages
        if !socket_path.exists() {
            return Err(DaemonClientError::ConnectionFailed(format!(
                "socket does not exist: {}",
                socket_path.display()
            )));
        }

        // Connect - this is where EPERM/EACCES may occur in sandboxed shells
        let stream = UnixStream::connect(socket_path).map_err(|e| {
            DaemonClientError::ConnectionFailed(format!(
                "failed to connect to {}: {}",
                socket_path.display(),
                e
            ))
        })?;

        // Set timeouts
        stream
            .set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECS)))
            .map_err(|e| {
                DaemonClientError::ConnectionFailed(format!("failed to set read timeout: {}", e))
            })?;

        stream
            .set_write_timeout(Some(Duration::from_secs(CONNECTION_TIMEOUT_SECS)))
            .map_err(|e| {
                DaemonClientError::ConnectionFailed(format!("failed to set write timeout: {}", e))
            })?;

        // Clone for reader
        let reader_stream = stream.try_clone().map_err(|e| {
            DaemonClientError::ConnectionFailed(format!("failed to clone stream: {}", e))
        })?;

        Ok(Self {
            stream,
            reader: BufReader::new(reader_stream),
        })
    }

    /// Send request and read response (internal implementation).
    fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DaemonClientError> {
        let id = uuid::Uuid::new_v4().to_string();

        let request = Request {
            id: &id,
            method,
            params,
        };

        let request_json = serde_json::to_string(&request)
            .map_err(|e| DaemonClientError::SendFailed(format!("failed to serialize: {}", e)))?;

        writeln!(self.stream, "{}", request_json)
            .map_err(|e| DaemonClientError::SendFailed(e.to_string()))?;

        self.stream
            .flush()
            .map_err(|e| DaemonClientError::SendFailed(format!("failed to flush: {}", e)))?;

        // Read responses until we get a result or error
        loop {
            let mut line = String::new();
            self.reader
                .read_line(&mut line)
                .map_err(|e| classify_read_error(e, READ_TIMEOUT_SECS))?;

            if line.is_empty() {
                return Err(DaemonClientError::ReadFailed(
                    "daemon closed connection".to_string(),
                ));
            }

            let response: Response = serde_json::from_str(&line).map_err(|e| {
                DaemonClientError::InvalidResponse(format!(
                    "failed to parse: {} (line: {})",
                    e,
                    line.trim()
                ))
            })?;

            if response.id != id {
                return Err(DaemonClientError::InvalidResponse(format!(
                    "response ID mismatch: expected {}, got {}",
                    id, response.id
                )));
            }

            // Skip progress events
            if response.progress.is_some() {
                continue;
            }

            if let Some(error) = response.error {
                return Err(DaemonClientError::DaemonError {
                    code: error.code,
                    message: error.message,
                    data: error.data,
                });
            }

            return Ok(response.result.unwrap_or(serde_json::Value::Null));
        }
    }
}

impl Transport for SocketTransport {
    fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DaemonClientError> {
        self.send_request(method, params)
    }

    fn ping(&mut self) -> Result<(), DaemonClientError> {
        let result = self.request("ping", None)?;

        if result.get("pong") == Some(&serde_json::Value::Bool(true)) {
            Ok(())
        } else {
            Err(DaemonClientError::InvalidResponse(
                "ping did not return pong".to_string(),
            ))
        }
    }

    fn mode_name(&self) -> &'static str {
        "socket"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn connect_fails_for_missing_socket() {
        let result = SocketTransport::connect(Path::new("/nonexistent/path.sock"));
        assert!(matches!(
            result,
            Err(DaemonClientError::ConnectionFailed(_))
        ));
    }

    #[test]
    fn ping_succeeds_with_mock_daemon() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let listener = UnixListener::bind(&path).unwrap();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            let mut line = String::new();
            reader.read_line(&mut line).unwrap();

            let req: serde_json::Value = serde_json::from_str(&line).unwrap();
            let id = req["id"].as_str().unwrap();

            let response = format!(r#"{{"id":"{}","result":{{"pong":true}}}}"#, id);
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        let mut transport = SocketTransport::connect(&path).unwrap();
        let result = transport.ping();

        handle.join().unwrap();

        assert!(result.is_ok());
        assert_eq!(transport.mode_name(), "socket");
    }
}
