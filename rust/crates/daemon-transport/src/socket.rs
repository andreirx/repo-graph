//! Unix socket transport for NDJSON communication.
//!
//! Binds a Unix domain socket, accepts client connections, and processes
//! NDJSON requests per connection. The daemon stays alive independently
//! of client connections.
//!
//! ## Stale Socket Handling
//!
//! On startup, if the socket path exists:
//! 1. Attempt to connect (check if another daemon is alive)
//! 2. If connection succeeds: another daemon is running, refuse to start
//! 3. If connection fails: stale socket from crashed daemon, remove it
//!
//! ## Shutdown Cleanup
//!
//! On clean shutdown (via signal or programmatic stop):
//! - Remove the socket file
//!
//! On crash/kill -9:
//! - Socket file may remain (stale), handled by next startup

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::dispatch::{DispatchResult, Dispatcher, EmitError, ProgressEmitter};
use crate::envelope::{ErrorDetail, ErrorResponse, ProgressDetail, ProgressResponse, Request};
use crate::error::TransportError;

/// Configuration for the socket transport.
pub struct SocketConfig {
    /// Path to the Unix domain socket.
    pub socket_path: PathBuf,
}

impl SocketConfig {
    /// Create a new socket configuration.
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }
}

/// Result of attempting to bind the socket.
pub enum BindResult {
    /// Successfully bound the socket.
    Bound(UnixListener),
    /// Another daemon is already running at this socket.
    AlreadyRunning,
}

/// Attempt to bind the Unix socket with stale socket handling.
///
/// Returns `BindResult::AlreadyRunning` if another daemon is alive.
/// Returns `BindResult::Bound(listener)` on success.
/// Returns `Err` on I/O errors.
pub fn bind_socket(path: &Path) -> Result<BindResult, TransportError> {
    // Check if socket already exists
    if path.exists() {
        // Try to connect to check if another daemon is running
        match UnixStream::connect(path) {
            Ok(_) => {
                // Connection succeeded — another daemon is alive
                return Ok(BindResult::AlreadyRunning);
            }
            Err(_) => {
                // Connection failed — stale socket, remove it
                std::fs::remove_file(path).map_err(|e| {
                    TransportError::SocketBind(format!(
                        "failed to remove stale socket '{}': {}",
                        path.display(),
                        e
                    ))
                })?;
            }
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                TransportError::SocketBind(format!(
                    "failed to create socket directory '{}': {}",
                    parent.display(),
                    e
                ))
            })?;
        }
    }

    // Bind the socket
    let listener = UnixListener::bind(path).map_err(|e| {
        TransportError::SocketBind(format!("failed to bind socket '{}': {}", path.display(), e))
    })?;

    Ok(BindResult::Bound(listener))
}

/// Remove the socket file if it exists.
///
/// Called during clean shutdown. Errors are logged but not fatal.
pub fn cleanup_socket(path: &Path) {
    if path.exists() {
        if let Err(e) = std::fs::remove_file(path) {
            eprintln!(
                "warning: failed to remove socket '{}': {}",
                path.display(),
                e
            );
        }
    }
}

/// Request-scoped progress emitter that writes to a stream.
struct StreamEmitter<'a, W: Write> {
    request_id: &'a str,
    output: &'a mut W,
}

impl<'a, W: Write> StreamEmitter<'a, W> {
    fn new(request_id: &'a str, output: &'a mut W) -> Self {
        Self { request_id, output }
    }
}

impl<W: Write> ProgressEmitter for StreamEmitter<'_, W> {
    fn emit(&mut self, detail: ProgressDetail) -> Result<(), EmitError> {
        let response = ProgressResponse {
            id: self.request_id.to_string(),
            progress: detail,
        };

        let json = serde_json::to_string(&response).map_err(EmitError::from_serialize)?;
        writeln!(self.output, "{}", json).map_err(EmitError::from_io)?;
        self.output.flush().map_err(EmitError::from_io)?;

        Ok(())
    }
}

