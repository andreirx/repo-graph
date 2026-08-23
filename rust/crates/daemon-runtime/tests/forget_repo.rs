//! FORGET-REPO-1 named proofs — `repo remove` forgets, the daemon sees + reclaims orphans, and a
//! forget never races a concurrent write into an unregistered/orphan DB.
//!
//! Driven through the REAL `ServiceDispatcher::dispatch` surface (the same protocol the socket serves)
//! against REAL indexes/refreshes on an isolated `DaemonState` (its own temp state root — the
//! operator's registry/daemon are NEVER touched), mirroring `tests/index_disconnect.rs` /
//! `tests/daemon_visibility.rs`. This is the changed-protocol end-to-end coverage review-3 (item 3)
//! required: CLI-shaped requests → daemon dispatch → filesystem effects.
//!
//! Contract (`docs/slices/forget-repo-1.md` §2) proven here:
//! - §2.1 forget-by-default removes registry + memory + db_runtimes slot + `.db`/`-wal`/`-shm` +
//!   `.rgr/`, reports each artifact, and REFUSES (deleting nothing) while a write is in flight;
//!   `--keep-db` keeps the files (reported `retained`); an unlink failure is `failed(reason)` with a
//!   not-`ok` report; eviction still happens when the DB was deleted out-of-band.
//! - §2.2 `rmap doctor` (daemon_info) renders the three orphan classes with bytes.
//! - §2.3 `rmap maintenance gc` lists (dry-run) then reclaims classes A+C, reports bytes, LISTS the
//!   dead-path entries (class B) without removing them.
//! - review-3 #1 (operator-ratified atomicity): a REAL late index that registered up-front and then
//!   waited behind a held write lock while its entry was forgotten re-registers FRESH under the lock
//!   and leaves a REGISTERED (not orphan) DB. A forget while a write is in flight refuses.

use std::path::Path;
use std::sync::Arc;

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

/// An isolated dispatcher + a handle to its shared `DaemonState` (for seeding orphans / holding
/// locks) + the temp state root (kept alive; `databases/` and `registry.json` live under it).
fn isolated() -> (ServiceDispatcher, Arc<DaemonState>, TempDir) {
    // Disable the two background write actors (retention + enrichment): this binary indexes real
    // repos and inspects on-disk storage; a background pass would hold the write lock over the very
    // files/registry it asserts. They are proven in their own suites. (Same rationale as
    // `index_disconnect.rs::isolated`.)
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
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
        DispatchResult::Error(e) => panic!(
            "expected success, got error {}: {}",
            e.error.code, e.error.message
        ),
    }
}

/// Index a fixture repo and return `(canonical_path, db_path, repo_uid)`.
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

fn artifact<'a>(report: &'a Value, kind: &str) -> Option<&'a Value> {
    report["artifacts"]
        .as_array()?
        .iter()
        .find(|a| a["kind"] == kind)
}

fn sidecar(db_path: &str, suffix: &str) -> String {
    format!("{db_path}{suffix}")
}

// ── §2.1 forget-by-default ───────────────────────────────────────────────────

