//! Low-level daemon socket connection.
//!
//! This module provides the transport adapter for CLI-to-daemon communication.
//! It handles socket connection, NDJSON serialization, and response parsing.
//!
//! ## Design Principles
//!
//! - Pure NDJSON over Unix socket, no human-readable text on the wire
//! - Same code path for health checks and production requests
//! - Connection errors are explicit, not silently swallowed
//! - No daemon logic here; this is transport only

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Default connection timeout in seconds.
const CONNECTION_TIMEOUT_SECS: u64 = 2;

/// Default read timeout in seconds.
///
/// RMAPD-PERF-1: Increased from 30s to 300s as mitigation for long-running
/// read operations (orient, check, trust, stats, cycles). Even with heartbeat
/// emission before queries, the queries themselves can exceed minutes on large
/// repos (django ~3000 files, duckdb ~5000 files).
///
/// The proper fix requires either:
/// - SQLite progress_handler callback for mid-query heartbeats
/// - Background thread execution with periodic heartbeat emission
///
/// Until then, 300s provides sufficient headroom for current corpus while
/// keeping failure detection under 5 minutes for true hangs.
///
/// See docs/slices/rmapd-perf-1-timeout.md.
const READ_TIMEOUT_SECS: u64 = 300;

/// Error returned when daemon communication fails.
#[derive(Debug)]
pub enum DaemonClientError {
    /// Could not connect to daemon socket.
    ConnectionFailed(String),
    /// Failed to send request to daemon.
    SendFailed(String),
    /// Failed to read response from daemon.
    ReadFailed(String),
    /// Response was not valid JSON.
    InvalidResponse(String),
    /// Daemon returned an error response.
    DaemonError {
        code: String,
        message: String,
        /// Optional structured data (e.g., ambiguous symbol matches).
        data: Option<serde_json::Value>,
    },
}

impl std::fmt::Display for DaemonClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "daemon connection failed: {}", msg),
            Self::SendFailed(msg) => write!(f, "failed to send request: {}", msg),
            Self::ReadFailed(msg) => write!(f, "failed to read response: {}", msg),
            Self::InvalidResponse(msg) => write!(f, "invalid response: {}", msg),
            Self::DaemonError { code, message, .. } => {
                write!(f, "daemon error [{}]: {}", code, message)
            }
        }
    }
}

impl std::error::Error for DaemonClientError {}

/// NDJSON request envelope (matches daemon-transport protocol).
#[derive(Debug, Serialize)]
struct Request<'a> {
    id: &'a str,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

/// NDJSON response envelope (matches daemon-transport protocol).
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
    /// Optional structured data (e.g., for AmbiguousSymbol errors).
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// A connection to the daemon socket.
pub struct DaemonConnection {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl DaemonConnection {
    /// Connect to the daemon at the given socket path.
    ///
    /// Returns `Err(ConnectionFailed)` if the socket doesn't exist or
    /// the daemon is not accepting connections.
    pub fn connect(socket_path: &Path) -> Result<Self, DaemonClientError> {
        // Check if socket exists first for better error messages
        if !socket_path.exists() {
            return Err(DaemonClientError::ConnectionFailed(format!(
                "socket does not exist: {}",
                socket_path.display()
            )));
        }

        // Connect with timeout
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

        // Clone for reader (UnixStream is both Read and Write)
        let reader_stream = stream.try_clone().map_err(|e| {
            DaemonClientError::ConnectionFailed(format!("failed to clone stream: {}", e))
        })?;

        Ok(Self {
            stream,
            reader: BufReader::new(reader_stream),
        })
    }

