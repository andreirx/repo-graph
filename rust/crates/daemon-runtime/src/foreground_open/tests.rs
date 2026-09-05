//! Unit coverage for the two seams this module owns: the exhausted-patience MESSAGE
//! ([`busy_message`]) and the classification the wrapper performs on the retry MECHANISM's
//! typed outcome. The full `open_repo_storage_for_request` + real-dispatch reproducing race
//! lives in `tests/foreground_lock_seam.rs` (a held write lock + a real `find` dispatch), which
//! needs an indexed repo the unit layer cannot cheaply build.

use super::*;
use crate::activity::OpKind;
use repo_graph_storage::StorageConnection;
use std::time::{Duration, Instant};
use tempfile::tempdir;

/// Create a real migrated DB file and return its path (kept alive by the returned `TempDir`).
fn migrated_db() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("repo.db");
    // `open` creates + migrates; drop closes it so the file is a valid, unlocked DB.
    let conn = StorageConnection::open(db_path.to_str().unwrap()).expect("create+migrate db");
    drop(conn);
    (dir, db_path)
}

/// Hold a raw SQLite WRITE lock on `db_path` for `hold`, on a detached thread. `BEGIN IMMEDIATE`
/// takes the reserved lock at once, so a concurrent `open_existing` migration-check write hits
/// `SQLITE_BUSY` immediately — the exact production shape. Returns once the lock is held.
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

// ── the exhausted-patience MESSAGE (§2.2) ────────────────────────────────

#[test]
fn busy_message_names_registered_holder_class() {
    let activity = ActivityRegistry::new();
    let db = Path::new("/db/x.db");
    let _g = activity.begin(OpKind::Retention, "/repos/x", None, db.to_path_buf());
    let msg = busy_message(db, &activity);
    // `kind.as_str()` is a STORED fact, not a name guess.
    assert!(
        msg.contains("retention"),
        "must name the holder class: {msg}"
    );
    assert!(msg.contains("retry"), "must state safe-to-retry: {msg}");
    assert!(
        msg.contains("/db/x.db"),
        "must include the store path: {msg}"
    );
    assert!(!msg.contains("InternalError"), "never InternalError: {msg}");
}

#[test]
fn busy_message_is_honest_unknown_with_no_registered_op() {
    let activity = ActivityRegistry::new(); // nothing registered
    let db = Path::new("/db/y.db");
    let msg = busy_message(db, &activity);
    assert!(
        msg.contains("concurrent operation"),
        "unknown holder must render an honest unknown, not a fabricated op: {msg}"
    );
    assert!(msg.contains("retry") && msg.contains("/db/y.db"), "{msg}");
}

// ── the retry MECHANISM + wrapper classification (§2.1 / §2.2) ───────────

#[test]
fn lock_that_clears_within_patience_succeeds() {
    let (_dir, db_path) = migrated_db();
    // Held ~200ms — well inside the 450ms foreground budget.
    hold_write_lock(&db_path, Duration::from_millis(200));
    let start = Instant::now();
    let result = open_existing_with_busy_retry(&db_path, OpenPatience::Foreground);
    assert!(
        result.is_ok(),
        "a lock that clears within patience must succeed, got {:?}",
        result.err().map(|e| e.to_string())
    );
    assert!(
        start.elapsed() < Duration::from_millis(1500),
        "foreground open must not stall for a background-length budget"
    );
}

#[test]
fn lock_held_beyond_patience_is_locked_after_retries_and_bounded() {
    let (_dir, db_path) = migrated_db();
    // Held ~1.2s — outlives the 450ms foreground budget.
    hold_write_lock(&db_path, Duration::from_millis(1200));
    let start = Instant::now();
    let err = open_existing_with_busy_retry(&db_path, OpenPatience::Foreground)
        .expect_err("must exhaust patience");
    assert!(
        matches!(err, OpenError::LockedAfterRetries { .. }),
        "an exhausted lock must classify as a transient, not Other: {err:?}"
    );
    // Foreground budget bound: 3 sleeps × 150ms = 450ms, plus open overhead — sub-second.
    assert!(
        start.elapsed() < Duration::from_millis(900),
        "foreground patience must be sub-second, took {:?}",
        start.elapsed()
    );
}

#[test]
fn missing_db_is_other_immediately() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("does-not-exist.db");
    let start = Instant::now();
    let err = open_existing_with_busy_retry(&db_path, OpenPatience::Foreground)
        .expect_err("missing db must fail");
    assert!(
        matches!(err, OpenError::Other(_)),
        "a non-lock fault must not be retried as a lock: {err:?}"
    );
    assert!(
        start.elapsed() < Duration::from_millis(150),
        "a non-lock fault must surface immediately, not after the retry budget"
    );
}

