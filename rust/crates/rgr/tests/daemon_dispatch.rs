//! Integration tests for daemon dispatch.
//!
//! Tests the service dispatcher through the transport layer.

use std::io::{BufReader, Cursor};
use std::sync::Arc;

use repo_graph_daemon_transport::run_transport;
use repo_graph_rgr::daemon::{DaemonState, ServiceDispatcher};
use tempfile::tempdir;

fn run_daemon_request(input: &str) -> String {
    let state = Arc::new(DaemonState::new());
    let dispatcher = ServiceDispatcher::new(state);

    let input = Cursor::new(input);
    let mut output = Vec::new();

    run_transport(BufReader::new(input), &mut output, &dispatcher).unwrap();

    String::from_utf8(output).unwrap()
}

#[test]
fn ping_returns_pong() {
    let output = run_daemon_request(r#"{"id":"1","method":"ping"}"#);
    assert!(output.contains(r#""id":"1""#));
    assert!(output.contains(r#""pong":true"#));
}

#[test]
fn echo_returns_params() {
    let output = run_daemon_request(r#"{"id":"2","method":"echo","params":{"test":"value"}}"#);
    assert!(output.contains(r#""id":"2""#));
    assert!(output.contains(r#""test":"value""#));
}

#[test]
fn unknown_method_returns_error() {
    let output = run_daemon_request(r#"{"id":"3","method":"bogus"}"#);
    assert!(output.contains(r#""id":"3""#));
    assert!(output.contains(r#""code":"UnknownMethod""#));
}

#[test]
fn list_repos_empty_initially() {
    let output = run_daemon_request(r#"{"id":"4","method":"list_repos"}"#);
    assert!(output.contains(r#""id":"4""#));
    assert!(output.contains(r#""repos":[]"#));
}

#[test]
fn callers_without_loaded_repo_returns_error() {
    // Use a temp dir so the db_path can be canonicalized
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    // Create an empty file so canonicalization works
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"5","method":"callers","params":{{"db_path":"{}","repo_uid":"test","symbol":"foo"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"5""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn callees_without_loaded_repo_returns_error() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"6","method":"callees","params":{{"db_path":"{}","repo_uid":"test","symbol":"foo"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"6""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn imports_without_loaded_repo_returns_error() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"7","method":"imports","params":{{"db_path":"{}","repo_uid":"test","file":"foo.ts"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"7""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn callers_missing_db_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"8","method":"callers","params":{"repo_uid":"test","symbol":"foo"}}"#,
    );
    assert!(output.contains(r#""id":"8""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("db_path"));
}

#[test]
fn callers_missing_repo_uid_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"8b","method":"callers","params":{"db_path":"/tmp/test.db","symbol":"foo"}}"#,
    );
    assert!(output.contains(r#""id":"8b""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo_uid"));
}

#[test]
fn callers_missing_symbol_param_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"9","method":"callers","params":{"db_path":"/tmp/test.db","repo_uid":"test"}}"#,
    );
    assert!(output.contains(r#""id":"9""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("symbol"));
}

#[test]
fn load_repo_missing_params_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"10","method":"load_repo"}"#);
    assert!(output.contains(r#""id":"10""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
}

#[test]
fn multiple_requests_processed_in_order() {
    let input = r#"{"id":"a","method":"ping"}
{"id":"b","method":"echo","params":"hello"}
{"id":"c","method":"list_repos"}"#;

    let output = run_daemon_request(input);
    let lines: Vec<&str> = output.lines().collect();

    assert_eq!(lines.len(), 3);
    assert!(lines[0].contains(r#""id":"a""#));
    assert!(lines[1].contains(r#""id":"b""#));
    assert!(lines[2].contains(r#""id":"c""#));
}

// ── D4: Write operation tests ───────────────────────────────────────

#[test]
fn index_missing_repo_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"d4-1","method":"index","params":{"db_path":"/tmp/test.db"}}"#,
    );
    assert!(output.contains(r#""id":"d4-1""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo_path"));
}

#[test]
fn index_missing_db_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"d4-2","method":"index","params":{"repo_path":"/tmp/repo"}}"#,
    );
    assert!(output.contains(r#""id":"d4-2""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("db_path"));
}

#[test]
fn index_nonexistent_repo_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"d4-3","method":"index","params":{"repo_path":"/nonexistent/path","db_path":"/tmp/test.db"}}"#,
    );
    assert!(output.contains(r#""id":"d4-3""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("does not exist"));
}

#[test]
fn refresh_without_loaded_repo_returns_error() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"d4-4","method":"refresh","params":{{"db_path":"{}","repo_uid":"test"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"d4-4""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn refresh_missing_db_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"d4-5","method":"refresh","params":{"repo_uid":"test"}}"#,
    );
    assert!(output.contains(r#""id":"d4-5""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("db_path"));
}

#[test]
fn refresh_missing_repo_uid_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"d4-5b","method":"refresh","params":{"db_path":"/tmp/test.db"}}"#,
    );
    assert!(output.contains(r#""id":"d4-5b""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo_uid"));
}

// ── D4: End-to-end write operation tests ────────────────────────────

/// Helper to run requests against a shared daemon state.
fn run_daemon_requests_with_state(inputs: Vec<&str>, state: Arc<DaemonState>) -> Vec<String> {
    let dispatcher = ServiceDispatcher::new(state);
    let mut results = Vec::new();

    for input in inputs {
        let cursor = Cursor::new(input);
        let mut output = Vec::new();
        run_transport(BufReader::new(cursor), &mut output, &dispatcher).unwrap();
        results.push(String::from_utf8(output).unwrap());
    }

    results
}

#[test]
fn index_then_load_then_refresh_end_to_end() {
    // Create temp directories for repo and db
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().join("test-repo");
    std::fs::create_dir(&repo_dir).unwrap();

    // Create a minimal source file
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let db_path = temp.path().join("test.db");
    let repo_path_str = repo_dir.to_string_lossy();
    let db_path_str = db_path.to_string_lossy();

    let state = Arc::new(DaemonState::new());

    // Step 1: Index the repo
    let index_request = format!(
        r#"{{"id":"e2e-1","method":"index","params":{{"repo_path":"{}","db_path":"{}"}}}}"#,
        repo_path_str, db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let index_output = &results[0];

    assert!(index_output.contains(r#""id":"e2e-1""#), "Index response: {}", index_output);
    assert!(index_output.contains(r#""repo_uid":"test-repo""#), "Index should return repo_uid: {}", index_output);
    assert!(index_output.contains(r#""snapshot_uid""#), "Index should return snapshot_uid: {}", index_output);
    assert!(!index_output.contains(r#""code""#), "Index should not return error: {}", index_output);

    // Step 2: Load the repo
    let load_request = format!(
        r#"{{"id":"e2e-2","method":"load_repo","params":{{"db_path":"{}","repo_uid":"test-repo"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&load_request], Arc::clone(&state));
    let load_output = &results[0];

    assert!(load_output.contains(r#""id":"e2e-2""#), "Load response: {}", load_output);
    assert!(load_output.contains(r#""loaded":"test-repo""#), "Load should succeed: {}", load_output);

    // Step 3: Refresh the repo (now requires db_path + repo_uid)
    let refresh_request = format!(
        r#"{{"id":"e2e-3","method":"refresh","params":{{"db_path":"{}","repo_uid":"test-repo"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&refresh_request], Arc::clone(&state));
    let refresh_output = &results[0];

    assert!(refresh_output.contains(r#""id":"e2e-3""#), "Refresh response: {}", refresh_output);
    assert!(refresh_output.contains(r#""snapshot_uid""#), "Refresh should return snapshot_uid: {}", refresh_output);
    assert!(!refresh_output.contains(r#""code""#), "Refresh should not return error: {}", refresh_output);
}
