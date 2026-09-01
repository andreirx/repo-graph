//! Churn handler — file churn metrics from git history.
//!
//! RS-MS-2: Query-time per-file git churn for indexed files.

use std::collections::HashSet;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::types::RepoRef;

use crate::state::DaemonState;

use super::support::{
    diagnose_history_json, get_optional_string_param, resolve_and_load_repo, resolve_root_path,
};

/// Compute file churn metrics from git history.
///
/// Request: `{"method": "churn", "params": {"repo": "<path>", "since": "90.days.ago"}}`
pub fn handle_churn(state: &DaemonState, request: &Request) -> DispatchResult {
    // REG-1: resolve repo from path/alias and auto-load
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Parse optional since param (default: 90.days.ago)
    let since = get_optional_string_param(&request.params, "since")
        .unwrap_or("90.days.ago")
        .to_string();

    // Acquire read lock
    let _read_guard = repo_state.coordinator.acquire_read();

    // D-S = S-A (DAEMON-CONCURRENCY-IMPL-1): open one fresh per-operation connection for this
    // handler's SQLite reads. The coordinator guard above keeps it snapshot-consistent for the request.
    let storage = match state.open_repo_storage_for_request(&repo_state) {
        Ok(s) => s,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Get latest snapshot
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(snap)) if snap.status == "ready" => snap,
        // DAEMON-VISIBILITY-1 (F2): no READY snapshot on a READY-requiring surface — NAME any existing
        // partial (state, when, on-disk size) + BOTH next actions via the shared helper, never the bare
        // day-2 gaslighting string. `get_latest_snapshot` is READY-only, so the non-ready `Ok(Some(_))`
        // is unreachable today; folded in so a future non-READY leak is honest too.
        Ok(Some(_)) | Ok(None) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::SnapshotNotFound,
                    crate::snapshot_facts::no_ready_snapshot_message(
                        &storage,
                        repo_state.db_path(),
                        &repo_uid,
                    ),
                ),
            );
        }
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    };

    // Get repo for root_path
    let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.clone())) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::RepoNotFound, "repo not found"),
            );
        }
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    };

    // Get indexed files for filtering
    let indexed_files = match storage.get_files_by_repo(&repo_uid) {
        Ok(files) => files,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    };

    let indexed_paths: HashSet<&str> = indexed_files.iter().map(|f| f.path.as_str()).collect();

    // Resolve root_path (stored relative to db_path) to absolute
    let root_path = resolve_root_path(repo_state.db_path(), &repo.root_path);

    // Call git crate for churn
    use repo_graph_git::{get_file_churn, ChurnWindow};
    let window = ChurnWindow::new(&since);

    let raw_churn = match get_file_churn(&root_path, &window) {
        Ok(c) => c,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("git churn failed: {}", e)),
            );
        }
    };

    // Filter to indexed files only
    let results: Vec<serde_json::Value> = raw_churn
        .into_iter()
        .filter(|entry| indexed_paths.contains(entry.file_path.as_str()))
        .map(|entry| {
            serde_json::json!({
                "file_path": entry.file_path,
                "commit_count": entry.commit_count,
                "lines_changed": entry.lines_changed,
            })
        })
        .collect();

    let count = results.len();

    // CHURN-SHALLOW-1 §2.1: diagnose the history shape at query time so the CLI can
    // FRAME the count honestly (a shallow depth-1 clone's whole-tree import must not
    // read as 90-day churn; a stale clone's zero-in-window cause must be stated, not
    // hedged). Additive `history` block; a failed git read renders unknown-with-reason.
    let history = diagnose_history_json(&root_path, &window);

    // Build envelope
    let toolchain: serde_json::Value = snapshot
        .toolchain_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);

    let response = serde_json::json!({
        "command": "churn",
        "repo": repo_uid,
        "snapshot": snapshot.snapshot_uid,
        "toolchain": toolchain,
        "since": since,
        "count": count,
        "results": results,
        "history": history,
    });

    DispatchResult::success(&request.id, response)
}
