//! FOREGROUND-LOCK-1 (§2.2): honest re-code of a transient foreground storage-open lock.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **what:** the single choke the dispatch handlers use to open a repo's storage for a
//!   foreground request. It opens with the SHORT [`crate::state::OpenPatience::Foreground`]
//!   budget and, when that budget is exhausted on a *transient lock*, returns an honest
//!   `Busy` error that names the holder CLASS (from the daemon's activity registry) and the
//!   next move — never the flat `InternalError` the bare `storage()` + call-site wrap produced.
//! - **concrete current users:** the two foreground-open seams that share the ONE-message choke —
//!   `ServiceDispatcher::open_storage` (handlers still inline in `dispatch.rs`) and
//!   `DaemonState::open_repo_storage_for_request` (the extracted `handlers/*` modules) — used in
//!   place of the bare `repo_state.storage()` at every foreground request open; plus the two SPLIT
//!   seams (`ServiceDispatcher::open_storage_split` / `DaemonState::open_repo_storage_for_request_split`)
//!   for the four secondary opens that keep their own §2.3 non-lock message (see
//!   [`ForegroundOpenFault`]).
//! - **named axis:** error-code + message classification of a foreground open outcome — a FIXED
//!   two-way split (`Busy` transient lock vs `InternalError` genuine fault), matched exhaustively
//!   on [`crate::state::OpenError`]. This is code/message policy, distinct from the retry
//!   *mechanism* (which lives in `state::open_existing_with_busy_retry`).
//! - **rejected simpler:** classify the lock at the transport egress by sniffing the error
//!   *string* — rejected: that is name/text classification of a rendered message (a standing
//!   honesty violation) and it cannot reach the activity registry to name the holder.
//!
//! Crate-private, well under the 500-line guardrail — pre-ratified by the slice packet.

use std::path::Path;

use repo_graph_daemon_transport::{ErrorCode, ErrorDetail};
use repo_graph_storage::StorageConnection;

use crate::activity::{ActivityRegistry, OpKind};
use crate::state::{open_existing_with_busy_retry, OpenError, OpenPatience, RepoState};

/// FOREGROUND-LOCK-1 (§2.3): the outcome of a foreground open for a caller that renders a genuine
/// NON-lock fault with its OWN pre-existing code + message (a secondary open whose historical text
/// differs from the shared read-open text). The lock transient still becomes the shared honest
/// `Busy` detail; the non-lock fault is handed back RAW so the caller re-applies its own prefix.
///
/// Abstraction ledger:
/// - **what:** a two-variant sum carrying a foreground open's non-success outcome to a caller that
///   owns its non-lock message.
/// - **concrete current users:** the four secondary foreground opens with a distinct pre-existing
///   non-lock message — `assess`/`coverage` second opens (via
///   [`crate::state::DaemonState::open_repo_storage_for_request_split`]) and `handle_enrich`/
///   `handle_docs_extract` (via `ServiceDispatcher::open_storage_split`).
/// - **named axis:** who renders the non-lock fault — the SHARED choke
///   ([`open_repo_storage_for_request`]) owns it (one message), vs. the CALLER owns it (its §2.3
///   message). Variants FIXED, matched exhaustively at each of the four sites.
/// - **rejected simpler:** route these four through the shared choke too (build-1 did) — rejected:
///   it overwrites their §2.3 non-lock messages with the shared text (the review-1 defect).
pub(crate) enum ForegroundOpenFault {
    /// A transient lock outlived the foreground patience budget — an honest, retryable `Busy`
    /// detail (holder-named, §2.2). Return it to the client unchanged.
    Busy(ErrorDetail),
    /// A genuine non-lock fault (missing DB, corruption, I/O) — the RAW error text (no prefix). The
    /// caller wraps it in its OWN pre-existing code + message (§2.3: unchanged).
    Other(String),
}

