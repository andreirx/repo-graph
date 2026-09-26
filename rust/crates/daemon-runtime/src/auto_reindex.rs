//! TOOLCHAIN-STALENESS-1 (RG-REQ-001-L06, D-STALE-SIGNAL-1, D-TS-RUNTIME-1): the lazy automatic
//! background FULL re-index on first use of a repository whose snapshot toolchain stamp differs from
//! the running rmap (or is unreadable), and the per-request toolchain status `orient`/`check` attach.
//!
//! # Contract (L06, slice §2.1 LD-03..LD-13)
//! - **First use** ([`note_first_use`], called by `DaemonState::load_repo` on a cache miss and in its
//!   rebuild-sentinel refusal branch): enqueue one stamp-check job for `(db, repo_uid)` if this process
//!   has none for it and the re-index is enabled. In memory only — the load path reads nothing.
//! - **The worker** (one thread per `DaemonState`, started by `ServiceDispatcher::new`, holding only a
//!   `Weak<DaemonState>`): wakes on every enqueue and at least every [`WORKER_TICK`]; walks the queue
//!   FIFO, skipping jobs whose gate is closed; runs one job at a time (so "no other automatic re-index
//!   is running" holds by construction); never drops a queued job.
//! - **The gate** (per repository): no `index`/`refresh` activity op on its db; no rebuild — the
//!   sentinel absent AND no explicit `index`/`refresh`/`repo_rebuild` request naming the repository in
//!   flight ([`explicit_write_guard`], held by `ServiceDispatcher::dispatch`). Rebuild and refresh load
//!   the repository BEFORE they lock it, so without the guard the check their own load queues would
//!   start an index under them.
//! - **Running a job**: re-read the latest READY snapshot's stamp through the gated open. Lock
//!   contention or a rebuild that began → the job stays queued for a later tick; any other read
//!   fault → failed with its reason (never retried, never an endless `queued`). Current or
//!   no ready snapshot → finished, no index. Differs or unreadable → mark running, call the test start
//!   hook, and dispatch an ordinary flagless `index` request (`repo_path` = the registry's canonical
//!   path) through the same `ServiceDispatcher::dispatch` an explicit `rmap index` reaches — never
//!   `rmap repo rebuild`. Success → finished; an error → failed with its message, never retried in
//!   this process (the job record outlives unload/reload).
//! - **Status** ([`orient_status`], [`check_status`]): compare the stamp of the snapshot the request
//!   SERVES (orient's served uid; the uid check's reducer evaluated, read by uid on its connection)
//!   with the running stamp; `current`, or `stale`/`unknown` with the re-index state: `disabled`
//!   (opt-out, then stdio), `failed` (the job's failure, or a worker that could not start or
//!   stopped), `queued`/`running`; with no job or a finished one, enqueue one and report `queued`.
//! - **stdio** ([`mark_stdio_transport`]): a stdio daemon exits at its client's EOF, so it schedules
//!   nothing and reports `disabled` with [`STDIO_DISABLED_REASON`] (D-TS-RUNTIME-1 §1).
//!
//! # Abstraction record
//! - what: the first-use scheduling, the single worker, the gate, the explicit-write guard and the
//!   status composition;
//! - concrete users: `state.rs` `load_repo` (the two hooks), `dispatch.rs` (worker start, the guard,
//!   the orient and check attach calls), `lib.rs` (the stdio mark);
//! - force: `state.rs` and `dispatch.rs` are far past the 500-line guardrail, and `reconcile.rs` /
//!   `enrich_pass.rs` are the precedent for a background pass in its own module;
//! - simpler alternative rejected: inlining the scheduler into `state.rs`/`dispatch.rs`.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use parking_lot::{Condvar, Mutex};
use repo_graph_agent::dto::toolchain_staleness::{
    compare_toolchain_stamps, ReindexState, SnapshotStamp, StampComparison, ToolchainStaleness,
};
use repo_graph_daemon_transport::{DispatchResult, Dispatcher, NoOpEmitter, Request};
use repo_graph_storage::StorageConnection;
use serde_json::Value;

use crate::activity::OpKind;
use crate::dispatch::ServiceDispatcher;
use crate::state::{DaemonState, RepoState};

/// The worker's wake-up period when nothing enqueues (the `REQUEUE_BACKOFF` value of the enrich
/// pass): a queued job whose gate opens starts within about this long, without another request.
pub const WORKER_TICK: Duration = Duration::from_millis(1000);

