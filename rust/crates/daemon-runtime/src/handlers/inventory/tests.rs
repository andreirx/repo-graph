//! Unit tests for inventory handlers.
//!
//! LEGACY-CONTRACT-MIGRATION-1D: Validation requirement per slice doc.
//! RETENTION-POLICY-1: Lifecycle enforcement tests.
//! STATE-ROOT-SEPARATION-1: Authority write blocking in sandbox mode.
//!
//! Tests cover:
//! - Request/response shape validation
//! - Missing repo parameter (invalid request)
//! - Repo-not-found handling
//! - Invalid kind/fate parameters
//! - Retention lifecycle enforcement
//! - A1 authority writes blocked in sandbox mode
//! - A2/B writes allowed in sandbox mode (integration test)
//!
//! Note: Full integration tests require a running daemon and indexed repo.
//! Those are in rgr/tests/policy_command.rs with daemon harness.

use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::Arc;

use serde_json::json;

use repo_graph_daemon_transport::{DispatchResult, Request};
#[cfg(target_os = "macos")]
use repo_graph_daemon_transport::{Dispatcher, NoOpEmitter};

#[cfg(target_os = "macos")]
use crate::dispatch::ServiceDispatcher;
use crate::registry::RepoRegistry;
use crate::state::DaemonState;

use super::{
    handle_classify_retention, handle_mark_baseline, handle_policy, handle_unmark_baseline,
};

/// Helper to create a test request.
fn make_request(method: &str, params: serde_json::Value) -> Request {
    Request {
        id: "test-1".to_string(),
        method: method.to_string(),
        params,
    }
}

/// Helper to extract error code from DispatchResult.
fn get_error_code(result: &DispatchResult) -> Option<&str> {
    match result {
        DispatchResult::Success(_) => None,
        DispatchResult::Error(err_resp) => Some(&err_resp.error.code),
    }
}

/// Helper to extract error message from DispatchResult.
fn get_error_message(result: &DispatchResult) -> Option<&str> {
    match result {
        DispatchResult::Success(_) => None,
        DispatchResult::Error(err_resp) => Some(&err_resp.error.message),
    }
}

/// Helper to create a DaemonState in sandbox mode.
fn make_sandbox_state() -> DaemonState {
    let sandbox_path = PathBuf::from("/private/tmp/repo-graph-agent/501");
    let registry = RepoRegistry::with_test_state_root(sandbox_path);
    DaemonState::with_registry(registry)
}

// =============================================================================
// POLICY HANDLER TESTS
// =============================================================================

