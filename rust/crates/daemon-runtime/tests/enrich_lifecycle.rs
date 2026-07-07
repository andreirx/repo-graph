//! ENRICH-LIFECYCLE-1 — daemon-level named proofs, driven through the REAL
//! `ServiceDispatcher::dispatch` surface against REAL dispatched `index` writes.
//!
//! # What this binary proves that the `enrich_pass` lib tests do not
//!
//! The `enrich_pass` module's own `#[cfg(test)]` proofs drive the PURE core (opt-out parsing,
//! `plan_languages` toolchain split, the generation counter, the run slot, outcome summaries). What
//! they cannot prove is the wiring one layer up: that a real `handle_index` dispatch **auto-triggers**
//! the pass (`finish_write_with_maintenance` → `spawn_auto_enrich`), that the two-gate + generation
//! discipline behaves against a REAL in-flight op, that opt-out gates the real trigger, that a real
//! eligible-edge repo with an absent toolchain produces an honest SKIP, and that `rmap enrich`
//! resolves the repo from cwd through the real `handle_enrich`. That is the gap this binary closes.
//!
//! # The named-proofs map
//!
//! | Proof (slice §5) | Home |
//! |------------------|------|
//! | **Auto-trigger** (real index → pass spawned → report recorded) | `auto_trigger_*` (HERE) |
//! | **Enrich→retention chain** (both ON → both passes run, in order, no starvation) | `enrich_chains_retention_*` (HERE) |
//! | **Supersede** (a newer trigger drops the older queued pass) | `newer_trigger_supersedes_*` (HERE) |
//! | **Toolchain-skip** (real eligible edges + absent toolchain → honest skip, no LSP) | `absent_toolchain_*` (HERE) |
//! | **Opt-out** (`RMAP_AUTO_ENRICH` off → no pass, honest "disabled" reply) | `opt_out_*` (HERE) |
//! | **Contention yield — pre-start** (defers while another op writes, runs when idle) | `enrich_yields_*` (HERE) |
//! | **Contention yield — cancel-of-running** (a RUNNING pass releases the write lock to an explicit index) | `explicit_index_makes_a_running_enrichment_yield_*` (HERE) + `enrich_pass::tests` + `enrichment`/`pipeline` |
//! | **Ergonomics / REG-1** (`rmap enrich` from cwd resolves the repo) | `cwd_resolved_manual_enrich_*` (HERE) |
//! | Opt-out value parsing / plan split / generation / slot / summaries | `enrich_pass::tests::*` (lib) |
//! | Doctor lifecycle line rendering | `rgr … doctor::daemon_info::enrichment_probe_*` |
//! | Completion-report "enrichment: … queued/disabled" line | `rgr … commands::index::format_enrichment_line` (via daemon reply) |
//!
//! # Determinism / no-LSP discipline
//!
//! `set_auto_enrich_for_test` + `set_auto_retention_for_test` are PROCESS-GLOBAL atomics, so every
//! test serializes on [`ENRICH_SERIAL`] and sets the overrides it needs while holding it. Retention is
//! forced OFF in every test EXCEPT the chain proof (`enrich_chains_retention_when_both_enabled`), which
//! turns it ON to verify enrichment chains retention and waits for BOTH reports before returning (so no
//! pass thread races tempdir teardown). OFF elsewhere makes the chained retention spawn a cheap no-op.
//! Every test is hermetic (NO real LSP): the trigger/contention/opt-out/cwd fixtures have ZERO
//! eligible edges (the pipeline early-returns before any resolver init); the toolchain-skip fixture
//! injects an `available = |_| false` predicate so `run_enrich_pass` returns at the plan step before
//! opening a resolver; and the cancel-of-running proof, which needs a REAL running pass with real
//! eligible edges, installs a fake `ParkingResolver` via `set_test_registry_builder` (cleared on exit
//! by a drop-guard) in place of the LSP resolvers. This keeps the suite fast and hermetic regardless
//! of what toolchains the host has.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use enrichment::{
    EligibilityQuery, EligibleEdge, EnrichmentLanguage, EnrichmentStoragePort,
    ReceiverTypeResolver, ReceiverTypeResult, ResolverError, ResolverProgress, ResolverRegistry,
};
use repo_graph_daemon_runtime::activity::OpKind;
use repo_graph_daemon_runtime::enrich_pass::{
    clear_test_registry_builder, run_auto_enrich, run_enrich_pass, set_auto_enrich_for_test,
    set_test_registry_builder, try_enrich_attempt, EnrichAttempt,
};
use repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test;
use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

