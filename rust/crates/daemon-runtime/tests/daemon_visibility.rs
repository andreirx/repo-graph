//! DAEMON-VISIBILITY-1 named proofs, driven through the REAL `ServiceDispatcher::dispatch` surface
//! against REAL in-flight index/refresh writes — the level the iteration-0 review required ("a REAL
//! in-flight write op (slow fixture index holding the DB), not synthetic JSON / manually stamped
//! registries").
//!
//! The in-flight write is a real dispatched `index`/`refresh` parked deterministically on a `Condvar`
//! inside its progress callback (the exact technique `tests/concurrency_dispatch.rs` uses): at the first
//! emit the handler is inside `handle_index`/`handle_refresh`'s `acquire_write()` scope, has stamped the
//! activity registry, and holds the DB write lock — a genuine in-flight write, no timer, no manual
//! `activity().begin()`.
//!
//! Proofs here:
//! - **D (status) + E (contention)** — `inflight_index_reported_by_status_and_contention_surfaces`:
//!   while a real index is parked in flight, `daemon_info` reports it (kind/repo/phase/counters/started)
//!   and `storage_health` reports healthy "in use by daemon" (NOT "error opening database"). After it
//!   completes, `daemon_info` is idle and `storage_health` reads snapshots normally.
//! - **D (status, refresh — review-6)** — `inflight_refresh_reported_with_phase_and_counters_by_status_surface`:
//!   contract C/D covers index AND refresh, but only `handle_index` teed the live phase/counters into
//!   the activity record. A real parked refresh now reports kind/repo/**non-null phase**/counters/started
//!   on `daemon_info` — the discriminator that the `handle_refresh` callback tee (`_activity.update`) is
//!   in place (without it the phase is null mid-refresh).
//! - **C (still-running INPUT is real)** — the same real `daemon_info` output carries the
//!   `active_operations` entry the client's still-running probe keys on. The pure classifier is
//!   unit-tested in `rgr` (`still_running_timeout_yields_distinct_non_failure_exit_status`); the full
//!   client transport path is proven by the socket E2E (`scripts/dv1-inflight-e2e.sh`).
//! - **F2** — `orient_on_repo_with_only_non_ready_snapshot_names_the_partial`: a repo whose only
//!   snapshot is non-READY makes the real `orient` dispatch return a message that NAMES the partial
//!   (state + size) and BOTH next actions — never a bare "index the repo first".
//! - **F2 (review-2)** — `quality_and_governance_surfaces_name_the_partial_when_only_non_ready`: the same
//!   partial-naming holds for churn/hotspots/risk/assess/violations/policy (never bare "no snapshot found").
//! - **F2 (review-4)** — `enrich_on_repo_with_only_non_ready_snapshot_names_the_partial`: the READY-requiring
//!   `enrich` surface names the partial too (the last bare "no snapshot found" in this crate).
//! - **F3 (operator Option A)** — `prune_reclaims_orphaned_non_ready_and_leaves_ready_untouched`: an
//!   interrupted (non-READY) snapshot holding real bytes is deleted by the real `classify_retention`
//!   (prune) handler, the disk is reclaimed (VACUUM), and the READY snapshot is untouched.

use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_storage::types::{CreateSnapshotInput, GraphNode, UpdateSnapshotStatusInput};
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

// ── Harness (mirrors tests/concurrency_dispatch.rs) ──────────────────────────

/// A progress emitter that discards events.
struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Shared rendezvous between the parked writer and the test thread (no timer).
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
/// handler is inside `handle_index`'s `acquire_write()` scope and has stamped the activity registry.
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
    // SNAPSHOT-RETENTION-1: these DAEMON-VISIBILITY proofs assert the daemon is IDLE right after an
    // index (no active op) and that the DB write lock is free for a parked re-index. The auto-retention
    // pass is a NEW background write-lock + activity actor that would perturb both, so disable it here
    // (this binary tests index/refresh visibility, not retention — the retention pass, incl. its honest
    // reader-vs-VACUUM behavior, is proven directly in the `retention_pass` lib tests).
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = Arc::new(ServiceDispatcher::new(Arc::clone(&state)));
    (dispatcher, state, state_root)
}

