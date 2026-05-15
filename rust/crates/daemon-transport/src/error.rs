//! Transport-layer errors.

use thiserror::Error;

/// Errors that can occur in the transport layer.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Failed to read from input.
    #[error("input read error: {0}")]
    InputRead(#[from] std::io::Error),

    /// Failed to parse request JSON.
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Failed to write to output.
    #[error("output write error: {0}")]
    OutputWrite(std::io::Error),

    /// Input stream closed (EOF).
    #[error("input stream closed")]
    InputClosed,

    /// Failed to bind Unix socket.
    #[error("socket bind error: {0}")]
    SocketBind(String),

    /// Failed to accept connection on socket.
    #[error("socket accept error: {0}")]
    SocketAccept(String),
}
