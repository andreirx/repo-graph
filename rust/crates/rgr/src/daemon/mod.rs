//! Daemon mode for rmap.
//!
//! Provides long-running daemon functionality with NDJSON transport
//! over stdin/stdout.
//!
//! # Architecture
//!
//! ```text
//! stdin → [NDJSON] → [ServiceDispatcher] → [RepoCoordinator] → [Storage] → response → stdout
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

mod dispatch;
mod state;

pub use dispatch::ServiceDispatcher;
pub use state::{DaemonState, RepoState};

use std::sync::Arc;

use repo_graph_daemon_transport::run_stdio;

/// Run the daemon in stdio mode.
///
/// Reads NDJSON requests from stdin, dispatches them, and writes
/// responses to stdout. Returns when stdin reaches EOF.
pub fn run_daemon() -> Result<(), String> {
    let state = Arc::new(DaemonState::new());
    let dispatcher = ServiceDispatcher::new(state);

    run_stdio(&dispatcher).map_err(|e| e.to_string())
}
