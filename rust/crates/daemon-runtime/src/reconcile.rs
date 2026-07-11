//! DAEMON-CRASH-RECOVERY-1 (F7/F11): reconcile crash-orphaned snapshots at boot AND on repo load.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** the guarded operation that finds a repo's crash-orphaned `building` snapshots (an
//!   index that started and never finalized — the daemon died, or the machine slept) and marks each
//!   `interrupted` (`failed` + `prunable`, via [`StorageConnection::mark_snapshot_interrupted`]),
//!   logging each through [`crate::oplog`]. Split from `state.rs`, which is already past the 500-line
//!   guardrail.
//! - **Concrete current users:** [`crate::state::DaemonState::load_repo`] (first-load hook) and the
//!   boot-sweep thread in `run_daemon` (via [`reconcile_all_repos`]). Two callers.
//! - **Named axis of variation:** one repo vs every registered repo; the per-repo mechanism is one.
//! - **Rejected simpler alternative:** inline the flip loop in `load_repo` — the boot sweep would then
//!   duplicate it, and `state.rs` (>500 lines) would grow a new responsibility. Rejected.
//!
//! ## The interrupted-detection needs no surviving-daemon evidence (VISION honesty)
//!
//! The detection is pure current-state: a `building` snapshot with **no live op on its DB** never
//! finalized. At boot the activity registry is empty *by construction* (it is in-memory, rebuilt each
//! start), so every `building` row is orphaned. On a later load the registry correctly reflects live
//! ops, so a genuinely-in-flight index is never mistaken for an orphan. No flag only a surviving
//! daemon could have written is required — exactly the field failure mode this slice closes.
//!
//! ## Safety — the two-gate rule (the shipped, operator-ratified discipline)
//!
//! Reconciliation writes, so it reuses verbatim the gate pair `try_retention_attempt` /
//! `reclaim_orphaned_non_ready` already use: (1) the activity registry is clear for this DB, and
//! (2) the non-blocking DB write lock is free. Both are non-blocking, so reconciliation NEVER stalls a
//! reader and NEVER touches a DB a live op owns; the storage-side `status='building'` guard is the
//! final backstop against a snapshot that finalized in the race window.

use std::path::{Path, PathBuf};

use repo_graph_storage::connection::StorageConnection;

use crate::state::DaemonState;

/// The single interruption class this module detects: an orphaned `building` snapshot left by a
/// daemon that died (or a machine that slept) mid-index. Recorded BOTH durably (merged into the
/// snapshot's extraction-diagnostics blob by [`StorageConnection::mark_snapshot_interrupted`], per the
/// operator resolution's Option B) AND in the F8 daemon log below, so the reason survives log rotation
/// and renders on doctor / repo-info / orient.
const DAEMON_RESTART_REASON: &str = "daemon restart";

/// The op that most likely created a snapshot, inferred from its `kind`, for the forensic
/// reconciliation log line only (not a persisted claim). `Refresh` snapshots came from `rmap
/// refresh`; `Full`/`Working`/`Sealed` from `rmap index`.
fn op_label_for_kind(kind: &str) -> &'static str {
    match kind {
        "refresh" => "refresh",
        _ => "index",
    }
}

