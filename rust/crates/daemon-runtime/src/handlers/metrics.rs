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

    // Collect database metrics
    let db_metrics = match repo_state.storage.collect_database_metrics() {
        Ok(m) => m,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
            )
        }
    };

    // Collect retention metrics
    let retention_metrics = match repo_state
        .storage
        .collect_snapshot_retention_metrics(repo_uid)
    {
        Ok(m) => m,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
            )
        }
    };

    // Collect retention class stats (CACHE-SEMANTICS-1)
    let (retention_stats, retention_stats_error) =
        match repo_state.storage.get_retention_stats(repo_uid) {
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
            "prunable": retention_stats.as_ref().map(|s| s.prunable),
            "unclassified": retention_stats.as_ref().map(|s| s.unclassified),
            "stale_epoch": retention_stats.as_ref().map(|s| s.stale_epoch),
            "_debug_error": retention_stats_error
        }
    });

    DispatchResult::success(&request.id, response)
}
