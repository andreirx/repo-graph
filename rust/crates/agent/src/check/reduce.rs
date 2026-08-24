//! Phase 2: Verdict reduction.
//!
//! Collapses a `Vec<ConditionResult>` into a single
//! `CheckVerdict` using strict precedence:
//!
//!   Incomplete > Fail > Pass
//!
//! If any condition is Incomplete, the verdict is Incomplete.
//! Otherwise, if any condition is Fail, the verdict is Fail.
//! Otherwise, the verdict is Pass.

use super::evaluate::evaluate_conditions;
use super::types::{CheckInput, CheckResult, CheckVerdict, ConditionResult, ConditionStatus};

/// Reduce a slice of condition results into a single verdict.
///
/// Precedence: Incomplete > Fail > Pass.
pub fn reduce_verdict(conditions: &[ConditionResult]) -> CheckVerdict {
    let mut has_incomplete = false;
    let mut has_fail = false;

    for c in conditions {
        match c.status {
            ConditionStatus::Incomplete => {
                has_incomplete = true;
            }
            ConditionStatus::Fail => {
                has_fail = true;
            }
            ConditionStatus::Pass => {}
        }
    }

    if has_incomplete {
        CheckVerdict::Incomplete
    } else if has_fail {
        CheckVerdict::Fail
    } else {
        CheckVerdict::Pass
    }
}

