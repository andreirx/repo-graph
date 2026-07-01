//! HONEST-DEGRADATION-IMPL-2 (D2 + D5) — daemon-runtime SURFACE proofs.
//!
//! review-0 required change (3): the D2/D5 honesty must be proven through the REAL handlers
//! (`handle_deps_list` / `handle_stats`), not only the pure helper / renderer units. Each test here
//! drives `ServiceDispatcher::dispatch` end-to-end: it indexes a real on-disk fixture in an ISOLATED
//! temp state root (the operator's registry/daemon are never touched), then dispatches `deps_list` /
//! `stats` and asserts the daemon KEYED the response correctly — the ecosystem/reader-context note (D2)
//! and the toolchain-aware next-action line (D5) keyed on the daemon's configured resolvers × the repo's
//! real extracted language.
//!
//! Why these fixtures go LOW (so D5 fires): import-graph reliability is LOW whenever there is ≥1
//! unresolved import (`trust::rules::compute_import_graph_reliability`). Each fixture imports an
//! external/system module that the snapshot cannot resolve, so the import graph is genuinely LOW — the
//! same honest-degradation condition the contract targets.
//!
//! The Java case requires the JDTLS resolver to be UNCONFIGURED; the test removes `JDTLS_PATH` to force
//! that deterministically (no other test sets it, so this is race-free). The Java-WITH-JDTLS branch is
//! covered deterministically by the pure unit test in `dispatch::honest_degradation_tests` (a
//! process-global env mutation cannot prove it without flaking a parallel suite).

use std::path::Path;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

// ── Harness ──────────────────────────────────────────────────────────────────

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Isolated daemon under a temp state root — never touches the operator's real registry/databases.
fn isolated() -> (ServiceDispatcher, TempDir) {
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(state);
    (dispatcher, state_root)
}

fn request(id: &str, method: &str, params: Value) -> Request {
    Request {
        id: id.to_string(),
        method: method.to_string(),
        params,
    }
}

fn run(dispatcher: &ServiceDispatcher, id: &str, method: &str, params: Value) -> DispatchResult {
    let mut emitter = Quiet;
    dispatcher.dispatch(&request(id, method, params), &mut emitter)
}

#[track_caller]
fn expect_success(result: DispatchResult) -> Value {
    match result {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => {
            panic!(
                "expected success, got error {}: {}",
                e.error.code, e.error.message
            )
        }
    }
}

/// Index `repo_dir` and return its canonical repo handle (the `repo` param for query dispatches).
fn index_repo(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> String {
    let indexed = expect_success(run(
        dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    indexed["canonical_path"]
        .as_str()
        .expect("index returns canonical_path")
        .to_string()
}

// ── Fixtures (each imports an external/system module ⇒ ≥1 unresolved import ⇒ import-graph LOW) ──

fn write_c_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("main.c"),
        "#include <stdio.h>\n\nint helper(int x) { return x + 1; }\n\nint main(void) {\n    return helper(0);\n}\n",
    )
    .unwrap();
}

fn write_rust_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    // `serde` is not present in the snapshot ⇒ the import is unresolved ⇒ import-graph LOW.
    std::fs::write(
        dir.join("main.rs"),
        "use serde::Serialize;\n\nfn helper() -> i32 { 1 }\n\nfn main() {\n    let _ = helper();\n}\n",
    )
    .unwrap();
}

fn write_java_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    // `java.util.List` is external to the snapshot ⇒ unresolved import ⇒ import-graph LOW.
    std::fs::write(
        dir.join("Main.java"),
        "import java.util.List;\n\npublic class Main {\n    int helper() { return 1; }\n    void run() { helper(); }\n}\n",
    )
    .unwrap();
}

// ── D2: deps ecosystem honesty through handle_deps_list ──────────────────────

/// D2 SURFACE PROOF — a C repo through `deps list` no longer reports `ecosystem:"npm"`; it reports the
/// honest `none-detected` plus the reader-context "external includes observed, not attributed" note over
/// the EXISTING `total_external_imports` count (no resolver ran — the count is real, unchanged).
#[test]
fn d2_c_repo_deps_list_not_npm_and_has_unattributed_note() {
    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("crepo");
    write_c_repo(&repo_dir);
    let repo = index_repo(&dispatcher, &repo_dir);

    let result = expect_success(run(&dispatcher, "d", "deps_list", json!({ "repo": repo })));

    // No false npm graph.
    assert_eq!(
        result["ecosystem"].as_str(),
        Some("none-detected"),
        "C repo must not be labelled an evaluated npm graph: {result}"
    );
    // The honest reader-context note, naming C and the observed-not-attributed external includes.
    let note = result["reader_context"].as_str().unwrap_or_else(|| {
        panic!("expected a reader_context note on a none-detected repo: {result}")
    });
    assert!(
        note.contains("no dependency-manifest reader for C on this build"),
        "note must name C in the reader's language: {note}"
    );
    assert!(
        note.contains("external includes observed, not attributed"),
        "note must surface the observed-unattributed framing: {note}"
    );
    assert!(!note.contains("npm"), "note must not mention npm: {note}");
    // The external-import count is real, present, and unchanged (no resolver ran).
    let n = result["total_external_imports"]
        .as_u64()
        .expect("total_external_imports present");
    assert!(
        note.contains(&n.to_string()),
        "the note's count ({n}) must be the existing total_external_imports: {note}"
    );
}

