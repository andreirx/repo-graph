//! DAEMON-RESIDUALS-2C §7 — `rmap repo rebuild` (the `repo_rebuild` wire method) named proofs.
//!
//! Driven through the REAL `ServiceDispatcher::dispatch` surface (the protocol the socket serves)
//! against REAL indexes on an isolated `DaemonState` (its own temp state root — the operator's
//! registry/daemon are NEVER touched), mirroring `tests/forget_repo.rs`.
//!
//! Contract (`docs/slices/daemon-residuals-2.md` §7) proven here:
//! - explicit intent REQUIRED: no `confirm` → refuse, nothing discarded;
//! - an unregistered repo → `RepoNotFound` (rebuild is recovery for an indexed repo, not a first index);
//! - success: the store's prior snapshots are DISCARDED and it is reindexed to a single fresh
//!   snapshot, while the registry entry (repo_uid) is KEPT;
//! - a NAMED `Busy` when a writer holds the DB write discipline, with NOTHING discarded (the store
//!   is intact after the bounce).

use std::path::Path;
use std::sync::Arc;

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

fn isolated() -> (ServiceDispatcher, Arc<DaemonState>, TempDir) {
    // Deterministic assertions: disable the background write actors (they hold the write lock over
    // the very store this suite inspects). Proven in their own suites.
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    let root = tempdir().expect("state root tempdir");
    let registry =
        RepoRegistry::with_state_root(root.path()).expect("isolated registry under temp root");
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(Arc::clone(&state));
    (dispatcher, state, root)
}

fn write_fixture(repo_dir: &Path) {
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
fn expect_error(result: DispatchResult) -> (String, String) {
    match result {
        DispatchResult::Success(s) => panic!("expected error, got success: {}", s.result),
        DispatchResult::Error(e) => (e.error.code.to_string(), e.error.message),
    }
}

fn index_fixture(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> (String, String, String) {
    let resp = expect_success(run(
        dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    (
        resp["canonical_path"].as_str().unwrap().to_string(),
        resp["db_path"].as_str().unwrap().to_string(),
        resp["repo_uid"].as_str().unwrap().to_string(),
    )
}

// ── explicit intent ───────────────────────────────────────────────────────────

#[test]
fn rebuild_without_confirm_refuses_and_discards_nothing() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, _uid) = index_fixture(&dispatcher, &repo_dir);

    let (_code, msg) = expect_error(run(
        &dispatcher,
        "rb",
        "repo_rebuild",
        json!({ "repo": canonical }), // no confirm
    ));
    assert!(
        msg.contains("not confirmed") || msg.contains("--yes"),
        "refusal names the missing confirmation: {msg}"
    );
    // The store is untouched — nothing was discarded on the unconfirmed call.
    assert!(Path::new(&db_path).exists(), "store intact after refusal");
}

#[test]
fn rebuild_unregistered_repo_is_repo_not_found() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("never-indexed");
    write_fixture(&repo_dir);

    let (code, _msg) = expect_error(run(
        &dispatcher,
        "rb",
        "repo_rebuild",
        json!({ "repo": repo_dir.to_string_lossy(), "confirm": true }),
    ));
    assert_eq!(code, "RepoNotFound", "unregistered repo → RepoNotFound");
}

// ── success: discard the store's snapshots, reindex to one fresh snapshot, keep the registry ─────

#[test]
fn rebuild_discards_snapshots_reindexes_and_keeps_registry_entry() {
    let (dispatcher, state, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);

    // Two indexes → two READY snapshots accumulate (retention off in this harness).
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);
    let second = expect_success(run(
        &dispatcher,
        "idx2",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let pre_rebuild_snapshot = second["snapshot_uid"].as_str().unwrap().to_string();

    let resp = expect_success(run(
        &dispatcher,
        "rb",
        "repo_rebuild",
        json!({ "repo": canonical, "confirm": true }),
    ));

    // A fresh snapshot, distinct from the pre-rebuild ones.
    let new_snapshot = resp["snapshot_uid"].as_str().unwrap().to_string();
    assert!(!new_snapshot.is_empty());
    assert_ne!(
        new_snapshot, pre_rebuild_snapshot,
        "rebuild produced a NEW snapshot"
    );
    // The store was reset: classify after the reindex sees exactly one snapshot (the fresh one) —
    // every prior snapshot was discarded with the store.
    assert_eq!(
        resp["retention"]["total"].as_i64(),
        Some(1),
        "store reset to a single fresh snapshot: {resp}"
    );
    assert_eq!(
        resp["rebuild"]["registry_entry_kept"],
        json!(true),
        "the verb reports the registry entry was kept"
    );
    assert!(
        resp["files_total"].as_u64().unwrap_or(0) >= 1,
        "reindex reported files: {resp}"
    );

    // The registry entry (repo_uid) is KEPT — same identity, still resolvable on disk.
    assert_eq!(
        resp["repo_uid"].as_str(),
        Some(uid.as_str()),
        "repo_uid unchanged (registry entry kept)"
    );
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    assert!(
        reloaded.resolve(&repo_dir).is_some(),
        "registry entry still present on disk after rebuild"
    );
    assert!(Path::new(&db_path).exists(), "a fresh store exists on disk");
    // The rebuilt store is queryable (a real reindex happened).
    let info = expect_success(run(
        &dispatcher,
        "info",
        "repo_info",
        json!({ "repo": canonical }),
    ));
    let _ = state; // state handle retained for isolation lifetime
    assert!(info["storage"].is_object(), "rebuilt repo reports storage");
}

// ── NAMED Busy when a writer holds the DB write discipline; nothing discarded ────────────────────

#[test]
fn rebuild_bounces_with_named_busy_when_a_writer_holds_the_lock_and_discards_nothing() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);
    // Load it so the coordinator exists (the same instance rebuild acquires).
    state.load_repo(Path::new(&db_path), &uid).unwrap();

    // Hold the DB write mutex — the universal write barrier a detached index also holds. Rebuild's
    // bounded-patience acquire (layer 1) will time out and bounce with a NAMED Busy.
    let db_runtime = state
        .get_or_create_db_runtime_for_new_db(Path::new(&db_path))
        .unwrap();
    let _held = db_runtime.acquire_write();

    let (code, msg) = expect_error(run(
        &dispatcher,
        "rb",
        "repo_rebuild",
        json!({ "repo": canonical, "confirm": true }),
    ));
    assert_eq!(code, "Busy", "a held write lock → NAMED Busy: {msg}");
    assert!(
        msg.contains("busy") || msg.contains("retry"),
        "the Busy message is reader-facing and retryable: {msg}"
    );
    // NOTHING was discarded: the store file is still there (the wipe happens only AFTER the guard is
    // acquired, which never happened).
    assert!(
        Path::new(&db_path).exists(),
        "store intact after a Busy bounce"
    );
}

