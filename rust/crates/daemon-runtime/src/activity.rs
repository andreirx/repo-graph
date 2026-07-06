//! DAEMON-VISIBILITY-1 (contract D): the daemon's in-flight operation record.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** a daemon-global registry of the write operations currently in flight
//!   (index / refresh / enrich), each carrying op kind, the repo it targets, when it
//!   started, and its live phase + file counters.
//! - **Concrete current users:** stamped by `dispatch::handle_index` / `handle_refresh` /
//!   `handle_enrich`; read by `dispatch::handle_daemon_info` (D — `rmap doctor` status line),
//!   `handlers::metrics::handle_storage_health` (E — "in use by daemon" contention truth),
//!   and `snapshot_facts` (F — "is this repo being indexed right now?").
//! - **Named axis of variation:** the coordinator state machine
//!   (`daemon-policy::CoordinatorState`) records only a *class* (Idle/Reading/Writing/…) and,
//!   critically, an *initial* `index` coordinates on the DB-level `Mutex<()>` — **not** the
//!   `RepoCoordinator` — so `coordinator.state()` does not even reflect a first index and never
//!   carries op-kind / repo / started-at / phase / counters.
//! - **Rejected simpler alternative:** read `RepoCoordinator::state()`. Rejected: it is blind to
//!   an initial index and carries none of the fields the visibility surface must report.
//!
//! This is **exposure, not instrumentation**: the phase + counters already flow through the
//! index/refresh/enrich progress callback (`ProgressEvent{phase,current,total}` →
//! `emitter.emit`); this module only *tees* that stream into a readable record. It adds no new
//! bookkeeping to the pipeline and does not touch the coordinator / W-B epoch invariants.
//!
//! Placed in its own module (not `dispatch.rs`/`state.rs`, both far over the 500-line structural
//! guardrail) mirroring the `resource_metrics` precedent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use serde_json::{json, Value};

/// The kind of in-flight write operation. Reader-frame verbs (VISION: "labels speak the reader's
/// language") — the operator sees "indexing <repo>", not an internal method name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Index,
    Refresh,
    Enrich,
    /// SNAPSHOT-RETENTION-1: the background snapshot-retention pass (prune + threshold VACUUM). Not a
    /// client request — the daemon spawns it after a successful index/refresh — but stamped in the
    /// SAME registry so `rmap doctor` shows it as an in-flight op like any other write, and so the
    /// two-gate contention check (`active_for_db`) sees a concurrent index/refresh.
    Retention,
}

impl OpKind {
    /// Present-progressive verb for human rendering ("indexing <repo>: …").
    pub fn gerund(self) -> &'static str {
        match self {
            OpKind::Index => "indexing",
            OpKind::Refresh => "refreshing",
            OpKind::Enrich => "enriching",
            OpKind::Retention => "reclaiming",
        }
    }

    /// Stable machine token for JSON consumers.
    pub fn as_str(self) -> &'static str {
        match self {
            OpKind::Index => "index",
            OpKind::Refresh => "refresh",
            OpKind::Enrich => "enrich",
            OpKind::Retention => "retention",
        }
    }
}

/// The most recently observed progress phase + counters for an operation.
///
/// Mirrors the pipeline's `ProgressEvent` (`repo-index::ProgressEvent`). `total == 0` means the
/// phase does not know its denominator yet (honest unknown, never rendered as a false 0/0).
#[derive(Debug, Clone, Default)]
struct PhaseSnapshot {
    phase: Option<String>,
    current: u64,
    total: u64,
}

/// One in-flight write operation. Shared as `Arc<ActiveOperation>` between the registry and the
/// stamping handler's [`ActivityGuard`]. Interior mutability on `phase` because the stamping
/// handler updates it from the progress callback while readers observe it concurrently.
#[derive(Debug)]
pub struct ActiveOperation {
    kind: OpKind,
    /// Human-facing repo identity (canonical repo path) — what the operator recognises.
    repo_display: String,
    /// Internal repo uid when known (index knows it at entry; None only if unresolved).
    repo_uid: Option<String>,
    /// Canonical DB path this op writes — the key E matches to reclassify a busy DB open.
    db_path: PathBuf,
    started_at: Instant,
    phase: Mutex<PhaseSnapshot>,
}

