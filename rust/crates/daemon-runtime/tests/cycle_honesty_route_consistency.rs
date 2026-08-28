//! CYCLE-HONESTY-1 (§2.4, operator ruling 2026-08-28 item 1 + review-2) — the TS/JS type-only caveat is
//! ROUTE-CONSISTENT: every `cycles` route derives `ts_type_only_caveat` from the SAME repo-level stored
//! language facts under the SAME ≥10%-of-code-files materiality gate (`snapshot_has_material_ts_js` →
//! `reader_context::repo_has_material_ts_js`), NOT from the in-memory answer envelope's
//! `contributing_languages` (the review-2 divergence).
//!
//! These SURFACE proofs drive `ServiceDispatcher::dispatch` end-to-end against a REAL on-disk index in an
//! ISOLATED temp state root (the operator's registry/daemon are never touched). They exercise the exact
//! code path review-2 flagged: the DEFAULT (`auto`) route's caveat read, and its agreement with the forced
//! `--engine sqlite` route.
//!
//! Scope note (honest): the caveat is `material_ts_js && count > 0`, so its TRUE value is only observable
//! when a cycle renders. This dispatcher-level harness never PRELOADS a LiveGraph (that needs a real SCIP
//! partition), so the DEFAULT (`auto`) route here always falls back to SQLite — asserted explicitly below,
//! so the "auto == sqlite" caveat agreement is not silently vacuous. The RESIDENT-LiveGraph route
//! consistency — the `auto` route SERVING `backend_used=livegraph`, the caveat's TRUE path, and equality
//! across the LiveGraph fastpath + the explicit `file-import`/`module-import` routes + the SQLite fallback —
//! is proven in-crate (where the faithful resident-LiveGraph fixture lives, `pub(crate)` and unreachable
//! from an integration-test crate) by
//! `livegraph_feed::…::cycles_caveat_route_consistent_on_resident_livegraph`. The explicit LiveGraph routes
//! ARE still driven end-to-end here (they answer `Unavailable` with no resident graph) to prove the dispatch
//! match arms route them and that they read the caveat from the same stored facts. The arrow↔import honesty
//! is validated by the live corpus proof (leveldb/django for arrows; a TS repo for the caveat) recorded in
//! the build report. The ≥10% gate itself is unit-tested in
//! `reader_context::material_ts_js_requires_ten_percent_not_mere_presence`.

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
    // dispatch queues: with a LIVE local embeddings endpoint the seed pass actually runs and
    // holds the DB while the test reads it -> `database is locked` flakes (4th recurrence of
    // this class, 2026-08-28; same override seed_seam.rs / forget_repo.rs use).
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
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

/// The raw `cycles` dispatch result for `engine`/`kind` (`engine == "auto"` + empty `kind` = the default
/// route). Passing an empty `kind` omits the param entirely.
fn cycles(dispatcher: &ServiceDispatcher, repo: &str, engine: &str, kind: &str) -> Value {
    let mut params = json!({ "repo": repo });
    if engine != "auto" {
        params["engine"] = json!(engine);
    }
    if !kind.is_empty() {
        params["kind"] = json!(kind);
    }
    expect_success(dispatch(dispatcher, "cyc", "cycles", params))
}

/// The `ts_type_only_caveat` field of a `cycles` result. Asserts it is present and boolean — a route that
/// dropped it (or emitted a non-bool) is itself a regression the honesty contract forbids.
#[track_caller]
fn caveat_of(out: &Value) -> bool {
    out["ts_type_only_caveat"]
        .as_bool()
        .unwrap_or_else(|| panic!("ts_type_only_caveat must be a bool: {out}"))
}

/// The `ts_type_only_caveat` field from a `cycles` dispatch on `engine` (`"auto"` = default route).
#[track_caller]
fn caveat(dispatcher: &ServiceDispatcher, repo: &str, engine: &str) -> bool {
    caveat_of(&cycles(dispatcher, repo, engine, ""))
}

fn write_rust_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("main.rs"),
        "fn helper() -> i32 { 1 }\n\nfn main() {\n    let _ = helper();\n}\n",
    )
    .unwrap();
}