// ── detect-and-name (HUMAN RULING, cycle 2): the `.rebuilding` sentinel refuses serving reads; the
//    verb is the remedy that clears it ─────────────────────────────────────────────────────────────

/// The sentinel `handle_repo_rebuild` writes beside the store before retiring it (`<db>.rebuilding`).
/// Constructed here from the CONVENTION so the test asserts that contract, not an internal path fn.
fn sentinel_path(db_path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{db_path}.rebuilding"))
}

/// The exact reason every store-open path returns while the sentinel is present (ruling-named).
const REBUILD_INTERRUPTED_REASON: &str =
    "rebuild interrupted — run `rmap repo rebuild <path>` again";

#[test]
fn a_present_sentinel_makes_serving_reads_refuse_with_the_named_reason() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (_canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);

    // Simulate an interrupted rebuild: a sentinel beside an otherwise-intact store.
    std::fs::write(sentinel_path(&db_path), b"interrupted").unwrap();

    // cycle-3 ruling: EVERY open path refuses on the sentinel BEFORE validation — the LOAD path
    // (`load_repo`, the gate all read handlers' `resolve_and_load_repo` funnel through) now refuses too,
    // not just the later serving-read open. So a reader never even loads the repo, let alone sees the
    // partial store (frozen invariant). `handle_repo_rebuild` (the remedy) does NOT reach here — it
    // selects its coordinator via `loaded_repo_by_uid`/a standalone coordinator.
    assert_plain_open_refuses(&state, &db_path, &uid);
}

