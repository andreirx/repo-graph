//! CACHE-SEMANTICS-1: Retention classification handler.
//!
//! Post-refresh handler to classify snapshot retention for a repo.
//! Called by the TypeScript extractor after a snapshot becomes ready.

use std::path::Path;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use serde_json::json;

use crate::state::DaemonState;

/// Handle `classify_retention` request.
///
/// Classifies all snapshots for a repo into retention classes:
/// - current: the most recent ready snapshot
/// - parent: parent of current snapshot
/// - baseline_auto: most recent ready before current (if not parent)
/// - prunable: all other ready snapshots
///
/// Does not modify snapshots marked as baseline_user.
///
/// Params:
///   - `path` (required): Repo path to classify
///
/// Response:
///   - `classified`: true if classification ran
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

    // Acquire write lock for classification
    let _write_guard = repo_state.coordinator.acquire_write();

    // Classify retention
    if let Err(e) = repo_state.storage.classify_repo_retention(repo_uid) {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
        );
    }

    let response = json!({
        "classified": true,
        "repo_path": entry.canonical_path
    });

    DispatchResult::success(&request.id, response)
}
