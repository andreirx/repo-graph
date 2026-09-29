//! Explain use-case tests: file target.

mod common;

use common::{FakeAgentStorage, TEST_NOW};
use repo_graph_agent::{
    orient, run_explain, AgentDirectoryGroup, AgentImportEntry, AgentPathResolution,
    AgentRepoSummary, AgentSymbolEntry, Budget, SignalCode, TrackedFileTestFlag, EXPLAIN_COMMAND,
};

fn seed_file_repo(fake: &mut FakeAgentStorage) {
    fake.seed_minimal_repo("r1", "my-repo", "snap1");

    // Path resolution: exact file match.
    fake.path_resolutions.insert(
        ("snap1".into(), "src/service.ts".into()),
        AgentPathResolution {
            has_exact_file: true,
            file_stable_key: Some("r1:src/service.ts:FILE".into()),
            has_content_under_prefix: false,
            module_stable_key: None,
        },
    );

    // File summary.
    fake.file_summaries.insert(
        ("snap1".into(), "src/service.ts".into()),
        AgentRepoSummary {
            file_count: 1,
            symbol_count: 3,
            languages: vec!["typescript".into()],
            tracked_only_count: 0,
        },
    );
}

#[test]
fn explain_file_has_identity_section() {
    let mut fake = FakeAgentStorage::new();
    seed_file_repo(&mut fake);

    let result = run_explain(&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW).unwrap();

    assert_eq!(result.command, EXPLAIN_COMMAND);
    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    assert!(
        codes.contains(&SignalCode::ExplainIdentity),
        "must have EXPLAIN_IDENTITY, got: {:?}",
        codes
    );
}

#[test]
fn explain_file_has_trust_section() {
    let mut fake = FakeAgentStorage::new();
    seed_file_repo(&mut fake);

    let result = run_explain(&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW).unwrap();

    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    assert!(
        codes.contains(&SignalCode::ExplainTrust),
        "must have EXPLAIN_TRUST, got: {:?}",
        codes
    );
}

#[test]
fn explain_file_no_path_only_sections() {
    let mut fake = FakeAgentStorage::new();
    seed_file_repo(&mut fake);

    let result = run_explain(&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW).unwrap();

    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    // File should NOT have callers, callees, cycles, boundary,
    // gate, files sections.
    assert!(
        !codes.contains(&SignalCode::ExplainCallers),
        "file must not have EXPLAIN_CALLERS"
    );
    assert!(
        !codes.contains(&SignalCode::ExplainCallees),
        "file must not have EXPLAIN_CALLEES"
    );
    assert!(
        !codes.contains(&SignalCode::ExplainCycles),
        "file must not have EXPLAIN_CYCLES"
    );
    assert!(
        !codes.contains(&SignalCode::ExplainBoundary),
        "file must not have EXPLAIN_BOUNDARY"
    );
    assert!(
        !codes.contains(&SignalCode::ExplainGate),
        "file must not have EXPLAIN_GATE"
    );
    assert!(
        !codes.contains(&SignalCode::ExplainFiles),
        "file must not have EXPLAIN_FILES"
    );
}

#[test]
fn explain_file_has_imports_when_present() {
    let mut fake = FakeAgentStorage::new();
    seed_file_repo(&mut fake);

    fake.file_imports.insert(
        ("snap1".into(), "src/service.ts".into()),
        vec![
            AgentImportEntry {
                target_file: "src/model.ts".into(),
            },
            AgentImportEntry {
                target_file: "src/utils.ts".into(),
            },
        ],
    );

    let result = run_explain(&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW).unwrap();

    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    assert!(
        codes.contains(&SignalCode::ExplainImports),
        "must have EXPLAIN_IMPORTS when imports exist"
    );
}

#[test]
fn explain_file_has_symbols_when_present() {
    let mut fake = FakeAgentStorage::new();
    seed_file_repo(&mut fake);

    fake.symbols_in_file.insert(
        ("snap1".into(), "src/service.ts".into()),
        vec![AgentSymbolEntry {
            stable_key: "r1:src/service.ts:foo:SYMBOL".into(),
            name: "foo".into(),
            qualified_name: None,
            subtype: Some("FUNCTION".into()),
            line_start: Some(1),
        }],
    );

    let result = run_explain(&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW).unwrap();

    let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
    assert!(
        codes.contains(&SignalCode::ExplainSymbols),
        "must have EXPLAIN_SYMBOLS when symbols exist"
    );
}

