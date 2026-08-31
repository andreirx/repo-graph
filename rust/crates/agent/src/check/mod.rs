//! Check use case — three-phase pipeline.
//!
//! Phase 1: Gather substrate facts through existing ports.
//! Phase 2: Build CheckInput, call the pure `check()` reducer.
//! Phase 3: Map CheckResult into the shared OrientResult envelope.
//!
//! The use case function `run_check` is the only public entry
//! point. It is generic over storage that satisfies both
//! `AgentStorageRead` (agent port) and `GateStorageRead` (gate
//! policy port), matching the orient pattern.

pub mod coherent;
pub mod evaluate;
pub mod reduce;
pub mod types;

pub use coherent::check_to_coherent;
pub use evaluate::{
    enrichment_state_summary, enrichment_state_token, evaluate_conditions,
    ENRICHMENT_SUMMARY_NOT_APPLICABLE, ENRICHMENT_SUMMARY_NOT_RUN, ENRICHMENT_SUMMARY_RAN,
    ENRICHMENT_SUMMARY_UNAVAILABLE,
};
pub use reduce::{check, reduce_verdict};
pub use types::*;

use repo_graph_gate::{GateMode, GateStorageRead};

use crate::confidence::derive_repo_confidence;
use crate::dto::ceiling_fact::CeilingFact;
use crate::dto::envelope::{Confidence, Focus, OrientResult, CHECK_COMMAND, ORIENT_SCHEMA};
use crate::dto::signal::{
    CheckConditionEvidence, CheckFailEvidence, CheckIncompleteEvidence, CheckPassEvidence, Signal,
    SnapshotInfoEvidence,
};
use crate::errors::CheckError;
use crate::ranking;
use crate::storage_port::{AgentCancelCheck, AgentStorageRead};

/// Entry point for the check use case.
///
/// Generic over a single storage handle that satisfies both
/// `AgentStorageRead` and `GateStorageRead`. Same pattern as
/// `orient()`.
///
/// `now` is an ISO 8601 timestamp used for waiver expiry
/// evaluation in the gate assembly. The check crate is
/// clock-free: callers must supply `now` explicitly.
///
/// Returns `CheckError::NoRepo` when the repo does not exist.
/// Missing snapshot is NOT an error -- it produces
/// `CHECK_INCOMPLETE` with only the `SNAPSHOT_EXISTS` condition.
pub fn run_check<S: AgentStorageRead + GateStorageRead + ?Sized>(
    storage: &S,
    repo_uid: &str,
    now: &str,
) -> Result<OrientResult, CheckError> {
    // Simple entry: no working-tree drift available (no repo path / git access at
    // this layer). The `INDEX_DRIFT` condition is omitted, not fabricated. The
    // daemon uses `run_check_cancellable` with a computed `IndexDrift`.
    // The trailing `None` is `ceiling_fact`: the simple entry performs no ceiling analysis.
    run_check_cancellable(storage, repo_uid, now, None, None, None, &mut || {
        std::ops::ControlFlow::Continue(())
    })
}

