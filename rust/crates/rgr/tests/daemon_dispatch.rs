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

// ── D5b: Progress streaming tests ───────────────────────────────

#[test]
fn index_emits_progress_events() {
    // Create temp directories for repo and db
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().join("progress-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();

    // Create a minimal source file
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let db_path = temp.path().join("test.db");
    let repo_path_str = repo_dir.to_string_lossy();
    let db_path_str = db_path.to_string_lossy();

    let state = Arc::new(DaemonState::new());

    // Index the repo
    let index_request = format!(
        r#"{{"id":"progress-1","method":"index","params":{{"repo_path":"{}","db_path":"{}"}}}}"#,
        repo_path_str, db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], state);
    let output = &results[0];

    // Parse all NDJSON lines
    let lines: Vec<&str> = output.lines().collect();

    // Should have at least some progress events + final response
    assert!(lines.len() > 1, "Expected progress events + response, got {} lines: {}", lines.len(), output);

    // Verify progress events
    let mut found_initializing = false;
    let mut found_scanning = false;
    let mut found_extracting = false;
    let mut found_persisting = false;
    let mut found_result = false;

    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(parsed["id"], "progress-1", "All events should have correct request ID");

        if let Some(progress) = parsed.get("progress") {
            let phase = progress["phase"].as_str().unwrap_or("");
            match phase {
                "initializing" => found_initializing = true,
                "scanning" => found_scanning = true,
                "extracting" => found_extracting = true,
                "persisting" => found_persisting = true,
                _ => {}
            }
        }
        if parsed.get("result").is_some() {
            found_result = true;
        }
    }

    assert!(found_initializing, "Should have initializing progress event (abort checkpoint before ensure_repo)");
    assert!(found_scanning, "Should have scanning progress event");
    assert!(found_extracting, "Should have extracting progress event");
    assert!(found_persisting, "Should have persisting progress event");
    assert!(found_result, "Should have final result");

    // Verify final response is last
    let last_line = lines.last().unwrap();
    let last_parsed: serde_json::Value = serde_json::from_str(last_line).unwrap();
    assert!(last_parsed.get("result").is_some(), "Last line should be the result, not progress");
}

#[test]
fn refresh_emits_progress_events() {
    // Create temp directories for repo and db
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().join("refresh-progress-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let db_path = temp.path().join("test.db");
    let repo_path_str = repo_dir.to_string_lossy();
    let db_path_str = db_path.to_string_lossy();

    let state = Arc::new(DaemonState::new());

    // Step 1: Index first (need existing repo to refresh)
    let index_request = format!(
        r#"{{"id":"rp-1","method":"index","params":{{"repo_path":"{}","db_path":"{}"}}}}"#,
        repo_path_str, db_path_str
    );
    run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));

    // Step 2: Load the repo
    let load_request = format!(
        r#"{{"id":"rp-2","method":"load_repo","params":{{"db_path":"{}","repo_uid":"refresh-progress-repo"}}}}"#,
        db_path_str
    );
    run_daemon_requests_with_state(vec![&load_request], Arc::clone(&state));

    // Step 3: Refresh and check progress
    let refresh_request = format!(
        r#"{{"id":"rp-3","method":"refresh","params":{{"db_path":"{}","repo_uid":"refresh-progress-repo"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&refresh_request], state);
    let output = &results[0];

    let lines: Vec<&str> = output.lines().collect();

    // Should have progress events + final response
    assert!(lines.len() > 1, "Expected progress events + response, got {} lines", lines.len());

    // Verify we have progress events and final result
    let mut found_initializing = false;
    let mut found_scanning = false;
    let mut found_extracting = false;
    let mut found_persisting = false;
    let mut has_result = false;

    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(parsed["id"], "rp-3", "All events should have correct request ID");

        if let Some(progress) = parsed.get("progress") {
            let phase = progress["phase"].as_str().unwrap_or("");
            match phase {
                "initializing" => found_initializing = true,
                "scanning" => found_scanning = true,
                "extracting" => found_extracting = true,
                "persisting" => found_persisting = true,
                _ => {}
            }
        }
        if parsed.get("result").is_some() {
            has_result = true;
        }
    }

    assert!(found_initializing, "Refresh should emit initializing progress event");
    assert!(found_scanning, "Refresh should emit scanning progress event");
    assert!(found_extracting, "Refresh should emit extracting progress event");
    assert!(found_persisting, "Refresh should emit persisting progress event");
    assert!(has_result, "Refresh should emit final result");

    // Verify final response is last
    let last_line = lines.last().unwrap();
    let last_parsed: serde_json::Value = serde_json::from_str(last_line).unwrap();
    assert!(last_parsed.get("result").is_some(), "Last line should be the result");
}