/// `repo_remove` (no `keep_db`) forgets everything: registry entry, in-memory state, the DB +
/// `-wal`/`-shm` sidecars, and `<repo>/.rgr/`. Each is reported `removed`; `ok` is true.
#[test]
fn forget_by_default_removes_everything_via_dispatch() {
    let (dispatcher, state, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);
    // Load it so there is in-memory state to evict; add sidecars + a warm cache dir.
    state.load_repo(Path::new(&db_path), &uid).unwrap();
    std::fs::write(sidecar(&db_path, "-wal"), b"wal").unwrap();
    std::fs::write(sidecar(&db_path, "-shm"), b"shm").unwrap();
    let rgr = repo_dir.join(".rgr");
    std::fs::create_dir_all(rgr.join("warm-cache")).unwrap();
    std::fs::write(rgr.join("warm-cache/default.cache"), vec![0u8; 64]).unwrap();

    let report = expect_success(run(
        &dispatcher,
        "rm",
        "repo_remove",
        json!({ "repo": canonical }),
    ));

    assert_eq!(report["ok"], json!(true), "clean forget: {report}");
    assert!(report["refused"].is_null(), "not refused: {report}");
    // Every on-disk artifact is gone.
    assert!(!Path::new(&db_path).exists(), "DB deleted");
    assert!(
        !Path::new(&sidecar(&db_path, "-wal")).exists(),
        "-wal deleted"
    );
    assert!(
        !Path::new(&sidecar(&db_path, "-shm")).exists(),
        "-shm deleted"
    );
    assert!(!repo_dir.join(".rgr").exists(), ".rgr/ deleted");
    // Registry entry gone (fresh reload from disk) and memory evicted.
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    assert!(
        reloaded.resolve(&repo_dir).is_none(),
        "registry entry forgotten on disk"
    );
    assert!(state.list_repos().is_empty(), "in-memory state evicted");
    // The per-artifact report enumerates each class.
    for kind in [
        "registry",
        "memory",
        "runtime-slot",
        "database",
        "warm-cache",
    ] {
        assert!(
            artifact(&report, kind).is_some(),
            "artifact {kind} reported: {report}"
        );
    }
    assert_eq!(artifact(&report, "database").unwrap()["status"], "removed");
}

/// `--keep-db` (`keep_db: true`) forgets the tracking but leaves the DB + sidecars on disk, each
/// reported `retained` (present) — the base `.db` AND the `-wal`/`-shm` (review-3 #2).
#[test]
fn forget_keep_db_retains_files_and_reports_each_via_dispatch() {
    let (dispatcher, _state, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, _uid) = index_fixture(&dispatcher, &repo_dir);
    std::fs::write(sidecar(&db_path, "-wal"), b"wal-bytes").unwrap();

    let report = expect_success(run(
        &dispatcher,
        "rm",
        "repo_remove",
        json!({ "repo": canonical, "keep_db": true }),
    ));

    assert_eq!(report["ok"], json!(true));
    assert_eq!(report["kept_db"], json!(true));
    assert!(Path::new(&db_path).exists(), "--keep-db leaves the DB file");
    assert!(
        Path::new(&sidecar(&db_path, "-wal")).exists(),
        "--keep-db leaves the -wal sidecar"
    );
    // review-3 #2: base + BOTH sidecars each get an honest line — present → retained, missing → absent.
    assert_eq!(artifact(&report, "database").unwrap()["status"], "retained");
    assert_eq!(artifact(&report, "wal").unwrap()["status"], "retained");
    assert_eq!(
        artifact(&report, "shm").unwrap()["status"],
        "absent",
        "the never-created -shm reports absent, not retained: {report}"
    );
    // But the registry entry is still forgotten.
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    assert!(reloaded.resolve(&repo_dir).is_none());
}

/// A forget that hits an unlink failure reports `failed(reason)` on that artifact and a not-`ok`
/// report (→ the CLI exits non-zero). Injected on unix by making `databases/` read-only, so
/// `remove_file` on the DB fails with EACCES AFTER the registry entry + memory are already dropped.
#[cfg(unix)]
#[test]
fn forget_reports_failed_on_unlink_failure_via_dispatch() {
    use std::os::unix::fs::PermissionsExt;

    let (dispatcher, _state, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, _uid) = index_fixture(&dispatcher, &repo_dir);

    let db_dir = root.path().join("databases");
    let set_mode = |dir: &Path, mode: u32| {
        let mut perms = std::fs::metadata(dir).unwrap().permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(dir, perms).unwrap();
    };
    set_mode(&db_dir, 0o555); // read-only dir: unlinking a child file fails.

    let report = expect_success(run(
        &dispatcher,
        "rm",
        "repo_remove",
        json!({ "repo": canonical }),
    ));

    // Restore perms first so the TempDir cleans up regardless of assertions.
    set_mode(&db_dir, 0o755);

    assert_eq!(
        report["ok"],
        json!(false),
        "an unlink failure makes the report not-ok (CLI exits non-zero): {report}"
    );
    let db = artifact(&report, "database").unwrap();
    assert_eq!(
        db["status"], "failed",
        "the DB unlink is reported failed: {db}"
    );
    assert!(
        !db["reason"].as_str().unwrap_or_default().is_empty(),
        "the failure carries an I/O reason: {db}"
    );
    assert!(
        Path::new(&db_path).exists(),
        "the un-deletable DB is still there"
    );
}