#[test]
fn explain_file_no_match_returns_empty() {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");

    let result = run_explain(&fake, "r1", "nonexistent.ts", Budget::Medium, TEST_NOW).unwrap();

    assert_eq!(result.command, EXPLAIN_COMMAND);
    assert!(!result.focus.resolved);
    assert!(result.signals.is_empty());
}

// ── TEST-EDGE-SCOPE-1A (RG-REQ-001-L07): explain <file> states an UNDETERMINED test status ──

fn tracked(path: &str, is_test: bool, has_file_node: bool) -> TrackedFileTestFlag {
    TrackedFileTestFlag {
        path: path.into(),
        is_test,
        has_file_node,
    }
}

/// Seed an exact FILE-bearing file `path` whose stored flag is `is_test`.
fn seed_file(fake: &mut FakeAgentStorage, path: &str, is_test: bool) {
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    fake.path_resolutions.insert(
        ("snap1".into(), path.into()),
        AgentPathResolution {
            has_exact_file: true,
            file_stable_key: Some(format!("r1:{path}:FILE")),
            has_content_under_prefix: false,
            module_stable_key: None,
        },
    );
    fake.file_summaries.insert(
        ("snap1".into(), path.into()),
        AgentRepoSummary {
            file_count: 1,
            symbol_count: 1,
            languages: vec!["cpp".into()],
            tracked_only_count: 0,
        },
    );
    fake.tracked_file_flags
        .insert("snap1".into(), vec![tracked(path, is_test, true)]);
}

/// The EXPLAIN_IDENTITY evidence as JSON.
fn identity_json(result: &repo_graph_agent::OrientResult) -> serde_json::Value {
    let sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainIdentity)
        .expect("EXPLAIN_IDENTITY");
    serde_json::to_value(sig).unwrap()["evidence"].clone()
}

#[test]
fn explain_file_marks_an_undetermined_file_with_its_word() {
    let mut fake = FakeAgentStorage::new();
    seed_file(&mut fake, "util/testutil.cc", false);
    let result = run_explain(&fake, "r1", "util/testutil.cc", Budget::Medium, TEST_NOW).unwrap();
    let id = identity_json(&result);
    assert_eq!(id["target_kind"], "file");
    assert_eq!(id["test_status"], "undetermined");
    assert_eq!(id["test_status_word"], "testutil");
}

#[test]
fn explain_file_carries_no_test_status_for_a_determined_file() {
    let mut fake = FakeAgentStorage::new();
    seed_file(&mut fake, "db/db_impl.cc", false);
    let result = run_explain(&fake, "r1", "db/db_impl.cc", Budget::Medium, TEST_NOW).unwrap();
    let id = identity_json(&result);
    assert!(id.get("test_status").is_none(), "{id}");
    assert!(id.get("test_status_word").is_none(), "{id}");
}

#[test]
fn explain_file_carries_no_test_status_for_a_test_file() {
    // leveldb `util/testutil.h`: test-worded, but its gtest marker made it test.
    let mut fake = FakeAgentStorage::new();
    seed_file(&mut fake, "util/testutil.h", true);
    let result = run_explain(&fake, "r1", "util/testutil.h", Budget::Medium, TEST_NOW).unwrap();
    let id = identity_json(&result);
    assert!(id.get("test_status").is_none(), "{id}");
}

#[test]
fn explain_tracked_only_undetermined_file_states_its_test_status() {
    // hadoop's `…-integration-tests/pom.xml` shape: tracked, no FILE node.
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    fake.tracked_file_flags.insert(
        "snap1".into(),
        vec![
            tracked("integration-tests/pom.xml", false, false),
            tracked("src/App.java", false, true),
        ],
    );
    let result = run_explain(
        &fake,
        "r1",
        "integration-tests/pom.xml",
        Budget::Medium,
        TEST_NOW,
    )
    .unwrap();
    assert!(result.focus.resolved, "a file identity, not a no-match");
    let id = identity_json(&result);
    assert_eq!(id["target_kind"], "file");
    assert_eq!(id["path"], "integration-tests/pom.xml");
    assert_eq!(id["test_status"], "undetermined");
    assert_eq!(id["test_status_word"], "tests");
    assert!(id.get("language").is_none() && id.get("symbol_count").is_none());
}

#[test]
fn explain_tracked_only_determined_file_stays_no_match() {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    // `tests/requirements.txt` is test by convention: determined → no-match as before.
    fake.tracked_file_flags.insert(
        "snap1".into(),
        vec![
            tracked("tests/requirements.txt", true, false),
            tracked("config/app.yaml", false, false),
        ],
    );
    for target in ["tests/requirements.txt", "config/app.yaml"] {
        let result = run_explain(&fake, "r1", target, Budget::Medium, TEST_NOW).unwrap();
        assert!(!result.focus.resolved, "{target}");
        assert!(result.signals.is_empty(), "{target}");
    }
}