/// A small cross-file import + call fixture so the index produces a real graph.
fn write_s1(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("helper.ts"),
        "export function helperFunction() {\n    console.log('helper');\n}\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("main.ts"),
        "import { helperFunction } from './helper';\n\nexport function mainEntry() {\n    helperFunction();\n}\n",
    )
    .unwrap();
}

/// Add a second symbol so a re-index has new work to do (drives a real in-flight re-index).
fn add_s2(repo_dir: &Path) {
    std::fs::write(
        repo_dir.join("second.ts"),
        "import { helperFunction } from './helper';\n\nexport function secondEntry() {\n    helperFunction();\n}\n",
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
        DispatchResult::Error(e) => {
            panic!(
                "expected success, got error {}: {}",
                e.error.code, e.error.message
            )
        }
    }
}

#[track_caller]
fn expect_error_message(result: DispatchResult) -> String {
    match result {
        DispatchResult::Error(e) => e.error.message,
        DispatchResult::Success(_) => panic!("expected an error, got success"),
    }
}

fn index_s1(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> Value {
    expect_success(run(
        dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ))
}

// ── D (status) + E (contention) — REAL in-flight index ───────────────────────

/// While a REAL dispatched index is parked in flight (holding the DB write lock, activity stamped),
/// `daemon_info` reports the operation (D) and `storage_health` reports healthy "in use by daemon" (E,
/// the fix for "error opening database"). After completion, status is idle and storage reads normally.
#[test]
fn inflight_index_reported_by_status_and_contention_surfaces() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);

    let indexed = index_s1(&dispatcher, &repo_dir);
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();

    // Baseline (idle): daemon_info reports NO activity (known-empty, never a false omission).
    let idle = expect_success(run(&dispatcher, "info0", "daemon_info", json!({})));
    assert_eq!(
        idle["active_operations"].as_array().map(|a| a.len()),
        Some(0),
        "idle daemon reports an empty activity list: {idle}"
    );

    // Park a REAL re-index in flight.
    add_s2(&repo_dir);
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
                    &mut emitter,
                ),
                DispatchResult::Success(_)
            )
        })
    };
    park.wait_until_entered(); // the index is provably in flight (inside acquire_write, activity stamped).

    // D (status): daemon_info reports the real in-flight index — kind / repo / started-at.
    let info = expect_success(run(&dispatcher, "info1", "daemon_info", json!({})));
    let ops = info["active_operations"]
        .as_array()
        .expect("active_operations array");
    assert_eq!(ops.len(), 1, "the real in-flight index is reported: {info}");
    assert_eq!(ops[0]["kind"], "index");
    assert_eq!(
        ops[0]["repo"],
        json!(canonical),
        "reports the repo being indexed"
    );
    assert!(
        ops[0]["started_secs_ago"].is_u64(),
        "carries an elapsed for the 'started N ago' line"
    );
    // D (live progress reaches the surface): the phase + counters are teed into the record from the
    // SAME progress event the attached client sees, so `rmap doctor` renders "indexing <repo>:
    // <phase> …". Parked at the FIRST emit, the record already carries that event's phase (the callback
    // tees BEFORE it emits) — so a NON-NULL phase here proves the tee end-to-end, not just in the unit
    // test. (The refresh analogue is `inflight_refresh_reported_with_phase_and_counters_by_status_surface`.)
    assert!(
        ops[0]["phase"].is_string(),
        "the live phase is teed onto the status surface (not null): {info}"
    );
    assert!(
        ops[0]["current"].is_u64() && ops[0]["total"].is_u64(),
        "the live file counters are present on the status surface: {info}"
    );

    // C (still-running input is REAL): the client's still-running probe keys on exactly this —
    // an active op for this repo in `daemon_info`. Prove that condition is produced by a real index.
    assert!(
        ops.iter()
            .any(|op| op["repo"] == json!(canonical) && op["kind"] == json!("index")),
        "the real daemon_info carries the active-op the still-running classifier consumes"
    );

    // E (contention): storage_health reports healthy "in use by daemon", NOT a busy-open error.
    let health = expect_success(run(
        &dispatcher,
        "sh1",
        "storage_health",
        json!({ "path": canonical }),
    ));
    assert_eq!(
        health["in_use_by_daemon"], true,
        "a DB held by a live daemon index is healthy-in-use, not an error: {health}"
    );
    assert_eq!(health["operation"]["kind"], "index");
    assert!(
        health.get("read_error").is_none(),
        "must NOT be a read error while the daemon holds its own lock: {health}"
    );
    assert!(
        health["snapshots"].is_null(),
        "snapshot detail is UNKNOWN (null) while the DB is written, not a false zero: {health}"
    );

    // Let the index finish.
    park.release();
    assert!(
        writer.join().expect("writer thread"),
        "the parked re-index completes after release"
    );

    // After completion: idle, and storage reads snapshots normally (completion is observable).
    let idle_again = expect_success(run(&dispatcher, "info2", "daemon_info", json!({})));
    assert_eq!(
        idle_again["active_operations"].as_array().map(|a| a.len()),
        Some(0),
        "after completion the activity list is empty again: {idle_again}"
    );
    // D2 (completion observable): idle `daemon_info` names the last COMPLETED snapshot (repo + time) —
    // the "idle; last snapshot <repo> @ <time>" doctor fact, sourced from the registry (no DB open).
    assert!(
        idle_again["last_snapshot"]["repo"].is_string(),
        "idle daemon_info names the last completed snapshot's repo: {idle_again}"
    );
    assert!(
        idle_again["last_snapshot"]["at"].is_string(),
        "idle daemon_info carries the last completed snapshot's time: {idle_again}"
    );
    let health_after = expect_success(run(
        &dispatcher,
        "sh2",
        "storage_health",
        json!({ "path": canonical }),
    ));
    assert_ne!(health_after["in_use_by_daemon"], json!(true), "idle now");
    assert!(
        health_after["total_snapshots"].as_i64().unwrap_or(0) >= 1,
        "storage reads snapshot counts normally once idle: {health_after}"
    );
}