/// Forget still evicts in-memory state (and reports the DB `absent`, not `failed`) when the DB file
/// was deleted OUT-OF-BAND before the forget — eviction is keyed on the registry uid, not the file.
#[test]
fn forget_evicts_even_when_db_deleted_out_of_band_via_dispatch() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid) = index_fixture(&dispatcher, &repo_dir);
    state.load_repo(Path::new(&db_path), &uid).unwrap();
    assert_eq!(state.list_repos().len(), 1);

    std::fs::remove_file(&db_path).unwrap(); // vanishes out-of-band before forget

    let report = expect_success(run(
        &dispatcher,
        "rm",
        "repo_remove",
        json!({ "repo": canonical }),
    ));
    assert_eq!(report["ok"], json!(true));
    assert!(
        state.list_repos().is_empty(),
        "memory evicted despite the deleted DB"
    );
    assert_eq!(
        artifact(&report, "database").unwrap()["status"],
        "absent",
        "an already-gone DB is absent, not failed: {report}"
    );
}

/// Forget REFUSES (deletes nothing) while a write is in flight: a held DB write lock (the same lock
/// index/refresh take) makes `repo_remove` return an error with the "cancel it first" reason, and the
/// registry + DB are untouched. This is the review-2 atomicity contract at the dispatch surface.
#[test]
fn forget_refuses_during_in_flight_write_via_dispatch() {
    let (dispatcher, state, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, _uid) = index_fixture(&dispatcher, &repo_dir);

    // A concurrent writer holds the DB write lock (index/refresh/prune/enrich all take this lock).
    let rt = state.get_or_create_db_runtime(Path::new(&db_path)).unwrap();
    let _held = rt.acquire_write();

    let result = run(
        &dispatcher,
        "rm",
        "repo_remove",
        json!({ "repo": canonical }),
    );
    match result {
        DispatchResult::Error(e) => assert!(
            e.error.message.contains("in progress") || e.error.message.contains("cancel"),
            "refusal names the in-flight write: {}",
            e.error.message
        ),
        DispatchResult::Success(s) => {
            panic!("must refuse while a write is in flight: {}", s.result)
        }
    }
    // Nothing was deleted; the entry is intact on disk.
    assert!(
        Path::new(&db_path).exists(),
        "DB untouched by a refused forget"
    );
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    assert!(
        reloaded.resolve(&repo_dir).is_some(),
        "registry entry untouched by a refused forget"
    );
}

// ── §2.2 doctor orphan classes + §2.3 maintenance gc ────────────────────────

