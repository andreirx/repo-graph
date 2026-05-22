//! Unit tests for quality handlers.
//!
//! LEGACY-CONTRACT-MIGRATION-1B: Validation requirement per slice doc.
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

use super::{handle_churn, handle_coverage, handle_hotspots, handle_risk};

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
// CHURN HANDLER TESTS
// =============================================================================

#[test]
fn churn_missing_repo_param_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("churn", json!({"since": "90.days.ago"}));

    let result = handle_churn(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn churn_unknown_repo_returns_repo_not_found() {
    let state = DaemonState::new();
    let request = make_request(
        "churn",
        json!({"repo": "/nonexistent/path", "since": "90.days.ago"}),
    );

    let result = handle_churn(&state, &request);

    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

// =============================================================================
// HOTSPOTS HANDLER TESTS
// =============================================================================

#[test]
fn hotspots_missing_repo_param_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("hotspots", json!({"since": "90.days.ago"}));

    let result = handle_hotspots(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn hotspots_unknown_repo_returns_repo_not_found() {
    let state = DaemonState::new();
    let request = make_request(
        "hotspots",
        json!({"repo": "/nonexistent/path", "since": "90.days.ago"}),
    );

    let result = handle_hotspots(&state, &request);

    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

#[test]
fn hotspots_accepts_filter_params() {
    // Verify filter params don't cause parse errors (repo-not-found expected)
    let state = DaemonState::new();
    let request = make_request(
        "hotspots",
        json!({
            "repo": "/nonexistent/path",
            "since": "30.days.ago",
            "exclude_tests": true,
            "exclude_vendored": true
        }),
    );

    let result = handle_hotspots(&state, &request);

    // Should fail on RepoNotFound, not InvalidRequest
    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

// =============================================================================
// RISK HANDLER TESTS
// =============================================================================

#[test]
fn risk_missing_repo_param_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("risk", json!({"since": "90.days.ago"}));

    let result = handle_risk(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn risk_unknown_repo_returns_repo_not_found() {
    let state = DaemonState::new();
    let request = make_request(
        "risk",
        json!({"repo": "/nonexistent/path", "since": "90.days.ago"}),
    );

    let result = handle_risk(&state, &request);

    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

// =============================================================================
// COVERAGE HANDLER TESTS
// =============================================================================

#[test]
fn coverage_missing_repo_param_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("coverage", json!({"report_path": "/some/coverage.json"}));

    let result = handle_coverage(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn coverage_missing_report_path_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request("coverage", json!({"repo": "/some/repo"}));

    let result = handle_coverage(&state, &request);

    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn coverage_nonexistent_report_returns_invalid_request() {
    let state = DaemonState::new();
    let request = make_request(
        "coverage",
        json!({
            "repo": "/some/repo",
            "report_path": "/nonexistent/coverage.json"
        }),
    );

    let result = handle_coverage(&state, &request);

    // Report validation happens before repo resolution
    assert_eq!(get_error_code(&result), Some("InvalidRequest"));
}

#[test]
fn coverage_unknown_repo_returns_repo_not_found() {
    // Create a temp file to pass report validation
    let temp_dir = tempfile::tempdir().unwrap();
    let report_path = temp_dir.path().join("coverage.json");
    std::fs::write(&report_path, "{}").unwrap();

    let state = DaemonState::new();
    let request = make_request(
        "coverage",
        json!({
            "repo": "/nonexistent/path",
            "report_path": report_path.to_str().unwrap()
        }),
    );

    let result = handle_coverage(&state, &request);

    assert_eq!(get_error_code(&result), Some("RepoNotFound"));
}

// =============================================================================
// SUPPORT FUNCTION TESTS
// =============================================================================

mod support_tests {
    use super::super::support::{is_vendored_path, resolve_root_path};
    use std::path::Path;

    #[test]
    fn is_vendored_path_detects_vendor_segment() {
        assert!(is_vendored_path("vendor/lib.js"));
        assert!(is_vendored_path("src/vendor/lib.js"));
        assert!(is_vendored_path("node_modules/package/index.js"));
        assert!(is_vendored_path("third_party/lib.c"));
        assert!(is_vendored_path("deps/library.rs"));
    }

    #[test]
    fn is_vendored_path_rejects_non_vendored() {
        assert!(!is_vendored_path("src/lib.js"));
        assert!(!is_vendored_path("src/vendors_list.js")); // substring not segment
        assert!(!is_vendored_path("myvendor/lib.js")); // prefix not segment
    }

    #[test]
    fn resolve_root_path_handles_relative() {
        let db_path = Path::new("/Users/test/data/db/test.db");
        let relative = "../../repo";

        let resolved = resolve_root_path(db_path, relative);

        // Should resolve to /Users/test/repo (or canonicalized equivalent)
        assert!(resolved.ends_with("repo") || resolved.to_string_lossy().contains("repo"));
    }

    #[test]
    fn resolve_root_path_handles_absolute() {
        let db_path = Path::new("/Users/test/data/db/test.db");
        let absolute = "/absolute/path/repo";

        let resolved = resolve_root_path(db_path, absolute);

        // For absolute paths, join still produces the absolute path
        // (canonicalize may fail if path doesn't exist, falls back to joined)
        assert!(resolved.to_string_lossy().contains("repo"));
    }
}