/// Convenience: evaluate conditions and reduce in one call.
pub fn check(input: &CheckInput) -> CheckResult {
    let conditions = evaluate_conditions(input);
    let verdict = reduce_verdict(&conditions);
    CheckResult {
        verdict,
        conditions,
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::types::{CheckVerdict, ConditionCode, ConditionStatus, GateOutcomeForCheck};
    use crate::storage_port::{AgentReliabilityLevel, EnrichmentState};

    /// Helper: builds a CheckInput where everything is passing.
    fn all_pass_input() -> CheckInput {
        CheckInput {
            snapshot_exists: true,
            files_total: 42,
            stale_file_count: 0,
            call_graph_reliability: Some(AgentReliabilityLevel::High),
            resolved_calls: 95,
            unresolved_calls_internal_like: 5,
            unresolved_calls: 5, // external = unresolved - internal_like = 0
            unresolved_calls_unknown: 0,
            external_targets: Vec::new(),
            enrichment_state: Some(EnrichmentState::Ran),
            gate_outcome: Some(GateOutcomeForCheck::Pass),
            // INDEX-BASIS-1: clean drift → INDEX_DRIFT Pass, so "all pass" holds.
            index_drift: Some(crate::dto::index_drift::IndexDrift::Clean {
                basis: "abcdef0123456789".to_string(),
            }),
        }
    }

    // ── 1. all_pass ─────────────────────────────────────────

    #[test]
    fn all_pass() {
        let result = check(&all_pass_input());
        assert_eq!(result.verdict, CheckVerdict::Pass);
        assert!(
            result
                .conditions
                .iter()
                .all(|c| c.status == ConditionStatus::Pass),
            "Expected all conditions to pass, got: {:?}",
            result.conditions,
        );
    }

    // ── 1b. call-graph condition speaks the reader's frame ──

    #[test]
    fn call_graph_condition_summary_is_reader_frame_from_shared_view() {
        // RELIABILITY-REFRAME-1 (REVISE #1): check CONSUMES the ONE shared projection —
        // its CALL_GRAPH_RELIABILITY summary is the reader-frame "Your code's calls M%
        // resolved (BAND)" (from `CallReliabilityView`, sentence-cased), NEVER "Call graph
        // reliability is BAND". Genuine in-scope failure (LOW) still fails plainly.
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        input.resolved_calls = 42;
        input.unresolved_calls_internal_like = 58; // 42 / (42 + 58) = 42% in-scope
        input.unresolved_calls = 58; // external = 58 - 58 = 0
        let result = check(&input);
        let cg = result
            .conditions
            .iter()
            .find(|c| matches!(c.code, ConditionCode::CallGraphReliability))
            .expect("CALL_GRAPH_RELIABILITY condition present");
        assert_eq!(cg.status, ConditionStatus::Fail);
        // The verdict sentence is the reader-frame rate + band, NEVER "Call graph reliability is BAND".
        assert!(
            cg.summary.starts_with(
                "Your code's calls 42% resolved (LOW) — verify call/dead claims against source."
            ),
            "reader-frame verdict sentence: {}",
            cg.summary
        );
        assert!(!cg.summary.contains("Call graph reliability"));
    }

    // ── 1b'. check carries the FULL external projection (review-3 §1) ──

    #[test]
    fn call_graph_condition_carries_full_external_projection() {
        // review-3 §1: check consumes the FULL shared projection — REAL external share + the
        // named coverage map + heuristic basis — NOT an `external=0` placeholder. review-3 §2:
        // the conservative-rate caveat rides here too when the unclassified share is material.
        use crate::reliability::ExternalTarget;
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        input.resolved_calls = 42;
        input.unresolved_calls_internal_like = 58; // 42 / (42 + 58) = 42% in-scope
        input.unresolved_calls = 158; // external = 158 - 58 = 100; total_calls = 42 + 158 = 200
        input.unresolved_calls_unknown = 30; // 30 / 100 = 30% ≥ 20% material
        input.external_targets = vec![
            ExternalTarget {
                type_name: "Value".into(),
                count: 30,
            },
            ExternalTarget {
                type_name: "Vec".into(),
                count: 12,
            },
        ];
        let cg = check(&input)
            .conditions
            .into_iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .expect("CALL_GRAPH_RELIABILITY present");
        // The reader-frame rate + band verdict.
        assert!(
            cg.summary.contains("Your code's calls 42% resolved (LOW)"),
            "{}",
            cg.summary
        );
        // The REAL external share (50%), not an external=0 placeholder.
        assert!(
            cg.summary
                .contains("50% of calls go into external libraries"),
            "external share reaches check: {}",
            cg.summary
        );
        // The named coverage map + its compact heuristic basis.
        assert!(
            cg.summary
                .contains("External coverage (heuristic): `Value` (30)"),
            "named targets reach check: {}",
            cg.summary
        );
        assert!(
            cg.summary.contains("not compiler-verified"),
            "heuristic basis reaches check: {}",
            cg.summary
        );
        // The conservative-rate caveat (unknown ≠ internal).
        assert!(
            cg.summary
                .contains("30 of these 100 calls are unclassified"),
            "conservative-rate caveat reaches check: {}",
            cg.summary
        );
    }

    // ── 1c. zero in-scope calls is Incomplete, never a vacuous Pass ──

    #[test]
    fn zero_in_scope_calls_is_incomplete_not_pass() {
        // RELIABILITY-REFRAME-1 (review-1 §1): a repo with NO in-scope calls has nothing to
        // measure. `compute_call_graph_reliability(0,0)` yields a vacuous HIGH; check must NOT
        // read that as PASS ("PASS: No in-scope calls measured"). Honest = Incomplete (unknown).
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::High); // vacuous band
        input.resolved_calls = 0;
        input.unresolved_calls_internal_like = 0;
        let result = check(&input);
        let cg = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .unwrap();
        assert_eq!(
            cg.status,
            ConditionStatus::Incomplete,
            "0-of-0 must be Incomplete, not Pass: {}",
            cg.summary
        );
        assert!(
            cg.summary.contains("No in-scope calls"),
            "honest no-measurement summary: {}",
            cg.summary
        );
        // Unknown wins the overall verdict — never a green Pass over "no data".
        assert_eq!(result.verdict, CheckVerdict::Incomplete);
    }

    #[test]
    fn zero_in_scope_calls_still_renders_full_coverage_map() {
        // iteration-5 §1: an all-external repo has NO in-scope calls to grade (Incomplete), but
        // check must NOT drop the coverage map — the external share + named targets + heuristic
        // basis still render (the same FULL projection orient/trust show), so the reader keeps
        // its orientation even when there is nothing to grade.
        use crate::reliability::ExternalTarget;
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::High); // vacuous 0-of-0 band
        input.resolved_calls = 0;
        input.unresolved_calls_internal_like = 0;
        input.unresolved_calls = 50; // all external: external = 50 - 0 = 50; total = 50
        input.unresolved_calls_unknown = 0;
        input.external_targets = vec![ExternalTarget {
            type_name: "Buffer".into(),
            count: 40,
        }];
        let cg = check(&input)
            .conditions
            .into_iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .expect("CALL_GRAPH_RELIABILITY present");
        // Incomplete — the in-scope rate is unknown, never a Pass/Fail over "no data".
        assert_eq!(cg.status, ConditionStatus::Incomplete, "{}", cg.summary);
        // The honest no-measurement phrase (shared vocabulary), NOT silence.
        assert!(
            cg.summary.contains("No in-scope calls measured"),
            "shared no-measurement phrase: {}",
            cg.summary
        );
        // The external share as reader context (100% here — every call is external).
        assert!(
            cg.summary
                .contains("100% of calls go into external libraries"),
            "external share still reaches check with zero in-scope: {}",
            cg.summary
        );
        // The named coverage map + its heuristic basis.
        assert!(
            cg.summary
                .contains("External coverage (heuristic): `Buffer` (40)"),
            "named target still reaches check with zero in-scope: {}",
            cg.summary
        );
        assert!(
            cg.summary.contains("not compiler-verified"),
            "heuristic basis text present: {}",
            cg.summary
        );
        // No in-scope denominator → no conservative caveat, no fabricated rate.
        assert!(
            !cg.summary.contains("unclassified"),
            "no conservative caveat when there is no in-scope rate: {}",
            cg.summary
        );
        assert!(
            !cg.summary.contains("resolved (HIGH)"),
            "the vacuous band never rides a no-calls line: {}",
            cg.summary
        );
    }

    // ── 2. one_fail_stale_files ─────────────────────────────

    #[test]
    fn one_fail_stale_files() {
        let mut input = all_pass_input();
        input.stale_file_count = 5;
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Fail);
    }

    // ── 3. one_incomplete_no_snapshot ────────────────────────

    #[test]
    fn one_incomplete_no_snapshot() {
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
            index_drift: None,
        };
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Incomplete);
    }

    // ── 4. fail_plus_incomplete ─────────────────────────────

    #[test]
    fn fail_plus_incomplete() {
        let mut input = all_pass_input();
        input.stale_file_count = 2; // Fail
        input.gate_outcome = Some(GateOutcomeForCheck::Incomplete); // Incomplete
        let result = check(&input);
        // Incomplete wins over Fail.
        assert_eq!(result.verdict, CheckVerdict::Incomplete);
    }

    // ── 5. medium_call_graph_with_everything_else_pass ──────

    #[test]
    fn medium_call_graph_with_everything_else_pass() {
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Medium);
        let result = check(&input);
        // MEDIUM call-graph is advisory → pass.
        assert_eq!(result.verdict, CheckVerdict::Pass);
        let cg = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .unwrap();
        assert_eq!(cg.status, ConditionStatus::Pass);
        assert!(
            cg.summary.contains("MEDIUM"),
            "Expected summary to mention MEDIUM, got: {}",
            cg.summary,
        );
    }

    // ── 6. gate_not_configured_with_everything_else_pass ────

    #[test]
    fn gate_not_configured_with_everything_else_pass() {
        let mut input = all_pass_input();
        input.gate_outcome = Some(GateOutcomeForCheck::NotConfigured);
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Pass);
    }

    // ── 7. enrichment_not_run ───────────────────────────────

    #[test]
    fn enrichment_not_run() {
        let mut input = all_pass_input();
        input.enrichment_state = Some(EnrichmentState::NotRun);
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Fail);
    }

    // ── 10. stale_files_present ─────────────────────────────

    #[test]
    fn stale_files_present() {
        let mut input = all_pass_input();
        input.stale_file_count = 3;
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Fail);
        let sf = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::StaleFiles)
            .unwrap();
        assert_eq!(sf.status, ConditionStatus::Fail);
        assert!(
            sf.summary.contains("3"),
            "Expected summary to mention count 3, got: {}",
            sf.summary,
        );
    }

    // ── 11. empty_snapshot ──────────────────────────────────

    #[test]
    fn empty_snapshot() {
        let mut input = all_pass_input();
        input.files_total = 0;
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Incomplete);
        let idx = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::IndexNotEmpty)
            .unwrap();
        assert_eq!(idx.status, ConditionStatus::Incomplete);
    }

    // ── 12. enrichment_not_applicable_is_pass ───────────────

    #[test]
    fn enrichment_not_applicable_is_pass() {
        let mut input = all_pass_input();
        input.enrichment_state = Some(EnrichmentState::NotApplicable);
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Pass);
        let en = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::EnrichmentState)
            .unwrap();
        assert_eq!(en.status, ConditionStatus::Pass);
    }

    // ── 13. enrichment_ran_is_pass ──────────────────────────

    #[test]
    fn enrichment_ran_is_pass() {
        let mut input = all_pass_input();
        input.enrichment_state = Some(EnrichmentState::Ran);
        let result = check(&input);
        let en = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::EnrichmentState)
            .unwrap();
        assert_eq!(en.status, ConditionStatus::Pass);
        assert_eq!(result.verdict, CheckVerdict::Pass);
    }

    // ── 14. gate_fail_causes_check_fail ─────────────────────

    #[test]
    fn gate_fail_causes_check_fail() {
        let mut input = all_pass_input();
        input.gate_outcome = Some(GateOutcomeForCheck::Fail);
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Fail);
        let gs = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::GateStatus)
            .unwrap();
        assert_eq!(gs.status, ConditionStatus::Fail);
    }

    // ── 15. gate_incomplete_causes_check_incomplete ─────────

    #[test]
    fn gate_incomplete_causes_check_incomplete() {
        let mut input = all_pass_input();
        input.gate_outcome = Some(GateOutcomeForCheck::Incomplete);
        let result = check(&input);
        assert_eq!(result.verdict, CheckVerdict::Incomplete);
        let gs = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::GateStatus)
            .unwrap();
        assert_eq!(gs.status, ConditionStatus::Incomplete);
    }

    // ── 16. condition_codes_serialize_screaming_snake ────────

    #[test]
    fn condition_codes_serialize_screaming_snake() {
        assert_eq!(ConditionCode::SnapshotExists.as_str(), "SNAPSHOT_EXISTS");
        assert_eq!(ConditionCode::IndexNotEmpty.as_str(), "INDEX_NOT_EMPTY");
        assert_eq!(ConditionCode::StaleFiles.as_str(), "STALE_FILES");
        assert_eq!(
            ConditionCode::CallGraphReliability.as_str(),
            "CALL_GRAPH_RELIABILITY"
        );
        assert_eq!(ConditionCode::EnrichmentState.as_str(), "ENRICHMENT_STATE");
        assert_eq!(ConditionCode::GateStatus.as_str(), "GATE_STATUS");
    }

    // ── 17. no_snapshot_only_evaluates_snapshot_exists ───────

    #[test]
    fn no_snapshot_only_evaluates_snapshot_exists() {
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
            index_drift: None,
        };
        let result = check(&input);
        assert_eq!(result.conditions.len(), 1);
        assert_eq!(result.conditions[0].code, ConditionCode::SnapshotExists);
        assert_eq!(result.conditions[0].status, ConditionStatus::Incomplete);
    }

    // ── 18. all_conditions_present_when_snapshot_exists ──────

    #[test]
    fn all_conditions_present_when_snapshot_exists() {
        // INDEX-BASIS-1: 8 conditions when snapshot exists AND drift is supplied —
        // UNPARSED_FILES (new) + STALE_FILES (deprecated alias) + INDEX_DRIFT added.
        let result = check(&all_pass_input());
        assert_eq!(
            result.conditions.len(),
            8,
            "Expected 8 conditions when snapshot exists, got {}",
            result.conditions.len(),
        );
        let codes: Vec<ConditionCode> = result.conditions.iter().map(|c| c.code).collect();
        assert_eq!(
            codes,
            vec![
                ConditionCode::SnapshotExists,
                ConditionCode::IndexNotEmpty,
                ConditionCode::UnparsedFiles,
                ConditionCode::StaleFiles,
                ConditionCode::IndexDrift,
                ConditionCode::CallGraphReliability,
                ConditionCode::EnrichmentState,
                ConditionCode::GateStatus,
            ],
        );
    }

    // ── INDEX-BASIS-1: parse-status rename + drift condition ─────

    #[test]
    fn unparsed_files_replaces_stale_and_keeps_deprecated_alias() {
        // The honest name UNPARSED_FILES carries the parse status; STALE_FILES is
        // emitted alongside (same status) with a deprecation note, for one release.
        let mut input = all_pass_input();
        input.stale_file_count = 3;
        let result = check(&input);
        let unparsed = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::UnparsedFiles)
            .expect("UNPARSED_FILES present");
        assert_eq!(unparsed.status, ConditionStatus::Fail);
        assert_eq!(unparsed.summary, "3 files could not be parsed.");

        let stale = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::StaleFiles)
            .expect("deprecated STALE_FILES still emitted");
        assert_eq!(
            stale.status,
            ConditionStatus::Fail,
            "same status as canonical"
        );
        assert!(
            stale
                .summary
                .starts_with("[deprecated: renamed UNPARSED_FILES]"),
            "carries deprecation note: {}",
            stale.summary
        );
        // Duplicate condition does not change the verdict (both Fail → still Fail).
        assert_eq!(result.verdict, CheckVerdict::Fail);
    }

    #[test]
    fn index_drift_incomplete_when_drifted() {
        use crate::dto::index_drift::IndexDrift;
        let mut input = all_pass_input();
        input.index_drift = Some(IndexDrift::Drifted {
            basis: "abcdef0123456789".to_string(),
            commits_ahead: 1,
            files_changed: 3,
            indexed_changed: 3,
            modules: vec!["src".to_string()],
        });
        let result = check(&input);
        let drift = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::IndexDrift)
            .expect("INDEX_DRIFT present");
        assert_eq!(drift.status, ConditionStatus::Incomplete);
        assert!(
            drift.summary.contains("1 commit ahead"),
            "{}",
            drift.summary
        );
        // Incomplete drift makes the whole verdict Incomplete (never Fail alone).
        assert_eq!(result.verdict, CheckVerdict::Incomplete);
    }

    #[test]
    fn index_drift_pass_when_clean() {
        let result = check(&all_pass_input()); // all_pass_input carries Clean drift
        let drift = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::IndexDrift)
            .expect("INDEX_DRIFT present");
        assert_eq!(drift.status, ConditionStatus::Pass);
        assert_eq!(result.verdict, CheckVerdict::Pass);
    }

    #[test]
    fn index_drift_incomplete_when_basis_unknown() {
        use crate::dto::index_drift::IndexDrift;
        let mut input = all_pass_input();
        input.index_drift = Some(IndexDrift::BasisUnknown);
        let result = check(&input);
        let drift = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::IndexDrift)
            .unwrap();
        assert_eq!(drift.status, ConditionStatus::Incomplete);
        assert!(drift.summary.contains("indexed before basis tracking"));
        assert_eq!(result.verdict, CheckVerdict::Incomplete);
    }

    #[test]
    fn index_drift_pass_when_not_git() {
        use crate::dto::index_drift::IndexDrift;
        let mut input = all_pass_input();
        input.index_drift = Some(IndexDrift::NotGit);
        let result = check(&input);
        let drift = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::IndexDrift)
            .unwrap();
        assert_eq!(
            drift.status,
            ConditionStatus::Pass,
            "not-a-git-repo is not-applicable → Pass, not a degradation"
        );
        assert_eq!(result.verdict, CheckVerdict::Pass);
    }

    #[test]
    fn index_drift_omitted_when_not_supplied() {
        // The simple `run_check` path supplies no drift → the condition is omitted,
        // not fabricated as a false "unknown". (7 conditions: no INDEX_DRIFT.)
        let mut input = all_pass_input();
        input.index_drift = None;
        let result = check(&input);
        assert!(
            !result
                .conditions
                .iter()
                .any(|c| c.code == ConditionCode::IndexDrift),
            "INDEX_DRIFT omitted when drift not supplied"
        );
        assert_eq!(result.conditions.len(), 7);
        assert_eq!(result.verdict, CheckVerdict::Pass);
    }
}
