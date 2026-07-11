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

use std::collections::BTreeMap;
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
    // DAEMON-CRASH-RECOVERY-1 (Option B): if the partial was reconciled (a crash orphan → daemon
    // restart), its DURABLE reason renders in the message — "interrupted — daemon restart, reconciled
    // <time>" — so the truth survives log rotation on the very surface the user hits.
    let reason = latest
        .as_ref()
        .and_then(|s| snapshot_interruption_reason(storage, &s.snapshot_uid));
    partial_snapshot_message(latest.as_ref(), db_size, reason.as_deref())
}

/// Pure formatter for the "no READY snapshot" message (unit-testable without a DB).
///
/// `latest` is the newest snapshot of ANY state (or `None` if the repo was never indexed). `reason`
/// is the durable interruption reason (Option B) when the partial was reconciled, else `None`.
pub fn partial_snapshot_message(
    latest: Option<&Snapshot>,
    db_size_bytes: u64,
    reason: Option<&str>,
) -> String {
    match latest {
        Some(snap) if snap.status != "ready" => {
            let state = snapshot_state_label(&snap.status, false);
            // A reconciled orphan names WHY: "interrupted — daemon restart, reconciled <time>".
            let state_frame = match reason {
                Some(r) => format!("{state} — {r}"),
                None => state.to_string(),
            };
            let size = format_bytes(db_size_bytes);
            format!(
                "no READY snapshot for this repo, but a snapshot from {created} exists that was not \
                 completed (state: {state_frame}; this repo holds {size} on disk). The last index did \
                 not finalize. Re-run `rmap index` to build a fresh snapshot; the interrupted snapshot \
                 is listed by `rmap maintenance prune`.",
                created = snap.created_at,
            )
        }
        // Genuinely never indexed (no snapshot at all), or a race left only a READY row.
        _ => "no snapshot for this repo yet. Index it first with `rmap index`.".to_string(),
    }
}

/// Read a snapshot's DURABLE interruption reason (DAEMON-CRASH-RECOVERY-1, Option B) from its
/// extraction-diagnostics blob, if any. Best-effort: a missing blob / read failure / a clean
/// snapshot all yield `None` (the common case). Daemon-coupled (needs the open connection); the
/// parse is the pure [`interruption_reason`].
fn snapshot_interruption_reason(storage: &StorageConnection, snapshot_uid: &str) -> Option<String> {
    let diag = repo_graph_trust::TrustStorageRead::get_snapshot_extraction_diagnostics(
        storage,
        snapshot_uid,
    )
    .ok()
    .flatten();
    interruption_reason(diag.as_deref())
}