/// A pure-TS repo (100% TS ⇒ materially TS/JS). Two files, one importing the other — no INTER-module
/// cycle (same directory ⇒ same module), so `count == 0` and the caveat is false; the point of this
/// fixture is that BOTH routes agree on that false, computed from the same stored TS facts.
fn write_ts_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("helper.ts"), "export function h() { return 1; }\n").unwrap();
    std::fs::write(
        dir.join("main.ts"),
        "import { h } from './helper';\nexport function m() { return h(); }\n",
    )
    .unwrap();
}

#[test]
fn rust_repo_caveat_false_and_identical_across_routes() {
    let (dispatcher, _root) = isolated();
    let repo_dir = tempdir().unwrap();
    write_rust_repo(repo_dir.path());
    let repo = index_repo(&dispatcher, repo_dir.path());

    let auto_out = cycles(&dispatcher, &repo, "auto", "");
    // No LiveGraph is preloaded in this hermetic harness, so the DEFAULT route falls back to SQLite. Assert
    // that explicitly: it documents WHY the auto/sqlite caveat agreement below is not vacuous, and pins the
    // fact that the resident-LiveGraph fastpath (proven in-crate) is simply not exercised HERE.
    assert_eq!(
        auto_out["backend_used"], "sqlite",
        "no preloaded LiveGraph -> the default route falls back to SQLite in this harness"
    );
    let auto = caveat_of(&auto_out);
    let sqlite = caveat(&dispatcher, &repo, "sqlite");
    // Non-TS repo: never flagged, and the two routes agree (both read the same stored language facts).
    assert!(
        !auto,
        "a Rust repo must not carry the TS/JS type-only caveat"
    );
    assert_eq!(
        auto, sqlite,
        "default (auto) and --engine sqlite must agree on the caveat basis"
    );
}

/// The explicit LiveGraph cycles routes (`--engine livegraph --kind {file-import,module-import}`) are
/// driven end-to-end through the dispatcher. With no preloaded LiveGraph they answer `Unavailable`
/// (`backend_used=livegraph`, `count=0`), and — the review-3 point — they read `ts_type_only_caveat` from
/// the SAME repo-level stored language facts as every other route (a non-TS repo is never flagged; the
/// value equals the SQLite route's). This proves the dispatch match arms route these engines and that they
/// share the caveat basis; the RESIDENT-LiveGraph TRUE path + fastpath `backend_used=livegraph` are proven
/// in-crate (`cycles_caveat_route_consistent_on_resident_livegraph`).
#[test]
fn explicit_livegraph_routes_share_the_caveat_basis() {
    let (dispatcher, _root) = isolated();
    let repo_dir = tempdir().unwrap();
    write_rust_repo(repo_dir.path());
    let repo = index_repo(&dispatcher, repo_dir.path());

    let sqlite = caveat(&dispatcher, &repo, "sqlite");
    for kind in ["file-import", "module-import"] {
        let out = cycles(&dispatcher, &repo, "livegraph", kind);
        assert_eq!(
            out["backend_used"], "livegraph",
            "the explicit livegraph {kind} route is dispatched to the LiveGraph engine"
        );
        assert_eq!(
            caveat_of(&out),
            sqlite,
            "the livegraph {kind} route reads the SAME caveat basis as the SQLite route"
        );
    }
}

#[test]
fn ts_repo_caveat_identical_across_routes() {
    let (dispatcher, _root) = isolated();
    let repo_dir = tempdir().unwrap();
    write_ts_repo(repo_dir.path());
    let repo = index_repo(&dispatcher, repo_dir.path());

    // The default (auto) route now reads the SAME stored per-language file facts as the SQLite route for
    // the caveat (review-2 fix: no longer `contributing_languages`). Whatever the value, the two routes
    // must produce it identically — a divergence is exactly the regression this slice removed.
    let auto = caveat(&dispatcher, &repo, "auto");
    let sqlite = caveat(&dispatcher, &repo, "sqlite");
    assert_eq!(
        auto, sqlite,
        "default (auto) and --engine sqlite must agree on the caveat basis for a TS repo"
    );
}
