//! PERF-OBS-1: Storage metrics handlers.
//!
//! Diagnostic handlers for performance observability.
//! These are one-shot measurement queries, not production read paths.

use std::path::Path;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use serde_json::json;

use crate::state::DaemonState;

/// Handle `perf` request.
///
/// Returns database metrics for a single repo or aggregate.
///
/// Params:
///   - `path` (required): Repo path to measure
///
/// Response:
///   - `db_size_bytes`: Total database size
///   - `page_size`: SQLite page size
///   - `page_count`: SQLite page count
///   - `tables`: Per-table metrics array
///   - `tiers`: Tier A/B aggregates
///   - `layers`: Layer 0-1/2/3 aggregates
///   - `retention`: Snapshot retention metrics
pub fn handle_perf(state: &DaemonState, request: &Request) -> DispatchResult {
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

    // Acquire read lock
    let _read_guard = repo_state.coordinator.acquire_read();

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

    // Collect database metrics
    let db_metrics = match storage.collect_database_metrics() {
        Ok(m) => m,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
            )
        }
    };

    // Collect retention metrics
    let retention_metrics = match storage.collect_snapshot_retention_metrics(repo_uid) {
        Ok(m) => m,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
            )
        }
    };

    // Collect retention class stats (CACHE-SEMANTICS-1)
    let (retention_stats, retention_stats_error) = match storage.get_retention_stats(repo_uid) {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(format!("{}", e))),
    };

    // Build response
    let tables: Vec<serde_json::Value> = db_metrics
        .tables
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "row_count": t.row_count,
                "size_bytes": t.size_bytes,
                "tier": t.tier,
                "layer": t.layer
            })
        })
        .collect();

    let response = json!({
        "repo_path": entry.canonical_path,
        "db_size_bytes": db_metrics.total_size_bytes,
        "page_size": db_metrics.page_size,
        "page_count": db_metrics.page_count,
        "tables": tables,
        "tiers": {
            "tier_a_rows": db_metrics.tier_a_rows,
            "tier_b_rows": db_metrics.tier_b_rows
        },
        "layers": {
            "layer_01_rows": db_metrics.layer_01_rows,
            "layer_2_rows": db_metrics.layer_2_rows,
            "layer_3_rows": db_metrics.layer_3_rows
        },
        "classification": {
            "total_rows": db_metrics.classification.total_rows,
            "classified_tier_rows": db_metrics.classification.classified_tier_rows,
            "unclassified_tier_rows": db_metrics.classification.unclassified_tier_rows,
            "classified_layer_rows": db_metrics.classification.classified_layer_rows,
            "unclassified_layer_rows": db_metrics.classification.unclassified_layer_rows,
            "unknown_tier_tables": db_metrics.classification.unknown_tier_tables,
            "unknown_layer_tables": db_metrics.classification.unknown_layer_tables
        },
        "retention": {
            "total_snapshots": retention_metrics.total_snapshots,
            "ready_snapshots": retention_metrics.ready_snapshots,
            "failed_snapshots": retention_metrics.failed_snapshots,
            "oldest_snapshot": retention_metrics.oldest_snapshot,
            "newest_snapshot": retention_metrics.newest_snapshot,
            // CACHE-SEMANTICS-1: retention class breakdown
            "current": retention_stats.as_ref().map(|s| s.current),
            "parent": retention_stats.as_ref().map(|s| s.parent),
            "baseline_auto": retention_stats.as_ref().map(|s| s.baseline_auto),
            "baseline_user": retention_stats.as_ref().map(|s| s.baseline_user),
            // EC-M7: stamp-only baseline marks (provenance stamp + measurements
            // retained; graph rows narrowed) — without this the breakdown would
            // silently omit a whole retention class (review-1 #6).
            "baseline_stamp": retention_stats.as_ref().map(|s| s.baseline_stamp),
            "prunable": retention_stats.as_ref().map(|s| s.prunable),
            "unclassified": retention_stats.as_ref().map(|s| s.unclassified),
            "stale_epoch": retention_stats.as_ref().map(|s| s.stale_epoch),
            "_debug_error": retention_stats_error
        }
    });

    DispatchResult::success(&request.id, response)
}

/// Cheap storage HEALTH summary for `rmap doctor` (DEV-INSTALL-DOCTOR-WAIT-1 +
/// DAEMON-VISIBILITY-1 E/F). Never runs the expensive per-table scan that `perf`
/// (`collect_database_metrics`) does; `rmap perf` is unchanged.
///
/// Delegates to `snapshot_facts::collect_snapshot_facts`, which returns one of:
/// - **in use by daemon** (E): the DB is held by a live index/refresh → healthy in-use, and the
///   (would-be-busy) open is SKIPPED — the fix for the field bug where a live daemon's own lock
///   produced "error opening database";
/// - **idle** (F): `db_size_bytes` + `total_snapshots`/`ready_snapshots`/`prunable_snapshots`
///   (additive superset of the pre-existing fields) + each snapshot's reader-frame state/outcome +
///   the interrupted (non-READY) set;
/// - **read error**: a genuine absent/corrupt DB.
pub fn handle_storage_health(state: &DaemonState, request: &Request) -> DispatchResult {
    let path: &str = match request.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'path' parameter"),
            )
        }
    };

    // Resolve path to repo entry (same "not indexed" contract as `perf`, so doctor's existing
    // "no repo indexed in cwd" handling is preserved).
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

    // DAEMON-VISIBILITY-1 (E + F): collect snapshot facts. This replaces the previous
    // open-then-COUNT(*) body. Crucially it FIRST checks the daemon's own activity registry: if an
    // index/refresh is writing this DB right now, it reports healthy "in use by daemon" and does NOT
    // attempt the (would-be-busy) open — fixing the old "error opening database" that a live daemon's
    // own lock produced (contract E). When idle, it enumerates every snapshot's state + outcome +
    // size (contract F) and preserves the pre-existing `db_size_bytes` / `total_snapshots` /
    // `prunable_snapshots` fields (additive superset — doctor's existing rendering is unaffected).
    let facts = crate::snapshot_facts::collect_snapshot_facts(state, db_path, repo_uid);
    DispatchResult::success(&request.id, facts)
}
