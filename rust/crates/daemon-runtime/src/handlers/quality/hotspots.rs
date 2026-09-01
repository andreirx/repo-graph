//! Hotspots handler — hotspot analysis (churn × complexity).
//!
//! RS-MS-3b: Query-time hotspot analysis.

use std::collections::HashSet;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::types::RepoRef;

use crate::state::DaemonState;

use super::support::{
    get_optional_string_param, is_vendored_path, resolve_and_load_repo, resolve_root_path,
};

/// Compute hotspot analysis (churn × complexity).
///
/// Request: `{"method": "hotspots", "params": {"repo": "<path>", "since": "90.days.ago", "exclude_tests": false, "exclude_vendored": false}}`
pub fn handle_hotspots(state: &DaemonState, request: &Request) -> DispatchResult {
    // REG-1: resolve repo
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Parse params
    let since = get_optional_string_param(&request.params, "since")
        .unwrap_or("90.days.ago")
        .to_string();
    let exclude_tests = request
        .params
        .get("exclude_tests")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let exclude_vendored = request
        .params
        .get("exclude_vendored")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let _read_guard = repo_state.coordinator.acquire_read();

    // D-S = S-A (DAEMON-CONCURRENCY-IMPL-1): open one fresh per-operation connection for this
    // handler's SQLite reads. The coordinator guard above keeps it snapshot-consistent for the request.
    let storage = match state.open_repo_storage_for_request(&repo_state) {
        Ok(s) => s,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Get snapshot
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

    // Get repo
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

    // Get indexed files
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

    // Get churn from git
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

    // Filter churn to indexed files
    let churn_inputs: Vec<repo_graph_classification::hotspot_scorer::ChurnInput> = raw_churn
        .into_iter()
        .filter(|entry| indexed_paths.contains(entry.file_path.as_str()))
        .map(
            |entry| repo_graph_classification::hotspot_scorer::ChurnInput {
                file_path: entry.file_path,
                commit_count: entry.commit_count,
                lines_changed: entry.lines_changed,
            },
        )
        .collect();

    // Get per-file complexity
    let complexity_rows = match storage.query_complexity_by_file(&snapshot.snapshot_uid) {
        Ok(rows) => rows,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    };

    let complexity_inputs: Vec<repo_graph_classification::hotspot_scorer::ComplexityInput> =
        complexity_rows
            .into_iter()
            .map(
                |row| repo_graph_classification::hotspot_scorer::ComplexityInput {
                    file_path: row.file_path,
                    sum_complexity: row.sum_complexity,
                },
            )
            .collect();

    // Compute hotspots
    let hotspots = repo_graph_classification::hotspot_scorer::compute_hotspots(
        &churn_inputs,
        &complexity_inputs,
    );

    // Build file_path -> is_test lookup
    let test_files: HashSet<&str> = indexed_files
        .iter()
        .filter(|f| f.is_test)
        .map(|f| f.path.as_str())
        .collect();

    // Apply filtering and count exclusions
    let mut excluded_tests_count = 0usize;
    let mut excluded_vendored_count = 0usize;
    let mut excluded_paths: HashSet<String> = HashSet::new();

    let results: Vec<serde_json::Value> = hotspots
        .into_iter()
        .filter_map(|h| {
            let is_test = test_files.contains(h.file_path.as_str());
            let is_vendored = is_vendored_path(&h.file_path);

            let exclude_as_test = exclude_tests && is_test;
            let exclude_as_vendored = exclude_vendored && is_vendored;

            if exclude_as_test {
                excluded_tests_count += 1;
            }
            if exclude_as_vendored {
                excluded_vendored_count += 1;
            }
            if exclude_as_test || exclude_as_vendored {
                excluded_paths.insert(h.file_path.clone());
                return None;
            }

            Some(serde_json::json!({
                "file_path": h.file_path,
                "commit_count": h.commit_count,
                "lines_changed": h.lines_changed,
                "sum_complexity": h.sum_complexity,
                "hotspot_score": h.hotspot_score,
            }))
        })
        .collect();

    let excluded_count = excluded_paths.len();
    let count = results.len();

    // Build envelope
    let toolchain: serde_json::Value = snapshot
        .toolchain_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);

    let mut response = serde_json::json!({
        "command": "hotspots",
        "repo": repo_uid,
        "snapshot": snapshot.snapshot_uid,
        "toolchain": toolchain,
        "since": since,
        "formula": "lines_changed * sum_complexity",
        "count": count,
        "results": results,
    });

    // Add filtering metadata only when filters are active
    if exclude_tests || exclude_vendored {
        response["filtering"] = serde_json::json!({
            "exclude_tests": exclude_tests,
            "exclude_vendored": exclude_vendored,
            "excluded_count": excluded_count,
            "excluded_tests_count": excluded_tests_count,
            "excluded_vendored_count": excluded_vendored_count,
        });
    }

    // METRIC-LANG-COVERAGE-1 (part A): the hotspot score is churn × complexity, so an unmeasured language
    // contributes complexity 0 and silently vanishes from the ranking. Attach the ALWAYS-PRESENT per-language
    // measurement-coverage block (data-driven caveat; disappears by itself when every significant language is
    // measured; explicit `unavailable` on a read failure — never a silent gap, which a consumer would read as
    // complete coverage) so the omission is always stated.
    response["measurement_coverage"] =
        crate::util::measurement_coverage_json(&storage, &snapshot.snapshot_uid);

    DispatchResult::success(&request.id, response)
}