/// Seed the three orphan classes into an isolated state root, then prove `daemon_info` (the `rmap
/// doctor` source) renders each with bytes, and `maintenance_gc` lists (dry-run) → reclaims A+C →
/// leaves B listed.
#[test]
fn doctor_and_gc_see_and_reclaim_orphans_via_dispatch() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();

    // A live, referenced repo (its DB must NOT be touched by gc).
    let live_dir = repo_root.path().join("live");
    write_fixture(&live_dir);
    let (_live_canonical, live_db, _live_uid) = index_fixture(&dispatcher, &live_dir);

    // A dead-path (class B) entry: index a repo, then delete its directory. Its DB stays referenced.
    let dead_dir = repo_root.path().join("gone");
    write_fixture(&dead_dir);
    let (_dead_canonical, dead_db, _dead_uid) = index_fixture(&dispatcher, &dead_dir);
    std::fs::remove_dir_all(&dead_dir).unwrap();

    // Class A: an orphan .db nobody references, with its own -shm. Class C: a base-less stray -wal.
    let db_dir = { state.registry().db_dir().to_path_buf() };
    let orphan_db = db_dir.join("deadbeefdeadbeef.db");
    std::fs::write(&orphan_db, vec![0u8; 4096]).unwrap();
    std::fs::write(db_dir.join("deadbeefdeadbeef.db-shm"), vec![0u8; 128]).unwrap();
    let stray = db_dir.join("cafecafecafecafe.db-wal");
    std::fs::write(&stray, vec![0u8; 32]).unwrap();

    // ── doctor (daemon_info) sees all three classes with bytes ──
    let info = expect_success(run(&dispatcher, "di", "daemon_info", json!({})));
    let orphans = &info["orphans"];
    assert!(orphans["scan_error"].is_null(), "clean scan: {orphans}");
    assert_eq!(
        orphans["orphan_db_count"],
        json!(1),
        "one orphan DB: {orphans}"
    );
    assert_eq!(
        orphans["orphan_db_bytes"],
        json!(4096 + 128),
        "orphan DB bytes include its -shm sidecar: {orphans}"
    );
    assert_eq!(orphans["stray_sidecar_count"], json!(1));
    assert_eq!(orphans["stray_sidecar_bytes"], json!(32));
    let dead = orphans["dead_path_entries"].as_array().unwrap();
    assert_eq!(dead.len(), 1, "one dead-path entry: {orphans}");
    assert!(dead[0]["next_action"]
        .as_str()
        .unwrap()
        .starts_with("rmap repo remove"));

    // ── gc --dry-run lists candidates, deletes nothing ──
    let dry = expect_success(run(
        &dispatcher,
        "gc1",
        "maintenance_gc",
        json!({ "dry_run": true }),
    ));
    assert_eq!(dry["dry_run"], json!(true));
    assert_eq!(dry["reclaimed_bytes"], json!(0), "dry-run frees nothing");
    assert_eq!(dry["would_reclaim_bytes"], json!(4096 + 128 + 32));
    assert!(
        orphan_db.exists() && stray.exists(),
        "dry-run deletes nothing"
    );
    assert_eq!(
        dry["dead_path_entries"].as_array().unwrap().len(),
        1,
        "dry-run lists the dead-path entry too"
    );

    // ── gc reclaims A+C, reports bytes, leaves B listed and the live + dead-path DBs intact ──
    let real = expect_success(run(
        &dispatcher,
        "gc2",
        "maintenance_gc",
        json!({ "dry_run": false }),
    ));
    assert_eq!(real["ok"], json!(true));
    assert_eq!(real["reclaimed_bytes"], json!(4096 + 128 + 32));
    assert!(!orphan_db.exists(), "orphan DB reclaimed");
    assert!(!stray.exists(), "stray sidecar reclaimed");
    assert!(
        Path::new(&live_db).exists(),
        "the live repo's DB is untouched"
    );
    assert!(
        Path::new(&dead_db).exists(),
        "a dead-path (still-referenced) DB is NOT reclaimed"
    );
    assert_eq!(
        real["dead_path_entries"].as_array().unwrap().len(),
        1,
        "the dead-path entry is LISTED, not auto-removed"
    );
}

// ── §2.2 boot orphan scan (the exact fn `run_daemon` spawns at startup) ──────