/// Reconcile ONE repo's crash-orphaned snapshots under the two-gate rule; returns the snapshot UIDs
/// it flipped to interrupted (empty in the common no-orphan case AND when it yields to a live op).
///
/// `repo_display` is the reader-facing repo label for the log line (alias, else `repo_uid`).
pub fn reconcile_repo(
    state: &DaemonState,
    db_path: &Path,
    repo_uid: &str,
    repo_display: &str,
) -> Vec<String> {
    // Gate 1 — never touch a DB a live op (index/refresh/enrich/retention) is writing. At boot this is
    // always clear; on load it catches an in-flight op on this repo.
    if state.activity().active_for_db(db_path).is_some() {
        return Vec::new();
    }
    // Gate 2 — take the DB write lock non-blockingly (excludes an initial index that coordinates on
    // this lock, not the RepoCoordinator). Held for the whole flip, so an index that would start
    // mid-reconcile waits it out. A contended lock or an open failure simply yields (empty) — the next
    // load / auto-pass retries; reconciliation never blocks a reader or fails a request.
    let db_runtime = match state.get_or_create_db_runtime(db_path) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let _db_guard = match db_runtime.try_acquire_write() {
        Some(g) => g,
        None => return Vec::new(),
    };
    // `open` runs migrations, so an older DB is fully migrated before the flip UPDATE touches it.
    let storage = match StorageConnection::open(db_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    reconcile_with_storage(&storage, repo_uid, repo_display)
}

/// The DB-only inner reconcile (NO gates) — flip every `building` snapshot to interrupted and log it;
/// returns the flipped snapshot UIDs.
///
/// Exposed so the named reconciliation test drives exactly the crash state (a seeded DB) directly,
/// without standing up a full daemon: at boot the gates are always open by construction, so this IS
/// the boot behavior. Callers that can race a live op MUST come through [`reconcile_repo`] (gated).
pub fn reconcile_with_storage(
    storage: &StorageConnection,
    repo_uid: &str,
    repo_display: &str,
) -> Vec<String> {
    let building: Vec<_> = storage
        .list_snapshots(repo_uid)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.status == "building")
        .collect();

    let mut reconciled = Vec::new();
    for snap in &building {
        // `mark_snapshot_interrupted` is a no-op (returns false) if the snapshot finalized in the race
        // window — we then leave it alone. A write error likewise leaves it for the next pass / prune.
        // On success it ALSO records `DAEMON_RESTART_REASON` durably in the snapshot's diagnostics blob
        // (Option B); the F8 log line below is the parallel forensic trail.
        if let Ok(true) =
            storage.mark_snapshot_interrupted(&snap.snapshot_uid, DAEMON_RESTART_REASON)
        {
            crate::oplog::log_op_outcome(
                op_label_for_kind(&snap.kind),
                repo_display,
                Some(&snap.snapshot_uid),
                &format!("interrupted ({DAEMON_RESTART_REASON})"),
            );
            reconciled.push(snap.snapshot_uid.clone());
        }
    }
    reconciled
}

