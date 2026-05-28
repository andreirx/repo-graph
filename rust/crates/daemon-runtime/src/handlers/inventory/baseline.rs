//! CACHE-SEMANTICS-1: User baseline marking handler.
//!
//! Allows users to explicitly mark a snapshot as a baseline for comparison.
//! User baselines are preserved across automatic retention classification.
//!
//! # Authority Classification (STATE-ROOT-SEPARATION-1)
//!
//! User baselines are A1 (User Authority) data:
//! - Represent explicit user decisions about retention
//! - Cannot be automatically recovered
//! - Blocked in sandbox-local mode
//!
//! See `agent_docs/storage-architecture-v2.md` for tier definitions.

use std::path::Path;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::retention::RetentionClass;
use serde_json::json;

use crate::require_global_mode_for_authority_write;
use crate::state::DaemonState;

/// Handle `mark_baseline` request.
///
/// Marks a specific snapshot as a user baseline. User baselines are preserved
/// across automatic retention classification and are never auto-pruned.
///
/// # Authority Classification
///
/// This is an A1 (User Authority) write. Blocked in sandbox-local mode.
///
/// Params:
///   - `path` (required): Repo path
///   - `snapshot_uid` (optional): Specific snapshot to mark. If omitted, marks
///     the current (most recent) snapshot.
///
/// Response:
///   - `marked`: true if marking succeeded
///   - `snapshot_uid`: the snapshot that was marked
///   - `repo_path`: canonical path of repo
pub fn handle_mark_baseline(state: &DaemonState, request: &Request) -> DispatchResult {
    // STATE-ROOT-SEPARATION-1: A1 authority write guard
    if let Err(e) = require_global_mode_for_authority_write(state, request, "mark_baseline") {
        return e;
    }

    let path: &str = match request.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'path' parameter"),
            )
        }
    };

    let snapshot_uid_param = request.params.get("snapshot_uid").and_then(|v| v.as_str());

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

    // Acquire write lock for marking
    let _write_guard = repo_state.coordinator.acquire_write();

    // Resolve snapshot UID
    let snapshot_uid = match snapshot_uid_param {
        Some(uid) => {
            // Verify snapshot exists and belongs to this repo
            match repo_state.storage.get_snapshot(uid) {
                Ok(Some(snap)) => {
                    if snap.repo_uid != *repo_uid {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::invalid_request(format!(
                                "snapshot '{}' belongs to repo '{}', not '{}'",
                                uid, snap.repo_uid, repo_uid
                            )),
                        );
                    }
                    uid.to_string()
                }
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::invalid_request(format!("snapshot '{}' not found", uid)),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            }
        }
        None => {
            // Use latest snapshot
            match repo_state.storage.get_latest_snapshot(repo_uid) {
                Ok(Some(snap)) => snap.snapshot_uid,
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::SnapshotNotFound,
                            format!("no snapshot found for repo '{}'", repo_uid),
                        ),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            }
        }
    };

    // Mark as user baseline
    if let Err(e) = repo_state
        .storage
        .mark_snapshot_retention(&snapshot_uid, RetentionClass::BaselineUser)
    {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
        );
    }

    // Re-run classification to maintain coherent current/parent/baseline_auto
    // (e.g., if we just marked the current snapshot, a new current must be assigned)
    if let Err(e) = repo_state.storage.classify_repo_retention(repo_uid) {
        // Non-fatal warning — the mark succeeded, classification is best-effort
        eprintln!(
            "warning: retention reclassification failed after mark_baseline: {}",
            e
        );
    }

    let response = json!({
        "marked": true,
        "snapshot_uid": snapshot_uid,
        "repo_path": entry.canonical_path
    });

    DispatchResult::success(&request.id, response)
}

/// Handle `unmark_baseline` request.
///
/// Removes the user baseline marking from a snapshot. The snapshot will be
/// reclassified during the next retention classification.
///
/// # Authority Classification
///
/// This is an A1 (User Authority) write. Blocked in sandbox-local mode.
///
/// Params:
///   - `path` (required): Repo path
///   - `snapshot_uid` (required): Snapshot to unmark
///
/// Response:
///   - `unmarked`: true if unmarking succeeded
///   - `snapshot_uid`: the snapshot that was unmarked
pub fn handle_unmark_baseline(state: &DaemonState, request: &Request) -> DispatchResult {
    // STATE-ROOT-SEPARATION-1: A1 authority write guard
    if let Err(e) = require_global_mode_for_authority_write(state, request, "unmark_baseline") {
        return e;
    }

    let path: &str = match request.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'path' parameter"),
            )
        }
    };

    let snapshot_uid: &str = match request.params.get("snapshot_uid").and_then(|v| v.as_str()) {
        Some(u) => u,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'snapshot_uid' parameter"),
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

    // Acquire write lock
    let _write_guard = repo_state.coordinator.acquire_write();

    // Verify snapshot exists and is marked as user baseline
    match repo_state.storage.get_snapshot(snapshot_uid) {
        Ok(Some(snap)) => {
            if snap.repo_uid != *repo_uid {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "snapshot '{}' belongs to repo '{}', not '{}'",
                        snapshot_uid, snap.repo_uid, repo_uid
                    )),
                );
            }
        }
        Ok(None) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(format!("snapshot '{}' not found", snapshot_uid)),
            );
        }
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    }

    // Mark as prunable (will be reclassified on next refresh)
    if let Err(e) = repo_state
        .storage
        .mark_snapshot_retention(snapshot_uid, RetentionClass::Prunable)
    {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
        );
    }

    // Re-run classification to assign proper class
    if let Err(e) = repo_state.storage.classify_repo_retention(repo_uid) {
        // Non-fatal warning
        eprintln!(
            "warning: retention reclassification failed after unmark: {}",
            e
        );
    }

    let response = json!({
        "unmarked": true,
        "snapshot_uid": snapshot_uid,
        "repo_path": entry.canonical_path
    });

    DispatchResult::success(&request.id, response)
}