#[test]
fn background_budget_waits_longer_than_foreground() {
    // Same locked DB, both budgets exhaust; Background (4×250ms) must exceed Foreground
    // (3×150ms). Proves the parameterization actually differs by class.
    let (_dir, db_path) = migrated_db();
    hold_write_lock(&db_path, Duration::from_secs(5));
    let t_fg = {
        let s = Instant::now();
        let _ = open_existing_with_busy_retry(&db_path, OpenPatience::Foreground);
        s.elapsed()
    };
    let t_bg = {
        let s = Instant::now();
        let _ = open_existing_with_busy_retry(&db_path, OpenPatience::Background);
        s.elapsed()
    };
    assert!(
        t_bg > t_fg,
        "background budget must wait longer: bg={t_bg:?} fg={t_fg:?}"
    );
}

// ── §2.3: non-lock message preservation (review-1 item 2) ────────────────

/// A genuine non-lock fault surfaces the RAW storage error (no shared prefix), so a secondary
/// open with a DISTINCT pre-existing message (assess/coverage: "storage open failed: …";
/// enrich: "failed to open storage for enrichment: …") renders it under its OWN prefix via the
/// split path WITHOUT double-prefixing. If `Other` carried the shared prefix (build-1's shape),
/// those callers would emit "storage open failed: failed to open storage connection: …" — the
/// exact §2.3 message-change the reviewer flagged.
#[test]
fn non_lock_fault_carries_raw_error_without_shared_prefix() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("missing.db");
    let err = open_existing_with_busy_retry(&db_path, OpenPatience::Foreground)
        .expect_err("missing db must fail");
    let raw = match err {
        OpenError::Other(raw) => raw,
        other => panic!("a missing db is a non-lock fault, must be Other, got {other:?}"),
    };
    assert!(
        !raw.contains("failed to open storage connection"),
        "Other must carry the RAW error, not the shared prefix (else callers double-prefix): {raw}"
    );
    assert!(
        !raw.is_empty(),
        "raw error must name the underlying cause: {raw}"
    );
    // A split caller re-applies its own prefix exactly once — the §2.3-preserved shape.
    let rendered = format!("storage open failed: {raw}");
    assert!(
        !rendered.contains(": failed to open storage connection:"),
        "the caller's own message must not sit on top of the shared prefix: {rendered}"
    );
}

/// The shared render path — `OpenError`'s `Display`, used by `RepoState::storage()`'s ~140
/// `String` callers and the flat foreground choke `open_repo_storage_for_request` — re-applies
/// the historical "failed to open storage connection: …" prefix, so the PRIMARY-open message is
/// byte-identical to pre-slice even though the raw text now lives in the variant.
#[test]
fn shared_display_reapplies_historical_prefix() {
    let other = OpenError::Other("database disk image is malformed".to_string());
    assert_eq!(
        other.to_string(),
        "failed to open storage connection: database disk image is malformed"
    );
    let locked = OpenError::LockedAfterRetries {
        attempts: 5,
        last: "database is locked".to_string(),
    };
    assert_eq!(
        locked.to_string(),
        "failed to open storage connection: database is locked (after 5 bounded busy-retry attempts)"
    );
}

// ── D1-A: the dual-layer foreground write acquire (DAEMON-RESIDUALS-1) ───

/// The #2 mechanism, re-coded: a foreground write command whose DB write mutex is held by a
/// concurrent pass gets an honest, HOLDER-NAMED `Busy` (never a block up to the 300s symptom,
/// never `InternalError`). The holder class is the STORED activity fact (`index`), not a guess.
#[test]
fn acquire_foreground_write_busy_names_holder_when_db_lock_held() {
    use crate::registry::RepoRegistry;
    use crate::state::DaemonState;
    use std::time::{Duration, Instant};

    let (_dir, db_path) = migrated_db();
    let state = DaemonState::with_registry(RepoRegistry::empty_non_persistent());
    let db_runtime = state
        .get_or_create_db_runtime(&db_path)
        .expect("db runtime");
    let coordinator = RepoCoordinator::new();
    let activity = ActivityRegistry::new();
    // Stamp an in-flight index on this DB so the Busy can name the holder class.
    let _op = activity.begin(
        OpKind::Index,
        "/repos/x",
        None,
        db_runtime.db_path().to_path_buf(),
    );
    // Hold the DB write mutex — the exact #2 block site.
    let _held = db_runtime.acquire_write();

    let start = Instant::now();
    let err = acquire_foreground_write(
        &db_runtime,
        &coordinator,
        &activity,
        db_runtime.db_path(),
        Duration::from_millis(80),
    )
    .err()
    .expect("a held DB write lock must surface Busy, never a block");
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "the foreground write acquire must be bounded, took {:?}",
        start.elapsed()
    );
    assert_eq!(err.code, "Busy", "must be a named Busy: {}", err.message);
    assert!(
        err.message.contains("index"),
        "must name the holder class (stored fact): {}",
        err.message
    );
    assert!(
        err.message.contains("retry"),
        "must state safe-to-retry: {}",
        err.message
    );
    assert!(
        !err.message.contains("InternalError"),
        "a contention transient must never render InternalError: {}",
        err.message
    );
}