/// Boot sweep: reconcile EVERY registered repo. Spawned on a background thread by `run_daemon` so it
/// never delays socket readiness. Each repo is independently gated (a busy one is skipped — its live
/// op finalizes its own snapshot). Cheap: one indexed query + at most a few updates per repo, no blob
/// scans.
pub fn reconcile_all_repos(state: &DaemonState) {
    // Snapshot the registry entries, then DROP the guard before any DB work (the registry Mutex is
    // held only briefly, never across the per-repo opens).
    let repos: Vec<(PathBuf, String, String)> = {
        let reg = state.registry();
        reg.list()
            .iter()
            .map(|e| {
                (
                    e.db_path.clone(),
                    e.repo_uid.clone(),
                    e.alias.clone().unwrap_or_else(|| e.repo_uid.clone()),
                )
            })
            .collect()
    };

    let mut total = 0usize;
    for (db_path, repo_uid, display) in &repos {
        total += reconcile_repo(state, db_path, repo_uid, display).len();
    }
    if total > 0 {
        eprintln!(
            "info: startup reconciliation marked {total} interrupted snapshot(s) (daemon restart) across {} registered repo(s)",
            repos.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_storage::types::{CreateSnapshotInput, Repo, UpdateSnapshotStatusInput};

    fn seed(repo_uid: &str) -> StorageConnection {
        let storage = StorageConnection::open_in_memory().expect("open in-memory storage");
        storage
            .add_repo(&Repo {
                repo_uid: repo_uid.to_string(),
                name: repo_uid.to_string(),
                root_path: ".".to_string(),
                default_branch: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .expect("add repo");
        storage
    }

    fn building_snapshot(storage: &StorageConnection, repo_uid: &str, kind: &str) -> String {
        storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo_uid.to_string(),
                kind: kind.to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create building snapshot")
            .snapshot_uid
    }

    // F7/F11 core: the crash state (building snapshots, empty activity by construction) → each is
    // flipped to interrupted (failed + prunable) and a reconciliation line is logged. A READY snapshot
    // is untouched.
    #[test]
    fn reconciles_orphaned_building_snapshots_and_logs_each() {
        crate::oplog::enable_oplog_capture_for_test();
        // Unique display so the process-global capture buffer is filtered to THIS test (parallel-safe).
        let repo = "recon-core-repo";
        let storage = seed(repo);
        let idx_uid = building_snapshot(&storage, repo, "full");
        let refresh_uid = building_snapshot(&storage, repo, "refresh");
        // A READY snapshot that must NOT be touched.
        let ready_uid = building_snapshot(&storage, repo, "full");
        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: ready_uid.clone(),
                status: "ready".to_string(),
                completed_at: None,
            })
            .unwrap();

        let reconciled = reconcile_with_storage(&storage, repo, repo);
        assert_eq!(reconciled.len(), 2, "both building orphans reconciled");

        // Both orphans are now the terminal interrupted state; the READY one is preserved.
        for uid in [&idx_uid, &refresh_uid] {
            let s = storage.get_snapshot(uid).unwrap().unwrap();
            assert_eq!(s.status, "failed", "orphan marked terminal");
            // Operator resolution (Option B): the reason is DURABLE in the diagnostics blob (not only
            // in the log), so it survives log rotation and renders on doctor/repo-info/orient.
            let diag = repo_graph_trust::TrustStorageRead::get_snapshot_extraction_diagnostics(
                &storage, uid,
            )
            .unwrap()
            .expect("interrupted diagnostics blob written");
            let v: serde_json::Value = serde_json::from_str(&diag).unwrap();
            assert_eq!(
                v["interrupted"]["reason"], "daemon restart",
                "durable reason recorded: {diag}"
            );
        }
        assert_eq!(
            storage.get_snapshot(&ready_uid).unwrap().unwrap().status,
            "ready",
            "the READY snapshot is untouched"
        );

        // review-1 (the blocking acceptance criterion): BOTH reconciled orphans are CLASSIFIED
        // prunable and the retention STAT counts them BEFORE any reclaim — so classification, doctor,
        // and `maintenance prune` all name them ("retention classifies them prunable"). The field bug
        // was `total 3, all classes 0`; here the two orphans move OUT of unclassified INTO `prunable`.
        // (The seeded READY snapshot is never run through `classify_repo_retention` in this direct-
        // storage test, so it stays unclassified — a test-seeding artifact, not an orphan. The two
        // orphans are what the reviewer's assertion is about, and `prunable == 2` proves it.)
        let stats = storage.get_retention_stats(repo).unwrap();
        assert_eq!(
            stats.prunable, 2,
            "both reconciled orphans are counted prunable pre-reclaim: {stats:?}"
        );

        // The reconciled orphans are NON-READY, so the F3 reclaim (the VACUUM path) collects exactly
        // them; the READY snapshot survives.
        let mut reclaimed = storage.prune_non_ready_snapshots(repo).unwrap();
        reclaimed.sort();
        let mut expected = vec![idx_uid.clone(), refresh_uid.clone()];
        expected.sort();
        assert_eq!(
            reclaimed, expected,
            "both orphans reclaimed, READY survives"
        );
        assert!(
            storage.get_snapshot(&ready_uid).unwrap().is_some(),
            "the READY snapshot is not reclaimed"
        );

        // The forensic reconciliation lines are in the LOG, keyed on op + snapshot + reason.
        let lines: Vec<String> = crate::oplog::oplog_lines_for_test()
            .into_iter()
            .filter(|l| l.contains(repo))
            .collect();
        assert!(
            lines.iter().any(
                |l| l.contains("op index interrupted (daemon restart)") && l.contains(&idx_uid)
            ),
            "index orphan logged with reason: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("op refresh interrupted (daemon restart)")
                    && l.contains(&refresh_uid)),
            "refresh orphan logged with the refresh op label: {lines:?}"
        );
    }

    // Idempotence: a second sweep over an already-reconciled repo flips nothing (the guard sees only
    // terminal rows) — a running daemon re-loading a repo never re-marks or duplicates the log.
    #[test]
    fn second_reconcile_is_a_noop() {
        let repo = "recon-idem-repo";
        let storage = seed(repo);
        let _ = building_snapshot(&storage, repo, "full");
        assert_eq!(reconcile_with_storage(&storage, repo, repo).len(), 1);
        assert_eq!(
            reconcile_with_storage(&storage, repo, repo).len(),
            0,
            "nothing left to reconcile"
        );
    }
}