/// Parse the durable interruption annotation (DAEMON-CRASH-RECOVERY-1, Option B) that reconciliation
/// merges into a snapshot's extraction-diagnostics blob (`{"interrupted": {"reason": …,
/// "reconciled_at": …}}`). Returns a reader-frame suffix — e.g. `"daemon restart, reconciled
/// 2026-07-02T…Z"` — or `None` when the snapshot was not reconciled (the overwhelmingly common
/// case). PURE and unit-testable; like [`extraction_degradations`] it reads extra keys the typed
/// `ExtractionDiagnostics` reader ignores, so it is the only place this key surfaces.
fn interruption_reason(diagnostics_json: Option<&str>) -> Option<String> {
    let value: Value = serde_json::from_str(diagnostics_json?).ok()?;
    let interrupted = value.get("interrupted")?.as_object()?;
    let reason = interrupted.get("reason")?.as_str()?;
    match interrupted.get("reconciled_at").and_then(|v| v.as_str()) {
        Some(at) => Some(format!("{reason}, reconciled {at}")),
        None => Some(reason.to_string()),
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
///
/// `reason` is the DURABLE interruption reason (DAEMON-CRASH-RECOVERY-1, Option B) when the snapshot
/// was reconciled (a crash orphan the boot sweep marked `failed`), else `None`. When present it names
/// WHY — "interrupted — daemon restart, reconciled <time>" — instead of the generic abort text, so the
/// truth survives log rotation on doctor / repo-info.
pub fn snapshot_outcome(snap: &Snapshot, is_active: bool, reason: Option<&str>) -> String {
    match snap.status.as_str() {
        "ready" => match &snap.completed_at {
            Some(ts) => format!("completed {ts}"),
            None => "completed".to_string(),
        },
        "building" if is_active => "in progress (indexing now)".to_string(),
        "building" => "interrupted before completion (index did not finalize)".to_string(),
        // A reconciled crash orphan carries its durable reason; a genuine index abort does not.
        "failed" => match reason {
            Some(r) => format!("interrupted — {r}"),
            None => match &snap.completed_at {
                Some(ts) => format!("interrupted (index failed or was aborted at {ts})"),
                None => "interrupted (index failed or was aborted)".to_string(),
            },
        },
        "stale" => "superseded by a newer snapshot".to_string(),
        other => format!("unknown state: {other}"),
    }
}

/// True if this snapshot's status is not a completed-and-usable READY.
pub fn is_non_ready(snap: &Snapshot) -> bool {
    snap.status != "ready"
}

/// Per-snapshot fact object (short uid + state + outcome + magnitude counts). `reason` is the
/// snapshot's durable interruption reason (Option B) when reconciled, threaded into the outcome.
fn snapshot_to_json(snap: &Snapshot, is_active: bool, reason: Option<&str>) -> Value {
    json!({
        "snapshot_uid": snap.snapshot_uid,
        "status": snap.status,
        "state": snapshot_state_label(&snap.status, is_active),
        "outcome": snapshot_outcome(snap, is_active, reason),
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
/// case), so no row here is "in progress"; any `building`/`failed` row is interrupted. `reasons` maps
/// a snapshot_uid to its durable interruption reason (Option B) — the caller reads the blobs (I/O),
/// keeping this mapping pure; a snapshot absent from the map renders its generic outcome.
pub fn map_snapshots(
    snapshots: &[Snapshot],
    db_size_bytes: u64,
    reasons: &BTreeMap<String, String>,
) -> Value {
    let reason_for = |s: &Snapshot| reasons.get(&s.snapshot_uid).map(|r| r.as_str());
    let per_snapshot: Vec<Value> = snapshots
        .iter()
        .map(|s| snapshot_to_json(s, false, reason_for(s)))
        .collect();

    let ready_count = snapshots.iter().filter(|s| s.status == "ready").count();
    let interrupted: Vec<Value> = snapshots
        .iter()
        .filter(|s| is_non_ready(s))
        .map(|s| snapshot_to_json(s, false, reason_for(s)))
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
    // DAEMON-CRASH-RECOVERY-1 (Option B): read the durable interruption reason for each NON-READY
    // snapshot (READY rows never carry it), so doctor / repo-info render "interrupted — daemon restart,
    // reconciled <time>" per snapshot. Bounded — retention keeps ≤2 snapshots plus any orphans — and on
    // the already-open connection; a clean/missing blob simply yields no reason.
    let mut reasons: BTreeMap<String, String> = BTreeMap::new();
    for s in snaps.iter().filter(|s| is_non_ready(s)) {
        if let Some(r) = snapshot_interruption_reason(&storage, &s.snapshot_uid) {
            reasons.insert(s.snapshot_uid.clone(), r);
        }
    }
    let mut facts = map_snapshots(&snaps, db_size_bytes, &reasons);
    // Preserve `prunable_snapshots` (doctor's storage line reads it). Retention class is not
    // derivable from `status`, so read it here on the already-open connection; a read failure
    // degrades to `null` (UNKNOWN), never a false 0.
    let prunable = storage
        .get_retention_stats(repo_uid)
        .ok()
        .map(|s| s.prunable);
    facts["prunable_snapshots"] = json!(prunable);

    // PERSIST-RECURSION-1: surface the CURRENT snapshot's honest extraction
    // degradations (deeply-nested files skipped, isolated postpass failures) from
    // its diagnostics blob. The latest snapshot is the most recent index attempt —
    // the same one doctor's other storage lines describe. Read-only + best-effort:
    // a missing blob or read failure simply shows no degradation line, never an error.
    if let Some(latest) = storage
        .get_latest_snapshot_any_state(repo_uid)
        .ok()
        .flatten()
    {
        let diag = repo_graph_trust::TrustStorageRead::get_snapshot_extraction_diagnostics(
            &storage,
            &latest.snapshot_uid,
        )
        .ok()
        .flatten();
        if let Some(degradations) = extraction_degradations(diag.as_deref()) {
            facts["extraction_degradations"] = degradations;
        }
    }
    facts
}

/// A read failure with `snapshots: null` (UNKNOWN) and an explicit reason — distinct from the in-use
/// case (`in_use_by_daemon: true`).
///
/// DAEMON-CRASH-RECOVERY-1 (F9): a `database is locked` failure here is NOT the daemon's own write
/// (the `in_use_by_daemon` short-circuit already ran and found no live op for this DB), so it is a
/// lock the daemon cannot attribute to itself — most likely another process holding the DB, or a
/// just-restarted daemon still opening it. That is a transient, reader-frame condition, NOT a
/// corrupt/absent DB, so it is tagged `locked_by_other` and rendered as a reader-frame note rather
/// than a raw FAIL (`storage_probe.rs`). Genuine failures (absent/corrupt) keep the plain reason.
fn facts_read_error(db_size_bytes: u64, reason: &str) -> Value {
    if is_lock_contention(reason) {
        return json!({
            "db_size_bytes": db_size_bytes,
            "in_use_by_daemon": false,
            "snapshots": Value::Null,
            "locked_by_other": true,
            "read_error": reason,
        });
    }
    json!({
        "db_size_bytes": db_size_bytes,
        "in_use_by_daemon": false,
        "snapshots": Value::Null,
        "read_error": reason,
    })
}

/// True if a storage-open/read error string is SQLite lock contention (`SQLITE_BUSY` →
/// "database is locked", `SQLITE_LOCKED` → "database table is locked"). Substring match on the
/// canonical SQLite text — the daemon does not have a typed error at this boundary (the reason is a
/// formatted string from several sources), and over-matching here only ever downgrades a genuine
/// failure to a softer reader-frame, never the reverse (a corrupt DB does not say "is locked").
pub fn is_lock_contention(reason: &str) -> bool {
    reason.contains("is locked")
}

/// PERSIST-RECURSION-1 (honest degradation surface): parse a snapshot's
/// extraction-diagnostics blob into the reader-facing degradations that doctor
/// and `rmap repo info` show — per-postpass counts of files skipped for
/// pathological AST nesting, and any isolated postpass failures.
///
/// Returns `None` when there is nothing to report (the overwhelmingly common
/// case), so callers only attach the field when a degradation actually happened.
/// PURE and unit-testable: the blob is the free-form JSON the indexer writes; the
/// typed `ExtractionDiagnostics` reader ignores these extra keys, so this is the
/// ONLY place they surface. The reader-language subject mapping lives here, in one
/// place, and the `lines` are printed verbatim by both consumers (matching this
/// module's "compute reader strings here" contract).
fn extraction_degradations(diagnostics_json: Option<&str>) -> Option<Value> {
    let value: Value = serde_json::from_str(diagnostics_json?).ok()?;
    let obj = value.as_object()?;

    // (postpass family, count) / (postpass family, message). Family is the key
    // with its role suffix stripped, e.g. `boundary_facts_files_skipped_deep_nesting`
    // and `boundary_facts_postpass_error` both map to family `boundary_facts`.
    let mut skips: Vec<(String, u64)> = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();
    for (key, val) in obj {
        if let Some(family) = key.strip_suffix("_files_skipped_deep_nesting") {
            // `> 0` only: 0 is "measured and absent", never a written key, but be
            // defensive so a stray 0 never produces a "skipped for 0 files" line.
            if let Some(n) = val.as_u64() {
                if n > 0 {
                    skips.push((family.to_string(), n));
                }
            }
        } else if let Some(family) = key.strip_suffix("_postpass_error") {
            if let Some(msg) = val.as_str() {
                errors.push((family.to_string(), msg.to_string()));
            }
        }
    }
    if skips.is_empty() && errors.is_empty() {
        return None;
    }
    // Deterministic (alphabetical by family) — no reader-facing order jitter.
    skips.sort();
    errors.sort();

    let mut lines: Vec<String> = Vec::new();
    let mut skips_obj = serde_json::Map::new();
    for (family, n) in &skips {
        let files = if *n == 1 { "file" } else { "files" };
        lines.push(format!(
            "{} skipped for {} {} (pathological nesting)",
            postpass_subject(family),
            n,
            files
        ));
        skips_obj.insert(family.clone(), json!(n));
    }
    let mut errors_obj = serde_json::Map::new();
    for (family, msg) in &errors {
        lines.push(format!(
            "{} not extracted for this snapshot — the postpass failed but the index completed ({})",
            postpass_subject(family),
            msg
        ));
        errors_obj.insert(family.clone(), json!(msg));
    }

    Some(json!({
        "deep_nesting_skips": Value::Object(skips_obj),
        "postpass_errors": Value::Object(errors_obj),
        "lines": lines,
    }))
}

/// Map a re-parse postpass family to the reader's own subject (VISION: labels
/// speak the reader's language, not our pipeline's). An unknown family (a future
/// postpass) degrades to a humanised form of the key rather than a raw identifier.
fn postpass_subject(family: &str) -> String {
    match family {
        "policy_facts" => "policy facts".to_string(),
        "boundary_facts" => "boundary facts".to_string(),
        "ts_boundary_facts" => "boundary facts (TypeScript/JavaScript)".to_string(),
        "express_surface_facts" => "HTTP route surfaces".to_string(),
        "react_inference_facts" => "React inferences".to_string(),
        other => other.replace('_', " "),
    }
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
            snapshot_outcome(&s, false, None),
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
            snapshot_outcome(&s, false, None),
            "interrupted before completion (index did not finalize)"
        );
        assert!(is_non_ready(&s));
    }

    #[test]
    fn building_with_active_op_is_in_progress() {
        assert_eq!(snapshot_state_label("building", true), "in progress");
        let s = snap("building", None);
        assert_eq!(
            snapshot_outcome(&s, true, None),
            "in progress (indexing now)"
        );
    }

    #[test]
    fn failed_is_interrupted() {
        assert_eq!(snapshot_state_label("failed", false), "interrupted");
        let s = snap("failed", Some("2026-07-02T10:03:00Z"));
        assert!(snapshot_outcome(&s, false, None).contains("interrupted"));
    }

    // DAEMON-CRASH-RECOVERY-1 (Option B): a reconciled crash orphan (terminal `failed`) renders its
    // DURABLE reason in the outcome — "interrupted — daemon restart, reconciled <time>" — instead of
    // the generic abort text. This is the doctor / repo-info per-snapshot render seam.
    #[test]
    fn failed_with_reason_names_the_daemon_restart() {
        let s = snap("failed", None);
        let reason = "daemon restart, reconciled 2026-07-02T11:00:00Z";
        assert_eq!(
            snapshot_outcome(&s, false, Some(reason)),
            "interrupted — daemon restart, reconciled 2026-07-02T11:00:00Z"
        );
    }

    // The pure parser reads back exactly what reconciliation merges into the diagnostics blob, and
    // returns None for a clean/absent/typed-only blob (the common case — no false "interrupted" note).
    #[test]
    fn interruption_reason_parses_the_durable_annotation() {
        assert!(interruption_reason(None).is_none(), "no blob → None");
        assert!(
            interruption_reason(Some(
                r#"{"diagnostics_version":1,"edges_total":100,"unresolved_total":2}"#
            ))
            .is_none(),
            "a typed-only diagnostics blob is not an interruption"
        );
        let blob = r#"{"edges_total":9,"interrupted":{"reason":"daemon restart","reconciled_at":"2026-07-02T11:00:00Z"}}"#;
        assert_eq!(
            interruption_reason(Some(blob)).as_deref(),
            Some("daemon restart, reconciled 2026-07-02T11:00:00Z"),
            "reason + reconciled time, coexisting with other diagnostics keys"
        );
        // A reason without a timestamp degrades to just the reason (defensive; reconciliation always
        // writes both).
        let no_ts = r#"{"interrupted":{"reason":"daemon restart"}}"#;
        assert_eq!(
            interruption_reason(Some(no_ts)).as_deref(),
            Some("daemon restart")
        );
    }

    // DAEMON-VISIBILITY-1 (F2): the orient/explain "no READY snapshot" message NAMES the partial
    // (state + when + size) and gives BOTH next actions — never a bare "index the repo first".
    #[test]
    fn partial_message_names_the_interrupted_snapshot_and_both_actions() {
        let s = snap("building", None);
        let msg = partial_snapshot_message(Some(&s), 4_000_000_000, None);
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
        let msg = partial_snapshot_message(None, 0, None);
        assert!(msg.contains("rmap index"));
        assert!(msg.contains("no snapshot"));
    }

    // DAEMON-CRASH-RECOVERY-1 (Option B): the F2 orient/explain message — the surface the user hits —
    // names the DURABLE reason of a reconciled orphan, so the truth survives log rotation right where
    // the user reads it: "state: interrupted — daemon restart, reconciled <time>".
    #[test]
    fn partial_message_names_the_durable_reconciliation_reason() {
        let s = snap("failed", None); // a reconciled crash orphan (boot sweep flipped it)
        let reason = "daemon restart, reconciled 2026-07-02T11:00:00Z";
        let msg = partial_snapshot_message(Some(&s), 4_000_000_000, Some(reason));
        assert!(
            msg.contains("interrupted — daemon restart, reconciled 2026-07-02T11:00:00Z"),
            "the F2 message names the durable reason: {msg}"
        );
        assert!(msg.contains("rmap index") && msg.contains("rmap maintenance prune"));
    }

    #[test]
    fn format_bytes_scales() {
        assert_eq!(format_bytes(4_000_000_000), "3.7 GB");
        assert_eq!(format_bytes(0), "0 B");
    }

    // DAEMON-CRASH-RECOVERY-1 (F9): a lock the daemon cannot attribute to itself is tagged
    // `locked_by_other` (transient reader-frame), while a genuine corrupt/absent DB is a plain read
    // error (a real FAIL downstream).
    #[test]
    fn lock_contention_is_tagged_but_corruption_is_not() {
        assert!(is_lock_contention("database is locked"));
        assert!(is_lock_contention(
            "Sqlite(SqliteFailure(.. database table is locked ..))"
        ));
        assert!(!is_lock_contention("database disk image is malformed"));

        let locked = facts_read_error(4_000_000_000, "database is locked");
        assert_eq!(locked["locked_by_other"], true, "lock is flagged: {locked}");

        let corrupt = facts_read_error(100, "database disk image is malformed");
        assert!(
            corrupt.get("locked_by_other").is_none(),
            "a genuine corruption is NOT a lock race: {corrupt}"
        );
        assert_eq!(corrupt["read_error"], "database disk image is malformed");
    }

    #[test]
    fn map_snapshots_separates_interrupted_and_ready() {
        let snaps = vec![snap("building", None), snap("ready", Some("t"))];
        let facts = map_snapshots(&snaps, 4_000_000_000, &BTreeMap::new());
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

    // DAEMON-CRASH-RECOVERY-1 (Option B): when a reconciled orphan's durable reason is supplied, the
    // per-snapshot `outcome` (rendered verbatim by doctor + repo-info) carries it.
    #[test]
    fn map_snapshots_renders_the_durable_reason_in_the_outcome() {
        let orphan = snap("failed", None);
        let mut reasons = BTreeMap::new();
        reasons.insert(
            orphan.snapshot_uid.clone(),
            "daemon restart, reconciled 2026-07-02T11:00:00Z".to_string(),
        );
        let facts = map_snapshots(&[orphan], 4_000_000_000, &reasons);
        let outcome = facts["interrupted_snapshots"][0]["outcome"]
            .as_str()
            .unwrap();
        assert_eq!(
            outcome, "interrupted — daemon restart, reconciled 2026-07-02T11:00:00Z",
            "the reconciled orphan's outcome names the durable reason"
        );
    }

    // PERSIST-RECURSION-1: a clean snapshot (no degradation keys) reports nothing —
    // callers must not attach an empty `extraction_degradations` field.
    #[test]
    fn extraction_degradations_none_when_clean() {
        assert!(extraction_degradations(None).is_none(), "no blob → None");
        let clean = r#"{"diagnostics_version":1,"edges_total":100,"unresolved_total":2,"unresolved_breakdown":{"other":2}}"#;
        assert!(
            extraction_degradations(Some(clean)).is_none(),
            "a blob with only typed diagnostics reports no degradation"
        );
        // A defensive zero must not produce a "skipped for 0 files" line.
        let zero = r#"{"boundary_facts_files_skipped_deep_nesting":0}"#;
        assert!(
            extraction_degradations(Some(zero)).is_none(),
            "0 is measured-and-absent, not a degradation"
        );
    }

    // The reader-frame line matches the slice's example wording exactly, and speaks
    // the reader's language (their file's nesting), not our pipeline's key.
    #[test]
    fn extraction_degradations_reports_skips_in_reader_frame() {
        let blob = r#"{"boundary_facts_files_skipped_deep_nesting":1}"#;
        let d = extraction_degradations(Some(blob)).expect("a skip is a degradation");
        let lines: Vec<&str> = d["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            lines,
            vec!["boundary facts skipped for 1 file (pathological nesting)"],
            "reader-frame line matches the slice example"
        );
        // The structured count is preserved for machine consumers (`--json`).
        assert_eq!(d["deep_nesting_skips"]["boundary_facts"].as_u64(), Some(1));

        // Plural + the express/react/ts families render their reader subjects.
        let blob = r#"{"react_inference_facts_files_skipped_deep_nesting":3,"ts_boundary_facts_files_skipped_deep_nesting":2}"#;
        let d = extraction_degradations(Some(blob)).unwrap();
        let joined = d["lines"].to_string();
        assert!(
            joined.contains("React inferences skipped for 3 files (pathological nesting)"),
            "{joined}"
        );
        assert!(
            joined.contains(
                "boundary facts (TypeScript/JavaScript) skipped for 2 files (pathological nesting)"
            ),
            "{joined}"
        );
    }

    // An isolated postpass failure (item 3) is surfaced honestly too: the index
    // completed, but those facts are missing — the reader is told, not left guessing.
    #[test]
    fn extraction_degradations_reports_isolated_postpass_errors() {
        let blob = r#"{"policy_facts_postpass_error":"index: simulated failure"}"#;
        let d = extraction_degradations(Some(blob)).unwrap();
        let joined = d["lines"].to_string();
        assert!(
            joined.contains("policy facts not extracted for this snapshot"),
            "{joined}"
        );
        assert!(joined.contains("the index completed"), "{joined}");
        assert!(joined.contains("index: simulated failure"), "{joined}");
        assert_eq!(
            d["postpass_errors"]["policy_facts"].as_str(),
            Some("index: simulated failure")
        );
    }

    // Multiple degradations render in a deterministic (alphabetical-by-family) order —
    // no reader-facing jitter across runs.
    #[test]
    fn extraction_degradations_deterministic_order() {
        let blob = r#"{"react_inference_facts_files_skipped_deep_nesting":1,"boundary_facts_files_skipped_deep_nesting":1,"policy_facts_files_skipped_deep_nesting":1}"#;
        let a = extraction_degradations(Some(blob)).unwrap();
        let b = extraction_degradations(Some(blob)).unwrap();
        assert_eq!(a, b, "same blob → identical output");
        let lines: Vec<String> = a["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        // boundary_facts < policy_facts < react_inference_facts alphabetically.
        assert!(lines[0].starts_with("boundary facts skipped"), "{lines:?}");
        assert!(lines[1].starts_with("policy facts skipped"), "{lines:?}");
        assert!(
            lines[2].starts_with("React inferences skipped"),
            "{lines:?}"
        );
    }
}
