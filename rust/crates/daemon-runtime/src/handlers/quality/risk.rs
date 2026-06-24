//! Risk handler — risk scoring (hotspot × coverage gap).
//!
//! RS-MS-4: Query-time risk analysis.

use std::collections::HashSet;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::types::RepoRef;

use crate::state::DaemonState;

use super::support::{get_optional_string_param, resolve_and_load_repo, resolve_root_path};

/// Compute risk analysis (hotspot × coverage gap).
///
/// Request: `{"method": "risk", "params": {"repo": "<path>", "since": "90.days.ago"}}`
pub fn handle_risk(state: &DaemonState, request: &Request) -> DispatchResult {
    // REG-1: resolve repo
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Parse params
    let since = get_optional_string_param(&request.params, "since")
        .unwrap_or("90.days.ago")
        .to_string();

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

    // Get snapshot
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(snap)) if snap.status == "ready" => snap,
        Ok(Some(snap)) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::SnapshotNotFound,
                    format!("latest snapshot is not ready (status: {})", snap.status),
                ),
            );
        }
        Ok(None) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::SnapshotNotFound, "no snapshot found"),
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

    // Compute hotspots first
    let hotspots = repo_graph_classification::hotspot_scorer::compute_hotspots(
        &churn_inputs,
        &complexity_inputs,
    );

    // Get coverage measurements
    let coverage_rows =
        match storage.query_measurements_by_kind(&snapshot.snapshot_uid, "line_coverage") {
            Ok(rows) => rows,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

    // Parse coverage measurements
    let expected_prefix = format!("{}:", repo_uid);
    let mut coverage_inputs: Vec<repo_graph_classification::risk_scorer::CoverageInput> =
        Vec::with_capacity(coverage_rows.len());

    for row in &coverage_rows {
        let file_path = match row
            .target_stable_key
            .strip_prefix(&expected_prefix)
            .and_then(|s| s.strip_suffix(":FILE"))
        {
            Some(p) => p,
            None => continue, // Skip malformed entries
        };

        let v: serde_json::Value = match serde_json::from_str(&row.value_json) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let line_coverage = match v.get("value").and_then(|v| v.as_f64()) {
            Some(c) => c,
            None => continue,
        };

        coverage_inputs.push(repo_graph_classification::risk_scorer::CoverageInput {
            file_path: file_path.to_string(),
            line_coverage,
        });
    }

    // Compute risk scores
    let risk_entries =
        repo_graph_classification::risk_scorer::compute_risk(&hotspots, &coverage_inputs);

    // Convert to output
    let results: Vec<serde_json::Value> = risk_entries
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "file_path": r.file_path,
                "risk_score": r.risk_score,
                "hotspot_score": r.hotspot_score,
                "line_coverage": r.line_coverage,
                "lines_changed": r.lines_changed,
                "sum_complexity": r.sum_complexity,
            })
        })
        .collect();

    let count = results.len();
    let hotspot_count = hotspots.len();
    let coverage_count = coverage_inputs.len();

    // Build envelope
    let toolchain: serde_json::Value = snapshot
        .toolchain_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);

    let response = serde_json::json!({
        "command": "risk",
        "repo": repo_uid,
        "snapshot": snapshot.snapshot_uid,
        "toolchain": toolchain,
        "since": since,
        "formula": "hotspot_score * (1 - line_coverage)",
        "hotspot_files": hotspot_count,
        "coverage_files": coverage_count,
        "joined_files": count,
        "count": count,
        "results": results,
    });

    DispatchResult::success(&request.id, response)
}