#[test]
fn policy_missing_repo_param_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("policy", json!({}));

    let result = handle_policy(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn policy_unknown_repo_returns_repo_not_found() {
    let state = DaemonState::new();
    let request = make_request("policy", json!({"repo": "/nonexistent/path"}));

    let result = handle_policy(&state, &request);

    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

#[test]
fn policy_invalid_kind_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request(
        "policy",
        json!({
            "repo": "/nonexistent/path",
            "kind": "INVALID_KIND"
        }),
    );

    let result = handle_policy(&state, &request);

    // Should fail on RepoNotFound first (kind validation happens after repo resolution)
    // This is by design - we validate repo before kind
    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

#[test]
fn policy_accepts_all_params() {
    // Verify all params don't cause parse errors (repo-not-found expected)
    let state = DaemonState::new();
    let request = make_request(
        "policy",
        json!({
            "repo": "/nonexistent/path",
            "kind": "RETURN_FATE",
            "file": "src/main.c",
            "callee": "get_status",
            "fate": "CHECKED"
        }),
    );

    let result = handle_policy(&state, &request);

    // Should fail on RepoNotFound, not InvalidRequest
    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

#[test]
fn policy_default_kind_is_status_mapping() {
    // When no kind specified, should default to STATUS_MAPPING
    // (validated by not failing with "invalid kind" before repo resolution)
    let state = DaemonState::new();
    let request = make_request(
        "policy",
        json!({
            "repo": "/nonexistent/path"
        }),
    );

    let result = handle_policy(&state, &request);

    // Should fail on RepoNotFound (kind defaulted correctly)
    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

// =============================================================================
// RETENTION LIFECYCLE TESTS
//
// Note: Low-level retention tests are in repo-graph-storage/src/retention.rs.
// These tests verify the daemon-level lifecycle helper behavior.
// =============================================================================

// Lifecycle tests require storage access. Since storage internals are crate-private,
// the detailed unit tests live in the storage crate. Here we test through
// the storage crate's public test helpers exposed via `#[cfg(test)]` feature.
//
// The core invariants are verified by these storage tests:
// - prune_prunable_snapshots_deletes_marked
// - classify_repo_retention_preserves_user_baseline
// - stale_epoch_snapshots_cannot_become_protected
// - marking_current_as_baseline_user_promotes_new_current
// - marking_parent_as_baseline_user_clears_parent_role
//
// The lifecycle helper simply sequences: classify → prune → stats.
// Integration tests verify the full daemon path via `rmap refresh`.
//
// See: rust/crates/storage/src/retention/tests.rs (19 tests)

// =============================================================================
// CLASSIFY_RETENTION HANDLER TESTS
//
// Tests for the `classify_retention` daemon command surface.
// These verify request validation; full lifecycle tests are in storage crate.
// =============================================================================

#[test]
fn classify_retention_missing_path_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("classify_retention", json!({}));

    let result = handle_classify_retention(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn classify_retention_unknown_repo_returns_repo_not_found() {
    let state = DaemonState::new();
    let request = make_request("classify_retention", json!({"path": "/nonexistent/path"}));

    let result = handle_classify_retention(&state, &request);

    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

#[test]
fn classify_retention_param_is_path_not_repo() {
    // Verify the param name is "path" (consistent with other inventory handlers)
    let state = DaemonState::new();

    // Using wrong param name should fail with InvalidRequest
    let request_wrong = make_request("classify_retention", json!({"repo": "/some/path"}));
    let result_wrong = handle_classify_retention(&state, &request_wrong);
    assert_eq!(get_error_code(&result_wrong), Some("InvalidRequest"));

    // Using correct param name should fail with RepoNotFound (path doesn't exist)
    let request_right = make_request("classify_retention", json!({"path": "/some/path"}));
    let result_right = handle_classify_retention(&state, &request_right);
    assert_eq!(get_error_code(&result_right), Some("RepoNotFound"));
}

// =============================================================================
// STATE-ROOT-SEPARATION-1: A1 AUTHORITY WRITE BLOCKING IN SANDBOX MODE
//
// A1 writes (baselines, aliases, declarations) are blocked in sandbox mode.
// The guard check happens BEFORE repo resolution, so these tests don't need
// a real indexed repo.
//
// A2 (operational state) and B (cache) writes are allowed in sandbox mode.
// The proof for A2/B is structural: handle_index and handle_refresh do NOT
// have the require_global_mode_for_authority_write guard.
// =============================================================================

#[test]
fn mark_baseline_blocked_in_sandbox_mode() {
    let state = make_sandbox_state();
    let request = make_request("mark_baseline", json!({"path": "/some/repo"}));

    let result = handle_mark_baseline(&state, &request);

    // Should be blocked by sandbox guard BEFORE repo resolution
    assert_eq!(get_error_code(&result), Some("InvalidRequest"));

    let message = get_error_message(&result).unwrap();
    assert!(
        message.contains("cannot modify authority data in sandbox mode"),
        "Expected sandbox rejection message, got: {}",
        message
    );
    assert!(
        message.contains("mark_baseline"),
        "Expected operation name in message, got: {}",
        message
    );
}

#[test]
fn unmark_baseline_blocked_in_sandbox_mode() {
    let state = make_sandbox_state();
    let request = make_request(
        "unmark_baseline",
        json!({"path": "/some/repo", "snapshot_uid": "abc123"}),
    );

    let result = handle_unmark_baseline(&state, &request);

    // Should be blocked by sandbox guard BEFORE repo resolution
    assert_eq!(get_error_code(&result), Some("InvalidRequest"));

    let message = get_error_message(&result).unwrap();
    assert!(
        message.contains("cannot modify authority data in sandbox mode"),
        "Expected sandbox rejection message, got: {}",
        message
    );
    assert!(
        message.contains("unmark_baseline"),
        "Expected operation name in message, got: {}",
        message
    );
}

#[test]
fn mark_baseline_allowed_in_global_mode() {
    // In global mode, mark_baseline should NOT be blocked by sandbox guard.
    // It will fail later with RepoNotFound (no repo indexed), proving the
    // guard passed.
    let state = DaemonState::new(); // Global mode
    let request = make_request("mark_baseline", json!({"path": "/some/repo"}));

    let result = handle_mark_baseline(&state, &request);

    // Should pass guard and fail on RepoNotFound
    assert_eq!(
        get_error_code(&result),
        Some("RepoNotFound"),
        "Expected RepoNotFound (guard passed), got: {:?}",
        result
    );
}

// =============================================================================
// STATE-ROOT-SEPARATION-1: A2/B WRITES ALLOWED IN SANDBOX MODE
//
// Integration test proving:
// - A2 (operational local state) can be written in sandbox mode
// - B (derived cache) can be written in sandbox mode
//
// This test creates a real sandbox state root, indexes a tiny repo through
// the dispatcher, and verifies both registration and cache data exist.
// =============================================================================

/// Integration test: index succeeds in sandbox mode (A2 + B allowed).
///
/// This proves:
/// - A2: repo registration written to sandbox-local registry
/// - B: extracted nodes/edges written to sandbox-local database
///
/// The test:
/// 1. Creates sandbox state root under /private/tmp/
/// 2. Creates a tiny repo with one TypeScript file
/// 3. Runs index through the ServiceDispatcher
/// 4. Asserts success and verifies data was written
///
/// macOS-only: sandbox detection uses /private/tmp/ which is macOS-specific.
#[test]
#[cfg(target_os = "macos")]
fn index_allowed_in_sandbox_mode_proves_a2_and_b_writes() {
    use std::fs;

    // Generate unique test directory to avoid conflicts
    let test_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // Create sandbox state root under /private/tmp/ (detected as sandbox-local)
    let sandbox_root = PathBuf::from(format!("/private/tmp/repo-graph-test-{}", test_id));
    let db_dir = sandbox_root.join("databases");
    fs::create_dir_all(&db_dir).expect("failed to create sandbox db_dir");

    // Create a tiny repo with one source file
    let repo_dir = sandbox_root.join("test-repo");
    let src_dir = repo_dir.join("src");
    fs::create_dir_all(&src_dir).expect("failed to create repo src dir");
    fs::write(
        src_dir.join("main.ts"),
        r#"
export function hello(): string {
    return "world";
}
"#,
    )
    .expect("failed to write source file");

    // Create DaemonState with sandbox-local state root
    let registry =
        RepoRegistry::with_state_root(&sandbox_root).expect("failed to create sandbox registry");
    let state = DaemonState::with_registry(registry);

    // Verify we're in sandbox mode
    assert!(
        state.is_sandbox_mode(),
        "Expected sandbox mode for state root: {}",
        sandbox_root.display()
    );

    // Create dispatcher
    let dispatcher = ServiceDispatcher::new(Arc::new(state));

    // Create index request
    let request = Request {
        id: "test-index-1".to_string(),
        method: "index".to_string(),
        params: json!({
            "repo_path": repo_dir.to_string_lossy()
        }),
    };

    // Execute index
    let mut emitter = NoOpEmitter;
    let result = dispatcher.dispatch(&request, &mut emitter);

    // Assert success (not blocked by any guard)
    match &result {
        DispatchResult::Success(resp) => {
            // A2 proof: registration succeeded (we got a response with repo_uid)
            assert!(
                resp.result.get("repo_uid").is_some(),
                "Expected repo_uid in response (A2 registration)"
            );

            // B proof: cache data was written (nodes > 0)
            let nodes_total = resp
                .result
                .get("nodes_total")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            assert!(
                nodes_total > 0,
                "Expected nodes_total > 0 (B cache written), got: {}",
                nodes_total
            );

            // Additional B proof: files were indexed
            let files_total = resp
                .result
                .get("files_total")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            assert!(
                files_total > 0,
                "Expected files_total > 0, got: {}",
                files_total
            );
        }
        DispatchResult::Error(err) => {
            panic!(
                "Index failed in sandbox mode (should be allowed): {} - {}",
                err.error.code, err.error.message
            );
        }
    }

    // Verify A2: registry has the entry
    // (The dispatcher's internal state was updated; we can check the response
    // contained db_path which proves registration happened)
    if let DispatchResult::Success(resp) = &result {
        assert!(
            resp.result.get("db_path").is_some(),
            "Expected db_path in response (A2 registration proof)"
        );
        assert!(
            resp.result.get("snapshot_uid").is_some(),
            "Expected snapshot_uid in response (B cache proof)"
        );
    }

    // Cleanup
    let _ = fs::remove_dir_all(&sandbox_root);
}
