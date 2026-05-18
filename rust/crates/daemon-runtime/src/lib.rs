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
/// Resolution order:
/// 1. `RMAP_SOCKET_PATH` environment variable (if set)
/// 2. Platform-native default path
///
/// This duplicates the logic from `rgr/src/cli/paths.rs` to avoid
/// a dependency from daemon-runtime to rgr. The paths must stay in sync.
///
/// Default paths:
/// - macOS: `~/Library/Application Support/repo-graph/daemon.sock`
/// - Linux: `~/.local/share/rmap/daemon.sock`
fn daemon_socket_path() -> Result<PathBuf, String> {
    // Check for environment override (used by tests)
    if let Ok(override_path) = std::env::var("RMAP_SOCKET_PATH") {
        return Ok(PathBuf::from(override_path));
    }

    #[cfg(target_os = "macos")]
    {
        dirs::data_dir()
            .map(|p| p.join("repo-graph").join("daemon.sock"))
            .ok_or_else(|| "could not determine data directory".to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        dirs::data_local_dir()
            .map(|p| p.join("rmap").join("daemon.sock"))
            .ok_or_else(|| "could not determine data directory".to_string())
    }
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