#[test]
fn rebuild_on_a_sentinelled_store_proceeds_and_clears_the_sentinel() {
    let (dispatcher, _state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, _uid) = index_fixture(&dispatcher, &repo_dir);

    // A leftover sentinel from an interrupted rebuild (store otherwise intact).
    std::fs::write(sentinel_path(&db_path), b"interrupted").unwrap();
    assert!(sentinel_path(&db_path).exists());

    // `repo rebuild` is the REMEDY: it PROCEEDS on a sentinel'd store (its own load + reindex are the
    // ungated paths), reindexes to a fresh snapshot, and clears the sentinel on success.
    let resp = expect_success(run(
        &dispatcher,
        "rb",
        "repo_rebuild",
        json!({ "repo": canonical, "confirm": true }),
    ));
    assert_eq!(
        resp["retention"]["total"].as_i64(),
        Some(1),
        "reindexed to a single fresh snapshot: {resp}"
    );
    // review-6 item 2: the reply carries the REAL symbol count as the additive `symbols_total` field —
    // the repo-level symbol COUNT(*) (`compute_repo_summary().symbol_count`), a SUBSET of `nodes_total`
    // (COUNT(*) over EVERY node kind), never `nodes_total` relabelled "symbols".
    let symbols = resp["symbols_total"]
        .as_u64()
        .expect("symbols_total present on a healthy rebuild");
    let nodes = resp["nodes_total"].as_u64().expect("nodes_total present");
    assert!(
        symbols <= nodes,
        "the real symbol count ({symbols}) is a subset of all node kinds ({nodes}): {resp}"
    );
    assert!(
        !sentinel_path(&db_path).exists(),
        "the sentinel is cleared after a successful rebuild → serving reads work again"
    );
    // Prove reads work again post-rebuild.
    let info = expect_success(run(
        &dispatcher,
        "info",
        "repo_info",
        json!({ "repo": canonical }),
    ));
    assert!(
        info["storage"].is_object(),
        "rebuilt repo serves reads again"
    );
}

// review-7 required change 1: the SUCCESS-PATH sentinel-unlink failure, driven end-to-end through the
// dispatcher (the cycle-7 test covered only the helper). The reindex COMMITS and `retired.discard()`
// runs, then the post-reindex `remove_rebuild_sentinel` fails — forced via the crate's thread-local
// fault seam, because this state is unreachable via the filesystem alone (the sentinel's create precedes
// its remove on the SAME path, so any obstruction that blocks the unlink blocks the create first, and
// the rebuild would abort at sentinel-WRITE, not sentinel-UNLINK). The handler must return a NAMED
// non-success naming the sentinel-unlink fault AND the rebuild remedy, leave the sentinel-path blocker
// present, and keep every subsequent ordinary store read refused with the exact ruling-named reason.
#[test]
fn post_reindex_sentinel_unlink_failure_is_a_named_nonsuccess_and_the_store_stays_gated() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);

    // Force the post-reindex sentinel unlink to fail on THIS (dispatching) thread. `dispatch` runs
    // `handle_repo_rebuild` → `remove_rebuild_sentinel` inline on this thread, so the thread-local fault
    // is observed here and never leaks into a peer rebuild test running concurrently in this binary.
    repo_graph_daemon_runtime::set_fail_sentinel_removal_for_test(true);
    let (_code, msg) = expect_error(run(
        &dispatcher,
        "rb",
        "repo_rebuild",
        json!({ "repo": canonical, "confirm": true }),
    ));
    // Reset before any assertion (and for thread-reuse hygiene) so nothing else observes the fault.
    repo_graph_daemon_runtime::set_fail_sentinel_removal_for_test(false);

    // (1) NAMED non-success whose message (2) names the sentinel-unlink fault AND the rebuild remedy.
    assert!(
        msg.contains("sentinel"),
        "the failure names the sentinel-unlink fault: {msg}"
    );
    assert!(
        msg.contains("rmap repo rebuild"),
        "the failure names the rebuild remedy: {msg}"
    );

    // (3) the sentinel-path blocker remains present — the unlink failed, so the fresh store stays gated.
    assert!(
        sentinel_path(&db_path).exists(),
        "the sentinel is retained after the unlink failure — the store stays gated"
    );

    // (4) a subsequent ordinary store read remains refused with the exact ruling-named reason (the
    // `load_repo` pre-canonicalization guard refuses BEFORE its cache lookup, even on a cached repo).
    assert_plain_open_refuses(&state, &db_path, &uid);
}

