//! CLI-to-daemon communication adapter.
//!
//! This module provides the client-side transport for communicating with
//! the repo-graph daemon (`rmapd`). It handles:
//!
//! - Socket connection management
//! - NDJSON request/response protocol
//! - Fallback policy enforcement
//! - Daemon availability checking
//!
//! ## Architecture
//!
//! ```text
//! CLI Command Handler
//!         │
//!         ▼
//! ┌───────────────────────┐
//! │    DaemonClient       │  ← This module
//! │   (adapter layer)     │
//! └───────────────────────┘
//!         │
//!         ▼
//!    Unix Socket
//!         │
//!         ▼
//! ┌───────────────────────┐
//! │       rmapd           │
//! │   (daemon process)    │
//! └───────────────────────┘
//! ```
//!
//! ## Fallback Policy
//!
//! When the daemon is unavailable:
//!
//! - **Read-only operations**: May proceed with direct DB access
//! - **Daemon-required operations**: Must fail with actionable error
//!
//! The fallback policy is enforced by the `DaemonClient::execute()` method,
//! which checks operation classification before attempting connection.
//!
//! ## Usage
//!
//! ```ignore
//! use repo_graph_rgr::daemon_client::{DaemonClient, OperationClass};
//!
//! let client = DaemonClient::new()?;
//!
//! // Check if daemon is available
//! if client.is_available() {
//!     let result = client.request("ping", None)?;
//! }
//!
//! // Or let the client handle fallback
//! let result = client.execute_or_fallback(
//!     &["graph", "query"],  // command path
//!     || { /* fallback implementation */ },
//! );
//! ```

mod connection;
mod fallback;

pub use connection::{is_daemon_reachable, DaemonClientError, DaemonConnection};
pub use fallback::{classify_operation, daemon_unavailable_message, OperationClass};

use crate::cli::paths::daemon_socket_path;
use std::path::PathBuf;

/// High-level daemon client with fallback support.
///
/// This is the main entry point for CLI commands that need to communicate
/// with the daemon. It handles:
///
/// - Daemon availability checking
/// - Connection management
/// - Fallback policy enforcement
pub struct DaemonClient {
    socket_path: PathBuf,
    connection: Option<DaemonConnection>,
}

impl DaemonClient {
    /// Create a new daemon client using the platform-native socket path.
    pub fn new() -> Result<Self, DaemonClientError> {
        let socket_path = daemon_socket_path().ok_or_else(|| {
            DaemonClientError::ConnectionFailed(
                "could not determine daemon socket path".to_string(),
            )
        })?;

        Ok(Self {
            socket_path,
            connection: None,
        })
    }

