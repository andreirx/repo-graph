//! ORIENT-FACT-COHERENCE-1 — the reproducing proof (operator ruling D-then-B → Option 2, 2026-08-31).
//!
//! # What this pins
//!
//! The FRAKTAG divergence (audit v0.11.0 fix #1) was a TEMPORAL race, not a budget/serving-route
//! effect: two captures of the SAME snapshot landed on opposite sides of a background enrichment
//! pass, so budgeted `orient` handed the reader a STALE "run `rmap enrich`" CTA (+ "Enrichment phase
//! did not run") while `--full`/`check` — captured after the pass — read "executed".
//!
//! The amended fix: while an enrichment pass is QUEUED or RUNNING **for this repo**, every surface that
//! consumes the shared enrichment-state accessor renders the in-flight truth, and the per-language
//! enrich CTA is suppressed. Both proofs below drive the REAL reader handlers (`handle_orient` /
//! `handle_check` / `handle_reliability`) concurrently with a REAL enrichment writer held in a
//! controlled RUNNING window, and assert the three surfaces tell ONE story:
//!
//!   1. [`auto_pass_in_flight_one_story`] — the AUTO background pass (`run_auto_enrich`), parked
//!      RUNNING at the resolver batch; readers admitted by the W-B epoch alongside the refresh.
//!   2. [`explicit_enrich_in_flight_one_story`] — the EXPLICIT `rmap enrich` handler, driven through
//!      the real dispatch with a NON-canonical legacy `db_path`, parked RUNNING at the resolver batch.
//!      This exercises operator ruling review-3(b): `handle_enrich` must stamp the CANONICAL db path so
//!      a reader querying `repo_state.db_path()` sees the in-flight enrich. Pre-fix (raw stamp) the CTA
//!      leaks through; post-fix it is suppressed — proven through the real handler, not a hand-stamped
//!      canonical path.
//!
//! # No-race / no-live-LSP discipline
//!
//! Auto-enrich / auto-retention / auto-seed are forced OFF (process-global atomics) so no unbidden
//! background thread mutates the snapshot under the assertions. The ONE hermetic stand-in is the
//! resolver (`GatedResolver`): it parks the pass at the resolver batch — after the writer has taken the
//! refresh permit and stamped its in-flight bookkeeping — until the test releases it. Both the auto pass
//! and the explicit handler honor the same `set_test_registry_builder` seam, so neither needs a live
//! tsserver/rust-analyzer/jdtls subprocess.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use enrichment::{
    BatchResolution, EligibilityQuery, EligibleEdge, EnrichmentLanguage, EnrichmentStoragePort,
    ReceiverTypeResolver, ReceiverTypeResult, ResolverError, ResolverProgress, ResolverRegistry,
};
use repo_graph_daemon_runtime::enrich_pass::{
    clear_test_registry_builder, run_auto_enrich, set_auto_enrich_for_test,
    set_test_registry_builder,
};
use repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test;
use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

/// Process-global enrich/seed atomics + the resolver seam ⇒ serialize this binary's tests.
static SERIAL: Mutex<()> = Mutex::new(());
fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _d: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// An isolated dispatcher + its shared state, under a throwaway state root — never touches the
/// operator's real registry. The `Arc<DaemonState>` is returned so the test can spawn the real
/// enrichment writer (`run_auto_enrich`) and build a second dispatcher that shares the same state.
fn isolated() -> (ServiceDispatcher, Arc<DaemonState>, TempDir) {
    let root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(root.path()).expect("isolated registry");
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(Arc::clone(&state));
    (dispatcher, state, root)
}

fn test_overrides() {
    set_auto_enrich_for_test(false);
    set_auto_retention_for_test(false);
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
}

fn req(id: &str, method: &str, params: Value) -> Request {
    Request {
        id: id.to_string(),
        method: method.to_string(),
        params,
    }
}

fn run(d: &ServiceDispatcher, id: &str, method: &str, params: Value) -> DispatchResult {
    let mut e = Quiet;
    d.dispatch(&req(id, method, params), &mut e)
}

#[track_caller]
fn ok(r: DispatchResult) -> Value {
    match r {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => panic!(
            "expected success, got {}: {}",
            e.error.code, e.error.message
        ),
    }
}

/// A multi-edge eligible TS repo (>= 2 untyped `x.method()` calls) so the REAL enrichment pass has a
/// resolver batch to run — the deterministic RUNNING window the gated resolver holds open. Syntax-only
/// indexing leaves the calls unresolved, so pre-enrichment the repo reads LOW call-graph reliability +
/// `NotRun` enrichment: exactly the state that fires the stale "run `rmap enrich`" CTA on orient and the
/// "Enrichment phase did not run" line on check.
fn write_eligible_ts(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("main.ts"),
        "export function run(x): void {\n    x.doThing();\n    x.doOther();\n    x.doThird();\n}\n",
    )
    .unwrap();
}