/// Open `repo_state`'s storage for a FOREGROUND request with bounded lock patience, mapping the
/// outcome to a client `ErrorDetail`:
///
/// - success → the open connection;
/// - transient lock that outlived the patience budget → [`ErrorCode::Busy`] + a reader-frame
///   message naming the holder class (§2.2) — a transient the caller can retry;
/// - any genuine fault (missing DB, corruption, I/O) → [`ErrorCode::InternalError`] with the
///   historical `"failed to open storage connection: …"` text — unchanged from pre-slice.
///
/// The primary opens (every foreground read + the write handlers' first open) share this one
/// message. A secondary open with a DISTINCT §2.3 message uses [`open_repo_storage_for_request_split`]
/// instead.
pub(crate) fn open_repo_storage_for_request(
    repo_state: &RepoState,
    activity: &ActivityRegistry,
) -> Result<StorageConnection, ErrorDetail> {
    match open_repo_storage_for_request_split(repo_state, activity) {
        Ok(storage) => Ok(storage),
        Err(ForegroundOpenFault::Busy(detail)) => Err(detail),
        // Re-apply the historical prefix so the shared-message callers are byte-identical to
        // pre-slice (`RepoState::storage()` produced exactly this text).
        Err(ForegroundOpenFault::Other(msg)) => Err(ErrorDetail::new(
            ErrorCode::InternalError,
            format!("failed to open storage connection: {msg}"),
        )),
    }
}

/// Like [`open_repo_storage_for_request`] but carries the lock/non-lock split to the caller: a
/// transient lock becomes the shared honest [`ForegroundOpenFault::Busy`] detail, while a genuine
/// non-lock fault is returned RAW ([`ForegroundOpenFault::Other`]) so the caller preserves its own
/// §2.3 code + message. Used only by the four secondary opens named on [`ForegroundOpenFault`].
pub(crate) fn open_repo_storage_for_request_split(
    repo_state: &RepoState,
    activity: &ActivityRegistry,
) -> Result<StorageConnection, ForegroundOpenFault> {
    match open_existing_with_busy_retry(repo_state.db_path(), OpenPatience::Foreground) {
        Ok(storage) => Ok(storage),
        Err(OpenError::LockedAfterRetries { .. }) => {
            Err(ForegroundOpenFault::Busy(ErrorDetail::new(
                ErrorCode::Busy,
                busy_message(repo_state.db_path(), activity),
            )))
        }
        Err(OpenError::Other(msg)) => Err(ForegroundOpenFault::Other(msg)),
    }
}

/// The reader-frame exhausted-patience message: names the holder CLASS from the activity registry
/// when the daemon knows an in-flight write op is on this DB, else states the honest unknown (a
/// concurrent operation we cannot name — e.g. another read's migration-check write, which is not
/// stamped in the registry). Either way it states safe-to-retry and includes the store path.
fn busy_message(db_path: &Path, activity: &ActivityRegistry) -> String {
    match activity.active_for_db(db_path) {
        // `op.kind.as_str()` is a STORED activity fact (index|refresh|enrich|retention), not a
        // guess from a name — honest holder-class naming per §2.2.
        // review-2: Index/Refresh are USER-INITIATED operations, not background passes —
        // the holder-class wording follows the op kind (exhaustive; a new OpKind variant
        // must choose its wording here by compiler force, never a wildcard default).
        Some(op) => {
            let holder = match op.kind {
                OpKind::Index | OpKind::Refresh => {
                    format!("an in-progress {} operation", op.kind.as_str())
                }
                OpKind::Enrich | OpKind::Retention => {
                    format!("a background {} pass", op.kind.as_str())
                }
            };
            format!(
                "the store is momentarily busy: {holder} is writing this repo's store. \
                 This is transient — retry in a moment. (store: {})",
                db_path.display()
            )
        }
        None => format!(
            "the store is momentarily busy: a concurrent operation is holding it. \
             This is transient — retry in a moment. (store: {})",
            db_path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
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
}