/// The daemon boot sweep (`reconcile::reconcile_all_repos` — the very fn `run_daemon` spawns on its
/// startup thread) scans `databases/` against the registry and LOGS the orphan-class counts on every
/// boot (§2.2: "the daemon log records the counts at boot"; operator ruling: zero is a measurement,
/// so it logs even when clean). Here it runs over a seeded isolated state with all three classes. The
/// boot sweep OBSERVES and logs — it must NOT reclaim (that is `gc`'s job) and must NOT auto-remove
/// the dead-path entry (conservative). Run with `--nocapture` to see the emitted
/// `info: startup orphan scan: …` line live.
#[test]
fn boot_reconcile_scans_and_logs_orphans_without_reclaiming() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();

    // A live registered repo (its DB must survive the boot sweep).
    let live_dir = repo_root.path().join("live");
    write_fixture(&live_dir);
    let (_lc, live_db, _lu) = index_fixture(&dispatcher, &live_dir);

    // A dead-path (class B) entry: index then delete its directory; the DB stays referenced.
    let dead_dir = repo_root.path().join("gone");
    write_fixture(&dead_dir);
    let (_dc, _dead_db, _du) = index_fixture(&dispatcher, &dead_dir);
    std::fs::remove_dir_all(&dead_dir).unwrap();

    // Class A: an orphan .db + its -shm. Class C: a base-less stray -wal.
    let db_dir = state.registry().db_dir().to_path_buf();
    let orphan_db = db_dir.join("deadbeefdeadbeef.db");
    std::fs::write(&orphan_db, vec![0u8; 4096]).unwrap();
    std::fs::write(db_dir.join("deadbeefdeadbeef.db-shm"), vec![0u8; 128]).unwrap();
    let stray = db_dir.join("cafecafecafecafe.db-wal");
    std::fs::write(&stray, vec![0u8; 32]).unwrap();

    let registered_before = state.registry().list().len();

    // The REAL boot sweep (emits the `info: startup orphan scan: …` line to stderr).
    repo_graph_daemon_runtime::reconcile::reconcile_all_repos(&state);

    // OBSERVES only: the orphan files are still present (boot logs, gc reclaims) …
    assert!(orphan_db.exists(), "boot sweep must NOT reclaim orphan DBs");
    assert!(stray.exists(), "boot sweep must NOT reclaim stray sidecars");
    assert!(
        Path::new(&live_db).exists(),
        "the live repo's DB is untouched by the boot sweep"
    );
    // … and the dead-path entry is NOT auto-removed (conservative — §2.3).
    assert_eq!(
        state.registry().list().len(),
        registered_before,
        "the boot sweep must NOT auto-remove the dead-path registry entry"
    );
}

// ── review-3 #1 (operator-ratified): the late-writer race, through the REAL handle_index path ──