/// `reindex_disabled_reason` when `RMAP_AUTO_REINDEX` is off.
pub const OPT_OUT_DISABLED_REASON: &str = "RMAP_AUTO_REINDEX=off";

/// `reindex_disabled_reason` of a stdio daemon (D-TS-RUNTIME-1 §1).
pub const STDIO_DISABLED_REASON: &str =
    "stdio transport \u{2014} a background re-index cannot outlive the request";

// ─────────────────────────────────────────────────────────────────────────────
// Opt-out switch — RMAP_AUTO_REINDEX, default ON (parsed exactly as RMAP_AUTO_ENRICH)
// ─────────────────────────────────────────────────────────────────────────────

/// Whether the automatic re-index is enabled, read per decision. Set `RMAP_AUTO_REINDEX` to
/// `0`/`false`/`off`/`no`/`disabled` (case-insensitive) to disable; any other value or unset → on.
pub fn auto_reindex_enabled() -> bool {
    match AUTO_REINDEX_OVERRIDE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => auto_reindex_enabled_from(std::env::var("RMAP_AUTO_REINDEX").ok().as_deref()),
    }
}

/// Test override for [`auto_reindex_enabled`]: 0 = none (use env), 1 = force on, 2 = force off.
static AUTO_REINDEX_OVERRIDE: AtomicU8 = AtomicU8::new(0);

/// TEST SEAM — force the automatic re-index on (`Some(true)`), off (`Some(false)`), or back to the
/// environment (`None`) for the current test binary (an atomic, never the process environment).
/// `#[doc(hidden)]`, `_for_test`-named: no production caller.
#[doc(hidden)]
pub fn set_auto_reindex_for_test(enabled: Option<bool>) {
    let v = match enabled {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    };
    AUTO_REINDEX_OVERRIDE.store(v, Ordering::Relaxed);
}

