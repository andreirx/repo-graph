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