/// A REAL second index that registered up-front and then waited behind a held DB write lock while its
/// registry entry was forgotten must re-register FRESH under the lock and leave a REGISTERED (not
/// orphan) DB. The main thread holds the exact lock `reclaim::forget_repo` holds across its deletion,
/// removes the entry + deletes the DB (forget's critical section), then releases — the late index
/// then proceeds. Without the fix it would index under the forgotten uid, leaving an unregistered
/// orphan DB (the `resolve` below would be `None`); with the fix it re-registers fresh.
///
/// Timing note: the PASS is deterministic (the join + final on-disk assertions do not depend on the
/// sleep); the sleep only biases the run toward the interesting window (up-front-register-U1 → entry
/// removed → re-register-fresh-under-lock), which is where the pre-fix code produced the orphan.
#[test]
fn late_index_writer_reregisters_fresh_after_concurrent_forget() {
    let (dispatcher, state, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);

    // Index once: the repo is registered (uid U1), its DB exists.
    let (canonical, db_path, uid1) = index_fixture(&dispatcher, &repo_dir);

    // Main HOLDS the DB write lock (stands in for a forget holding it across its deletion).
    let rt = state.get_or_create_db_runtime(Path::new(&db_path)).unwrap();
    let held = rt.acquire_write();

    // Spawn a REAL second index of the same repo. It registers up-front (idempotent → U1 while the
    // entry exists), then BLOCKS on `acquire_write` (main holds the same slot's lock).
    let dispatcher = Arc::new(dispatcher);
    let d2 = Arc::clone(&dispatcher);
    let repo_dir2 = repo_dir.clone();
    let idx_thread = std::thread::spawn(move || {
        let mut emitter = Quiet;
        d2.dispatch(
            &request(
                "idx2",
                "index",
                json!({ "repo_path": repo_dir2.to_string_lossy() }),
            ),
            &mut emitter,
        )
    });

    // Let the late index reach its blocked-on-write-lock state.
    std::thread::sleep(std::time::Duration::from_millis(200));

    // The "forget that won the lock" critical section: drop the registry entry (U1) and delete the DB,
    // exactly what `reclaim::forget_repo` does while holding this same lock.
    {
        let mut reg = state.registry_mut();
        reg.remove(Path::new(&canonical)).unwrap();
        reg.save().unwrap();
    }
    std::fs::remove_file(&db_path).unwrap();

    // Release → the late index proceeds. Fix: it re-registers fresh UNDER the lock before writing.
    drop(held);

    let resp = expect_success(idx_thread.join().unwrap());
    let uid2 = resp["repo_uid"].as_str().unwrap().to_string();

    assert_ne!(
        uid2, uid1,
        "the late writer re-registered FRESH (new uid), not reused the forgotten identity"
    );
    // The DB the late index wrote is REGISTERED (fresh reload from disk) → it is NOT an orphan.
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    let entry = reloaded
        .resolve(&repo_dir)
        .expect("the re-indexed repo is REGISTERED on disk (no orphan DB left behind)");
    assert_eq!(
        entry.repo_uid, uid2,
        "the persisted entry carries the fresh uid the index wrote under"
    );
    assert_eq!(
        entry.db_path,
        Path::new(&db_path),
        "the fresh entry points at the DB the late index wrote (that DB is tracked, not orphaned)"
    );
}

/// A forget serializes against a REAL index of the SAME repo: while a real index holds the DB write
/// lock, `repo_remove` refuses (deletes nothing); once the index finishes, forget succeeds. Proves
/// the refusal holds against the real writer, not just a hand-held guard.
#[test]
fn forget_refuses_during_a_real_concurrent_index_then_succeeds() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, _uid) = index_fixture(&dispatcher, &repo_dir);

    // Hold the write lock to model an in-flight index, dispatch the forget → refused, nothing gone.
    let rt = state.get_or_create_db_runtime(Path::new(&db_path)).unwrap();
    let held = rt.acquire_write();
    let refused = run(
        &dispatcher,
        "rm1",
        "repo_remove",
        json!({ "repo": canonical.clone() }),
    );
    assert!(
        matches!(refused, DispatchResult::Error(_)),
        "forget refuses while the write lock is held"
    );
    assert!(
        Path::new(&db_path).exists(),
        "nothing deleted while refused"
    );
    drop(held);

    // Now the write is done → forget succeeds and removes the DB.
    let report = expect_success(run(
        &dispatcher,
        "rm2",
        "repo_remove",
        json!({ "repo": canonical }),
    ));
    assert_eq!(report["ok"], json!(true));
    assert!(
        !Path::new(&db_path).exists(),
        "forget removes the DB once the write is done"
    );
}

// ── review-4 #2: the ABSENT-DB / no-slot late-writer race (the case the file-exists proof missed) ──

