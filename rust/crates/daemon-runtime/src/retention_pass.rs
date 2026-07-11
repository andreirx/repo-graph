//! SNAPSHOT-RETENTION-1: the automatic background snapshot-retention pass.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** the daemon's automatic, background snapshot-retention pass — the LIFECYCLE the shipped
//!   retention model (`repo_graph_storage::retention`) never had. After a successful index/refresh the
//!   daemon spawns this pass; it applies the ratified keep-set (current + delta-base parent + user
//!   baselines — enforced in `classify_repo_retention`), prunes everything else PLUS orphaned
//!   non-READY snapshots, and — under the repo coordinator's `Writing` state so concurrent reads block
//!   honestly instead of hitting the VACUUM's exclusive lock — threshold-gates a rare VACUUM so disk
//!   shrinks when it is majority-dead, without paying a full-file rewrite on every small refresh.
//! - **Concrete current users:** spawned via [`spawn_auto_retention`] from two sites — chained by
//!   `enrich_pass::run_auto_enrich` after each enrichment pass (the default enrich-ON path, so retention
//!   never contends with a long enrichment for the write lock), and directly by
//!   `dispatch::ServiceDispatcher::finish_write_with_maintenance` when enrichment is opted out — i.e. on
//!   every index/refresh success either way. The gate + pass core
//!   ([`try_retention_attempt`] / [`run_retention_pass`]) are also driven directly by the named
//!   steady-state / baseline-user / contention / threshold tests AND the two reader-vs-VACUUM
//!   production-interaction proofs (`vacuum_defers_to_an_active_reader_then_runs_when_idle`,
//!   `reader_arriving_during_vacuum_window_blocks_then_reads_correct_data`) — the Test API seam.
//! - **Named axis of variation:** none beyond the op that triggers it. This is a cohesion split from
//!   `dispatch.rs` and `handlers::inventory::retention` (both near/over the 500-line structural
//!   guardrail), NOT a variation seam.
//! - **Rejected simpler alternative:** inline the pass in `handle_index`/`handle_refresh`. Rejected on
//!   two counts: (1) it would run ON the foreground request path — REFRESH-HANG-1 proved a 60+s prune
//!   there blocks the client; the slice's hard invariant is "NEVER on the foreground request path";
//!   (2) it would duplicate the two-gate contention discipline across two 8000-line handlers.
//!
//! # Contention safety — writers vs. the pass, and READERS vs. the VACUUM
//!
//! Two distinct hazards, two distinct mechanisms. The slice stop-condition ("retention NEVER runs while
//! any write op is active") is the WRITER hazard; the operator's iteration-1 note ("a reader during the
//! VACUUM must get honest behavior, never a raw `SQLITE_BUSY`") is the READER hazard. They need
//! different locks because the daemon coordinates writers and readers on different objects.
//!
//! ## Writers — the two gates (reused verbatim from the orphan-prune handler)
//!
//! Before touching the DB the pass checks BOTH gates the ratified
//! `handlers::inventory::retention::reclaim_orphaned_non_ready` uses:
//!
//! 1. **Activity registry clear for OTHER ops** — `state.activity().active_for_db(db)` finds any
//!    in-flight index/refresh/enrich (an initial index coordinates on the DB lock, NOT the
//!    `RepoCoordinator`, so this registry is the only thing that sees it early). Checked BEFORE the
//!    pass stamps its own `Retention` op, so it sees only OTHER ops.
//! 2. **Non-blocking DB write lock** — `try_acquire_write()` (never blocks) on the same
//!    `DatabaseState` lock an index takes. Held for the whole pass, so an index that starts mid-pass
//!    blocks on `acquire_write()` until the (bounded, steady-state ≈ one-snapshot) pass ends.
//!
//! If either gate is closed the pass **yields and requeues** (a bounded sleep-and-retry loop) —
//! explicit user ops always win. Because the triggering index still holds its own write lock + activity
//! stamp when the pass is spawned, the pass's first attempt naturally yields until that index fully
//! finishes, then runs on a later attempt.
//!
//! ## Readers vs. the VACUUM — the coordinator `Writing` guard (the iteration-1 fix)
//!
//! The two write-gates above exclude WRITERS; they do NOT touch READERS. Coordinated reads
//! (`orient`/`callers`/`explain`/… — every `dispatch` read handler) take the repo's
//! [`RepoCoordinator`] read-lock (`acquire_read`), NOT the DB write lock, and rely on SQLite WAL
//! snapshot isolation to coexist with writers. The prune's `DELETE`s honor that (WAL) and never block a
//! reader even when slow. The **VACUUM does not**: `StorageConnection::vacuum` drops to a rollback
//! journal to truncate the file, taking a SQLite **EXCLUSIVE** lock — which, with no `busy_timeout`
//! configured, returns a raw `database is locked` to any concurrent reader. That was the iteration-0
//! defect.
//!
//! The fix reuses the daemon's OWN contention rule — the ONE the shipped maintenance-prune handler
//! already holds around its VACUUM: the pass acquires the SAME shared `RepoCoordinator`'s **`Writing`**
//! state (`try_acquire_write`) around the VACUUM only. `Writing` excludes readers at the coordinator
//! (Rust-side), so a reader that ARRIVES during the VACUUM blocks honestly on `acquire_read` and gets
//! the correct post-prune answer a moment later — never a busy error, never a wrong answer. If a reader
//! is ALREADY mid-request, `try_acquire_write` returns `None` and the pass **defers** the VACUUM (the
//! reader is untouched; the freed pages recycle; the next pass retries). The VACUUM is threshold-gated
//! to be RARE (see [`decide_vacuum`]), so this brief reader-exclusion window is a rare event.
//!
//! # Detached completion (INDEX-DISCONNECT-1 principle) — why `detached.rs` is NOT reused
//!
//! The pass has no client (it is spawned AFTER the index response was already sent), so it is
//! inherently detached: it runs to completion in the background regardless of any client, records its
//! outcome for `rmap doctor` ([`crate::state::DaemonState::record_retention_report`]), and logs one
//! reader-frame completion line to the daemon log. `detached.rs`'s "client disconnected; op continues
//! detached" line describes a DIFFERENT situation (an attached client that vanished mid-op); there was
//! never an attached client here, so that line would misdescribe the reader's world and is not reused.
//!
//! # References
//! - `docs/slices/snapshot-retention-1.md`
//! - `docs/slices/daemon-visibility-1.md` (§2 F3 — the two-gate orphan reclaim this reuses)
//! - `docs/slices/refresh-hang-1.md` (why prune is off the foreground path)

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use repo_graph_daemon_policy::RepoCoordinator;
use repo_graph_storage::connection::StorageConnection;
use repo_graph_storage::error::StorageError;

