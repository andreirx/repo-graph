//! Shared LSP subprocess transport for language server adapters.
//!
//! This crate provides the common transport machinery for language servers
//! that speak LSP over stdin/stdout:
//!
//! - Content-Length framing (LSP specification)
//! - Blocking stdin/stdout subprocess session
//! - Reader thread with channel timeout enforcement
//! - Request/response ID correlation
//! - Notification skipping
//! - Process exit detection
//!
//! # Architecture
//!
//! This is an outer-layer support module. It does NOT own:
//! - Tool-specific requests (hover, completion, etc.)
//! - Response parsing/type extraction
//! - Project grouping
//! - Type validation heuristics
//!
//! Those belong in the resolver adapters that use this crate.
//!
//! # Usage
//!
//! ```ignore
//! use lsp_subprocess::{IdGenerator, ReaderHandle, write_request, write_notification};
//! use std::process::{Command, Stdio};
//!
//! // Spawn LSP server
//! let mut process = Command::new("my-language-server")
//!     .stdin(Stdio::piped())
//!     .stdout(Stdio::piped())
//!     .spawn()?;
//!
//! let mut stdin = process.stdin.take().unwrap();
//! let stdout = process.stdout.take().unwrap();
//!
//! // Start reader thread
//! let reader = ReaderHandle::spawn(stdout);
//! let mut ids = IdGenerator::new();
//!
//! // Send request
//! let id = ids.next();
//! write_request(&mut stdin, id, "initialize", init_params)?;
//!
//! // Wait for response with timeout
//! let response = reader.recv_response::<InitializeResult>(id, Duration::from_secs(30))?;
//! ```

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

/// Errors from LSP transport operations.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid header: {0}")]
    InvalidHeader(String),

    #[error("missing content-length")]
    MissingContentLength,

    #[error("timeout after {0:?}")]
    Timeout(Duration),

    #[error("process exited")]
    ProcessExited,

    #[error("reader thread terminated")]
    ReaderDead,

    #[error("channel error: {0}")]
    Channel(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// ID Generator
// ─────────────────────────────────────────────────────────────────────────────

/// LSP JSON-RPC message ID counter.
pub struct IdGenerator {
    next: i32,
}

impl IdGenerator {
    pub fn new() -> Self {
        Self { next: 1 }
    }

    pub fn next(&mut self) -> i32 {
        let id = self.next;
        self.next += 1;
        id
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Write Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Write an LSP request to stdin.
///
/// Formats the request with Content-Length header per LSP specification.
pub fn write_request<P: Serialize>(
    stdin: &mut ChildStdin,
    id: i32,
    method: &str,
    params: P,
) -> Result<(), TransportError> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let body_str = serde_json::to_string(&body)?;
    let header = format!("Content-Length: {}\r\n\r\n", body_str.len());

    stdin.write_all(header.as_bytes())?;
    stdin.write_all(body_str.as_bytes())?;
    stdin.flush()?;

    Ok(())
}

/// Write an LSP notification to stdin (no id, no response expected).
///
/// Formats the notification with Content-Length header per LSP specification.
pub fn write_notification<P: Serialize>(
    stdin: &mut ChildStdin,
    method: &str,
    params: P,
) -> Result<(), TransportError> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    let body_str = serde_json::to_string(&body)?;
    let header = format!("Content-Length: {}\r\n\r\n", body_str.len());

    stdin.write_all(header.as_bytes())?;
    stdin.write_all(body_str.as_bytes())?;
    stdin.flush()?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Read Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Read a single LSP message from a BufReader.
///
/// Blocks until a complete message is received.
/// Implements Content-Length framing per LSP specification.
fn read_message_blocking(reader: &mut BufReader<ChildStdout>) -> Result<Value, TransportError> {
    // Read headers until empty line
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Err(TransportError::ProcessExited);
        }

        // Trim CRLF
        let line = line.trim_end_matches(['\r', '\n']);

        if line.is_empty() {
            // End of headers
            break;
        }

        // Parse header
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse()
                    .map_err(|_| TransportError::InvalidHeader(line.to_string()))?,
            );
        }
    }

    let length = content_length.ok_or(TransportError::MissingContentLength)?;

    // Read body
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    let value: Value = serde_json::from_slice(&body)?;
    Ok(value)
}