/// Uncontended, both write guards are taken at once and released on drop (no leak) — the
/// no-contention path is behaviourally identical to the historical inline unbounded acquire.
#[test]
fn acquire_foreground_write_succeeds_and_releases_uncontended() {
    use crate::registry::RepoRegistry;
    use crate::state::DaemonState;
    use std::time::Duration;

    let (_dir, db_path) = migrated_db();
    let state = DaemonState::with_registry(RepoRegistry::empty_non_persistent());
    let db_runtime = state
        .get_or_create_db_runtime(&db_path)
        .expect("db runtime");
    let coordinator = RepoCoordinator::new();
    let activity = ActivityRegistry::new();

    let guards = acquire_foreground_write(
        &db_runtime,
        &coordinator,
        &activity,
        db_runtime.db_path(),
        Duration::from_millis(200),
    );
    assert!(
        guards.is_ok(),
        "uncontended foreground write coordination must succeed"
    );
    drop(guards);
    // Both guards released on drop.
    assert!(
        db_runtime.try_acquire_write().is_some(),
        "DB write mutex must be released when the guards drop"
    );
}

/// D1-A review item 2: the SECOND bounded layer. The DB write mutex is free (layer 1 acquires the
/// guard), but a held coordinator READER makes the refresh guard (layer 2) time out — the acquire
/// must then return a NAMED `Busy` (not a block, not `InternalError`) AND must have dropped the
/// already-held DB write guard as it unwinds (no leak): `try_acquire_write` succeeds afterwards.
#[test]
fn acquire_foreground_write_busy_on_coordinator_timeout_releases_db_guard() {
    use crate::registry::RepoRegistry;
    use crate::state::DaemonState;
    use std::time::{Duration, Instant};

    let (_dir, db_path) = migrated_db();
    let state = DaemonState::with_registry(RepoRegistry::empty_non_persistent());
    let db_runtime = state
        .get_or_create_db_runtime(&db_path)
        .expect("db runtime");
    let coordinator = RepoCoordinator::new();
    let activity = ActivityRegistry::new();

    // Hold a READER on the coordinator: a refresh acquire must wait for it to drain, so the bounded
    // refresh times out while the DB write mutex itself is UNcontended (isolates layer 2).
    let _reader = coordinator.acquire_read();

    let start = Instant::now();
    let err = acquire_foreground_write(
        &db_runtime,
        &coordinator,
        &activity,
        db_runtime.db_path(),
        Duration::from_millis(80),
    )
    .err()
    .expect("a held coordinator reader must surface Busy from the refresh layer, never a block");
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "the coordinator-layer timeout must be bounded, took {:?}",
        start.elapsed()
    );
    assert_eq!(err.code, "Busy", "must be a named Busy: {}", err.message);
    assert!(
        err.message.contains("retry"),
        "must state safe-to-retry: {}",
        err.message
    );
    assert!(
        !err.message.contains("InternalError"),
        "a coordinator-contention transient must never render InternalError: {}",
        err.message
    );
    // The DB write guard taken by layer 1 must have been dropped when layer 2 timed out — no leak.
    assert!(
        db_runtime.try_acquire_write().is_some(),
        "the DB write guard must be released after a coordinator-refresh timeout (no leak)"
    );
}

/// D1-A review item 3: the known-holder `Busy` names not just the holder CLASS but HOW LONG it has
/// been running (ratified D1 wording "… started Nm ago"). A just-stamped op renders "started 0s ago"
/// — the clause is present and honest.
#[test]
fn busy_message_names_holder_elapsed() {
    let activity = ActivityRegistry::new();
    let db = Path::new("/db/z.db");
    let _g = activity.begin(OpKind::Enrich, "/repos/z", None, db.to_path_buf());
    let msg = busy_message(db, &activity);
    assert!(
        msg.contains("enrich"),
        "must name the holder class (stored fact): {msg}"
    );
    assert!(
        msg.contains("background") && msg.contains("pass"),
        "enrich is a background pass in the reader-frame wording: {msg}"
    );
    assert!(
        msg.contains("started") && msg.contains("ago"),
        "must render the holder's elapsed ('started Nm ago'): {msg}"
    );
}

/// The elapsed humanizer is exact at the minute boundary: seconds below 60, whole minutes at/above.
/// Deterministic (no wall-clock) — the `busy_message` test above proves the clause is wired in.
#[test]
fn humanize_elapsed_seconds_then_minutes() {
    assert_eq!(humanize_elapsed(0), "0s");
    assert_eq!(humanize_elapsed(59), "59s");
    assert_eq!(humanize_elapsed(60), "1m");
    assert_eq!(humanize_elapsed(125), "2m");
    assert_eq!(humanize_elapsed(3600), "60m");
}