// ── D (status) — REAL in-flight REFRESH carries phase + counters ─────────────

/// review-6 required change: contract C/D covers index AND refresh, but only `handle_index` teed the
/// live phase/counters into the activity record — an in-flight refresh reported a NULL phase on the
/// status surface. This proof parks a REAL dispatched `refresh` in flight (the same Condvar technique as
/// the index proof: at the first progress emit the handler is inside `handle_refresh`'s `acquire_write()`
/// scope, has stamped the activity registry, and holds the DB write + repo refresh locks) and asserts
/// `daemon_info` reports the refresh with kind / repo / a NON-NULL phase / live counters / started-at —
/// the fields `rmap doctor` renders as "refreshing <repo>: <phase> …". Without the refresh-callback tee
/// (`_activity.update(...)`) the phase is `null` even mid-refresh; that regression is exactly what the
/// NON-NULL phase assertion guards. `daemon_info` is lock-light (no repo read guard), so it does not
/// deadlock against the parked refresh's held locks.
#[test]
fn inflight_refresh_reported_with_phase_and_counters_by_status_surface() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);

    // Index once so the repo is registered + loaded (refresh resolves it from the registry by path).
    let indexed = index_s1(&dispatcher, &repo_dir);
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();
    add_s2(&repo_dir); // give the refresh real work (parity with the index proof; not required to park).

    // Park a REAL refresh in flight (dispatched by `repo`, the key `resolve_and_load_repo` reads).
    let park = ParkHandle::default();
    let writer = {
        let dispatcher = Arc::clone(&dispatcher);
        let park = park.clone();
        let repo = canonical.clone();
        thread::spawn(move || {
            let mut emitter = ParkOnceEmitter {
                handle: park,
                parked: false,
            };
            matches!(
                dispatcher.dispatch(
                    &request("rf1", "refresh", json!({ "repo": repo })),
                    &mut emitter,
                ),
                DispatchResult::Success(_)
            )
        })
    };
    park.wait_until_entered(); // the refresh is provably in flight (inside acquire_write, activity stamped).

    // D (status): daemon_info reports the real in-flight refresh — kind / repo / phase / counters / started.
    let info = expect_success(run(&dispatcher, "info-rf", "daemon_info", json!({})));
    let ops = info["active_operations"]
        .as_array()
        .expect("active_operations array");
    assert_eq!(
        ops.len(),
        1,
        "the real in-flight refresh is reported: {info}"
    );
    assert_eq!(
        ops[0]["kind"], "refresh",
        "reports it as a refresh op: {info}"
    );
    assert_eq!(
        ops[0]["repo"],
        json!(canonical),
        "reports the repo being refreshed"
    );
    // THE review-6 discriminator: the phase is teed from the refresh progress event (NON-NULL). Without
    // the `handle_refresh` callback tee this field is `null` even though the refresh is in flight.
    assert!(
        ops[0]["phase"].is_string(),
        "the refresh's live phase is on the status surface (not null): {info}"
    );
    assert!(
        ops[0]["current"].is_u64() && ops[0]["total"].is_u64(),
        "the refresh's live file counters are present: {info}"
    );
    assert!(
        ops[0]["started_secs_ago"].is_u64(),
        "carries an elapsed for the 'started N ago' line: {info}"
    );

    // Let the refresh finish; status returns to idle (RAII deregister — no leak).
    park.release();
    assert!(
        writer.join().expect("refresh writer thread"),
        "the parked refresh completes after release"
    );
    let idle_again = expect_success(run(&dispatcher, "info-rf2", "daemon_info", json!({})));
    assert_eq!(
        idle_again["active_operations"].as_array().map(|a| a.len()),
        Some(0),
        "after the refresh completes the activity list is empty again: {idle_again}"
    );
}

