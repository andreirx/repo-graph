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

    // DAEMON-CRASH-RECOVERY-1 (F9): a lock the daemon cannot attribute to its own activity (the
    // in_use_by_daemon short-circuit already ran) is transient — another process holds the DB, or a
    // just-restarted daemon is still opening it. Reader-frame, NOT a raw FAIL: the DB is fine, the
    // read simply cannot proceed this instant. `passed` stays true (a retryable state never flips the
    // health verdict).
    if response
        .get("locked_by_other")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return ProbeResult {
            name: "storage".to_string(),
            passed: true,
            message: format!("db: {size_human}, held by another process — daemon restarting?"),
            details: Some(
                "the repo database is locked by another process; if the daemon just restarted this \
                 clears in a moment — re-run `rmap doctor`"
                    .to_string(),
            ),
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
        // "last enrichment pass (daemon-wide)" line (daemon_info); this Storage-section pointer just
        // states it is automatic so the old "not run automatically" claim is no longer a standing
        // (now-false) statement.
        detail_lines.push(
            "enrichment: runs automatically after each index \
             (see the Daemon 'last enrichment pass (daemon-wide)' line)"
                .to_string(),
        );
    }

    // PERSIST-RECURSION-1: honest degradation from the latest index — files skipped for
    // pathological AST nesting, or an isolated postpass failure. The index COMPLETED, so this
    // is NOT a health failure (`passed` stays true); the reader is simply told which facts are
    // missing and why. The reader-language lines are computed daemon-side (snapshot_facts) and
    // printed verbatim — this is a trivial array extraction, no reader-language logic here.
    if let Some(lines) = response
        .get("extraction_degradations")
        .and_then(|d| d.get("lines"))
        .and_then(|v| v.as_array())
    {
        for line in lines.iter().filter_map(|l| l.as_str()) {
            detail_lines.push(line.to_string());
        }
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

/// RECON-M-R3a: the `witness_ledger` doctor probe from the `storage_health` facts — the
/// OPERATIONAL half of the divergence posture (recon-design-1 §5.4): ledger presence/currency +
/// the LAST BUILD OUTCOME with its failure reason when absent, per-partition adoption counts,
/// the R-RAT-4 colliding keys, the occurrence-delta enumeration, and the per-partition coverage
/// regimes with their reason-specific next actions. `None` when the daemon attached no block
/// (zero-SCIP repos: absence, never zeros — R-0).
///
/// Health verdict: always `passed` — an absent/superseded/failed-to-build ledger is a
/// self-healing measurement state (rebuilt on the next call-graph read), never installation
/// ill-health; the FACT is stated, the verdict unchanged (the F9 transient precedent). A
/// genuine storage fault also fails the sibling `storage` probe on its own evidence.
pub(super) fn witness_probe_from_facts(response: &serde_json::Value) -> Option<ProbeResult> {
    let block = response.get("witness_ledger")?;
    let ledger = block.get("ledger")?;
    let mut details: Vec<String> = Vec::new();

    let message = if ledger.get("current").and_then(|v| v.as_bool()) == Some(true) {
        match block.get("measured") {
            Some(m) if !m.is_null() => {
                details.extend(crate::presentation::witnesses::measurement_lines(m));
                witness_measured_details(m, &mut details);
                let fp = ledger
                    .get("fingerprint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                details.push(format!("measured at fingerprint {fp}"));
                "ledger current — union accounting measured".to_string()
            }
            _ => {
                let reason = ledger
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unmeasured");
                format!("ledger present but measured nothing ({reason})")
            }
        }
    } else if ledger.get("present").and_then(|v| v.as_bool()) == Some(true) {
        // Review-0 defect (a): a superseded ledger must not MASK a failed latest rebuild —
        // when the daemon retained a failure beside the superseded fact, both render.
        if ledger.get("last_build_outcome").and_then(|v| v.as_str()) == Some("failed") {
            let reason = ledger
                .get("failure_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown reason");
            if let Some(fp) = ledger.get("failed_fingerprint").and_then(|v| v.as_str()) {
                details.push(format!("failed rebuild was keyed at fingerprint {fp}"));
            }
            format!(
                "ledger superseded by witness movement, and the latest re-measurement \
                 attempt failed ({reason}) — retried on the next call-graph read"
            )
        } else {
            "ledger superseded by witness movement — re-measured on the next call-graph read"
                .to_string()
        }
    } else {
        // Absent: the LAST CAPTURE OUTCOME + its failure reason (the §4.2 transient-2 fact —
        // an operational truth about US, stated here, never on a repo-facts surface).
        match ledger.get("last_build_outcome").and_then(|v| v.as_str()) {
            Some("failed") => {
                let reason = ledger
                    .get("failure_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown reason");
                if let Some(fp) = ledger.get("failed_fingerprint").and_then(|v| v.as_str()) {
                    details.push(format!("failed build was keyed at fingerprint {fp}"));
                }
                format!("ledger absent — last measurement attempt failed ({reason})")
            }
            Some(outcome) => format!("ledger absent — {outcome}"),
            None => "ledger absent".to_string(),
        }
    };

    // Producer capability truth (doctor's toolchain surface — the D-R1 carve-out).
    if let Some(producer) = block.get("producer") {
        if let (Some(name), Some(provisioned)) = (
            producer.get("name").and_then(|v| v.as_str()),
            producer.get("provisioned").and_then(|v| v.as_bool()),
        ) {
            details.push(format!(
                "producer {name}: {}",
                if provisioned {
                    "provisioned"
                } else {
                    "not provisioned"
                }
            ));
        }
    }
    // The reason-specific W-ONE posture lines (three distinct reasons + next actions).
    details.extend(crate::presentation::witnesses::regime_lines(block));

    Some(ProbeResult {
        name: "witness_ledger".to_string(),
        passed: true,
        message,
        details: if details.is_empty() {
            None
        } else {
            Some(details.join("\n        "))
        },
    })
}

/// The measured block's OPERATIONAL detail lines (doctor-only tier): per-partition adoption
/// counts, colliding keys, and the occurrence-delta enumeration. Long enumerations truncate
/// WITH a stated count (no silent caps).
fn witness_measured_details(measured: &serde_json::Value, details: &mut Vec<String>) {
    if let Some(adoption) = measured.get("adoption").and_then(|v| v.as_object()) {
        for (partition, counts) in adoption {
            // Review-1 item 5: all three populations or no line — a missing field must never
            // render as a measured zero (malformed/additive payload → the line is absent).
            let (Some(adopted), Some(fallback), Some(file_scope)) = (
                counts.get("adopted").and_then(|v| v.as_u64()),
                counts.get("fallback").and_then(|v| v.as_u64()),
                counts.get("file_scope").and_then(|v| v.as_u64()),
            ) else {
                continue;
            };
            details.push(format!(
                "adoption {partition}: {adopted} adopted / {fallback} fallback / \
                 {file_scope} file-scope"
            ));
        }
    }
    if let Some(colliding) = measured.get("colliding_keys").and_then(|v| v.as_object()) {
        if let Some(line) = measured.get("collision_line").and_then(|v| v.as_str()) {
            details.push(line.to_string());
        }
        const KEY_CAP: usize = 8;
        for (partition, keys) in colliding {
            let keys: Vec<&str> = keys
                .as_array()
                .map(|a| a.iter().filter_map(|k| k.as_str()).collect())
                .unwrap_or_default();
            let shown: Vec<&str> = keys.iter().take(KEY_CAP).copied().collect();
            let suffix = if keys.len() > KEY_CAP {
                format!(" … and {} more", keys.len() - KEY_CAP)
            } else {
                String::new()
            };
            details.push(format!(
                "colliding keys in {partition}: {}{suffix}",
                shown.join(", ")
            ));
        }
    }
    if let Some(deltas) = measured
        .get("occurrence_delta_pairs")
        .and_then(|v| v.as_array())
    {
        const DELTA_CAP: usize = 8;
        // Review-1 item 5: a pair renders only with BOTH its counts — a missing count must
        // never render as "syntax 0" / "compiler 0" (an invented measurement). The stated
        // remainder counts RENDERABLE pairs, so the cap line stays truthful under skips.
        let renderable: Vec<String> = deltas
            .iter()
            .filter_map(|d| {
                let caller = d.get("caller").and_then(|v| v.as_str())?;
                let callee = d.get("callee").and_then(|v| v.as_str())?;
                let p = d.get("p").and_then(|v| v.as_u64())?;
                let s = d.get("s_calls").and_then(|v| v.as_u64())?;
                Some(format!(
                    "occurrence delta: {caller} → {callee} (syntax {p}, compiler {s})"
                ))
            })
            .collect();
        details.extend(renderable.iter().take(DELTA_CAP).cloned());
        if renderable.len() > DELTA_CAP {
            details.push(format!(
                "… and {} more occurrence deltas",
                renderable.len() - DELTA_CAP
            ));
        }
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
            details.contains("Daemon 'last enrichment pass (daemon-wide)' line"),
            "and points at the authoritative daemon-wide lifecycle line (ruling CS1-4): {details}"
        );
    }

    // PERSIST-RECURSION-1: a deep-nesting skip (or isolated postpass failure) from the latest
    // index is surfaced as an honest reader-frame line in the storage details — NOT a health
    // failure (the index completed), just a statement of which facts are missing and why.
    #[test]
    fn extraction_degradation_is_surfaced_but_not_a_failure() {
        let response = json!({
            "db_size_bytes": 1_000_000_u64,
            "in_use_by_daemon": false,
            "total_snapshots": 1,
            "ready_snapshots": 1,
            "prunable_snapshots": 0,
            "interrupted_snapshots": [],
            "snapshots": [ { "state": "ready", "outcome": "completed", "created_at": "t" } ],
            "extraction_degradations": {
                "deep_nesting_skips": { "boundary_facts": 1 },
                "postpass_errors": {},
                "lines": [ "boundary facts skipped for 1 file (pathological nesting)" ],
            },
        });
        let probe = storage_probe_from_facts(&response);
        assert!(
            probe.passed,
            "an honest skip is not a health failure: {probe:?}"
        );
        let details = probe.details.expect("degradation detail present");
        assert!(
            details.contains("boundary facts skipped for 1 file (pathological nesting)"),
            "the reader-frame line is rendered in doctor's storage details: {details}"
        );
    }

    // DAEMON-CRASH-RECOVERY-1 (F9): a lock the daemon cannot attribute to itself is a transient,
    // reader-frame condition ("held by another process — daemon restarting?"), NOT a raw FAIL — the
    // DB is fine, the read simply cannot proceed this instant.
    #[test]
    fn lock_race_is_reader_frame_not_a_failure() {
        let response = json!({
            "db_size_bytes": 4_000_000_000_u64,
            "in_use_by_daemon": false,
            "locked_by_other": true,
            "read_error": "database is locked",
            "snapshots": serde_json::Value::Null,
        });
        let probe = storage_probe_from_facts(&response);
        assert!(
            probe.passed,
            "a lock race is transient, not a health failure: {probe:?}"
        );
        assert!(
            probe.message.contains("held by another process")
                && probe.message.contains("daemon restarting?"),
            "reader-frame message: {}",
            probe.message
        );
        assert!(
            !probe.message.to_lowercase().contains("cannot read"),
            "must not read as a corrupt-DB failure: {}",
            probe.message
        );
    }

    // ── RECON-M-R3a: the witness-ledger operational probe ───────────────────────────────

    // R-0: no daemon block → no probe (absence, never zeros).
    #[test]
    fn witness_probe_absent_when_daemon_attached_no_block() {
        let response = json!({ "db_size_bytes": 1, "in_use_by_daemon": false });
        assert!(witness_probe_from_facts(&response).is_none());
    }

    // Ledger absent + retained build failure: the LAST CAPTURE OUTCOME + its reason render
    // (the M-R3a gate's ledger-ABSENT doctor rendering) — and the probe stays healthy (a
    // self-healing measurement state is not installation ill-health).
    #[test]
    fn witness_probe_renders_last_build_failure_reason() {
        let response = json!({
            "db_size_bytes": 1, "in_use_by_daemon": false,
            "witness_ledger": {
                "producer": {"name": "scip-typescript", "provisioned": false},
                "regimes": [],
                "ledger": {
                    "present": false,
                    "last_build_outcome": "failed",
                    "failed_fingerprint": "fp1",
                    "failure_reason": "sqlite_error_during_ledger_walk",
                },
            },
        });
        let probe = witness_probe_from_facts(&response).expect("block → probe");
        assert!(probe.passed, "measurement absence is not ill-health");
        assert!(
            probe.message.contains("last measurement attempt failed")
                && probe.message.contains("sqlite_error_during_ledger_walk"),
            "{}",
            probe.message
        );
        let details = probe.details.expect("failure fingerprint + producer line");
        assert!(details.contains("fp1"), "{details}");
        assert!(details.contains("producer scip-typescript: not provisioned"));
    }

    // Review-0 defect (a): a superseded ledger with a retained failure renders BOTH facts —
    // the supersession AND the latest failed rebuild — never "superseded" alone.
    #[test]
    fn witness_probe_superseded_renders_the_latest_build_failure_beside_it() {
        let response = json!({
            "db_size_bytes": 1, "in_use_by_daemon": false,
            "witness_ledger": {
                "producer": {"name": "scip-typescript", "provisioned": true},
                "regimes": [],
                "ledger": {
                    "present": true,
                    "current": false,
                    "note": "superseded by witness movement; rebuilt on the next call-graph read",
                    "last_build_outcome": "failed",
                    "failed_fingerprint": "fp_new",
                    "failure_reason": "sqlite_error_during_ledger_walk",
                },
            },
        });
        let probe = witness_probe_from_facts(&response).expect("block → probe");
        assert!(probe.passed, "self-healing measurement state stays healthy");
        assert!(
            probe.message.contains("superseded")
                && probe
                    .message
                    .contains("failed (sqlite_error_during_ledger_walk)"),
            "both facts must render (the masked-failure defect): {}",
            probe.message
        );
        assert!(probe
            .details
            .expect("fingerprint detail")
            .contains("fp_new"));

        // Without a retained failure the plain superseded line is unchanged.
        let plain = json!({
            "db_size_bytes": 1, "in_use_by_daemon": false,
            "witness_ledger": {
                "producer": {"name": "scip-typescript", "provisioned": true},
                "regimes": [],
                "ledger": {"present": true, "current": false},
            },
        });
        let probe = witness_probe_from_facts(&plain).expect("block → probe");
        assert!(
            probe.message.contains("superseded") && !probe.message.contains("failed"),
            "{}",
            probe.message
        );
    }

    // Measured: the operational tier renders adoption counts, the collision line + keys, the
    // occurrence-delta enumeration, and the W-ONE regime next-actions.
    #[test]
    fn witness_probe_renders_measured_operational_detail() {
        let response = json!({
            "db_size_bytes": 1, "in_use_by_daemon": false,
            "witness_ledger": {
                "producer": {"name": "scip-typescript", "provisioned": true},
                "regimes": [
                    {"partition": "app", "language": "TypeScript", "regime": "W-ONE",
                     "reason": "stale",
                     "posture": "compiler-side analysis here is out of date (the source changed after the compiler last ran)",
                     "next_action": "refresh `app` to re-enable corroboration"},
                ],
                "ledger": {"present": true, "current": true, "fingerprint": "fp2"},
                "measured": {
                    "accounting": "union",
                    "coverage": {"languages": ["TypeScript"], "partitions": ["app"], "fingerprint": "fp2"},
                    "pipeline_calls": 10, "union_calls": 12, "dual_measured": 9,
                    "agreement_pct": 88.9,
                    "both": {"instances": 8, "identities": 8},
                    "syntactic_only": {"boundary": 1, "file_scope": 0, "uncorroborated": 0, "multiplicity": 0, "identities": 1},
                    "semantic_only_calls": {"new_pair": 2, "multiplicity": 1, "identities": 2},
                    "unmeasured_edges": {"instances": 1, "identities": 1},
                    "identity_suspect": 0,
                    "identity_collision": {"instances": 2, "identities": 2},
                    "projections": {"total": 20, "unanswerable": 3},
                    "references": 40,
                    "adoption": {"app": {"language": "TypeScript", "adopted": 5, "fallback": 2, "file_scope": 1}},
                    "colliding_keys": {"app": ["k1", "k2"]},
                    "collision_line": "2 symbol identities collide between the syntax index and the compiler index — 2 compiler-witnessed call instances withheld; shown separately, never merged",
                    "occurrence_delta_pairs": [{"caller": "a", "callee": "b", "p": 2, "s_calls": 1}],
                },
            },
        });
        let probe = witness_probe_from_facts(&response).expect("block → probe");
        assert!(
            probe.message.contains("ledger current"),
            "{}",
            probe.message
        );
        let details = probe.details.expect("operational detail");
        assert!(details.contains("adoption app: 5 adopted / 2 fallback / 1 file-scope"));
        // Defect (b): both populations, each with its unit (keys collide; instances withheld).
        assert!(details.contains("2 symbol identities collide"), "{details}");
        assert!(
            details.contains("2 compiler-witnessed call instances withheld"),
            "{details}"
        );
        assert!(details.contains("colliding keys in app: k1, k2"));
        assert!(
            details.contains("occurrence delta: a → b (syntax 2, compiler 1)"),
            "{details}"
        );
        assert!(
            details.contains("app: compiler-side analysis here is out of date")
                && details.contains("refresh `app` to re-enable corroboration"),
            "the W-ONE reason line + next action render on doctor: {details}"
        );
        assert!(details.contains("measured at fingerprint fp2"));
    }

    /// Review-1 item 5: a malformed/additive measured payload must never render an absent
    /// count as a measured zero — the adoption line requires ALL three populations, and a
    /// delta pair requires BOTH counts; malformed entries render absence, intact ones render.
    #[test]
    fn witness_probe_malformed_counts_render_absence_never_invented_zeros() {
        let response = json!({
            "db_size_bytes": 1, "in_use_by_daemon": false,
            "witness_ledger": {
                "producer": {"name": "scip-typescript", "provisioned": true},
                "regimes": [],
                "ledger": {"present": true, "current": true, "fingerprint": "fp3"},
                "measured": {
                    "accounting": "union",
                    "coverage": {"languages": ["TypeScript"], "partitions": ["app"], "fingerprint": "fp3"},
                    "pipeline_calls": 10, "union_calls": 10, "dual_measured": 9,
                    "both": {"instances": 9, "identities": 9},
                    // `fallback` missing on `app`; `broken` carries no counts at all.
                    "adoption": {
                        "app": {"language": "TypeScript", "adopted": 5, "file_scope": 1},
                        "ok": {"language": "TypeScript", "adopted": 3, "fallback": 0, "file_scope": 2},
                        "broken": {"language": "TypeScript"},
                    },
                    // First pair lacks `s_calls`; second is intact.
                    "occurrence_delta_pairs": [
                        {"caller": "a", "callee": "b", "p": 2},
                        {"caller": "c", "callee": "d", "p": 1, "s_calls": 3},
                    ],
                },
            },
        });
        let probe = witness_probe_from_facts(&response).expect("block → probe");
        let details = probe.details.expect("detail lines");
        assert!(
            !details.contains("adoption app:") && !details.contains("adoption broken:"),
            "a partial adoption row must not render invented zeros: {details}"
        );
        assert!(
            details.contains("adoption ok: 3 adopted / 0 fallback / 2 file-scope"),
            "the intact row still renders (0 here is MEASURED, present in the payload): {details}"
        );
        assert!(
            !details.contains("a → b"),
            "a delta pair without both counts must not render: {details}"
        );
        assert!(
            details.contains("occurrence delta: c → d (syntax 1, compiler 3)"),
            "{details}"
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