// ── D5: toolchain-aware next-action through handle_stats (keyed on configured resolvers) ──

/// D5 SURFACE PROOF (C / no resolver) — a LOW-reliability C repo through `stats` emits the honest
/// dead-end line naming C, and NEVER suggests `rmap enrich` (no resolver exists for C on any build).
#[test]
fn d5_c_repo_stats_states_no_resolution_path() {
    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("crepo");
    write_c_repo(&repo_dir);
    let repo = index_repo(&dispatcher, &repo_dir);

    let result = expect_success(run(&dispatcher, "s", "stats", json!({ "repo": repo })));
    let line = result["relationship_next_action"].as_str().unwrap_or_else(|| {
        panic!("expected a LOW-reliability next-action on a C repo (unresolved #include): {result}")
    });
    assert!(
        line.contains("no semantic-resolution path exists for C"),
        "C must get the honest dead-end, not a remedy: {line}"
    );
    assert!(
        !line.contains("rmap enrich"),
        "must NEVER suggest enrich on C (no resolver): {line}"
    );
}

/// D5 SURFACE PROOF (Rust / built-in resolver) — a LOW-reliability Rust repo through `stats` suggests
/// running enrichment (Rust's resolver is compiled in / unconditional).
#[test]
fn d5_rust_repo_stats_suggests_enrich() {
    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("rustrepo");
    write_rust_repo(&repo_dir);
    let repo = index_repo(&dispatcher, &repo_dir);

    let result = expect_success(run(&dispatcher, "s", "stats", json!({ "repo": repo })));
    let line = result["relationship_next_action"].as_str().unwrap_or_else(|| {
        panic!("expected a LOW-reliability next-action on a Rust repo (unresolved `use serde`): {result}")
    });
    assert!(
        line.contains("rmap enrich"),
        "Rust has a built-in resolver ⇒ suggest enrich: {line}"
    );
}

/// D5 SURFACE PROOF (Java without JDTLS / false-promise guard) — a LOW-reliability Java repo through
/// `stats`, with JDTLS UNCONFIGURED, names the JDTLS requirement instead of a blind enrich suggestion (a
/// `languages:["java"]` enrich would error without JDTLS — the exact false-trust mode D5 prevents).
#[test]
fn d5_java_without_jdtls_says_configure_jdtls() {
    // Force the unconfigured-JDTLS scenario deterministically (no other test sets JDTLS_PATH).
    std::env::remove_var("JDTLS_PATH");

    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("javarepo");
    write_java_repo(&repo_dir);
    let repo = index_repo(&dispatcher, &repo_dir);

    let result = expect_success(run(&dispatcher, "s", "stats", json!({ "repo": repo })));
    let line = result["relationship_next_action"].as_str().unwrap_or_else(|| {
        panic!("expected a LOW-reliability next-action on a Java repo (unresolved import): {result}")
    });
    assert!(
        line.contains("requires JDTLS") && line.contains("JDTLS_PATH"),
        "Java without JDTLS must point at the JDTLS requirement: {line}"
    );
    assert!(
        !line.contains("rmap enrich"),
        "must NOT suggest a blind enrich that would error without JDTLS: {line}"
    );
}

/// D5 SURFACE PROOF (orient parity) — the SAME next-action keying reaches `orient` (the other
/// posture-bearing surface), proving the two surfaces render ONE coherent line for the same repo.
#[test]
fn d5_orient_renders_same_next_action_as_stats() {
    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("crepo");
    write_c_repo(&repo_dir);
    let repo = index_repo(&dispatcher, &repo_dir);

    let oriented = expect_success(run(
        &dispatcher,
        "o",
        "orient",
        json!({ "repo": repo, "budget": "full" }),
    ));
    // orient serves a CoherenceEnvelope; the next-action rides `value.relationship_next_action`.
    let line = oriented["value"]["relationship_next_action"]
        .as_str()
        .unwrap_or_else(|| {
            panic!("expected orient to carry the next-action on a LOW C repo: {oriented}")
        });
    assert!(
        line.contains("no semantic-resolution path exists for C"),
        "orient must render the SAME honest C dead-end as stats: {line}"
    );
}
