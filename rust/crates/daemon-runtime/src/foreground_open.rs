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
//! Crate-private. The unit coverage lives in the sibling child-module file `foreground_open/tests.rs`
//! (relocated for DAEMON-RESIDUALS-1 review item 4 — the D1-A additions pushed the inline `mod tests`
//! past the 500-line structural guardrail; a child module keeps `pub(crate)`/private access to this
//! file while the production module stays well under the limit).

use std::path::Path;
use std::time::Duration;

use repo_graph_daemon_policy::{RepoCoordinator, WriteGuard};
use repo_graph_daemon_transport::{ErrorCode, ErrorDetail};
use repo_graph_storage::StorageConnection;

use crate::activity::{ActivityRegistry, OpKind};
use crate::state::{
    open_existing_with_busy_retry, DatabaseState, DbWriteGuard, OpenError, OpenPatience, RepoState,
};

/// DAEMON-RESIDUALS-1 (D1-A): the bounded patience a FOREGROUND write command waits for the DB
/// write mutex AND the repo coordinator's refresh guard before it re-codes the contention as an
/// honest, holder-named [`ErrorCode::Busy`] transient (rather than blocking unbounded up to the
/// 300s client-timeout SYMPTOM — the #2 mechanism).
///
/// 3s is chosen against measured evidence: it EXCEEDS the operator's measured persist block
/// (1.30/1.44s) with ~2x margin, so brief write contention is waited out and completes normally
/// rather than spuriously returning Busy; and it sits at ~1% of the 300s symptom threshold (a
/// FROZEN value, §3 — not a knob), so a genuinely long holder (a full index/enrich/retention pass)
/// yields a PROMPT named Busy the caller can retry. Recorded as a local calibration (non-blocking):
/// the mechanism (bounded patience + named Busy on both layers) is the operator-ratified D1 Option C;
/// only this duration is the builder's, tied to the measured block.
pub(crate) const FOREGROUND_WRITE_PATIENCE: Duration = Duration::from_secs(3);

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

/// DAEMON-RESIDUALS-1 (D1-A): acquire a FOREGROUND write command's coordination — the DB write
/// mutex AND the repo coordinator's refresh guard — with BOUNDED patience, re-coding contention as
/// an honest, holder-named [`ErrorCode::Busy`] transient instead of blocking unbounded (the #2
/// mechanism: `assess`/`coverage` held on `db_runtime.acquire_write()` up to the 300s symptom).
///
/// Both layers use the same `patience` budget; a timeout on EITHER returns the SAME §2.2 holder-named
/// [`busy_message`] the foreground-open choke uses (index|refresh|enrich|retention, honest unknown
/// otherwise) — one Busy vocabulary across every foreground contention surface. On success the two
/// guards are returned in acquisition order (DB write first, then refresh) so their Drop order
/// (refresh, then DB write) matches the historical inline `let _db = …; let _refresh = …;`. A
/// timeout on the coordinator refresh drops the already-held DB write guard as this returns Err — no
/// leak, and NO partial write (the caller writes only after both guards are held).
///
/// Abstraction ledger:
/// - **what:** the single choke a foreground write command uses to take its two write guards with
///   bounded patience + honest Busy — the write-side peer of [`open_repo_storage_for_request`].
/// - **concrete current users:** `handlers::governance::assess::handle_assess` and
///   `handlers::quality::coverage::handle_coverage` (the two foreground quality/governance write
///   commands whose unbounded lock order is the EVIDENCED #2 symptom).
/// - **named axis:** none new — it composes the two ratified bounded primitives
///   ([`DatabaseState::acquire_write_timeout`] + [`RepoCoordinator::acquire_refresh_timeout`]) and
///   the FIXED Busy-vs-acquired outcome, reusing [`busy_message`]. `patience` is a parameter so a
///   unit test can inject a short budget (the const drives production) — a real test seam.
/// - **rejected simpler:** inline the two bounded acquires + Busy mapping at each of the two call
///   sites — rejected: duplicates the correctness-sensitive dual-layer timeout + holder-naming, so a
///   future budget/message change would have to be found at two places, and diverges from the
///   open-choke seam.
pub(crate) fn acquire_foreground_write<'a>(
    db_runtime: &'a DatabaseState,
    coordinator: &'a RepoCoordinator,
    activity: &ActivityRegistry,
    db_path: &Path,
    patience: Duration,
) -> Result<(DbWriteGuard<'a>, WriteGuard<'a>), ErrorDetail> {
    // Layer 1: the DB write mutex (the #2 block site).
    let db_guard = match db_runtime.acquire_write_timeout(patience) {
        Some(g) => g,
        None => {
            return Err(ErrorDetail::new(
                ErrorCode::Busy,
                busy_message(db_path, activity),
            ))
        }
    };
    // Layer 2: the repo coordinator's refresh guard. Once the DB write mutex is held no other write
    // pass can be mid-order, so this waits only for active readers to drain (bounded) — still
    // bounded + named for defence in depth (D1 Option C: BOTH layers). On timeout `db_guard` drops
    // here (lock released) as we return Err.
    let refresh_guard = match coordinator.acquire_refresh_timeout(patience) {
        Ok(g) => g,
        Err(_timeout) => {
            return Err(ErrorDetail::new(
                ErrorCode::Busy,
                busy_message(db_path, activity),
            ))
        }
    };
    Ok((db_guard, refresh_guard))
}