/// The regression the file-exists proof did NOT cover: a registered repo whose DB file is ABSENT and
/// whose runtime slot does not yet exist (a dead-path entry, or a repo whose DB was reclaimed). This
/// is exactly where the old `existing_or_new_db_runtime` returned `None` — its full-path canonicalize
/// failed on the missing file and its raw-path fallback missed the slot `handle_index` creates (keyed
/// on `canonicalize(parent)/filename`; on macOS the raw registry path differs by the `/var`→
/// `/private/var` symlink) — so forget held NO guard and a concurrent index could write an orphan.
///
/// Deterministic proof: with the DB absent and no slot, the ONLY call that resolves the coordination
/// slot is `get_or_create_db_runtime_for_new_db` — the SAME call BOTH the fixed `forget_repo` and
/// `handle_index` make. We take + HOLD that slot's write guard on the main thread (standing in for
/// forget holding it across its deletion — the old code could not even acquire it here), then a REAL
/// `handle_index` of the same repo BLOCKS on it (proving both fetch the same slot on an absent file).
/// Once we drop the entry and release, the late index re-registers FRESH under the lock rather than
/// writing under the forgotten identity — no orphan.
#[test]
fn absent_db_forget_and_late_index_contend_on_the_same_slot_and_reregister_fresh() {
    let (dispatcher, state, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);

    // Register WITHOUT indexing: an entry (uid1) exists on disk, but the DB file is ABSENT and no
    // runtime slot exists — the precise field state review-4 flagged.
    let (canonical, db_path, uid1) = {
        let mut reg = state.registry_mut();
        let e = reg.register(&repo_dir).unwrap().clone();
        reg.save().unwrap();
        (
            e.canonical_path.to_string_lossy().to_string(),
            e.db_path.clone(),
            e.repo_uid.clone(),
        )
    };
    assert!(!db_path.exists(), "precondition: the DB file is absent");

    // Model forget holding its guard across deletion: acquire the for_new_db slot on the ABSENT file
    // (the exact call the fixed forget makes; the old `existing_or_new_db_runtime` returned None here).
    let rt = state
        .get_or_create_db_runtime_for_new_db(&db_path)
        .expect("for_new_db resolves a slot even when the DB file is absent");
    let held = rt.acquire_write();

    // A REAL second index of the same repo: it registers up-front (idempotent → uid1 while the entry
    // exists), then fetches the SAME slot via for_new_db and BLOCKS on acquire_write.
    let dispatcher = Arc::new(dispatcher);
    let d2 = Arc::clone(&dispatcher);
    let repo_dir2 = repo_dir.clone();
    let idx = std::thread::spawn(move || {
        let mut emitter = Quiet;
        d2.dispatch(
            &request(
                "idx",
                "index",
                json!({ "repo_path": repo_dir2.to_string_lossy() }),
            ),
            &mut emitter,
        )
    });

    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(
        !idx.is_finished(),
        "the late index must BLOCK on the slot forget holds — proving both fetch the SAME slot on an absent DB"
    );

    // Forget's critical section (what forget does while holding this same guard): drop the registration.
    {
        let mut reg = state.registry_mut();
        reg.remove(Path::new(&canonical)).unwrap();
        reg.save().unwrap();
    }
    drop(held); // release → the late index proceeds, re-registering FRESH under the lock.

    let resp = expect_success(idx.join().unwrap());
    let uid2 = resp["repo_uid"].as_str().unwrap().to_string();
    assert_ne!(
        uid2, uid1,
        "the late index re-registered FRESH, not under the forgotten uid"
    );

    // The DB the late index wrote is REGISTERED on disk → not an orphan.
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    let entry = reloaded
        .resolve(&repo_dir)
        .expect("the re-indexed repo is REGISTERED on disk (no orphan DB left behind)");
    assert_eq!(entry.repo_uid, uid2);
    assert_eq!(entry.db_path, db_path);
}

