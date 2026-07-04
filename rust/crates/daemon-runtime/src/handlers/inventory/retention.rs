//! RETENTION-POLICY-1: Retention lifecycle enforcement.
//!
//! # Architecture (REFRESH-HANG-1 fix)
//!
//! Retention is split into two phases:
//!
//! **Foreground (synchronous index/refresh):**
//! - `classify_retention_only()` — assign retention classes, return stats
//! - Fast (~1ms), never blocks user commands
//! - Reports prunable_count so user knows maintenance is needed
//!
//! **Maintenance (explicit or deferred):**
//! - `enforce_retention_lifecycle()` — classify + prune + stats
//! - Can be slow (deletes rows), runs only on explicit request
//! - Called by `classify_retention` daemon command
//!
//! # Why prune is not on the hot path
//!
//! Prune deletes potentially millions of rows from `unresolved_edges` and
//! other tables. On repos with many stale snapshots, this can take 60+
//! seconds. That must never block interactive index/refresh.
//!
//! # Invariants
//!
//! - Never prunes: current, parent, baseline_auto, baseline_user
//! - Stale-epoch snapshots are prunable because classification made them so
//! - READY-retention prune is post-snapshot-success only
//! - Prune is idempotent (no-op if no prunable snapshots)
//!
//! # DAEMON-VISIBILITY-1 (F3) amendment — interrupted-snapshot reclaim (operator Option A, 2026-07-03)
//!
//! The READY-retention invariants above are unchanged. This handler ALSO reclaims ORPHANED non-READY
//! (interrupted / failed) snapshots — the day-2 field bug was a 4 GB `building` snapshot invisible to
//! the READY model that silently held disk. "Prune is post-snapshot-success only" therefore no longer
//! describes the whole handler: it governs the READY path; the non-READY reclaim is gated instead on
//! "no live write op on this DB" (the operator's ratified safety rule — consult the activity registry
//! AND hold the DB write lock, so an in-flight index's `building` snapshot is never touched). See
//! `reclaim_orphaned_non_ready` below and `docs/slices/daemon-visibility-1.md` §2 F3.
//!
//! # References
//!
//! - `docs/slices/retention-policy-1.md`
//! - `docs/slices/cache-semantics-1.md`
//! - `docs/slices/refresh-hang-1.md`

use std::path::Path;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::connection::StorageConnection;
use repo_graph_storage::error::StorageError;
use repo_graph_storage::retention::RetentionStats;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::DaemonState;

/// Result of retention lifecycle enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleResult {
    /// Retention classification completed
    pub classified: bool,
    /// Number of snapshots pruned (0 if prune was not performed)
    pub pruned_count: i64,
    /// Number of snapshots marked prunable but not yet deleted
    pub prunable_count: i64,
    /// Retention stats after lifecycle enforcement
    pub stats: RetentionStats,
}

/// Classify retention only — foreground path for index/refresh.
///
/// This is the fast path for synchronous index/refresh. It:
/// 1. Classifies all snapshots (assigns retention classes)
/// 2. Returns stats including prunable_count
///
/// It does **NOT** prune. Pruning is deferred to explicit maintenance
/// because deleting rows from large tables can take 60+ seconds.
///
/// # When to use
///
/// Call from `handle_index` and `handle_refresh` after successful
/// snapshot commit. The user sees `prunable_count` in the response
/// and knows to run maintenance if needed.
///
/// # Performance
///
/// Classification is fast (~1ms). Safe to call on every index/refresh.
pub fn classify_retention_only(
    storage: &StorageConnection,
    repo_uid: &str,
) -> Result<LifecycleResult, StorageError> {
    // 1. Classify all snapshots
    storage.classify_repo_retention(repo_uid)?;

    // 2. Get current retention stats (includes prunable count)
    let stats = storage.get_retention_stats(repo_uid)?;

    // Calculate prunable count from stats
    let prunable_count = stats
        .total
        .saturating_sub(stats.current)
        .saturating_sub(stats.parent)
        .saturating_sub(stats.baseline_auto)
        .saturating_sub(stats.baseline_user);

    Ok(LifecycleResult {
        classified: true,
        pruned_count: 0,
        prunable_count,
        stats,
    })
}