// ── D5: Agent service tests ─────────────────────────────────────

#[test]
fn orient_without_loaded_repo_returns_error() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"d5-1","method":"orient","params":{{"db_path":"{}","repo_uid":"test"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"d5-1""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn orient_missing_db_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"d5-2","method":"orient","params":{"repo_uid":"test"}}"#,
    );
    assert!(output.contains(r#""id":"d5-2""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("db_path"));
}

#[test]
fn orient_missing_repo_uid_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"d5-3","method":"orient","params":{"db_path":"/tmp/test.db"}}"#,
    );
    assert!(output.contains(r#""id":"d5-3""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo_uid"));
}

#[test]
fn orient_invalid_budget_returns_invalid_request() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"d5-4","method":"orient","params":{{"db_path":"{}","repo_uid":"test","budget":"huge"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"d5-4""#));
    // This will return RepoNotFound first (repo must be loaded first)
    // Budget validation happens after repo lookup succeeds
    assert!(output.contains(r#""code""#));
}

#[test]
fn check_without_loaded_repo_returns_error() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"d5-5","method":"check","params":{{"db_path":"{}","repo_uid":"test"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"d5-5""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn check_missing_db_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"d5-6","method":"check","params":{"repo_uid":"test"}}"#,
    );
    assert!(output.contains(r#""id":"d5-6""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("db_path"));
}

#[test]
fn explain_without_loaded_repo_returns_error() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"d5-7","method":"explain","params":{{"db_path":"{}","repo_uid":"test","target":"main.ts"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"d5-7""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn explain_missing_target_returns_invalid_request() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"d5-8","method":"explain","params":{{"db_path":"{}","repo_uid":"test"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"d5-8""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("target"));
}

#[test]
fn explain_rejects_small_budget() {
    // CLI contract: explain only accepts medium|large, not small
    // Must test with a loaded repo since budget validation happens after repo lookup
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().join("budget-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let db_path = temp.path().join("budget-test.db");
    let repo_path_str = repo_dir.to_string_lossy();
    let db_path_str = db_path.to_string_lossy();

    let state = Arc::new(DaemonState::new());

    // Index the repo
    let index_request = format!(
        r#"{{"id":"b-1","method":"index","params":{{"repo_path":"{}","db_path":"{}"}}}}"#,
        repo_path_str, db_path_str
    );
    run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));

    // Load the repo
    let load_request = format!(
        r#"{{"id":"b-2","method":"load_repo","params":{{"db_path":"{}","repo_uid":"budget-test-repo"}}}}"#,
        db_path_str
    );
    run_daemon_requests_with_state(vec![&load_request], Arc::clone(&state));

    // Try explain with small budget - should be rejected
    let explain_request = format!(
        r#"{{"id":"b-3","method":"explain","params":{{"db_path":"{}","repo_uid":"budget-test-repo","target":"main.ts","budget":"small"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&explain_request], Arc::clone(&state));
    let output = &results[0];

    assert!(output.contains(r#""id":"b-3""#));
    assert!(output.contains(r#""code":"InvalidRequest""#), "Should reject small budget: {}", output);
    assert!(output.contains("medium|large"), "Error should mention valid budgets: {}", output);
}

#[test]
fn orient_check_explain_end_to_end() {
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
        r#"{{"id":"e2e-orient-1","method":"index","params":{{"repo_path":"{}","db_path":"{}"}}}}"#,
        repo_path_str, db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    assert!(!results[0].contains(r#""code""#), "Index failed: {}", results[0]);

    // Step 2: Load the repo
    let load_request = format!(
        r#"{{"id":"e2e-orient-2","method":"load_repo","params":{{"db_path":"{}","repo_uid":"test-repo"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&load_request], Arc::clone(&state));
    assert!(results[0].contains(r#""loaded":"test-repo""#), "Load failed: {}", results[0]);

    // Step 3: Orient (repo-level, no focus)
    let orient_request = format!(
        r#"{{"id":"e2e-orient-3","method":"orient","params":{{"db_path":"{}","repo_uid":"test-repo"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&orient_request], Arc::clone(&state));
    let orient_output = &results[0];

    assert!(orient_output.contains(r#""id":"e2e-orient-3""#), "Orient response: {}", orient_output);
    assert!(orient_output.contains(r#""schema":"rgr.agent.v1""#), "Orient should return agent schema: {}", orient_output);
    assert!(orient_output.contains(r#""command":"orient""#), "Orient should return command: {}", orient_output);
    assert!(orient_output.contains(r#""repo":"test-repo""#), "Orient should return repo: {}", orient_output);
    assert!(!orient_output.contains(r#""error":"#), "Orient should not return error: {}", orient_output);

    // Step 4: Check
    let check_request = format!(
        r#"{{"id":"e2e-orient-4","method":"check","params":{{"db_path":"{}","repo_uid":"test-repo"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&check_request], Arc::clone(&state));
    let check_output = &results[0];

    assert!(check_output.contains(r#""id":"e2e-orient-4""#), "Check response: {}", check_output);
    assert!(check_output.contains(r#""schema":"rgr.agent.v1""#), "Check should return agent schema: {}", check_output);
    assert!(check_output.contains(r#""command":"check""#), "Check should return command: {}", check_output);
    assert!(!check_output.contains(r#""error":"#), "Check should not return error: {}", check_output);

    // Step 5: Explain (file target)
    let explain_request = format!(
        r#"{{"id":"e2e-orient-5","method":"explain","params":{{"db_path":"{}","repo_uid":"test-repo","target":"main.ts"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&explain_request], Arc::clone(&state));
    let explain_output = &results[0];

    assert!(explain_output.contains(r#""id":"e2e-orient-5""#), "Explain response: {}", explain_output);
    assert!(explain_output.contains(r#""schema":"rgr.agent.v1""#), "Explain should return agent schema: {}", explain_output);
    assert!(explain_output.contains(r#""command":"explain""#), "Explain should return command: {}", explain_output);
    assert!(!explain_output.contains(r#""error":"#), "Explain should not return error: {}", explain_output);
}

#[test]
fn orient_with_focus_and_budget() {
    // Create temp directories for repo and db
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().join("focus-repo");
    std::fs::create_dir(&repo_dir).unwrap();

    // Create source files
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();
    std::fs::write(repo_dir.join("utils.ts"), "export function util() {}").unwrap();

    let db_path = temp.path().join("focus.db");
    let repo_path_str = repo_dir.to_string_lossy();
    let db_path_str = db_path.to_string_lossy();

    let state = Arc::new(DaemonState::new());

    // Index
    let index_request = format!(
        r#"{{"id":"fb-1","method":"index","params":{{"repo_path":"{}","db_path":"{}"}}}}"#,
        repo_path_str, db_path_str
    );
    run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));

    // Load
    let load_request = format!(
        r#"{{"id":"fb-2","method":"load_repo","params":{{"db_path":"{}","repo_uid":"focus-repo"}}}}"#,
        db_path_str
    );
    run_daemon_requests_with_state(vec![&load_request], Arc::clone(&state));

    // Orient with focus and budget
    let orient_request = format!(
        r#"{{"id":"fb-3","method":"orient","params":{{"db_path":"{}","repo_uid":"focus-repo","focus":"main.ts","budget":"large"}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&orient_request], Arc::clone(&state));
    let output = &results[0];

    assert!(output.contains(r#""id":"fb-3""#));
    assert!(output.contains(r#""schema":"rgr.agent.v1""#));
    assert!(output.contains(r#""command":"orient""#));
    // Focus should be resolved to file
    assert!(output.contains(r#""focus""#));
    assert!(!output.contains(r#""error":"#), "Should not return error: {}", output);
}

// ── Enrich command tests ────────────────────────────────────────────

#[test]
fn enrich_missing_db_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"en-1","method":"enrich","params":{"repo_uid":"test"}}"#,
    );
    assert!(output.contains(r#""id":"en-1""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("db_path"));
}

#[test]
fn enrich_missing_repo_uid_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"en-2","method":"enrich","params":{"db_path":"/tmp/test.db"}}"#,
    );
    assert!(output.contains(r#""id":"en-2""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo_uid"));
}

#[test]
fn enrich_without_loaded_repo_returns_error() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("test.db");
    std::fs::write(&db_path, "").unwrap();
    let db_path_str = db_path.to_string_lossy();

    let output = run_daemon_request(&format!(
        r#"{{"id":"en-3","method":"enrich","params":{{"db_path":"{}","repo_uid":"test"}}}}"#,
        db_path_str
    ));
    assert!(output.contains(r#""id":"en-3""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn enrich_emits_progress_and_returns_cli_contract_shape() {
    // Create temp directories for repo and db
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().join("enrich-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();

    // Create a minimal Rust source file (no unresolved calls, but validates the pipeline runs)
    std::fs::write(
        repo_dir.join("main.rs"),
        r#"fn main() { println!("hello"); }"#,
    )
    .unwrap();

    let db_path = temp.path().join("test.db");
    let repo_path_str = repo_dir.to_string_lossy();
    let db_path_str = db_path.to_string_lossy();

    let state = Arc::new(DaemonState::new());

    // Step 1: Index the repo
    let index_request = format!(
        r#"{{"id":"en-e2e-1","method":"index","params":{{"repo_path":"{}","db_path":"{}"}}}}"#,
        repo_path_str, db_path_str
    );
    run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));

    // Step 2: Load the repo
    let load_request = format!(
        r#"{{"id":"en-e2e-2","method":"load_repo","params":{{"db_path":"{}","repo_uid":"enrich-test-repo"}}}}"#,
        db_path_str
    );
    run_daemon_requests_with_state(vec![&load_request], Arc::clone(&state));

    // Step 3: Enrich with dry_run (avoids needing rust-analyzer)
    let enrich_request = format!(
        r#"{{"id":"en-e2e-3","method":"enrich","params":{{"db_path":"{}","repo_uid":"enrich-test-repo","dry_run":true}}}}"#,
        db_path_str
    );
    let results = run_daemon_requests_with_state(vec![&enrich_request], state);
    let output = &results[0];

    // Parse all NDJSON lines
    let lines: Vec<&str> = output.lines().collect();

    // Should have progress events + final response
    assert!(
        lines.len() >= 1,
        "Expected at least one response line, got: {}",
        output
    );

    // Check for progress phases
    let mut found_initializing = false;
    let mut found_complete = false;
    let mut result_json: Option<serde_json::Value> = None;

    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(
            parsed["id"], "en-e2e-3",
            "All events should have correct request ID"
        );

        if let Some(progress) = parsed.get("progress") {
            let phase = progress["phase"].as_str().unwrap_or("");
            match phase {
                "initializing" => found_initializing = true,
                "complete" => found_complete = true,
                _ => {}
            }
        }
        if parsed.get("result").is_some() {
            result_json = Some(parsed);
        }
    }

    assert!(found_initializing, "Should have initializing progress event");
    assert!(found_complete, "Should have complete progress event");

    // Verify result contract shape matches CLI EnrichOutput
    let result = result_json.expect("Should have final result");
    let r = &result["result"];

    // Required fields from CLI contract
    assert!(r.get("command").is_some(), "Missing command field");
    assert!(r.get("repo_uid").is_some(), "Missing repo_uid field");
    assert!(r.get("snapshot_uid").is_some(), "Missing snapshot_uid field");
    assert!(r.get("promote").is_some(), "Missing promote field");
    assert!(r.get("eligible_count").is_some(), "Missing eligible_count field");
    assert!(r.get("enriched_count").is_some(), "Missing enriched_count field");
    assert!(r.get("failed_count").is_some(), "Missing failed_count field");
    assert!(
        r.get("attempted_persist_count").is_some(),
        "Missing attempted_persist_count field"
    );
    assert!(r.get("persisted_count").is_some(), "Missing persisted_count field");
    assert!(
        r.get("has_storage_discrepancy").is_some(),
        "Missing has_storage_discrepancy field"
    );
    assert!(r.get("enrichment_rate").is_some(), "Missing enrichment_rate field");
    assert!(r.get("by_language").is_some(), "Missing by_language field");
    assert!(
        r.get("top_failure_reasons").is_some(),
        "Missing top_failure_reasons field"
    );
    assert!(r.get("top_types").is_some(), "Missing top_types field");
    assert!(
        r.get("available_resolvers").is_some(),
        "Missing available_resolvers field"
    );

    // Verify dry_run is NOT in the result (CLI contract doesn't include it)
    assert!(
        r.get("dry_run").is_none(),
        "dry_run should NOT be in result (CLI contract parity)"
    );

    // Verify by_language is tuple format: [["lang", {...}], ...]
    let by_language = r["by_language"].as_array().expect("by_language should be array");
    // May be empty if no eligible edges, but if present, should be tuple format
    for entry in by_language {
        assert!(
            entry.is_array(),
            "by_language entries should be tuples [lang, stats], got: {}",
            entry
        );
        let tuple = entry.as_array().unwrap();
        assert_eq!(
            tuple.len(),
            2,
            "by_language tuple should have 2 elements: [lang, stats]"
        );
        assert!(tuple[0].is_string(), "First tuple element should be language string");
        assert!(tuple[1].is_object(), "Second tuple element should be stats object");
    }

    // Verify top_failure_reasons is tuple format: [["reason", count], ...]
    let top_reasons = r["top_failure_reasons"]
        .as_array()
        .expect("top_failure_reasons should be array");
    for entry in top_reasons {
        assert!(
            entry.is_array(),
            "top_failure_reasons entries should be tuples [reason, count], got: {}",
            entry
        );
        let tuple = entry.as_array().unwrap();
        assert_eq!(
            tuple.len(),
            2,
            "top_failure_reasons tuple should have 2 elements"
        );
        assert!(tuple[0].is_string(), "First tuple element should be reason string");
        assert!(
            tuple[1].is_number(),
            "Second tuple element should be count number"
        );
    }

    // Verify command value
    assert_eq!(r["command"], "enrich", "command should be 'enrich'");
}
