//! Coverage handler — coverage import (write operation).
//!
//! Imports Istanbul coverage reports and stores measurements.

use std::collections::HashSet;
use std::path::Path;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::types::RepoRef;

use crate::state::DaemonState;
use crate::util::utc_now_iso8601;

use super::support::{get_optional_string_param, resolve_and_load_repo, resolve_root_path};

/// Import coverage report and store measurements (write operation).
///
/// Request: `{"method": "coverage", "params": {"repo": "<path>", "report_path": "<path>"}}`
pub fn handle_coverage(state: &DaemonState, request: &Request) -> DispatchResult {
    // Get report_path (required)
    let report_path_str = match get_optional_string_param(&request.params, "report_path") {
        Some(p) => p,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing 'report_path' parameter"),
            );
        }
    };
    let report_path = Path::new(report_path_str);

    // Validate report exists
    if !report_path.is_file() {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::invalid_request(format!(
                "coverage report not found: {}",
                report_path.display()
            )),
        );
    }

    // REG-1: resolve repo
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Get db_path for coordination
    let db_path = repo_state.db_path().to_path_buf();

    // Acquire DB write coordination first (measurements is a write)
    let db_runtime = match state.get_or_create_db_runtime(&db_path) {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            );
        }
    };
    let _db_write_guard = db_runtime.acquire_write();

    // Then acquire repo refresh lock (blocks new readers, waits for active readers)
    let _refresh_guard = repo_state.coordinator.acquire_refresh();

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

    // Resolve root_path (stored relative to db_path) to absolute
    let root_path = resolve_root_path(&db_path, &repo.root_path);
    let root_path_str = root_path.to_str().unwrap_or("");

    // Parse coverage report
    use repo_graph_coverage::parse_istanbul_file;
    let parse_result = match parse_istanbul_file(report_path.to_str().unwrap_or(""), root_path_str)
    {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("failed to parse coverage report: {}", e),
                ),
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

    let indexed_paths: HashSet<String> = indexed_files.iter().map(|f| f.path.clone()).collect();

    // Match coverage to indexed files using shared matcher
    use repo_graph_classification::coverage_matcher::match_coverage_to_indexed;

    let match_result = match match_coverage_to_indexed(&parse_result, &indexed_paths) {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    };

    // Adapt matched facts to MeasurementInput for storage
    use repo_graph_storage::types::MeasurementInput;
    use sha2::{Digest, Sha256};

    let now = utc_now_iso8601();
    let measurements: Vec<MeasurementInput> = match_result
        .matched
        .iter()
        .map(|fact| {
            // Target identity: file stable key format {repo_uid}:{file_path}:FILE
            let target_stable_key = format!("{}:{}:FILE", repo_uid, fact.file_path);
            let kind = "line_coverage";

            // Measurement UID: SHA-256 of identity tuple, truncated to 32 hex chars
            let identity = format!("{}:{}:{}", snapshot.snapshot_uid, target_stable_key, kind);
            let hash = Sha256::digest(identity.as_bytes());
            let measurement_uid = format!("msr:{:x}", hash)
                .chars()
                .take(36)
                .collect::<String>();

            // Value JSON with ratio and underlying counts
            let value_json = format!(
                r#"{{"value":{},"covered":{},"total":{}}}"#,
                fact.line_coverage, fact.covered_statements, fact.total_statements
            );

            MeasurementInput {
                measurement_uid,
                snapshot_uid: snapshot.snapshot_uid.clone(),
                repo_uid: repo_uid.clone(),
                target_stable_key,
                kind: kind.to_string(),
                value_json,
                source: "coverage-istanbul:0.1.0".to_string(),
                created_at: now.clone(),
            }
        })
        .collect();

    // Open a fresh storage connection for the write (under coordination). NO-CREATE (FORGET-REPO-1):
    // writes an EXISTING indexed DB; a missing file fails honestly, never resurrected as an orphan.
    // FOREGROUND-LOCK-1 (§2.2/§2.3): route through the SPLIT bounded-patience choke so a transient
    // lock becomes the honest `Busy` transient (never `InternalError`), while a genuine non-lock
    // fault keeps this open's pre-existing "storage open failed: …" message verbatim (§2.3).
    let mut storage = match state.open_repo_storage_for_request_split(&repo_state) {
        Ok(s) => s,
        Err(crate::foreground_open::ForegroundOpenFault::Busy(detail)) => {
            return DispatchResult::error(&request.id, detail)
        }
        Err(crate::foreground_open::ForegroundOpenFault::Other(e)) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("storage open failed: {}", e),
                ),
            )
        }
    };

    // Atomically replace existing line_coverage measurements
    if let Err(e) = storage.replace_measurements_by_kind(
        &snapshot.snapshot_uid,
        &["line_coverage"],
        &measurements,
    ) {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::new(
                ErrorCode::InternalError,
                format!("failed to replace coverage measurements: {}", e),
            ),
        );
    }

    // Build output from matched facts directly (no re-parsing needed)
    let results: Vec<serde_json::Value> = match_result
        .matched
        .iter()
        .map(|fact| {
            serde_json::json!({
                "file_path": fact.file_path,
                "line_coverage": fact.line_coverage,
                "covered_statements": fact.covered_statements,
                "total_statements": fact.total_statements,
            })
        })
        .collect();

    let count = results.len();

    // Build envelope
    let toolchain: serde_json::Value = snapshot
        .toolchain_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);

    let mut response = serde_json::json!({
        "command": "coverage",
        "repo": repo_uid,
        "snapshot": snapshot.snapshot_uid,
        "toolchain": toolchain,
        "count": count,
        "imported_count": match_result.matched.len(),
        "unnormalized_count": match_result.unnormalized_paths.len(),
        "unmatched_indexed_count": match_result.unmatched_indexed_paths.len(),
        "results": results,
    });

    // Add sample unmatched paths for debugging (max 10)
    if !match_result.unnormalized_paths.is_empty() {
        let sample: Vec<_> = match_result
            .unnormalized_paths
            .iter()
            .take(10)
            .cloned()
            .collect();
        response["unnormalized_paths_sample"] = serde_json::to_value(sample).unwrap();
    }
    if !match_result.unmatched_indexed_paths.is_empty() {
        let sample: Vec<_> = match_result
            .unmatched_indexed_paths
            .iter()
            .take(10)
            .cloned()
            .collect();
        response["unmatched_indexed_paths_sample"] = serde_json::to_value(sample).unwrap();
    }

    DispatchResult::success(&request.id, response)
}
