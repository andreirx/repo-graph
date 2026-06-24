//! Assess handler — quality policy assessment (write operation).
//!
//! LEGACY-CONTRACT-MIGRATION-1C: Migrated from legacy CLI contract.
//!
//! Runs quality policy evaluation and persists assessments atomically.

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};

use crate::handlers::support::{get_optional_string_param, resolve_and_load_repo};
use crate::state::DaemonState;

/// Run quality policy assessment for a snapshot.
///
/// Request: `{"method": "assess", "params": {"repo": "<path>", "baseline": "<snapshot_uid>"}}`
///
/// - `repo` (required): path or alias
/// - `baseline` (optional): baseline snapshot UID for comparative policies
///
/// This is a WRITE operation: assessments are persisted atomically.
pub fn handle_assess(state: &DaemonState, request: &Request) -> DispatchResult {
    // REG-1: resolve repo
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Parse optional baseline param
    let baseline_snapshot_uid = get_optional_string_param(&request.params, "baseline");

    // Get db_path for coordination
    let db_path = repo_state.db_path().to_path_buf();

    // Acquire DB write coordination first (assessments is a write)
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
    let storage = match repo_state.storage() {
        Ok(s) => s,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // Get latest snapshot
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

    // Open fresh storage connection for write (under coordination)
    use repo_graph_storage::StorageConnection;
    let storage = match StorageConnection::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("storage open failed: {}", e),
                ),
            );
        }
    };

    // Run assessment via the runner
    use repo_graph_quality_policy_runner::QualityPolicyRunner;

    let mut runner = QualityPolicyRunner::new(storage);
    let result =
        match runner.assess_snapshot(&repo_uid, &snapshot.snapshot_uid, baseline_snapshot_uid) {
            Ok(r) => r,
            Err(e) => {
                // Map runner errors to appropriate error codes
                let error_msg = e.to_string();
                let code = if error_msg.contains("baseline") {
                    // BaselineRequired or BaselineInvalid
                    ErrorCode::InvalidRequest
                } else if error_msg.contains("InvalidPolicy") {
                    ErrorCode::InvalidRequest
                } else {
                    ErrorCode::InternalError
                };
                return DispatchResult::error(&request.id, ErrorDetail::new(code, error_msg));
            }
        };

    // Build envelope
    let toolchain: serde_json::Value = snapshot
        .toolchain_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);

    let response = serde_json::json!({
        "command": "assess",
        "repo": repo_uid,
        "snapshot": snapshot.snapshot_uid,
        "toolchain": toolchain,
        "baseline_snapshot": baseline_snapshot_uid,
        "assessments": {
            "total": result.total_assessments,
            "pass": result.pass_count,
            "fail": result.fail_count,
            "not_applicable": result.not_applicable_count,
            "not_comparable": result.not_comparable_count,
        },
        "baseline_required_count": result.baseline_required_count,
    });

    DispatchResult::success(&request.id, response)
}