// ── F2 — orient names the partial (REAL dispatch) ────────────────────────────

/// A repo whose only snapshot is non-READY (the day-2 field case: an interrupted finalize) makes the
/// real `orient` dispatch return an honest message — NAMES the interrupted snapshot (state + on-disk
/// size) and BOTH next actions (re-index / `maintenance prune`) — never a bare "index the repo first".
#[test]
fn orient_on_repo_with_only_non_ready_snapshot_names_the_partial() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);

    let indexed = index_s1(&dispatcher, &repo_dir);
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();

    // Simulate the interrupted finalize: flip the only snapshot to non-READY. Now `get_latest_snapshot`
    // (READY-only) sees nothing, but a 'building' partial exists holding disk.
    let conn = StorageConnection::open(&db_path).unwrap();
    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid,
        status: "building".to_string(),
        completed_at: None,
    })
    .unwrap();
    assert!(
        repo_graph_agent::AgentStorageRead::get_latest_snapshot(&conn, &repo_uid)
            .unwrap()
            .is_none(),
        "precondition: no READY snapshot after the flip"
    );
    drop(conn);

    let msg = expect_error_message(run(
        &dispatcher,
        "or",
        "orient",
        json!({ "repo": canonical }),
    ));
    assert!(
        msg.contains("interrupted"),
        "names the partial's state: {msg}"
    );
    assert!(
        msg.contains("on disk"),
        "names the on-disk size held: {msg}"
    );
    assert!(
        msg.contains("rmap index"),
        "next action 1 (re-index): {msg}"
    );
    assert!(
        msg.contains("rmap maintenance prune"),
        "next action 2 (reclaim): {msg}"
    );
    assert!(
        !msg.contains("index the repo first"),
        "must NOT be the bare gaslighting message: {msg}"
    );
}

// ── F2 (review-2) — quality + governance surfaces name the partial too ───────

/// review-2 required change #1: F2 must hold on the quality/governance READY-requiring surfaces, not
/// only orient/explain. Same day-2 fixture (a repo whose only snapshot is a non-READY interrupted
/// partial), dispatched through EACH real handler: churn / hotspots / risk / assess / violations /
/// policy must NAME the partial (state + on-disk size + both next actions), never a bare
/// "no snapshot found".
///
/// `coverage` is excluded from the runtime loop only because it validates its required `report_path`
/// param BEFORE resolving the repo (a `{repo}`-only request fails earlier). Its snapshot-check fix is
/// the identical shared edit; the message text is unit-covered by the helper's own tests
/// (`snapshot_facts::tests::partial_message_names_the_interrupted_snapshot_and_both_actions`).
#[test]
fn quality_and_governance_surfaces_name_the_partial_when_only_non_ready() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);

    let indexed = index_s1(&dispatcher, &repo_dir);
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();

    // Interrupted-finalize fixture: flip the only snapshot to non-READY (same as the orient proof).
    let conn = StorageConnection::open(&db_path).unwrap();
    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid,
        status: "building".to_string(),
        completed_at: None,
    })
    .unwrap();
    assert!(
        repo_graph_agent::AgentStorageRead::get_latest_snapshot(&conn, &repo_uid)
            .unwrap()
            .is_none(),
        "precondition: no READY snapshot after the flip"
    );
    drop(conn);

    // Each of these is a READY-requiring quality/governance surface now routed through the shared
    // honest helper (previously each returned a bare "no snapshot found" — the review-2 gap).
    for method in [
        "churn",
        "hotspots",
        "risk",
        "assess",
        "violations",
        "policy",
    ] {
        let msg = expect_error_message(run(
            &dispatcher,
            method,
            method,
            json!({ "repo": canonical }),
        ));
        assert!(
            !msg.contains("no snapshot found"),
            "[{method}] must NOT be the bare gaslighting message: {msg}"
        );
        assert!(
            msg.contains("interrupted"),
            "[{method}] names the partial's state: {msg}"
        );
        assert!(
            msg.contains("on disk"),
            "[{method}] names the on-disk size held: {msg}"
        );
        assert!(
            msg.contains("rmap index"),
            "[{method}] next action 1 (re-index): {msg}"
        );
        assert!(
            msg.contains("rmap maintenance prune"),
            "[{method}] next action 2 (reclaim): {msg}"
        );
    }
}

