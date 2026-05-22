//! Unit tests for governance handlers.
//!
//! LEGACY-CONTRACT-MIGRATION-1C: Validation requirement per slice doc.
//!
//! Tests cover:
//! - Request/response shape validation
//! - Missing repo parameter (invalid request)
//! - Repo-not-found handling
//!
//! Note: Full integration tests require a running daemon and indexed repo.
//! Those are in rgr/tests/*_command.rs files with daemon harness.

use serde_json::json;

use repo_graph_daemon_transport::{DispatchResult, Request};

use crate::state::DaemonState;

use super::{handle_assess, handle_violations};

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

// =============================================================================
// ASSESS HANDLER TESTS
// =============================================================================

#[test]
fn assess_missing_repo_param_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("assess", json!({}));

    let result = handle_assess(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn assess_unknown_repo_returns_repo_not_found() {
    let state = DaemonState::new();
    let request = make_request("assess", json!({"repo": "/nonexistent/path"}));

    let result = handle_assess(&state, &request);

    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

#[test]
fn assess_accepts_baseline_param() {
    // Verify baseline param doesn't cause parse errors (repo-not-found expected)
    let state = DaemonState::new();
    let request = make_request(
        "assess",
        json!({
            "repo": "/nonexistent/path",
            "baseline": "some-snapshot-uid"
        }),
    );

    let result = handle_assess(&state, &request);

    // Should fail on RepoNotFound, not InvalidRequest
    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

// =============================================================================
// VIOLATIONS HANDLER TESTS
// =============================================================================

#[test]
fn violations_missing_repo_param_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("violations", json!({}));

    let result = handle_violations(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn violations_unknown_repo_returns_repo_not_found() {
    let state = DaemonState::new();
    let request = make_request("violations", json!({"repo": "/nonexistent/path"}));

    let result = handle_violations(&state, &request);

    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}