impl ActiveOperation {
    fn new(kind: OpKind, repo_display: String, repo_uid: Option<String>, db_path: PathBuf) -> Self {
        Self {
            kind,
            repo_display,
            repo_uid,
            db_path,
            started_at: Instant::now(),
            phase: Mutex::new(PhaseSnapshot::default()),
        }
    }

    /// Record the latest progress phase + counters (called from the pipeline progress callback).
    fn update(&self, phase: &str, current: u64, total: u64) {
        let mut p = self.phase.lock();
        p.phase = Some(phase.to_string());
        p.current = current;
        p.total = total;
    }

    /// A serialisable, point-in-time view for the visibility surfaces.
    fn view(&self) -> ActiveOperationView {
        let p = self.phase.lock();
        ActiveOperationView {
            kind: self.kind,
            repo_display: self.repo_display.clone(),
            repo_uid: self.repo_uid.clone(),
            db_path: self.db_path.clone(),
            phase: p.phase.clone(),
            current: p.current,
            total: p.total,
            started_secs_ago: self.started_at.elapsed().as_secs(),
        }
    }
}

/// A point-in-time snapshot of an [`ActiveOperation`], safe to serialise and hand to CLI renderers.
#[derive(Debug, Clone)]
pub struct ActiveOperationView {
    pub kind: OpKind,
    pub repo_display: String,
    pub repo_uid: Option<String>,
    pub db_path: PathBuf,
    pub phase: Option<String>,
    pub current: u64,
    pub total: u64,
    pub started_secs_ago: u64,
}

impl ActiveOperationView {
    /// Serialise to the JSON shape the `daemon_info` / `storage_health` replies carry. `phase` is
    /// `null` (unknown) until the first progress event; `total` 0 renders as unknown downstream.
    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "repo": self.repo_display,
            "repo_uid": self.repo_uid,
            "phase": self.phase,
            "current": self.current,
            "total": self.total,
            "started_secs_ago": self.started_secs_ago,
        })
    }
}

/// Daemon-global registry of in-flight write operations.
///
/// A `Vec` (not a map) because concurrent writers are serialised *per database* but different
/// databases can index concurrently (the accept loop is concurrent), so more than one op may be
/// live at once. Membership churns at operation granularity (seconds+), so linear scan is trivial.
#[derive(Debug, Default)]
pub struct ActivityRegistry {
    ops: Mutex<Vec<Arc<ActiveOperation>>>,
}

impl ActivityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new in-flight operation. The returned [`ActivityGuard`] removes it on drop, so
    /// the record is cleared on **every** handler exit path (success, error, or panic-unwind).
    pub fn begin(
        &self,
        kind: OpKind,
        repo_display: impl Into<String>,
        repo_uid: Option<String>,
        db_path: impl Into<PathBuf>,
    ) -> ActivityGuard<'_> {
        let op = Arc::new(ActiveOperation::new(
            kind,
            repo_display.into(),
            repo_uid,
            db_path.into(),
        ));
        self.ops.lock().push(Arc::clone(&op));
        ActivityGuard { registry: self, op }
    }

    /// A view of every in-flight operation (for the `daemon_info` status surface).
    pub fn snapshot(&self) -> Vec<ActiveOperationView> {
        self.ops.lock().iter().map(|o| o.view()).collect()
    }

    /// The active operation writing `db_path`, if any (E — reclassify a busy DB open as
    /// healthy-in-use). Matched on the canonical DB path the write handler stamped.
    pub fn active_for_db(&self, db_path: &Path) -> Option<ActiveOperationView> {
        self.ops
            .lock()
            .iter()
            .find(|o| o.db_path == db_path)
            .map(|o| o.view())
    }

    fn remove(&self, op: &Arc<ActiveOperation>) {
        // Identity removal by Arc pointer — never removes a same-repo op stamped by another
        // concurrent handler.
        self.ops.lock().retain(|o| !Arc::ptr_eq(o, op));
    }
}

