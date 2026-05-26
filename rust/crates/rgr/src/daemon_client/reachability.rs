//! Socket reachability checking.
//!
//! Quick connectivity tests for daemon socket health diagnostics.
//! These functions check if the daemon is accepting connections but
//! do NOT send any application-level requests.
//!
//! For a full health check including daemon responsiveness, use
//! `DaemonConnection::ping()`.

use std::os::unix::net::UnixStream;
use std::path::Path;

/// Result of a socket connectivity check.
#[derive(Debug, Clone)]
pub enum SocketConnectResult {
    /// Socket file does not exist.
    SocketMissing,
    /// Socket file exists but connect failed.
    ConnectFailed {
        /// The underlying OS error message.
        error: String,
        /// OS error code (e.g., ECONNREFUSED = 61 on macOS, 111 on Linux).
        code: Option<i32>,
    },
    /// Connect succeeded, daemon is accepting connections.
    Connected,
}

impl SocketConnectResult {
    /// Returns true if daemon is reachable.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Human-readable description for error messages.
    pub fn description(&self) -> String {
        match self {
            Self::SocketMissing => "socket file does not exist".to_string(),
            Self::ConnectFailed { error, code } => {
                if let Some(c) = code {
                    format!("{} (errno {})", error, c)
                } else {
                    error.clone()
                }
            }
            Self::Connected => "connected".to_string(),
        }
    }
}

/// Check socket connectivity and return detailed result.
///
/// This is a quick connectivity test that does NOT send any requests.
/// Use `DaemonConnection::ping()` for a full health check including
/// daemon responsiveness.
pub fn check_socket_connectivity(socket_path: &Path) -> SocketConnectResult {
    if !socket_path.exists() {
        return SocketConnectResult::SocketMissing;
    }

    // Try to connect briefly
    match UnixStream::connect(socket_path) {
        Ok(stream) => {
            // Connection succeeded, daemon is accepting
            drop(stream);
            SocketConnectResult::Connected
        }
        Err(e) => SocketConnectResult::ConnectFailed {
            error: e.to_string(),
            code: e.raw_os_error(),
        },
    }
}

/// Check if the daemon is available at the given socket path.
///
/// This is a quick connectivity test that does NOT send any requests.
/// Use `DaemonConnection::ping()` for a full health check.
pub fn is_daemon_reachable(socket_path: &Path) -> bool {
    check_socket_connectivity(socket_path).is_connected()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use tempfile::tempdir;

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
    fn check_socket_connectivity_returns_missing_for_nonexistent() {
        let result = check_socket_connectivity(Path::new("/nonexistent/path.sock"));
        assert!(matches!(result, SocketConnectResult::SocketMissing));
    }

    #[test]
    fn check_socket_connectivity_returns_connected_for_listener() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let _listener = UnixListener::bind(&path).unwrap();

        let result = check_socket_connectivity(&path);
        assert!(matches!(result, SocketConnectResult::Connected));
        assert!(result.is_connected());
    }

    #[test]
    fn socket_connect_result_description() {
        assert_eq!(
            SocketConnectResult::SocketMissing.description(),
            "socket file does not exist"
        );
        assert_eq!(SocketConnectResult::Connected.description(), "connected");

        let failed = SocketConnectResult::ConnectFailed {
            error: "Connection refused".to_string(),
            code: Some(61),
        };
        assert!(failed.description().contains("errno 61"));
    }
}