/// The reader-frame exhausted-patience message: names the holder CLASS **and how long it has been
/// running** from the activity registry when the daemon knows an in-flight write op is on this DB,
/// else states the honest unknown (a concurrent operation we cannot name — e.g. another read's
/// migration-check write, which is not stamped in the registry). Either way it states safe-to-retry
/// and includes the store path.
///
/// DAEMON-RESIDUALS-1 (D1-A): `pub(crate)` and reused by [`acquire_foreground_write`] so the DB
/// write-mutex / coordinator-refresh timeout renders the SAME honest Busy vocabulary as the storage
/// open choke — one message shape across every foreground contention layer. The holder ELAPSED is
/// the ratified D1 wording ("… started Nm ago"): the operator can tell a brief persist from a long
/// index/enrich pass without opening `doctor`.
pub(crate) fn busy_message(db_path: &Path, activity: &ActivityRegistry) -> String {
    match activity.active_for_db(db_path) {
        // `op.kind.as_str()` is a STORED activity fact (index|refresh|enrich|retention), not a
        // guess from a name — honest holder-class naming per §2.2. `op.started_secs_ago` is the
        // op's monotonic elapsed (a stored fact, not a fallible read — HONESTY RULE 2).
        // review-2: Index/Refresh are USER-INITIATED operations, not background passes —
        // the holder-class wording follows the op kind (exhaustive; a new OpKind variant
        // must choose its wording here by compiler force, never a wildcard default).
        Some(op) => {
            let elapsed = humanize_elapsed(op.started_secs_ago);
            let holder = match op.kind {
                OpKind::Index | OpKind::Refresh => {
                    format!(
                        "an in-progress {} operation (started {elapsed} ago)",
                        op.kind.as_str()
                    )
                }
                OpKind::Enrich | OpKind::Retention => {
                    format!(
                        "a background {} pass (started {elapsed} ago)",
                        op.kind.as_str()
                    )
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

/// Human-readable elapsed rendering for the Busy holder identity (ratified D1 wording: "started
/// Nm ago"). Whole seconds under a minute, whole minutes above — enough for the operator to tell a
/// brief persist from a long index/enrich pass without opening `doctor`. `secs` is the op's stored
/// monotonic elapsed (`ActiveOperationView::started_secs_ago`), never a fallible read, so there is
/// no `unwrap_or(0)` / lossy fallback here (STANDING HONESTY RULE 2).
fn humanize_elapsed(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{}m", secs / 60)
    }
}

#[cfg(test)]
mod tests;