/// Enforce full retention lifecycle: classify → prune → return summary.
///
/// This is the **maintenance** path that includes pruning.
/// Use for explicit maintenance commands, NOT for interactive index/refresh.
///
/// # Sequence
///
/// 1. Classify all snapshots for the repo (assigns retention classes)
/// 2. Prune all snapshots marked `prunable`
/// 3. Return stats
///
/// # Warning
///
/// Pruning can be slow (60+ seconds) on repos with many stale snapshots.
/// Do NOT call from synchronous index/refresh hot path.
///
/// # Transaction Boundaries
///
/// Classification and prune are each atomic (single transaction), but
/// the combined lifecycle is **sequenced, not single-transaction atomic**.
///
/// # Idempotence
///
/// Safe to call multiple times. Second call with no new prunable snapshots
/// returns pruned_count = 0.
pub fn enforce_retention_lifecycle(
    storage: &StorageConnection,
    repo_uid: &str,
) -> Result<LifecycleResult, StorageError> {
    // 1. Classify all snapshots
    storage.classify_repo_retention(repo_uid)?;

    // 2. Prune prunable snapshots
    let pruned_count = storage.prune_prunable_snapshots(repo_uid)?;

    // 3. Get current retention stats
    let stats = storage.get_retention_stats(repo_uid)?;

    // Log if anything was pruned
    if pruned_count > 0 {
        eprintln!(
            "retention: pruned {} snapshot(s) for repo {}",
            pruned_count, repo_uid
        );
    }

    Ok(LifecycleResult {
        classified: true,
        pruned_count,
        prunable_count: 0, // All prunable were just pruned
        stats,
    })
}

/// Handle `classify_retention` request.
///
/// Runs full retention lifecycle: classify → prune → report.
/// Uses the shared `enforce_retention_lifecycle` helper.
///
/// Params:
///   - `path` (required): Repo path to classify
///
/// Response:
///   - `classified`: true if classification ran
///   - `pruned_count`: number of snapshots pruned
///   - `retention`: current retention stats
///   - `repo_path`: canonical path of repo
pub fn handle_classify_retention(state: &DaemonState, request: &Request) -> DispatchResult {
    let path: &str = match request.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'path' parameter"),
            )
        }
    };

    // Resolve path to repo entry
    let entry = match state.resolve_alias_or_path(path) {
        Some(e) => e,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::RepoNotFound,
                    format!("repo not indexed: {} (run: rmap index {})", path, path),
                ),
            )
        }
    };

    let db_path = Path::new(&entry.db_path);
    let repo_uid = &entry.repo_uid;

    // Load repo state
    let repo_state = match state.load_repo(db_path, repo_uid) {
        Ok(rs) => rs,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // Acquire write lock for lifecycle enforcement
    let _write_guard = repo_state.coordinator.acquire_write();

    // D-S = S-A (DAEMON-CONCURRENCY-IMPL-1): open one fresh per-operation connection for this
    // handler's SQLite reads. The coordinator guard above keeps it snapshot-consistent for the request.
    let storage = match repo_state.storage() {
        Ok(s) => s,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // Enforce retention lifecycle
    let result = match enforce_retention_lifecycle(&storage, repo_uid) {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
            );
        }
    };

    // DAEMON-VISIBILITY-1 (F3): non-READY (interrupted) snapshots are OUTSIDE the READY retention
    // model — `classify_repo_retention` only classifies `status='ready'` and `prune_prunable_snapshots`
    // only deletes `retention_class='prunable'`, so an interrupted 4 GB `building` snapshot is invisible
    // to it and silently holds disk (the day-2 field bug). Enumerate them here (pre-reclaim, so the
    // report can NAME state + when even after they are deleted), then RECLAIM the orphaned ones.
    let interrupted: Vec<serde_json::Value> = storage
        .list_snapshots(repo_uid)
        .unwrap_or_default()
        .iter()
        .filter(|s| s.status != "ready")
        .map(|s| {
            json!({
                "snapshot_uid": s.snapshot_uid,
                "status": s.status,
                "state": crate::snapshot_facts::snapshot_state_label(&s.status, false),
                "created_at": s.created_at,
                "files_total": s.files_total,
            })
        })
        .collect();

    // F3 (operator Option A): actually delete + reclaim the orphaned non-READY snapshots. Gated so a
    // live index's `building` snapshot is NEVER touched (see `reclaim_orphaned_non_ready`). This runs
    // AFTER the enumeration above and re-queries under the DB lock, so the reported list is stable.
    let reclaim =
        reclaim_orphaned_non_ready(state, &storage, db_path, repo_uid, !interrupted.is_empty());

    // Measured AFTER the reclaim: the honest "storage this repo holds now" (post-VACUUM if we ran one).
    let db_size_bytes = std::fs::metadata(&entry.db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    let response = json!({
        "classified": result.classified,
        "pruned_count": result.pruned_count,
        "retention": {
            "current": result.stats.current,
            "parent": result.stats.parent,
            "baseline_auto": result.stats.baseline_auto,
            "baseline_user": result.stats.baseline_user,
            "prunable": result.stats.prunable,
            "total": result.stats.total,
        },
        // F3: interrupted snapshots that were present (named for the report), + the reclaim outcome.
        "interrupted_snapshots": interrupted,
        "non_ready_reclaim": reclaim,
        "db_size_bytes": db_size_bytes,
        "repo_path": entry.canonical_path
    });

    DispatchResult::success(&request.id, response)
}

