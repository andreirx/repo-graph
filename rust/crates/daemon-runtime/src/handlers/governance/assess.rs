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
    // DAEMON-RESIDUALS-1 (D1-A): take BOTH write guards (DB write mutex + coordinator refresh) with
    // BOUNDED foreground patience. Under a concurrent write pass this returns an honest, holder-named
    // `Busy` transient instead of blocking unbounded up to the 300s client-timeout SYMPTOM (the #2
    // mechanism: this handler previously `acquire_write()`d the DB mutex with no bound). No partial
    // write on timeout — the assessment writes happen only after both guards are held.
    let (_db_write_guard, _refresh_guard) = match crate::foreground_open::acquire_foreground_write(
        &db_runtime,
        &repo_state.coordinator,
        state.activity(),
        &db_path,
        crate::foreground_open::FOREGROUND_WRITE_PATIENCE,
    ) {
        Ok(guards) => guards,
        Err(detail) => return DispatchResult::error(&request.id, detail),
    };

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

    // Open a fresh storage connection for the write (under coordination). NO-CREATE (FORGET-REPO-1):
    // writes an EXISTING indexed DB; a missing file fails honestly, never resurrected as an orphan.
    // FOREGROUND-LOCK-1 (§2.2/§2.3): route through the SPLIT bounded-patience choke so a transient
    // lock becomes the honest `Busy` transient (never `InternalError`), while a genuine non-lock
    // fault keeps this open's pre-existing "storage open failed: …" message verbatim (§2.3).
    let storage = match state.open_repo_storage_for_request_split(&repo_state) {
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
        // GOV-ARMED-1: additive configuration-presence fact — whether any
        // quality policy is configured for this repo.
        "armed": result.armed,
    });

    DispatchResult::success(&request.id, response)
}