    /// Create a daemon client with a custom socket path (for testing).
    pub fn with_socket_path(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            connection: None,
        }
    }

    /// Get the socket path this client uses.
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Check if the daemon is reachable (quick connectivity test).
    ///
    /// This does NOT send any requests. For a full health check, use `ping()`.
    pub fn is_available(&self) -> bool {
        is_daemon_reachable(&self.socket_path)
    }

    /// Connect to the daemon if not already connected.
    ///
    /// Returns a mutable reference to the connection, or an error if
    /// connection fails.
    fn ensure_connected(&mut self) -> Result<&mut DaemonConnection, DaemonClientError> {
        if self.connection.is_none() {
            self.connection = Some(DaemonConnection::connect(&self.socket_path)?);
        }
        Ok(self.connection.as_mut().unwrap())
    }

    /// Send a request to the daemon.
    ///
    /// Establishes a connection if not already connected.
    pub fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DaemonClientError> {
        let conn = self.ensure_connected()?;
        conn.request(method, params)
    }

    /// Ping the daemon to verify it's responsive.
    ///
    /// Uses the same code path as all other requests.
    pub fn ping(&mut self) -> Result<(), DaemonClientError> {
        let conn = self.ensure_connected()?;
        conn.ping()
    }

    /// Execute an operation with fallback support.
    ///
    /// This method enforces the fallback policy:
    ///
    /// 1. If daemon is available, try the daemon path first
    /// 2. If daemon is unavailable:
    ///    - For `ReadOnly` operations: call the fallback function
    ///    - For `DaemonRequired` operations: return an actionable error
    ///    - For `Static` operations: call the fallback function
    ///
    /// ## Arguments
    ///
    /// - `command_path`: The command path (e.g., `["repo", "list"]`)
    /// - `daemon_fn`: Function to call via daemon (receives mutable client reference)
    /// - `fallback_fn`: Function to call for direct/local execution
    ///
    /// ## Returns
    ///
    /// - `Ok(T)` from either daemon or fallback path
    /// - `Err` if daemon required but unavailable, or if execution fails
    pub fn execute_or_fallback<T, D, F>(
        &mut self,
        command_path: &[&str],
        daemon_fn: D,
        fallback_fn: F,
    ) -> Result<T, String>
    where
        D: FnOnce(&mut Self) -> Result<T, String>,
        F: FnOnce() -> Result<T, String>,
    {
        let operation_class = classify_operation(command_path);

        match operation_class {
            OperationClass::Static => {
                // Static operations never use daemon
                fallback_fn()
            }

            OperationClass::ReadOnly => {
                // Try daemon first, fall back if unavailable
                if self.is_available() {
                    daemon_fn(self)
                } else {
                    fallback_fn()
                }
            }

            OperationClass::DaemonRequired => {
                // Must use daemon, fail if unavailable
                if self.is_available() {
                    daemon_fn(self)
                } else {
                    let operation_name = command_path.join(" ");
                    Err(daemon_unavailable_message(
                        &self.socket_path,
                        &operation_name,
                    ))
                }
            }
        }
    }

    /// Check if an operation requires the daemon.
    ///
    /// Useful for command handlers to decide whether to attempt connection.
    pub fn requires_daemon(command_path: &[&str]) -> bool {
        classify_operation(command_path) == OperationClass::DaemonRequired
    }

    /// Check if an operation can fall back to local execution.
    pub fn can_fallback(command_path: &[&str]) -> bool {
        matches!(
            classify_operation(command_path),
            OperationClass::Static | OperationClass::ReadOnly
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use tempfile::tempdir;

    #[test]
    fn client_reports_unavailable_for_missing_socket() {
        let client = DaemonClient::with_socket_path(PathBuf::from("/nonexistent/path.sock"));
        assert!(!client.is_available());
    }

    #[test]
    fn client_reports_available_for_listening_socket() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        let _listener = UnixListener::bind(&path).unwrap();

        let client = DaemonClient::with_socket_path(path);
        assert!(client.is_available());
    }

    #[test]
    fn execute_or_fallback_uses_fallback_for_static() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        // No listener, daemon unavailable
        let mut client = DaemonClient::with_socket_path(path);

        let result = client.execute_or_fallback(
            &["version"],
            |_| panic!("should not call daemon"),
            || Ok("fallback"),
        );

        assert_eq!(result, Ok("fallback"));
    }

    #[test]
    fn execute_or_fallback_uses_fallback_for_readonly_when_unavailable() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        // No listener, daemon unavailable
        let mut client = DaemonClient::with_socket_path(path);

        let result = client.execute_or_fallback(
            &["repo", "list"],
            |_| panic!("should not call daemon"),
            || Ok("fallback"),
        );

        assert_eq!(result, Ok("fallback"));
    }

    #[test]
    fn execute_or_fallback_fails_for_daemon_required_when_unavailable() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sock");

        // No listener, daemon unavailable
        let mut client = DaemonClient::with_socket_path(path);

        let result: Result<(), String> = client.execute_or_fallback(
            &["index"],
            |_| panic!("should not call daemon"),
            || panic!("should not call fallback"),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Daemon unavailable"));
        assert!(err.contains("index"));
    }

    #[test]
    fn requires_daemon_returns_correct_values() {
        assert!(!DaemonClient::requires_daemon(&["version"]));
        assert!(!DaemonClient::requires_daemon(&["repo", "list"]));
        assert!(DaemonClient::requires_daemon(&["index"]));
        assert!(DaemonClient::requires_daemon(&["hook", "session-start"]));
    }

    #[test]
    fn can_fallback_returns_correct_values() {
        assert!(DaemonClient::can_fallback(&["version"]));
        assert!(DaemonClient::can_fallback(&["repo", "list"]));
        assert!(!DaemonClient::can_fallback(&["index"]));
        assert!(!DaemonClient::can_fallback(&["hook", "session-start"]));
    }
}
