//! FOREGROUND-LOCK-1 (§4 reproducing proof, END-TO-END): a held write transaction on a repo's DB
//! + a foreground dispatch, driven through the REAL `ServiceDispatcher::dispatch` surface.
//!
//! This is the product-side of the test-flake family (root-caused 5535092): before the fix a
//! foreground open under a concurrent write lock failed IMMEDIATELY with
//! `InternalError: … database is locked`. After the fix a lock held BEYOND the short foreground
//! patience budget surfaces as the honest `Busy` transient — never `InternalError`.
//!
//! Shares the isolated dispatcher/real-git harness in `tests/seed_harness/mod.rs`.

mod seed_harness;
use seed_harness::*;

use serde_json::json;
use std::path::Path;
use std::time::Duration;

/// Hold a raw SQLite WRITE lock on `db_path` for `hold`, on a detached thread. `BEGIN IMMEDIATE`
/// takes the reserved lock at once, so a concurrent foreground `open_existing` migration-check
/// write hits `SQLITE_BUSY` immediately — the production shape. Returns once the lock is held.
fn hold_write_lock(db_path: &Path, hold: Duration) {
    let path = db_path.to_path_buf();
    let started = std::sync::Arc::new(std::sync::Barrier::new(2));
    let started_c = started.clone();
    std::thread::spawn(move || {
        let conn = rusqlite::Connection::open(&path).expect("raw open");
        conn.execute_batch("BEGIN IMMEDIATE; CREATE TABLE IF NOT EXISTS _lk(x);")
            .expect("acquire write lock");
        started_c.wait();
        std::thread::sleep(hold);
        let _ = conn.execute_batch("COMMIT;");
    });
    started.wait();
}

/// Index `repo` in `d`, hold a write lock BEYOND the foreground patience budget, then dispatch
/// `method`/`params`. Returns the resulting error object + the store path so each case can assert
/// the honest-`Busy` shape. Shared by every foreground request path exercised below.
fn busy_error_under_held_lock(
    d: &repo_graph_daemon_runtime::ServiceDispatcher,
    repo: &tempfile::TempDir,
    method: &str,
    params: serde_json::Value,
) -> (serde_json::Value, String) {
    let idx = dispatch_ok(
        d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, _uid) = coords(&idx);
    // Hold the write lock across the WHOLE foreground budget (450ms) plus margin.
    hold_write_lock(Path::new(&db_path), Duration::from_millis(1500));
    let err = dispatch_error(d, method, params);
    (err, db_path)
}

/// The exhausted-patience contract (§2.2): a transient lock renders as `Busy` (never
/// `InternalError`), with a safe-to-retry message that carries the store path.
fn assert_busy_transient(err: &serde_json::Value, db_path: &str) {
    assert_eq!(
        err["code"], "Busy",
        "a transient lock must surface as Busy, never InternalError: {err}"
    );
    // The rendered protocol message is CLASSIFIED here, so an absent/non-string `message` is a
    // test FAILURE, not a silently-empty string (STANDING HONESTY RULE 1 — never `unwrap_or_default`
    // on a read whose result is asserted on).
    let msg = err["message"]
        .as_str()
        .unwrap_or_else(|| panic!("Busy error must carry a string `message`, got: {err}"));
    assert!(
        msg.contains("retry"),
        "must state the next move (retry): {err}"
    );
    assert!(msg.contains(db_path), "must include the store path: {err}");
    assert!(
        !msg.contains("InternalError"),
        "a lock transient must never render InternalError: {err}"
    );
}

/// An INLINE-in-`dispatch.rs` foreground read (`callers`, via `ServiceDispatcher::open_storage`)
/// whose storage open contends with a lock held beyond the foreground budget surfaces `Busy`.
#[test]
fn foreground_inline_read_under_held_lock_is_busy_not_internal_error() {
    let (d, _root) = isolated_quiet(); // background passes off — the ONLY writer is our test lock
    let repo = make_repo(); // main.ts's mainEntry calls helper.ts's helperFunction
    let (err, db_path) = busy_error_under_held_lock(
        &d,
        &repo,
        "callers",
        json!({ "repo": repo.path().to_string_lossy(), "symbol": "helperFunction" }),
    );
    assert_busy_transient(&err, &db_path);
}

/// An EXTRACTED foreground read handler (`handlers/map.rs`, via
/// `DaemonState::open_repo_storage_for_request`) — the seam the prior build left rendering
/// `InternalError` — now surfaces `Busy` under a held lock too.
#[test]
fn foreground_extracted_read_under_held_lock_is_busy_not_internal_error() {
    let (d, _root) = isolated_quiet();
    let repo = make_repo();
    let (err, db_path) = busy_error_under_held_lock(
        &d,
        &repo,
        "map",
        json!({ "repo": repo.path().to_string_lossy() }),
    );
    assert_busy_transient(&err, &db_path);
}

/// `find` — the EXACT foreground read the flake family root-caused (§1.2: the retention pass held
/// the DB while a `find` open failed fast). It routes through `ServiceDispatcher::open_storage` too;
/// a held lock now surfaces `Busy`, not `InternalError`.
#[test]
fn foreground_find_under_held_lock_is_busy_not_internal_error() {
    let (d, _root) = isolated_quiet();
    let repo = make_repo();
    let (err, db_path) = busy_error_under_held_lock(
        &d,
        &repo,
        "find",
        json!({ "repo": repo.path().to_string_lossy(), "query": "helperFunction" }),
    );
    assert_busy_transient(&err, &db_path);
}

