//! Daemon service runtime for repo-graph.
//!
//! This crate provides the shared runtime for the repo-graph daemon,
//! including state management, request dispatch, and the main daemon loop.
//!
//! # Architecture
//!
//! ```text
//! stdin → [NDJSON] → [ServiceDispatcher] → [Application Services] → response → stdout
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
//! # Usage
//!
//! Both `rmap` (CLI compatibility shim) and `rmapd` (dedicated daemon binary)
//! use this crate as their daemon runtime:
//!
//! ```ignore
//! use repo_graph_daemon_runtime::run_daemon;
//!
//! fn main() {
//!     if let Err(e) = run_daemon() {
//!         eprintln!("daemon error: {}", e);
//!         std::process::exit(1);
//!     }
//! }
//! ```

pub mod dispatch;
pub mod state;
pub mod util;

pub use dispatch::ServiceDispatcher;
pub use state::{DaemonState, RepoKey, RepoState};

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