// ── cycle-3 REAL DEFECT: the remedy must work on the failure state it is for ─────────────────────
//    A crash mid-retire can leave the base `.db` absent (or every store file gone) beside the sentinel.
//    `repo rebuild` MUST proceed on ANY residual subset (intact / only sidecars / nothing) — it checks
//    the sentinel and selects its coordinator BEFORE opening/validating the DB — and every OTHER open
//    path (here: the shared `load_repo` gate that all serving/read handlers funnel through) must REFUSE
//    with the exact named reason before validation. Three residual states, all rebuilt to a fresh store.

/// Assert the plain serving/load open refuses with the exact ruling-named reason while the sentinel is up.
/// `load_repo` is the single gate every read handler's `resolve_and_load_repo` funnels through, and it
/// refuses BEFORE canonicalizing/validating the DB — so it names the remedy even when the `.db` is gone.
#[track_caller]
fn assert_plain_open_refuses(state: &Arc<DaemonState>, db_path: &str, uid: &str) {
    // `Arc<RepoState>` is not `Debug`, so match rather than `expect_err`.
    match state.load_repo(Path::new(db_path), uid) {
        Ok(_) => panic!("a plain load/serve open must refuse while the sentinel is present"),
        Err(err) => assert!(
            err.contains(REBUILD_INTERRUPTED_REASON),
            "the plain-open refusal carries the exact ruling-named reason: {err}"
        ),
    }
}

fn rm(path: &str) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("could not remove {path}: {e}"),
    }
}

/// Drive `repo rebuild` on a sentinel'd store and assert it rebuilt to a single fresh snapshot, cleared
/// the sentinel, and serves reads again. Shared tail of the three residual-state tests below.
#[track_caller]
fn assert_rebuild_recovers(dispatcher: &ServiceDispatcher, canonical: &str, db_path: &str) {
    let resp = expect_success(run(
        dispatcher,
        "rb",
        "repo_rebuild",
        json!({ "repo": canonical, "confirm": true }),
    ));
    assert_eq!(
        resp["retention"]["total"].as_i64(),
        Some(1),
        "recovered to a single fresh snapshot: {resp}"
    );
    assert!(
        !sentinel_path(db_path).exists(),
        "the sentinel is cleared after recovery → serving reads work again"
    );
    let info = expect_success(run(
        dispatcher,
        "info",
        "repo_info",
        json!({ "repo": canonical }),
    ));
    assert!(
        info["storage"].is_object(),
        "recovered repo serves reads again"
    );
}

#[test]
fn rebuild_recovers_a_sentinelled_store_with_the_base_db_intact() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);

    // Interrupted rebuild, base `.db` still on disk.
    std::fs::write(sentinel_path(&db_path), b"interrupted").unwrap();

    assert_plain_open_refuses(&state, &db_path, &uid);
    assert_rebuild_recovers(&dispatcher, &canonical, &db_path);
}

#[test]
fn rebuild_recovers_a_sentinelled_store_with_the_base_db_missing() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);

    // Crash AFTER the base `.db` was retired: `.db`/`-shm` gone, a `-wal` sidecar left behind, sentinel up.
    // This is the state the reviewer flagged — `load_repo`/`RepoState::open` would fail with "database
    // not found" BEFORE the sentinel logic, so the OLD code's advertised remedy could not run.
    std::fs::write(sentinel_path(&db_path), b"interrupted").unwrap();
    rm(&db_path);
    rm(&format!("{db_path}-shm"));
    std::fs::write(format!("{db_path}-wal"), b"stale-wal").unwrap();
    assert!(!Path::new(&db_path).exists(), "base .db is absent");

    assert_plain_open_refuses(&state, &db_path, &uid);
    assert_rebuild_recovers(&dispatcher, &canonical, &db_path);
    assert!(
        Path::new(&db_path).exists(),
        "a fresh base .db exists after recovery"
    );
}

