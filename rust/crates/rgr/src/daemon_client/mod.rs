//! CLI-to-daemon communication adapter.
//!
//! This module provides the client-side transport for communicating with
//! the repo-graph daemon (`rmapd`). It handles:
//!
//! - Transport abstraction (socket vs stdio)
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
//! │   (transport layer)   │
//! └───────────────────────┘
//!         │
//!    ┌────┴────┐
//!    ▼         ▼
//! Socket    Stdio
//! (primary) (fallback)
//!    │         │
//!    ▼         ▼
//! ┌───────────────────────┐
//! │       rmapd           │
//! │   (daemon/subprocess) │
//! └───────────────────────┘
//! ```
//!
//! ## Transport Selection (STDIO-TRANSPORT-1)
//!
//! 1. `RMAP_TRANSPORT=stdio` → use stdio subprocess unconditionally
//! 2. `RMAP_TRANSPORT=socket` → use socket (fail if unavailable)
//! 3. Default (auto):
//!    - Try socket transport
//!    - If EPERM/EACCES (sandbox denial) → fall back to stdio
//!    - Other errors → return error (no fallback)
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

mod connection;
mod fallback;
mod reachability;
mod socket_transport;
mod stdio_transport;
mod transport;

pub use connection::{DaemonClientError, DaemonConnection};
pub use fallback::{classify_operation, daemon_unavailable_message, OperationClass};
pub use reachability::{check_socket_connectivity, is_daemon_reachable, SocketConnectResult};
pub use stdio_transport::StateRootMode;
pub use transport::{Transport, TransportMode};

use crate::cli::paths::daemon_socket_path;
use socket_transport::SocketTransport;
use std::path::PathBuf;
use stdio_transport::StdioTransport;
use transport::is_permission_denied_connection_error;

/// High-level daemon client with transport abstraction.
///
/// This is the main entry point for CLI commands that need to communicate
/// with the daemon. It handles:
///
/// - Transport selection (socket vs stdio)
/// - Bounded automatic fallback on sandbox denial
/// - Connection management
/// - State root tracking for sandbox mode
/// - Fallback policy enforcement
pub struct DaemonClient {
    socket_path: PathBuf,
    transport: Option<Box<dyn Transport>>,
    transport_mode: TransportMode,
    /// Sandbox state root if stdio fallback was used with injected root.
    sandbox_state_root: Option<PathBuf>,
}

impl DaemonClient {
    /// Create a new daemon client using the platform-native socket path.
    ///
    /// Transport mode is determined by `RMAP_TRANSPORT` env var:
    /// - `stdio` → use stdio subprocess unconditionally
    /// - `socket` → use socket (fail if unavailable)
    /// - unset/other → auto (socket with bounded stdio fallback)
    pub fn new() -> Result<Self, DaemonClientError> {
        let socket_path = daemon_socket_path().ok_or_else(|| {
            DaemonClientError::ConnectionFailed(
                "could not determine daemon socket path".to_string(),
            )
        })?;

        Ok(Self {
            socket_path,
            transport: None,
            transport_mode: TransportMode::from_env(),
            sandbox_state_root: None,
        })
    }

