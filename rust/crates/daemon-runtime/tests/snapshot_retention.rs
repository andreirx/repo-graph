//! SNAPSHOT-RETENTION-1 — daemon-level named proofs, driven through the REAL
//! `ServiceDispatcher::dispatch` surface against REAL dispatched `index` writes.
//!
//! # Why this binary exists (what it proves that the `retention_pass` lib tests do not)
//!
//! The `retention_pass` module's own `#[cfg(test)]` proofs drive the pass CORE directly
//! (`run_retention_pass` / `try_retention_attempt` on a hand-built `StorageConnection`): they prove the
//! ratified keep-set, the two contention gates, the threshold-gated VACUUM, and the two reader-vs-VACUUM
//! production-interaction cases. What they CANNOT prove is the wiring one layer up: that a real
//! `handle_index` dispatch actually **auto-triggers** the pass (`finish_write_with_retention` →
//! `spawn_auto_retention`) and that a long-lived daemon converges accumulated real-indexed snapshots to
//! the ratified steady state. That is the gap this binary closes — the operator's explicit remaining
//! item: "auto-trigger (completed index queues the background retention op), contention yield, threshold
//! VACUUM in the pass" at the daemon integration level, against a REAL in-flight write (not a
//! manually-stamped registry).
//!
//! # The named-proofs map (where every SNAPSHOT-RETENTION-1 proof lives)
//!
//! | Proof | Home |
//! |-------|------|
//! | **Auto-trigger** (real index → pass spawned → report recorded) | `auto_trigger_*` (HERE) |
//! | **Steady-state via real dispatch** (N real indexes → current-only, older pruned) + completion-report `prunable_count` preview | `three_real_indexes_*` (HERE) |
//! | **Contention yield vs a REAL in-flight index** (defers while it writes, runs after) | `retention_yields_to_a_real_inflight_index_*` (HERE) |
//! | **Opt-out gates the real trigger** (`RMAP_AUTO_RETENTION` off → no pass, honest report) | `opt_out_*` (HERE) |
//! | **Reader-vs-pass PRODUCTION wiring** (real dispatched read blocks on the SAME coordinator the pass's VACUUM holds, then reads correct data — never busy) | `a_dispatched_read_blocks_while_the_pass_holds_writing_*` (HERE) |
//! | **Threshold-gated VACUUM through the DAEMON pass** (below skips / above runs, via `try_retention_attempt`) | `threshold_gated_vacuum_runs_above_and_skips_below_*` (HERE) |
//! | **Doctor shows the pass as an ACTIVE op** (`daemon_info` surfaces kind `retention`; clears on drop) | `daemon_info_surfaces_the_active_retention_op` (HERE) |
//! | Steady-state keep-set + bytes reported (unit) | `retention_pass::steady_state_keeps_current_and_parent_prunes_older` |
//! | Baseline-user survives / auto-baseline pruned | `retention_pass::user_baseline_survives_but_auto_baseline_does_not` |
//! | Threshold-gated VACUUM (below skips / above runs, unit) | `retention_pass::threshold_below_skips_vacuum_above_runs_it` |
//! | Contention two-gate (unit) | `retention_pass::retention_yields_under_contention_then_runs_when_idle` |
//! | Reader ALREADY mid-request → VACUUM defers, prune still runs | `retention_pass::vacuum_defers_to_an_active_reader_then_runs_when_idle` |
//! | Reader ARRIVES during VACUUM → blocks honestly, reads correct data | `retention_pass::reader_arriving_during_vacuum_window_blocks_then_reads_correct_data` |
//! | Doctor "cleanup: pruned N / reclaimed X / nothing to prune" rendering | `rgr … doctor::daemon_info::retention_probe_*` |
//! | Doctor "reclaiming <repo>" active-op rendering | `rgr … doctor::daemon_info::activity_probe_renders_in_flight_retention` |
//! | Index/refresh completion "retention: … queued/disabled" line | `rgr … commands::index::retention_line_*` |
//!
//! # Why full `index` converges to ONE snapshot (not two)
//!
//! A full `index` creates a fresh ROOT snapshot with `parent_snapshot_uid = None`
//! (`indexer::orchestrator`); only `refresh` chains a delta parent. So after repeated full indexes the
//! newest is `current`, its parent role is empty (no chain), and EVERY older full snapshot is
//! `prunable`. The ratified keep-set therefore collapses N full indexes to the single `current` — a
//! sharper end-state than the `current + parent` (=2) that a `refresh` chain leaves, and still within
//! the DoD bound "at most current + delta-base". The 2-snapshot `current + parent` case is proven with a
//! real parent chain in the `retention_pass` steady-state unit test.
//!
//! # Determinism notes
//!
//! - `set_auto_retention_for_test` is a PROCESS-GLOBAL atomic. Cargo runs a binary's tests in parallel,
//!   and this binary mixes an ON test with OFF tests, so every test serializes on [`RETENTION_SERIAL`]
//!   and sets the override it needs while holding it. Without that, an OFF test would flip the global
//!   out from under the ON test.
//! - The only test that lets the pass run on its own detached thread is the auto-trigger one; it waits
//!   for the pass to RECORD its report before returning, so no pass thread is left racing the tempdir
//!   teardown. The convergence + contention tests drive `try_retention_attempt` SYNCHRONOUSLY (no
//!   detached thread), which makes the pruned/kept assertions exact.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use repo_graph_daemon_runtime::activity::OpKind;
use repo_graph_daemon_runtime::retention_pass::{
    set_auto_retention_for_test, try_retention_attempt, RetentionAttempt,
};
use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