/// The in-flight reader phrase the shared accessor renders (substring assertion — robust to the exact
/// sentence). Kept in sync with `agent::check::ENRICHMENT_SUMMARY_IN_FLIGHT`.
const IN_FLIGHT_PHRASE: &str = "in progress";

/// The orient enrich CTA string (empty when absent), for the baseline/after assertions.
fn orient_cta(d: &ServiceDispatcher, id: &str, repo: &str) -> String {
    let orient = ok(run(
        d,
        id,
        "orient",
        json!({ "repo": repo, "budget": "medium" }),
    ));
    orient["value"]["relationship_next_action"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// Assert the three FRAKTAG surfaces tell ONE in-flight story: orient's stale CTA suppressed + the
/// in-flight phrase rendered, check's ENRICHMENT_STATE a non-failing in-flight form (never "did not
/// run"), reliability's shared enrichment summary + machine token following the SAME fact.
#[track_caller]
fn assert_one_in_flight_story(d: &ServiceDispatcher, repo: &str, tag: &str) {
    let orient = ok(run(
        d,
        &format!("{tag}-o"),
        "orient",
        json!({ "repo": repo, "budget": "medium" }),
    ));
    let cta = orient["value"]["relationship_next_action"]
        .as_str()
        .unwrap_or("");
    assert!(
        !cta.contains("rmap enrich"),
        "{tag}: in-flight orient must NOT hand a stale enrich CTA: {cta}"
    );
    assert!(
        cta.contains(IN_FLIGHT_PHRASE),
        "{tag}: in-flight orient must render the in-flight truth: {cta}"
    );

    let check_s = ok(run(
        d,
        &format!("{tag}-c"),
        "check",
        json!({ "repo": repo }),
    ))
    .to_string();
    assert!(
        check_s.contains(IN_FLIGHT_PHRASE),
        "{tag}: in-flight check must render the in-flight truth: {check_s}"
    );
    assert!(
        !check_s.contains("Enrichment phase did not run"),
        "{tag}: in-flight check must NOT say the phase did not run: {check_s}"
    );

    let rel = ok(run(
        d,
        &format!("{tag}-r"),
        "reliability",
        json!({ "repo": repo }),
    ));
    assert_eq!(
        rel["enrichment_state"].as_str(),
        Some("in_flight"),
        "{tag}: reliability machine token follows the in-flight fact"
    );
    assert!(
        rel["enrichment_summary"]
            .as_str()
            .unwrap_or("")
            .contains(IN_FLIGHT_PHRASE),
        "{tag}: reliability summary is in-flight"
    );
}

/// A hermetic stand-in for a real LSP resolver (no tsserver subprocess). It records that it STARTED
/// (so the test knows the writer reached the resolver batch — i.e. is holding the refresh lock and has
/// stamped its in-flight bookkeeping), then PARKS until the test releases it (bounded so a bug fails
/// fast), then resolves every edge successfully so the pass completes normally (not cancelled). This
/// gives a deterministic "a real enrichment writer is RUNNING" window without racing a live toolchain.
struct GatedResolver {
    lang: EnrichmentLanguage,
    started: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
}

impl ReceiverTypeResolver for GatedResolver {
    fn language(&self) -> EnrichmentLanguage {
        self.lang
    }
    fn resolve_batch(
        &self,
        _repo_root: &Path,
        edges: &[EligibleEdge],
        _progress: Option<&dyn ResolverProgress>,
        _cancel: Option<&dyn Fn() -> bool>,
    ) -> BatchResolution {
        self.started.store(true, Ordering::SeqCst);
        let start = Instant::now();
        while !self.release.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(30) {
            thread::sleep(Duration::from_millis(5));
        }
        BatchResolution::from_results(
            edges
                .iter()
                .map(|e| {
                    ReceiverTypeResult::success(
                        e.edge_uid.clone(),
                        "SomeType".to_string(),
                        Some("SomeType".to_string()),
                        false,
                    )
                })
                .collect(),
        )
    }
    fn initialize(&mut self, _repo_root: &Path) -> Result<(), ResolverError> {
        Ok(())
    }
    fn shutdown(&mut self) {}
}

/// Install the gated fake resolver backend (honored by BOTH the auto pass and the explicit handler) and
/// return `(started, release, seam_guard)`. The guard clears the process-global seam on ANY exit BEFORE
/// the serial lock releases, so no other test inherits it.
fn install_gated_resolver() -> (Arc<AtomicBool>, Arc<AtomicBool>, SeamClear) {
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    {
        let started = Arc::clone(&started);
        let release = Arc::clone(&release);
        set_test_registry_builder(move |to_run: &[EnrichmentLanguage]| {
            let mut reg = ResolverRegistry::new();
            for &lang in to_run {
                reg.register(Box::new(GatedResolver {
                    lang,
                    started: Arc::clone(&started),
                    release: Arc::clone(&release),
                }));
            }
            reg
        });
    }
    (started, release, SeamClear)
}

struct SeamClear;
impl Drop for SeamClear {
    fn drop(&mut self) {
        clear_test_registry_builder();
    }
}

/// Assert the fixture really produced >= 2 eligible TS edges (else there is no running batch to park in).
/// Read through the same eligibility query the pass uses.
fn assert_eligible_ts(db_path: &str, repo_uid: &str) {
    let storage = StorageConnection::open(db_path).unwrap();
    let snap = storage.get_latest_snapshot(repo_uid).unwrap().unwrap();
    let n = storage
        .query_eligible_edges(&EligibilityQuery::new(&snap.snapshot_uid))
        .unwrap()
        .into_iter()
        .filter(|e| e.language == EnrichmentLanguage::TypeScript)
        .count();
    assert!(
        n >= 2,
        "fixture must yield >= 2 eligible TS edges for the running window (got {n})"
    );
}

/// Block until `started` AND the writer has reached the resolver batch, or fail fast.
fn wait_started(started: &AtomicBool) {
    let start = Instant::now();
    while !started.load(Ordering::SeqCst) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the enrichment writer never reached the resolver batch"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// Proof 1 — the AUTO background pass. Spawns the REAL `run_auto_enrich` → `run_enrich_pass` →
/// resolver batch, parks it RUNNING (holding the repo refresh lock + registered running + in-flight
/// bookkeeping), and asserts orient/check/reliability — admitted concurrently by the W-B epoch — render
/// the in-flight truth with the stale CTA suppressed. Releasing the resolver lets the pass complete; the
/// surfaces then leave the in-flight form.
#[test]
fn auto_pass_in_flight_one_story() {
    let _g = serial();
    test_overrides();
    let (dispatcher, state, _root) = isolated();

    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("tsrepo");
    write_eligible_ts(&repo_dir);

    let indexed = ok(run(
        &dispatcher,
        "i",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let repo_param = repo_dir.to_string_lossy().to_string();
    assert_eligible_ts(&db_path, &repo_uid);

    // Baseline: honest pre-enrichment CTA present.
    let cta_before = orient_cta(&dispatcher, "o0", &repo_param);
    assert!(
        cta_before.contains("rmap enrich"),
        "baseline: pre-enrichment enrich CTA present: {cta_before}"
    );

    let (started, release, _seam) = install_gated_resolver();

    // Spawn the REAL background pass (bump the generation the way `spawn_auto_enrich` does).
    let my_gen = state.enrich_coord().bump_generation(&repo_uid);
    let pass = {
        let state = Arc::clone(&state);
        let db_path = db_path.clone();
        let repo_uid = repo_uid.clone();
        let display = repo_param.clone();
        thread::spawn(move || {
            run_auto_enrich(&state, Path::new(&db_path), &repo_uid, &display, my_gen);
        })
    };

    wait_started(&started);
    assert_eq!(
        state.enrich_coord().activity_state(),
        "running",
        "the pass holding the refresh lock is running"
    );

    // DURING the real running pass: the three surfaces tell ONE in-flight story, CTA suppressed.
    assert_one_in_flight_story(&dispatcher, &repo_param, "auto-running");

    // Release the pass; it completes + promotes, then drops the refresh/running/flight guards.
    release.store(true, Ordering::SeqCst);
    pass.join().expect("pass thread joined");
    assert_eq!(
        state.enrich_coord().activity_state(),
        "idle",
        "after the real pass completes, the coordinator is idle again"
    );

    // AFTER completion: the surfaces leave the in-flight form (never the stale in-flight line for a pass
    // that has finished).
    let orient_after = ok(run(
        &dispatcher,
        "o2",
        "orient",
        json!({ "repo": repo_param, "budget": "medium" }),
    ));
    let cta_after = orient_after["value"]["relationship_next_action"]
        .as_str()
        .unwrap_or("");
    assert!(
        !cta_after.contains(IN_FLIGHT_PHRASE),
        "after completion, orient must NOT render the in-flight line: {cta_after}"
    );
    let rel_after = ok(run(
        &dispatcher,
        "r2",
        "reliability",
        json!({ "repo": repo_param }),
    ));
    assert_ne!(
        rel_after["enrichment_state"].as_str(),
        Some("in_flight"),
        "after completion, reliability must NOT report in_flight"
    );
}

/// Proof 2 — the EXPLICIT `rmap enrich` handler, through the REAL dispatch, exercising the canon-stamp
/// fix (operator ruling review-3(b)). An explicit enrich lives in the ActivityRegistry (`OpKind::Enrich`),
/// and `handle_enrich` must stamp the CANONICAL db path so a reader querying `repo_state.db_path()` sees
/// it. We drive the LEGACY `db_path` + `repo_uid` form with a deliberately NON-canonical `db_path`
/// spelling (a `..` round-trip): `RepoKey::new` canonicalizes it for the lookup, so `repo_state.db_path()`
/// is canonical while the raw param is not. Pre-fix `handle_enrich` stamped the raw spelling → the reader's
/// exact-match query missed it → the stale CTA leaked; post-fix it stamps the canonical path → the CTA is
/// suppressed. The gated resolver parks the REAL handler RUNNING (after it has stamped the activity) so a
/// concurrent real `orient`/`check`/`reliability` observes the live stamp.
#[test]
fn explicit_enrich_in_flight_one_story() {
    let _g = serial();
    test_overrides();
    let (dispatcher, state, _root) = isolated();

    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("tsrepo");
    write_eligible_ts(&repo_dir);

    let indexed = ok(run(
        &dispatcher,
        "i",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let repo_param = repo_dir.to_string_lossy().to_string();
    assert_eligible_ts(&db_path, &repo_uid);

    // A deliberately NON-canonical spelling of the SAME db file: `<parent>/../<parent_name>/<file>`.
    // `Path` exact-equality (what `active_for_db` uses) sees this as distinct from the canonical path,
    // while `RepoKey::new`/`canonicalize` collapse it back — so a raw stamp would miss the reader's query.
    let db = Path::new(&db_path);
    let file_name = db.file_name().expect("db file name");
    let parent = db.parent().expect("db parent");
    let parent_name = parent.file_name().expect("parent name");
    let non_canonical = parent.join("..").join(parent_name).join(file_name);
    let non_canonical_str = non_canonical.to_string_lossy().to_string();
    assert_ne!(
        Path::new(&non_canonical_str),
        std::fs::canonicalize(&db_path).unwrap().as_path(),
        "the constructed legacy db_path must be non-canonical for this test to discriminate the fix"
    );

    // Baseline: honest pre-enrichment CTA present.
    let cta_before = orient_cta(&dispatcher, "o0", &repo_param);
    assert!(
        cta_before.contains("rmap enrich"),
        "baseline: pre-enrichment enrich CTA present: {cta_before}"
    );

    let (started, release, _seam) = install_gated_resolver();

    // Spawn the REAL explicit-enrich handler on a second dispatcher sharing the same state, using the
    // LEGACY positional form with the NON-canonical db_path. It takes the refresh permit, stamps the
    // `OpKind::Enrich` activity op (canonical, post-fix), then parks in the gated resolver batch.
    let enrich = {
        let d2 = ServiceDispatcher::new(Arc::clone(&state));
        let db_path = non_canonical_str.clone();
        let repo_uid = repo_uid.clone();
        thread::spawn(move || {
            run(
                &d2,
                "e",
                "enrich",
                json!({ "db_path": db_path, "repo_uid": repo_uid, "languages": ["typescript"] }),
            )
        })
    };

    wait_started(&started);

    // DURING the real running explicit enrich: the three surfaces tell ONE in-flight story with the stale
    // CTA suppressed. This ONLY passes if `handle_enrich` stamped the CANONICAL path (the readers query
    // `repo_state.db_path()`); with the pre-fix raw stamp the CTA would leak here — proving the fix
    // through the real handler, not a hand-canonicalized test stamp.
    assert_one_in_flight_story(&dispatcher, &repo_param, "explicit-enrich");

    // Release the resolver; the explicit enrich completes and drops its activity op.
    release.store(true, Ordering::SeqCst);
    let enrich_result = enrich.join().expect("enrich thread joined");
    // The real handler completed successfully (reached the resolver batch via the seam, not an early
    // "no resolvers available" error).
    let _ = ok(enrich_result);

    // AFTER completion: the honest pre-enrichment CTA returns (default enrich does not promote, so the
    // persisted enrichment state is still NotRun once the in-flight stamp clears).
    let cta_after = orient_cta(&dispatcher, "o2", &repo_param);
    assert!(
        cta_after.contains("rmap enrich"),
        "after the explicit enrich clears, the honest enrich CTA returns: {cta_after}"
    );
}
