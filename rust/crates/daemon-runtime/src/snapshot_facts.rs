//! DAEMON-VISIBILITY-1 (contracts E + F): per-repo snapshot facts for `rmap doctor` and
//! `rmap repo info`.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** collect, for one repo, the reader-frame state + outcome of every snapshot (READY /
//!   in-progress / interrupted), the repo's on-disk DB size, and — crucially — whether the DB is
//!   currently held by a daemon write op (so a busy DB open is reported as *healthy in-use*, not an
//!   error).
//! - **Concrete current users:** `handlers::metrics::handle_storage_health` (doctor's storage probe)
//!   and `dispatch::handle_repo_info`. Two surfaces, identical facts.
//! - **Named axis of variation:** none imagined — it exists because two repo-scoped surfaces need
//!   the same non-trivial mapping (`status`+`completed_at`+live-activity → reader-frame state/outcome,
//!   plus the in-use short-circuit).
//! - **Rejected simpler alternative:** inline the enumeration + mapping in both handlers — rejected,
//!   it duplicates non-trivial logic across two files.
//!
//! ## Honesty notes (VISION)
//!
//! - **State/outcome are Layer-1 derived facts**, sourced only from `snapshot.status`,
//!   `completed_at`, and the live activity registry — no new instrumentation. A `building` snapshot
//!   with **no** active op writing its DB is an index that started and never finalized → reported as
//!   *interrupted*, not "in progress".
//! - **Size is the whole-repo DB file**, labelled as such. Per-snapshot bytes are NOT tracked
//!   (verified: the `snapshots` table has only `*_total` counts, no size column). We do not claim a
//!   per-snapshot byte figure we do not have; the DB size is the honest "storage this repo holds".
//! - **"In use by daemon"** replaces a bare open failure during an index: the daemon KNOWS it is
//!   writing this DB (its own activity registry), so lock contention is normal operation, not a
//!   health failure (contract E).

use std::path::Path;

use repo_graph_storage::connection::StorageConnection;
use repo_graph_storage::types::Snapshot;
use serde_json::{json, Value};

use crate::activity::ActiveOperationView;
use crate::state::DaemonState;

/// Humanise a byte count for reader-facing messages (mirrors the CLI `format_size` scale). Kept
/// daemon-side because the honest next-action strings (below) are computed here and printed verbatim.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// DAEMON-VISIBILITY-1 (F2): the honest "no READY snapshot" message for orient/explain (and any
/// READY-requiring surface). Never a bare "index the repo first" when a partial snapshot exists.
///
/// If a non-READY snapshot exists, NAME it (state, when, the repo's on-disk size) and give BOTH
/// next actions (re-index / where the interrupted snapshot is reclaimable). Only when the repo was
/// genuinely never indexed does it fall back to the plain "index the repo first".
pub fn no_ready_snapshot_message(
    storage: &StorageConnection,
    db_path: &Path,
    repo_uid: &str,
) -> String {
    let latest = storage
        .get_latest_snapshot_any_state(repo_uid)
        .ok()
        .flatten();
    let db_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    partial_snapshot_message(latest.as_ref(), db_size)
}

/// Pure formatter for the "no READY snapshot" message (unit-testable without a DB).
///
/// `latest` is the newest snapshot of ANY state (or `None` if the repo was never indexed).
pub fn partial_snapshot_message(latest: Option<&Snapshot>, db_size_bytes: u64) -> String {
    match latest {
        Some(snap) if snap.status != "ready" => {
            let state = snapshot_state_label(&snap.status, false);
            let size = format_bytes(db_size_bytes);
            format!(
                "no READY snapshot for this repo, but a snapshot from {created} exists that was not \
                 completed (state: {state}; this repo holds {size} on disk). The last index did not \
                 finalize. Re-run `rmap index` to build a fresh snapshot; the interrupted snapshot is \
                 listed by `rmap maintenance prune`.",
                created = snap.created_at,
            )
        }
        // Genuinely never indexed (no snapshot at all), or a race left only a READY row.
        _ => "no snapshot for this repo yet. Index it first with `rmap index`.".to_string(),
    }
}

/// Reader-frame state label for a snapshot, given whether its repo is being written right now.
///
/// `is_active` is true only when the activity registry reports a live write op on this repo's DB —
/// which is the difference between "in progress" and "interrupted" for a `building` row.
pub fn snapshot_state_label(status: &str, is_active: bool) -> &'static str {
    match status {
        "ready" => "ready",
        "building" if is_active => "in progress",
        // A `building` snapshot with no live writer never finalized (daemon restart / machine sleep
        // mid-index) — the day-2 "4 GB non-READY" case.
        "building" => "interrupted",
        // Abort checkpoints write FAILED (indexer orchestrator) — an interrupted/aborted index.
        "failed" => "interrupted",
        "stale" => "superseded",
        other => other_leaked(other),
    }
}

/// Fallback for an unrecognised status string. The `status` column is unconstrained TEXT, so a
/// future writer could add a value; we surface it verbatim rather than guess (honest unknown).
fn other_leaked(_status: &str) -> &'static str {
    "unknown"
}

