//! Daemon service runtime for repo-graph.
//!
//! This crate provides the shared runtime for the repo-graph daemon,
//! including state management, request dispatch, and the main daemon loop.
//!
//! # Architecture
//!
//! ```text
//! [Unix socket / stdin] → [NDJSON] → [ServiceDispatcher] → [Application Services] → response
//! ```
//!
//! The daemon holds per-repo state including:
//! - Storage connection
//! - Concurrency coordinator (readers-writer lock)
//!
//! Requests are routed through the dispatcher which:
//! 1. Parses method and params
//! 2. Looks up the repo state
//! 3. Acquires appropriate lock (read for queries, write for mutations)
//! 4. Calls the service
//! 5. Returns the result
//!
//! # Transport Modes
//!
//! - **Socket mode** (default): Binds Unix domain socket, accepts connections,
//!   stays alive as a resident daemon. Used by systemd/launchd services.
//!
//! - **Stdio mode** (`--stdio`): Reads from stdin, writes to stdout, exits on EOF.
//!   For testing and debugging only.
//!
//! # Usage
//!
//! ```ignore
//! use repo_graph_daemon_runtime::{run_daemon, run_daemon_stdio};
//!
//! // Default: socket mode (resident daemon)
//! if let Err(e) = run_daemon() {
//!     eprintln!("daemon error: {}", e);
//!     std::process::exit(1);
//! }
//!
//! // Debug/test: stdio mode
//! if let Err(e) = run_daemon_stdio() {
//!     eprintln!("daemon error: {}", e);
//!     std::process::exit(1);
//! }
//! ```

pub mod dispatch;
pub mod handlers;
pub mod registry;
pub mod state;
pub mod util;

pub use dispatch::ServiceDispatcher;
pub use registry::{RegistryEntry, RegistryError, RepoRegistry};
pub use state::{DaemonState, RepoKey, RepoState};

use std::path::PathBuf;
use std::sync::Arc;

use repo_graph_daemon_transport::{run_socket_transport, run_stdio, SocketConfig};

/// Returns the daemon socket path.
///
/// Resolution is delegated to `platform-paths` crate, which is the
/// single source of truth for path resolution across both CLI and daemon.
///
/// Resolution order (per platform-paths):
/// 1. `RMAP_SOCKET_PATH` environment variable (if set)
/// 2. Canonical path from passwd home (stable across sandboxed shells)
/// 3. Legacy path from `$HOME` (migration fallback)
fn daemon_socket_path() -> Result<PathBuf, String> {
    repo_graph_platform_paths::daemon_socket_path()
        .ok_or_else(|| "could not determine daemon socket path".to_string())
}

/// Run the daemon in socket mode (default).
///
/// Binds a Unix domain socket, accepts connections, and processes requests.
/// Stays alive as a resident daemon until shutdown signal (SIGTERM/SIGINT).
///
/// This is the primary daemon mode used by systemd/launchd services.
pub fn run_daemon() -> Result<(), String> {
    let socket_path = daemon_socket_path()?;
    let config = SocketConfig::new(socket_path);

    // DaemonState is !Send/!Sync due to interior mutability. Arc is used for
    // shared ownership, not cross-thread access. The daemon is single-threaded.
    #[allow(clippy::arc_with_non_send_sync)]
    let state = Arc::new(DaemonState::new());
    let dispatcher = ServiceDispatcher::new(state);

    run_socket_transport(&config, &dispatcher).map_err(|e| e.to_string())
}

/// Run the daemon in stdio mode (debug/test only).
///
/// Reads NDJSON requests from stdin, dispatches them, and writes
/// responses to stdout. Returns when stdin reaches EOF.
///
/// **Warning:** This mode is for testing and debugging only.
/// Do not use for production services.
pub fn run_daemon_stdio() -> Result<(), String> {
    // DaemonState is !Send/!Sync due to interior mutability. Arc is used for
    // shared ownership, not cross-thread access. The daemon is single-threaded.
    #[allow(clippy::arc_with_non_send_sync)]
    let state = Arc::new(DaemonState::new());
    let dispatcher = ServiceDispatcher::new(state);

    run_stdio(&dispatcher).map_err(|e| e.to_string())
}