/// An EXTRACTED foreground WRITE handler (`handlers/governance/assess.rs`) — which opens storage
/// TWICE (a read pre-check + a second open for the write) — surfaces `Busy` on a held lock. The
/// first open (the shared choke) trips first, proving the extracted write path is covered. The
/// second open was migrated to the SPLIT choke (`open_repo_storage_for_request_split`) so it too
/// re-codes a transient lock as `Busy` while preserving its own "storage open failed: …" non-lock
/// message — the lock→`Busy` and the non-lock→raw halves of that split are unit-covered in
/// `foreground_open::tests` (a held-lock e2e cannot isolate the second open, since the first trips).
#[test]
fn foreground_extracted_write_under_held_lock_is_busy_not_internal_error() {
    let (d, _root) = isolated_quiet();
    let repo = make_repo();
    let (err, db_path) = busy_error_under_held_lock(
        &d,
        &repo,
        "assess",
        json!({ "repo": repo.path().to_string_lossy() }),
    );
    assert_busy_transient(&err, &db_path);
}

// ── DAEMON-RESIDUALS-1 (D1-A): the IN-PROCESS write-mutex layer, through the real dispatcher ──
//
// The held-*file*-lock cases above trip the storage open (§2.2) and never reach D1-A's new branch:
// a foreground WRITE handler takes `acquire_foreground_write` (the DB write mutex + coordinator
// refresh) as its FIRST lock op, BEFORE opening storage. These cases hold the daemon's IN-PROCESS
// `DatabaseState::write_lock` (the exact #2 block site) and drive `assess`/`coverage` through the
// real dispatcher, proving the handler re-codes the contention as an honest, HOLDER-NAMED `Busy`
// (with the ratified "started Nm ago" elapsed) within the bounded patience — never a block to the
// 300s symptom, never `InternalError`.

use repo_graph_daemon_runtime::activity::OpKind;

/// Index `repo` in a dispatcher whose `DaemonState` we also hold, stamp an in-flight `index` on its
/// DB (so the Busy can name a holder + elapsed), hold the IN-PROCESS write mutex, then dispatch
/// `method`/`params`. Asserts the bounded, holder-named honest-`Busy` D1-A contract.
fn assert_write_busy_under_held_inprocess_lock(
    method: &str,
    make_params: impl Fn(&tempfile::TempDir) -> serde_json::Value,
) {
    let (d, state, _root) = isolated_quiet_with_state();
    let repo = make_repo();
    let idx = dispatch_ok(
        &d,
        "index",
        json!({ "repo_path": repo.path().to_string_lossy() }),
    );
    let (db_path, _uid) = coords(&idx);

    // The SAME `Arc<DatabaseState>` the handler resolves (cached by canonical path).
    let db_runtime = state
        .get_or_create_db_runtime(Path::new(&db_path))
        .expect("db runtime for the indexed repo");
    // Stamp an in-flight index on this DB so the Busy names the holder class + elapsed (D1 wording).
    // Key it on the runtime's CANONICAL db path — the exact path the handler passes to `busy_message`
    // (`repo_state.db_path()`), which `ActivityRegistry::active_for_db` matches by exact PathBuf. The
    // index-response `db_path` is the pre-canonical temp path (`/var/…` vs `/private/var/…` on macOS),
    // which would silently miss the registry lookup and render the honest-unknown message instead.
    let _op = state.activity().begin(
        OpKind::Index,
        repo.path().to_string_lossy().to_string(),
        None,
        db_runtime.db_path().to_path_buf(),
    );
    // Hold the in-process write mutex — the exact #2 block site, contended BEFORE any storage open.
    let _held = db_runtime.acquire_write();

    let start = std::time::Instant::now();
    let err = dispatch_error(&d, method, make_params(&repo));
    // Bounded: the handler waits at most FOREGROUND_WRITE_PATIENCE (3s) then returns Busy — it does
    // NOT block up to the 300s client-timeout SYMPTOM. Generous ceiling to stay non-flaky on CI.
    assert!(
        start.elapsed() < Duration::from_secs(30),
        "foreground write must be bounded (never the 300s symptom), took {:?}",
        start.elapsed()
    );
    assert_busy_transient(&err, &db_path);
    let msg = err["message"]
        .as_str()
        .unwrap_or_else(|| panic!("Busy error must carry a string `message`, got: {err}"));
    assert!(
        msg.contains("index"),
        "must name the holder class (stored activity fact): {err}"
    );
    assert!(
        msg.contains("started") && msg.contains("ago"),
        "must render the holder elapsed (ratified D1 wording 'started Nm ago'): {err}"
    );
}

/// `assess` (governance write) under a held in-process write mutex → honest holder-named `Busy`.
#[test]
fn assess_under_held_inprocess_write_lock_is_named_busy() {
    assert_write_busy_under_held_inprocess_lock(
        "assess",
        |repo| json!({ "repo": repo.path().to_string_lossy() }),
    );
}

/// `coverage` (quality write) under a held in-process write mutex → honest holder-named `Busy`.
/// `coverage` validates a `report_path` file BEFORE the lock, so we hand it a real (dummy) file; the
/// handler returns Busy at the write acquire before it ever parses the report.
#[test]
fn coverage_under_held_inprocess_write_lock_is_named_busy() {
    assert_write_busy_under_held_inprocess_lock("coverage", |repo| {
        let report = repo.path().join("cov.json");
        std::fs::write(&report, "{}").expect("write dummy coverage report");
        json!({
            "repo": repo.path().to_string_lossy(),
            "report_path": report.to_string_lossy(),
        })
    });
}