// ── F2 (review-4) — the enrich surface names the partial too ──────────────────

/// review-4 required change: the daemon `enrich` surface is READY-requiring too — it was the LAST
/// bare "no snapshot found" in `daemon-runtime`. On the day-2 fixture (a repo whose only snapshot is
/// a non-READY interrupted partial), a dispatched `enrich` with no explicit snapshot resolves the
/// latest READY snapshot (`get_latest_snapshot`, READY-only), finds none, and MUST name the partial
/// (state + on-disk size) + BOTH next actions (`rmap index` / `rmap maintenance prune`) under the
/// honest `SnapshotNotFound` code — never the bare "no snapshot found" it previously emitted.
///
/// enrich differs from the quality/governance handlers: it is addressed by raw `db_path` + `repo_uid`
/// and requires the repo already loaded (`get_repo_by_key`), so the test loads it explicitly.
#[test]
fn enrich_on_repo_with_only_non_ready_snapshot_names_the_partial() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);

    let indexed = index_s1(&dispatcher, &repo_dir);
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();

    // Interrupted-finalize fixture: flip the only snapshot to non-READY (same as the orient proof).
    let conn = StorageConnection::open(&db_path).unwrap();
    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid,
        status: "building".to_string(),
        completed_at: None,
    })
    .unwrap();
    assert!(
        repo_graph_agent::AgentStorageRead::get_latest_snapshot(&conn, &repo_uid)
            .unwrap()
            .is_none(),
        "precondition: no READY snapshot after the flip"
    );
    drop(conn);

    // enrich requires the repo LOADED (get_repo_by_key) — addressed by db_path + repo_uid, not `repo`.
    expect_success(run(
        &dispatcher,
        "load",
        "load_repo",
        json!({ "db_path": db_path, "repo_uid": repo_uid }),
    ));

    // enrich with no explicit snapshot → latest-READY resolution → Ok(None) → the honest F2 message.
    let msg = expect_error_message(run(
        &dispatcher,
        "enr",
        "enrich",
        json!({ "db_path": db_path, "repo_uid": repo_uid }),
    ));
    assert!(
        !msg.contains("no snapshot found"),
        "enrich must NOT be the bare gaslighting message: {msg}"
    );
    assert!(
        msg.contains("interrupted"),
        "enrich names the partial's state: {msg}"
    );
    assert!(
        msg.contains("on disk"),
        "enrich names the on-disk size held: {msg}"
    );
    assert!(
        msg.contains("rmap index"),
        "enrich next action 1 (re-index): {msg}"
    );
    assert!(
        msg.contains("rmap maintenance prune"),
        "enrich next action 2 (reclaim): {msg}"
    );
}

// ── F3 (operator Option A) — prune reclaims the orphaned non-READY snapshot ───

