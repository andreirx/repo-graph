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

pub mod activity;
pub mod callgraph_cert;
pub mod cancel;
pub mod check_coherence;
pub mod cycle_completeness_audit;
pub mod cycle_output;
// INDEX-DISCONNECT-1: the shared "client vanished; write op continues detached" notice + its
// parallel-safe test-capture seam, used by handle_index/handle_refresh's best-effort callbacks.
pub mod detached;
pub mod dispatch;
// ENRICH-LIFECYCLE-1: the automatic background enrichment pass (toolchain-aware, activity-stamped,
// two-gate contention), spawned after every successful index/refresh — mirrors `retention_pass`.
pub mod enrich_pass;
pub mod explain_coherence;
pub mod explain_lg_identity;
pub mod explain_lg_serve;
pub mod focus_resolution_cert;
pub mod handlers;
mod http_boundary_read; // HTTP-BOUNDARY-1: read-time HTTP boundary render helpers (crate-private)
                        // INDEX-BASIS-1: stamp the git basis at index/refresh; compute working-tree drift at
                        // query time. Crate-internal glue between repo-graph-git and the agent IndexDrift DTO;
                        // the five callers (index/refresh/orient/check/explain handlers) are all in this crate.
pub(crate) mod index_drift;
pub mod livegraph_feed;
pub mod livegraph_refresh;
pub mod livegraph_warm_cache;
// MODULE-OWNERSHIP-DUPLICATE-1: maps a duplicate-ownership load failure to a labeled
// degradation for the module command surface (keeps it out of the oversized dispatch.rs).
// Crate-internal: the five callers (dispatch.rs + governance/violations.rs) are all in
// this crate, so this stays `pub(crate)` — no external consumer.
pub(crate) mod module_degradation;
// EC-M2-LEAF-SERVE-1: the MODULE_SUMMARY identity-reconciliation cert + the decorator's
// summary serve helpers (a sibling cert module like focus_resolution_cert / callgraph_cert).
pub mod module_summary_cert;
// DAEMON-CRASH-RECOVERY-1 (F8): op-lifecycle lines in the daemon LOG (index/refresh/enrich/retention/
// reconcile), so a crashed op's forensics survive in the ONE surface a dead daemon leaves behind.
pub mod oplog;
pub mod orient_coherence;
pub mod orient_lg_decisions;
pub mod orient_serve;
pub mod partition_discovery;
// HONEST-DEGRADATION-IMPL-2-REFACTOR: pure reader-context label helpers (D2 + D5), extracted verbatim
// from the oversized `dispatch.rs`. Crate-internal (all public items are `pub(crate)`); `dispatch`
// re-exports them so existing call sites resolve unchanged.
pub(crate) mod reader_context;
// DAEMON-CRASH-RECOVERY-1 (F7/F11): boot + repo-load reconciliation of crash-orphaned `building`
// snapshots (flip to terminal `failed` + log; the non-READY prune reclaims them and F12 stats name
// them). Two callers: `load_repo` + the boot sweep.
pub mod reconcile;
// FORGET-REPO-1: forget a repo (registry + memory + db_runtimes slot + .db/-wal/-shm + .rgr/) and
// detect / reclaim orphaned on-disk storage. Split from the oversized dispatch.rs per the 500-line
// guardrail; four callers (repo_remove, doctor, gc, boot orphan log), all in this crate. Crate-internal
// (`pub(crate)`, like `module_degradation`/`reader_context`) — NOT a public API boundary: nothing
// outside daemon-runtime consumes it (deterministic workspace grep for `reclaim::` found only
// `crate::reclaim::` callers). See the module ledger in reclaim.rs.
pub(crate) mod reclaim;
pub mod registry;
pub mod resource_metrics;
pub mod retention_pass;
pub mod seed; // EMBED-SEED-IMPL-1: option-(a) Embedder (a2 transport) + query-time fallback
pub mod seed_pass; // EMBED-SEED-IMPL-1: background embed pass + coordinator
pub mod snapshot_facts;
pub mod state;
pub mod trust_coherence;
/// RECON-M-R2: flag-gated UNION serving for callers/callees in W-BOTH (recon-design-1 §5.2 /
/// §6.1 M-R2). Flag off ⇒ never entered ⇒ byte-identical serving everywhere.
pub mod union_serve;
pub mod util;
/// RECON-M-R3a: the SHARED WITNESS PROJECTION — ONE computation feeding every witness read
/// surface (trust witnesses block, doctor operational block, orient/stats g1u, modules g2u,
/// explain union degree, map g3u). Peek-only over the M-R1 ledger + partition state; renders
/// unknown, never a stale number (recon-design-1 §5.3.2-4/§5.4).
pub mod witness_projection;