/// Pure core of [`auto_reindex_enabled`] (env value in, decision out).
fn auto_reindex_enabled_from(val: Option<&str>) -> bool {
    match val {
        Some(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no" | "disabled"
        ),
        None => true,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Start-hook test seam — production always None
// ─────────────────────────────────────────────────────────────────────────────

type StartHook = Arc<dyn Fn(&Path) + Send + Sync>;

static START_HOOK: Mutex<Option<StartHook>> = parking_lot::const_mutex(None);

/// TEST SEAM — a hook the worker calls with the repository's canonical path after marking a job
/// running and before dispatching its index (a test parks the job there to observe `running`).
/// No production caller.
#[doc(hidden)]
pub fn set_start_hook_for_test<F>(hook: F)
where
    F: Fn(&Path) + Send + Sync + 'static,
{
    *START_HOOK.lock() = Some(Arc::new(hook));
}

/// TEST SEAM — remove the start hook (see [`set_start_hook_for_test`]).
#[doc(hidden)]
pub fn clear_start_hook_for_test() {
    *START_HOOK.lock() = None;
}

type ReadHook = Arc<dyn Fn() + Send + Sync>;

static SERVED_STAMP_READ_HOOK: Mutex<Option<ReadHook>> = parking_lot::const_mutex(None);

/// TEST SEAM — a hook `check` calls after its reducer returned and immediately before it reads the
/// evaluated snapshot's stamp (a test promotes a newer ready snapshot there). No production caller.
#[doc(hidden)]
pub fn set_served_stamp_read_hook_for_test<F>(hook: F)
where
    F: Fn() + Send + Sync + 'static,
{
    *SERVED_STAMP_READ_HOOK.lock() = Some(Arc::new(hook));
}

/// TEST SEAM — remove the served-stamp read hook (see [`set_served_stamp_read_hook_for_test`]).
#[doc(hidden)]
pub fn clear_served_stamp_read_hook_for_test() {
    *SERVED_STAMP_READ_HOOK.lock() = None;
}

fn call_served_stamp_read_hook() {
    let hook = SERVED_STAMP_READ_HOOK.lock().clone();
    if let Some(hook) = hook {
        hook();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Coordinator
// ─────────────────────────────────────────────────────────────────────────────

/// Where a repository's job stands in this process.
#[derive(Debug, Clone, PartialEq, Eq)]
enum JobState {
    Queued,
    Running,
    /// Ended without a failure (current stamp, no ready snapshot, a forgotten repository, or a
    /// completed re-index).
    Finished,
    /// The automatic index failed; never retried in this process.
    Failed(String),
}

#[derive(Debug)]
struct Job {
    /// The db path keyed through [`job_key`].
    db_key: PathBuf,
    repo_uid: String,
    state: JobState,
}

#[derive(Debug, Default)]
struct Queue {
    /// FIFO; a job record stays after it ends so the process remembers it had one.
    jobs: Vec<Job>,
    /// In-flight explicit `index`/`refresh`/`repo_rebuild` requests, by db key.
    explicit_writes: Vec<PathBuf>,
}

impl Queue {
    fn job_mut(&mut self, db_key: &Path, repo_uid: &str) -> Option<&mut Job> {
        self.jobs
            .iter_mut()
            .find(|j| j.db_key == db_key && j.repo_uid == repo_uid)
    }
}

#[derive(Debug, Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    worker_started: AtomicBool,
    stdio: AtomicBool,
    /// Why this process's worker cannot run jobs (it could not start, or it stopped); once set,
    /// every stale or unknown status reports `failed` with it instead of an unstartable `queued`.
    worker_failure: Mutex<Option<String>>,
}

/// The daemon-global automatic re-index coordination (owned by `DaemonState`). Interior-mutable, so
/// it does not affect `DaemonState: Send + Sync`.
#[derive(Debug, Default)]
pub struct AutoReindexCoordinator {
    shared: Arc<Shared>,
}

impl AutoReindexCoordinator {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The identity a job is keyed by: the db path with its directory canonicalized (the file itself
/// may be absent mid-rebuild; its directory is not), so registry-form and canonical-form paths of
/// one store name one job. When the directory cannot be canonicalized the path is used as given.
fn job_key(db_path: &Path) -> PathBuf {
    match (db_path.parent(), db_path.file_name()) {
        (Some(dir), Some(name)) => match std::fs::canonicalize(dir) {
            Ok(dir) => dir.join(name),
            Err(_) => db_path.to_path_buf(),
        },
        _ => db_path.to_path_buf(),
    }
}

/// The running rmap's toolchain stamp — the stamp this build writes on every index and refresh.
fn running_stamp() -> &'static str {
    static RUNNING: OnceLock<String> = OnceLock::new();
    RUNNING.get_or_init(repo_graph_repo_index::toolchain::running_toolchain_json)
}

/// Mark this daemon as a stdio daemon (LD-12): it schedules nothing and reports `disabled`.
/// Called by `run_daemon_stdio` before the dispatcher (and its worker) is created.
pub fn mark_stdio_transport(state: &DaemonState) {
    state
        .auto_reindex()
        .shared
        .stdio
        .store(true, Ordering::Relaxed);
}

/// LD-03: note the first use of `(db_path, repo_uid)` in this process — enqueue one stamp-check job
/// if this process has none for it and the automatic re-index is enabled. Reads no storage.
pub(crate) fn note_first_use(state: &DaemonState, db_path: &Path, repo_uid: &str) {
    let shared = &state.auto_reindex().shared;
    if shared.stdio.load(Ordering::Relaxed) || !auto_reindex_enabled() {
        return;
    }
    let key = job_key(db_path);
    let mut queue = shared.queue.lock();
    if queue.job_mut(&key, repo_uid).is_none() {
        queue.jobs.push(Job {
            db_key: key,
            repo_uid: repo_uid.to_string(),
            state: JobState::Queued,
        });
        shared.wake.notify_all();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Explicit-write guard (LD-05 b)
// ─────────────────────────────────────────────────────────────────────────────

/// Held by `ServiceDispatcher::dispatch` for the duration of an explicit `index`, `refresh` or
/// `repo_rebuild` request naming a registered repository; while held, that repository's automatic
/// re-index does not start.
pub struct ExplicitWriteGuard {
    shared: Arc<Shared>,
    db_key: PathBuf,
}

impl Drop for ExplicitWriteGuard {
    fn drop(&mut self) {
        let mut queue = self.shared.queue.lock();
        if let Some(at) = queue.explicit_writes.iter().position(|k| *k == self.db_key) {
            queue.explicit_writes.swap_remove(at);
        }
        self.shared.wake.notify_all();
    }
}

/// The explicit-write guard for `request`, or `None` when it is not an `index`/`refresh`/
/// `repo_rebuild` request or its repository parameter (`repo_path` for index, `repo` otherwise)
/// resolves to no registered repository — such a request cannot name a repository that has a job.
pub(crate) fn explicit_write_guard(
    state: &DaemonState,
    request: &Request,
) -> Option<ExplicitWriteGuard> {
    let param = match request.method.as_str() {
        "index" => "repo_path",
        "refresh" | "repo_rebuild" => "repo",
        _ => return None,
    };
    let reference = request.params.get(param).and_then(Value::as_str)?;
    let entry = state.resolve_alias_or_path(reference)?;
    let shared = Arc::clone(&state.auto_reindex().shared);
    let db_key = job_key(&entry.db_path);
    shared.queue.lock().explicit_writes.push(db_key.clone());
    Some(ExplicitWriteGuard { shared, db_key })
}

// ─────────────────────────────────────────────────────────────────────────────
// Status (LD-07)
// ─────────────────────────────────────────────────────────────────────────────

/// The toolchain status `orient` attaches: the stamp of the snapshot it serves (`snapshot_uid`)
/// against the running stamp. A failed read is `unknown` with the read error — never current.
pub(crate) fn orient_status(
    state: &DaemonState,
    storage: &StorageConnection,
    repo_state: &RepoState,
    repo_uid: &str,
    snapshot_uid: &str,
) -> ToolchainStaleness {
    let comparison = compare_snapshot_stamp(storage, snapshot_uid);
    compose_status(state, &repo_state.key.db_path, repo_uid, comparison)
}

/// The stamp comparison of the snapshot `check`'s reducer EVALUATED — `snapshot_uid` is the one its
/// result reports (empty when there was no ready snapshot → `None`, and `check` attaches nothing).
/// Read by uid on the check's own connection, never by a second "latest" lookup, so a snapshot
/// promoted while `check` runs can never be described in place of the one it evaluated.
pub(crate) fn compare_evaluated_snapshot_stamp(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Option<StampComparison> {
    if snapshot_uid.is_empty() {
        return None;
    }
    call_served_stamp_read_hook();
    Some(compare_snapshot_stamp(storage, snapshot_uid))
}

/// The toolchain status `check` attaches, composed from [`compare_evaluated_snapshot_stamp`]'s
/// comparison with the daemon's re-index state; `None` when there was no ready snapshot.
pub(crate) fn check_status(
    state: &DaemonState,
    repo_state: &RepoState,
    evaluated: Option<StampComparison>,
) -> Option<ToolchainStaleness> {
    let (db_path, repo_uid) = (&repo_state.key.db_path, repo_state.key.repo_uid.as_str());
    evaluated.map(|comparison| compose_status(state, db_path, repo_uid, comparison))
}

/// The stamp of snapshot `snapshot_uid` against the running stamp; a failed or empty read is
/// `unknown` with its reason.
fn compare_snapshot_stamp(storage: &StorageConnection, snapshot_uid: &str) -> StampComparison {
    match storage.get_snapshot(snapshot_uid) {
        Ok(Some(snapshot)) => compare_served(snapshot.toolchain_json.as_deref()),
        Ok(None) => compare_toolchain_stamps(
            SnapshotStamp::ReadFailed("the served snapshot's row was not found"),
            running_stamp(),
        ),
        Err(e) => {
            compare_toolchain_stamps(SnapshotStamp::ReadFailed(&e.to_string()), running_stamp())
        }
    }
}

/// Attach `check`'s status as the additive `value.toolchain_staleness` member (nothing when there is
/// no ready snapshot). Never a condition: the verdict and exit code do not read it.
pub(crate) fn attach_check_status(
    output: &mut Value,
    status: Option<&ToolchainStaleness>,
    repo_uid: &str,
) {
    if let Some(status) = status {
        crate::dispatch::inject_value_field(output, "toolchain_staleness", status, repo_uid);
    }
}

fn compare_served(stamp: Option<&str>) -> StampComparison {
    let stamp = match stamp {
        Some(text) => SnapshotStamp::Present(text),
        None => SnapshotStamp::Missing,
    };
    compare_toolchain_stamps(stamp, running_stamp())
}

fn compose_status(
    state: &DaemonState,
    db_path: &Path,
    repo_uid: &str,
    comparison: StampComparison,
) -> ToolchainStaleness {
    if comparison == StampComparison::Current {
        return ToolchainStaleness::Current;
    }
    let reindex = reindex_state_for_request(state, db_path, repo_uid);
    ToolchainStaleness::from_comparison(comparison, reindex)
}

/// The re-index state of a stale or unknown stamp. With no job, or a finished one (a request that
/// pinned an older snapshot while a job completed, or a snapshot another writer produced), enqueue
/// one and report `queued`. A failed job is never re-enqueued in this process.
fn reindex_state_for_request(state: &DaemonState, db_path: &Path, repo_uid: &str) -> ReindexState {
    let shared = &state.auto_reindex().shared;
    if !auto_reindex_enabled() {
        return ReindexState::Disabled {
            reason: OPT_OUT_DISABLED_REASON.to_string(),
        };
    }
    if shared.stdio.load(Ordering::Relaxed) {
        return ReindexState::Disabled {
            reason: STDIO_DISABLED_REASON.to_string(),
        };
    }
    let worker_failure = shared.worker_failure.lock().clone();
    let key = job_key(db_path);
    let mut queue = shared.queue.lock();
    let existing = queue.job_mut(&key, repo_uid).map(|j| j.state.clone());
    if let Some(JobState::Failed(failure)) = existing {
        return ReindexState::Failed { failure };
    }
    // No worker can run a job: `queued`/`running` would never end, so the status is `failed`.
    if let Some(failure) = worker_failure {
        return ReindexState::Failed { failure };
    }
    match existing {
        Some(JobState::Queued) => ReindexState::Queued,
        Some(JobState::Running) => ReindexState::Running,
        Some(JobState::Failed(failure)) => ReindexState::Failed { failure },
        Some(JobState::Finished) | None => {
            queue
                .jobs
                .retain(|j| !(j.db_key == key && j.repo_uid == repo_uid));
            queue.jobs.push(Job {
                db_key: key,
                repo_uid: repo_uid.to_string(),
                state: JobState::Queued,
            });
            shared.wake.notify_all();
            ReindexState::Queued
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Worker (LD-04..LD-06)
// ─────────────────────────────────────────────────────────────────────────────

/// Start this daemon's worker once (idempotent). A stdio daemon starts none.
pub(crate) fn start_worker(state: &Arc<DaemonState>) {
    let shared = Arc::clone(&state.auto_reindex().shared);
    if shared.stdio.load(Ordering::Relaxed) || shared.worker_started.swap(true, Ordering::AcqRel) {
        return;
    }
    let weak = Arc::downgrade(state);
    let spawned = std::thread::Builder::new()
        .name("rmapd-auto-reindex".to_string())
        .spawn(move || worker_loop(weak, shared));
    if let Err(e) = spawned {
        record_worker_failure(
            &state.auto_reindex().shared,
            format!("the automatic re-index worker could not start: {e}"),
        );
    }
}

/// Record why this process's worker cannot run jobs (see [`Shared::worker_failure`]); the first
/// reason is kept.
fn record_worker_failure(shared: &Shared, reason: String) {
    eprintln!("warning: {reason}; stale indexes report the re-index as failed in this process");
    let mut failure = shared.worker_failure.lock();
    if failure.is_none() {
        *failure = Some(reason);
    }
}

/// The worker thread: runs [`worker_jobs`]; if it unwinds (a panic outside the index dispatch, which
/// has its own guard), no job can run any more, so the failure is recorded rather than leaving
/// queued jobs reported `queued` forever.
fn worker_loop(weak: Weak<DaemonState>, shared: Arc<Shared>) {
    let jobs_shared = Arc::clone(&shared);
    if std::panic::catch_unwind(AssertUnwindSafe(|| worker_jobs(weak, jobs_shared))).is_err() {
        record_worker_failure(
            &shared,
            "the automatic re-index worker stopped unexpectedly".to_string(),
        );
    }
}

fn worker_jobs(weak: Weak<DaemonState>, shared: Arc<Shared>) {
    loop {
        let Some(state) = weak.upgrade() else {
            return;
        };
        let worked = run_one_job(&state, &shared);
        drop(state);
        if !worked {
            let mut queue = shared.queue.lock();
            shared.wake.wait_for(&mut queue, WORKER_TICK);
        }
    }
}

/// Is this repository's gate open (LD-05)? Called with the queue locked.
fn gate_open(state: &DaemonState, queue: &Queue, db_key: &Path) -> bool {
    if queue.explicit_writes.iter().any(|k| k == db_key) {
        return false;
    }
    if crate::rebuild::sentinel_present(db_key) {
        return false;
    }
    !state.activity().snapshot().iter().any(|op| {
        matches!(op.kind, OpKind::Index | OpKind::Refresh) && job_key(&op.db_path) == db_key
    })
}

/// The first queued job whose gate is open, in FIFO order.
fn next_startable(state: &DaemonState, shared: &Shared) -> Option<(PathBuf, String)> {
    let queue = shared.queue.lock();
    queue
        .jobs
        .iter()
        .filter(|j| j.state == JobState::Queued)
        .find(|j| gate_open(state, &queue, &j.db_key))
        .map(|j| (j.db_key.clone(), j.repo_uid.clone()))
}

fn set_job_state(shared: &Shared, db_key: &Path, repo_uid: &str, to: JobState) {
    if let Some(job) = shared.queue.lock().job_mut(db_key, repo_uid) {
        job.state = to;
    }
}

/// What the worker found when it read a queued job's stamp.
enum StampCheck {
    /// Nothing to re-index (current stamp, no ready snapshot, or a forgotten repository).
    Nothing,
    /// The stamp differs or is unreadable; re-index this canonical repository path.
    Reindex(String),
    /// The store is busy or a rebuild of it began; a later tick can read it — the job stays queued.
    Contended,
    /// The store cannot be read (missing, corrupt, any fault but lock contention): the job fails
    /// with `reason` (in the reader's frame, no store path) and is not retried in this process;
    /// `detail` is the raw error for the daemon log.
    Unreadable { reason: String, detail: String },
}

/// Is a failed store read SQLite's own lock/busy signal — the only class a later attempt can clear?
/// The same rule the bounded-busy-retry open applies (`state::open_existing_with_busy_retry`).
fn is_lock_contention(error: &str) -> bool {
    error.contains("locked") || error.contains("busy")
}

/// The `reindex_failure` of a store the worker cannot read (LD-02's read-failure wording).
fn unreadable(what: &str, detail: String) -> StampCheck {
    StampCheck::Unreadable {
        reason: format!("could not read this index's toolchain stamp: {what}"),
        detail,
    }
}

fn read_stamp(state: &DaemonState, db_key: &Path, repo_uid: &str) -> StampCheck {
    let entry = state
        .registry()
        .list()
        .into_iter()
        .find(|e| e.repo_uid == repo_uid && job_key(&e.db_path) == db_key)
        .cloned();
    let Some(entry) = entry else {
        return StampCheck::Nothing;
    };
    let storage = match crate::state::open_existing_with_busy_retry(
        db_key,
        crate::state::OpenPatience::Background,
    ) {
        Ok(s) => s,
        Err(crate::state::OpenError::LockedAfterRetries { .. }) => return StampCheck::Contended,
        // A rebuild began after the gate was read: the gate holds the job until it ends.
        Err(_) if crate::rebuild::sentinel_present(db_key) => return StampCheck::Contended,
        Err(e) => return unreadable("its store could not be opened", e.to_string()),
    };
    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => return StampCheck::Nothing,
        Err(e) if is_lock_contention(&e.to_string()) => return StampCheck::Contended,
        Err(e) => return unreadable("its snapshots could not be read", e.to_string()),
    };
    match compare_served(snapshot.toolchain_json.as_deref()) {
        StampComparison::Current => StampCheck::Nothing,
        StampComparison::Differs(_) | StampComparison::Unreadable(_) => {
            StampCheck::Reindex(entry.canonical_path.to_string_lossy().to_string())
        }
    }
}

/// Run at most one job. Returns whether a job was started or ended (so the loop looks again at
/// once) — `false` means nothing could start now.
fn run_one_job(state: &Arc<DaemonState>, shared: &Shared) -> bool {
    let Some((db_key, repo_uid)) = next_startable(state, shared) else {
        return false;
    };
    let repo_path = match read_stamp(state, &db_key, &repo_uid) {
        StampCheck::Nothing => {
            set_job_state(shared, &db_key, &repo_uid, JobState::Finished);
            return true;
        }
        StampCheck::Unreadable { reason, detail } => {
            eprintln!("warning: automatic re-index check failed (not retried in this process): {reason}: {detail}");
            set_job_state(shared, &db_key, &repo_uid, JobState::Failed(reason));
            return true;
        }
        StampCheck::Contended => {
            // Move it behind the other queued jobs so one contended store cannot hold them up.
            let mut queue = shared.queue.lock();
            if let Some(at) = queue
                .jobs
                .iter()
                .position(|j| j.db_key == db_key && j.repo_uid == repo_uid)
            {
                let job = queue.jobs.remove(at);
                queue.jobs.push(job);
            }
            return false;
        }
        StampCheck::Reindex(path) => path,
    };
    // Re-check the gate and mark running atomically: an explicit write that began while the stamp
    // was read goes first.
    {
        let mut queue = shared.queue.lock();
        if !gate_open(state, &queue, &db_key) {
            return false;
        }
        match queue.job_mut(&db_key, &repo_uid) {
            Some(job) if job.state == JobState::Queued => job.state = JobState::Running,
            _ => return true,
        }
    }
    let hook = START_HOOK.lock().clone();
    if let Some(hook) = hook {
        hook(Path::new(&repo_path));
    }
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let dispatcher = ServiceDispatcher::new(Arc::clone(state));
        let request = Request {
            id: "auto-reindex".to_string(),
            method: "index".to_string(),
            params: serde_json::json!({ "repo_path": repo_path }),
        };
        dispatcher.dispatch(&request, &mut NoOpEmitter)
    }));
    let end = match outcome {
        Ok(DispatchResult::Success(_)) => JobState::Finished,
        Ok(DispatchResult::Error(response)) => JobState::Failed(response.error.message),
        Err(_) => JobState::Failed("the automatic re-index panicked".to_string()),
    };
    if let JobState::Failed(reason) = &end {
        eprintln!("warning: automatic re-index of {repo_path} failed (not retried in this process): {reason}");
    }
    set_job_state(shared, &db_key, &repo_uid, end);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_reindex_enabled_from_parses_the_opt_out_values_and_defaults_on() {
        assert!(auto_reindex_enabled_from(None));
        for off in [
            "0",
            "false",
            "off",
            "no",
            "disabled",
            "OFF",
            " Disabled ",
            "False",
        ] {
            assert!(!auto_reindex_enabled_from(Some(off)), "{off}");
        }
        for on in ["1", "true", "on", "yes", "", "anything"] {
            assert!(auto_reindex_enabled_from(Some(on)), "{on}");
        }
    }

    #[test]
    fn only_lock_contention_on_a_stamp_read_is_retried() {
        for transient in [
            "database is locked",
            "database table is locked",
            "database is busy",
        ] {
            assert!(is_lock_contention(transient), "{transient}");
        }
        for terminal in [
            "database file does not exist: /x/y.db",
            "file is not a database",
            "disk I/O error",
        ] {
            assert!(!is_lock_contention(terminal), "{terminal}");
            let StampCheck::Unreadable { reason, detail } =
                unreadable("its store could not be opened", terminal.to_string())
            else {
                panic!("a non-lock fault is terminal");
            };
            assert_eq!(
                reason,
                "could not read this index's toolchain stamp: its store could not be opened"
            );
            assert_eq!(
                detail, terminal,
                "the raw error goes to the daemon log only"
            );
        }
    }

    #[test]
    fn worker_failure_reports_failed_with_its_reason_never_queued() {
        if !auto_reindex_enabled() {
            return; // RMAP_AUTO_REINDEX=off in the test environment: `disabled` wins by contract.
        }
        let root = tempfile::tempdir().expect("tempdir");
        let registry = crate::RepoRegistry::with_state_root(root.path()).expect("registry");
        let state = DaemonState::with_registry(registry);
        let db = root.path().join("store.db");
        note_first_use(&state, &db, "repo_x");
        assert_eq!(
            reindex_state_for_request(&state, &db, "repo_x"),
            ReindexState::Queued
        );
        let reason = "the automatic re-index worker could not start: no threads".to_string();
        record_worker_failure(&state.auto_reindex().shared, reason.clone());
        for _ in 0..2 {
            assert_eq!(
                reindex_state_for_request(&state, &db, "repo_x"),
                ReindexState::Failed {
                    failure: reason.clone()
                }
            );
            // A repository with no job yet reports the same failure, not a fresh `queued`.
            assert_eq!(
                reindex_state_for_request(&state, &db, "repo_y"),
                ReindexState::Failed {
                    failure: reason.clone()
                }
            );
        }
        // The first reason is kept.
        record_worker_failure(&state.auto_reindex().shared, "later".to_string());
        assert_eq!(
            reindex_state_for_request(&state, &db, "repo_x"),
            ReindexState::Failed { failure: reason }
        );
    }
}