    /// Send a request and wait for the response.
    ///
    /// Progress events are silently consumed; only the final result/error is returned.
    pub fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DaemonClientError> {
        // Generate request ID
        let id = uuid::Uuid::new_v4().to_string();

        // Build request
        let request = Request {
            id: &id,
            method,
            params,
        };

        // Serialize to NDJSON
        let request_json = serde_json::to_string(&request)
            .map_err(|e| DaemonClientError::SendFailed(format!("failed to serialize: {}", e)))?;

        // Send request
        writeln!(self.stream, "{}", request_json)
            .map_err(|e| DaemonClientError::SendFailed(e.to_string()))?;

        self.stream
            .flush()
            .map_err(|e| DaemonClientError::SendFailed(format!("failed to flush: {}", e)))?;

        // Read responses until we get a result or error (skip progress events)
        loop {
            let mut line = String::new();
            self.reader
                .read_line(&mut line)
                .map_err(|e| DaemonClientError::ReadFailed(e.to_string()))?;

            if line.is_empty() {
                return Err(DaemonClientError::ReadFailed(
                    "daemon closed connection".to_string(),
                ));
            }

            // Parse response
            let response: Response = serde_json::from_str(&line).map_err(|e| {
                DaemonClientError::InvalidResponse(format!(
                    "failed to parse: {} (line: {})",
                    e,
                    line.trim()
                ))
            })?;

            // Verify request ID matches
            if response.id != id {
                // Unexpected response ID - protocol error
                return Err(DaemonClientError::InvalidResponse(format!(
                    "response ID mismatch: expected {}, got {}",
                    id, response.id
                )));
            }

            // Check for progress event (skip it, wait for final response)
            if response.progress.is_some() {
                continue;
            }

            // Check for error
            if let Some(error) = response.error {
                return Err(DaemonClientError::DaemonError {
                    code: error.code,
                    message: error.message,
                    data: error.data,
                });
            }

            // Return result
            return Ok(response.result.unwrap_or(serde_json::Value::Null));
        }
    }

    /// Send a ping request to verify the daemon is responsive.
    ///
    /// Uses the same code path as all other requests (no special handling).
    pub fn ping(&mut self) -> Result<(), DaemonClientError> {
        let result = self.request("ping", None)?;

        // Verify pong response
        if result.get("pong") == Some(&serde_json::Value::Bool(true)) {
            Ok(())
        } else {
            Err(DaemonClientError::InvalidResponse(
                "ping did not return pong".to_string(),
            ))
        }
    }
}

/// Check if the daemon is available at the given socket path.
///
/// This is a quick connectivity test that does NOT send any requests.
/// Use `ping()` for a full health check.
pub fn is_daemon_reachable(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }

    // Try to connect briefly
    match UnixStream::connect(socket_path) {
        Ok(stream) => {
            // Connection succeeded, daemon is accepting
            drop(stream);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn connection_fails_for_missing_socket() {
        let result = DaemonConnection::connect(Path::new("/nonexistent/path.sock"));
        assert!(matches!(
            result,
            Err(DaemonClientError::ConnectionFailed(_))
        ));
    }

    #[test]
    fn is_daemon_reachable_returns_false_for_missing_socket() {
        assert!(!is_daemon_reachable(Path::new("/nonexistent/path.sock")));
    }

    #[test]
    fn is_daemon_reachable_returns_true_for_listening_socket() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let _listener = UnixListener::bind(&path).unwrap();

        assert!(is_daemon_reachable(&path));
    }

    #[test]
    fn ping_succeeds_with_mock_daemon() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let listener = UnixListener::bind(&path).unwrap();

        // Spawn mock daemon
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            // Read request
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();

            // Parse to get ID
            let req: serde_json::Value = serde_json::from_str(&line).unwrap();
            let id = req["id"].as_str().unwrap();

            // Send pong response
            let response = format!(r#"{{"id":"{}","result":{{"pong":true}}}}"#, id);
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        // Connect and ping
        let mut conn = DaemonConnection::connect(&path).unwrap();
        let result = conn.ping();

        handle.join().unwrap();

        assert!(result.is_ok());
    }

    #[test]
    fn request_returns_error_on_daemon_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let listener = UnixListener::bind(&path).unwrap();

        // Spawn mock daemon that returns error
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            let mut line = String::new();
            reader.read_line(&mut line).unwrap();

            let req: serde_json::Value = serde_json::from_str(&line).unwrap();
            let id = req["id"].as_str().unwrap();

            let response = format!(
                r#"{{"id":"{}","error":{{"code":"TestError","message":"test failure"}}}}"#,
                id
            );
            writeln!(stream, "{}", response).unwrap();
            stream.flush().unwrap();
        });

        let mut conn = DaemonConnection::connect(&path).unwrap();
        let result = conn.request("test", None);

        handle.join().unwrap();

        match result {
            Err(DaemonClientError::DaemonError { code, message, .. }) => {
                assert_eq!(code, "TestError");
                assert_eq!(message, "test failure");
            }
            other => panic!("expected DaemonError, got {:?}", other),
        }
    }
}
