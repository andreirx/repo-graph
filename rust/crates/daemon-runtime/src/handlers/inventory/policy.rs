//! Policy handler — query policy facts.
//!
//! LEGACY-CONTRACT-MIGRATION-1D: Migrated from legacy CLI contract.
//!
//! Supports three policy fact kinds:
//! - STATUS_MAPPING: C function status code translation patterns
//! - BEHAVIORAL_MARKER: Behavioral annotations in C code
//! - RETURN_FATE: How return values are used by callers
//!
//! This is a READ operation.

use std::collections::BTreeMap;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_policy_facts::{FateKind, PolicyFactsStorageRead};

use crate::handlers::support::{get_optional_string_param, resolve_and_load_repo};
use crate::state::DaemonState;

/// Query policy facts.
///
/// Request: `{"method": "policy", "params": {"repo": "<path>", ...}}`
///
/// Parameters:
/// - `repo` (required): path or alias
/// - `kind` (optional): STATUS_MAPPING | BEHAVIORAL_MARKER | RETURN_FATE (default: STATUS_MAPPING)
/// - `file` (optional): filter by file path
/// - `callee` (optional): filter by callee name (RETURN_FATE only)
/// - `fate` (optional): filter by fate kind (RETURN_FATE only)
///
/// This is a READ operation.
pub fn handle_policy(state: &DaemonState, request: &Request) -> DispatchResult {
    // REG-1: resolve repo
    let (repo_state, repo_uid) = match resolve_and_load_repo(state, &request.params) {
        Ok(r) => r,
        Err(e) => return DispatchResult::error(&request.id, e),
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

    // Parse parameters
    let kind = get_optional_string_param(&request.params, "kind")
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| "STATUS_MAPPING".to_string());

    let file_filter = get_optional_string_param(&request.params, "file");
    let callee_filter = get_optional_string_param(&request.params, "callee");
    let fate_filter_str = get_optional_string_param(&request.params, "fate");

    // Parse fate filter if provided
    let fate_filter: Option<FateKind> = match fate_filter_str {
        Some(s) => match s.to_uppercase().parse::<FateKind>() {
            Ok(f) => Some(f),
            Err(_) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "invalid fate kind: {} (supported: IGNORED, CHECKED, PROPAGATED, TRANSFORMED, STORED)",
                        s
                    )),
                );
            }
        },
        None => None,
    };

    // Validate kind
    if kind != "STATUS_MAPPING" && kind != "BEHAVIORAL_MARKER" && kind != "RETURN_FATE" {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::invalid_request(format!(
                "unsupported policy kind: {} (supported: STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE)",
                kind
            )),
        );
    }

    // Query and return based on kind
    match kind.as_str() {
        "STATUS_MAPPING" => query_status_mappings(
            request,
            &storage,
            &repo_uid,
            &snapshot.snapshot_uid,
            file_filter,
        ),
        "BEHAVIORAL_MARKER" => query_behavioral_markers(
            request,
            &storage,
            &repo_uid,
            &snapshot.snapshot_uid,
            file_filter,
        ),
        "RETURN_FATE" => query_return_fates(
            request,
            &storage,
            &repo_uid,
            &snapshot.snapshot_uid,
            file_filter,
            callee_filter,
            fate_filter,
        ),
        _ => unreachable!(),
    }
}

/// Query STATUS_MAPPING facts.
fn query_status_mappings(
    request: &Request,
    storage: &dyn PolicyFactsStorageRead,
    repo_uid: &str,
    snapshot_uid: &str,
    file_filter: Option<&str>,
) -> DispatchResult {
    let mappings = match storage.query_status_mappings(snapshot_uid, file_filter) {
        Ok(m) => m,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("failed to query policy facts: {}", e),
                ),
            );
        }
    };

    let count = mappings.len();

    let result = serde_json::json!({
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        "kind": "STATUS_MAPPING",
        "count": count,
        "facts": mappings,
    });

    DispatchResult::success(&request.id, result)
}

/// Query BEHAVIORAL_MARKER facts.
fn query_behavioral_markers(
    request: &Request,
    storage: &dyn PolicyFactsStorageRead,
    repo_uid: &str,
    snapshot_uid: &str,
    file_filter: Option<&str>,
) -> DispatchResult {
    let markers = match storage.query_behavioral_markers(snapshot_uid, file_filter, None) {
        Ok(m) => m,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("failed to query policy facts: {}", e),
                ),
            );
        }
    };

    let count = markers.len();

    let result = serde_json::json!({
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        "kind": "BEHAVIORAL_MARKER",
        "count": count,
        "facts": markers,
    });

    DispatchResult::success(&request.id, result)
}

/// Query RETURN_FATE facts.
fn query_return_fates(
    request: &Request,
    storage: &dyn PolicyFactsStorageRead,
    repo_uid: &str,
    snapshot_uid: &str,
    file_filter: Option<&str>,
    callee_filter: Option<&str>,
    fate_filter: Option<FateKind>,
) -> DispatchResult {
    let fates =
        match storage.query_return_fates(snapshot_uid, file_filter, callee_filter, fate_filter) {
            Ok(f) => f,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to query policy facts: {}", e),
                    ),
                );
            }
        };

    let count = fates.len();

    // Build summary by fate kind
    let mut by_fate: BTreeMap<String, usize> = BTreeMap::new();
    for fate in &fates {
        *by_fate.entry(fate.fate.to_string()).or_insert(0) += 1;
    }

    let result = serde_json::json!({
        "repo": repo_uid,
        "snapshot": snapshot_uid,
        "kind": "RETURN_FATE",
        "count": count,
        "facts": fates,
        "summary": {
            "by_fate": by_fate,
        },
    });

    DispatchResult::success(&request.id, result)
}