/// RAII handle for an in-flight operation. Held by the stamping handler; drop deregisters.
pub struct ActivityGuard<'a> {
    registry: &'a ActivityRegistry,
    op: Arc<ActiveOperation>,
}

impl ActivityGuard<'_> {
    /// Tee a pipeline progress event into the activity record. Cheap (one small mutex).
    pub fn update(&self, phase: &str, current: u64, total: u64) {
        self.op.update(phase, current, total);
    }
}

impl Drop for ActivityGuard<'_> {
    fn drop(&mut self) {
        self.registry.remove(&self.op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_registers_and_guard_drop_deregisters() {
        let reg = ActivityRegistry::new();
        assert!(reg.snapshot().is_empty());
        {
            let _g = reg.begin(
                OpKind::Index,
                "/repos/big",
                Some("uid-1".into()),
                "/db/big.db",
            );
            let snap = reg.snapshot();
            assert_eq!(snap.len(), 1);
            assert_eq!(snap[0].kind, OpKind::Index);
            assert_eq!(snap[0].repo_display, "/repos/big");
        }
        // Guard dropped → record cleared (the "completion stays observable / no leak" invariant).
        assert!(reg.snapshot().is_empty());
    }

    #[test]
    fn update_flows_phase_and_counters() {
        let reg = ActivityRegistry::new();
        let g = reg.begin(OpKind::Index, "/repos/big", None, "/db/big.db");
        g.update("extracting", 42_000, 160_000);
        let snap = reg.snapshot();
        assert_eq!(snap[0].phase.as_deref(), Some("extracting"));
        assert_eq!(snap[0].current, 42_000);
        assert_eq!(snap[0].total, 160_000);
    }

    #[test]
    fn active_for_db_matches_by_db_path() {
        let reg = ActivityRegistry::new();
        let _g = reg.begin(OpKind::Refresh, "/repos/a", None, "/db/a.db");
        assert!(reg.active_for_db(Path::new("/db/a.db")).is_some());
        assert!(reg.active_for_db(Path::new("/db/other.db")).is_none());
    }

    #[test]
    fn concurrent_ops_on_distinct_dbs_coexist() {
        let reg = ActivityRegistry::new();
        let _a = reg.begin(OpKind::Index, "/repos/a", None, "/db/a.db");
        let _b = reg.begin(OpKind::Index, "/repos/b", None, "/db/b.db");
        assert_eq!(reg.snapshot().len(), 2);
        // Dropping one leaves the other (identity removal, not repo-name removal).
        drop(_a);
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].repo_display, "/repos/b");
    }

    #[test]
    fn view_json_carries_reader_frame_fields() {
        let reg = ActivityRegistry::new();
        let g = reg.begin(
            OpKind::Index,
            "/repos/big",
            Some("uid-1".into()),
            "/db/big.db",
        );
        g.update("extracting", 10, 100);
        let v = reg.snapshot().pop().unwrap();
        let j = v.to_json();
        assert_eq!(j["kind"], "index");
        assert_eq!(j["repo"], "/repos/big");
        assert_eq!(j["phase"], "extracting");
        assert_eq!(j["current"], 10);
        assert_eq!(j["total"], 100);
    }

    #[test]
    fn gerund_is_reader_frame() {
        assert_eq!(OpKind::Index.gerund(), "indexing");
        assert_eq!(OpKind::Refresh.gerund(), "refreshing");
        assert_eq!(OpKind::Enrich.gerund(), "enriching");
        assert_eq!(OpKind::Retention.gerund(), "reclaiming");
    }
}
