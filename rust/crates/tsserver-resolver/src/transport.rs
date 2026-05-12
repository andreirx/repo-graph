//! TSServer transport layer.
//!
//! Handles communication with tsserver subprocess:
//! - Newline-delimited JSON framing
//! - Seq number correlation for request/response matching
//! - Reader thread with channel for timeout enforcement
//! - Event filtering (responses only for requests)
//!
//! Unlike LSP, TSServer uses newline-delimited JSON without Content-Length headers.

use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Serialize;

use crate::protocol::{GenericMessage, Response};

/// Errors from TSServer transport operations.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("timeout after {0:?}")]
    Timeout(Duration),

    #[error("process exited")]
    ProcessExited,

    #[error("reader thread terminated")]
    ReaderDead,

    #[error("tsserver error: {0}")]
    TsServerError(String),
}

/// Sequence number generator for request/response correlation.
pub struct SeqGenerator {
    next: i32,
}

impl SeqGenerator {
    pub fn new() -> Self {
        Self { next: 1 }
    }

    pub fn next(&mut self) -> i32 {
        let seq = self.next;
        self.next += 1;
        seq
    }
}

impl Default for SeqGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Write a request to tsserver stdin.
///
/// TSServer uses newline-delimited JSON (no Content-Length headers).
pub fn write_request<T: Serialize>(
    stdin: &mut ChildStdin,
    request: &T,
) -> Result<(), TransportError> {
    let json = serde_json::to_string(request)?;
    writeln!(stdin, "{}", json)?;
    stdin.flush()?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Reader Thread
// ─────────────────────────────────────────────────────────────────────────────

/// A message read by the reader thread.
#[derive(Debug)]
pub enum ReaderMessage {
    /// Successfully read a TSServer message.
    Message(String),
    /// Error reading.
    Error(String),
    /// Reader is shutting down.
    Shutdown,
}

/// Handle to a background reader thread.
///
/// The reader thread continuously reads TSServer messages (newline-delimited JSON)
/// and sends them through a channel. Callers use `recv_response` to wait for
/// responses with real timeout enforcement.
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
            let reader = BufReader::new(stdout);

            for line_result in reader.lines() {
                match line_result {
                    Ok(line) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        if tx.send(ReaderMessage::Message(line)).is_err() {
                            // Receiver dropped — exit
                            break;
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::UnexpectedEof {
                            let _ = tx.send(ReaderMessage::Shutdown);
                            break;
                        }
                        let _ = tx.send(ReaderMessage::Error(e.to_string()));
                        // Continue trying to read
                    }
                }
            }

            // EOF reached
            let _ = tx.send(ReaderMessage::Shutdown);
        });

        Self {
            rx,
            thread: Some(thread),
        }
    }

    /// Wait for a response with the given request_seq, with timeout.
    ///
    /// Discards events and responses with other request_seq values.
    pub fn recv_response(
        &self,
        expected_seq: i32,
        timeout: Duration,
    ) -> Result<Response, TransportError> {
        let deadline = std::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(TransportError::Timeout(timeout));
            }

            match self.rx.recv_timeout(remaining) {
                Ok(ReaderMessage::Message(json)) => {
                    // Parse as generic message to check type
                    let generic: GenericMessage = match serde_json::from_str(&json) {
                        Ok(g) => g,
                        Err(_) => continue, // Malformed JSON — skip
                    };

                    // Skip events
                    if generic.is_event() {
                        continue;
                    }

                    // Check if this is a response with the expected seq
                    if generic.is_response() {
                        if let Some(req_seq) = generic.request_seq {
                            if req_seq == expected_seq {
                                // Parse full response
                                let response: Response = serde_json::from_str(&json)?;
                                return Ok(response);
                            }
                        }
                        // Response for different seq — discard
                    }
                }
                Ok(ReaderMessage::Error(e)) => {
                    return Err(TransportError::TsServerError(e));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn test_seq_generator() {
        let mut gen = SeqGenerator::new();
        assert_eq!(gen.next(), 1);
        assert_eq!(gen.next(), 2);
        assert_eq!(gen.next(), 3);
    }

    #[test]
    fn test_recv_response_timeout() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Should timeout (no messages sent)
        let result = reader.recv_response(1, Duration::from_millis(50));
        assert!(matches!(result, Err(TransportError::Timeout(_))));

        drop(tx);
    }

    #[test]
    fn test_recv_response_success() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Send a response with matching seq
        let response_json = r#"{
            "type": "response",
            "seq": 0,
            "request_seq": 42,
            "command": "quickinfo",
            "success": true,
            "body": {}
        }"#;
        tx.send(ReaderMessage::Message(response_json.to_string()))
            .unwrap();

        let result = reader.recv_response(42, Duration::from_secs(1));
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.request_seq, 42);
        assert!(resp.success);
    }

    #[test]
    fn test_recv_response_wrong_seq_then_correct() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Send response with wrong seq first
        let wrong_json = r#"{
            "type": "response",
            "seq": 0,
            "request_seq": 99,
            "command": "configure",
            "success": true
        }"#;
        tx.send(ReaderMessage::Message(wrong_json.to_string()))
            .unwrap();

        // Then send correct one
        let correct_json = r#"{
            "type": "response",
            "seq": 0,
            "request_seq": 42,
            "command": "quickinfo",
            "success": true,
            "body": {"kind": "method"}
        }"#;
        tx.send(ReaderMessage::Message(correct_json.to_string()))
            .unwrap();

        // Should skip wrong seq and return correct one
        let result = reader.recv_response(42, Duration::from_secs(1));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().request_seq, 42);
    }

    #[test]
    fn test_recv_response_skips_events() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Send an event (should be skipped)
        let event_json = r#"{
            "type": "event",
            "seq": 5,
            "event": "projectLoadingFinish",
            "body": {}
        }"#;
        tx.send(ReaderMessage::Message(event_json.to_string()))
            .unwrap();

        // Then send the actual response
        let response_json = r#"{
            "type": "response",
            "seq": 0,
            "request_seq": 1,
            "command": "open",
            "success": true
        }"#;
        tx.send(ReaderMessage::Message(response_json.to_string()))
            .unwrap();

        // Should skip event and return response
        let result = reader.recv_response(1, Duration::from_secs(1));
        assert!(result.is_ok());
    }

    #[test]
    fn test_recv_response_process_exited() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        tx.send(ReaderMessage::Shutdown).unwrap();

        let result = reader.recv_response(1, Duration::from_secs(1));
        assert!(matches!(result, Err(TransportError::ProcessExited)));
    }

    #[test]
    fn test_recv_response_reader_dead() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        // Drop sender to simulate reader thread dying
        drop(tx);

        let result = reader.recv_response(1, Duration::from_secs(1));
        assert!(matches!(result, Err(TransportError::ReaderDead)));
    }

    #[test]
    fn test_recv_response_with_error() {
        let (tx, rx) = channel::<ReaderMessage>();
        let reader = ReaderHandle::from_receiver(rx);

        tx.send(ReaderMessage::Error("some error".to_string()))
            .unwrap();

        let result = reader.recv_response(1, Duration::from_secs(1));
        assert!(matches!(result, Err(TransportError::TsServerError(_))));
    }
}