/// Handle a single client connection.
///
/// Reads NDJSON requests, dispatches them, writes responses.
/// Returns when the client disconnects or an error occurs.
fn handle_connection<D: Dispatcher>(stream: UnixStream, dispatcher: &D) {
    let peer = stream
        .peer_addr()
        .map(|a| format!("{:?}", a))
        .unwrap_or_else(|_| "unknown".to_string());

    // Clone stream for writing (UnixStream implements Read + Write)
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("failed to clone stream for {}: {}", peer, e);
            return;
        }
    };

    let reader = BufReader::new(stream);

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                // Client disconnected or read error
                if e.kind() != std::io::ErrorKind::UnexpectedEof {
                    eprintln!("read error from {}: {}", peer, e);
                }
                break;
            }
        };

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse and dispatch
        let response_json = parse_and_dispatch(&line, dispatcher, &mut writer);

        // Write final response
        if let Err(e) = writeln!(writer, "{}", response_json) {
            eprintln!("write error to {}: {}", peer, e);
            break;
        }

        if let Err(e) = writer.flush() {
            eprintln!("flush error to {}: {}", peer, e);
            break;
        }
    }
}

/// Parse a request line and dispatch it.
fn parse_and_dispatch<D: Dispatcher, W: Write>(
    line: &str,
    dispatcher: &D,
    output: &mut W,
) -> String {
    // Try to parse the request
    let request: Request = match serde_json::from_str(line) {
        Ok(req) => req,
        Err(e) => {
            let error_resp =
                ErrorResponse::for_unparseable(ErrorDetail::parse_error(e.to_string()));
            return serde_json::to_string(&error_resp).unwrap_or_else(|_| {
                r#"{"id":"unknown","error":{"code":"InternalError","message":"failed to serialize error"}}"#.to_string()
            });
        }
    };

    // Validate required fields
    if request.id.is_empty() {
        let error_resp = ErrorResponse::for_unparseable(ErrorDetail::invalid_request(
            "missing or empty 'id' field",
        ));
        return serde_json::to_string(&error_resp).unwrap();
    }

    if request.method.is_empty() {
        let error_resp = ErrorResponse::new(
            &request.id,
            ErrorDetail::invalid_request("missing or empty 'method' field"),
        );
        return serde_json::to_string(&error_resp).unwrap();
    }

    // Create request-scoped progress emitter
    let mut emitter = StreamEmitter::new(&request.id, output);

    // Dispatch the request
    let result = dispatcher.dispatch(&request, &mut emitter);

    // Serialize the final response
    match result {
        DispatchResult::Success(resp) => serde_json::to_string(&resp).unwrap(),
        DispatchResult::Error(resp) => serde_json::to_string(&resp).unwrap(),
    }
}

/// Run the socket transport loop.
///
/// Accepts connections on the Unix socket and handles requests.
/// Runs until `shutdown` is set to true or a fatal error occurs.
///
/// # Arguments
/// * `listener` - The bound Unix socket listener
/// * `dispatcher` - The request dispatcher
/// * `shutdown` - Atomic flag to signal shutdown
///
/// # Returns
/// * `Ok(())` on clean shutdown
/// * `Err(TransportError)` on fatal error
pub fn run_socket<D: Dispatcher>(
    listener: &UnixListener,
    dispatcher: &D,
    shutdown: &AtomicBool,
) -> Result<(), TransportError> {
    // Set non-blocking so we can check shutdown flag periodically
    listener
        .set_nonblocking(true)
        .map_err(|e| TransportError::SocketBind(format!("failed to set non-blocking: {}", e)))?;

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _addr)) => {
                // Set the client stream back to blocking for simpler I/O
                if let Err(e) = stream.set_nonblocking(false) {
                    eprintln!("warning: failed to set client stream blocking: {}", e);
                }
                handle_connection(stream, dispatcher);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No pending connection, sleep briefly and check shutdown
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                // Fatal accept error
                return Err(TransportError::SocketAccept(e.to_string()));
            }
        }
    }

    Ok(())
}

