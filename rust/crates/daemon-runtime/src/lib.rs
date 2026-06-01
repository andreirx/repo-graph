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
pub mod livegraph_feed;
pub mod livegraph_refresh;
pub mod livegraph_warm_cache;
pub mod registry;
pub mod state;
pub mod util;

pub use dispatch::ServiceDispatcher;
pub use registry::{RegistryEntry, RegistryError, RepoRegistry};
pub use state::{DaemonState, RepoKey, RepoState, StateRootMode};

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use repo_graph_daemon_transport::{
    run_socket_transport, run_stdio, DispatchResult, ErrorCode, ErrorDetail, Request, SocketConfig,
};

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

// ── A1 Authority Write Guard (STATE-ROOT-SEPARATION-1) ─────────────────

/// Require global state root mode for A1 (user authority) writes.
///
/// A1 writes include:
/// - Explicit baselines (`mark_baseline`, `unmark_baseline`)
/// - Aliases (`repo_alias`)
/// - Declarations, waivers, quality policies (future)
///
/// These represent user intent that would be silently lost in sandbox-local
/// mode (cleared on daemon restart). This guard enforces the boundary.
///
/// # Returns
///
/// - `Ok(())` in global mode: write may proceed
/// - `Err(DispatchResult)` in sandbox-local mode: return this error to client
///
/// # Usage
///
/// ```ignore
/// pub fn handle_mark_baseline(state: &DaemonState, request: &Request) -> DispatchResult {
///     if let Err(e) = require_global_mode_for_authority_write(state, request, "mark_baseline") {
///         return e;
///     }
///     // ... proceed with A1 write
/// }
/// ```
#[allow(clippy::result_large_err)] // Rich error detail is intentional
pub fn require_global_mode_for_authority_write(
    state: &DaemonState,
    request: &Request,
    operation: &str,
) -> Result<(), DispatchResult> {
    if state.is_sandbox_mode() {
        let state_root = state.registry().state_root().display().to_string();
        Err(DispatchResult::error(
            &request.id,
            ErrorDetail::new(
                ErrorCode::InvalidRequest,
                format!(
                    "cannot modify authority data in sandbox mode: {} \
                     (state root: {}, mode: sandbox-local). \
                     Authority data (baselines, aliases, declarations) must be written \
                     via the socket daemon for durability.",
                    operation, state_root
                ),
            ),
        ))
    } else {
        Ok(())
    }
}

/// Clear stale sandbox state on daemon startup.
///
/// The sandbox root (`/private/tmp/repo-graph-agent/<uid>/`) is used by stdio
/// subprocess transport when socket access is denied (sandbox environments).
/// This state is ephemeral and should not persist across daemon restarts.
///
/// **Why clear on socket daemon startup:**
/// - Socket daemon is the authoritative persistent service
/// - Sandbox state is a temporary fallback, not durable
/// - Prevents stale sandbox state from accumulating
/// - Makes lifecycle semantics explicit: sandbox = ephemeral
///
/// See `docs/architecture/state-root-lifecycle.md` for full lifecycle model.
#[cfg(unix)]
fn clear_stale_sandbox_state() {
    // SAFETY: geteuid() is always safe to call
    let uid = unsafe { libc::geteuid() };
    let sandbox_root = PathBuf::from(format!("/private/tmp/repo-graph-agent/{}", uid));

    if sandbox_root.exists() {
        eprintln!(
            "note: clearing stale sandbox state: {}",
            sandbox_root.display()
        );
        if let Err(e) = std::fs::remove_dir_all(&sandbox_root) {
            eprintln!(
                "warning: failed to clear sandbox state: {} ({})",
                sandbox_root.display(),
                e
            );
        }
    }
}

#[cfg(not(unix))]
fn clear_stale_sandbox_state() {
    // No sandbox root on non-Unix platforms
}

/// Run the daemon in socket mode (default).
///
/// Binds a Unix domain socket, accepts connections, and processes requests.
/// Stays alive as a resident daemon until shutdown signal (SIGTERM/SIGINT).
///
/// This is the primary daemon mode used by systemd/launchd services.
///
/// On startup, clears any stale sandbox state from `/private/tmp/repo-graph-agent/<uid>/`.
/// This ensures sandbox mode is ephemeral and doesn't accumulate state across daemon restarts.
///
/// Logs startup timing at INFO level (PERF-OBS-1).
pub fn run_daemon() -> Result<(), String> {
    let startup_start = Instant::now();

    // Clear stale sandbox state before starting
    // Sandbox root is ephemeral; socket daemon is authoritative
    clear_stale_sandbox_state();
    let sandbox_cleared = startup_start.elapsed();

    let socket_path = daemon_socket_path()?;
    let config = SocketConfig::new(socket_path.clone());

    // DaemonState is !Send/!Sync due to interior mutability. Arc is used for
    // shared ownership, not cross-thread access. The daemon is single-threaded.
    #[allow(clippy::arc_with_non_send_sync)]
    let state = Arc::new(DaemonState::new());
    let state_init = startup_start.elapsed();

    let dispatcher = ServiceDispatcher::new(state);
    let dispatcher_init = startup_start.elapsed();

    // Determine startup mode (cold vs warm based on registry existence)
    let startup_mode = if std::path::Path::new(&socket_path)
        .parent()
        .map(|p| p.join("registry.json").exists())
        .unwrap_or(false)
    {
        "warm" // Existing registry
    } else {
        "cold" // Fresh install
    };

    // Log startup timing (PERF-OBS-1)
    eprintln!(
        "info: daemon startup ({}) - total: {:?}, sandbox_clear: {:?}, state_init: {:?}, dispatcher: {:?}",
        startup_mode,
        dispatcher_init,
        sandbox_cleared,
        state_init - sandbox_cleared,
        dispatcher_init - state_init
    );

    run_socket_transport(&config, &dispatcher).map_err(|e| e.to_string())
}

/// Run the daemon in stdio mode (debug/test only).
///
/// Reads NDJSON requests from stdin, dispatches them, and writes
/// responses to stdout. Returns when stdin reaches EOF.
///
/// **Warning:** This mode is for testing and debugging only.
/// Do not use for production services.
///
/// **Sandbox mode:** If the state root is under `/private/tmp/`, the daemon
/// is running in sandbox-local mode. A1 authority writes (baselines, aliases,
/// declarations) will be blocked. Cache operations (index, refresh, queries)
/// remain allowed.
pub fn run_daemon_stdio() -> Result<(), String> {
    // DaemonState is !Send/!Sync due to interior mutability. Arc is used for
    // shared ownership, not cross-thread access. The daemon is single-threaded.
    #[allow(clippy::arc_with_non_send_sync)]
    let state = Arc::new(DaemonState::new());

    // STATE-ROOT-SEPARATION-1: Warn on sandbox mode startup
    if state.is_sandbox_mode() {
        let state_root = state.registry().state_root().display().to_string();
        eprintln!(
            "note: running in sandbox-local mode (state root: {})",
            state_root
        );
        eprintln!("note: authority writes (baselines, aliases, declarations) are blocked");
        eprintln!("note: cache operations (index, refresh, queries) are allowed");
    }

    let dispatcher = ServiceDispatcher::new(state);

    run_stdio(&dispatcher).map_err(|e| e.to_string())
}