use crate::state::DaemonState;

/// VACUUM only when the reclaimable freelist is at least this fraction of the DB file — i.e. the file
/// is **majority-dead**. Why this is set high, and why there is deliberately NO absolute-bytes gate:
///
/// SQLite reuses freed (freelist) pages for the NEXT write before it extends the file. In steady state
/// the pass prunes ONE snapshot — roughly `1/3` of a `current + parent + one-old` file — and the next
/// index writes ~one snapshot, so those freed pages are **recycled** and the file stays flat with NO
/// VACUUM. VACUUMing that transient `~1/3` freelist would be pure waste: a full-file, exclusive-lock
/// rewrite (the REFRESH-HANG-1 cost, and the reads-vs-VACUUM window this slice must keep rare) whose
/// space the very next index would have reused anyway.
///
/// A VACUUM returns real disk to the OS only when the live data will STAY a minority of the file — the
/// freelist a majority: a one-time backlog cleanup (retention was off; many old snapshots pruned at
/// once), a repo that shrank, or a dominant orphaned partial (the 4 GB field bug). A 50% gate fires on
/// exactly those and skips the recyclable steady-state case, so VACUUM is **rare**.
///
/// The iteration-0 draft added an absolute `≥ 1 GiB` gate; it is REMOVED. On a large repo a single
/// steady-state snapshot exceeds 1 GiB, so an absolute gate would force a multi-GB VACUUM on *every*
/// index — frequent reader-exclusion windows + wasted rewrites, the exact pathology this gate avoids.
/// (Revises the slice §3 "≥ 25% or ≥ 1 GB" starting proposal, which the slice delegates to the builder;
/// see `docs/slices/snapshot-retention-1.md` §3 and the build report.)
pub const VACUUM_MIN_FRACTION: f64 = 0.50;

/// Bounded sleep-and-retry loop parameters. Steady-state passes are ~one-snapshot fast; the loop only
/// exists to WAIT OUT a concurrent index/refresh (the "yields and requeues" rule). ~60 attempts ×
/// ~1s ≈ up to a minute of waiting for a busy DB before deferring — the next successful write requeues
/// a fresh pass anyway, so deferral loses nothing but the wait.
const REQUEUE_MAX_ATTEMPTS: u32 = 60;
const REQUEUE_BACKOFF: Duration = Duration::from_millis(1000);

/// Decide whether the background pass runs a VACUUM, given the reclaimable freelist bytes and the
/// current DB file size.
///
/// True iff `reclaimable ≥ 50% of the file` (the file is majority-dead — see [`VACUUM_MIN_FRACTION`]
/// for why steady-state prunes fall below this and recycle instead of vacuuming). A zero-byte file
/// (unknown/empty) never vacuums.
pub fn decide_vacuum(reclaimable_bytes: u64, file_size_bytes: u64) -> bool {
    if file_size_bytes == 0 {
        return false;
    }
    (reclaimable_bytes as f64) >= (file_size_bytes as f64) * VACUUM_MIN_FRACTION
}

/// Opt-out switch for the automatic background retention pass (default ON — the ratified posture is
/// aggressive cleanup). Consistent with the daemon's established env-var config precedent
/// (`RMAP_STATE_ROOT`, `RMAP_PERF`, `RMAP_TRANSPORT`): set `RMAP_AUTO_RETENTION` to
/// `0`/`false`/`off`/`no`/`disabled` (case-insensitive) to disable. Any other value — or unset —
/// leaves it ON.
pub fn auto_retention_enabled() -> bool {
    // Test override (0 = none, 1 = force ON, 2 = force OFF) wins over the env. Production leaves it at
    // 0 → one relaxed atomic load, then the env default.
    match AUTO_RETENTION_OVERRIDE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => auto_retention_enabled_from(std::env::var("RMAP_AUTO_RETENTION").ok().as_deref()),
    }
}

/// Test override for [`auto_retention_enabled`]: 0 = no override (use env), 1 = force ON, 2 = force OFF.
static AUTO_RETENTION_OVERRIDE: AtomicU8 = AtomicU8::new(0);

/// TEST SEAM — force the auto-retention pass ON/OFF for the current test binary, race-free (an atomic,
/// NOT the process-global `RMAP_AUTO_RETENTION` env var, which is UB to mutate while the daemon's
/// retention threads read it). Integration tests that exercise index/refresh VISIBILITY, write-lock
/// contention, or snapshot counts (NOT retention itself) disable the pass so its background write-lock
/// actor cannot perturb their deterministic assertions. `#[doc(hidden)]`, `_for_test`-named: no
/// production caller (production reads the env default, so the override stays 0).
#[doc(hidden)]
pub fn set_auto_retention_for_test(enabled: bool) {
    AUTO_RETENTION_OVERRIDE.store(if enabled { 1 } else { 2 }, Ordering::Relaxed);
}

/// Pure core of [`auto_retention_enabled`] (env value in, decision out) — unit-tested without mutating
/// the process-global environment (which would race parallel tests).
fn auto_retention_enabled_from(val: Option<&str>) -> bool {
    match val {
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no" | "disabled"
        ),
        None => true,
    }
}

/// The honest fate of the threshold-gated VACUUM in one pass. Three distinct outcomes the reader must
/// be able to tell apart — never collapsed into a bare "the file didn't shrink":
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VacuumStatus {
    /// The VACUUM ran (readers excluded via the repo coordinator's `Writing` state) and returned bytes
    /// to the OS — `reclaimed_bytes` is the on-disk delta.
    Ran,
    /// Reclaim was below [`VACUUM_MIN_FRACTION`]; the freed pages sit on the freelist and are recycled
    /// by the next index. Skipping the VACUUM here is the correct, cheap steady-state behavior.
    SkippedBelowThreshold,
    /// Reclaim was above threshold, but a reader held the repo read-lock, so the pass DEFERRED the
    /// VACUUM rather than take the SQLite exclusive lock out from under it (which would `SQLITE_BUSY`
    /// the reader). The rows are already pruned; the freed pages recycle; the next pass retries.
    DeferredReadersActive,
}

impl VacuumStatus {
    /// The stable token surfaced on `daemon_info.last_retention` for the `rmap doctor` cleanup line.
    pub fn as_str(&self) -> &'static str {
        match self {
            VacuumStatus::Ran => "ran",
            VacuumStatus::SkippedBelowThreshold => "below_threshold",
            VacuumStatus::DeferredReadersActive => "deferred_readers_active",
        }
    }
}

