//! RETENTION-POLICY-1: Retention lifecycle enforcement.
//!
//! Provides a single path for retention lifecycle:
//! 1. classify_repo_retention — assign retention classes
//! 2. prune_prunable_snapshots — delete prunable snapshots
//! 3. emit summary — report what changed
//!
//! Called after successful index/refresh. Never called on failure.
//!
//! # Invariants
//!
//! - Never prunes: current, parent, baseline_auto, baseline_user
//! - Stale-epoch snapshots are prunable because classification made them so
//! - Prune is post-snapshot-success only
//! - Prune is idempotent (no-op if no prunable snapshots)
//!
//! # References
//!
//! - `docs/slices/retention-policy-1.md`
//! - `docs/slices/cache-semantics-1.md`

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
    /// Number of snapshots pruned
    pub pruned_count: i64,
    /// Retention stats after lifecycle enforcement
    pub stats: RetentionStats,
}

/// Enforce retention lifecycle: classify → prune → return summary.
///
/// This is the single path for retention lifecycle enforcement.
/// Call only after successful snapshot commit.
///
/// # Sequence
///
/// 1. Classify all snapshots for the repo (assigns retention classes)
/// 2. Prune all snapshots marked `prunable`
/// 3. Return stats
///
/// # Transaction Boundaries
///
/// Classification and prune are each atomic (single transaction), but
/// the combined lifecycle is **sequenced, not single-transaction atomic**.
///
/// If classification commits and prune fails, the repo has correct
/// retention classes but pruning is deferred. The next lifecycle run
/// will complete pruning. This is a safe intermediate state.
///
/// # Invariants
///
/// - Classification assigns: current, parent, baseline_auto, prunable
/// - Classification preserves: baseline_user
/// - Prune deletes only: prunable (WHERE retention_class = 'prunable')
/// - Stale-epoch snapshots are prunable because classification excluded them
///   from protected roles
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

    // Enforce retention lifecycle
    let result = match enforce_retention_lifecycle(&repo_state.storage, repo_uid) {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
            );
        }
    };

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
        "repo_path": entry.canonical_path
    });

    DispatchResult::success(&request.id, response)
}
