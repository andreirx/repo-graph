//! Transport abstraction for daemon communication.
//!
//! This module provides a transport-agnostic interface for CLI-to-daemon
//! communication. Two transport implementations are available:
//!
//! - `SocketTransport`: Unix domain socket (primary, for normal shells)
//! - `StdioTransport`: Subprocess stdio (fallback, for sandboxed shells)
//!
//! ## Transport Selection Policy
//!
//! 1. `RMAP_TRANSPORT=stdio` → use StdioTransport unconditionally
//! 2. `RMAP_TRANSPORT=socket` → use SocketTransport (fail if unavailable)
//! 3. Default (no override):
//!    a. Try SocketTransport
//!    b. If connect fails with EPERM/EACCES → fall back to StdioTransport
//!    c. If connect fails with other error → return socket error (no fallback)
//!
//! ## Why Bounded Fallback
//!
//! Only permission-denied errors (EPERM/EACCES) trigger automatic fallback.
//! Other errors (ECONNREFUSED, timeout, protocol errors) indicate daemon
//! issues that should surface as errors, not be masked by transport switch.

use super::connection::DaemonClientError;

/// Transport mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// Use Unix socket transport (primary).
    Socket,
    /// Use stdio subprocess transport (for sandboxed shells).
    Stdio,
    /// Auto-select: try socket, fallback to stdio on permission denied.
    Auto,
}

impl TransportMode {
    /// Parse from environment variable.
    pub fn from_env() -> Self {
        match std::env::var("RMAP_TRANSPORT").as_deref() {
            Ok("stdio") => Self::Stdio,
            Ok("socket") => Self::Socket,
            _ => Self::Auto,
        }
    }
}

impl std::fmt::Display for TransportMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Socket => write!(f, "socket"),
            Self::Stdio => write!(f, "stdio"),
            Self::Auto => write!(f, "auto"),
        }
    }
}

/// Transport trait for daemon communication.
///
/// Implementations handle the actual wire protocol (socket or subprocess).
/// The interface is the same regardless of transport.
pub trait Transport {
    /// Send a request and wait for the response.
    ///
    /// Progress events are silently consumed; only the final result/error is returned.
    fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DaemonClientError>;

    /// Send a ping request to verify the daemon is responsive.
    fn ping(&mut self) -> Result<(), DaemonClientError>;

    /// Get the transport mode name for diagnostics.
    fn mode_name(&self) -> &'static str;
}

/// Error codes that trigger stdio fallback.
///
/// EPERM (1) and EACCES (13) indicate sandbox/permission denial.
/// These are the only errors that justify automatic transport switch.
#[allow(dead_code)] // Low-level variant, use is_permission_denied_connection_error instead
pub fn is_permission_denied_error(error: &std::io::Error) -> bool {
    match error.raw_os_error() {
        Some(1) => true,  // EPERM (Operation not permitted)
        Some(13) => true, // EACCES (Permission denied)
        _ => false,
    }
}

/// Check if a connection error is permission-denied.
///
/// Extracts the underlying errno from DaemonClientError::ConnectionFailed.
pub fn is_permission_denied_connection_error(error: &DaemonClientError) -> bool {
    if let DaemonClientError::ConnectionFailed(msg) = error {
        // Check for errno patterns in the error message
        // Format: "failed to connect to ...: <os error message> (os error N)"
        if msg.contains("(os error 1)") || msg.contains("(os error 13)") {
            return true;
        }
        // Also check for explicit permission messages
        if msg.contains("Operation not permitted") || msg.contains("Permission denied") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_mode_from_env_default() {
        // Clear env var for test
        std::env::remove_var("RMAP_TRANSPORT");
        assert_eq!(TransportMode::from_env(), TransportMode::Auto);
    }

    #[test]
    fn is_permission_denied_detects_eperm() {
        // Create from raw to get the errno
        let err = std::io::Error::from_raw_os_error(1);
        assert!(is_permission_denied_error(&err));
    }

    #[test]
    fn is_permission_denied_detects_eacces() {
        let err = std::io::Error::from_raw_os_error(13);
        assert!(is_permission_denied_error(&err));
    }

    #[test]
    fn is_permission_denied_rejects_econnrefused() {
        // ECONNREFUSED is 61 on macOS, 111 on Linux
        let err = std::io::Error::from_raw_os_error(61);
        assert!(!is_permission_denied_error(&err));
    }

    #[test]
    fn is_permission_denied_connection_error_detects_os_error_1() {
        let err = DaemonClientError::ConnectionFailed(
            "failed to connect to /path: Operation not permitted (os error 1)".to_string(),
        );
        assert!(is_permission_denied_connection_error(&err));
    }

    #[test]
    fn is_permission_denied_connection_error_rejects_connrefused() {
        let err = DaemonClientError::ConnectionFailed(
            "failed to connect to /path: Connection refused (os error 61)".to_string(),
        );
        assert!(!is_permission_denied_connection_error(&err));
    }
}