/// Insert `n` SYMBOL nodes under `snapshot_uid` to bloat the DB by a measurable amount, so the
/// post-prune VACUUM has real bytes to return to the OS.
fn bloat_nodes(db_path: &str, repo_uid: &str, snapshot_uid: &str, n: usize) {
    let mut conn = StorageConnection::open(db_path).expect("open db to bloat");
    let nodes: Vec<GraphNode> = (0..n)
        .map(|i| GraphNode {
            node_uid: format!("bloat-{i}"),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            stable_key: format!("{repo_uid}:bloat{i}:SYMBOL"),
            kind: "SYMBOL".to_string(),
            subtype: Some("FUNCTION".to_string()),
            name: format!("bloat{i}"),
            qualified_name: Some(format!("bloated::module::path::to::symbol::number::{i}")),
            file_uid: None,
            parent_node_uid: None,
            location: None,
            signature: Some("fn bloat(a: usize, b: usize, c: usize) -> usize".to_string()),
            visibility: Some("export".to_string()),
            doc_comment: Some(
                "a padded doc comment to grow the row size for a measurable reclaim".to_string(),
            ),
            metadata_json: None,
        })
        .collect();
    conn.insert_nodes(&nodes).expect("insert bloat nodes");
    // Drop the connection here (end of fn) → last-connection WAL checkpoint moves the bloat into the
    // main DB file, so the prune handler's pre-delete size measurement reflects it.
}

/// Create an interrupted (non-READY) snapshot holding real bytes, then the real `classify_retention`
/// (prune) handler deletes it, reclaims the disk (VACUUM), and leaves the READY snapshot untouched —
/// the operator's required end-to-end proof (rows gone + disk reclaimed + READY untouched).
#[test]
fn prune_reclaims_orphaned_non_ready_and_leaves_ready_untouched() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_s1(&repo_dir);

    let indexed = index_s1(&dispatcher, &repo_dir);
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let ready_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();

    // Add an interrupted (building) snapshot holding ~20k bloat nodes — an orphaned partial that never
    // finalized. No live op touches this DB (the index completed), so it is genuinely orphaned.
    let building = {
        let conn = StorageConnection::open(&db_path).unwrap();
        conn.create_snapshot(&CreateSnapshotInput {
            repo_uid: repo_uid.clone(),
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            parent_snapshot_uid: None,
            label: None,
            toolchain_json: None,
        })
        .unwrap()
        // status defaults to 'building' — a non-READY partial.
    };
    assert_eq!(building.status, "building");
    bloat_nodes(&db_path, &repo_uid, &building.snapshot_uid, 20_000);

    // Precondition: two snapshots (READY + interrupted), and the interrupted one is listed.
    {
        let conn = StorageConnection::open(&db_path).unwrap();
        let snaps = conn.list_snapshots(&repo_uid).unwrap();
        assert_eq!(snaps.len(), 2, "READY + interrupted present before prune");
        assert!(snaps.iter().any(|s| s.status == "building"));
    }

    // Prune (the real `rmap maintenance prune` daemon call).
    let resp = expect_success(run(
        &dispatcher,
        "prune",
        "classify_retention",
        json!({ "path": canonical }),
    ));
    let reclaim = &resp["non_ready_reclaim"];
    assert_eq!(
        reclaim["reclaimed"], true,
        "the orphaned non-READY snapshot was reclaimed: {resp}"
    );
    assert!(
        reclaim["deleted_count"].as_u64().unwrap_or(0) >= 1,
        "at least the interrupted snapshot was deleted: {resp}"
    );
    assert!(
        reclaim["reclaimed_bytes"].as_u64().unwrap_or(0) > 0,
        "disk was ACTUALLY reclaimed (VACUUM shrank the file): {resp}"
    );

    // Rows gone + READY untouched: only the READY snapshot remains.
    {
        let conn = StorageConnection::open(&db_path).unwrap();
        let snaps = conn.list_snapshots(&repo_uid).unwrap();
        assert_eq!(
            snaps.len(),
            1,
            "only the READY snapshot survives: {snaps:?}"
        );
        assert_eq!(
            snaps[0].snapshot_uid, ready_uid,
            "the READY snapshot is untouched"
        );
        assert_eq!(snaps[0].status, "ready");
    }

    // READY untouched at the QUERY level: the original symbol still resolves through the dispatcher.
    let callers = run(
        &dispatcher,
        "chk",
        "callers",
        json!({ "repo": canonical, "symbol": "helperFunction" }),
    );
    assert!(
        matches!(callers, DispatchResult::Success(_)),
        "after reclaim the READY snapshot still answers callers (untouched)"
    );
}