/// Cancellable entry point for the check use case (DAEMON-CANCEL-3).
///
/// Identical to [`run_check`] but threads a cooperative `cancel` checkpoint into the
/// one demonstrated heavy path `check` inherits — the trust assembly's
/// unresolved-sample loop, reached via `get_trust_summary_cancellable`. The daemon's
/// check handler runs this whole function on a worker thread under CANCEL-2's
/// `sqlite3_interrupt` supervisor, so the trust `compute_module_stats` SQL and the
/// gate complexity-measurement load (both opaque `SELECT`s) are aborted by the
/// interrupt on disconnect, while this `cancel` covers the pure trust loop — the two
/// mechanisms compose. [`run_check`] passes a no-op, preserving byte-identical
/// behavior for every other caller. The small gate-evidence parsing loops are left
/// alone (NARROW scope; bounded by matching obligations, and SQL-interruptible).
pub fn run_check_cancellable<S: AgentStorageRead + GateStorageRead + ?Sized>(
    storage: &S,
    repo_uid: &str,
    now: &str,
    index_drift: Option<crate::dto::index_drift::IndexDrift>,
    // ORIENT-FACT-COHERENCE-1 (operator ruling review-3 = Option 2): the daemon-injected enrichment-
    // lifecycle override, an enum-typed fact the pure core cannot derive from storage (mirrors
    // `index_drift` above — same daemon→agent injection precedent, INDEX-BASIS-1). `None` = daemon
    // supplied no override — derive the enrichment state from storage as before (NOT `NotRun`).
    // `Some(state)` = authoritative daemon lifecycle truth; today the daemon injects
    // `EnrichmentState::InFlight` when a pass is queued/running, so the ENRICHMENT_STATE condition
    // renders the honest non-failing in-flight form instead of the stale "Enrichment phase did not run"
    // Fail — check and orient tell ONE story for one snapshot. Non-daemon callers (the `run_check`
    // wrapper, tests) pass `None`, preserving byte-identical output.
    enrich_state_override: Option<crate::storage_port::EnrichmentState>,
    // CHECK-SIGNAL-1: the daemon-injected call-graph-resolution CAPABILITY fact (§2.1), computed
    // from the SAME materially-present-language × resolver facts the D5 CTA uses (`reader_context`)
    // — same daemon→agent injection precedent as `index_drift` / `enrich_state_override`. An
    // exhaustive 3-variant sum ([`CeilingFact`]), operator ruling 2026-08-31 `ceiling-read-unknown`
    // (superseding build-1's `Option<ResolutionCeiling>` whose `None` conflated no-ceiling with a
    // failed read): `Some(Ceiling)` = permanent ceiling → passing stated limitation +
    // non-failing ENRICHMENT_STATE; `Some(NoCeiling)` = actionable → pre-slice failing; `Some(Unknown)`
    // = capability read failed → pre-slice failing WITH the reason surfaced (never a false Pass). The
    // OUTER `None` (the `run_check` wrapper / tests) = caller performed no ceiling analysis →
    // pre-slice behavior, byte-identical (mirrors the sibling `Option<IndexDrift>`).
    ceiling_fact: Option<CeilingFact>,
    cancel: AgentCancelCheck<'_>,
) -> Result<OrientResult, CheckError> {
    // ── Phase 1: Gather ─────────────────────────────────────────

    // 1. Resolve repo identity.
    let repo = storage
        .get_repo(repo_uid)?
        .ok_or_else(|| CheckError::NoRepo {
            repo_uid: repo_uid.to_string(),
        })?;

    // 2. Try to get snapshot.
    let snapshot_opt = storage.get_latest_snapshot(repo_uid)?;

    let (input, snapshot_uid, confidence) = match snapshot_opt {
        None => {
            // No snapshot: build minimal CheckInput. The reducer
            // will produce CHECK_INCOMPLETE with only
            // SNAPSHOT_EXISTS condition.
            let input = CheckInput {
                snapshot_exists: false,
                files_total: 0,
                stale_file_count: 0,
                call_graph_reliability: None,
                resolved_calls: 0,
                unresolved_calls_internal_like: 0,
                unresolved_calls: 0,
                unresolved_calls_unknown: 0,
                external_targets: Vec::new(),
                enrichment_state: None,
                gate_outcome: None,
                // No snapshot → conditions 2+ (incl. INDEX_DRIFT) are not evaluated.
                index_drift: None,
                // No snapshot → the call-graph condition is not evaluated; the ceiling fact is moot.
                ceiling_fact: None,
            };
            (input, String::new(), Confidence::Low)
        }
        Some(ref snapshot) => {
            let snap_uid = snapshot.snapshot_uid.clone();

            // 3. Get stale files.
            let stale_files = storage.get_stale_files(&snap_uid)?;

            // 4. Get trust summary (DAEMON-CANCEL-3: cancellable — the heavy trust
            //    sample loop check inherits; the SQL is interrupt-covered on the worker).
            let trust = storage.get_trust_summary_cancellable(repo_uid, &snap_uid, cancel)?;

            // 5. Get gate outcome.
            let gate_outcome = gather_gate_outcome(storage, repo_uid, &snap_uid, now);

            // Derive confidence from trust data.
            let stale = !stale_files.is_empty();
            let conf = derive_repo_confidence(&trust, stale);

            let input = CheckInput {
                snapshot_exists: true,
                files_total: snapshot.files_total,
                stale_file_count: stale_files.len() as u64,
                call_graph_reliability: Some(trust.call_graph_reliability.level),
                // RELIABILITY-REFRAME-1: the FULL reader-frame projection facts —
                // straight projection of the summary already read above (no new
                // storage read). review-3 §1: external share (via `unresolved_calls`
                // total) + named targets reach check like orient/trust; review-3 §2:
                // the unclassified count feeds the conservative-rate caveat.
                resolved_calls: trust.resolved_calls,
                unresolved_calls_internal_like: trust.unresolved_calls_internal_like,
                unresolved_calls: trust.unresolved_calls,
                unresolved_calls_unknown: trust.unresolved_calls_unknown,
                external_targets: trust.external_targets.clone(),
                // ORIENT-FACT-COHERENCE-1: overlay the daemon-injected lifecycle fact onto the
                // persisted enrichment state, so the condition never renders "did not run" for a phase
                // that is running. `None` keeps the persisted state; `Some(state)` is authoritative. The
                // `Some(_)` (state-present) invariant is preserved.
                enrichment_state: Some(enrich_state_override.unwrap_or(trust.enrichment_state)),
                gate_outcome: Some(gate_outcome),
                // INDEX-BASIS-1: the daemon-computed working-tree drift (git +
                // storage), passed through as pre-fetched data. `None` only on the
                // simple `run_check` entry / tests, where the condition is omitted.
                index_drift: index_drift.clone(),
                // CHECK-SIGNAL-1: the daemon-injected call-graph capability fact, threaded through
                // verbatim (the pure reducer performs no I/O). `None` on the `run_check` wrapper /
                // tests → pre-slice behavior; `Some(Ceiling|NoCeiling|Unknown)` from the daemon.
                ceiling_fact: ceiling_fact.clone(),
            };

            (input, snap_uid, conf)
        }
    };

    // ── Phase 2: Reduce ─────────────────────────────────────────

    let result = check(&input);

    // ── Phase 3: Format ─────────────────────────────────────────

    let mut signals = vec![build_verdict_signal(&result)];

    // Add SNAPSHOT_INFO if snapshot exists.
    if let Some(ref snapshot) = snapshot_opt {
        signals.push(Signal::snapshot_info(SnapshotInfoEvidence {
            snapshot_uid: snapshot.snapshot_uid.clone(),
            scope: snapshot.scope.clone(),
            basis_commit: snapshot.basis_commit.clone(),
            created_at: snapshot.created_at.clone(),
        }));
    }

    // Sort + rank (even with 1-2 signals, keeps the contract
    // consistent with orient).
    ranking::sort_and_rank(&mut signals);

    Ok(OrientResult {
        schema: ORIENT_SCHEMA,
        command: CHECK_COMMAND,
        repo: repo.name,
        display_name: None, // Populated by daemon handler
        snapshot: snapshot_uid,
        focus: Focus::repo(),
        confidence,

        documentation: None,

        signals,
        signals_truncated: None,
        signals_omitted_count: None,

        limits: Vec::new(),
        limits_truncated: None,
        limits_omitted_count: None,

        next: Vec::new(),
        next_truncated: None,
        next_omitted_count: None,

        truncated: false,
    })
}