/// Run the socket transport with signal handling.
///
/// This is the main entry point for socket mode. Binds the socket,
/// installs signal handlers, and runs the accept loop.
///
/// # Arguments
/// * `config` - Socket configuration (path)
/// * `dispatcher` - The request dispatcher
///
/// # Returns
/// * `Ok(())` on clean shutdown
/// * `Err` if socket bind fails or another daemon is running
pub fn run_socket_transport<D: Dispatcher>(
    config: &SocketConfig,
    dispatcher: &D,
) -> Result<(), TransportError> {
    // Bind socket with stale handling
    let listener = match bind_socket(&config.socket_path)? {
        BindResult::Bound(l) => l,
        BindResult::AlreadyRunning => {
            return Err(TransportError::SocketBind(format!(
                "another daemon is already running at '{}'",
                config.socket_path.display()
            )));
        }
    };

    eprintln!("daemon listening on {}", config.socket_path.display());

    // Shutdown flag for signal handling
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);

    // Install signal handler for clean shutdown
    ctrlc::set_handler(move || {
        eprintln!("\nreceived shutdown signal");
        shutdown_clone.store(true, Ordering::Relaxed);
    })
    .map_err(|e| TransportError::SocketBind(format!("failed to install signal handler: {}", e)))?;

    // Run the accept loop
    let result = run_socket(&listener, dispatcher, &shutdown);

    // Clean up socket on shutdown
    cleanup_socket(&config.socket_path);

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::MockDispatcher;
    use std::io::{Read, Write};
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn bind_creates_socket() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let result = bind_socket(&path).unwrap();
        assert!(matches!(result, BindResult::Bound(_)));
        assert!(path.exists());
    }

    #[test]
    fn bind_detects_already_running() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        // First bind succeeds
        let _listener = match bind_socket(&path).unwrap() {
            BindResult::Bound(l) => l,
            BindResult::AlreadyRunning => panic!("unexpected AlreadyRunning"),
        };

        // Second bind detects already running
        let result = bind_socket(&path).unwrap();
        assert!(matches!(result, BindResult::AlreadyRunning));
    }

    #[test]
    fn bind_removes_stale_socket() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        // Create a stale socket file (not a real listener)
        std::fs::write(&path, "stale").unwrap();

        // Bind should remove stale and succeed
        let result = bind_socket(&path).unwrap();
        assert!(matches!(result, BindResult::Bound(_)));
    }

    #[test]
    fn cleanup_removes_socket() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        // Create socket
        let _listener = UnixListener::bind(&path).unwrap();
        assert!(path.exists());

        // Cleanup removes it
        cleanup_socket(&path);
        assert!(!path.exists());
    }

    #[test]
    fn socket_ping_pong() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let listener = match bind_socket(&path).unwrap() {
            BindResult::Bound(l) => l,
            _ => panic!("bind failed"),
        };

        let dispatcher = MockDispatcher::new();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);

        // Spawn server thread
        let path_clone = path.clone();
        let server = thread::spawn(move || run_socket(&listener, &dispatcher, &shutdown_clone));

        // Give server time to start
        thread::sleep(Duration::from_millis(50));

        // Connect as client
        let mut client = UnixStream::connect(&path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();

        // Send ping
        writeln!(client, r#"{{"id":"1","method":"ping"}}"#).unwrap();
        client.flush().unwrap();

        // Read response
        let mut response = String::new();
        let mut buf = [0u8; 1024];
        let n = client.read(&mut buf).unwrap();
        response.push_str(&String::from_utf8_lossy(&buf[..n]));

        assert!(response.contains(r#""id":"1""#));
        assert!(response.contains(r#""pong":true"#));

        // Signal shutdown
        shutdown.store(true, Ordering::Relaxed);
        drop(client);

        // Wait for server
        server.join().unwrap().unwrap();

        cleanup_socket(&path_clone);
    }

    #[test]
    fn socket_handles_multiple_requests() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let listener = match bind_socket(&path).unwrap() {
            BindResult::Bound(l) => l,
            _ => panic!("bind failed"),
        };

        let dispatcher = MockDispatcher::new();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);

        let path_clone = path.clone();
        let server = thread::spawn(move || run_socket(&listener, &dispatcher, &shutdown_clone));

        thread::sleep(Duration::from_millis(50));

        let mut client = UnixStream::connect(&path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();

        // Send multiple requests
        writeln!(client, r#"{{"id":"1","method":"ping"}}"#).unwrap();
        writeln!(client, r#"{{"id":"2","method":"echo","params":"hello"}}"#).unwrap();
        client.flush().unwrap();

        // Read responses - may come in multiple reads
        let mut response = String::new();
        let mut buf = [0u8; 2048];

        // Keep reading until we have at least 2 lines or timeout
        let start = std::time::Instant::now();
        while response.lines().count() < 2 && start.elapsed() < Duration::from_secs(2) {
            match client.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => response.push_str(&String::from_utf8_lossy(&buf[..n])),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }

        let lines: Vec<&str> = response.lines().collect();
        assert!(
            lines.len() >= 2,
            "expected at least 2 lines, got {}: {:?}",
            lines.len(),
            lines
        );
        assert!(lines[0].contains(r#""id":"1""#));
        assert!(lines[1].contains(r#""id":"2""#));

        shutdown.store(true, Ordering::Relaxed);
        drop(client);
        server.join().unwrap().unwrap();
        cleanup_socket(&path_clone);
    }
}