#[test]
fn rebuild_recovers_a_sentinelled_store_with_nothing_left() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);

    // Crash with every store file already gone (retire completed, reindex never started), sentinel up.
    std::fs::write(sentinel_path(&db_path), b"interrupted").unwrap();
    rm(&db_path);
    rm(&format!("{db_path}-wal"));
    rm(&format!("{db_path}-shm"));

    assert_plain_open_refuses(&state, &db_path, &uid);
    assert_rebuild_recovers(&dispatcher, &canonical, &db_path);
    assert!(
        Path::new(&db_path).exists(),
        "a fresh base .db exists after recovery from an empty store"
    );
}

// ── reader-held-coordinator NAMED Busy (HUMAN RULING, cycle 2 live-proof mechanism; a writer-held
//    Busy is NOT the same proof) — deterministic peer of the isolated-leveldb live capture ──────────

// RATIFIED PROOF (DAEMON-RESIDUALS-2C cycle-3 ruling `reader_held_live_leveldb_proof` = A): five live
// isolated-leveldb attempts could not provoke the reader-held-coordinator race (a reader never holds the
// coordinator long enough on leveldb); a race that cannot be provoked without a test seam is not
// evidence. THIS deterministic dispatch-level test — driving the SAME coordinator-acquisition path
// (`acquire_foreground_write` → `RepoCoordinator::acquire_refresh_timeout`) — IS the ratified proof of
// the reader-held bounce; the live run stays as the happy-path proof.
//
// A coordinated READER holding the repo coordinator (the 2026-09-04 hazard: startup readers held it
// while an operator tried to rebuild) makes `repo rebuild` wait the bounded patience for the reader to
// DRAIN, then bounce with a NAMED `Busy` that NAMES THE KIND (a concurrent READ) — never yanking the
// store out from under the reader. When the reader releases, the rebuild acquires the coordinator and
// succeeds with a fresh snapshot. This drives the REAL dispatch surface through the LAYER-2 (coordinator
// refresh) timeout — distinct from the LAYER-1 (DB write mutex) writer-held bounce proven above.
// NOTE: takes ~3s (FOREGROUND_WRITE_PATIENCE).
#[test]
fn rebuild_bounces_named_busy_while_a_reader_holds_the_coordinator_then_succeeds_when_released() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);

    // A real coordinated reader holds the SAME coordinator instance rebuild will acquire (load_repo is
    // cached, so this is the exact object rebuild's `acquire_foreground_write` contends on).
    let repo_state = state.load_repo(Path::new(&db_path), &uid).unwrap();
    let read_guard = repo_state.coordinator.acquire_read();

    let (code, msg) = expect_error(run(
        &dispatcher,
        "rb",
        "repo_rebuild",
        json!({ "repo": canonical.clone(), "confirm": true }),
    ));
    assert_eq!(
        code, "Busy",
        "a reader holding the coordinator → NAMED Busy: {msg}"
    );
    // cycle-3 ruling `reader_busy_identity_age` = A: the reader-held Busy must NAME THE KIND with this
    // EXACT clause — never the generic "a concurrent operation" (pure readers carry no holder
    // identity/age; a tracking surface for them is out of scope).
    assert!(
        msg.contains("a concurrent READ is holding this repo's store — retry in a moment"),
        "the Busy NAMES THE KIND (a concurrent READ) with the exact ruling text: {msg}"
    );
    assert!(
        !msg.contains("a concurrent operation"),
        "the reader Busy must NOT fall back to the generic holder-unknown wording: {msg}"
    );
    // Nothing discarded while the reader held the coordinator — the wipe happens only AFTER both guards
    // are acquired, which never happened.
    assert!(
        Path::new(&db_path).exists(),
        "store intact after a reader-held Busy bounce"
    );

    // Reader releases → the rebuild now acquires the coordinator and succeeds with a fresh snapshot.
    drop(read_guard);
    let resp = expect_success(run(
        &dispatcher,
        "rb2",
        "repo_rebuild",
        json!({ "repo": canonical, "confirm": true }),
    ));
    assert_eq!(
        resp["retention"]["total"].as_i64(),
        Some(1),
        "rebuild succeeds once the reader releases: {resp}"
    );
    assert!(
        !resp["snapshot_uid"].as_str().unwrap_or("").is_empty(),
        "a fresh snapshot uid is reported: {resp}"
    );
}
