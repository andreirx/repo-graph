//! METRIC-LANG-COVERAGE-1 (part A) — SURFACE proof through the REAL daemon handlers.
//!
//! review-6 item 3: prove the `measurement_coverage` block reaches the ACTUAL dispatched
//! output of the complexity-bearing surfaces end-to-end from an indexed mixed-language
//! fixture — not only the storage / classification / presentation units in isolation.
//! Each test indexes a real on-disk fixture in an ISOLATED temp state root (the
//! operator's registry/daemon are never touched) and drives `ServiceDispatcher::dispatch`.
//!
//! The fixture is NON-CIRCULAR: Rust carries a real measured function body (the part-B
//! deliverable) — one deliberately branchy fn above the cyclomatic-20 threshold so
//! `orient` emits HIGH_COMPLEXITY — while a bodyless TypeScript interface (METHOD symbols
//! with no body → no metric) is the unmeasured vehicle. So the data-driven caveat names
//! TypeScript, never the just-measured Rust.

use std::path::Path;
use std::process::Command;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

// ── Harness (mirrors honest_degradation_impl2.rs) ────────────────────────────

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Isolated daemon under a temp state root — never touches the operator's real registry.
fn isolated() -> (ServiceDispatcher, TempDir) {
    // Disable the REAL background maintenance passes (enrich -> seed -> retention) the index
    // dispatch queues: with a LIVE local embeddings endpoint the seed pass actually runs and
    // holds the DB while the test reads it -> `database is locked` flakes (4th recurrence of
    // this class, 2026-08-28; same override seed_seam.rs / forget_repo.rs use).
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    (ServiceDispatcher::new(state), state_root)
}

fn run(dispatcher: &ServiceDispatcher, id: &str, method: &str, params: Value) -> DispatchResult {
    let mut emitter = Quiet;
    dispatcher.dispatch(
        &Request {
            id: id.to_string(),
            method: method.to_string(),
            params,
        },
        &mut emitter,
    )
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

// ── Fixture ──────────────────────────────────────────────────────────────────

/// A branchy Rust fn (cyclomatic 25 → over the 20 threshold, so `orient` emits
/// HIGH_COMPLEXITY and the fn ranks in complexity centers — the slice's headline
/// defect) plus a bodyless TypeScript interface (3 METHOD symbols, 0 metrics). Rust is
/// MEASURED, TS is not, so the data-driven caveat names TypeScript (non-circular).
fn write_mixed_fixture(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // base 1 + 24 `if`s = cyclomatic 25 (same decision-point rules as C/TS).
    let mut body = String::from("pub fn hot(x: i32) -> i32 {\n    let mut n = 0;\n");
    for i in 0..24 {
        body.push_str(&format!("    if x > {i} {{ n += 1; }}\n"));
    }
    body.push_str("    n\n}\n");
    std::fs::write(dir.join("src/hot.rs"), body).unwrap();
    std::fs::write(
        dir.join("src/ports.ts"),
        "export interface Store {\n  save(x: number): void;\n  load(): number;\n  remove(id: number): boolean;\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"m","version":"0.0.0"}"#,
    )
    .unwrap();
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Make `dir` a git repo with one commit — `hotspots` scores churn × complexity and
/// errors before the coverage attach if there is no git history.
fn git_init_commit(dir: &Path) {
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git runs")
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "fixture"]);
}

// ── Surface proofs ───────────────────────────────────────────────────────────

#[test]
fn orient_surface_carries_available_coverage_block_and_ranks_rust() {
    // Two proofs through the REAL orient handler: (1) the slice's headline defect —
    // Rust now appears in complexity centers (pre-slice this was empty for Rust); and
    // (2) the always-present `available` coverage block naming the unmeasured TS. No git
    // needed for orient.
    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("mixed");
    write_mixed_fixture(&repo_dir);
    let repo = index_repo(&dispatcher, &repo_dir);

    let oriented = expect_success(run(
        &dispatcher,
        "o",
        "orient",
        json!({ "repo": repo, "budget": "full" }),
    ));

    // (1) Rust ranks — HIGH_COMPLEXITY emitted, the `hot` fn named.
    let blob = oriented.to_string();
    assert!(
        blob.contains("HIGH_COMPLEXITY"),
        "orient must emit HIGH_COMPLEXITY for the branchy Rust fn: {oriented}"
    );
    assert!(
        blob.contains("hot.rs"),
        "the Rust fn `hot` (src/hot.rs) must rank in complexity centers: {oriented}"
    );

    // (2) The always-present coverage block rides `value.measurement_coverage`.
    let cov = &oriented["value"]["measurement_coverage"];
    assert_eq!(
        cov["status"], "available",
        "orient must carry the coverage block: {oriented}"
    );
    assert_eq!(
        cov["unmeasured"],
        json!(["TypeScript"]),
        "TypeScript is the unmeasured vehicle: {cov}"
    );
    let caveat = cov["caveat"].as_str().expect("caveat present");
    assert!(
        caveat.contains("TypeScript (75% of functions)"),
        "caveat names the unmeasured language with its share: {caveat}"
    );
    // Non-circular: the measured language may (correctly) appear in the "measured for
    // Rust only" lead, but MUST NEVER appear as a caveated "(NN% of functions) is not yet
    // measured" entry — that form is reserved for the unmeasured languages.
    assert!(
        !caveat.contains("Rust ("),
        "measured Rust must not be listed as unmeasured (non-circular): {caveat}"
    );
}

#[test]
fn hotspots_surface_carries_available_coverage_block() {
    // hotspots attaches the block UNCONDITIONALLY (an unmeasured language can be *why* the
    // ranking is empty) via `measurement_coverage_json`. Needs a git history for churn.
    if !git_available() {
        eprintln!("SKIP hotspots_surface_carries_available_coverage_block: git not available");
        return;
    }
    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("mixed");
    write_mixed_fixture(&repo_dir);
    git_init_commit(&repo_dir);
    let repo = index_repo(&dispatcher, &repo_dir);

    let result = expect_success(run(&dispatcher, "h", "hotspots", json!({ "repo": repo })));
    let cov = &result["measurement_coverage"];
    assert_eq!(
        cov["status"], "available",
        "hotspots must carry the coverage block: {result}"
    );
    assert_eq!(cov["unmeasured"], json!(["TypeScript"]));
    assert!(cov["caveat"]
        .as_str()
        .expect("caveat present")
        .contains("TypeScript"));
}