// COHERENCE-LEAF-SERVE-IMPL-2: explain's consumer of the SHARED `OrientServeDecorator` + bounded cert
// (the dispatch wiring lives in `dispatch::handle_explain`; these are its serve/no-eager-read/honest-bound/
// RED-fallback proofs, kept out of the 500-line `orient_serve` per its "no behavior change" scope).
#[cfg(test)]
mod explain_serve_tests;

// DOCTOR-RESOURCE-REPORT: proves `daemon_info` carries a real (non-zero) daemon RSS
// + total `databases/` disk + repo count through the full dispatch path.
#[cfg(test)]
mod daemon_info_resource_tests;

// DAEMON-VISIBILITY-1: proves the activity/status surface (D) + doctor contention truth (E)
// through the real dispatch/handler path.
#[cfg(test)]
mod activity_visibility_tests;

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

/// PERF-INSTRUMENTATION-1: force the one-time `RMAP_PERF` read at startup and,
/// when perf tracing is on, announce the active level once in the daemon log.
///
/// This is the daemon-startup init the slice asks for; placing it here (not in
/// rmapd) keeps the rmapd binary "wiring only" per CLAUDE.md. The gate global
/// itself lives in `repo-graph-repo-index` so both this crate's `perf_trace!`
/// and repo-index's `perf_log!` share one process-global.
fn log_perf_startup() {
    let level = repo_graph_repo_index::perf::init();
    if level > 0 {
        eprintln!(
            "info: perf tracing ENABLED (RMAP_PERF={}) — emitting [PERF] markers to stderr (daemon log)",
            level
        );
    }
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

    // PERF-INSTRUMENTATION-1: read RMAP_PERF once and announce if enabled.
    log_perf_startup();

    // Clear stale sandbox state before starting
    // Sandbox root is ephemeral; socket daemon is authoritative
    clear_stale_sandbox_state();
    let sandbox_cleared = startup_start.elapsed();

    let socket_path = daemon_socket_path()?;
    let config = SocketConfig::new(socket_path.clone());

    let state = Arc::new(DaemonState::new());
    let state_init = startup_start.elapsed();

    // DAEMON-CRASH-RECOVERY-1 (F7/F11): sweep every registered repo for crash-orphaned `building`
    // snapshots left by a previous (dead) daemon — flip each to the terminal `failed` state, classify
    // it `prunable` (retention stats then count it), and log it (the non-READY prune + VACUUM then
    // reclaims it). Runs on a BACKGROUND thread so it
    // NEVER delays the socket bind below (the contract's
    // "must not materially delay socket readiness"); each repo is two-gate guarded, so a repo a fresh
    // client is already indexing is skipped (its live op finalizes its own snapshot). The lazy
    // `load_repo` hook (state.rs) covers any repo the sweep has not reached yet.
    {
        let boot_state = Arc::clone(&state);
        std::thread::spawn(move || crate::reconcile::reconcile_all_repos(&boot_state));
    }

    // DAEMON-CONCURRENCY-IMPL-1 (D-C = C-B): the dispatcher is shared across
    // concurrent connection-handler threads as `Arc<ServiceDispatcher>`.
    // `DaemonState` is now `Send + Sync` (registry behind a `parking_lot::Mutex`;
    // reads use connection-per-operation), so the prior `arc_with_non_send_sync`
    // allow is gone — its removal compiling is the proof the state is `Send + Sync`.
    let dispatcher = Arc::new(ServiceDispatcher::new(state));
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

    run_socket_transport(&config, dispatcher).map_err(|e| e.to_string())
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
    // PERF-INSTRUMENTATION-1: read RMAP_PERF once and announce if enabled. The
    // isolated dogfood drives this stdio path, so `[PERF]` markers it emits land
    // in the per-call stderr capture.
    log_perf_startup();

    // DAEMON-CONCURRENCY-IMPL-1: DaemonState is Send + Sync now, so this is a plain `Arc` (the prior
    // arc_with_non_send_sync allow is gone — its removal compiling proves Send + Sync). Stdio mode is
    // single-connection, but the shared state is the same shared-safe type the socket daemon uses.
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