/// DAEMON-VISIBILITY-1 (F3, operator Option A): delete + reclaim the ORPHANED non-READY snapshots for
/// this repo, returning a reader-frame outcome object for the prune response.
///
/// # Safety (two gates, both required before any deletion)
///
/// The operator's ratified rule: delete a non-READY snapshot only when NO live operation is attached.
/// An initial index coordinates on the DB-level write lock — NOT the `RepoCoordinator` this handler
/// already holds — so the repo write lock alone is blind to it. We therefore require BOTH:
///
/// 1. **Activity registry clear** — no index/refresh/enrich has stamped an in-flight op on this DB
///    (`state.activity().active_for_db`). Every write handler stamps this at entry.
/// 2. **DB write lock free** — `try_acquire_write()` (NON-blocking) on the same `DatabaseState` lock an
///    initial index takes. `try_lock` cannot deadlock against the repo write lock we already hold, and
///    holding it for the deletion + VACUUM excludes any index that would start mid-reclaim.
///
/// If either gate is closed, we SKIP deletion and report "not reclaimed — an operation is in progress"
/// (honest: the interrupted snapshot is still listed; the operator re-runs prune when idle). A live
/// index's in-flight `building` snapshot is thus never reachable by the delete.
///
/// On success it deletes every non-READY snapshot (reusing storage's transactional cascade), runs
/// `VACUUM` to realise the on-disk reclaim (SQLite does not shrink on DELETE), and reports the byte
/// delta. READY snapshots are never touched (the storage query filters `status != 'ready'`).
fn reclaim_orphaned_non_ready(
    state: &DaemonState,
    storage: &StorageConnection,
    db_path: &Path,
    repo_uid: &str,
    has_interrupted: bool,
) -> serde_json::Value {
    let skipped = |reason: &str| json!({ "reclaimed": false, "skipped_reason": reason, "deleted_count": 0, "reclaimed_bytes": 0 });
    if !has_interrupted {
        return json!({ "reclaimed": false, "deleted_count": 0, "reclaimed_bytes": 0 });
    }
    // Gate 1 — operator's named rule: never delete while a live op writes this DB.
    if state.activity().active_for_db(db_path).is_some() {
        return skipped("an operation is in progress on this repo");
    }
    // Gate 2 — take the DB write lock non-blockingly (excludes an initial index coordinating on it).
    let db_runtime = match state.get_or_create_db_runtime(db_path) {
        Ok(r) => r,
        Err(e) => return skipped(&format!("could not resolve db runtime: {e}")),
    };
    let _db_guard = match db_runtime.try_acquire_write() {
        Some(g) => g,
        None => return skipped("an operation is in progress on this repo"),
    };

    let size_before = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    let deleted = match storage.prune_non_ready_snapshots(repo_uid) {
        Ok(uids) => uids,
        Err(e) => return skipped(&format!("delete failed: {e}")),
    };
    if deleted.is_empty() {
        // Raced away between enumeration and the lock (e.g. a just-finalized index): nothing to do.
        return json!({ "reclaimed": true, "deleted_count": 0, "reclaimed_bytes": 0 });
    }
    // Realise the reclaim on disk. If VACUUM fails the rows are still gone; report honestly.
    if let Err(e) = storage.vacuum() {
        return json!({
            "reclaimed": true,
            "deleted_count": deleted.len(),
            "reclaimed_bytes": 0,
            "vacuum_error": e.to_string(),
        });
    }
    let size_after = std::fs::metadata(db_path)
        .map(|m| m.len())
        .unwrap_or(size_before);
    json!({
        "reclaimed": true,
        "deleted_count": deleted.len(),
        "reclaimed_bytes": size_before.saturating_sub(size_after),
    })
}