// ─────────────────────────────────────────────────────────────────────────────
// Response Types
// ─────────────────────────────────────────────────────────────────────────────

/// LSP response structure.
#[derive(Debug)]
pub struct LspResponse<T> {
    /// Response ID (matches request ID).
    #[allow(dead_code)]
    pub id: i32,
    /// Successful result, if any.
    pub result: Option<T>,
    /// Error, if any.
    pub error: Option<LspError>,
}

/// LSP error structure.
#[derive(Debug, Clone)]
pub struct LspError {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Reader Thread
// ─────────────────────────────────────────────────────────────────────────────

/// A message read by the reader thread.
#[derive(Debug)]
pub enum ReaderMessage {
    /// Successfully read a JSON-RPC message.
    Message(Value),
    /// Error reading.
    Error(String),
    /// Reader is shutting down.
    Shutdown,
}

/// Handle to a background reader thread.
///
/// The reader thread continuously reads LSP messages and sends them
/// through a channel. Callers use `recv_response` to wait for responses
/// with real timeout enforcement.
///
/// # Timeout Enforcement
///
/// Unlike blocking reads on stdout, this design uses a reader thread
/// with channel timeout (`recv_timeout`). This provides true timeout
/// enforcement even when the LSP server is slow or hung.
pub struct ReaderHandle {
    rx: Receiver<ReaderMessage>,
    #[allow(dead_code)]
    thread: Option<JoinHandle<()>>,
}

impl ReaderHandle {
    /// Spawn a reader thread for the given stdout.
    pub fn spawn(stdout: ChildStdout) -> Self {
        let (tx, rx) = mpsc::channel();

        let thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message_blocking(&mut reader) {
                    Ok(msg) => {
                        if tx.send(ReaderMessage::Message(msg)).is_err() {
                            // Receiver dropped — exit
                            break;
                        }
                    }
                    Err(TransportError::ProcessExited) => {
                        let _ = tx.send(ReaderMessage::Shutdown);
                        break;
                    }
                    Err(e) => {
                        let _ = tx.send(ReaderMessage::Error(e.to_string()));
                        // Continue trying to read
                    }
                }
            }
        });

        Self {
            rx,
            thread: Some(thread),
        }
    }

    /// Wait for a response with the given ID, with timeout.
    ///
    /// Discards notifications and responses with other IDs.
    ///
    /// # Returns
    ///
    /// - `Ok(LspResponse)` if a response with matching ID is received
    /// - `Err(TransportError::Timeout)` if timeout is reached
    /// - `Err(TransportError::ProcessExited)` if the LSP server exits
    /// - `Err(TransportError::ReaderDead)` if the reader thread terminates
    pub fn recv_response<T: DeserializeOwned>(
        &self,
        expected_id: i32,
        timeout: Duration,
    ) -> Result<LspResponse<T>, TransportError> {
        let deadline = std::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(TransportError::Timeout(timeout));
            }

            match self.rx.recv_timeout(remaining) {
                Ok(ReaderMessage::Message(msg)) => {
                    // Check if this is a response with the expected ID
                    if let Some(id) = msg.get("id").and_then(|v| v.as_i64()) {
                        if id as i32 == expected_id {
                            let error = msg.get("error").and_then(|e| {
                                Some(LspError {
                                    code: e.get("code")?.as_i64()? as i32,
                                    message: e.get("message")?.as_str()?.to_string(),
                                })
                            });

                            let result = if let Some(r) = msg.get("result") {
                                serde_json::from_value(r.clone()).ok()
                            } else {
                                None
                            };

                            return Ok(LspResponse {
                                id: expected_id,
                                result,
                                error,
                            });
                        }
                        // Response for different ID — discard
                    }
                    // Notification — discard
                }
                Ok(ReaderMessage::Error(e)) => {
                    return Err(TransportError::Channel(e));
                }
                Ok(ReaderMessage::Shutdown) => {
                    return Err(TransportError::ProcessExited);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(TransportError::Timeout(timeout));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(TransportError::ReaderDead);
                }
            }
        }
    }

    /// Check if there's a message available without blocking.
    ///
    /// Returns None if no message ready, Some(msg) if available.
    #[allow(dead_code)]
    pub fn try_recv(&self) -> Option<ReaderMessage> {
        self.rx.try_recv().ok()
    }
}