/// Gather the gate outcome for check input.
///
/// Calls `repo_graph_gate::assemble_from_requirements` through the
/// `GateStorageRead` port. Maps the result to `GateOutcomeForCheck`.
///
/// - No active requirements -> `NotConfigured`.
/// - Gate error -> `Incomplete` (not a check error).
/// - `outcome == "pass"` with `total > 0` -> `Pass`.
/// - `outcome == "pass"` with `total == 0` -> `NotConfigured`.
/// - `outcome == "fail"` -> `Fail`.
/// - `outcome == "incomplete"` -> `Incomplete`.
fn gather_gate_outcome<S: GateStorageRead + ?Sized>(
    storage: &S,
    repo_uid: &str,
    snapshot_uid: &str,
    now: &str,
) -> GateOutcomeForCheck {
    // Fetch requirements to detect "not configured".
    let requirements = match storage.get_active_requirements(repo_uid) {
        Ok(reqs) => reqs,
        Err(_) => return GateOutcomeForCheck::Incomplete,
    };

    if requirements.is_empty() {
        return GateOutcomeForCheck::NotConfigured;
    }

    let report = match repo_graph_gate::assemble_from_requirements(
        storage,
        repo_uid,
        snapshot_uid,
        GateMode::Default,
        now,
        requirements,
    ) {
        Ok(r) => r,
        Err(_) => return GateOutcomeForCheck::Incomplete,
    };

    // Zero obligations after assembly = effectively not configured.
    if report.outcome.counts.total == 0 {
        return GateOutcomeForCheck::NotConfigured;
    }

    match report.outcome.outcome.as_str() {
        "pass" => GateOutcomeForCheck::Pass,
        "fail" => GateOutcomeForCheck::Fail,
        "incomplete" => GateOutcomeForCheck::Incomplete,
        _ => GateOutcomeForCheck::Incomplete, // defensive
    }
}