/// Serializes every test in this binary — the enrich/retention ON/OFF switches are process-global
/// atomics. Poison-tolerant so one panicking test does not cascade-fail the rest.
static ENRICH_SERIAL: Mutex<()> = Mutex::new(());

fn serial_guard() -> MutexGuard<'static, ()> {
    ENRICH_SERIAL
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// The overrides EVERY test wants: retention OFF (not under test; keeps the chained retention spawn a
/// no-op), enrich set per the caller. Held under the serial lock.
fn set_overrides(enrich_on: bool) {
    set_auto_retention_for_test(false);
    set_auto_enrich_for_test(enrich_on);
}

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

fn isolated() -> (ServiceDispatcher, std::sync::Arc<DaemonState>, TempDir) {
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(std::sync::Arc::clone(&state));
    (dispatcher, state, state_root)
}

/// A TypeScript repo with ZERO eligible (obj.method-needs-type) edges — a lone free function. The
/// enrichment pipeline early-returns on an empty eligible set, so the pass records a report WITHOUT
/// ever starting a resolver: fast + hermetic. Used by every proof that only needs the pass to
/// run/record (trigger / contention / opt-out / cwd-manual).
fn write_no_eligible(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("lib.ts"),
        "export function onlyAFreeFunction(): number {\n    return 42;\n}\n",
    )
    .unwrap();
}

/// A TypeScript repo WITH an eligible edge: `x.doThing()` on an untyped receiver is the canonical
/// `CallsObjMethodNeedsTypeInfo` case the enrichment eligibility contract targets. Used by the
/// toolchain-skip proof (which then injects `available = |_| false`, so no resolver ever starts).
fn write_with_eligible(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("main.ts"),
        "export function run(x): void {\n    x.doThing();\n    x.doOther();\n}\n",
    )
    .unwrap();
}

/// A TypeScript repo with SEVERAL eligible edges (three untyped `x.method()` calls). The
/// cancel-of-running proof needs >= 2 so the fake resolver can process the first edge, then PARK at
/// the batch boundary waiting for the explicit index's yield signal — leaving the rest unprocessed
/// (concretely "stopped WITHIN the batch").
fn write_multi_eligible(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("main.ts"),
        "export function run(x): void {\n    x.doThing();\n    x.doOther();\n    x.doThird();\n}\n",
    )
    .unwrap();
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
        DispatchResult::Error(e) => panic!(
            "expected success, got error {}: {}",
            e.error.code, e.error.message
        ),
    }
}

fn index(dispatcher: &ServiceDispatcher, id: &str, repo_dir: &Path) -> Value {
    expect_success(run(
        dispatcher,
        id,
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ))
}