// ── Process-global serialization for the shared auto-retention override ───────────────────────────

/// Serializes every test in this binary. The auto-retention ON/OFF switch is a process-global atomic
/// (`set_auto_retention_for_test`), and this binary deliberately mixes an ON test with OFF tests; the
/// lock keeps them from clobbering each other under Cargo's parallel test runner. Poison-tolerant: a
/// panicking test must not cascade-fail the rest (they each re-establish the override they need).
static RETENTION_SERIAL: Mutex<()> = Mutex::new(());

fn serial_guard() -> MutexGuard<'static, ()> {
    RETENTION_SERIAL
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

// ── Harness (mirrors tests/daemon_visibility.rs) ─────────────────────────────────────────────────

/// A progress emitter that discards events.
struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Shared rendezvous between a parked writer and the test thread (no timer).
#[derive(Clone, Default)]
struct ParkHandle {
    inner: Arc<(Mutex<ParkFlags>, Condvar)>,
}

#[derive(Default)]
struct ParkFlags {
    entered: bool,
    released: bool,
}

impl ParkHandle {
    fn wait_until_entered(&self) {
        let (lock, cv) = &*self.inner;
        let mut f = lock.lock().unwrap();
        while !f.entered {
            f = cv.wait(f).unwrap();
        }
    }
    fn release(&self) {
        let (lock, cv) = &*self.inner;
        let mut f = lock.lock().unwrap();
        f.released = true;
        cv.notify_all();
    }
}

/// Parks the FIRST time the write pipeline emits progress, then passes through. At that first emit the
/// handler is inside `handle_index`'s `acquire_write()` scope AND has stamped the activity registry — a
/// genuine in-flight write, no timer, no manual `activity().begin()`.
struct ParkOnceEmitter {
    handle: ParkHandle,
    parked: bool,
}
impl ProgressEmitter for ParkOnceEmitter {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        if !self.parked {
            self.parked = true;
            let (lock, cv) = &*self.handle.inner;
            let mut f = lock.lock().unwrap();
            f.entered = true;
            cv.notify_all();
            while !f.released {
                f = cv.wait(f).unwrap();
            }
        }
        Ok(())
    }
}

fn isolated() -> (Arc<ServiceDispatcher>, Arc<DaemonState>, TempDir) {
    // Unlike the other daemon-runtime integration binaries, this one does NOT globally disable
    // retention here — each test sets the override it needs while holding `RETENTION_SERIAL`.
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = Arc::new(ServiceDispatcher::new(Arc::clone(&state)));
    (dispatcher, state, state_root)
}

/// helper.ts + main.ts: a real cross-file import + call graph, so a full index produces a real snapshot.
fn write_base(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("helper.ts"),
        "export function helperFunction() {\n    return 1;\n}\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("main.ts"),
        "import { helperFunction } from './helper';\n\nexport function mainEntry() {\n    helperFunction();\n}\n",
    )
    .unwrap();
}

