//! EXPLAIN-CYCLES-HONEST-1 (ECH-C04) — `explain`'s Import-cycles walk is ROUTE-CONSISTENT with
//! `orient`'s: both derive the cycle's directed `walk` from the SAME SQLite serving computation
//! (`storage::agent_cycle_labeling::label_{module,focus}_cycles` → the shared `cycle_walk` kernel),
//! so a symbol-focus `explain` and a path-focus `explain` carry, for a cycle, the SAME ordered ring
//! `orient` carries for that same member set. Before this slice the focus-scoped reads built their
//! cycles with `walk: None`, and `explain`'s renderer drew a ring from the lexically-sorted member
//! SET (RC-4) — arrows over edges the import graph does not hold.
//!
//! These are SURFACE proofs: they drive `ServiceDispatcher::dispatch` end-to-end against a REAL
//! on-disk index in an ISOLATED temp state root (the operator's registry/daemon are never touched).
//! The fixture is a real 3-module import ring — `src/a` -> `src/b` -> `src/c` -> `src/a` via TS
//! relative imports across directories — so the real indexer produces a genuine MODULE-cycle SCC
//! with a verifiable directed walk, exercising the exact focus-read → labeling-kernel path the
//! slice rewires (not a synthetic ring injected past the resolver).

use std::path::Path;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

fn isolated() -> (ServiceDispatcher, TempDir) {
    // Disable the REAL background maintenance passes (enrich -> seed -> retention) the index
    // dispatch queues, so they never hold the DB while the test reads it (the `database is
    // locked` flake class; same override cycle_honesty_route_consistency.rs uses).
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(state);
    (dispatcher, state_root)
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

fn dispatch(
    dispatcher: &ServiceDispatcher,
    id: &str,
    method: &str,
    params: Value,
) -> DispatchResult {
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

fn index_repo(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> String {
    let indexed = expect_success(dispatch(
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

/// A real 3-module import ring: `src/a` -> `src/b` -> `src/c` -> `src/a`, each directory a module
/// whose `index.ts` imports the next directory's module via a relative path. Each module also
/// exports a uniquely-named function so a symbol-focus `explain` has a symbol to resolve.
fn write_ring_repo(dir: &Path) {
    for (m, next, fnname) in [
        ("a", "b", "alphaEntry"),
        ("b", "c", "betaEntry"),
        ("c", "a", "gammaEntry"),
    ] {
        let md = dir.join("src").join(m);
        std::fs::create_dir_all(&md).unwrap();
        std::fs::write(
            md.join("index.ts"),
            format!(
                "import {{ next as _next }} from '../{next}';\nexport function {fnname}() {{ return _next; }}\nexport const next = 1;\n"
            ),
        )
        .unwrap();
    }
}

/// The `signals` array of a dispatched orient/explain CoherenceEnvelope (`value.signals`), each
/// element itself a `{ kind, value: Signal }` envelope.
fn signals(out: &Value) -> &Vec<Value> {
    out["value"]["signals"]
        .as_array()
        .expect("coherence envelope carries value.signals[]")
}

/// The evidence object of the first signal whose `value.code` equals `code`, or `None` if absent.
fn evidence_of<'a>(out: &'a Value, code: &str) -> Option<&'a Value> {
    signals(out).iter().find_map(|s| {
        let sig = &s["value"];
        (sig["code"] == code).then(|| &sig["evidence"])
    })
}

/// The first cycle's `walk` from an evidence object whose cycle list lives under `array_key`
/// (`"cycles"` for orient's IMPORT_CYCLES, `"items"` for explain's EXPLAIN_CYCLES). Returns the
/// ordered ring as `Vec<String>` (asserting it is a non-empty array of strings) or `None` when no
/// walk is carried.
fn first_walk(evidence: &Value, array_key: &str) -> Option<Vec<String>> {
    let first = evidence.get(array_key)?.as_array()?.first()?;
    let walk = first.get("walk")?.as_array()?;
    let names: Vec<String> = walk
        .iter()
        .map(|v| v.as_str().expect("walk element is a string").to_string())
        .collect();
    Some(names)
}

/// orient's IMPORT_CYCLES first cycle walk on the repo focus (the reference derivation).
fn orient_walk(dispatcher: &ServiceDispatcher, repo: &str) -> Option<Vec<String>> {
    let out = expect_success(dispatch(
        dispatcher,
        "ori",
        "orient",
        json!({ "repo": repo }),
    ));
    let ev = evidence_of(&out, "IMPORT_CYCLES")?;
    first_walk(ev, "cycles")
}

/// explain's EXPLAIN_CYCLES first cycle walk for a given focus target.
fn explain_walk(dispatcher: &ServiceDispatcher, repo: &str, target: &str) -> Option<Vec<String>> {
    let out = expect_success(dispatch(
        dispatcher,
        "exp",
        "explain",
        json!({ "repo": repo, "target": target }),
    ));
    let ev = evidence_of(&out, "EXPLAIN_CYCLES")?;
    first_walk(ev, "items")
}

#[test]
fn explain_symbol_focus_cycle_walk_equals_orient_cycle_walk() {
    let (dispatcher, _root) = isolated();
    let repo_dir = tempdir().unwrap();
    write_ring_repo(repo_dir.path());
    let repo = index_repo(&dispatcher, repo_dir.path());

    let orient = orient_walk(&dispatcher, &repo)
        .expect("orient carries a verified walk for the real module ring");
    assert!(
        orient.len() >= 2,
        "the ring's walk is a directed cycle of >= 2 members: {orient:?}"
    );

    // Symbol focus: `explain alphaEntry` routes through find_cycles_involving_module (the owning
    // module's cycles). Its EXPLAIN_CYCLES walk must be the SAME ring orient derived.
    let symbol = explain_walk(&dispatcher, &repo, "alphaEntry")
        .expect("explain (symbol focus) carries a walk from the shared labeling kernel");
    assert_eq!(
        symbol, orient,
        "explain (symbol focus) and orient must derive the SAME cycle walk"
    );
}

#[test]
fn explain_path_focus_cycle_walk_equals_orient_cycle_walk() {
    let (dispatcher, _root) = isolated();
    let repo_dir = tempdir().unwrap();
    write_ring_repo(repo_dir.path());
    let repo = index_repo(&dispatcher, repo_dir.path());

    let orient = orient_walk(&dispatcher, &repo)
        .expect("orient carries a verified walk for the real module ring");
    assert!(orient.len() >= 2, "walk is a real ring: {orient:?}");

    // Path focus: `explain src/a` routes through find_cycles_involving_path. Its EXPLAIN_CYCLES
    // walk must be the SAME ring orient derived.
    let path = explain_walk(&dispatcher, &repo, "src/a")
        .expect("explain (path focus) carries a walk from the shared labeling kernel");
    assert_eq!(
        path, orient,
        "explain (path focus) and orient must derive the SAME cycle walk"
    );
}