/// Poll `last_enrichment_json()` until the detached background pass records a report, or panic.
fn wait_for_enrichment_report(state: &DaemonState, timeout: Duration) -> Value {
    let start = Instant::now();
    loop {
        if let Some(v) = state.last_enrichment_json() {
            return v;
        }
        assert!(
            start.elapsed() < timeout,
            "background enrichment pass did not record a report within {timeout:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// Poll `last_retention_json()` until the CHAINED retention pass records a report, or panic. Used only
/// by the enrich→retention chain proof (the one test here that runs with retention ON).
fn wait_for_retention_report(state: &DaemonState, timeout: Duration) -> Value {
    let start = Instant::now();
    loop {
        if let Some(v) = state.last_retention_json() {
            return v;
        }
        assert!(
            start.elapsed() < timeout,
            "chained retention pass did not record a report within {timeout:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn attempt_label(a: &EnrichAttempt) -> String {
    match a {
        EnrichAttempt::Ran(o) => format!(
            "Ran(eligible={}, skipped={})",
            o.eligible_count,
            o.skipped.len()
        ),
        EnrichAttempt::Yielded(r) => format!("Yielded({r})"),
        EnrichAttempt::Superseded => "Superseded".to_string(),
        EnrichAttempt::Failed(e) => format!("Failed({e})"),
    }
}

// ── AUTO-TRIGGER — a completed real index spawns the pass, which runs and records ─────────────────

#[test]
fn auto_trigger_queues_pass_and_records_report() {
    let _serial = serial_guard();
    set_overrides(true); // enrich ON

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_no_eligible(&repo_dir);

    let indexed = index(&dispatcher, "idx", &repo_dir);

    // (1) The synchronous reply proves the dispatch path DECIDED to queue enrichment (never runs it here).
    assert_eq!(
        indexed["enrichment"]["auto_pass"], "queued",
        "a completed index with enrichment ON must report the background pass as queued: {indexed}"
    );

    // (2) The spawned pass actually RAN and recorded its report (proves spawn_auto_enrich executed).
    let report = wait_for_enrichment_report(&state, Duration::from_secs(20));
    assert_eq!(
        report["eligible_count"], 0,
        "the zero-eligible fixture resolves nothing (no LSP): {report}"
    );
    assert_eq!(
        report["state"], "completed",
        "nothing eligible → a clean completed pass, not an error: {report}"
    );
}

// ── ENRICH→RETENTION CHAIN — both passes run, in order, without starving each other ───────────────

/// The SEQUENCING proof (slice §3, packet "VERIFY the interaction; both passes must not deadlock or
/// starve each other"). With BOTH enrichment AND retention ON, a single real index queues enrichment,
/// which on completion CHAINS retention (`run_auto_enrich` → `chain_retention` → `spawn_auto_retention`).
/// Proven end-to-end: the enrichment report is recorded, AND the retention report is recorded shortly
/// after — so retention is neither skipped (enrich ON must not swallow it) nor starved (a long enrich
/// holding the write lock cannot block retention forever; they run in sequence, not in contention).
///
/// This is the ONE test in this binary that runs with retention ON; it waits for BOTH reports before
/// returning, so no background pass thread is left racing the tempdir teardown. Uses the zero-eligible
/// fixture, so enrichment early-returns with no LSP and the chain is fast + hermetic.
#[test]
fn enrich_chains_retention_when_both_enabled() {
    let _serial = serial_guard();
    // Both ON — NOT the shared `set_overrides` helper (which forces retention OFF): this is the sole
    // test that exercises the chain, so it opts retention back in while holding the serial lock.
    set_auto_retention_for_test(true);
    set_auto_enrich_for_test(true);

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_no_eligible(&repo_dir);

    let indexed = index(&dispatcher, "idx", &repo_dir);
    // The synchronous reply reports BOTH maintenance passes as queued (enrichment directly; retention
    // via the chain — the reply's `retention.auto_pass` reflects `auto_retention_enabled()`).
    assert_eq!(
        indexed["enrichment"]["auto_pass"], "queued",
        "enrichment queued on the completion reply: {indexed}"
    );
    assert_eq!(
        indexed["retention"]["auto_pass"], "queued",
        "retention queued too (it chains behind enrichment): {indexed}"
    );

    // (1) Enrichment ran and recorded (the head of the chain).
    let enr = wait_for_enrichment_report(&state, Duration::from_secs(20));
    assert_eq!(enr["state"], "completed", "enrichment completed: {enr}");

    // (2) Retention ran and recorded (the chained tail) — proves enrich did NOT swallow it and it was
    // not starved. One snapshot → nothing to prune (honest), same as the retention binary's own proof.
    let ret = wait_for_retention_report(&state, Duration::from_secs(20));
    assert_eq!(
        ret["pruned_count"], 0,
        "one snapshot → nothing to prune, but retention DID run (chained): {ret}"
    );
}

// ── SUPERSEDE — a newer trigger drops the older queued pass (slice §3.1) ───────────────────────────

#[test]
fn newer_trigger_supersedes_the_older_queued_pass() {
    let _serial = serial_guard();
    // OFF so the index's own auto pass does not race the SYNCHRONOUS `try_enrich_attempt` calls
    // below (same rationale as `cwd_resolved_manual_enrich_runs_without_identifiers`). The supersede
    // rule under test is pure `try_enrich_attempt` generation logic, driven manually here; a
    // background auto pass holding the write lock at the wrong instant would make the g2 attempt
    // Yield instead of Run (the intermittent "current pass must run" flake). The REAL index →
    // auto-trigger path is proven separately by `auto_trigger_queues_pass_and_records_report`.
    set_overrides(false);

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_no_eligible(&repo_dir);
    let indexed = index(&dispatcher, "idx", &repo_dir);
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let display = repo_dir.to_string_lossy().to_string();

    // Two triggers land back-to-back: g1 is the older queued pass, g2 the newer one.
    let g1 = state.enrich_coord().bump_generation(&repo_uid);
    let g2 = state.enrich_coord().bump_generation(&repo_uid);
    assert!(g2 > g1);

    // The OLDER pass (g1) sees a newer generation → it drops itself, never touching the DB.
    match try_enrich_attempt(&state, Path::new(&db_path), &repo_uid, &display, g1) {
        EnrichAttempt::Superseded => {}
        other => panic!(
            "the older queued pass must be superseded by the newer trigger: {}",
            attempt_label(&other)
        ),
    }

    // The CURRENT pass (g2) is not superseded → it runs (zero eligible → a clean completed outcome).
    match try_enrich_attempt(&state, Path::new(&db_path), &repo_uid, &display, g2) {
        EnrichAttempt::Ran(o) => assert_eq!(o.eligible_count, 0, "zero-eligible fixture"),
        other => panic!("the current pass must run: {}", attempt_label(&other)),
    }
}

// ── TOOLCHAIN-SKIP — real eligible edges + absent toolchain → honest skip, no error, no LSP ────────

#[test]
fn absent_toolchain_yields_an_honest_skip_not_an_error() {
    let _serial = serial_guard();
    set_overrides(true);

    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_with_eligible(&repo_dir);
    let indexed = index(&dispatcher, "idx", &repo_dir);
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();

    // Precondition: the fixture really produced TypeScript eligible edges (else the skip is vacuous).
    let ts_eligible = {
        let storage = StorageConnection::open(&db_path).unwrap();
        let snap = storage.get_latest_snapshot(&repo_uid).unwrap().unwrap();
        storage
            .query_eligible_edges(&EligibilityQuery::new(&snap.snapshot_uid))
            .unwrap()
            .into_iter()
            .filter(|e| e.language == EnrichmentLanguage::TypeScript)
            .count()
    };
    assert!(
        ts_eligible > 0,
        "fixture must produce >=1 TypeScript eligible edge for the skip to mean something (got {ts_eligible}) — adjust write_with_eligible if the extractor's eligibility contract changed"
    );

    // Toolchain forced absent via the injected predicate → the pass SKIPS TypeScript honestly and
    // NEVER opens a resolver (it returns at the plan step). Never an error.
    let outcome = run_enrich_pass(Path::new(&db_path), &repo_uid, None, &|_| false, &|| false)
        .expect("a missing toolchain is an honest skip, not a pass failure");
    assert_eq!(outcome.enriched_count, 0, "nothing runs with no toolchain");
    assert!(
        outcome.skipped.iter().any(|s| s.language == "typescript"),
        "the eligible TypeScript language is honestly skipped: {:?}",
        outcome.skipped
    );
    let ts_skip = outcome
        .skipped
        .iter()
        .find(|s| s.language == "typescript")
        .unwrap();
    assert!(
        ts_skip.reason.contains("tsserver not found")
            && ts_skip.reason.contains("npm i -g typescript"),
        "the skip reason is reader-frame with the install next-action: {}",
        ts_skip.reason
    );
    assert_eq!(
        outcome.lifecycle_state(),
        "skipped",
        "no language ran → the doctor lifecycle state is `skipped`"
    );
}

// ── OPT-OUT — RMAP_AUTO_ENRICH off gates the real trigger (slice §3.3) ─────────────────────────────

#[test]
fn opt_out_disables_the_trigger_and_reports_disabled() {
    let _serial = serial_guard();
    set_overrides(false); // enrich OFF

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_no_eligible(&repo_dir);

    let indexed = index(&dispatcher, "idx", &repo_dir);
    assert_eq!(
        indexed["enrichment"]["auto_pass"], "disabled",
        "with enrichment OFF the reply must say disabled, not queued: {indexed}"
    );

    // Give any (erroneously) spawned pass a chance to record — it must NOT (opt-out means no pass).
    thread::sleep(Duration::from_millis(200));
    assert!(
        state.last_enrichment_json().is_none(),
        "opt-out means no enrichment pass ran, so doctor has no last-enrichment to show"
    );
}

// ── CONTENTION — the pass yields while another op writes the DB, runs when idle ───────────────────

#[test]
fn enrich_yields_under_contention_then_runs_when_idle() {
    let _serial = serial_guard();
    set_overrides(true);

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_no_eligible(&repo_dir);
    let indexed = index(&dispatcher, "idx", &repo_dir);
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let display = repo_dir.to_string_lossy().to_string();
    let db_canon = Path::new(&db_path).canonicalize().unwrap();

    let gen = state.enrich_coord().bump_generation(&repo_uid);

    // Gate 1 CLOSED — a live index op is stamped in the activity registry for this DB.
    {
        let _op = state.activity().begin(
            OpKind::Index,
            display.clone(),
            Some(repo_uid.clone()),
            db_canon.clone(),
        );
        match try_enrich_attempt(&state, &db_canon, &repo_uid, &display, gen) {
            EnrichAttempt::Yielded(_) => {}
            other => panic!(
                "enrichment must YIELD while an index is active (gate 1): {}",
                attempt_label(&other)
            ),
        }
    }

    // Gate 2 CLOSED — the DB write lock is held by "another op".
    {
        let rt = state.get_or_create_db_runtime(&db_canon).unwrap();
        let _held = rt.acquire_write();
        match try_enrich_attempt(&state, &db_canon, &repo_uid, &display, gen) {
            EnrichAttempt::Yielded(_) => {}
            other => panic!(
                "enrichment must YIELD while the DB write lock is held (gate 2): {}",
                attempt_label(&other)
            ),
        }
    }

    // Both gates clear → the pass RUNS (zero eligible → clean completed, no LSP).
    match try_enrich_attempt(&state, &db_canon, &repo_uid, &display, gen) {
        EnrichAttempt::Ran(o) => assert_eq!(o.eligible_count, 0),
        other => panic!("enrichment must RUN once idle: {}", attempt_label(&other)),
    }
}

// ── CONTENTION (cancel-of-running) — an explicit index makes a RUNNING REAL pass yield the lock ────
//
// The second half of the contention coverage (slice §3.4, ratified 2026-07-06). The `enrich_yields_*`
// test above proves PRE-START yield (a queued pass defers while another op writes); THIS proves
// CANCEL-OF-RUNNING: a pass already RUNNING — holding the DB write lock, resolving a real batch —
// releases the lock to an explicit index, requeues, and is superseded by the fresh index.
//
// review-0 item 3 required this drive the REAL pass, not a hand-rolled lock+flag loop. It does: the
// REAL `run_auto_enrich` requeue loop runs the REAL `try_enrich_attempt` → `run_enrich_pass` →
// pipeline → resolver batch loop against a REAL indexed fixture. The ONE hermetic stand-in is the
// resolver (`ParkingResolver` via `set_test_registry_builder`) — the three shipped resolvers spawn LSP
// subprocesses that cannot run in a unit/integration test. That the real resolvers poll the cancel
// flag at their own batch boundaries is proven by `resolve_batch_stops_within_the_batch_on_cancel`
// (enrichment) + `run_cancellable_stops_within_the_batch_on_mid_cancel` (pipeline); the live
// self-dogfood (operator recipe) exercises real rust-analyzer end-to-end.

/// A hermetic stand-in for a real LSP resolver (no rust-analyzer/tsserver subprocess): it resolves the
/// FIRST eligible edge, then PARKS polling the cancel flag until an explicit index/refresh latches it
/// (bounded by a safety timeout so a bug fails fast instead of hanging). This gives the test a
/// deterministic "running" window to drive the REAL daemon pass into a batch-boundary yield. Shared
/// atomics report what it observed: `processed` (edges completed) and `observed_cancel` (saw the latch).
struct ParkingResolver {
    lang: EnrichmentLanguage,
    processed: Arc<AtomicUsize>,
    observed_cancel: Arc<AtomicBool>,
}

impl ReceiverTypeResolver for ParkingResolver {
    fn language(&self) -> EnrichmentLanguage {
        self.lang
    }
    fn resolve_batch(
        &self,
        _repo_root: &Path,
        edges: &[EligibleEdge],
        _progress: Option<&dyn ResolverProgress>,
        cancel: Option<&dyn Fn() -> bool>,
    ) -> Vec<ReceiverTypeResult> {
        let cancelled = || cancel.is_some_and(|c| c());
        let mut out = Vec::new();
        for (i, e) in edges.iter().enumerate() {
            // Before every edge after the first, PARK until the explicit index latches the cancel
            // flag — the deterministic running window (bounded so a broken latch fails fast).
            if i > 0 {
                let start = Instant::now();
                while !cancelled() && start.elapsed() < Duration::from_secs(20) {
                    thread::sleep(Duration::from_millis(5));
                }
            }
            if cancelled() {
                self.observed_cancel.store(true, Ordering::SeqCst);
                break; // stop WITHIN the batch — the tail is abandoned, never fabricated
            }
            out.push(ReceiverTypeResult::success(
                e.edge_uid.clone(),
                "SomeType".to_string(),
                Some("SomeType".to_string()),
                false,
            ));
            self.processed.fetch_add(1, Ordering::SeqCst);
        }
        out
    }
    fn initialize(&mut self, _repo_root: &Path) -> Result<(), ResolverError> {
        Ok(())
    }
    fn shutdown(&mut self) {}
}

#[test]
fn explicit_index_makes_a_running_enrichment_yield_the_write_lock_and_proceeds() {
    let _serial = serial_guard();
    // enrich + retention OFF: we drive the ONE real pass ourselves via `run_auto_enrich`, so no
    // background auto pass competes for the write lock / run slot. The explicit index below STILL
    // latches the running pass — `request_yield_for_db` is unconditional (not gated on the enrich
    // setting), which is exactly the invariant that lets an explicit write win a running enrichment.
    set_overrides(false);

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_multi_eligible(&repo_dir);
    let indexed = index(&dispatcher, "idx1", &repo_dir);
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let display = repo_dir.to_string_lossy().to_string();

    // Precondition: the fixture really produced >= 2 TypeScript eligible edges (else there is no
    // "stopped partway" and no parking window).
    let ts_eligible = {
        let storage = StorageConnection::open(&db_path).unwrap();
        let snap = storage.get_latest_snapshot(&repo_uid).unwrap().unwrap();
        storage
            .query_eligible_edges(&EligibilityQuery::new(&snap.snapshot_uid))
            .unwrap()
            .into_iter()
            .filter(|e| e.language == EnrichmentLanguage::TypeScript)
            .count()
    };
    assert!(
        ts_eligible >= 2,
        "fixture must yield >= 2 eligible TS edges for the parking window (got {ts_eligible}) — adjust write_multi_eligible if the extractor's eligibility contract changed"
    );

    // Install the fake cancellable resolver for the REAL pass. A drop-guard clears the process-global
    // seam on ANY exit (incl. panic) BEFORE the serial lock releases, so no other test inherits it.
    let processed = Arc::new(AtomicUsize::new(0));
    let observed_cancel = Arc::new(AtomicBool::new(false));
    {
        let processed = Arc::clone(&processed);
        let observed_cancel = Arc::clone(&observed_cancel);
        set_test_registry_builder(move |to_run: &[EnrichmentLanguage]| {
            let mut reg = ResolverRegistry::new();
            for &lang in to_run {
                reg.register(Box::new(ParkingResolver {
                    lang,
                    processed: Arc::clone(&processed),
                    observed_cancel: Arc::clone(&observed_cancel),
                }));
            }
            reg
        });
    }
    struct SeamClear;
    impl Drop for SeamClear {
        fn drop(&mut self) {
            clear_test_registry_builder();
        }
    }
    let _seam_clear = SeamClear;

    // Drive the REAL requeue loop (`run_auto_enrich`) in a thread with a captured generation. Its
    // first attempt runs the REAL pass, which holds the DB write lock and parks in the fake resolver.
    let my_gen = state.enrich_coord().bump_generation(&repo_uid);
    let pass = {
        let state = Arc::clone(&state);
        let db_path = db_path.clone();
        let repo_uid = repo_uid.clone();
        let display = display.clone();
        thread::spawn(move || {
            run_auto_enrich(&state, Path::new(&db_path), &repo_uid, &display, my_gen);
        })
    };

    // Wait until the REAL pass is running the resolver (processed the first edge → now parked holding
    // the write lock).
    let start = Instant::now();
    while processed.load(Ordering::SeqCst) == 0 {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the real enrichment pass never started resolving"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        state.enrich_coord().activity_state(),
        "running",
        "doctor sees the pass holding the write lock as running"
    );

    // A newer trigger lands — exactly what the incoming index does on completion with enrich ON
    // (`spawn_auto_enrich` bumps the generation). Reproduced here because index #2 runs enrich-OFF for
    // determinism; the requeued pass will see this newer generation and supersede.
    let _newer = state.enrich_coord().bump_generation(&repo_uid);

    // Dispatch the explicit index for the SAME repo. `handle_index` latches the running pass's cancel
    // flag (`request_yield_for_db`) BEFORE it blocks on `acquire_write`; the pass yields the lock at
    // its next batch boundary and this dispatch proceeds. This call BLOCKS until the pass yields.
    let indexed2 = index(&dispatcher, "idx2", &repo_dir);
    assert_eq!(
        indexed2["repo_uid"].as_str().unwrap(),
        repo_uid,
        "the explicit index proceeded after the running pass yielded the write lock"
    );

    // The REAL requeue loop returned: Yielded → requeue → Superseded by the newer generation.
    pass.join().unwrap();

    // The running pass saw the yield signal at a batch boundary and stopped WITHIN the batch.
    assert!(
        observed_cancel.load(Ordering::SeqCst),
        "the running enrichment observed the explicit index's yield signal at a batch boundary"
    );
    let done = processed.load(Ordering::SeqCst);
    assert!(
        done >= 1 && done < ts_eligible,
        "the pass stopped WITHIN the batch (processed {done}/{ts_eligible})"
    );

    // Requeue → supersede: the yielded pass requeued, saw the newer trigger, and was superseded — so
    // it recorded NO completed report (the fresh index owns enrichment now). This is the daemon-level
    // requeue/supersede-after-cancellation behavior review-0 item 3 required.
    assert!(
        state.last_enrichment_json().is_none(),
        "a yielded-then-superseded pass records no completed enrichment report"
    );
}

// ── CONTENTION (window race) — a yield latched in the acquire→register window makes the pass yield ─
//
// review-1's blocking defect: `try_enrich_attempt` takes the DB write lock BEFORE it registers its
// cancel flag (with run-slot/generation/repo-load/refresh-lock/activity-stamp work in between). An
// explicit index/refresh whose `request_yield_for_db` lands in THAT window found no flag to latch — a
// lost no-op — so the pass ran to completion while the explicit write blocked, violating "explicit
// writes always win". This proves the fix END-TO-END through the REAL `try_enrich_attempt`: a yield
// requested while no flag is registered is recorded PENDING and ADOPTED at registration, so the real
// pass YIELDS (which `run_auto_enrich` requeues) instead of Running to completion.
//
// Deterministic, no timing race: "request_yield_for_db with no flag registered" IS the window state
// (the pass has not yet registered), and `register_running` inside `try_enrich_attempt` adopts it. The
// zero-eligible fixture makes the pass's own cancel flag — pre-cancelled by the adopt — map the clean
// early-return to `Yielded` via `classify_completed_attempt`, no resolver needed. Before the fix this
// returned `Ran` (fresh un-cancelled flag), which is exactly the defect.
#[test]
fn yield_latched_in_the_acquire_to_register_window_makes_the_real_pass_yield() {
    let _serial = serial_guard();
    set_overrides(false); // we drive the ONE pass ourselves; no background auto pass competes

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_no_eligible(&repo_dir);
    let indexed = index(&dispatcher, "idx", &repo_dir);
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let display = repo_dir.to_string_lossy().to_string();
    let db_canon = Path::new(&db_path).canonicalize().unwrap();

    let gen = state.enrich_coord().bump_generation(&repo_uid);

    // Reproduce the window: an explicit write requested a yield for this DB while the pass had not yet
    // registered its flag (no registered flag == exactly the acquire→register window state). Before the
    // fix this was a lost no-op; the fix records it as a pending marker.
    state.enrich_coord().request_yield_for_db(&db_canon);

    // The REAL pass runs: `try_enrich_attempt` acquires the write lock, registers (ADOPTING the pending
    // yield → its flag starts cancelled), and maps to Yielded — NOT Ran-to-completion.
    match try_enrich_attempt(&state, &db_canon, &repo_uid, &display, gen) {
        EnrichAttempt::Yielded(_) => {}
        other => panic!(
            "a yield latched in the acquire→register window must make the real pass yield, not run: {}",
            attempt_label(&other)
        ),
    }

    // A yielded pass records no completed report (the requeue/supersede path owns the outcome).
    assert!(
        state.last_enrichment_json().is_none(),
        "a pass that yielded to a window-latched explicit write records no completed report"
    );

    // And the marker was CONSUMED by the adopt: a subsequent pass (nothing new requested) runs cleanly,
    // proving the pending signal is one-shot, not a sticky cancel.
    match try_enrich_attempt(&state, &db_canon, &repo_uid, &display, gen) {
        EnrichAttempt::Ran(o) => {
            assert_eq!(o.eligible_count, 0, "zero-eligible fixture runs clean")
        }
        other => panic!(
            "after the pending yield was adopted once, the next pass must run, not yield again: {}",
            attempt_label(&other)
        ),
    }
}

// ── ERGONOMICS / REG-1 — `rmap enrich` resolves the repo from cwd (slice §3.6) ────────────────────

#[test]
fn cwd_resolved_manual_enrich_runs_without_identifiers() {
    let _serial = serial_guard();
    set_overrides(false); // OFF so the index's own auto pass does not race this manual one

    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_no_eligible(&repo_dir);
    index(&dispatcher, "idx", &repo_dir);
    let repo_ref = repo_dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // The REG-1 form: `repo` only — NO db_path / repo_uid. The daemon resolves it via the registry.
    let result = expect_success(run(
        &dispatcher,
        "enr",
        "enrich",
        json!({ "repo": repo_ref }),
    ));
    assert_eq!(
        result["command"], "enrich",
        "the cwd-resolved manual enrich reaches handle_enrich and completes: {result}"
    );
    assert_eq!(
        result["eligible_count"], 0,
        "zero-eligible fixture → nothing to resolve (no LSP): {result}"
    );
}