/// What one retention pass did — the data the daemon records + reports.
#[derive(Debug, Clone)]
pub struct RetentionPassOutcome {
    /// READY snapshots pruned by the ratified keep-set (`classify` → `prune_prunable_snapshots`).
    pub pruned_count: i64,
    /// Orphaned non-READY (interrupted/failed) snapshots reclaimed in the SAME pass (slice §2).
    pub non_ready_reclaimed: usize,
    /// Freelist bytes a VACUUM would return to the OS, measured (freelist × page size) BEFORE the
    /// VACUUM decision — the honest gate input.
    pub reclaimable_bytes: u64,
    /// The honest fate of the threshold-gated VACUUM (ran / below-threshold / reader-deferred).
    pub vacuum: VacuumStatus,
    /// Bytes actually returned to the OS (file-size delta). 0 unless `vacuum == Ran`.
    pub reclaimed_bytes: u64,
    /// DB file size after the pass.
    pub db_size_after: u64,
}

impl RetentionPassOutcome {
    /// Total snapshots removed this pass (READY prune + non-READY reclaim).
    pub fn total_removed(&self) -> i64 {
        self.pruned_count + self.non_ready_reclaimed as i64
    }
}

/// A completed retention pass, kept on `DaemonState` so `rmap doctor` can report what the last
/// background pass reclaimed — the honesty surface for "pruned N / reclaimed X GB". The pass is async
/// (spawned after the index response is sent), so the synchronous index reply cannot carry its result;
/// doctor reads this. Mirrors the daemon_info `last_snapshot` "last completed X" precedent.
#[derive(Debug, Clone)]
pub struct RetentionReport {
    pub repo_display: String,
    pub outcome: RetentionPassOutcome,
    at: Instant,
}

impl RetentionReport {
    pub fn new(repo_display: String, outcome: RetentionPassOutcome) -> Self {
        Self {
            repo_display,
            outcome,
            at: Instant::now(),
        }
    }

    /// The JSON shape `daemon_info.last_retention` carries (read by the `rmap doctor` retention probe).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "repo": self.repo_display,
            "pruned_count": self.outcome.pruned_count,
            "non_ready_reclaimed": self.outcome.non_ready_reclaimed,
            "reclaimable_bytes": self.outcome.reclaimable_bytes,
            "vacuum_status": self.outcome.vacuum.as_str(),
            "reclaimed_bytes": self.outcome.reclaimed_bytes,
            "db_size_bytes": self.outcome.db_size_after,
            "finished_secs_ago": self.at.elapsed().as_secs(),
        })
    }
}