/// Build the single verdict signal from the check result.
fn build_verdict_signal(result: &CheckResult) -> Signal {
    match result.verdict {
        CheckVerdict::Pass => {
            let conditions = result
                .conditions
                .iter()
                .map(condition_to_evidence)
                .collect();
            Signal::check_pass(CheckPassEvidence { conditions })
        }
        CheckVerdict::Fail => {
            let mut fail_conditions = Vec::new();
            let mut passing = Vec::new();
            for c in &result.conditions {
                let ev = condition_to_evidence(c);
                match c.status {
                    ConditionStatus::Fail => fail_conditions.push(ev),
                    _ => passing.push(ev),
                }
            }
            Signal::check_fail(CheckFailEvidence {
                fail_conditions,
                passing,
            })
        }
        CheckVerdict::Incomplete => {
            let mut incomplete_conditions = Vec::new();
            let mut fail_conditions = Vec::new();
            let mut passing = Vec::new();
            for c in &result.conditions {
                let ev = condition_to_evidence(c);
                match c.status {
                    ConditionStatus::Incomplete => incomplete_conditions.push(ev),
                    ConditionStatus::Fail => fail_conditions.push(ev),
                    ConditionStatus::Pass => passing.push(ev),
                }
            }
            Signal::check_incomplete(CheckIncompleteEvidence {
                incomplete_conditions,
                fail_conditions,
                passing,
            })
        }
    }
}

/// Map a `ConditionResult` to a `CheckConditionEvidence`.
fn condition_to_evidence(c: &ConditionResult) -> CheckConditionEvidence {
    CheckConditionEvidence {
        code: c.code.as_str().to_string(),
        status: match c.status {
            ConditionStatus::Pass => "pass".to_string(),
            ConditionStatus::Fail => "fail".to_string(),
            ConditionStatus::Incomplete => "incomplete".to_string(),
        },
        summary: c.summary.clone(),
        // CHECK-SIGNAL-1 (§2.3): the additive `ceiling: true` JSON marker — emitted ONLY on a
        // reclassified permanent-ceiling condition, ABSENT (serialized-skipped `None`) otherwise,
        // so existing consumers stay byte-compatible.
        ceiling: if c.ceiling { Some(true) } else { None },
    }
}

#[cfg(test)]
mod marker_tests {
    use super::*;
    use crate::check::types::{ConditionCode, ConditionResult, ConditionStatus};

    /// CHECK-SIGNAL-1 (§2.3): the additive JSON `ceiling` marker is emitted ONLY on a reclassified
    /// ceiling condition, and ABSENT (serde-skipped) otherwise — so existing consumers keyed on
    /// `{code, status, summary}` see byte-compatible JSON.
    #[test]
    fn ceiling_marker_present_only_when_set_and_serde_skipped_otherwise() {
        let ordinary = condition_to_evidence(&ConditionResult {
            code: ConditionCode::CallGraphReliability,
            status: ConditionStatus::Fail,
            summary: "x".to_string(),
            ceiling: false,
        });
        assert_eq!(ordinary.ceiling, None);
        let json = serde_json::to_string(&ordinary).unwrap();
        assert!(!json.contains("ceiling"), "absent on the wire: {json}");

        let ceiling = condition_to_evidence(&ConditionResult {
            code: ConditionCode::CallGraphReliability,
            status: ConditionStatus::Pass,
            summary: "x".to_string(),
            ceiling: true,
        });
        assert_eq!(ceiling.ceiling, Some(true));
        let json = serde_json::to_string(&ceiling).unwrap();
        assert!(
            json.contains("\"ceiling\":true"),
            "present when set: {json}"
        );
    }
}
