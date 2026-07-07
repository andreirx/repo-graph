//! `storage_health`-derived doctor storage probe (DAEMON-VISIBILITY-1 E + F).
//!
//! **Abstraction note (per repo structural guardrail):** extracted from `doctor/mod.rs` because the
//! snapshot-facts parse/format pushed that file past the 500-line guardrail (the same reason
//! `daemon_info.rs` was split). One concrete current caller: [`super::storage_summary_probe`], via
//! [`storage_probe_from_facts`]. Axis of variation: none claimed — a cohesion/size split.
//! `format_size` is shared from the parent via `super::`.

use crate::platform::ProbeResult;

use super::format_size;

/// Build the `storage` probe from the `storage_health` facts (DAEMON-VISIBILITY-1 E + F).
///
/// Pure (no I/O). Three shapes:
/// - **in use by daemon** (contract E): the DB is held by a live write op → healthy
///   "in use by daemon — indexing <repo>", NEVER the old "error opening database".
/// - **idle with snapshots** (contract F): per-snapshot state/outcome + size; interrupted
///   (non-READY) snapshots named in the details, plus the enrichment next-action.
/// - **read error**: DB absent/corrupt → a genuine FAIL with the reason.
pub(super) fn storage_probe_from_facts(response: &serde_json::Value) -> ProbeResult {
    let db_size = response
        .get("db_size_bytes")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let size_human = format_size(db_size);

    // Contract E: the daemon is writing this DB right now — healthy in-use, not an error.
    if response
        .get("in_use_by_daemon")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let op = response.get("operation");
        let verb = match op.and_then(|o| o.get("kind")).and_then(|v| v.as_str()) {
            Some("index") => "indexing",
            Some("refresh") => "refreshing",
            Some("enrich") => "enriching",
            _ => "in use by",
        };
        let repo = op
            .and_then(|o| o.get("repo"))
            .and_then(|v| v.as_str())
            .unwrap_or("this repo");
        return ProbeResult {
            name: "storage".to_string(),
            passed: true,
            message: format!("db: {size_human}, in use by daemon — {verb} {repo}"),
            details: Some("snapshot detail is unavailable while the daemon writes the database; re-run doctor after it completes".to_string()),
        };
    }

    // Read error (DB absent/corrupt): a genuine health failure (contract E's "error" case).
    if let Some(reason) = response.get("read_error").and_then(|v| v.as_str()) {
        return ProbeResult {
            name: "storage".to_string(),
            passed: false,
            message: format!("db: {size_human}, cannot read snapshots"),
            details: Some(reason.to_string()),
        };
    }

    // Idle: report per-snapshot state (contract F). Counts + interrupted detail + enrichment line.
    let total = response
        .get("total_snapshots")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let ready = response
        .get("ready_snapshots")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let prunable = response
        .get("prunable_snapshots")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let interrupted: Vec<&serde_json::Value> = response
        .get("interrupted_snapshots")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();

    let snap_word = if total == 1 { "snapshot" } else { "snapshots" };
    let mut message = format!("db: {size_human}, {total} {snap_word}");
    if !interrupted.is_empty() {
        // F: an interrupted snapshot is a FACT to surface, never a silent "N snapshots".
        message.push_str(&format!(", {} interrupted", interrupted.len()));
    } else if prunable > 0 {
        message.push_str(&format!(" ({prunable} prunable)"));
    }

    // Details (F1): name EVERY snapshot's reader-frame state + outcome — a READY snapshot's
    // "completed <time>" is a first-class fact too, not only the interrupted ones (review-2 gap: doctor
    // must render per-snapshot state/outcome from `snapshots[]`). Mirrors `rmap repo info`'s per-snapshot
    // lines (same facts, same reader frame). Then the interrupted reclaim next-action, then the D3
    // enrichment next-action when a READY snapshot exists.
    let all_snapshots: Vec<&serde_json::Value> = response
        .get("snapshots")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let mut detail_lines: Vec<String> = Vec::new();
    for snap in &all_snapshots {
        let state = snap.get("state").and_then(|v| v.as_str()).unwrap_or("?");
        let outcome = snap.get("outcome").and_then(|v| v.as_str()).unwrap_or("");
        let created = snap
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        detail_lines.push(format!("{state}: {outcome} (created {created})"));
    }
    if !interrupted.is_empty() {
        detail_lines.push(
            "reclaim: re-run `rmap index` for a fresh snapshot; interrupted snapshots are listed by `rmap maintenance prune`".to_string(),
        );
    }
    if ready > 0 {
        // ENRICH-LIFECYCLE-1: auto-enrichment now runs after every index/refresh. The full lifecycle
        // (completed / skipped-with-reason / disabled) is the authoritative Daemon-section
        // `enrichment` line (daemon_info); this Storage-section pointer just states it is automatic
        // so the old "not run automatically" claim is no longer a standing (now-false) statement.
        detail_lines.push(
            "enrichment: runs automatically after each index (see the Daemon 'enrichment' line)"
                .to_string(),
        );
    }

    ProbeResult {
        name: "storage".to_string(),
        passed: true,
        message,
        details: if detail_lines.is_empty() {
            None
        } else {
            Some(detail_lines.join("\n        "))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Contract E: a DB held by a live daemon op is healthy "in use by daemon", NEVER an error.
    #[test]
    fn in_use_by_daemon_is_healthy_not_error() {
        let response = json!({
            "db_size_bytes": 4_000_000_000_u64,
            "in_use_by_daemon": true,
            "operation": { "kind": "index", "repo": "/repos/big" },
            "snapshots": serde_json::Value::Null,
        });
        let probe = storage_probe_from_facts(&response);
        assert!(
            probe.passed,
            "in-use is healthy, not a failure: {:?}",
            probe
        );
        assert!(probe.message.contains("in use by daemon"));
        assert!(probe.message.contains("indexing /repos/big"));
        assert!(
            !probe.message.to_lowercase().contains("error opening"),
            "must never say 'error opening database' for a live daemon lock"
        );
    }

    // Contract F: an interrupted (non-READY) snapshot is NAMED with state + outcome, not hidden.
    #[test]
    fn interrupted_snapshot_is_named_with_state_and_outcome() {
        let response = json!({
            "db_size_bytes": 4_000_000_000_u64,
            "in_use_by_daemon": false,
            "total_snapshots": 1,
            "ready_snapshots": 0,
            "prunable_snapshots": 0,
            "interrupted_snapshots": [
                { "state": "interrupted", "outcome": "interrupted before completion (index did not finalize)", "created_at": "2026-07-02T10:00:00Z" }
            ],
            "snapshots": [
                { "state": "interrupted", "outcome": "interrupted before completion (index did not finalize)", "created_at": "2026-07-02T10:00:00Z" }
            ],
        });
        let probe = storage_probe_from_facts(&response);
        // The "1 snapshot, 4 GB" non-answer is replaced by a stated interrupted count + detail.
        assert!(probe.message.contains("1 interrupted"), "{}", probe.message);
        let details = probe.details.expect("interrupted detail present");
        // F1: the per-snapshot line names STATE + OUTCOME + when (uniform "<state>: <outcome> (created …)").
        assert!(
            details.contains("interrupted:"),
            "names the state: {details}"
        );
        assert!(
            details.contains("interrupted before completion"),
            "names the outcome: {details}"
        );
        assert!(details.contains("2026-07-02"), "names when: {details}");
        assert!(details.contains("rmap maintenance prune"));
    }

    // Contract F1 (review-2 gap): a READY snapshot's state + outcome ("ready: completed <time>") is a
    // first-class fact in doctor's storage details too — rendered from `snapshots[]`, not only the
    // interrupted ones. Same facts `rmap repo info` already shows.
    #[test]
    fn ready_snapshot_state_and_outcome_are_rendered() {
        let response = json!({
            "db_size_bytes": 1_000_000_u64,
            "in_use_by_daemon": false,
            "total_snapshots": 1,
            "ready_snapshots": 1,
            "prunable_snapshots": 0,
            "interrupted_snapshots": [],
            "snapshots": [
                { "state": "ready", "outcome": "completed 2026-07-02T10:05:00Z", "created_at": "2026-07-02T10:00:00Z" }
            ],
        });
        let probe = storage_probe_from_facts(&response);
        assert!(probe.passed);
        let details = probe
            .details
            .expect("per-snapshot detail present for a READY-only repo");
        assert!(
            details.contains("ready:"),
            "renders the READY state: {details}"
        );
        assert!(
            details.contains("completed 2026-07-02T10:05:00Z"),
            "renders the last-index outcome: {details}"
        );
    }

    // Contract D3 (ENRICH-LIFECYCLE-1): a READY snapshot's Storage-section enrichment line states that
    // enrichment runs AUTOMATICALLY and points at the authoritative Daemon `enrichment` lifecycle line.
    // Pre-slice this was a manual "run `rmap enrich`" next-action; that claim is now FALSE (enrichment
    // auto-runs after every index), so the line — and this assertion — moved to the automatic form. The
    // manual next-action (install a toolchain) now lives on the Daemon `enrichment` line (daemon_info).
    #[test]
    fn ready_snapshot_notes_enrichment_runs_automatically() {
        let response = json!({
            "db_size_bytes": 1_000_000_u64,
            "in_use_by_daemon": false,
            "total_snapshots": 1,
            "ready_snapshots": 1,
            "prunable_snapshots": 0,
            "interrupted_snapshots": [],
            "snapshots": [ { "state": "ready", "outcome": "completed", "created_at": "t" } ],
        });
        let probe = storage_probe_from_facts(&response);
        let details = probe.details.expect("enrichment line present");
        assert!(details.contains("enrichment:"), "{details}");
        assert!(
            details.contains("runs automatically"),
            "the Storage line states enrichment is automatic, not a manual next-action: {details}"
        );
        assert!(
            details.contains("Daemon 'enrichment' line"),
            "and points at the authoritative Daemon lifecycle line: {details}"
        );
    }

    // Contract E: a genuine (non-contention) read failure is a real FAIL, distinct from in-use.
    #[test]
    fn genuine_read_error_fails() {
        let response = json!({
            "db_size_bytes": 100_u64,
            "in_use_by_daemon": false,
            "read_error": "database disk image is malformed",
            "snapshots": serde_json::Value::Null,
        });
        let probe = storage_probe_from_facts(&response);
        assert!(!probe.passed, "a corrupt/absent DB is a genuine failure");
        assert!(probe.details.unwrap().contains("malformed"));
    }
}
