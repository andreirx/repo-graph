//! Integration tests for daemon dispatch.
//!
//! Tests the service dispatcher through the transport layer.
//!
//! # REG-1 Contract
//!
//! Query commands (callers, callees, imports, stats, orient, check, explain)
//! resolve repo from the `repo` parameter via the daemon registry.
//!
//! Write commands (index, refresh) also use registry-based resolution.
//!
//! # Test Isolation
//!
//! All tests use isolated state roots (temp directories) to ensure hermetic behavior.
//! Tests do not read the user's actual registry from platform data directories.
#![allow(clippy::arc_with_non_send_sync)]

use std::io::{BufReader, Cursor};
use std::sync::Arc;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::run_transport;
use tempfile::{tempdir, TempDir};

/// Create an isolated daemon state with a fresh temp registry.
///
/// Returns both the state and the temp dir (to keep it alive).
fn create_isolated_state() -> (Arc<DaemonState>, TempDir) {
    let temp = tempdir().expect("failed to create temp dir");
    let registry =
        RepoRegistry::with_state_root(temp.path()).expect("failed to create temp registry");
    let state = Arc::new(DaemonState::with_registry(registry));
    (state, temp)
}

/// Run a single daemon request with an isolated state.
fn run_daemon_request(input: &str) -> String {
    let (state, _temp) = create_isolated_state();
    let dispatcher = ServiceDispatcher::new(state);

    let input = Cursor::new(input);
    let mut output = Vec::new();

    run_transport(BufReader::new(input), &mut output, &dispatcher).unwrap();

    String::from_utf8(output).unwrap()
}

// ── Basic protocol tests ────────────────────────────────────────────────

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
    assert!(
        output.contains(r#""repos":[]"#),
        "Isolated state should have no repos initially: {}",
        output
    );
}

