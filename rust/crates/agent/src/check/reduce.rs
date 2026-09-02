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
    use crate::dto::ceiling_fact::CeilingFact;
    use crate::storage_port::{AgentReliabilityLevel, EnrichmentState};

    /// CHECK-SIGNAL-1 test helper: an affirmative permanent-ceiling fact naming one language.
    fn ceiling(lang: &str) -> Option<CeilingFact> {
        Some(CeilingFact::Ceiling {
            languages: vec![lang.to_string()],
        })
    }

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
            // CHECK-SIGNAL-1: default not-a-ceiling (actionable) → pre-slice call-graph behavior.
            ceiling_fact: None,
            // CHECK-LANG-SPLIT-1: no daemon-computed breakdown in the pure-reducer tests.
            reliability_by_language: None,
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

    // ── CHECK-LANG-SPLIT-1 (§2 + ruling A): the per-language breakdown rides any blended figure ──

    #[test]
    fn per_language_breakdown_appends_under_the_low_call_graph_figure() {
        // A daemon-computed breakdown line for a MIXED repo renders UNDER the blended figure on a LOW
        // verdict (which language carries the unresolved mass), sentence-cased + period-terminated, AFTER
        // the figure.
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        input.resolved_calls = 42;
        input.unresolved_calls_internal_like = 58;
        input.unresolved_calls = 58;
        input.reliability_by_language =
            Some("by language: TypeScript 24% of 100 calls · Java 11% of 113 calls".to_string());
        let cg = check(&input)
            .conditions
            .into_iter()
            .find(|c| matches!(c.code, ConditionCode::CallGraphReliability))
            .unwrap();
        assert!(
            cg.summary
                .contains("By language: TypeScript 24% of 100 calls · Java 11% of 113 calls."),
            "breakdown appended under the figure, sentence-cased + period: {}",
            cg.summary
        );
        // It follows the blended figure, not precedes it.
        assert!(
            cg.summary.find("42% resolved (LOW)").unwrap()
                < cg.summary.find("By language:").unwrap()
        );
    }

    #[test]
    fn per_language_breakdown_renders_on_every_band_with_a_call_figure() {
        // Operator ruling A (`leveldb-breakdown-contract`, 2026-09-02): the split is a UNIFORM materiality
        // gate — it renders wherever there IS a blended call-resolution figure to decompose, on ANY band
        // (HIGH / MEDIUM / LOW), NOT only LOW. review-0 §3: a mixed MEDIUM repo's blended figure is still
        // rendered, so suppressing its breakdown would re-hide the per-language confidence difference (§1).
        for band in [
            AgentReliabilityLevel::Low,
            AgentReliabilityLevel::Medium,
            AgentReliabilityLevel::High,
        ] {
            let mut input = all_pass_input(); // resolved 95 / total 100 → resolution present on every band
            input.call_graph_reliability = Some(band);
            input.reliability_by_language = Some(
                "by language: TypeScript 90% of 100 calls · Java 40% of 113 calls".to_string(),
            );
            let cg = check(&input)
                .conditions
                .into_iter()
                .find(|c| matches!(c.code, ConditionCode::CallGraphReliability))
                .unwrap();
            assert!(
                cg.summary
                    .contains("By language: TypeScript 90% of 100 calls · Java 40% of 113 calls."),
                "breakdown renders on {band:?} (a figure is present to split): {}",
                cg.summary
            );
        }
    }

    #[test]
    fn per_language_breakdown_silent_when_no_call_figure() {
        // No in-scope calls to resolve → `resolution` is None → no blended figure to decompose, so no
        // breakdown even when the daemon supplied a line (each cell would be a vacuous "unknown"). This is
        // the ONLY suppression that survives ruling A (it is "no figure", not a band gate).
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        input.resolved_calls = 0;
        input.unresolved_calls_internal_like = 0;
        input.unresolved_calls = 0; // total_calls == 0 → resolution None
        input.reliability_by_language =
            Some("by language: TypeScript no in-scope calls measured".to_string());
        let cg = check(&input)
            .conditions
            .into_iter()
            .find(|c| matches!(c.code, ConditionCode::CallGraphReliability))
            .unwrap();
        assert!(
            !cg.summary.to_lowercase().contains("by language"),
            "no figure to split → no breakdown: {}",
            cg.summary
        );
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

    // ── CHECK-SIGNAL-1: the four cells (ceiling / actionable / mixed / in-flight) ──

    /// CELL "no-path only" (leveldb / django shape): a PERMANENT-ceiling repo whose only degrading
    /// signals are the no-resolver call-graph + "did not run" enrichment → both reclassify to
    /// PASSING stated limitations, and the verdict MOVES to Pass (the intended discrimination).
    #[test]
    fn ceiling_repo_low_call_graph_and_not_run_enrichment_pass_and_verdict_moves() {
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        input.resolved_calls = 42;
        input.unresolved_calls_internal_like = 58; // 42% in-scope
        input.unresolved_calls = 58; // external = 0
        input.enrichment_state = Some(EnrichmentState::NotRun);
        input.ceiling_fact = ceiling("C++");
        let result = check(&input);

        let cg = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .unwrap();
        assert_eq!(cg.status, ConditionStatus::Pass, "{}", cg.summary);
        assert!(
            cg.ceiling,
            "call-graph condition carries the ceiling marker"
        );
        // FIGURES UNCHANGED (§2.1 / §3): the deterministic-extraction rate still renders verbatim.
        assert!(cg.summary.contains("42% resolved"), "{}", cg.summary);
        assert!(
            cg.summary
                .contains("reached this build's ceiling for C++ (no resolver exists)"),
            "{}",
            cg.summary
        );

        let en = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::EnrichmentState)
            .unwrap();
        assert_eq!(en.status, ConditionStatus::Pass, "{}", en.summary);
        assert!(en.ceiling);
        assert!(
            en.summary
                .contains("No semantic-resolution path exists for C++ on this build"),
            "{}",
            en.summary
        );
        assert!(!en.summary.contains("did not run"), "{}", en.summary);

        // The intended movement: with the two former Fails now Pass (rest of all_pass_input passes),
        // the verdict is Pass. Mapping frozen; the movement is the point (§2.3).
        assert_eq!(result.verdict, CheckVerdict::Pass);
    }

    /// CELL "no-path only" with NO in-scope calls (a pure-external ceiling repo): still a PASSING
    /// stated limitation, using the honest no-measurement figure (never a fabricated rate), coverage
    /// map preserved.
    #[test]
    fn ceiling_repo_no_in_scope_calls_passes_with_ceiling_form() {
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::High); // vacuous 0-of-0 band
        input.resolved_calls = 0;
        input.unresolved_calls_internal_like = 0;
        input.unresolved_calls = 50; // all external
        input.ceiling_fact = ceiling("C");
        let cg = check(&input)
            .conditions
            .into_iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .unwrap();
        assert_eq!(cg.status, ConditionStatus::Pass, "{}", cg.summary);
        assert!(cg.ceiling);
        assert!(
            cg.summary
                .contains("no in-scope calls to resolve on this build"),
            "{}",
            cg.summary
        );
        // Coverage map still renders (figures unchanged) — the external share reaches the reader.
        assert!(
            cg.summary.contains("go into external libraries"),
            "{}",
            cg.summary
        );
    }

    /// CELL "enrichable only" / "mixed": NO ceiling fact (`None`) → the pre-slice degrading verdict
    /// is UNCHANGED, byte-identical — LOW call-graph stays Fail, "did not run" enrichment stays Fail,
    /// neither marked ceiling. This is the discrimination's other half.
    #[test]
    fn actionable_repo_keeps_degrading_verdict_unchanged() {
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        input.resolved_calls = 42;
        input.unresolved_calls_internal_like = 58;
        input.unresolved_calls = 58;
        input.enrichment_state = Some(EnrichmentState::NotRun);
        input.ceiling_fact = None; // enrichable / mixed → actionable
        let result = check(&input);

        let cg = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .unwrap();
        assert_eq!(cg.status, ConditionStatus::Fail);
        assert!(!cg.ceiling);
        assert!(cg
            .summary
            .contains("verify call/dead claims against source"));

        let en = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::EnrichmentState)
            .unwrap();
        assert_eq!(en.status, ConditionStatus::Fail);
        assert!(!en.ceiling);
        assert!(en.summary.contains("did not run"));

        assert_eq!(result.verdict, CheckVerdict::Fail);
    }

    /// CHECK-SIGNAL-1 (operator ruling `ceiling-read-unknown`): a FAILED capability read
    /// (`CeilingFact::Unknown`) on a DEGRADING call-graph condition may NEVER mint a Pass — it keeps
    /// the pre-slice FAILING classification (exactly as the no-fact/NoCeiling case) AND renders the
    /// unknown WITH its reason in-band (STANDING HONESTY RULE #1: a classified fallible read is never
    /// swallowed to a sentinel). This is the error path review-1 flagged as uncovered.
    #[test]
    fn unknown_capability_low_call_graph_stays_failing_and_surfaces_reason() {
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        input.resolved_calls = 42;
        input.unresolved_calls_internal_like = 58; // 42% in-scope
        input.unresolved_calls = 58; // external = 0
        input.ceiling_fact = Some(CeilingFact::Unknown {
            reason: "storage read failed: disk I/O error".to_string(),
        });
        let result = check(&input);

        let cg = result
            .conditions
            .iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .unwrap();
        // FAILING — a read failure never improves the verdict.
        assert_eq!(cg.status, ConditionStatus::Fail, "{}", cg.summary);
        assert!(
            !cg.ceiling,
            "Unknown is NOT the ceiling form: {}",
            cg.summary
        );
        // The pre-slice degrading verdict sentence is preserved verbatim …
        assert!(
            cg.summary
                .contains("Your code's calls 42% resolved (LOW) — verify call/dead claims"),
            "{}",
            cg.summary
        );
        // … and the unknown is surfaced WITH its reason (not swallowed to stderr).
        assert!(
            cg.summary
                .contains("Whether this is a permanent no-resolver ceiling is unknown"),
            "unknown rendered: {}",
            cg.summary
        );
        assert!(
            cg.summary.contains("disk I/O error"),
            "reason carried in-band: {}",
            cg.summary
        );
        // A read failure may never mint a Pass — the verdict stays Fail.
        assert_eq!(result.verdict, CheckVerdict::Fail);
    }

    /// CHECK-SIGNAL-1: on an UNKNOWN capability, a "did not run" ENRICHMENT_STATE keeps its pre-slice
    /// FAILING form (only `CeilingFact::Ceiling` reclassifies NotRun) — an Unknown never mints the
    /// non-failing "enrichment does not apply" Pass. The read failure is surfaced on the degrading
    /// CALL_GRAPH_RELIABILITY condition (the only site where the capability is material), so it is
    /// never swallowed.
    #[test]
    fn unknown_capability_not_run_enrichment_stays_failing() {
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        input.resolved_calls = 42;
        input.unresolved_calls_internal_like = 58;
        input.unresolved_calls = 58;
        input.enrichment_state = Some(EnrichmentState::NotRun);
        input.ceiling_fact = Some(CeilingFact::Unknown {
            reason: "language breakdown unavailable".to_string(),
        });
        let en = check(&input)
            .conditions
            .into_iter()
            .find(|c| c.code == ConditionCode::EnrichmentState)
            .unwrap();
        assert_eq!(en.status, ConditionStatus::Fail, "{}", en.summary);
        assert!(
            !en.ceiling,
            "Unknown never mints the enrichment ceiling form"
        );
        assert!(en.summary.contains("did not run"), "{}", en.summary);
    }

    /// CHECK-SIGNAL-1: an affirmative `CeilingFact::NoCeiling` is byte-identical to the not-supplied
    /// (`None`) path — both keep the pre-slice degrading classification. This pins the distinction
    /// that build-1's `Option<ResolutionCeiling>` could not express: NoCeiling ≠ Unknown ≠ Ceiling.
    #[test]
    fn no_ceiling_matches_not_supplied_degrading_verdict() {
        let mut base = all_pass_input();
        base.call_graph_reliability = Some(AgentReliabilityLevel::Low);
        base.resolved_calls = 42;
        base.unresolved_calls_internal_like = 58;
        base.unresolved_calls = 58;
        base.enrichment_state = Some(EnrichmentState::NotRun);

        let mut not_supplied = base.clone();
        not_supplied.ceiling_fact = None;
        let mut no_ceiling = base;
        no_ceiling.ceiling_fact = Some(CeilingFact::NoCeiling);

        // Same conditions (status + summary + marker) for both → the reducer treats an affirmative
        // no-ceiling exactly like "no analysis supplied": pre-slice, byte-identical.
        assert_eq!(
            check(&not_supplied).conditions,
            check(&no_ceiling).conditions
        );
        assert_eq!(check(&no_ceiling).verdict, CheckVerdict::Fail);
    }

    /// CELL "in-flight": an in-flight enrichment pass keeps OFC-1's honest non-failing form even on a
    /// ceiling repo — the ceiling override touches ONLY the `NotRun` case, never in-flight.
    #[test]
    fn in_flight_enrichment_keeps_its_form_on_ceiling_repo() {
        let mut input = all_pass_input();
        input.enrichment_state = Some(EnrichmentState::InFlight);
        input.ceiling_fact = ceiling("Python");
        let en = check(&input)
            .conditions
            .into_iter()
            .find(|c| c.code == ConditionCode::EnrichmentState)
            .unwrap();
        assert_eq!(en.status, ConditionStatus::Pass);
        assert!(!en.ceiling, "in-flight is not the ceiling form");
        assert!(en.summary.contains("in progress"), "{}", en.summary);
    }

    /// A ceiling repo whose call-graph is (implausibly) MEDIUM keeps the ordinary passing form — the
    /// ceiling override reclassifies ONLY the degrading (LOW / no-in-scope) case, never fabricating a
    /// ceiling sentence over an already-passing rate.
    #[test]
    fn ceiling_does_not_touch_medium_call_graph() {
        let mut input = all_pass_input();
        input.call_graph_reliability = Some(AgentReliabilityLevel::Medium);
        input.ceiling_fact = ceiling("C");
        let cg = check(&input)
            .conditions
            .into_iter()
            .find(|c| c.code == ConditionCode::CallGraphReliability)
            .unwrap();
        assert_eq!(cg.status, ConditionStatus::Pass);
        assert!(!cg.ceiling, "MEDIUM is already passing — not reclassified");
        assert!(cg.summary.contains("MEDIUM"), "{}", cg.summary);
        assert!(!cg.summary.contains("ceiling"), "{}", cg.summary);
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
            ceiling_fact: None,
            reliability_by_language: None,
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
            ceiling_fact: None,
            reliability_by_language: None,
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
            excluded: 0,
            unreadable: 0,
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