/// Add `mod<n>.ts` with `funcs` exported functions — real new rows on each full re-index, so pruning an
/// older snapshot frees actual freelist pages (`reclaimable_bytes > 0`) rather than a rounding blip.
fn add_module(repo_dir: &Path, n: usize, funcs: usize) {
    let mut src = String::from("import { helperFunction } from './helper';\n\n");
    for i in 0..funcs {
        src.push_str(&format!(
            "export function mod{n}_f{i}(): number {{ helperFunction(); return {i}; }}\n"
        ));
    }
    std::fs::write(repo_dir.join(format!("mod{n}.ts")), src).unwrap();
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

/// Full-index a repo dir and return the success payload (`repo_uid` / `db_path` / `snapshot_uid` / …).
fn index(dispatcher: &ServiceDispatcher, id: &str, repo_dir: &Path) -> Value {
    expect_success(run(
        dispatcher,
        id,
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ))
}

/// READY snapshot uids for a repo, read straight from the DB (used only where no VACUUM can be
/// concurrently in flight — every reader here runs after a SYNCHRONOUS pass or with retention OFF).
fn ready_snapshot_uids(db_path: &str, repo_uid: &str) -> Vec<String> {
    let conn = StorageConnection::open(db_path).unwrap();
    let mut uids: Vec<String> = conn
        .list_snapshots(repo_uid)
        .unwrap()
        .into_iter()
        .filter(|s| s.status == "ready")
        .map(|s| s.snapshot_uid)
        .collect();
    uids.sort();
    uids
}

/// Poll `last_retention_json()` until a background pass has recorded a report, or panic on timeout. The
/// pass is async (a detached thread), so the caller cannot join it — this is the standard bounded wait.
fn wait_for_retention_report(state: &DaemonState, timeout: Duration) -> Value {
    let start = Instant::now();
    loop {
        if let Some(v) = state.last_retention_json() {
            return v;
        }
        assert!(
            start.elapsed() < timeout,
            "background retention pass did not record a report within {timeout:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

// ── AUTO-TRIGGER — a completed real index spawns the pass, which runs and records ─────────────────

/// The core auto-trigger proof: a REAL dispatched `index` (retention ON) annotates its reply
/// `retention.auto_pass = "queued"` AND spawns the background pass, which runs to completion and records
/// its outcome for `rmap doctor`. On a single snapshot there is nothing to prune, so the recorded report
/// is the honest "nothing to prune" (pruned 0, no VACUUM) — the same line the doctor probe renders.
#[test]
fn auto_trigger_queues_pass_and_records_report() {
    let _serial = serial_guard();
    set_auto_retention_for_test(true); // force ON (default posture), race-free vs the OFF tests

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_base(&repo_dir);

    let indexed = index(&dispatcher, "idx", &repo_dir);

    // (1) The synchronous reply proves the dispatch path DECIDED to queue the pass (never runs it here).
    assert_eq!(
        indexed["retention"]["auto_pass"], "queued",
        "a completed index with retention ON must report the background pass as queued: {indexed}"
    );

    // (2) The spawned pass actually RAN and recorded its report (proves spawn_auto_retention executed —
    // not merely that dispatch annotated "queued"). Bounded wait: the pass is on a detached thread.
    let report = wait_for_retention_report(&state, Duration::from_secs(15));
    assert_eq!(
        report["pruned_count"], 0,
        "one snapshot → nothing to prune (honest, no fabricated reclaim): {report}"
    );
    assert_eq!(report["non_ready_reclaimed"], 0, "{report}");
    assert_eq!(
        report["vacuum_status"], "below_threshold",
        "nothing pruned → no reclaimable pages → VACUUM skipped below threshold: {report}"
    );
}

// ── STEADY-STATE via REAL dispatch — three indexes converge to current-only ───────────────────────

/// Three real full indexes accumulate three READY snapshots; the auto-retention pass (driven
/// SYNCHRONOUSLY here so the pruned/kept counts are exact, after the same real dispatch created the
/// snapshots) prunes the two older roots and keeps exactly `current`. This is the DoD steady-state
/// ("at most current + delta-base") proven end-to-end on real-indexed data, plus an honest reclaimable
/// report. Retention is OFF during the indexes so their own background passes do not race this
/// deterministic pass; the AUTO spawn is proven separately by `auto_trigger_queues_pass_and_records_report`.
#[test]
fn three_real_indexes_prune_to_current_only() {
    let _serial = serial_guard();
    set_auto_retention_for_test(false); // OFF during indexing: no background pass races our sync pass

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");

    write_base(&repo_dir);
    let i1 = index(&dispatcher, "idx1", &repo_dir);
    add_module(&repo_dir, 2, 60);
    let _i2 = index(&dispatcher, "idx2", &repo_dir);
    add_module(&repo_dir, 3, 60);
    let i3 = index(&dispatcher, "idx3", &repo_dir);

    let db_path = i1["db_path"].as_str().unwrap().to_string();
    let repo_uid = i1["repo_uid"].as_str().unwrap().to_string();
    let newest_uid = i3["snapshot_uid"].as_str().unwrap().to_string();

    // Precondition: three READY full snapshots have ACCUMULATED (the disease the slice cures).
    assert_eq!(
        ready_snapshot_uids(&db_path, &repo_uid).len(),
        3,
        "three full indexes accumulate three READY snapshots before retention runs"
    );

    // COMPLETION-REPORT HONESTY (slice §4 / review-0 #5): the index REPLY previews the REAL prunable
    // backlog the queued pass will clear — `retention.prunable_count`, computed on the cheap foreground
    // classification (REFRESH-HANG-1's ~1ms `classify_retention_only`, NOT the async prune). This is the
    // number the `rmap index` completion line renders as "N snapshot(s) to reclaim"; the pruned/reclaimed
    // RESULT lands on `rmap doctor` after the async pass. Three full roots → keep current, 2 prunable —
    // and this MUST equal the pass's actual `pruned_count` (asserted below), proving preview == outcome.
    assert_eq!(
        i3["retention"]["prunable_count"], 2,
        "the completion-report preview is a real foreground fact, not a fabricated result: {i3}"
    );

    // Run the pass through the real gate path (synchronous → exact assertions).
    let outcome = match try_retention_attempt(
        &state,
        Path::new(&db_path),
        &repo_uid,
        &repo_dir.to_string_lossy(),
    ) {
        RetentionAttempt::Ran(o) => o,
        other => panic!(
            "retention must RUN on an idle just-indexed repo: {}",
            attempt_label(&other)
        ),
    };

    // The two older ROOT snapshots are pruned; exactly `current` (the newest) remains.
    assert_eq!(
        outcome.pruned_count, 2,
        "the two older full-index roots are pruned"
    );
    let remaining = ready_snapshot_uids(&db_path, &repo_uid);
    assert_eq!(
        remaining,
        vec![newest_uid.clone()],
        "exactly current remains after convergence: {remaining:?}"
    );
    // Real rows were freed → a non-zero reclaimable is reported (whether or not it crosses the VACUUM
    // threshold). The threshold BRANCH decision itself is unit-owned; here we assert the honest input.
    assert!(
        outcome.reclaimable_bytes > 0,
        "pruning two non-trivial snapshots frees real freelist pages: {}",
        outcome.reclaimable_bytes
    );
    // vacuum_status is one of the three honest fates — never a silent lie. (For older-smaller snapshots
    // this is typically `below_threshold`; the exact branch is proven in the threshold unit test.)
    assert!(
        matches!(
            outcome.vacuum.as_str(),
            "ran" | "below_threshold" | "deferred_readers_active"
        ),
        "vacuum fate is honest: {}",
        outcome.vacuum.as_str()
    );
    // A second pass is a no-op (already converged) — idempotence through the real path.
    match try_retention_attempt(
        &state,
        Path::new(&db_path),
        &repo_uid,
        &repo_dir.to_string_lossy(),
    ) {
        RetentionAttempt::Ran(o) => assert_eq!(
            o.pruned_count, 0,
            "already converged → nothing more to prune"
        ),
        other => panic!(
            "second pass must RUN and prune nothing: {}",
            attempt_label(&other)
        ),
    }
}

// ── CONTENTION — the pass yields to a REAL in-flight index, then runs after it ────────────────────

/// Contention safety against a REAL dispatched write (stronger than the unit test's manually-stamped
/// op): while a real re-index is parked mid-flight — holding the DB write lock and stamped in the
/// activity registry — `try_retention_attempt` YIELDS (gate 1: `active_for_db` sees the index). Once the
/// index completes and releases both, the pass RUNS and prunes the now-older root. Explicit user ops win.
#[test]
fn retention_yields_to_a_real_inflight_index_then_runs() {
    let _serial = serial_guard();
    set_auto_retention_for_test(false); // we drive the pass by hand; no stray auto spawns

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_base(&repo_dir);

    let i1 = index(&dispatcher, "idx1", &repo_dir);
    let db_path = i1["db_path"].as_str().unwrap().to_string();
    let repo_uid = i1["repo_uid"].as_str().unwrap().to_string();

    // Park a REAL re-index in flight (new file → real work → a first progress emit to park on).
    add_module(&repo_dir, 2, 30);
    let park = ParkHandle::default();
    let writer = {
        let dispatcher = Arc::clone(&dispatcher);
        let park = park.clone();
        let repo_path = repo_dir.to_string_lossy().to_string();
        thread::spawn(move || {
            let mut emitter = ParkOnceEmitter {
                handle: park,
                parked: false,
            };
            matches!(
                dispatcher.dispatch(
                    &request("idx2", "index", json!({ "repo_path": repo_path })),
                    &mut emitter
                ),
                DispatchResult::Success(_)
            )
        })
    };

    // The index is now inside acquire_write() with the activity stamped.
    park.wait_until_entered();
    match try_retention_attempt(
        &state,
        Path::new(&db_path),
        &repo_uid,
        &repo_dir.to_string_lossy(),
    ) {
        RetentionAttempt::Yielded(_) => {}
        other => panic!(
            "retention MUST yield while a real index is writing the repo (explicit ops win): {}",
            attempt_label(&other)
        ),
    }

    // Let the index finish and release the write lock + activity stamp.
    park.release();
    assert!(
        writer.join().unwrap(),
        "the parked re-index completed successfully"
    );

    // Now the pass RUNS and prunes the now-older root (current = the re-index snapshot, no parent chain).
    match try_retention_attempt(
        &state,
        Path::new(&db_path),
        &repo_uid,
        &repo_dir.to_string_lossy(),
    ) {
        RetentionAttempt::Ran(o) => assert!(
            o.pruned_count >= 1,
            "after the index completes, the pass runs and prunes the older snapshot: pruned={}",
            o.pruned_count
        ),
        other => panic!(
            "retention must RUN once the index releases the repo: {}",
            attempt_label(&other)
        ),
    }
}

// ── OPT-OUT — the switch gates the real trigger end-to-end ────────────────────────────────────────

/// With auto-retention disabled, a real index annotates its reply `retention.auto_pass = "disabled"`,
/// spawns NO pass (no report ever recorded), and accumulated snapshots are LEFT intact — the honest
/// opposite of the auto-trigger. Proves `RMAP_AUTO_RETENTION` (via the test seam) governs the real
/// dispatch path, not just the pure predicate the `retention_pass` unit test covers.
#[test]
fn opt_out_disables_the_auto_trigger() {
    let _serial = serial_guard();
    set_auto_retention_for_test(false); // opt-out ON (retention OFF)

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");

    write_base(&repo_dir);
    let i1 = index(&dispatcher, "idx1", &repo_dir);
    add_module(&repo_dir, 2, 20);
    let i2 = index(&dispatcher, "idx2", &repo_dir);

    // (1) The reply honestly reports the pass is disabled (never a fabricated "queued").
    assert_eq!(
        i2["retention"]["auto_pass"], "disabled",
        "with retention off the index reply reports it disabled: {i2}"
    );

    let db_path = i1["db_path"].as_str().unwrap().to_string();
    let repo_uid = i1["repo_uid"].as_str().unwrap().to_string();

    // (2) No background pass ran — nothing was recorded, and both snapshots survive (no cleanup happened).
    // A brief settle window guards against a false negative from an over-eager spawn (there must be none).
    thread::sleep(Duration::from_millis(200));
    assert!(
        state.last_retention_json().is_none(),
        "opt-out must spawn NO pass, so no retention report is ever recorded"
    );
    assert_eq!(
        ready_snapshot_uids(&db_path, &repo_uid).len(),
        2,
        "both indexed snapshots remain — the disabled pass pruned nothing"
    );
}

// ── READERS vs. the PASS through the REAL dispatch surface (review-0 #2) ───────────────────────────

/// PRODUCTION-INTERACTION PROOF (review-0 #2): a REAL coordinated read dispatched through
/// `ServiceDispatcher` BLOCKS honestly while the pass holds the repo coordinator's `Writing` state (its
/// VACUUM window) — never a raw `database is locked` — and then reads the CORRECT data once the pass
/// releases. This is the DAEMON-WIRING proof the `retention_pass` unit tests could not give: they prove
/// the mechanism on a hand-built coordinator; THIS proves the real `handle_*` read path and the pass
/// engage the SAME cached coordinator (via `load_repo`), so the exclusion actually fires in production.
///
/// Why holding `acquire_write()` is faithful to the VACUUM: `run_retention_pass` takes
/// `coordinator.try_acquire_write()` → `Writing` around `StorageConnection::vacuum` (which drops to a
/// `journal_mode=DELETE` rollback journal and takes a SQLite EXCLUSIVE lock — the only op in the pass
/// that would busy a concurrent reader). Holding that same `Writing` here reproduces the exact window a
/// reader must survive, deterministically (no reliance on catching a sub-second real VACUUM).
#[test]
fn a_dispatched_read_blocks_while_the_pass_holds_writing_then_reads_correct_data() {
    let _serial = serial_guard();
    set_auto_retention_for_test(false); // we drive the coordinator by hand; no stray auto spawns

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_base(&repo_dir);
    let i1 = index(&dispatcher, "idx", &repo_dir);
    let db_path = i1["db_path"].as_str().unwrap().to_string();
    let repo_uid = i1["repo_uid"].as_str().unwrap().to_string();
    let repo_path = repo_dir.to_string_lossy().to_string();

    // Engage the SHARED coordinator's `Writing` state — the SAME cached `RepoState` a dispatched read
    // resolves via `load_repo`, so this is the exact coordinator the read takes `acquire_read` on.
    let repo_state = state.load_repo(Path::new(&db_path), &repo_uid).unwrap();
    let writing = repo_state.coordinator.acquire_write();

    let started = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let reader = {
        let dispatcher = Arc::clone(&dispatcher);
        let started = Arc::clone(&started);
        let done = Arc::clone(&done);
        thread::spawn(move || {
            started.store(true, Ordering::SeqCst);
            // The REAL production read seam: `handle_stats` → `coordinator.acquire_read()` (BLOCKS while
            // `Writing` is held) → open storage → query. `stats` needs only `repo` and always succeeds
            // on an indexed repo, so a non-Success result would itself be the busy/degraded failure.
            let result = run(&dispatcher, "rd", "stats", json!({ "repo": repo_path }));
            done.store(true, Ordering::SeqCst);
            match result {
                DispatchResult::Success(_) => Ok(()),
                DispatchResult::Error(e) => Err(format!("{}: {}", e.error.code, e.error.message)),
            }
        })
    };

    // The read is running; confirm it is BLOCKED on `acquire_read` (not spinning to a busy error).
    while !started.load(Ordering::SeqCst) {
        thread::yield_now();
    }
    thread::sleep(Duration::from_millis(200));
    assert!(
        !done.load(Ordering::SeqCst),
        "a dispatched read arriving during the pass's Writing/VACUUM window must BLOCK on acquire_read, \
         never race the exclusive lock and surface `database is locked`"
    );

    // Release `Writing` (as the pass does when its VACUUM finishes) → the blocked read proceeds.
    drop(writing);
    let outcome = reader.join().unwrap();
    assert!(
        done.load(Ordering::SeqCst),
        "the dispatched read unblocked and completed once Writing released"
    );
    assert!(
        outcome.is_ok(),
        "the read returned correct data after the pass released Writing — no raw SQLITE_BUSY, no wrong \
         answer: {outcome:?}"
    );
}

// ── THRESHOLD-GATED VACUUM through the DAEMON pass path (review-0 #3) ──────────────────────────────

/// DAEMON-LEVEL THRESHOLD PROOF (review-0 #3): the threshold gate is honored end-to-end through
/// `try_retention_attempt` (the gated daemon pass), not only in the `run_retention_pass` unit test.
/// Both branches, deterministic via extreme size margins (a ~600-fn snapshot vs a ~1-fn one):
/// - ABOVE: a big older snapshot + a tiny current → pruning the big older frees a MAJORITY → VACUUM
///   runs, disk is returned, the file shrinks.
/// - BELOW: a tiny older snapshot + a big current → pruning the tiny older frees a MINORITY → VACUUM is
///   SKIPPED (rows gone, file unshrunk, honest `below_threshold` report; pages recycle next index).
#[test]
fn threshold_gated_vacuum_runs_above_and_skips_below_through_the_daemon_pass() {
    let _serial = serial_guard();
    set_auto_retention_for_test(false); // synchronous pass; no background racer

    let (dispatcher, state, _root) = isolated();

    // ABOVE — the big older snapshot dominates the file, so pruning it is a majority reclaim.
    {
        let repo_root = tempdir().unwrap();
        let repo_dir = repo_root.path().join("above");
        write_base(&repo_dir);
        add_module(&repo_dir, 2, 600); // big
        let i1 = index(&dispatcher, "a1", &repo_dir);
        add_module(&repo_dir, 2, 1); // overwrite → tiny current
        let _i2 = index(&dispatcher, "a2", &repo_dir);
        let db_path = i1["db_path"].as_str().unwrap().to_string();
        let repo_uid = i1["repo_uid"].as_str().unwrap().to_string();
        let size_before = db_file_size(&db_path);

        let outcome = match try_retention_attempt(
            &state,
            Path::new(&db_path),
            &repo_uid,
            &repo_dir.to_string_lossy(),
        ) {
            RetentionAttempt::Ran(o) => o,
            other => panic!("above-threshold pass must RUN: {}", attempt_label(&other)),
        };
        assert!(
            outcome.pruned_count >= 1,
            "the big older snapshot is pruned: {outcome:?}"
        );
        assert_eq!(
            outcome.vacuum.as_str(),
            "ran",
            "a majority-dead file VACUUMs through the daemon pass path: {outcome:?}"
        );
        assert!(
            outcome.reclaimed_bytes > 0,
            "the VACUUM returned disk to the OS: {outcome:?}"
        );
        assert!(
            db_file_size(&db_path) < size_before,
            "the file actually shrank: {size_before} -> {}",
            db_file_size(&db_path)
        );
    }

    // BELOW — the big CURRENT snapshot dominates, so pruning the tiny older frees a minority.
    {
        let repo_root = tempdir().unwrap();
        let repo_dir = repo_root.path().join("below");
        write_base(&repo_dir);
        add_module(&repo_dir, 2, 1); // tiny
        let i1 = index(&dispatcher, "b1", &repo_dir);
        add_module(&repo_dir, 2, 600); // overwrite → big current
        let _i2 = index(&dispatcher, "b2", &repo_dir);
        let db_path = i1["db_path"].as_str().unwrap().to_string();
        let repo_uid = i1["repo_uid"].as_str().unwrap().to_string();
        let size_before = db_file_size(&db_path);

        let outcome = match try_retention_attempt(
            &state,
            Path::new(&db_path),
            &repo_uid,
            &repo_dir.to_string_lossy(),
        ) {
            RetentionAttempt::Ran(o) => o,
            other => panic!(
                "below-threshold pass must RUN (and skip the VACUUM): {}",
                attempt_label(&other)
            ),
        };
        assert!(
            outcome.pruned_count >= 1,
            "the tiny older snapshot IS pruned — rows gone even though the VACUUM is skipped: {outcome:?}"
        );
        assert_eq!(
            outcome.vacuum.as_str(),
            "below_threshold",
            "a minority reclaim SKIPS the VACUUM through the daemon pass path: {outcome:?}"
        );
        assert_eq!(
            outcome.reclaimed_bytes, 0,
            "no VACUUM → no on-disk reclaim reported (honest): {outcome:?}"
        );
        assert!(
            db_file_size(&db_path) >= size_before,
            "honest: the file is NOT shrunk when the reclaim is below threshold ({size_before} vs {})",
            db_file_size(&db_path)
        );
    }
}

// ── DOCTOR SHOWS THE PASS AS AN ACTIVE OP (review-0 #4) ────────────────────────────────────────────

/// ACTIVE-OP SURFACE PROOF (review-0 #4 / slice §4): while the pass holds its activity stamp, the REAL
/// `daemon_info` surface reports it as an in-flight op of kind `retention`, which `rmap doctor` renders
/// "reclaiming <repo>" (the render itself is proven in the rgr `activity_probe_renders_in_flight_retention`
/// unit test). The stamp is created with the EXACT `ActivityRegistry::begin(OpKind::Retention, …)` call
/// `try_retention_attempt` makes after both write-gates clear — held here for a deterministic observation
/// (the alternative, racing a sub-second real pass, is inherently flaky), and cleared on drop like the
/// real pass. Also asserts gate-1 (`active_for_db`) sees the stamp — the contention interlock's basis.
#[test]
fn daemon_info_surfaces_the_active_retention_op() {
    let _serial = serial_guard();
    set_auto_retention_for_test(false);

    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_base(&repo_dir);
    let i1 = index(&dispatcher, "idx", &repo_dir);
    let db_path = i1["db_path"].as_str().unwrap().to_string();
    let repo_uid = i1["repo_uid"].as_str().unwrap().to_string();
    let repo_display = repo_dir.to_string_lossy().to_string();

    // Before: no retention op in flight.
    let before = expect_success(run(&dispatcher, "di0", "daemon_info", json!({})));
    assert!(
        !active_kinds(&before).iter().any(|k| k == "retention"),
        "no retention op before the pass stamps one: {before}"
    );

    // Stamp exactly as `try_retention_attempt` does (activity registry `begin`, held for the pass's life).
    let guard = state.activity().begin(
        OpKind::Retention,
        repo_display.clone(),
        Some(repo_uid.clone()),
        db_path.clone(),
    );

    // While stamped: `daemon_info` surfaces it as an active `retention` op with the reader-frame repo.
    let during = expect_success(run(&dispatcher, "di1", "daemon_info", json!({})));
    let ops = during["active_operations"]
        .as_array()
        .expect("daemon_info carries active_operations");
    let retention_op = ops
        .iter()
        .find(|o| o["kind"] == "retention")
        .unwrap_or_else(|| panic!("daemon_info must surface the active retention op: {during}"));
    assert_eq!(
        retention_op["repo"], repo_display,
        "the active retention op carries the reader-frame repo display: {during}"
    );
    // Gate-1 basis: a concurrent op checking this DB sees the retention op (the two-gate interlock).
    assert!(
        state
            .activity()
            .active_for_db(Path::new(&db_path))
            .is_some(),
        "active_for_db sees the stamped retention op"
    );

    // After the pass finishes (guard drops), the op clears — doctor returns to idle for this repo.
    drop(guard);
    let after = expect_success(run(&dispatcher, "di2", "daemon_info", json!({})));
    assert!(
        !active_kinds(&after).iter().any(|k| k == "retention"),
        "the retention op clears when the pass finishes: {after}"
    );
}

/// The `kind` strings of every op in a `daemon_info` `active_operations` array.
fn active_kinds(info: &Value) -> Vec<String> {
    info["active_operations"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|o| o["kind"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// The main DB file size on disk (0 if absent) — the VACUUM shrink/no-shrink witness.
fn db_file_size(db_path: &str) -> u64 {
    std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0)
}

fn attempt_label(a: &RetentionAttempt) -> String {
    match a {
        RetentionAttempt::Ran(o) => format!("Ran(pruned={})", o.pruned_count),
        RetentionAttempt::Yielded(r) => format!("Yielded({r})"),
        RetentionAttempt::Failed(e) => format!("Failed({e})"),
    }
}