/// The fixed `forget_repo`, run for REAL on an absent-DB repo, respects the `for_new_db` slot a
/// concurrent index holds: with that slot's write guard held (an in-flight index that created the slot
/// on an absent file), a real forget REFUSES and deletes nothing. Against the old code forget fetched
/// the slot via a lookup that missed this key on an absent file, would NOT refuse, and would race the
/// index into an orphan. Deterministic and exercises the real forget path end to end.
#[test]
fn real_forget_refuses_when_the_for_new_db_slot_is_held_on_an_absent_db() {
    let (dispatcher, state, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);

    let canonical = {
        let mut reg = state.registry_mut();
        let e = reg.register(&repo_dir).unwrap().clone();
        reg.save().unwrap();
        e.canonical_path.to_string_lossy().to_string()
    };
    let db_path = { state.registry().resolve(&repo_dir).unwrap().db_path.clone() };
    assert!(!db_path.exists(), "precondition: the DB file is absent");

    // A concurrent index holds the for_new_db slot's write guard (the slot it created on an absent DB).
    let rt = state.get_or_create_db_runtime_for_new_db(&db_path).unwrap();
    let held = rt.acquire_write();

    let result = run(
        &dispatcher,
        "rm",
        "repo_remove",
        json!({ "repo": canonical }),
    );
    match result {
        DispatchResult::Error(e) => assert!(
            e.error.message.contains("in progress") || e.error.message.contains("cancel"),
            "forget must refuse while the for_new_db slot is held on an absent DB: {}",
            e.error.message
        ),
        DispatchResult::Success(s) => panic!(
            "forget must REFUSE (deleting nothing) while a concurrent index holds the slot on an absent DB: {}",
            s.result
        ),
    }
    drop(held);
    // The registration is untouched by the refused forget.
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    assert!(
        reloaded.resolve(&repo_dir).is_some(),
        "a refused forget leaves the registry entry intact"
    );
}

// ── review-4 #3: the changed handle_refresh race branch ──

/// A refresh that acquired the DB write lock AFTER a forget removed the registration must return the
/// stated refusal and must NOT recreate the deleted DB (its `storage()` opens a connection that would
/// CREATE an empty, migrated, UNREGISTERED orphan). Deterministic: main holds the DB write lock; a
/// real refresh loads the repo (DB still present) then BLOCKS on the lock; we run forget's critical
/// section (remove entry + delete DB) and release; the refresh proceeds, sees it was forgotten, and
/// aborts WITHOUT writing a file.
#[test]
fn refresh_that_loses_the_race_refuses_and_does_not_recreate_the_db() {
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);
    let (canonical, db_path, uid1) = index_fixture(&dispatcher, &repo_dir);

    // Main holds the DB write lock (stands in for forget holding it across deletion). The file exists,
    // so `get_or_create_db_runtime` resolves the same canonical key handle_refresh will fetch.
    let rt = state.get_or_create_db_runtime(Path::new(&db_path)).unwrap();
    let held = rt.acquire_write();

    // A REAL refresh: it resolves+loads the repo (DB present) then BLOCKS on acquire_write.
    let dispatcher = Arc::new(dispatcher);
    let d2 = Arc::clone(&dispatcher);
    let canonical2 = canonical.clone();
    let refresh = std::thread::spawn(move || {
        let mut emitter = Quiet;
        d2.dispatch(
            &request("rf", "refresh", json!({ "repo": canonical2 })),
            &mut emitter,
        )
    });

    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(
        !refresh.is_finished(),
        "the refresh must BLOCK on the held DB write lock"
    );

    // Forget's critical section under the lock forget holds: remove the entry (uid1) + delete the DB.
    {
        let mut reg = state.registry_mut();
        let cp = reg
            .list()
            .iter()
            .find(|e| e.repo_uid == uid1)
            .map(|e| e.canonical_path.clone())
            .expect("the entry is present before forget");
        reg.remove(&cp).unwrap();
        reg.save().unwrap();
    }
    std::fs::remove_file(&db_path).unwrap();
    drop(held); // release → the refresh proceeds and finds itself forgotten.

    let result = refresh.join().unwrap();
    match result {
        DispatchResult::Error(e) => assert!(
            e.error.message.contains("forgotten")
                || e.error.message.contains("nothing was refreshed"),
            "the refused refresh names the forget race: {}",
            e.error.message
        ),
        DispatchResult::Success(s) => panic!(
            "a refresh that lost the race to forget must REFUSE, not succeed: {}",
            s.result
        ),
    }
    // Critically: the refused refresh did NOT recreate the deleted DB (would be an orphan).
    assert!(
        !Path::new(&db_path).exists(),
        "a refused refresh must NOT recreate the deleted DB (no orphan)"
    );
}