    /// Create a daemon client with a custom socket path (for testing).
    pub fn with_socket_path(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            transport: None,
            transport_mode: TransportMode::from_env(),
            sandbox_state_root: None,
        }
    }

    /// Create a daemon client with explicit transport mode (for testing).
    #[allow(dead_code)]
    pub fn with_transport_mode(socket_path: PathBuf, mode: TransportMode) -> Self {
        Self {
            socket_path,
            transport: None,
            transport_mode: mode,
            sandbox_state_root: None,
        }
    }

    /// Get the socket path this client uses.
    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }

    /// Get the configured transport mode.
    pub fn transport_mode(&self) -> TransportMode {
        self.transport_mode
    }

    /// Get the active transport name (if connected).
    pub fn active_transport_name(&self) -> Option<&'static str> {
        self.transport.as_ref().map(|t| t.mode_name())
    }

    /// Get the state root mode for this client.
    ///
    /// Returns the mode after connection is established.
    pub fn state_root_mode(&self) -> StateRootMode {
        if self.sandbox_state_root.is_some() {
            StateRootMode::SandboxLocal
        } else if std::env::var("RMAP_STATE_ROOT").is_ok() {
            StateRootMode::Override
        } else {
            StateRootMode::Global
        }
    }

    /// Get the sandbox state root path if one was injected.
    pub fn sandbox_state_root(&self) -> Option<&PathBuf> {
        self.sandbox_state_root.as_ref()
    }

    /// Check if the daemon is reachable via socket (quick connectivity test).
    ///
    /// This does NOT send any requests. For a full health check, use `ping()`.
    /// Note: This checks socket reachability, not stdio availability.
    pub fn is_available(&self) -> bool {
        is_daemon_reachable(&self.socket_path)
    }

    /// Connect to the daemon using the configured transport.
    ///
    /// Implements bounded fallback: if socket fails with EPERM/EACCES
    /// (sandbox denial), automatically falls back to stdio transport.
    fn ensure_connected(&mut self) -> Result<&mut Box<dyn Transport>, DaemonClientError> {
        if let Some(ref mut transport) = self.transport {
            return Ok(transport);
        }

        let transport: Box<dyn Transport> = match self.transport_mode {
            TransportMode::Stdio => {
                // Explicit stdio mode - no sandbox root injection
                Box::new(StdioTransport::spawn(None)?)
            }
            TransportMode::Socket => {
                // Explicit socket mode - no fallback
                Box::new(SocketTransport::connect(&self.socket_path)?)
            }
            TransportMode::Auto => {
                // Auto mode: try socket, fallback to stdio on permission denied
                match SocketTransport::connect(&self.socket_path) {
                    Ok(socket) => Box::new(socket),
                    Err(ref e) if is_permission_denied_connection_error(e) => {
                        // Sandbox denial - fall back to stdio with sandbox state root
                        eprintln!(
                            "note: socket connection denied (sandbox), using stdio transport"
                        );
                        // Compute and track sandbox root
                        let sandbox_root = StdioTransport::prepare_sandbox_state_root()?;
                        self.sandbox_state_root = sandbox_root.clone();
                        Box::new(StdioTransport::spawn(sandbox_root)?)
                    }
                    Err(e) => {
                        // Other error - do not mask with stdio fallback
                        return Err(e);
                    }
                }
            }
        };

        self.transport = Some(transport);
        Ok(self.transport.as_mut().unwrap())
    }

    /// Send a request to the daemon.
    ///
    /// Establishes a connection if not already connected.
    pub fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, DaemonClientError> {
        let transport = self.ensure_connected()?;
        transport.request(method, params)
    }

    /// Send a request with a custom timeout.
    ///
    /// # MAINTENANCE-CLI-1 Technical Debt
    ///
    /// This method allows operations like `maintenance prune` to use longer
    /// timeouts than the default 300s. The proper fix is to have the daemon
    /// emit progress events during long operations.
    ///
    /// Note: For StdioTransport, the timeout parameter is ignored because
    /// subprocess I/O doesn't have the same timeout semantics as sockets.
    pub fn request_with_timeout(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
        timeout_secs: u64,
    ) -> Result<serde_json::Value, DaemonClientError> {
        let transport = self.ensure_connected()?;
        transport.request_with_timeout(method, params, timeout_secs)
    }

    /// Send a request, rendering the daemon's progress frames via `on_progress` as they arrive
    /// (DAEMON-VISIBILITY-1 contract C2). Used by `index`/`refresh` so a long operation surfaces
    /// coarse progress while attached instead of blocking silently. On the stdio transport this
    /// falls back to the default (no progress) — attached progress is a socket-daemon nicety.
    pub fn request_with_progress(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
        timeout_secs: u64,
        on_progress: &mut dyn FnMut(&serde_json::Value),
    ) -> Result<serde_json::Value, DaemonClientError> {
        let transport = self.ensure_connected()?;
        transport.request_with_progress(method, params, timeout_secs, on_progress)
    }

    /// Ping the daemon to verify it's responsive.
    ///
    /// Uses the same code path as all other requests.
    pub fn ping(&mut self) -> Result<(), DaemonClientError> {
        let transport = self.ensure_connected()?;
        transport.ping()
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
                // Note: with transport abstraction, we try to connect which
                // may use stdio fallback automatically
                match self.ensure_connected() {
                    Ok(_) => daemon_fn(self),
                    Err(_) => fallback_fn(),
                }
            }

            OperationClass::DaemonRequired => {
                // Must use daemon, fail if unavailable
                match self.ensure_connected() {
                    Ok(_) => daemon_fn(self),
                    Err(_) => {
                        let operation_name = command_path.join(" ");
                        Err(daemon_unavailable_message(
                            &self.socket_path,
                            &operation_name,
                        ))
                    }
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