#[test]
fn explain_file_without_a_tracked_row_is_an_error_not_determined() {
    let mut fake = FakeAgentStorage::new();
    seed_file(&mut fake, "util/testutil.cc", false);
    // The store holds a FILE node for the path but no tracked row on the snapshot.
    fake.tracked_file_flags.insert("snap1".into(), Vec::new());
    let err = run_explain(&fake, "r1", "util/testutil.cc", Budget::Medium, TEST_NOW)
        .expect_err("no tracked row is an inconsistency, never read as determined");
    assert!(
        err.to_string().contains("util/testutil.cc"),
        "the error names the file: {err}"
    );
}

// ── TEST-EDGE-SCOPE-1A: orient's package groups state their undetermined files ──
// (The agent integration-test home allocated to this slice; drives the real `orient`
// use case through the shared fake.)

fn seed_groups(fake: &mut FakeAgentStorage) {
    fake.seed_minimal_repo("r1", "my-repo", "snap1");
    fake.directory_groups.insert(
        "snap1".into(),
        vec![
            AgentDirectoryGroup {
                path: "db".into(),
                file_count: 3,
                test_file_count: 1,
            },
            AgentDirectoryGroup {
                path: "util".into(),
                file_count: 3,
                test_file_count: 1,
            },
        ],
    );
    fake.tracked_file_flags.insert(
        "snap1".into(),
        vec![
            tracked("db/c_test.c", false, true),
            tracked("db/db_impl.cc", false, true),
            tracked("db/db_test.cc", true, true),
            tracked("util/testutil.cc", false, true),
            tracked("util/testutil.h", true, true),
            tracked("util/env.cc", false, true),
            // Tracked only (no FILE node): never a grouped file.
            tracked("integration-tests/pom.xml", false, false),
        ],
    );
}

fn module_summary_json(fake: &FakeAgentStorage) -> serde_json::Value {
    let result = orient(fake, "r1", None, Budget::Full, TEST_NOW).unwrap();
    let sig = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ModuleSummary)
        .expect("MODULE_SUMMARY");
    serde_json::to_value(sig).unwrap()["evidence"].clone()
}

#[test]
fn orient_module_summary_states_undetermined_grouped_files() {
    let mut fake = FakeAgentStorage::new();
    seed_groups(&mut fake);
    let ev = module_summary_json(&fake);
    assert_eq!(
        ev["package_groups_test_status_undetermined"],
        serde_json::json!({
            "count": 2,
            "paths": ["db/c_test.c", "util/testutil.cc"],
            "universe": "grouped_files",
            "universe_count": 4,
            "unknown_count": 0
        })
    );
    // The package groups' test counts are the stored ones.
    let tests: u64 = ev["package_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["test_file_count"].as_u64().unwrap())
        .sum();
    assert_eq!(tests, 2);
}

#[test]
fn orient_module_summary_grouped_count_equals_the_shared_function_over_grouped_files() {
    let mut fake = FakeAgentStorage::new();
    seed_groups(&mut fake);
    let ev = module_summary_json(&fake);
    let expected: Vec<String> = fake.tracked_file_flags["snap1"]
        .iter()
        .filter(|f| f.has_file_node)
        .filter(|f| {
            repo_graph_classification::test_path::undetermined_test_word(&f.path, f.is_test)
                .is_some()
        })
        .map(|f| f.path.clone())
        .collect();
    let block = &ev["package_groups_test_status_undetermined"];
    let got: Vec<String> = block["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    assert_eq!(got, expected);
    assert_eq!(block["count"].as_u64().unwrap(), expected.len() as u64);
    let groups = ev["package_groups"].as_array().unwrap();
    let files: u64 = groups
        .iter()
        .map(|g| g["file_count"].as_u64().unwrap())
        .sum();
    let tests: u64 = groups
        .iter()
        .map(|g| g["test_file_count"].as_u64().unwrap())
        .sum();
    assert_eq!(block["universe_count"].as_u64().unwrap(), files - tests);
    // A failed tracked-flags read is stated unavailable with its reason, never zero.
    *fake.force_error_on.borrow_mut() = Some("query_tracked_file_test_flags");
    let ev = module_summary_json(&fake);
    let block = &ev["package_groups_test_status_undetermined"];
    assert_eq!(block["universe"], "grouped_files");
    assert!(block["unavailable"]
        .as_str()
        .unwrap()
        .contains("forced failure"));
    assert!(block.get("count").is_none());
}