/// Reader-frame outcome sentence for a snapshot (the "last index outcome" fact).
pub fn snapshot_outcome(snap: &Snapshot, is_active: bool) -> String {
    match snap.status.as_str() {
        "ready" => match &snap.completed_at {
            Some(ts) => format!("completed {ts}"),
            None => "completed".to_string(),
        },
        "building" if is_active => "in progress (indexing now)".to_string(),
        "building" => "interrupted before completion (index did not finalize)".to_string(),
        "failed" => match &snap.completed_at {
            Some(ts) => format!("interrupted (index failed or was aborted at {ts})"),
            None => "interrupted (index failed or was aborted)".to_string(),
        },
        "stale" => "superseded by a newer snapshot".to_string(),
        other => format!("unknown state: {other}"),
    }
}

/// True if this snapshot's status is not a completed-and-usable READY.
pub fn is_non_ready(snap: &Snapshot) -> bool {
    snap.status != "ready"
}

/// Per-snapshot fact object (short uid + state + outcome + magnitude counts).
fn snapshot_to_json(snap: &Snapshot, is_active: bool) -> Value {
    json!({
        "snapshot_uid": snap.snapshot_uid,
        "status": snap.status,
        "state": snapshot_state_label(&snap.status, is_active),
        "outcome": snapshot_outcome(snap, is_active),
        "created_at": snap.created_at,
        "completed_at": snap.completed_at,
        // Magnitude of the snapshot (per-snapshot BYTES are not tracked; these counts are the honest
        // proxy for "how much was extracted").
        "files_total": snap.files_total,
        "nodes_total": snap.nodes_total,
        "edges_total": snap.edges_total,
    })
}

/// Map a snapshot list to the aggregate facts block (PURE — no I/O, unit-testable).
///
/// Called only when the repo is NOT being actively written (the caller short-circuits the active
/// case), so no row here is "in progress"; any `building`/`failed` row is interrupted.
pub fn map_snapshots(snapshots: &[Snapshot], db_size_bytes: u64) -> Value {
    let per_snapshot: Vec<Value> = snapshots
        .iter()
        .map(|s| snapshot_to_json(s, false))
        .collect();

    let ready_count = snapshots.iter().filter(|s| s.status == "ready").count();
    let interrupted: Vec<Value> = snapshots
        .iter()
        .filter(|s| is_non_ready(s))
        .map(|s| snapshot_to_json(s, false))
        .collect();

    json!({
        "db_size_bytes": db_size_bytes,
        "in_use_by_daemon": false,
        "total_snapshots": snapshots.len(),
        "ready_snapshots": ready_count,
        "interrupted_snapshots": interrupted,
        "snapshots": per_snapshot,
    })
}

/// The "in use by daemon" facts block (contract E) — the DB is held by a live write op, so we do
/// NOT attempt to read the (locked) snapshot table; we report the op instead. `db_size_bytes` is a
/// filesystem stat, which never blocks on the DB lock.
pub fn in_use_facts(db_size_bytes: u64, op: &ActiveOperationView) -> Value {
    json!({
        "db_size_bytes": db_size_bytes,
        "in_use_by_daemon": true,
        "operation": op.to_json(),
        // Snapshot detail is deliberately null (UNKNOWN, not zero): the table cannot be read while
        // the daemon writes it. It becomes available once the op completes.
        "snapshots": Value::Null,
    })
}

/// Collect snapshot facts for one repo, handling the in-use short-circuit (E) and the snapshot
/// enumeration (F). Daemon-coupled (opens storage under a read guard when idle).
///
/// `db_path` / `repo_uid` come from the resolved registry entry. Returns a JSON facts block for the
/// caller to merge into its own reply.
pub fn collect_snapshot_facts(state: &DaemonState, db_path: &Path, repo_uid: &str) -> Value {
    // `db_size_bytes` is filesystem metadata on the DB file — always readable, even mid-index.
    let db_size_bytes = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    // Contract E: if the daemon is writing this DB right now, report healthy-in-use and do NOT
    // attempt the (would-be-busy) open. This is the daemon's own authoritative knowledge.
    if let Some(op) = state.activity().active_for_db(db_path) {
        return in_use_facts(db_size_bytes, &op);
    }

    // Idle path: open the DB read-only and enumerate every snapshot's state.
    let repo_state = match state.load_repo(db_path, repo_uid) {
        Ok(rs) => rs,
        Err(e) => return facts_read_error(db_size_bytes, &e),
    };
    let _read_guard = repo_state.coordinator.acquire_read();
    let storage = match repo_state.storage() {
        Ok(s) => s,
        Err(e) => {
            // Fallback reclassification (contract E): a race where an op started between the check
            // above and here would surface as a busy open. Re-check the activity registry and, if a
            // write op is now live, report in-use rather than a health failure.
            if let Some(op) = state.activity().active_for_db(db_path) {
                return in_use_facts(db_size_bytes, &op);
            }
            return facts_read_error(db_size_bytes, &e);
        }
    };

    let snaps = match storage.list_snapshots(repo_uid) {
        Ok(s) => s,
        Err(e) => return facts_read_error(db_size_bytes, &format!("{e}")),
    };
    let mut facts = map_snapshots(&snaps, db_size_bytes);
    // Preserve `prunable_snapshots` (doctor's storage line reads it). Retention class is not
    // derivable from `status`, so read it here on the already-open connection; a read failure
    // degrades to `null` (UNKNOWN), never a false 0.
    let prunable = storage
        .get_retention_stats(repo_uid)
        .ok()
        .map(|s| s.prunable);
    facts["prunable_snapshots"] = json!(prunable);
    facts
}