impl Drop for ReaderHandle {
    fn drop(&mut self) {
        // Thread will exit when stdout closes (process killed)
        // We don't join here to avoid blocking on a potentially hung read
        if let Some(thread) = self.thread.take() {
            drop(thread); // Detach
        }
    }
}

#[cfg(test)]
impl ReaderHandle {
    /// Create a ReaderHandle from a channel receiver (for testing).
    pub fn from_receiver(rx: Receiver<ReaderMessage>) -> Self {
        Self { rx, thread: None }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn test_id_generator() {
        let mut gen = IdGenerator::new();
        assert_eq!(gen.next(), 1);
        assert_eq!(gen.next(), 2);
        assert_eq!(gen.next(), 3);
    }

    #[test]
    fn test_recv_response_timeout() {
        // Create a channel but never send anything
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Should timeout
        let result: Result<LspResponse<serde_json::Value>, _> =
            reader.recv_response(1, Duration::from_millis(50));

        assert!(matches!(result, Err(TransportError::Timeout(_))));

        // Prevent tx from being dropped before the test completes
        drop(tx);
    }

    #[test]
    fn test_recv_response_success() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Send a response with matching ID
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {"foo": "bar"}
        });
        tx.send(ReaderMessage::Message(response)).unwrap();

        // Should receive successfully
        let result: Result<LspResponse<serde_json::Value>, _> =
            reader.recv_response(42, Duration::from_secs(1));

        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, 42);
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_recv_response_wrong_id_then_correct() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Send a response with wrong ID first
        let wrong_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "result": null
        });
        tx.send(ReaderMessage::Message(wrong_response)).unwrap();

        // Then send the correct one
        let correct_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {"correct": true}
        });
        tx.send(ReaderMessage::Message(correct_response)).unwrap();

        // Should skip wrong ID and return correct one
        let result: Result<LspResponse<serde_json::Value>, _> =
            reader.recv_response(42, Duration::from_secs(1));

        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.id, 42);
    }

    #[test]
    fn test_recv_response_process_exited() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Send shutdown message (simulates process exit)
        tx.send(ReaderMessage::Shutdown).unwrap();

        let result: Result<LspResponse<serde_json::Value>, _> =
            reader.recv_response(1, Duration::from_secs(1));

        assert!(matches!(result, Err(TransportError::ProcessExited)));
    }

    #[test]
    fn test_recv_response_reader_dead() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Drop the sender to simulate reader thread dying
        drop(tx);

        let result: Result<LspResponse<serde_json::Value>, _> =
            reader.recv_response(1, Duration::from_secs(1));

        assert!(matches!(result, Err(TransportError::ReaderDead)));
    }

    #[test]
    fn test_recv_response_skips_notifications() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Send a notification (no id field)
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {}
        });
        tx.send(ReaderMessage::Message(notification)).unwrap();

        // Then send the actual response
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {}
        });
        tx.send(ReaderMessage::Message(response)).unwrap();

        // Should skip notification and return response
        let result: Result<LspResponse<serde_json::Value>, _> =
            reader.recv_response(1, Duration::from_secs(1));

        assert!(result.is_ok());
    }

    #[test]
    fn test_recv_response_with_error() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Send an error response
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32600,
                "message": "Invalid Request"
            }
        });
        tx.send(ReaderMessage::Message(response)).unwrap();

        let result: Result<LspResponse<serde_json::Value>, _> =
            reader.recv_response(1, Duration::from_secs(1));

        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Invalid Request");
    }
}