/// The outcome of one gated attempt at the pass.
pub enum RetentionAttempt {
    /// The pass ran to completion; carries its outcome.
    Ran(RetentionPassOutcome),
    /// A contention gate was closed (another op writes this DB) — the caller should requeue.
    Yielded(&'static str),
    /// The pass could not start / errored (storage open or a storage error mid-pass).
    Failed(String),
}

/// Apply the ratified retention keep-set to `repo_uid` on an OPEN storage connection, then
/// threshold-gate a reader-safe VACUUM.
///
/// **The caller MUST already hold the DB write lock and have cleared the write-contention gates** (see
/// [`try_retention_attempt`]); this is the raw mechanism, exposed so the named tests can drive it
/// directly. `coordinator` is the SAME [`RepoCoordinator`] readers take `acquire_read` on — the pass
/// enters its `Writing` state around the VACUUM so a concurrent reader blocks honestly instead of
/// hitting the VACUUM's exclusive lock. Tests that don't exercise readers pass a fresh idle coordinator.
///
/// Sequence: classify (ratified keep-set) → prune prunable READY → reclaim orphaned non-READY (same
/// pass, slice §2), all WAL-safe for readers → measure reclaimable → IF above threshold, take the
/// coordinator `Writing` guard and VACUUM (else skip / defer) → measure the on-disk reclaim.
pub fn run_retention_pass(
    storage: &StorageConnection,
    db_path: &Path,
    repo_uid: &str,
    coordinator: &RepoCoordinator,
) -> Result<RetentionPassOutcome, StorageError> {
    // 1. Ratified keep-set, then prune the READY snapshots it marks prunable. These are ordinary WAL
    //    writes: a concurrent reader (holding the coordinator read-lock) sees the pre-delete snapshot
    //    via WAL snapshot isolation, so the prune NEVER blocks a reader even when it is slow
    //    (REFRESH-HANG-1's row-delete cost). The coordinator is engaged ONLY for the VACUUM below.
    storage.classify_repo_retention(repo_uid)?;
    let pruned_count = storage.prune_prunable_snapshots(repo_uid)?;

    // 2. Reclaim orphaned non-READY (interrupted/failed) snapshots in the SAME pass (slice §2 — the
    //    DAEMON-VISIBILITY-1 reclaim, re-run here since the retention pass already holds the gates).
    let non_ready = storage.prune_non_ready_snapshots(repo_uid)?;

    // 3. Threshold-gated VACUUM. `reclaimable_bytes` measures what a VACUUM would return to the OS
    //    WITHOUT paying it; below threshold we skip (freed pages are reused by the next index).
    let reclaimable = storage.reclaimable_bytes()?;
    let size_before = file_size(db_path);
    let mut vacuum = VacuumStatus::SkippedBelowThreshold;
    let mut reclaimed_bytes = 0;
    if decide_vacuum(reclaimable, size_before) {
        // `StorageConnection::vacuum` drops to a rollback journal to truncate the file, taking a SQLite
        // EXCLUSIVE lock that would `SQLITE_BUSY` a concurrent reader. Enter the coordinator's `Writing`
        // state first: a reader that ARRIVES during the VACUUM blocks honestly on `acquire_read` and
        // gets the correct post-prune answer when we release. If a reader is ALREADY mid-request,
        // `try_acquire_write` returns None (the coordinator is `Reading`) → DEFER (untouched reader,
        // recyclable pages, next-pass retry). This is the `Writing`-around-VACUUM protection the shipped
        // maintenance-prune handler holds; iteration-0 omitted it, which caused the raw busy errors.
        //
        // ASSUMPTION (matches the shipped maintenance-prune handler): one repo per DB file. `db_path` is
        // `SHA256(canonical_repo_path)` (registry::allocate_db_path), so every coordinated reader of this
        // file goes through THIS repo's coordinator. If a future change ever co-located repos in one DB
        // file, a sibling repo's reader would bypass this guard — the VACUUM would need a DB-file-level
        // reader gate then. No caller does that today.
        match coordinator.try_acquire_write() {
            Some(_writing) => {
                storage.vacuum()?;
                vacuum = VacuumStatus::Ran;
                reclaimed_bytes = size_before.saturating_sub(file_size(db_path));
                // `_writing` drops here → any reader blocked during the VACUUM proceeds.
            }
            None => {
                vacuum = VacuumStatus::DeferredReadersActive;
            }
        }
    }

    Ok(RetentionPassOutcome {
        pruned_count,
        non_ready_reclaimed: non_ready.len(),
        reclaimable_bytes: reclaimable,
        vacuum,
        reclaimed_bytes,
        db_size_after: file_size(db_path),
    })
}

/// Try to run the retention pass once, honoring the two contention gates.
///
/// Stamps a `Retention` activity op (so `rmap doctor` renders "reclaiming <repo>") ONLY after both
/// gates pass — so gate 1 (`active_for_db`) sees only OTHER ops, never this pass itself.
pub fn try_retention_attempt(
    state: &DaemonState,
    db_path: &Path,
    repo_uid: &str,
    repo_display: &str,
) -> RetentionAttempt {
    // Gate 1 — operator's ratified rule: never touch the DB while a live op (index/refresh/enrich, or
    // a sibling retention) writes it. Checked before stamping our own op, so we see only OTHERS.
    if state.activity().active_for_db(db_path).is_some() {
        return RetentionAttempt::Yielded("another operation is writing this repo");
    }

    // Gate 2 — take the DB write lock non-blockingly (excludes an initial index that coordinates on
    // this lock, not the RepoCoordinator). Held for the whole pass, so a later index waits it out.
    let db_runtime = match state.get_or_create_db_runtime(db_path) {
        Ok(r) => r,
        Err(e) => return RetentionAttempt::Failed(format!("could not resolve db runtime: {e}")),
    };
    let _db_guard = match db_runtime.try_acquire_write() {
        Some(g) => g,
        None => return RetentionAttempt::Yielded("another operation is writing this repo"),
    };

    // The SHARED repo state — its `RepoCoordinator` is the SAME instance coordinated reads take
    // `acquire_read` on, so the VACUUM inside `run_retention_pass` can exclude them via `Writing`.
    // Loaded here (a cache hit after the just-finished index) so the coordinator outlives the pass. A
    // load failure is a real error — the repo was just indexed — so surface it rather than silently
    // VACUUM uncoordinated (which would risk the raw busy error this whole change exists to prevent).
    let repo_state = match state.load_repo(db_path, repo_uid) {
        Ok(rs) => rs,
        Err(e) => {
            return RetentionAttempt::Failed(format!(
                "could not load repo to coordinate readers: {e}"
            ))
        }
    };

    // Both write-gates clear → NOW make the pass visible on `rmap doctor` for its duration. (Doctor /
    // storage-health reads short-circuit on this activity stamp and report "reclaiming <repo>" instead
    // of attempting a would-be-busy open — the existing DAEMON-VISIBILITY-1 contention path.)
    let _activity = state.activity().begin(
        crate::activity::OpKind::Retention,
        repo_display.to_string(),
        Some(repo_uid.to_string()),
        db_path.to_path_buf(),
    );

    let storage = match StorageConnection::open(db_path) {
        Ok(s) => s,
        Err(e) => return RetentionAttempt::Failed(format!("could not open storage: {e}")),
    };
    // DAEMON-CRASH-RECOVERY-1 (F8): the op-START line for retention (the outcome is logged by
    // `run_auto_retention`'s summary line). Emitted only once both gates passed and the pass truly runs.
    crate::oplog::log_op_start("retention", repo_uid, None);
    match run_retention_pass(&storage, db_path, repo_uid, &repo_state.coordinator) {
        Ok(outcome) => RetentionAttempt::Ran(outcome),
        Err(e) => RetentionAttempt::Failed(e.to_string()),
    }
    // _activity + _db_guard drop here (op deregistered, write lock released).
}

/// Spawn the automatic background retention pass for a DB after a successful index/refresh.
///
/// No-op when opted out (`RMAP_AUTO_RETENTION`). Otherwise detaches a thread that runs the two-gate
/// pass with bounded requeue, records the outcome for `rmap doctor`, and logs one completion line.
/// NEVER runs on the caller's (foreground) thread — that is the slice's hard invariant.
pub fn spawn_auto_retention(
    state: Arc<DaemonState>,
    db_path: PathBuf,
    repo_uid: String,
    repo_display: String,
) {
    if !auto_retention_enabled() {
        return;
    }
    std::thread::spawn(move || {
        run_auto_retention(&state, &db_path, &repo_uid, &repo_display);
    });
}

/// The detached pass body: requeue until a gate opens, then run + record + log. Separated from the
/// thread spawn so the gate/run logic is testable without threads (the named tests drive
/// [`try_retention_attempt`] directly).
fn run_auto_retention(state: &DaemonState, db_path: &Path, repo_uid: &str, repo_display: &str) {
    for _ in 0..REQUEUE_MAX_ATTEMPTS {
        match try_retention_attempt(state, db_path, repo_uid, repo_display) {
            RetentionAttempt::Ran(outcome) => {
                // DAEMON-CRASH-RECOVERY-1 (F8, review-0 item a): the op-lifecycle OUTCOME line —
                // same shape as index/refresh (op, repo, outcome), paired with the op-START line
                // `try_retention_attempt` logged. Replaces the ad-hoc "retention: …" summary so the
                // daemon log reads as ONE uniform op lifecycle; `summarize_outcome` is the reader-frame
                // detail (pruned N, reclaimed X / below-threshold / deferred).
                crate::oplog::log_op_outcome(
                    "retention",
                    repo_uid,
                    None,
                    &format!("completed ({})", summarize_outcome(&outcome)),
                );
                state.record_retention_report(RetentionReport::new(
                    repo_display.to_string(),
                    outcome,
                ));
                return;
            }
            RetentionAttempt::Failed(e) => {
                crate::oplog::log_op_outcome("retention", repo_uid, None, &format!("failed: {e}"));
                return;
            }
            RetentionAttempt::Yielded(_reason) => {
                // Explicit user op in progress — yield and requeue (the ratified rule). Not a terminal
                // outcome (no op ran this attempt), so no outcome line — the pass never logged a start.
                std::thread::sleep(REQUEUE_BACKOFF);
            }
        }
    }
    // Deferred: the pass never got past the gates (never logged a start), so this is the terminal
    // disposition of a spawned pass that never ran — an honest op-lifecycle line, not a lone outcome.
    crate::oplog::log_op_outcome(
        "retention",
        repo_uid,
        None,
        &format!(
            "deferred (repo stayed busy for {}s; the next index/refresh retries)",
            (REQUEUE_MAX_ATTEMPTS as u64) * REQUEUE_BACKOFF.as_secs()
        ),
    );
}

/// Reader-frame one-liner for the daemon log, honest about all three VACUUM fates: "pruned N, reclaimed
/// X on disk" | "pruned N (X reclaimable, VACUUM skipped — below threshold, pages recycle)" | "pruned N
/// (X reclaimable, VACUUM deferred — repo was being read; retries next pass)" | "nothing to prune".
pub fn summarize_outcome(o: &RetentionPassOutcome) -> String {
    let removed = o.total_removed();
    if removed == 0 {
        return "nothing to prune".to_string();
    }
    match o.vacuum {
        VacuumStatus::Ran => format!(
            "pruned {removed} snapshot(s), reclaimed {} on disk",
            format_bytes(o.reclaimed_bytes)
        ),
        VacuumStatus::SkippedBelowThreshold => format!(
            "pruned {removed} snapshot(s) ({} reclaimable, VACUUM skipped — below threshold, pages recycle)",
            format_bytes(o.reclaimable_bytes)
        ),
        VacuumStatus::DeferredReadersActive => format!(
            "pruned {removed} snapshot(s) ({} reclaimable, VACUUM deferred — repo was being read; retries next pass)",
            format_bytes(o.reclaimable_bytes)
        ),
    }
}

/// Coarse human byte formatter for the daemon-log line (doctor formats its own via `format_size`).
fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn file_size(db_path: &Path) -> u64 {
    std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RepoRegistry;
    use repo_graph_storage::types::{
        CreateSnapshotInput, GraphNode, Repo, UpdateSnapshotStatusInput,
    };
    use tempfile::{tempdir, TempDir};

    // ── decide_vacuum: the threshold policy ───────────────────────────────────────────────────

    #[test]
    fn decide_vacuum_majority_gate() {
        // Majority-dead (≥ 50% of file) → vacuum; below half → skip (pages recycle next index).
        assert!(
            decide_vacuum(500, 1000),
            "reclaimable == 50% of file → vacuum"
        );
        assert!(decide_vacuum(800, 1000), "reclaimable > 50% → vacuum");
        assert!(
            !decide_vacuum(499, 1000),
            "reclaimable < 50% → skip (recycle)"
        );
        // The steady-state ~1/3 reclaim is BELOW the gate → no VACUUM (the recyclable case).
        assert!(
            !decide_vacuum(333, 1000),
            "steady-state ~1/3 reclaim stays below the gate and recycles"
        );
    }

    #[test]
    fn decide_vacuum_has_no_absolute_gate_so_big_repos_are_not_vacuumed_every_index() {
        // The removed `≥ 1 GiB` absolute gate was the big-repo pathology: a single steady-state
        // snapshot on a large repo exceeds 1 GiB, so an absolute gate forced a multi-GB VACUUM (and
        // its reader-exclusion window) on EVERY index. With the majority-only gate, freeing one 2 GiB
        // snapshot from a 6 GiB file (~33%) does NOT vacuum — the pages recycle into the next index.
        let two_gib = 2 * 1024 * 1024 * 1024_u64;
        let six_gib = 6 * 1024 * 1024 * 1024_u64;
        assert!(
            !decide_vacuum(two_gib, six_gib),
            "2 GiB reclaimable on a 6 GiB file is only ~33% → no VACUUM (no absolute gate)"
        );
        // But a majority-dead large file (5 GiB free of 6 GiB — e.g. a big orphan reclaim) DOES vacuum.
        let five_gib = 5 * 1024 * 1024 * 1024_u64;
        assert!(
            decide_vacuum(five_gib, six_gib),
            "5 GiB of 6 GiB is majority-dead → VACUUM (backlog/orphan cleanup)"
        );
    }

    #[test]
    fn decide_vacuum_zero_file_never_vacuums() {
        assert!(!decide_vacuum(0, 0));
        assert!(
            !decide_vacuum(1_000_000, 0),
            "unknown/zero file size → never vacuum"
        );
    }

    // ── auto_retention_enabled: the opt-out switch (default ON) ────────────────────────────────

    #[test]
    fn auto_retention_default_on_and_opt_out_values() {
        // Default ON (unset) — the ratified aggressive-cleanup posture.
        assert!(auto_retention_enabled_from(None));
        // Every documented off-value disables (case-insensitive, trimmed).
        for off in [
            "0", "false", "off", "no", "disabled", "FALSE", " Off ", "No",
        ] {
            assert!(
                !auto_retention_enabled_from(Some(off)),
                "{off:?} must disable"
            );
        }
        // Anything else stays ON (fail-safe toward cleanup).
        for on in ["1", "true", "on", "yes", "enabled", ""] {
            assert!(auto_retention_enabled_from(Some(on)), "{on:?} must stay ON");
        }
    }

    // ── the pass core ─────────────────────────────────────────────────────────────────────────

    /// A padded SYMBOL node so a few thousand of them make a measurable file-size difference.
    fn bloat_node(repo: &str, snapshot_uid: &str, node_uid: &str) -> GraphNode {
        GraphNode {
            node_uid: node_uid.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo.to_string(),
            stable_key: format!("{repo}:{node_uid}:SYMBOL"),
            kind: "SYMBOL".to_string(),
            subtype: Some("FUNCTION".to_string()),
            name: node_uid.to_string(),
            qualified_name: Some(format!("bloated::module::path::to::{node_uid}")),
            file_uid: None,
            parent_node_uid: None,
            location: None,
            signature: Some("fn bloat(a: usize, b: usize, c: usize) -> usize".to_string()),
            visibility: Some("export".to_string()),
            doc_comment: Some(
                "a padded doc comment to grow the row size for a measurable reclaim".to_string(),
            ),
            metadata_json: None,
        }
    }

    fn add_repo(storage: &StorageConnection, repo: &str) {
        storage
            .add_repo(&Repo {
                repo_uid: repo.to_string(),
                name: format!("Test {repo}"),
                root_path: ".".to_string(),
                default_branch: Some("main".to_string()),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();
    }

    /// Create a READY snapshot (optionally chained to `parent`, optionally bloated) and return its
    /// uid. Sleeps 10ms afterward so the NEXT snapshot gets a strictly-later `created_at` — the
    /// classifier picks `current` by `created_at DESC`, so ordering must be deterministic.
    fn ready_snapshot(
        storage: &mut StorageConnection,
        repo: &str,
        parent: Option<&str>,
        bloat: usize,
    ) -> String {
        let snap = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo.to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: parent.map(|s| s.to_string()),
                label: None,
                toolchain_json: None,
            })
            .unwrap();
        let uid = snap.snapshot_uid.clone();
        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: uid.clone(),
                status: "ready".to_string(),
                completed_at: None,
            })
            .unwrap();
        if bloat > 0 {
            let nodes: Vec<GraphNode> = (0..bloat)
                .map(|i| bloat_node(repo, &uid, &format!("{uid}-n{i}")))
                .collect();
            storage.insert_nodes(&nodes).unwrap();
        }
        std::thread::sleep(Duration::from_millis(10));
        uid
    }

    fn ready_uids(storage: &StorageConnection, repo: &str) -> Vec<String> {
        let mut uids: Vec<String> = storage
            .list_snapshots(repo)
            .unwrap()
            .into_iter()
            .filter(|s| s.status == "ready")
            .map(|s| s.snapshot_uid)
            .collect();
        uids.sort();
        uids
    }

    // SNAPSHOT-RETENTION-1 STEADY-STATE PROOF (named test): a chained current←parent←older repo, after
    // the pass, holds EXACTLY current + delta-base parent; the older READY snapshot is pruned and the
    // pass reports what it removed. This is the ratified "steady state ≤ 2 snapshots/repo".
    #[test]
    fn steady_state_keeps_current_and_parent_prunes_older() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("steady.db");
        let (s2, s3) = {
            let mut storage = StorageConnection::open(&db_path).unwrap();
            add_repo(&storage, "r1");
            // The pruned snapshot dominates the file, so freeing it is a MAJORITY reclaim (> the 50%
            // gate) — a one-shot cleanup, the case a VACUUM is actually for. (In real steady state the
            // three snapshots are comparable and the ~1/3 reclaim stays below the gate and recycles;
            // that recyclable case is proven by `threshold_below_skips_vacuum_above_runs_it` BELOW.)
            let s1 = ready_snapshot(&mut storage, "r1", None, 8_000); // oldest, bloated → pruned
            let s2 = ready_snapshot(&mut storage, "r1", Some(&s1), 0); // delta base (parent)
            let s3 = ready_snapshot(&mut storage, "r1", Some(&s2), 0); // current
            (s2, s3)
        }; // drop → WAL checkpoint folds the bloat into the main file

        let storage = StorageConnection::open(&db_path).unwrap();
        // A fresh idle coordinator: no reader is active, so the VACUUM proceeds (this test proves the
        // prune/keep-set + reclaim, not the reader interaction — that is the two new tests below).
        let outcome =
            run_retention_pass(&storage, &db_path, "r1", &RepoCoordinator::new()).unwrap();

        assert_eq!(
            outcome.pruned_count, 1,
            "exactly the older READY snapshot pruned"
        );
        let remaining = ready_uids(&storage, "r1");
        let mut expect = vec![s2, s3];
        expect.sort();
        assert_eq!(remaining, expect, "only current + delta-base parent remain");
        // Pruning the bloated s1 frees a majority of the file → the pass vacuums + reports bytes.
        assert_eq!(
            outcome.vacuum,
            VacuumStatus::Ran,
            "majority reclaim above threshold → VACUUM ran"
        );
        assert!(outcome.reclaimed_bytes > 0, "reclaimed bytes reported");
    }

    // SNAPSHOT-RETENTION-1 BASELINE-USER PROOF (named test): a user-marked baseline survives the pass;
    // an ordinary older snapshot (what earlier policy would auto-baseline) does not.
    #[test]
    fn user_baseline_survives_but_auto_baseline_does_not() {
        use repo_graph_storage::retention::RetentionClass;

        let dir = tempdir().unwrap();
        let db_path = dir.path().join("baseline.db");
        let user_uid = {
            let mut storage = StorageConnection::open(&db_path).unwrap();
            add_repo(&storage, "r1");
            // s1 = human-marked baseline (keep). s2 = an ordinary older snapshot (auto → prune).
            // s3 = parent (delta base). s4 = current. All independent-of-user in the chain sense:
            // s2←s3←s4 chain; s1 stands alone as the explicit baseline.
            let s1 = ready_snapshot(&mut storage, "r1", None, 0);
            let s2 = ready_snapshot(&mut storage, "r1", None, 4_000); // ordinary older → prune
            let s3 = ready_snapshot(&mut storage, "r1", Some(&s2), 0);
            let _s4 = ready_snapshot(&mut storage, "r1", Some(&s3), 0);
            storage
                .mark_snapshot_retention(&s1, RetentionClass::BaselineUser)
                .unwrap();
            s1
        };

        let storage = StorageConnection::open(&db_path).unwrap();
        let _ = run_retention_pass(&storage, &db_path, "r1", &RepoCoordinator::new()).unwrap();

        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.baseline_user, 1, "the user-marked baseline survives");
        assert_eq!(stats.baseline_auto, 0, "no auto-baseline is ever retained");
        let remaining = ready_uids(&storage, "r1");
        assert!(
            remaining.contains(&user_uid),
            "the explicit human baseline is still present: {remaining:?}"
        );
    }

    fn isolated_state() -> DaemonState {
        // Non-persistent registry so the test never touches the operator's real registry; the pass
        // uses only the activity registry + db-runtime map, neither of which reads the registry.
        DaemonState::with_registry(RepoRegistry::empty_non_persistent())
    }

    // SNAPSHOT-RETENTION-1 CONTENTION PROOF (named test): the pass YIELDS while another op writes the
    // DB (either gate closed), and RUNS once the DB is idle. Both gates are exercised.
    #[test]
    fn retention_yields_under_contention_then_runs_when_idle() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("contention.db");
        {
            let mut storage = StorageConnection::open(&db_path).unwrap();
            add_repo(&storage, "r1");
            let s1 = ready_snapshot(&mut storage, "r1", None, 2_000); // prunable
            let s2 = ready_snapshot(&mut storage, "r1", Some(&s1), 0);
            let _s3 = ready_snapshot(&mut storage, "r1", Some(&s2), 0);
        }
        let db_path_canon = db_path.canonicalize().unwrap();
        let state = isolated_state();

        // Gate 1 CLOSED — a live index is stamped in the activity registry for this DB.
        {
            let _op = state.activity().begin(
                crate::activity::OpKind::Index,
                "r1".to_string(),
                Some("r1".to_string()),
                db_path_canon.clone(),
            );
            match try_retention_attempt(&state, &db_path_canon, "r1", "r1") {
                RetentionAttempt::Yielded(_) => {}
                other => panic!(
                    "retention must YIELD while an index is active (gate 1): {}",
                    attempt_label(&other)
                ),
            }
        }

        // Gate 2 CLOSED — the DB write lock is held by "another op".
        {
            let rt = state.get_or_create_db_runtime(&db_path_canon).unwrap();
            let _held = rt.acquire_write();
            match try_retention_attempt(&state, &db_path_canon, "r1", "r1") {
                RetentionAttempt::Yielded(_) => {}
                other => panic!(
                    "retention must YIELD while the DB write lock is held (gate 2): {}",
                    attempt_label(&other)
                ),
            }
        }

        // Both gates now clear → the pass RUNS and prunes the older snapshot.
        match try_retention_attempt(&state, &db_path_canon, "r1", "r1") {
            RetentionAttempt::Ran(outcome) => {
                assert_eq!(
                    outcome.pruned_count, 1,
                    "runs once idle and prunes the older READY"
                );
            }
            other => panic!("retention must RUN once idle: {}", attempt_label(&other)),
        }
    }

    fn attempt_label(a: &RetentionAttempt) -> String {
        match a {
            RetentionAttempt::Ran(o) => format!("Ran(pruned={})", o.pruned_count),
            RetentionAttempt::Yielded(r) => format!("Yielded({r})"),
            RetentionAttempt::Failed(e) => format!("Failed({e})"),
        }
    }

    // DAEMON-CRASH-RECOVERY-1 (F8, review-0 item a): a real background retention pass writes an
    // explicit op-lifecycle START + OUTCOME pair to the log sink — the same `op <op> <outcome> (repo
    // <repo>)` shape as index/refresh — NOT the old ad-hoc "retention: …" summary. Drives the real
    // `run_auto_retention` requeue loop (idle → runs → records) end to end.
    #[test]
    fn run_auto_retention_logs_the_op_lifecycle_outcome_line() {
        crate::oplog::enable_oplog_capture_for_test();
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("retention_outcome.db");
        // Unique repo → the process-global (non-draining) capture buffer is filtered to THIS test.
        let repo = "retention-outcome-repo";
        {
            let mut storage = StorageConnection::open(&db_path).unwrap();
            add_repo(&storage, repo);
            let s1 = ready_snapshot(&mut storage, repo, None, 2_000); // prunable (older)
            let s2 = ready_snapshot(&mut storage, repo, Some(&s1), 0);
            let _s3 = ready_snapshot(&mut storage, repo, Some(&s2), 0);
        }
        let db_path = db_path.canonicalize().unwrap();
        let state = isolated_state();

        // Idle DB → the pass RUNS on its first attempt and records its outcome.
        run_auto_retention(&state, &db_path, repo, repo);

        let lines: Vec<String> = crate::oplog::oplog_lines_for_test()
            .into_iter()
            .filter(|l| l.contains(repo))
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("op retention started")),
            "the pass logs an op-START line: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("op retention completed") && l.contains("pruned")),
            "the pass logs an op-lifecycle OUTCOME line (not the old ad-hoc 'retention: …' summary): {lines:?}"
        );
        // The recorded doctor report still reflects the run (unchanged behavior).
        assert!(
            state.last_retention_json().is_some(),
            "the retention report is still recorded for doctor"
        );
    }

    // SNAPSHOT-RETENTION-1 THRESHOLD PROOF (named test): below-threshold reclaim SKIPS the VACUUM
    // (rows gone, file unshrunk, honest report); above-threshold RUNS it (file shrinks).
    #[test]
    fn threshold_below_skips_vacuum_above_runs_it() {
        // BELOW: the bloat lives on the KEPT current snapshot; the pruned snapshot is empty, so
        // pruning it frees a negligible fraction of the file → VACUUM skipped, but the row IS gone.
        {
            let dir = tempdir().unwrap();
            let db_path = dir.path().join("below.db");
            {
                let mut storage = StorageConnection::open(&db_path).unwrap();
                add_repo(&storage, "r1");
                let s1 = ready_snapshot(&mut storage, "r1", None, 0); // prunable, empty
                let s2 = ready_snapshot(&mut storage, "r1", Some(&s1), 0);
                let _s3 = ready_snapshot(&mut storage, "r1", Some(&s2), 8_000); // current, bloated (kept)
            }
            let size_before = file_size(&db_path);
            let storage = StorageConnection::open(&db_path).unwrap();
            let outcome =
                run_retention_pass(&storage, &db_path, "r1", &RepoCoordinator::new()).unwrap();

            assert_eq!(
                outcome.pruned_count, 1,
                "the empty older snapshot IS pruned (rows gone)"
            );
            assert_eq!(
                outcome.vacuum,
                VacuumStatus::SkippedBelowThreshold,
                "below threshold → VACUUM skipped"
            );
            assert_eq!(outcome.reclaimed_bytes, 0, "no VACUUM → no on-disk reclaim");
            assert_eq!(
                file_size(&db_path),
                size_before,
                "honest: the file is NOT shrunk when the reclaim is below threshold"
            );
        }

        // ABOVE: the bloat lives on the PRUNED snapshot, so pruning it frees a large fraction of the
        // file → VACUUM runs and the file shrinks.
        {
            let dir = tempdir().unwrap();
            let db_path = dir.path().join("above.db");
            {
                let mut storage = StorageConnection::open(&db_path).unwrap();
                add_repo(&storage, "r1");
                let s1 = ready_snapshot(&mut storage, "r1", None, 8_000); // prunable, bloated
                let s2 = ready_snapshot(&mut storage, "r1", Some(&s1), 0);
                let _s3 = ready_snapshot(&mut storage, "r1", Some(&s2), 0); // current, empty
            }
            let size_before = file_size(&db_path);
            let storage = StorageConnection::open(&db_path).unwrap();
            let outcome =
                run_retention_pass(&storage, &db_path, "r1", &RepoCoordinator::new()).unwrap();

            assert_eq!(outcome.pruned_count, 1);
            assert_eq!(
                outcome.vacuum,
                VacuumStatus::Ran,
                "above threshold → VACUUM ran"
            );
            assert!(outcome.reclaimed_bytes > 0, "reclaimed bytes reported");
            assert!(
                file_size(&db_path) < size_before,
                "the file actually shrank: before={size_before} after={}",
                file_size(&db_path)
            );
        }
    }

    // ── READERS vs. the VACUUM — the iteration-1 production-interaction proofs ────────────────────

    /// Build a repo whose oldest snapshot dominates the file (a majority reclaim → the VACUUM gate
    /// fires), returning the tempdir (kept alive by the caller), db path, and the isolated
    /// `DaemonState` with the repo loaded. The loaded repo's `coordinator` is the SAME instance a
    /// reader would take `acquire_read` on.
    fn majority_dead_repo() -> (TempDir, PathBuf, DaemonState) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("readers.db");
        {
            let mut storage = StorageConnection::open(&db_path).unwrap();
            add_repo(&storage, "r1");
            let s1 = ready_snapshot(&mut storage, "r1", None, 8_000); // bloated → pruned (majority)
            let s2 = ready_snapshot(&mut storage, "r1", Some(&s1), 0); // parent
            let _s3 = ready_snapshot(&mut storage, "r1", Some(&s2), 0); // current
        }
        let db_path = db_path.canonicalize().unwrap();
        let state = isolated_state();
        // Load the repo so its shared coordinator exists (what the pass will engage for the VACUUM).
        state.load_repo(&db_path, "r1").unwrap();
        (dir, db_path, state)
    }

    // PRODUCTION-INTERACTION PROOF #1 — a reader that is ALREADY mid-request makes the pass DEFER the
    // VACUUM (never yanking the exclusive lock out from under it → never a `SQLITE_BUSY`), while the
    // prune STILL happens. Then, with the reader gone, the VACUUM runs. This is the "reader present →
    // honest defer, correct answer" half of the operator's requirement.
    #[test]
    fn vacuum_defers_to_an_active_reader_then_runs_when_idle() {
        let (_dir, db_path, state) = majority_dead_repo();
        let repo_state = state.load_repo(&db_path, "r1").unwrap();

        // A real coordinated reader is mid-request (holds the read-lock, exactly as a `dispatch` read
        // handler does for the whole request).
        let read_guard = repo_state.coordinator.acquire_read();

        // The reader can still read the DB correctly right now (WAL) — prove there is no busy error.
        {
            let s = repo_state.storage().unwrap();
            assert_eq!(
                s.list_snapshots("r1").unwrap().len(),
                3,
                "the reader sees all snapshots pre-prune, no busy error"
            );
        }

        // Scope the pass's connection exactly as `try_retention_attempt` does — one connection per
        // attempt, dropped when it returns. (A DELETE-mode VACUUM needs EXCLUSIVE access: no other
        // connection open. The coordinator `Writing` guard keeps readers from opening one during the
        // VACUUM; this scoping keeps the deferred pass's own connection from lingering into the next.)
        let outcome = {
            let storage = StorageConnection::open(&db_path).unwrap();
            run_retention_pass(&storage, &db_path, "r1", &repo_state.coordinator).unwrap()
        };

        // The prune HAPPENED (rows gone) but the VACUUM DEFERRED — the reader was never disturbed.
        assert_eq!(
            outcome.pruned_count, 1,
            "prune runs even with a reader active (WAL-safe)"
        );
        assert_eq!(
            outcome.vacuum,
            VacuumStatus::DeferredReadersActive,
            "a reader holding the read-lock → VACUUM deferred, NOT a busy error"
        );
        assert_eq!(
            outcome.reclaimed_bytes, 0,
            "deferred VACUUM reclaims nothing yet"
        );
        // The reader, still holding its guard, reads the CORRECT post-prune set — no wrong answer.
        {
            let s = repo_state.storage().unwrap();
            assert_eq!(
                s.list_snapshots("r1")
                    .unwrap()
                    .iter()
                    .filter(|s| s.status == "ready")
                    .count(),
                2,
                "reader now sees current+parent (pruned), still no busy error"
            );
        }

        // Reader leaves → the next pass VACUUMs (the deferred reclaim is realised).
        drop(read_guard);
        let outcome2 = {
            let storage = StorageConnection::open(&db_path).unwrap();
            run_retention_pass(&storage, &db_path, "r1", &repo_state.coordinator).unwrap()
        };
        assert_eq!(
            outcome2.vacuum,
            VacuumStatus::Ran,
            "with no reader, the deferred VACUUM now runs"
        );
        assert!(outcome2.reclaimed_bytes > 0, "and reclaims disk");
    }

    // PRODUCTION-INTERACTION PROOF #2 — a reader that ARRIVES during the VACUUM window BLOCKS honestly
    // and then reads the CORRECT data — never a raw `database is locked`. This deterministically
    // replicates `run_retention_pass`'s VACUUM-under-`Writing` step (holding the `Writing` guard while
    // a real DELETE-mode VACUUM runs) so a real reader can be injected at the exact exclusive-lock
    // window that iteration-0 exposed. The read path is the real one: `acquire_read` → open storage →
    // query.
    #[test]
    fn reader_arriving_during_vacuum_window_blocks_then_reads_correct_data() {
        use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
        use std::sync::Arc as StdArc;

        let (_dir, db_path, state) = majority_dead_repo();
        let repo_state = state.load_repo(&db_path, "r1").unwrap();

        // Do the WAL-safe prune first (the pass's steps 1–2), leaving a majority freelist for the VACUUM.
        {
            let storage = StorageConnection::open(&db_path).unwrap();
            storage.classify_repo_retention("r1").unwrap();
            assert_eq!(storage.prune_prunable_snapshots("r1").unwrap(), 1);
        }

        let started = StdArc::new(AtomicBool::new(false));
        let done = StdArc::new(AtomicBool::new(false));

        // The pass holds `Writing` (the exclusive-lock window) exactly as `run_retention_pass` does.
        let writing = repo_state.coordinator.acquire_write();

        // Scoped thread so the reader can borrow the coordinator on `repo_state`.
        let ready = std::thread::scope(|scope| {
            let t_started = StdArc::clone(&started);
            let t_done = StdArc::clone(&done);
            let coordinator = &repo_state.coordinator;
            let t_db = db_path.clone();
            let handle = scope.spawn(move || {
                t_started.store(true, AtomicOrdering::SeqCst);
                // The REAL production read seam: coordinated read-lock, then open + query.
                let _rg = coordinator.acquire_read(); // BLOCKS while `Writing` is held
                let storage = StorageConnection::open(&t_db).unwrap();
                let ready = storage
                    .list_snapshots("r1")
                    .unwrap()
                    .into_iter()
                    .filter(|s| s.status == "ready")
                    .count();
                t_done.store(true, AtomicOrdering::SeqCst);
                ready
            });

            // Wait until the reader is at/around `acquire_read`, then confirm it is BLOCKED.
            while !started.load(AtomicOrdering::SeqCst) {
                std::thread::yield_now();
            }
            std::thread::sleep(Duration::from_millis(100));
            assert!(
                !done.load(AtomicOrdering::SeqCst),
                "a reader arriving during the VACUUM window must BLOCK on acquire_read, not error"
            );

            // Run the REAL DELETE-mode VACUUM under `Writing` (the exclusive-lock operation).
            {
                let storage = StorageConnection::open(&db_path).unwrap();
                storage.vacuum().unwrap();
            }
            // Release `Writing` → the blocked reader proceeds.
            drop(writing);
            handle.join().unwrap()
        });

        assert!(
            done.load(AtomicOrdering::SeqCst),
            "the reader unblocked and completed"
        );
        assert_eq!(
            ready, 2,
            "the reader read the CORRECT post-prune data (current+parent) after the VACUUM — \
             no wrong answer, no `database is locked`"
        );
    }
}