/// A genuine (non-contention) read failure: DB absent/corrupt. `snapshots: null` (UNKNOWN) and an
/// explicit reason — distinct from the in-use case (`in_use_by_daemon: true`).
fn facts_read_error(db_size_bytes: u64, reason: &str) -> Value {
    json!({
        "db_size_bytes": db_size_bytes,
        "in_use_by_daemon": false,
        "snapshots": Value::Null,
        "read_error": reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(status: &str, completed: Option<&str>) -> Snapshot {
        Snapshot {
            snapshot_uid: format!("uid/{status}"),
            repo_uid: "repo-1".to_string(),
            parent_snapshot_uid: None,
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            dirty_hash: None,
            status: status.to_string(),
            files_total: 160_000,
            nodes_total: 5,
            edges_total: 7,
            created_at: "2026-07-02T10:00:00Z".to_string(),
            completed_at: completed.map(|s| s.to_string()),
            label: None,
            toolchain_json: None,
        }
    }

    #[test]
    fn ready_snapshot_is_completed() {
        assert_eq!(snapshot_state_label("ready", false), "ready");
        let s = snap("ready", Some("2026-07-02T10:05:00Z"));
        assert_eq!(
            snapshot_outcome(&s, false),
            "completed 2026-07-02T10:05:00Z"
        );
        assert!(!is_non_ready(&s));
    }

    #[test]
    fn building_without_active_op_is_interrupted() {
        // The day-2 "4 GB non-READY" case: building, no live writer.
        assert_eq!(snapshot_state_label("building", false), "interrupted");
        let s = snap("building", None);
        assert_eq!(
            snapshot_outcome(&s, false),
            "interrupted before completion (index did not finalize)"
        );
        assert!(is_non_ready(&s));
    }

    #[test]
    fn building_with_active_op_is_in_progress() {
        assert_eq!(snapshot_state_label("building", true), "in progress");
        let s = snap("building", None);
        assert_eq!(snapshot_outcome(&s, true), "in progress (indexing now)");
    }

    #[test]
    fn failed_is_interrupted() {
        assert_eq!(snapshot_state_label("failed", false), "interrupted");
        let s = snap("failed", Some("2026-07-02T10:03:00Z"));
        assert!(snapshot_outcome(&s, false).contains("interrupted"));
    }

    // DAEMON-VISIBILITY-1 (F2): the orient/explain "no READY snapshot" message NAMES the partial
    // (state + when + size) and gives BOTH next actions — never a bare "index the repo first".
    #[test]
    fn partial_message_names_the_interrupted_snapshot_and_both_actions() {
        let s = snap("building", None);
        let msg = partial_snapshot_message(Some(&s), 4_000_000_000);
        assert!(msg.contains("interrupted"), "names the state: {msg}");
        assert!(
            msg.contains("2026-07-02"),
            "names when it was created: {msg}"
        );
        assert!(msg.contains("GB"), "names the size on disk: {msg}");
        assert!(
            msg.contains("rmap index"),
            "next action 1 (re-index): {msg}"
        );
        assert!(
            msg.contains("rmap maintenance prune"),
            "next action 2 (reclaim): {msg}"
        );
        assert!(
            !msg.contains("index the repo first"),
            "must NOT be the bare gaslighting message: {msg}"
        );
    }

    #[test]
    fn partial_message_falls_back_when_never_indexed() {
        // No snapshot at all → the plain "index it first" is correct (not gaslighting).
        let msg = partial_snapshot_message(None, 0);
        assert!(msg.contains("rmap index"));
        assert!(msg.contains("no snapshot"));
    }

    #[test]
    fn format_bytes_scales() {
        assert_eq!(format_bytes(4_000_000_000), "3.7 GB");
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn map_snapshots_separates_interrupted_and_ready() {
        let snaps = vec![snap("building", None), snap("ready", Some("t"))];
        let facts = map_snapshots(&snaps, 4_000_000_000);
        assert_eq!(facts["db_size_bytes"], 4_000_000_000u64);
        assert_eq!(facts["total_snapshots"], 2);
        assert_eq!(facts["ready_snapshots"], 1);
        assert_eq!(facts["in_use_by_daemon"], false);
        assert_eq!(
            facts["interrupted_snapshots"].as_array().unwrap().len(),
            1,
            "the building snapshot is surfaced as interrupted"
        );
    }
}