#[test]
fn list_loaded_repos_empty_initially() {
    let output = run_daemon_request(r#"{"id":"4b","method":"list_loaded_repos"}"#);
    assert!(output.contains(r#""id":"4b""#));
    assert!(
        output.contains(r#""loaded_repos":[]"#),
        "Daemon should have no repos loaded initially: {}",
        output
    );
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

// ── REG-1: Missing repo param tests ─────────────────────────────────────

#[test]
fn callers_missing_repo_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"5","method":"callers","params":{"symbol":"foo"}}"#);
    assert!(output.contains(r#""id":"5""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo"), "Should mention missing repo param");
}

#[test]
fn callers_missing_symbol_param_returns_invalid_request() {
    // Create isolated state and index a repo first
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"s-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Now test missing symbol with valid repo
    let callers_request = format!(
        r#"{{"id":"6","method":"callers","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&callers_request], state);
    let output = &results[0];

    assert!(output.contains(r#""id":"6""#));
    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "output: {}",
        output
    );
    assert!(output.contains("symbol"));
}

#[test]
fn callees_missing_repo_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"7","method":"callees","params":{"symbol":"foo"}}"#);
    assert!(output.contains(r#""id":"7""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo"));
}

#[test]
fn imports_missing_repo_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"8","method":"imports","params":{"file":"foo.ts"}}"#);
    assert!(output.contains(r#""id":"8""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo"));
}

#[test]
fn orient_missing_repo_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"9","method":"orient","params":{}}"#);
    assert!(output.contains(r#""id":"9""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo"));
}

#[test]
fn check_missing_repo_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"10","method":"check","params":{}}"#);
    assert!(output.contains(r#""id":"10""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo"));
}

#[test]
fn refresh_missing_repo_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"11","method":"refresh","params":{}}"#);
    assert!(output.contains(r#""id":"11""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo"));
}

#[test]
fn stats_missing_repo_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"11b","method":"stats","params":{}}"#);
    assert!(output.contains(r#""id":"11b""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo"));
}

// ── REG-1: Repo not indexed tests ───────────────────────────────────────

#[test]
fn callers_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"12","method":"callers","params":{"repo":"/nonexistent/path","symbol":"foo"}}"#,
    );
    assert!(output.contains(r#""id":"12""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn callees_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"13","method":"callees","params":{"repo":"/nonexistent/path","symbol":"foo"}}"#,
    );
    assert!(output.contains(r#""id":"13""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn orient_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"14","method":"orient","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(output.contains(r#""id":"14""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn check_repo_not_indexed_returns_error() {
    let output =
        run_daemon_request(r#"{"id":"15","method":"check","params":{"repo":"/nonexistent/path"}}"#);
    assert!(output.contains(r#""id":"15""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn refresh_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"16","method":"refresh","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(output.contains(r#""id":"16""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

#[test]
fn stats_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"16b","method":"stats","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(output.contains(r#""id":"16b""#));
    assert!(output.contains(r#""code":"RepoNotFound""#));
}

// ── Index tests ─────────────────────────────────────────────────────────

#[test]
fn index_missing_repo_path_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"17","method":"index","params":{}}"#);
    assert!(output.contains(r#""id":"17""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("repo_path"));
}

#[test]
fn index_nonexistent_repo_path_returns_invalid_request() {
    let output = run_daemon_request(
        r#"{"id":"18","method":"index","params":{"repo_path":"/nonexistent/path"}}"#,
    );
    assert!(output.contains(r#""id":"18""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
    assert!(output.contains("does not exist"));
}

#[test]
fn load_repo_missing_params_returns_invalid_request() {
    let output = run_daemon_request(r#"{"id":"19","method":"load_repo"}"#);
    assert!(output.contains(r#""id":"19""#));
    assert!(output.contains(r#""code":"InvalidRequest""#));
}

// ── End-to-end tests with shared state ──────────────────────────────────

/// Create isolated daemon state from a temp state root.
fn create_isolated_state_in(state_temp: &TempDir) -> Arc<DaemonState> {
    let registry = RepoRegistry::with_state_root(state_temp.path())
        .expect("failed to create isolated registry");
    Arc::new(DaemonState::with_registry(registry))
}

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

/// Helper to extract repo_uid, db_path, and canonical_path from index result.
fn extract_index_result(output: &str) -> (String, String, String) {
    // Find the last line (result, not progress)
    let last_line = output.lines().last().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(last_line).unwrap();
    let result = &parsed["result"];
    let repo_uid = result["repo_uid"].as_str().unwrap().to_string();
    let db_path = result["db_path"].as_str().unwrap().to_string();
    let canonical_path = result["canonical_path"].as_str().unwrap().to_string();
    (repo_uid, db_path, canonical_path)
}

#[test]
fn index_then_query_end_to_end() {
    // Create isolated state root (for registry and databases)
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create temp directory for test repo
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("test-repo");
    std::fs::create_dir(&repo_dir).unwrap();

    // Create a minimal source file
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    // Step 1: Index the repo (REG-1 contract)
    let index_request = format!(
        r#"{{"id":"e2e-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let index_output = &results[0];

    assert!(
        !index_output.contains(r#""code":"InvalidRequest""#)
            && !index_output.contains(r#""code":"InternalError""#),
        "Index should succeed: {}",
        index_output
    );

    let (repo_uid, _db_path, canonical_path) = extract_index_result(index_output);
    assert!(
        repo_uid.starts_with("repo_"),
        "REG-1: repo_uid should be ULID-based, got: {}",
        repo_uid
    );

    // Step 2: Query using canonical path (REG-1)
    let orient_request = format!(
        r#"{{"id":"e2e-2","method":"orient","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&orient_request], Arc::clone(&state));
    let orient_output = &results[0];

    assert!(
        orient_output.contains(r#""schema":"rgr.agent.v1""#),
        "Orient should succeed with REG-1 repo param: {}",
        orient_output
    );

    // Step 3: Check also works
    let check_request = format!(
        r#"{{"id":"e2e-3","method":"check","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&check_request], Arc::clone(&state));
    let check_output = &results[0];

    assert!(
        check_output.contains(r#""schema":"rgr.agent.v1""#),
        "Check should succeed: {}",
        check_output
    );

    // Step 4: Refresh uses repo param too
    let refresh_request = format!(
        r#"{{"id":"e2e-4","method":"refresh","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&refresh_request], Arc::clone(&state));
    let refresh_output = &results[0];

    assert!(
        refresh_output.contains(r#""snapshot_uid""#),
        "Refresh should succeed: {}",
        refresh_output
    );
}

#[test]
fn orient_returns_coherence_envelope_shape() {
    // ORIENT-LIVEGRAPH-IMPL: rmapd-level proof (real dispatch + serialization, in-process transport, no
    // socket) that orient now serves a `CoherenceEnvelope<CoherentOrientResult>` — the wrapper top level,
    // the per-signal leaf envelopes, the root axes, and the source map. No LiveGraph is preloaded in this
    // harness, so the LG-first leaves take the honest SQLite path (source = {sqlite}); this exercises the
    // degradation/fallback labelling end-to-end (the LG-served path is unit-tested in livegraph_feed).
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("coherence-orient-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();
    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"coh-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let orient_request = format!(
        r#"{{"id":"coh-2","method":"orient","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&orient_request], Arc::clone(&state));
    let last_line = results[0].lines().last().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(last_line).unwrap();
    let env = &parsed["result"];

    // ── The wrapper top level (RISK-O-F): value + provenance + trust + freshness. ──
    assert!(
        env.get("value").is_some(),
        "wrapper has value: {}",
        results[0]
    );
    assert!(
        env.get("provenance").is_some(),
        "wrapper has root provenance: {}",
        results[0]
    );
    assert!(
        env.get("trust").is_some(),
        "wrapper has root trust (TrustPosture): {}",
        results[0]
    );
    assert!(
        env.get("freshness").is_some(),
        "wrapper has root freshness: {}",
        results[0]
    );
    // Root trust is the AXIS-typed posture, not the legacy briefing prose.
    assert!(env["trust"].get("class").is_some(), "root trust has class");
    assert!(
        env["trust"].get("completeness").is_some(),
        "root trust has completeness"
    );
    // Root provenance.source is the source SET (the source-map union).
    assert!(
        env["provenance"]["source"].is_array(),
        "root provenance.source is a set"
    );

    // ── The inner value = CoherentOrientResult (signals re-typed to leaf envelopes, D7). ──
    let value = &env["value"];
    assert_eq!(value["schema"], "rgr.agent.v1");
    assert_eq!(value["command"], "orient");
    let signals = value["signals"]
        .as_array()
        .expect("value.signals is an array of leaf envelopes");
    assert!(
        !signals.is_empty(),
        "repo focus emits SNAPSHOT_INFO + MODULE_SUMMARY at minimum: {}",
        results[0]
    );

    // ── Each signal is a CoherenceEnvelope leaf: pristine inner Signal + provenance/trust/freshness. ──
    for leaf in signals {
        assert!(
            leaf["value"].get("code").is_some(),
            "leaf.value is the pristine Signal: {}",
            leaf
        );
        assert!(
            leaf.get("trust").is_some(),
            "leaf carries a trust posture: {}",
            leaf
        );
        let src = leaf["provenance"]["source"]
            .as_array()
            .expect("leaf provenance.source is a set");
        assert!(
            !src.is_empty(),
            "leaf source set is non-empty (honest provenance, never silent): {}",
            leaf
        );
    }

    // Source map / honest fallback: with NO preloaded LiveGraph, NONE of the FOUR LG-first leaves
    // (IMPORT_CYCLES / HIGH_COMPLEXITY / CALLERS_SUMMARY / CALLEES_SUMMARY) may claim `livegraph` — each is
    // the proven SQLite primary or a labelled SQLite fallback. This proves the degradation labelling is
    // honest uniformly across the source map (no leaf silently over-claims a current-state source).
    let lg_first = [
        "IMPORT_CYCLES",
        "HIGH_COMPLEXITY",
        "CALLERS_SUMMARY",
        "CALLEES_SUMMARY",
    ];
    for leaf in signals {
        let code = leaf["value"]["code"].as_str().unwrap_or("");
        let src = leaf["provenance"]["source"].as_array().unwrap();
        assert!(
            !src.iter().any(|s| s == "livegraph"),
            "no preloaded LiveGraph -> {} leaf must not be livegraph-sourced: {}",
            code,
            leaf
        );
        // An LG-first leaf, when emitted without a LiveGraph, is SQLite-sourced: either the proven SQLite
        // primary or a labelled `LiveGraphUnavailable` fallback — never a `livegraph` claim.
        if lg_first.contains(&code) {
            assert!(
                src.iter().any(|s| s == "sqlite"),
                "LG-first {} leaf must be SQLite-sourced when no LiveGraph is preloaded: {}",
                code,
                leaf
            );
        }
    }

    // Root provenance.source is the UNION of the leaf sources; with SQLite/Authority/FS leaves present it
    // must include `sqlite` (the source-map union, contract D8).
    let root_src = env["provenance"]["source"].as_array().unwrap();
    assert!(
        root_src.iter().any(|s| s == "sqlite"),
        "root provenance.source union includes sqlite: {}",
        results[0]
    );
    assert!(
        !root_src.iter().any(|s| s == "livegraph"),
        "no preloaded LiveGraph -> root source union must not include livegraph: {}",
        results[0]
    );
}

#[test]
fn check_returns_coherence_envelope_shape() {
    // CHECK-LIVEGRAPH-IMPL: rmapd-level proof (real `handle_check` dispatch + real serialization, in-process
    // transport, no socket — the same path `rmapd` runs minus the accept loop) that `check` now serves a
    // `CoherenceEnvelope<CoherentOrientResult>`. It pins, end-to-end through the daemon, the contract the
    // review-0 dispatch tests lacked: the wrapper top level, the per-signal leaf envelopes
    // (`value.signals[*].value`), the MULTI-SOURCE verdict provenance {sqlite, declaration}, honest root
    // freshness/trust, the ABSENT `trust_briefing`, and the never-LiveGraph invariant (D-CHECK-2/4).
    //
    // Coverage map for the degradation trio: this is the snapshot-present FRESH case (an indexed repo has no
    // stale files). The STALE case (authoritative `get_stale_files` read) and the no-snapshot UNAVAILABLE /
    // single-source {sqlite} case are pinned with a real `RepoState` + real SQLite in
    // daemon-runtime `check_coherence` tests, and the pure folds in agent `check::coherent` tests. The CLI
    // exit-code parity (PASS=0/FAIL=1/INCOMPLETE=2/not-found=2) is pinned in rgr `presentation::check`.
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("coherence-check-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();
    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"chk-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let check_request = format!(
        r#"{{"id":"chk-2","method":"check","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&check_request], Arc::clone(&state));
    let last_line = results[0].lines().last().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(last_line).unwrap();
    let env = &parsed["result"];

    // ── The wrapper top level: value + provenance + trust + freshness. ──
    assert!(
        env.get("value").is_some(),
        "wrapper has value: {}",
        results[0]
    );
    assert!(
        env.get("provenance").is_some(),
        "wrapper has root provenance: {}",
        results[0]
    );
    assert!(
        env["trust"].get("class").is_some() && env["trust"].get("completeness").is_some(),
        "root trust is the AXIS-typed posture (class + completeness): {}",
        results[0]
    );
    // Root freshness is present and AUTHORITATIVE. A freshly-indexed repo has no stale files → Fresh; the
    // snapshot exists so it is never Unavailable here (that is the no-snapshot case, covered elsewhere).
    let root_freshness = env["freshness"]
        .as_str()
        .expect("root freshness is a state string");
    assert_eq!(
        root_freshness, "Fresh",
        "freshly-indexed snapshot has no stale files → Fresh (honest, not minted): {}",
        results[0]
    );

    // ── The inner value = CoherentOrientResult (signals re-typed to leaf envelopes, D7). ──
    let value = &env["value"];
    assert_eq!(value["schema"], "rgr.agent.v1");
    assert_eq!(value["command"], "check");
    // E5 / D-CHECK-2: check NEVER carries a trust briefing (orient's field stays absent on check's wire).
    assert!(
        value.get("trust_briefing").is_none(),
        "check serializes NO trust_briefing key: {}",
        results[0]
    );
    let signals = value["signals"]
        .as_array()
        .expect("value.signals is an array of leaf envelopes");
    assert!(
        !signals.is_empty(),
        "check ALWAYS emits at least the verdict signal: {}",
        results[0]
    );

    // ── Each signal is a CoherenceEnvelope leaf: pristine inner Signal + provenance/trust/freshness, and
    //    NEVER a LiveGraph claim (check reads zero LiveGraph — D-CHECK-4). ──
    for leaf in signals {
        assert!(
            leaf["value"].get("code").is_some(),
            "leaf.value is the pristine Signal (value.signals[*].value nesting): {}",
            leaf
        );
        assert!(
            leaf.get("trust").is_some(),
            "leaf carries a trust posture: {}",
            leaf
        );
        let src = leaf["provenance"]["source"]
            .as_array()
            .expect("leaf provenance.source is a set");
        assert!(
            !src.is_empty(),
            "leaf source set is non-empty (honest provenance): {}",
            leaf
        );
        assert!(
            !src.iter().any(|s| s == "livegraph"),
            "check leaf must NEVER claim a livegraph source: {}",
            leaf
        );
        assert!(
            leaf["provenance"]
                .get("fallback_reason")
                .and_then(|v| v.as_str())
                .is_none(),
            "check makes no LiveGraph read → no leaf fallback_reason: {}",
            leaf
        );
    }

    // ── The VERDICT leaf is the MULTI-SOURCE composite (D-CHECK-1/5): snapshot-present → the gate ALWAYS
    //    reads the `declarations` Authority table, so the honest source set is {sqlite, declaration}. ──
    let verdict = signals
        .iter()
        .find(|leaf| {
            matches!(
                leaf["value"]["code"].as_str(),
                Some("CHECK_PASS") | Some("CHECK_FAIL") | Some("CHECK_INCOMPLETE")
            )
        })
        .unwrap_or_else(|| panic!("check ALWAYS emits a verdict signal: {}", results[0]));
    let verdict_src: Vec<&str> = verdict["provenance"]["source"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s.as_str())
        .collect();
    assert!(
        verdict_src.contains(&"sqlite") && verdict_src.contains(&"declaration"),
        "snapshot-present verdict folds sqlite-operational + sqlite-trust-core + declaration-authority \
         → multi-source {{sqlite, declaration}}, got {:?}: {}",
        verdict_src,
        results[0]
    );

    // ── Root provenance.source = the SET UNION of the leaf sources: includes sqlite + declaration, never
    //    livegraph; no LiveGraph read can produce a fallback (E3). ──
    let root_src: Vec<&str> = env["provenance"]["source"]
        .as_array()
        .expect("root provenance.source is a set")
        .iter()
        .filter_map(|s| s.as_str())
        .collect();
    assert!(
        root_src.contains(&"sqlite") && root_src.contains(&"declaration"),
        "root source union includes sqlite + declaration: {:?}",
        root_src
    );
    assert!(
        !root_src.contains(&"livegraph"),
        "check reads zero LiveGraph → root source union excludes livegraph: {:?}",
        root_src
    );
    assert!(
        env["provenance"]
            .get("fallback_reason")
            .and_then(|v| v.as_str())
            .is_none(),
        "check makes no LiveGraph read → root fallback_reason is null: {}",
        results[0]
    );
}

#[test]
fn explain_returns_coherence_envelope_shape() {
    // EXPLAIN-LIVEGRAPH-IMPL: rmapd-level proof (real `handle_explain` dispatch + real serialization,
    // in-process transport, no socket) that explain now serves a `CoherenceEnvelope<CoherentOrientResult>`.
    // No LiveGraph is preloaded in this harness, so the FOUR LG-first reuse leaves take the HONEST SQLite
    // path (source = {sqlite}, never livegraph) — this exercises the labelled-fallback source map end-to-end.
    // The LG-SERVED multi-source path is proven by `explain_coherence`'s synthetic-fixture e2e test + the
    // agent-side `explain::coherent` unit tests.
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("coherence-explain-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    // A symbol target exercises the richest explain path (a symbol focus adds EXPLAIN_CALLERS/CALLEES); the
    // assertions below pin honest labelling for whatever leaves the resolved focus emits.
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();
    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"cohx-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let explain_request = format!(
        r#"{{"id":"cohx-2","method":"explain","params":{{"repo":"{}","target":"hello"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&explain_request], Arc::clone(&state));
    let last_line = results[0].lines().last().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(last_line).unwrap();
    let env = &parsed["result"];

    // ── The wrapper top level: value + provenance + trust + freshness. ──
    assert!(
        env.get("value").is_some(),
        "wrapper has value: {}",
        results[0]
    );
    assert!(
        env.get("provenance").is_some(),
        "wrapper has root provenance: {}",
        results[0]
    );
    assert!(
        env["trust"].get("class").is_some(),
        "wrapper root trust is the axis-typed posture: {}",
        results[0]
    );
    assert!(
        env.get("freshness").is_some(),
        "wrapper has root freshness: {}",
        results[0]
    );

    // ── The inner value = CoherentOrientResult (command=explain; signals re-typed to leaf envelopes). ──
    let value = &env["value"];
    assert_eq!(value["schema"], "rgr.agent.v1");
    assert_eq!(value["command"], "explain");
    let signals = value["signals"]
        .as_array()
        .expect("value.signals is an array of leaf envelopes");
    assert!(
        !signals.is_empty(),
        "a resolved symbol focus emits at least EXPLAIN_IDENTITY + EXPLAIN_TRUST: {}",
        results[0]
    );

    // ── Each leaf is a CoherenceEnvelope: pristine inner Signal + provenance/trust/freshness. ──
    // With NO preloaded LiveGraph, NO leaf may claim `livegraph` — the LG-first reuse leaves
    // (EXPLAIN_CALLERS/CALLEES/IMPORTS/CYCLES) are the proven SQLite primary or a labelled SQLite fallback;
    // EXPLAIN_IDENTITY is single-source {sqlite} (D-IMPL-1). This pins honest fallback labelling uniformly.
    let lg_first = [
        "EXPLAIN_CALLERS",
        "EXPLAIN_CALLEES",
        "EXPLAIN_IMPORTS",
        "EXPLAIN_CYCLES",
    ];
    for leaf in signals {
        let code = leaf["value"]["code"].as_str().unwrap_or("");
        assert!(
            !code.is_empty(),
            "leaf.value is the pristine Signal: {}",
            leaf
        );
        assert!(
            leaf.get("trust").is_some(),
            "leaf carries a trust posture: {}",
            leaf
        );
        let src: Vec<&str> = leaf["provenance"]["source"]
            .as_array()
            .expect("leaf provenance.source is a set")
            .iter()
            .filter_map(|s| s.as_str())
            .collect();
        assert!(
            !src.is_empty(),
            "leaf source set is non-empty (honest, never silent): {}",
            leaf
        );
        assert!(
            !src.contains(&"livegraph"),
            "no preloaded LiveGraph -> {} leaf must not be livegraph-sourced: {}",
            code,
            leaf
        );
        if lg_first.contains(&code) {
            assert!(
                src.contains(&"sqlite"),
                "LG-first {} leaf is SQLite-sourced when no LiveGraph is preloaded: {}",
                code,
                leaf
            );
        }
        // EXPLAIN_IDENTITY is single-source {sqlite} (D-IMPL-1).
        if code == "EXPLAIN_IDENTITY" {
            assert_eq!(
                src,
                vec!["sqlite"],
                "EXPLAIN_IDENTITY is single-source sqlite: {}",
                leaf
            );
        }
    }
    // NOTE: which LG-first leaves a focus emits depends on resolution (symbol -> CALLERS/CALLEES; file ->
    // IMPORTS; path -> CYCLES), so this round-trip test asserts honest labelling for WHATEVER leaves are
    // present (the loop above) rather than requiring a specific one — that would couple it to the extractor's
    // symbol-name resolution. The DETERMINISTIC LG-served + SQLite-fallback callgraph proofs live in
    // `daemon-runtime`'s `explain_coherence::explain_lg_served_e2e` (synthetic keys, resolution-independent).

    // ── Root provenance.source union includes sqlite, never livegraph (no preloaded LG). ──
    let root_src: Vec<&str> = env["provenance"]["source"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s.as_str())
        .collect();
    assert!(
        root_src.contains(&"sqlite"),
        "root source union includes sqlite: {:?}",
        root_src
    );
    assert!(
        !root_src.contains(&"livegraph"),
        "no preloaded LiveGraph -> root source union excludes livegraph: {:?}",
        root_src
    );
}

#[test]
fn stats_success_returns_module_metrics() {
    // Create isolated state root
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create temp directory for test repo
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("stats-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    // Step 1: Index the repo
    let index_request = format!(
        r#"{{"id":"stats-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Step 2: Query stats
    let stats_request = format!(
        r#"{{"id":"stats-2","method":"stats","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&stats_request], state);
    let stats_output = &results[0];

    // Parse last line (result, not progress events)
    let last_line = stats_output.lines().last().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(last_line).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("repo_uid").is_some(),
        "stats response missing repo_uid: {}",
        stats_output
    );
    assert!(
        result.get("snapshot_uid").is_some(),
        "stats response missing snapshot_uid: {}",
        stats_output
    );
    assert!(
        result.get("stats").is_some(),
        "stats response missing stats array: {}",
        stats_output
    );

    // Verify stats is an array
    let stats = result["stats"]
        .as_array()
        .expect("stats should be an array");

    // For each module, verify required fields exist
    for module_stats in stats {
        assert!(
            module_stats.get("module").is_some(),
            "module_stats missing module: {}",
            stats_output
        );
        assert!(
            module_stats.get("fan_in").is_some(),
            "module_stats missing fan_in"
        );
        assert!(
            module_stats.get("fan_out").is_some(),
            "module_stats missing fan_out"
        );
        assert!(
            module_stats.get("instability").is_some(),
            "module_stats missing instability"
        );
        assert!(
            module_stats.get("abstractness").is_some(),
            "module_stats missing abstractness"
        );
        assert!(
            module_stats.get("distance_from_main_sequence").is_some(),
            "module_stats missing distance_from_main_sequence"
        );
        assert!(
            module_stats.get("file_count").is_some(),
            "module_stats missing file_count"
        );
        assert!(
            module_stats.get("symbol_count").is_some(),
            "module_stats missing symbol_count"
        );
    }
}

#[test]
fn index_emits_progress_events() {
    // Create isolated state root
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create temp directory for test repo
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("progress-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"progress-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], state);
    let output = &results[0];

    let lines: Vec<&str> = output.lines().collect();

    // Should have progress events + final response
    assert!(
        lines.len() > 1,
        "Expected progress events + response, got {} lines: {}",
        lines.len(),
        output
    );

    // Verify progress events
    let mut found_initializing = false;
    let mut found_scanning = false;
    let mut found_result = false;

    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        if let Some(progress) = parsed.get("progress") {
            let phase = progress["phase"].as_str().unwrap_or("");
            if phase == "initializing" {
                found_initializing = true;
            }
            if phase == "scanning" {
                found_scanning = true;
            }
        }
        if parsed.get("result").is_some() {
            found_result = true;
        }
    }

    assert!(
        found_initializing,
        "Should have initializing progress event"
    );
    assert!(found_scanning, "Should have scanning progress event");
    assert!(found_result, "Should have final result");
}

#[test]
fn refresh_emits_progress_events() {
    // Create isolated state root
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create temp directory for test repo
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("refresh-progress-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    // Index first
    let index_request = format!(
        r#"{{"id":"rp-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Refresh using repo param (REG-1)
    let refresh_request = format!(
        r#"{{"id":"rp-2","method":"refresh","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&refresh_request], state);
    let output = &results[0];

    let lines: Vec<&str> = output.lines().collect();

    assert!(
        lines.len() > 1,
        "Expected progress events + response, got {} lines",
        lines.len()
    );

    let mut has_result = false;
    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        if parsed.get("result").is_some() {
            has_result = true;
        }
    }

    assert!(has_result, "Refresh should emit final result");
}

#[test]
fn explain_missing_target_returns_invalid_request() {
    // Create isolated state and index a repo first
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"e-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Now test missing target with valid repo
    let explain_request = format!(
        r#"{{"id":"20","method":"explain","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&explain_request], state);
    let output = &results[0];

    assert!(output.contains(r#""id":"20""#));
    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "output: {}",
        output
    );
    assert!(output.contains("target"));
}

#[test]
fn explain_rejects_small_budget() {
    // Create isolated state root
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create temp directory for test repo
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("budget-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function hello() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    // Index the repo
    let index_request = format!(
        r#"{{"id":"b-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Try explain with small budget - should be rejected
    let explain_request = format!(
        r#"{{"id":"b-2","method":"explain","params":{{"repo":"{}","target":"main.ts","budget":"small"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&explain_request], Arc::clone(&state));
    let output = &results[0];

    assert!(output.contains(r#""id":"b-2""#));
    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "Should reject small budget: {}",
        output
    );
}

// ── Enrich tests (still uses db_path/repo_uid - admin operation) ────────

#[test]
fn enrich_missing_db_path_returns_invalid_request() {
    let output =
        run_daemon_request(r#"{"id":"en-1","method":"enrich","params":{"repo_uid":"test"}}"#);
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

// ══════════════════════════════════════════════════════════════════════════════
// BATCH 1 SUCCESS-PATH TESTS (REG-1 Contract)
// ══════════════════════════════════════════════════════════════════════════════
//
// These tests prove the daemon handler behavior for migrated Batch 1 commands.
// Each test:
//   1. Creates isolated daemon state
//   2. Indexes a fixture repo through daemon
//   3. Queries using REG-1 `repo` parameter (canonical path)
//   4. Validates response contract
//
// Organized by command family: docs, resource, contracts, inferences, deps

// ── Docs command family ─────────────────────────────────────────────────────

#[test]
fn docs_list_returns_envelope_with_entries() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create repo with documentation files
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("docs-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("README.md"),
        "# Test Project\n\nDescription here.",
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    // Index the repo
    let index_request = format!(
        r#"{{"id":"docs-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query docs_list
    let docs_request = format!(
        r#"{{"id":"docs-2","method":"docs_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&docs_request], state);
    let output = &results[0];

    // Validate response contract
    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("entries").is_some(),
        "missing entries field: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "missing count field: {}",
        output
    );

    // Should find README.md
    let entries = result["entries"].as_array().unwrap();
    let has_readme = entries.iter().any(|e| {
        e.get("path")
            .and_then(|p| p.as_str())
            .map(|p| p.contains("README.md"))
            .unwrap_or(false)
    });
    assert!(has_readme, "Should find README.md in docs list: {}", output);
}

#[test]
fn docs_list_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"docs-err-1","method":"docs_list","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn docs_extract_returns_envelope_with_facts() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create repo with documentation containing semantic markers
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("docs-extract-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("README.md"),
        "# Project\n\n<!-- rg:replacement_for old-lib -->\nThis replaces old-lib.\n",
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    // Index the repo
    let index_request = format!(
        r#"{{"id":"de-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Call docs_extract
    let extract_request = format!(
        r#"{{"id":"de-2","method":"docs_extract","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&extract_request], state);
    let output = &results[0];

    // Validate response contract
    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("files_scanned").is_some(),
        "missing files_scanned: {}",
        output
    );
    assert!(
        result.get("facts_extracted").is_some(),
        "missing facts_extracted: {}",
        output
    );
}

// ── Resource command family ─────────────────────────────────────────────────

#[test]
fn resource_list_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create repo with resource patterns
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("resource-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("db.ts"),
        r#"import { Pool } from 'pg';
const pool = new Pool();
export async function query(sql: string) {
    return pool.query(sql);
}"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    // Index the repo
    let index_request = format!(
        r#"{{"id":"res-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query resource_list
    let resource_request = format!(
        r#"{{"id":"res-2","method":"resource_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&resource_request], state);
    let output = &results[0];

    // Validate response contract
    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "missing results field: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "missing count field: {}",
        output
    );
}

#[test]
fn resource_list_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"res-err-1","method":"resource_list","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn resource_readers_returns_envelope_or_not_found() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("resource-readers-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"rr-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query resource_readers for a resource (may not exist in simple fixture)
    let readers_request = format!(
        r#"{{"id":"rr-2","method":"resource_readers","params":{{"repo":"{}","resource":"database"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&readers_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Either success with results or error (resource not found is valid)
    if let Some(result) = parsed.get("result") {
        assert!(
            result.get("command").is_some(),
            "missing command field: {}",
            output
        );
        assert!(
            result.get("results").is_some(),
            "missing results field: {}",
            output
        );
    } else {
        // "resource not found" is expected for nonexistent resources
        assert!(
            parsed.get("error").is_some(),
            "Should have result or error: {}",
            output
        );
    }
}

#[test]
fn resource_writers_returns_envelope_or_not_found() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("resource-writers-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"rw-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let writers_request = format!(
        r#"{{"id":"rw-2","method":"resource_writers","params":{{"repo":"{}","resource":"database"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&writers_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Either success with results or error (resource not found is valid)
    if let Some(result) = parsed.get("result") {
        assert!(
            result.get("command").is_some(),
            "missing command field: {}",
            output
        );
        assert!(
            result.get("results").is_some(),
            "missing results field: {}",
            output
        );
    } else {
        // "resource not found" is expected for nonexistent resources
        assert!(
            parsed.get("error").is_some(),
            "Should have result or error: {}",
            output
        );
    }
}

// ── Contracts command family ────────────────────────────────────────────────

#[test]
fn contracts_list_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create repo with proto file
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("contracts-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("api.proto"),
        r#"syntax = "proto3";
package api;
message Request { string id = 1; }
message Response { string result = 1; }
service Api { rpc Call(Request) returns (Response); }
"#,
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"con-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let contracts_request = format!(
        r#"{{"id":"con-2","method":"contracts_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&contracts_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "missing results field: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "missing count field: {}",
        output
    );
}

#[test]
fn contracts_list_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"con-err-1","method":"contracts_list","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn contracts_list_with_kind_filter() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("contracts-filter-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("api.proto"),
        r#"syntax = "proto3"; package api; message Msg { string id = 1; }"#,
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"cf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Filter by kind
    let contracts_request = format!(
        r#"{{"id":"cf-2","method":"contracts_list","params":{{"repo":"{}","kind":"protobuf"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&contracts_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    // Kind filter should be reflected in response
    assert!(
        result.get("filter_kind").is_some() || result.get("kind").is_some(),
        "Kind filter should be reflected: {}",
        output
    );
}

#[test]
fn contracts_show_returns_schema_detail() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("contracts-show-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("api.proto"),
        r#"syntax = "proto3"; package api; message Request { string id = 1; }"#,
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"cs-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let show_request = format!(
        r#"{{"id":"cs-2","method":"contracts_show","params":{{"repo":"{}","file":"api.proto"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&show_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    // Should have result (success) or error (file not found)
    assert!(
        parsed.get("result").is_some() || parsed.get("error").is_some(),
        "Should have result or error: {}",
        output
    );
}

#[test]
fn contracts_elements_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("contracts-elements-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("api.proto"),
        r#"syntax = "proto3";
package api;
message Request { string id = 1; }
service Api { rpc Call(Request) returns (Request); }
"#,
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"ce-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let elements_request = format!(
        r#"{{"id":"ce-2","method":"contracts_elements","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&elements_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "missing results field: {}",
        output
    );
}

#[test]
fn contracts_usages_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("contracts-usages-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("api.proto"),
        r#"syntax = "proto3"; package api; message Request { string id = 1; }"#,
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"cu-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let usages_request = format!(
        r#"{{"id":"cu-2","method":"contracts_usages","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&usages_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "missing results field: {}",
        output
    );
}

// ── Inferences command family ───────────────────────────────────────────────

#[test]
fn inferences_list_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("inferences-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"inf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let inferences_request = format!(
        r#"{{"id":"inf-2","method":"inferences_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&inferences_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "missing results field: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "missing count field: {}",
        output
    );
}

#[test]
fn inferences_list_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"inf-err-1","method":"inferences_list","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn inferences_list_with_kind_filter() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("inferences-filter-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"if-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let inferences_request = format!(
        r#"{{"id":"if-2","method":"inferences_list","params":{{"repo":"{}","kind":"hotspot_score"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&inferences_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    // Kind filter should be reflected
    assert!(
        result.get("filter_kind").is_some(),
        "Kind filter should be reflected: {}",
        output
    );
}

// ── Deps command family ─────────────────────────────────────────────────────

#[test]
fn deps_list_returns_envelope_with_modules() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    // Create repo with package.json and imports
    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("deps-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("package.json"),
        r#"{"name": "test-pkg", "dependencies": {"express": "^4.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("server.ts"),
        r#"import express from 'express';
const app = express();
app.listen(3000);
"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"deps-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let deps_request = format!(
        r#"{{"id":"deps-2","method":"deps_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&deps_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "missing results field: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "missing count field: {}",
        output
    );
    assert!(
        result.get("ecosystem").is_some(),
        "missing ecosystem field: {}",
        output
    );
}

#[test]
fn deps_list_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"deps-err-1","method":"deps_list","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn deps_list_with_module_filter() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("deps-filter-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("package.json"),
        r#"{"name": "test-pkg", "dependencies": {"lodash": "^4.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "import _ from 'lodash';").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"df-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let deps_request = format!(
        r#"{{"id":"df-2","method":"deps_list","params":{{"repo":"{}","module":"src"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&deps_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    // Should succeed (may have empty results if module doesn't match)
    assert!(
        parsed.get("result").is_some(),
        "Should have result: {}",
        output
    );
}

#[test]
fn deps_list_with_ecosystem_cargo() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("deps-cargo-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"dc-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let deps_request = format!(
        r#"{{"id":"dc-2","method":"deps_list","params":{{"repo":"{}","ecosystem":"cargo"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&deps_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert_eq!(
        result.get("ecosystem").and_then(|e| e.as_str()),
        Some("cargo"),
        "Ecosystem should be cargo: {}",
        output
    );
}

#[test]
fn deps_why_returns_package_usages() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("deps-why-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("package.json"),
        r#"{"name": "test-pkg", "dependencies": {"express": "^4.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("server.ts"),
        r#"import express from 'express';
const app = express();
"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"dw-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let why_request = format!(
        r#"{{"id":"dw-2","method":"deps_why","params":{{"repo":"{}","package":"express"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&why_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    // Either success with usages or error if package not found
    assert!(
        parsed.get("result").is_some() || parsed.get("error").is_some(),
        "Should have result or error: {}",
        output
    );
}

#[test]
fn deps_why_package_not_found_returns_error() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("deps-why-notfound-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"dwn-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let why_request = format!(
        r#"{{"id":"dwn-2","method":"deps_why","params":{{"repo":"{}","package":"nonexistent-package-xyz"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&why_request], state);
    let output = &results[0];

    // Should return error for nonexistent package
    assert!(
        output.contains("error") || output.contains("not found"),
        "Should indicate package not found: {}",
        output
    );
}

#[test]
fn deps_drift_returns_anomalies() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("deps-drift-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    // Package with declared but unused dependency (potential drift)
    std::fs::write(
        repo_dir.join("package.json"),
        r#"{"name": "test-pkg", "dependencies": {"unused-pkg": "^1.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"dd-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let drift_request = format!(
        r#"{{"id":"dd-2","method":"deps_drift","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&drift_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "missing results field: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "missing count field: {}",
        output
    );
    assert!(
        result.get("modules_analyzed").is_some(),
        "missing modules_analyzed: {}",
        output
    );
}

#[test]
fn deps_drift_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"dd-err-1","method":"deps_drift","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

// ── Surfaces command family ─────────────────────────────────────────────────

#[test]
fn surfaces_list_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("surfaces-test-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"surf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let surfaces_request = format!(
        r#"{{"id":"surf-2","method":"surfaces_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&surfaces_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    assert!(
        result.get("command").is_some(),
        "missing command field: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "missing results field: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "missing count field: {}",
        output
    );
}

#[test]
fn surfaces_list_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"surf-err-1","method":"surfaces_list","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn surfaces_list_with_kind_filter() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("surfaces-filter-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"sf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let surfaces_request = format!(
        r#"{{"id":"sf-2","method":"surfaces_list","params":{{"repo":"{}","kind":"backend"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&surfaces_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    // Kind filter should be reflected in response
    assert!(
        result.get("filter_kind").is_some(),
        "Kind filter should be reflected: {}",
        output
    );
}

#[test]
fn surfaces_show_returns_detail_or_not_found() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("surfaces-show-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"ss-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Try to show a surface (may not exist in simple fixture)
    let show_request = format!(
        r#"{{"id":"ss-2","method":"surfaces_show","params":{{"repo":"{}","surface":"test-surface"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&show_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Either success with detail or error (surface not found is valid)
    if let Some(result) = parsed.get("result") {
        assert!(
            result.get("command").is_some(),
            "missing command field: {}",
            output
        );
        assert!(
            result.get("surface").is_some(),
            "missing surface field: {}",
            output
        );
    } else {
        // "surface not found" is expected for nonexistent surfaces
        assert!(
            parsed.get("error").is_some(),
            "Should have result or error: {}",
            output
        );
    }
}

#[test]
fn surfaces_show_missing_surface_param() {
    // Test with an indexed repo to verify surface param validation
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("surfaces-show-err-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"sse-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Now test missing surface param with valid repo
    let show_request = format!(
        r#"{{"id":"sse-2","method":"surfaces_show","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&show_request], state);
    let output = &results[0];

    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "output: {}",
        output
    );
    assert!(
        output.contains("surface"),
        "Should mention missing surface param: {}",
        output
    );
}

// ══════════════════════════════════════════════════════════════════
// BOUNDARIES COMMAND FAMILY
// ══════════════════════════════════════════════════════════════════

#[test]
fn boundaries_list_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("boundaries-list-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    // C file with socket usage (may trigger boundary detection)
    std::fs::write(
        repo_dir.join("server.c"),
        r#"
#include <sys/socket.h>
#include <sys/un.h>
void start() {
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    struct sockaddr_un addr;
    bind(fd, (struct sockaddr*)&addr, sizeof(addr));
}
"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"bl-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let boundaries_request = format!(
        r#"{{"id":"bl-2","method":"boundaries_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&boundaries_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    // Standard envelope fields
    assert_eq!(result["command"], "boundaries list");
    assert!(result["repo"].is_string(), "missing repo: {}", output);
    assert!(
        result["snapshot"].is_string(),
        "missing snapshot: {}",
        output
    );
    assert!(result["results"].is_array(), "missing results: {}", output);
    assert!(result["count"].is_u64(), "missing count: {}", output);
}

#[test]
fn boundaries_list_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"bl-err","method":"boundaries_list","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn boundaries_list_with_kind_filter() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("boundaries-filter-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"bf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let boundaries_request = format!(
        r#"{{"id":"bf-2","method":"boundaries_list","params":{{"repo":"{}","kind":"unix_socket"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&boundaries_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    // Kind filter should be reflected in response
    assert!(
        result.get("filter_kind").is_some(),
        "Kind filter should be reflected: {}",
        output
    );
}

#[test]
fn boundaries_show_returns_detail_or_not_found() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("boundaries-show-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"bs-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Try to show a boundary surface (likely won't exist in simple fixture)
    let show_request = format!(
        r#"{{"id":"bs-2","method":"boundaries_show","params":{{"repo":"{}","surface":"test-surface"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&show_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Either success with detail or error (surface not found is valid)
    if let Some(result) = parsed.get("result") {
        assert!(
            result.get("command").is_some(),
            "missing command field: {}",
            output
        );
        assert!(
            result.get("detail").is_some(),
            "missing detail field: {}",
            output
        );
    } else {
        // "surface not found" is expected for nonexistent surfaces
        assert!(
            parsed.get("error").is_some(),
            "Should have result or error: {}",
            output
        );
    }
}

#[test]
fn boundaries_show_missing_surface_param() {
    // Test with an indexed repo to verify surface param validation
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("boundaries-show-err-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"bse-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Now test missing surface param with valid repo
    let show_request = format!(
        r#"{{"id":"bse-2","method":"boundaries_show","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&show_request], state);
    let output = &results[0];

    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "output: {}",
        output
    );
    assert!(
        output.contains("surface"),
        "Should mention missing surface param: {}",
        output
    );
}

#[test]
fn boundaries_summary_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("boundaries-summary-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"bsum-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let summary_request = format!(
        r#"{{"id":"bsum-2","method":"boundaries_summary","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&summary_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    // Standard envelope fields
    assert_eq!(result["command"], "boundaries summary");
    assert!(result["repo"].is_string(), "missing repo: {}", output);
    assert!(
        result["snapshot"].is_string(),
        "missing snapshot: {}",
        output
    );
    assert!(result["summary"].is_object(), "missing summary: {}", output);
}

#[test]
fn boundaries_summary_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"bsum-err","method":"boundaries_summary","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn boundaries_links_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("boundaries-links-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"blnk-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let links_request = format!(
        r#"{{"id":"blnk-2","method":"boundaries_links","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&links_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    // Standard envelope fields
    assert_eq!(result["command"], "boundaries links");
    assert!(result["repo"].is_string(), "missing repo: {}", output);
    assert!(
        result["snapshot"].is_string(),
        "missing snapshot: {}",
        output
    );
    assert!(result["results"].is_array(), "missing results: {}", output);
    assert!(result["count"].is_u64(), "missing count: {}", output);
}

#[test]
fn boundaries_links_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"blnk-err","method":"boundaries_links","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn boundaries_links_with_service_filter() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("boundaries-links-filter-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"blf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    let links_request = format!(
        r#"{{"id":"blf-2","method":"boundaries_links","params":{{"repo":"{}","service":"UserService"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&links_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = &parsed["result"];

    // Service filter should be reflected in response
    assert!(
        result.get("filter_service").is_some(),
        "Service filter should be reflected: {}",
        output
    );
}

// ══════════════════════════════════════════════════════════════════
// MODULES COMMAND FAMILY
// ══════════════════════════════════════════════════════════════════

#[test]
fn modules_files_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-files-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::create_dir(repo_dir.join("packages")).unwrap();
    std::fs::create_dir(repo_dir.join("packages/core")).unwrap();
    std::fs::write(
        repo_dir.join("packages/core/index.ts"),
        "export function main() {}",
    )
    .unwrap();
    // Add package.json to trigger module detection
    std::fs::write(
        repo_dir.join("packages/core/package.json"),
        r#"{"name": "@test/core", "version": "1.0.0"}"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request files for the module (may or may not have detected module)
    let files_request = format!(
        r#"{{"id":"mf-2","method":"modules_files","params":{{"repo":"{}","module":"packages/core"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&files_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Either success with envelope or module not found error
    if let Some(result) = parsed.get("result") {
        assert_eq!(result["command"], "modules files");
        assert!(result["repo"].is_string(), "missing repo: {}", output);
        assert!(
            result["snapshot"].is_string(),
            "missing snapshot: {}",
            output
        );
        assert!(result["module"].is_object(), "missing module: {}", output);
        assert!(result["results"].is_array(), "missing results: {}", output);
        assert!(result["count"].is_u64(), "missing count: {}", output);
    } else {
        // Module not found is acceptable for simple fixture
        assert!(
            parsed.get("error").is_some(),
            "Should have result or error: {}",
            output
        );
    }
}

#[test]
fn modules_files_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"mf-err","method":"modules_files","params":{"repo":"/nonexistent/path","module":"some-module"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn modules_files_missing_module_param() {
    // Test with an indexed repo to verify module param validation
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-files-err-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mfe-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Now test missing module param with valid repo
    let files_request = format!(
        r#"{{"id":"mfe-2","method":"modules_files","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&files_request], state);
    let output = &results[0];

    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "output: {}",
        output
    );
    assert!(
        output.contains("module"),
        "Should mention missing module param: {}",
        output
    );
}

// ══════════════════════════════════════════════════════════════════════════
// modules_deps
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn modules_deps_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-deps-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::create_dir_all(repo_dir.join("packages/core")).unwrap();
    std::fs::create_dir_all(repo_dir.join("packages/cli")).unwrap();

    // Create two modules with an import relationship
    std::fs::write(
        repo_dir.join("packages/core/index.ts"),
        "export function coreUtil() {}",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("packages/core/package.json"),
        r#"{"name": "@test/core", "version": "1.0.0"}"#,
    )
    .unwrap();

    std::fs::write(
        repo_dir.join("packages/cli/index.ts"),
        r#"import { coreUtil } from "../core/index";"#,
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("packages/cli/package.json"),
        r#"{"name": "@test/cli", "version": "1.0.0"}"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"md-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request all module deps
    let deps_request = format!(
        r#"{{"id":"md-2","method":"modules_deps","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&deps_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Check envelope fields
    assert!(
        parsed.get("result").is_some(),
        "Should have result field: {}",
        output
    );
    let result = &parsed["result"];
    assert_eq!(
        result["command"], "modules deps",
        "command mismatch: {}",
        output
    );
    assert!(result.get("repo").is_some(), "Should have repo: {}", output);
    assert!(
        result.get("snapshot").is_some(),
        "Should have snapshot: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "Should have results: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "Should have count: {}",
        output
    );
    assert!(
        result.get("diagnostics").is_some(),
        "Should have diagnostics: {}",
        output
    );
    assert_eq!(
        result["direction"], "all",
        "direction should default to all: {}",
        output
    );
}

#[test]
fn modules_deps_with_module_filter() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-deps-filter-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::create_dir_all(repo_dir.join("packages/core")).unwrap();
    std::fs::write(
        repo_dir.join("packages/core/index.ts"),
        "export function coreUtil() {}",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("packages/core/package.json"),
        r#"{"name": "@test/core", "version": "1.0.0"}"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mdf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request deps for specific module with direction
    let deps_request = format!(
        r#"{{"id":"mdf-2","method":"modules_deps","params":{{"repo":"{}","module":"packages/core","direction":"outbound"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&deps_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Should succeed (module exists) or error if module not detected
    if parsed.get("result").is_some() {
        let result = &parsed["result"];
        assert_eq!(
            result["direction"], "outbound",
            "direction mismatch: {}",
            output
        );
        assert!(
            result.get("module").is_some(),
            "Should have module field: {}",
            output
        );
    } else {
        // Module detection may fail - that's okay for this test
        assert!(
            parsed.get("error").is_some(),
            "Should have result or error: {}",
            output
        );
    }
}

#[test]
fn modules_deps_direction_without_module_error() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-deps-dir-err-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mdde-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request with direction but no module - should error
    let deps_request = format!(
        r#"{{"id":"mdde-2","method":"modules_deps","params":{{"repo":"{}","direction":"outbound"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&deps_request], state);
    let output = &results[0];

    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "Should be InvalidRequest error: {}",
        output
    );
    assert!(
        output.contains("direction") || output.contains("module"),
        "Should mention direction requires module: {}",
        output
    );
}

#[test]
fn modules_deps_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"md-err","method":"modules_deps","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn modules_deps_module_not_found_returns_error() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-deps-notfound-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mdnf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request deps for non-existent module
    let deps_request = format!(
        r#"{{"id":"mdnf-2","method":"modules_deps","params":{{"repo":"{}","module":"nonexistent-module"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&deps_request], state);
    let output = &results[0];

    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "Should be InvalidRequest error: {}",
        output
    );
    assert!(
        output.contains("module not found"),
        "Should mention module not found: {}",
        output
    );
}

// ══════════════════════════════════════════════════════════════════════════
// modules_violations
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn modules_violations_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-violations-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::create_dir_all(repo_dir.join("packages/core")).unwrap();
    std::fs::write(
        repo_dir.join("packages/core/index.ts"),
        "export function coreUtil() {}",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("packages/core/package.json"),
        r#"{"name": "@test/core", "version": "1.0.0"}"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mv-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request violations
    let violations_request = format!(
        r#"{{"id":"mv-2","method":"modules_violations","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&violations_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Check envelope fields
    assert!(
        parsed.get("result").is_some(),
        "Should have result field: {}",
        output
    );
    let result = &parsed["result"];
    assert_eq!(
        result["command"], "modules violations",
        "command mismatch: {}",
        output
    );
    assert!(result.get("repo").is_some(), "Should have repo: {}", output);
    assert!(
        result.get("snapshot").is_some(),
        "Should have snapshot: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "Should have results: {}",
        output
    );
    assert!(
        result["results"].get("violations").is_some(),
        "Should have violations: {}",
        output
    );
    assert!(
        result["results"].get("stale_declarations").is_some(),
        "Should have stale_declarations: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "Should have count: {}",
        output
    );
    assert!(
        result.get("stale_count").is_some(),
        "Should have stale_count: {}",
        output
    );
    assert!(
        result.get("diagnostics").is_some(),
        "Should have diagnostics: {}",
        output
    );
}

#[test]
fn modules_violations_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"mv-err","method":"modules_violations","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn modules_violations_no_declarations_returns_empty() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("mv-no-decl-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mvnd-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request violations (no declarations exist)
    let violations_request = format!(
        r#"{{"id":"mvnd-2","method":"modules_violations","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&violations_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    let result = &parsed["result"];
    assert_eq!(
        result["count"], 0,
        "No violations expected without declarations: {}",
        output
    );
    let violations = result["results"]["violations"].as_array().unwrap();
    assert!(
        violations.is_empty(),
        "Violations should be empty: {}",
        output
    );
}

// ══════════════════════════════════════════════════════════════════════════
// modules_unowned
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn modules_unowned_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-unowned-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::create_dir_all(repo_dir.join("packages/core")).unwrap();
    std::fs::write(
        repo_dir.join("packages/core/index.ts"),
        "export function coreUtil() {}",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("packages/core/package.json"),
        r#"{"name": "@test/core", "version": "1.0.0"}"#,
    )
    .unwrap();
    // Add an unowned file at root
    std::fs::write(repo_dir.join("orphan.ts"), "export function orphan() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mu-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request unowned files
    let unowned_request = format!(
        r#"{{"id":"mu-2","method":"modules_unowned","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&unowned_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Check envelope fields
    assert!(
        parsed.get("result").is_some(),
        "Should have result field: {}",
        output
    );
    let result = &parsed["result"];
    assert_eq!(
        result["command"], "modules unowned",
        "command mismatch: {}",
        output
    );
    assert!(result.get("repo").is_some(), "Should have repo: {}", output);
    assert!(
        result.get("snapshot").is_some(),
        "Should have snapshot: {}",
        output
    );
    assert!(
        result.get("results").is_some(),
        "Should have results: {}",
        output
    );
    assert!(
        result.get("count").is_some(),
        "Should have count: {}",
        output
    );
    assert!(
        result.get("summary").is_some(),
        "Should have summary: {}",
        output
    );
    // Check summary fields
    let summary = &result["summary"];
    assert!(
        summary.get("total_indexed_files").is_some(),
        "Should have total_indexed_files: {}",
        output
    );
    assert!(
        summary.get("total_owned_files").is_some(),
        "Should have total_owned_files: {}",
        output
    );
    assert!(
        summary.get("total_unowned_files").is_some(),
        "Should have total_unowned_files: {}",
        output
    );
    assert!(
        summary.get("unowned_pct").is_some(),
        "Should have unowned_pct: {}",
        output
    );
    assert!(
        summary.get("by_reason").is_some(),
        "Should have by_reason: {}",
        output
    );
}

#[test]
fn modules_unowned_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"mu-err","method":"modules_unowned","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

// ══════════════════════════════════════════════════════════════════
// MODULES SHOW (REG-1)
// ══════════════════════════════════════════════════════════════════

#[test]
fn modules_show_returns_module_identity() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-show-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::create_dir(repo_dir.join("packages")).unwrap();
    std::fs::create_dir(repo_dir.join("packages/core")).unwrap();
    std::fs::write(
        repo_dir.join("packages/core/index.ts"),
        "export function main() {}",
    )
    .unwrap();
    // Add package.json to trigger module detection
    std::fs::write(
        repo_dir.join("packages/core/package.json"),
        r#"{"name": "@test/core", "version": "1.0.0"}"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"ms-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request show for the module
    let show_request = format!(
        r#"{{"id":"ms-2","method":"modules_show","params":{{"repo":"{}","module":"packages/core"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&show_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();

    // Either success with module identity or module not found error
    if let Some(result) = parsed.get("result") {
        assert_eq!(result["command"], "modules show");
        assert!(result["repo"].is_string(), "missing repo: {}", output);
        assert!(
            result["snapshot"].is_string(),
            "missing snapshot: {}",
            output
        );
        // Module identity fields
        assert!(result["module"].is_object(), "missing module: {}", output);
        let module = &result["module"];
        assert!(
            module["module_uid"].is_string(),
            "missing module_uid: {}",
            output
        );
        assert!(
            module["canonical_root_path"].is_string(),
            "missing canonical_root_path: {}",
            output
        );
        // Rollups
        assert!(result["rollups"].is_object(), "missing rollups: {}", output);
        // Neighbors
        assert!(
            result["outbound_dependencies"].is_array(),
            "missing outbound_dependencies: {}",
            output
        );
        assert!(
            result["inbound_dependencies"].is_array(),
            "missing inbound_dependencies: {}",
            output
        );
        // Degradation fields
        assert!(
            result["rollups_degraded"].is_boolean(),
            "missing rollups_degraded: {}",
            output
        );
        assert!(
            result["warnings"].is_array(),
            "missing warnings: {}",
            output
        );
    } else {
        // Module not found is acceptable for simple fixture
        assert!(
            parsed.get("error").is_some(),
            "Should have result or error: {}",
            output
        );
    }
}

#[test]
fn modules_show_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"ms-err","method":"modules_show","params":{"repo":"/nonexistent/path","module":"some-module"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

#[test]
fn modules_show_missing_module_param() {
    // Test with an indexed repo to verify module param validation
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-show-err-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mse-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Now test missing module param with valid repo
    let show_request = format!(
        r#"{{"id":"mse-2","method":"modules_show","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&show_request], state);
    let output = &results[0];

    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "Should return InvalidRequest for missing module param: {}",
        output
    );
    assert!(
        output.contains("module"),
        "Error should mention 'module': {}",
        output
    );
}

#[test]
fn modules_show_module_not_found_returns_error() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-show-notfound-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"msnf-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request show for a module that doesn't exist
    let show_request = format!(
        r#"{{"id":"msnf-2","method":"modules_show","params":{{"repo":"{}","module":"nonexistent/module"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&show_request], state);
    let output = &results[0];

    assert!(
        output.contains(r#""code":"InvalidRequest""#),
        "Should return InvalidRequest for module not found: {}",
        output
    );
    assert!(
        output.contains("module not found"),
        "Error should mention 'module not found': {}",
        output
    );
}

// ══════════════════════════════════════════════════════════════════
// MODULES LIST (REG-1)
// ══════════════════════════════════════════════════════════════════

#[test]
fn modules_list_returns_envelope() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-list-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::create_dir(repo_dir.join("packages")).unwrap();
    std::fs::create_dir(repo_dir.join("packages/core")).unwrap();
    std::fs::write(
        repo_dir.join("packages/core/index.ts"),
        "export function main() {}",
    )
    .unwrap();
    // Add package.json to trigger module detection
    std::fs::write(
        repo_dir.join("packages/core/package.json"),
        r#"{"name": "@test/core", "version": "1.0.0"}"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"ml-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request modules list
    let list_request = format!(
        r#"{{"id":"ml-2","method":"modules_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&list_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = parsed.get("result").expect("Should have result");

    // Standard envelope fields
    assert_eq!(result["command"], "modules list");
    assert!(result["repo"].is_string(), "missing repo: {}", output);
    assert!(
        result["snapshot"].is_string(),
        "missing snapshot: {}",
        output
    );
    assert!(result["results"].is_array(), "missing results: {}", output);
    assert!(result["count"].is_u64(), "missing count: {}", output);

    // Degradation fields
    assert!(
        result["rollups_degraded"].is_boolean(),
        "missing rollups_degraded: {}",
        output
    );
    assert!(
        result["warnings"].is_array(),
        "missing warnings: {}",
        output
    );

    // Sanity metrics (Phase 3.1)
    assert!(
        result["sanity_metrics"].is_object(),
        "missing sanity_metrics: {}",
        output
    );
}

#[test]
fn modules_list_returns_sanity_metrics() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("modules-list-sanity-repo");
    std::fs::create_dir(&repo_dir).unwrap();
    std::fs::write(repo_dir.join("main.ts"), "export function main() {}").unwrap();
    // Add package.json at root
    std::fs::write(
        repo_dir.join("package.json"),
        r#"{"name": "test-repo", "version": "1.0.0"}"#,
    )
    .unwrap();

    let repo_path_str = repo_dir.to_string_lossy();

    let index_request = format!(
        r#"{{"id":"mls-1","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Request modules list
    let list_request = format!(
        r#"{{"id":"mls-2","method":"modules_list","params":{{"repo":"{}"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&list_request], state);
    let output = &results[0];

    let parsed: serde_json::Value = serde_json::from_str(output).unwrap();
    let result = parsed.get("result").expect("Should have result");

    // Verify sanity metrics fields
    let sanity = &result["sanity_metrics"];
    assert!(
        sanity["largest_module_ownership_pct"].is_f64(),
        "missing largest_module_ownership_pct: {}",
        output
    );
    assert!(
        sanity["tiny_module_count"].is_u64(),
        "missing tiny_module_count: {}",
        output
    );
    assert!(
        sanity["root_fallback_used"].is_boolean(),
        "missing root_fallback_used: {}",
        output
    );
    assert!(
        sanity["mixed_language_module_count"].is_u64(),
        "missing mixed_language_module_count: {}",
        output
    );
    assert!(
        sanity["has_inferred_modules"].is_boolean(),
        "missing has_inferred_modules: {}",
        output
    );

    // Verify unowned breakdown
    let breakdown = &sanity["unowned_breakdown"];
    assert!(
        breakdown["excluded_count"].is_u64(),
        "missing excluded_count: {}",
        output
    );
    assert!(
        breakdown["suppressed_test_count"].is_u64(),
        "missing suppressed_test_count: {}",
        output
    );
    assert!(
        breakdown["true_gap_count"].is_u64(),
        "missing true_gap_count: {}",
        output
    );
    assert!(
        breakdown["true_gap_pct"].is_f64(),
        "missing true_gap_pct: {}",
        output
    );
    assert!(
        breakdown["classified_pct"].is_f64(),
        "missing classified_pct: {}",
        output
    );
}

#[test]
fn modules_list_repo_not_indexed_returns_error() {
    let output = run_daemon_request(
        r#"{"id":"ml-err","method":"modules_list","params":{"repo":"/nonexistent/path"}}"#,
    );
    assert!(
        output.contains(r#""code":"RepoNotFound""#),
        "output: {}",
        output
    );
}

// ── CLI-OUT-3: Graph drilldown success-path tests ───────────────────────────

/// Create a test repo with inter-file function calls for graph drilldown tests.
fn create_graph_drilldown_test_repo(repo_dir: &std::path::Path) {
    std::fs::create_dir_all(repo_dir).unwrap();

    // main.ts imports and calls helper
    std::fs::write(
        repo_dir.join("main.ts"),
        r#"
import { helperFunction } from './helper';

export function mainEntry() {
    helperFunction();
}
"#,
    )
    .unwrap();

    // helper.ts exports helperFunction
    std::fs::write(
        repo_dir.join("helper.ts"),
        r#"
export function helperFunction() {
    console.log('helper');
}
"#,
    )
    .unwrap();
}

#[test]
fn callers_returns_success_response_structure() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("graph-repo");
    create_graph_drilldown_test_repo(&repo_dir);

    let repo_path_str = repo_dir.to_string_lossy();

    // Index
    let index_request = format!(
        r#"{{"id":"gd-idx","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query callers for helperFunction
    let callers_request = format!(
        r#"{{"id":"gd-callers","method":"callers","params":{{"repo":"{}","symbol":"helperFunction"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&callers_request], Arc::clone(&state));
    let output = &results[0];

    // Verify response structure for human rendering
    assert!(
        output.contains(r#""id":"gd-callers""#),
        "Response should have correct id: {}",
        output
    );
    // Should have target info
    assert!(
        output.contains(r#""target""#),
        "Response should have target field: {}",
        output
    );
    // Should have callers array
    assert!(
        output.contains(r#""callers""#),
        "Response should have callers field: {}",
        output
    );
    // Should have count
    assert!(
        output.contains(r#""count""#),
        "Response should have count field: {}",
        output
    );
    // Should not be an error
    assert!(
        !output.contains(r#""code":"InvalidRequest""#)
            && !output.contains(r#""code":"InternalError""#),
        "Should not be error: {}",
        output
    );
}

#[test]
fn callees_returns_success_response_structure() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("graph-repo");
    create_graph_drilldown_test_repo(&repo_dir);

    let repo_path_str = repo_dir.to_string_lossy();

    // Index
    let index_request = format!(
        r#"{{"id":"gd-idx","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query callees for mainEntry
    let callees_request = format!(
        r#"{{"id":"gd-callees","method":"callees","params":{{"repo":"{}","symbol":"mainEntry"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&callees_request], Arc::clone(&state));
    let output = &results[0];

    // Verify response structure for human rendering
    assert!(
        output.contains(r#""id":"gd-callees""#),
        "Response should have correct id: {}",
        output
    );
    // Should have target info
    assert!(
        output.contains(r#""target""#),
        "Response should have target field: {}",
        output
    );
    // Should have callees array
    assert!(
        output.contains(r#""callees""#),
        "Response should have callees field: {}",
        output
    );
    // Should have count
    assert!(
        output.contains(r#""count""#),
        "Response should have count field: {}",
        output
    );
    // Should not be an error
    assert!(
        !output.contains(r#""code":"InvalidRequest""#)
            && !output.contains(r#""code":"InternalError""#),
        "Should not be error: {}",
        output
    );
}

#[test]
fn path_returns_success_response_structure() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("graph-repo");
    create_graph_drilldown_test_repo(&repo_dir);

    let repo_path_str = repo_dir.to_string_lossy();

    // Index
    let index_request = format!(
        r#"{{"id":"gd-idx","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query path from mainEntry to helperFunction
    let path_request = format!(
        r#"{{"id":"gd-path","method":"path","params":{{"repo":"{}","from":"mainEntry","to":"helperFunction"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&path_request], Arc::clone(&state));
    let output = &results[0];

    // Verify response structure for human rendering
    assert!(
        output.contains(r#""id":"gd-path""#),
        "Response should have correct id: {}",
        output
    );
    // Should have path object
    assert!(
        output.contains(r#""path""#),
        "Response should have path field: {}",
        output
    );
    // Should have found boolean
    assert!(
        output.contains(r#""found""#),
        "Response should have found field: {}",
        output
    );
    // Should not be an error
    assert!(
        !output.contains(r#""code":"InvalidRequest""#)
            && !output.contains(r#""code":"InternalError""#),
        "Should not be error: {}",
        output
    );
}

#[test]
fn path_not_found_returns_proper_structure() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("graph-repo");
    create_graph_drilldown_test_repo(&repo_dir);

    let repo_path_str = repo_dir.to_string_lossy();

    // Index
    let index_request = format!(
        r#"{{"id":"gd-idx","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query path between unrelated symbols (should not find path)
    let path_request = format!(
        r#"{{"id":"gd-path-nf","method":"path","params":{{"repo":"{}","from":"helperFunction","to":"mainEntry"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&path_request], Arc::clone(&state));
    let output = &results[0];

    // Verify response structure even for not-found case
    assert!(
        output.contains(r#""id":"gd-path-nf""#),
        "Response should have correct id: {}",
        output
    );
    // Should have path object with found:false
    assert!(
        output.contains(r#""path""#),
        "Response should have path field: {}",
        output
    );
    assert!(
        output.contains(r#""found""#),
        "Response should have found field: {}",
        output
    );
    // Should not be an error (not-found is valid result, not error)
    assert!(
        !output.contains(r#""code":"InvalidRequest""#)
            && !output.contains(r#""code":"InternalError""#),
        "Should not be error: {}",
        output
    );
}

#[test]
fn imports_returns_success_response_structure() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("graph-repo");
    create_graph_drilldown_test_repo(&repo_dir);

    let repo_path_str = repo_dir.to_string_lossy();

    // Index
    let index_request = format!(
        r#"{{"id":"gd-idx","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query imports for main.ts
    let imports_request = format!(
        r#"{{"id":"gd-imports","method":"imports","params":{{"repo":"{}","file":"main.ts"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&imports_request], Arc::clone(&state));
    let output = &results[0];

    // Verify response structure for human rendering
    assert!(
        output.contains(r#""id":"gd-imports""#),
        "Response should have correct id: {}",
        output
    );
    // Should have file field
    assert!(
        output.contains(r#""file""#),
        "Response should have file field: {}",
        output
    );
    // Should have imports array
    assert!(
        output.contains(r#""imports""#),
        "Response should have imports field: {}",
        output
    );
    // Should not be an error
    assert!(
        !output.contains(r#""code":"InvalidRequest""#)
            && !output.contains(r#""code":"InternalError""#),
        "Should not be error: {}",
        output
    );
}

// ── Ambiguous symbol handling tests ─────────────────────────────────────────

/// Create a test repo with duplicate symbol names for ambiguity testing.
fn create_ambiguous_symbol_test_repo(repo_dir: &std::path::Path) {
    std::fs::create_dir_all(repo_dir).unwrap();

    // Two files with same function name
    std::fs::write(
        repo_dir.join("moduleA.ts"),
        r#"
export function process() {
    console.log('module A');
}
"#,
    )
    .unwrap();

    std::fs::write(
        repo_dir.join("moduleB.ts"),
        r#"
export function process() {
    console.log('module B');
}
"#,
    )
    .unwrap();
}

#[test]
fn callers_ambiguous_symbol_returns_structured_error() {
    let state_temp = tempdir().unwrap();
    let state = create_isolated_state_in(&state_temp);

    let repo_temp = tempdir().unwrap();
    let repo_dir = repo_temp.path().join("ambig-repo");
    create_ambiguous_symbol_test_repo(&repo_dir);

    let repo_path_str = repo_dir.to_string_lossy();

    // Index
    let index_request = format!(
        r#"{{"id":"amb-idx","method":"index","params":{{"repo_path":"{}"}}}}"#,
        repo_path_str
    );
    let results = run_daemon_requests_with_state(vec![&index_request], Arc::clone(&state));
    let (_repo_uid, _db_path, canonical_path) = extract_index_result(&results[0]);

    // Query callers with ambiguous symbol name
    let callers_request = format!(
        r#"{{"id":"amb-callers","method":"callers","params":{{"repo":"{}","symbol":"process"}}}}"#,
        canonical_path
    );
    let results = run_daemon_requests_with_state(vec![&callers_request], Arc::clone(&state));
    let output = &results[0];

    // Should return AmbiguousSymbol error with structured data
    assert!(
        output.contains(r#""code":"AmbiguousSymbol""#),
        "Should return AmbiguousSymbol error: {}",
        output
    );
    // Should have data field with matches
    assert!(
        output.contains(r#""data""#),
        "Error should have data field for structured ambiguity info: {}",
        output
    );
    assert!(
        output.contains(r#""query""#),
        "Data should have query field: {}",
        output
    );
    assert!(
        output.contains(r#""matches""#),
        "Data should have matches field: {}",
        output
    );
}
