//! Pure gate compute layer.
//!
//! `compute(input) -> GateReport` evaluates every obligation,
//! applies the waiver overlay, and reduces the verdict list to
//! a `GateOutcome`. No I/O, no storage, no clocks beyond the
//! caller-supplied `now` on the input.
//!
//! ── Waiver semantics (preserved from Rust-25) ────────────────
//!
//! Non-PASS computed verdicts with a matching active waiver
//! become `effective_verdict = WAIVED`. A PASS obligation stays
//! PASS regardless of waiver presence — no policy exception
//! occurred, so no transformation is performed. This diverges
//! intentionally from the TS prototype (which unconditionally
//! marks WAIVED when any matching waiver exists). The Rust
//! model is the corrected policy and must not be "fixed back"
//! to TS behavior during this relocation. See TECH-DEBT.md.
//!
//! ── Method coverage ─────────────────────────────────────────
//!
//! Supported: `arch_violations`, `coverage_threshold`,
//! `complexity_threshold`, `hotspot_threshold`,
//! `module_violations`. All other methods return UNSUPPORTED.
//! Adding a new method is a two-step change:
//!
//!   1. Extend `MethodEvidence` with a new variant.
//!   2. Handle the new variant inside `evaluate_obligation`.
//!
//! Both sites live in this file.

use std::collections::HashMap;

use crate::types::{
	EffectiveVerdict, GateAssessmentState, GateAssessmentVerdict, GateCounts,
	GateMode, GateObligation, GateOutcome, GateQualityAssessmentEvaluation,
	GateQualityAssessmentFact, GateQualityCounts, GateQualityPolicySeverity,
	GateReport, GateRequirement, GateWaiver, ObligationEvaluation, Verdict,
	WaiverBasis,
};

// ── Method-specific pre-fetched evidence ─────────────────────────

/// Evidence bundle for a single obligation, pre-fetched by
/// `assemble` based on the obligation's method. `compute`
/// dispatches on the variant and performs only numeric/logical
/// work — no storage reads.
#[derive(Debug, Clone, PartialEq)]
pub enum MethodEvidence {
	/// `arch_violations` evidence: list of
	/// `(forbidden_module, violation_edge_count)` pairs for
	/// each boundary declaration attached to the target
	/// module. Empty vec when the target has no boundary
	/// declarations → the compute layer emits
	/// MISSING_EVIDENCE.
	ArchViolations {
		target: String,
		per_boundary_counts: Vec<(String, usize)>,
		/// `true` iff the storage layer had boundary
		/// declarations for ANY source module, but none named
		/// this obligation's target. Needed to distinguish
		/// "no boundaries declared at all" from "boundaries
		/// declared but not for this target". The existing
		/// gate.rs treats both as MISSING_EVIDENCE with a
		/// single reason string — we preserve that behavior.
		has_any_boundary_for_target: bool,
	},

	/// `coverage_threshold` evidence: the full list of
	/// coverage measurements that match the obligation's
	/// target prefix. Compute parses `value_json`, averages the
	/// `value` field, and compares to the threshold.
	CoverageThreshold {
		target: String,
		matching_measurements: Vec<PolicyMeasurement>,
	},

	/// `complexity_threshold` evidence: the full list of
	/// complexity measurements that match the target prefix.
	/// Compute parses each row, finds the max, compares.
	ComplexityThreshold {
		target: String,
		matching_measurements: Vec<PolicyMeasurement>,
	},

	/// `hotspot_threshold` evidence: the full list of hotspot
	/// inferences. Target is optional (whole-repo scope when
	/// absent).
	HotspotThreshold {
		target: Option<String>,
		matching_inferences: Vec<PolicyMeasurement>,
	},

	/// `module_violations` evidence: pre-evaluated boundary
	/// violation counts from RS-MG-4. PASS if violations_count
	/// is zero, FAIL otherwise. Stale count is informational.
	///
	/// Always repo-wide. The `target` field on the obligation is
	/// ignored — `module_violations` evaluates ALL discovered-
	/// module boundaries. If scoped evaluation is needed, it
	/// requires a separate method with explicit design.
	ModuleViolations {
		violations_count: usize,
		stale_declarations_count: usize,
	},

	/// Obligation's `target` was absent where the method
	/// requires one. Compute maps this to MISSING_EVIDENCE.
	TargetMissing,

	/// Obligation's `threshold` was absent where the method
	/// requires one. Compute maps this to MISSING_EVIDENCE.
	ThresholdMissing,

	/// The method is not one of the four supported methods.
	/// Compute maps this to UNSUPPORTED.
	UnsupportedMethod,
}

/// Narrow projection of a measurement/inference row consumed
/// by compute.
///
/// Pre-parsed: the numeric `value` has already been extracted
/// from the underlying `value_json` column at the `assemble`
/// layer. Compute never touches JSON. This keeps compute
/// total over well-formed inputs and moves all structural
/// validation to the assembler, where malformed rows become
/// `GateError::MalformedEvidence` and propagate up to the
/// caller (CLI or agent aggregator).
///
/// Rationale for carrying `target_stable_key` even though
/// compute does not use it: existing CLI tests assert stable
/// diagnostic output that names the offending row on
/// malformed evidence. Those assertions still pass because
/// the assemble layer produces the error; compute receives
/// only validated rows. The field is kept here for future
/// uses (sorted evidence, per-row flagging) without widening
/// the type again.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyMeasurement {
	pub target_stable_key: String,
	pub value: f64,
}

// ── Gate input ───────────────────────────────────────────────────

/// Fully pre-fetched gate input. `compute` consumes this and
/// returns a `GateReport` without any I/O.
///
/// `method_evidence` is keyed by `(req_id, req_version,
/// obligation_id)`. A missing key is treated as an internal
/// error (the assembler should have inserted an entry for every
/// obligation). Missing keys become UNSUPPORTED at evaluation
/// time so compute stays total — no panics.
///
/// `matching_waivers` is similarly keyed. A missing key is
/// treated as "no waivers match", which is indistinguishable
/// from an empty vec for overlay purposes.
///
/// `quality_assessment_facts` contains one entry per active
/// quality-policy declaration. The storage adapter joins
/// declarations with assessment rows and produces enriched facts.
/// Missing assessments are represented with `assessment_state =
/// Missing`.
#[derive(Debug, Clone)]
pub struct GateInput {
	pub requirements: Vec<GateRequirement>,
	pub mode: GateMode,
	pub now: String,
	pub method_evidence: HashMap<ObligationKey, MethodEvidence>,
	pub matching_waivers: HashMap<ObligationKey, Vec<GateWaiver>>,
	/// Quality-policy assessment facts (one per active policy).
	pub quality_assessment_facts: Vec<GateQualityAssessmentFact>,
}

/// Composite key identifying one obligation. `i64` version
/// matches the storage column type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObligationKey {
	pub req_id: String,
	pub req_version: i64,
	pub obligation_id: String,
}

impl ObligationKey {
	pub fn new(req_id: &str, req_version: i64, obligation_id: &str) -> Self {
		Self {
			req_id: req_id.to_string(),
			req_version,
			obligation_id: obligation_id.to_string(),
		}
	}
}

// ── compute() ────────────────────────────────────────────────────

/// Pure evaluation of a gate input.
///
/// Iterates requirements → obligations, evaluates each using
/// the pre-fetched method evidence, applies waiver overlay, and
/// reduces the verdict list to a `GateOutcome` using the
/// input's `mode`. Returns a fully-populated `GateReport`.
///
/// Quality-policy assessments are processed separately and
/// contribute to the final outcome based on severity:
/// - Missing → incomplete (exit 2)
/// - NOT_COMPARABLE → incomplete (exit 2)
/// - FAIL + severity=Fail → fail (exit 1)
/// - FAIL + severity=Advisory → non-blocking (reported only)
pub fn compute(input: GateInput) -> GateReport {
	let mut evaluations: Vec<ObligationEvaluation> = Vec::new();

	for req in &input.requirements {
		for obl in &req.obligations {
			let key = ObligationKey::new(&req.req_id, req.version, &obl.obligation_id);
			let evidence = input
				.method_evidence
				.get(&key)
				.cloned()
				.unwrap_or(MethodEvidence::UnsupportedMethod);
			let waivers = input
				.matching_waivers
				.get(&key)
				.map(|v| v.as_slice())
				.unwrap_or(&[]);

			let mut eval = evaluate_obligation(req, obl, evidence);
			apply_waiver_overlay(&mut eval, waivers);
			evaluations.push(eval);
		}
	}

	// Convert quality-policy facts into evaluations.
	let quality_assessments: Vec<GateQualityAssessmentEvaluation> = input
		.quality_assessment_facts
		.iter()
		.map(GateQualityAssessmentEvaluation::from)
		.collect();

	let outcome = reduce_outcome(&evaluations, &input.quality_assessment_facts, input.mode);

	GateReport {
		obligations: evaluations,
		quality_assessments,
		outcome,
	}
}

// ── Obligation evaluation ────────────────────────────────────────

fn evaluate_obligation(
	req: &GateRequirement,
	obl: &GateObligation,
	evidence: MethodEvidence,
) -> ObligationEvaluation {
	let mut eval = ObligationEvaluation {
		req_id: req.req_id.clone(),
		req_version: req.version,
		obligation_id: obl.obligation_id.clone(),
		obligation: obl.obligation.clone(),
		method: obl.method.clone(),
		target: obl.target.clone(),
		threshold: obl.threshold,
		operator: obl.operator.clone(),
		computed_verdict: Verdict::UNSUPPORTED,
		effective_verdict: EffectiveVerdict::UNSUPPORTED,
		evidence: serde_json::json!({}),
		waiver_basis: None,
	};

	match evidence {
		MethodEvidence::TargetMissing | MethodEvidence::ThresholdMissing => {
			// The existing gate.rs used the same "target or
			// threshold not specified" reason text for both
			// target-missing and threshold-missing coverage/
			// complexity/hotspot cases. Preserved verbatim for
			// byte-stable output.
			eval.computed_verdict = Verdict::MISSING_EVIDENCE;
			eval.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
			eval.evidence = serde_json::json!({
				"reason": reason_for_missing_params(&obl.method),
			});
		}

		MethodEvidence::UnsupportedMethod => {
			eval.computed_verdict = Verdict::UNSUPPORTED;
			eval.effective_verdict = EffectiveVerdict::UNSUPPORTED;
			eval.evidence = serde_json::json!({
				"reason": format!("method \"{}\" not yet supported", obl.method),
			});
		}

		MethodEvidence::ArchViolations {
			target: _,
			per_boundary_counts,
			has_any_boundary_for_target,
		} => evaluate_arch_violations(
			&mut eval,
			per_boundary_counts,
			has_any_boundary_for_target,
		),

		MethodEvidence::CoverageThreshold { target: _, matching_measurements } => {
			let op = obl.operator.as_deref().unwrap_or(">=").to_string();
			let threshold = obl.threshold.unwrap_or(0.0);
			evaluate_coverage_threshold(
				&mut eval,
				matching_measurements,
				&op,
				threshold,
			);
		}

		MethodEvidence::ComplexityThreshold { target: _, matching_measurements } => {
			let op = obl.operator.as_deref().unwrap_or("<=").to_string();
			let threshold = obl.threshold.unwrap_or(0.0);
			evaluate_complexity_threshold(
				&mut eval,
				matching_measurements,
				&op,
				threshold,
			);
		}

		MethodEvidence::HotspotThreshold { target, matching_inferences } => {
			let op = obl.operator.as_deref().unwrap_or("<=").to_string();
			let threshold = obl.threshold.unwrap_or(0.0);
			evaluate_hotspot_threshold(
				&mut eval,
				matching_inferences,
				&op,
				threshold,
				target.is_some(),
			);
		}

		MethodEvidence::ModuleViolations {
			violations_count,
			stale_declarations_count,
		} => {
			evaluate_module_violations(&mut eval, violations_count, stale_declarations_count);
		}
	}

	eval
}

fn reason_for_missing_params(method: &str) -> String {
	// Hotspot_threshold uses "threshold not specified" for the
	// threshold-only case because its target is optional. All
	// other methods use "target or threshold not specified"
	// even when only one is missing, matching existing gate.rs
	// behavior.
	match method {
		"hotspot_threshold" => "threshold not specified".to_string(),
		_ => "target or threshold not specified".to_string(),
	}
}

// ── arch_violations ──────────────────────────────────────────────

fn evaluate_arch_violations(
	eval: &mut ObligationEvaluation,
	per_boundary_counts: Vec<(String, usize)>,
	has_any_boundary_for_target: bool,
) {
	if !has_any_boundary_for_target {
		eval.computed_verdict = Verdict::MISSING_EVIDENCE;
		eval.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
		eval.evidence = serde_json::json!({
			"reason": "no boundary declarations for target",
		});
		return;
	}

	let total_violations: usize =
		per_boundary_counts.iter().map(|(_, n)| *n).sum();

	let verdict = if total_violations == 0 {
		Verdict::PASS
	} else {
		Verdict::FAIL
	};
	eval.computed_verdict = verdict;
	eval.effective_verdict = verdict.into();
	eval.evidence = serde_json::json!({
		"violation_count": total_violations,
	});
}

// ── coverage_threshold ───────────────────────────────────────────

fn evaluate_coverage_threshold(
	eval: &mut ObligationEvaluation,
	matching: Vec<PolicyMeasurement>,
	op: &str,
	threshold: f64,
) {
	if matching.is_empty() {
		eval.computed_verdict = Verdict::MISSING_EVIDENCE;
		eval.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
		eval.evidence = serde_json::json!({
			"reason": "no coverage data for target path",
		});
		return;
	}

	let sum: f64 = matching.iter().map(|r| r.value).sum();
	let avg = sum / matching.len() as f64;
	let pass = compare_values(avg, op, threshold);
	let verdict = if pass { Verdict::PASS } else { Verdict::FAIL };
	eval.computed_verdict = verdict;
	eval.effective_verdict = verdict.into();
	eval.evidence = serde_json::json!({
		"avg_coverage": (avg * 10000.0).round() / 10000.0,
		"threshold": threshold,
		"operator": op,
		"files_measured": matching.len(),
	});
}

// ── complexity_threshold ─────────────────────────────────────────

fn evaluate_complexity_threshold(
	eval: &mut ObligationEvaluation,
	matching: Vec<PolicyMeasurement>,
	op: &str,
	threshold: f64,
) {
	if matching.is_empty() {
		eval.computed_verdict = Verdict::MISSING_EVIDENCE;
		eval.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
		eval.evidence = serde_json::json!({
			"reason": "no complexity data for target path",
		});
		return;
	}

	let max_cc = matching
		.iter()
		.map(|r| r.value)
		.fold(f64::NEG_INFINITY, f64::max);

	let pass = compare_values(max_cc, op, threshold);
	let verdict = if pass { Verdict::PASS } else { Verdict::FAIL };
	eval.computed_verdict = verdict;
	eval.effective_verdict = verdict.into();
	eval.evidence = serde_json::json!({
		"max_complexity": max_cc,
		"threshold": threshold,
		"operator": op,
		"functions_measured": matching.len(),
	});
}

// ── module_violations ─────────────────────────────────────────────

fn evaluate_module_violations(
	eval: &mut ObligationEvaluation,
	violations_count: usize,
	stale_declarations_count: usize,
) {
	let verdict = if violations_count == 0 {
		Verdict::PASS
	} else {
		Verdict::FAIL
	};
	eval.computed_verdict = verdict;
	eval.effective_verdict = verdict.into();
	eval.evidence = serde_json::json!({
		"violations_count": violations_count,
		"stale_declarations_count": stale_declarations_count,
	});
}

// ── hotspot_threshold ────────────────────────────────────────────

fn evaluate_hotspot_threshold(
	eval: &mut ObligationEvaluation,
	matching: Vec<PolicyMeasurement>,
	op: &str,
	threshold: f64,
	has_target: bool,
) {
	if matching.is_empty() {
		let reason = if has_target {
			"no hotspot data for target path"
		} else {
			"no hotspot data"
		};
		eval.computed_verdict = Verdict::MISSING_EVIDENCE;
		eval.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
		eval.evidence = serde_json::json!({ "reason": reason });
		return;
	}

	let max_hs = matching
		.iter()
		.map(|r| r.value)
		.fold(f64::NEG_INFINITY, f64::max);

	let pass = compare_values(max_hs, op, threshold);
	let verdict = if pass { Verdict::PASS } else { Verdict::FAIL };
	eval.computed_verdict = verdict;
	eval.effective_verdict = verdict.into();
	eval.evidence = serde_json::json!({
		"max_hotspot_score": max_hs,
		"threshold": threshold,
	});
}

// ── helpers ──────────────────────────────────────────────────────

fn compare_values(value: f64, op: &str, threshold: f64) -> bool {
	match op {
		">=" => value >= threshold,
		">" => value > threshold,
		"<=" => value <= threshold,
		"<" => value < threshold,
		"==" => (value - threshold).abs() < f64::EPSILON,
		_ => false,
	}
}

// ── Waiver overlay ───────────────────────────────────────────────

fn apply_waiver_overlay(eval: &mut ObligationEvaluation, waivers: &[GateWaiver]) {
	if eval.computed_verdict == Verdict::PASS {
		return;
	}
	if let Some(w) = waivers.first() {
		eval.effective_verdict = EffectiveVerdict::WAIVED;
		eval.waiver_basis = Some(WaiverBasis {
			waiver_uid: w.waiver_uid.clone(),
			reason: w.reason.clone(),
			created_at: w.created_at.clone(),
			created_by: w.created_by.clone(),
			expires_at: w.expires_at.clone(),
			rationale_category: w.rationale_category.clone(),
			policy_basis: w.policy_basis.clone(),
		});
	}
}

// ── Outcome reduction ────────────────────────────────────────────

fn reduce_outcome(
	evals: &[ObligationEvaluation],
	quality_facts: &[GateQualityAssessmentFact],
	mode: GateMode,
) -> GateOutcome {
	let mut counts = GateCounts {
		total: evals.len(),
		pass: 0,
		fail: 0,
		waived: 0,
		missing_evidence: 0,
		unsupported: 0,
	};

	for eval in evals {
		match eval.effective_verdict {
			EffectiveVerdict::PASS => counts.pass += 1,
			EffectiveVerdict::FAIL => counts.fail += 1,
			EffectiveVerdict::MISSING_EVIDENCE => counts.missing_evidence += 1,
			EffectiveVerdict::UNSUPPORTED => counts.unsupported += 1,
			EffectiveVerdict::WAIVED => counts.waived += 1,
		}
	}

	// Count quality-policy assessments.
	let mut quality_counts = GateQualityCounts {
		total: quality_facts.len(),
		..Default::default()
	};

	for fact in quality_facts {
		match fact.assessment_state {
			GateAssessmentState::Missing => {
				quality_counts.missing += 1;
			}
			GateAssessmentState::Present => {
				match fact.computed_verdict {
					Some(GateAssessmentVerdict::Pass) => {
						quality_counts.pass += 1;
					}
					Some(GateAssessmentVerdict::Fail) => {
						match fact.severity {
							GateQualityPolicySeverity::Fail => {
								quality_counts.fail += 1;
							}
							GateQualityPolicySeverity::Advisory => {
								quality_counts.advisory_fail += 1;
							}
						}
					}
					Some(GateAssessmentVerdict::NotApplicable) => {
						quality_counts.not_applicable += 1;
					}
					Some(GateAssessmentVerdict::NotComparable) => {
						quality_counts.not_comparable += 1;
					}
					None => {
						// Present state with no verdict is a storage inconsistency.
						// Treat as missing to surface the problem.
						quality_counts.missing += 1;
					}
				}
			}
		}
	}

	// Obligation-level signals.
	let has_obl_fail = counts.fail > 0;
	let has_obl_incomplete = counts.missing_evidence > 0 || counts.unsupported > 0;

	// Quality-level signals.
	// - missing: no assessment computed
	// - not_comparable: comparative policy without baseline
	// - fail: severity=Fail failures (gate-blocking)
	// Advisory failures do NOT contribute to blocking.
	let has_quality_fail = quality_counts.fail > 0;
	let has_quality_incomplete =
		quality_counts.missing > 0 || quality_counts.not_comparable > 0;

	let (outcome, exit_code) = match mode {
		GateMode::Default => {
			if has_obl_fail || has_quality_fail {
				("fail", 1)
			} else if has_obl_incomplete || has_quality_incomplete {
				("incomplete", 2)
			} else {
				("pass", 0)
			}
		}
		GateMode::Strict => {
			if has_obl_fail || has_obl_incomplete || has_quality_fail || has_quality_incomplete {
				("fail", 1)
			} else {
				("pass", 0)
			}
		}
		GateMode::Advisory => {
			if has_obl_fail || has_quality_fail {
				("fail", 1)
			} else {
				("pass", 0)
			}
		}
	};

	GateOutcome {
		outcome: outcome.to_string(),
		exit_code,
		mode: mode.as_str().to_string(),
		counts,
		quality_counts,
	}
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::GateObligation;

	fn req(req_id: &str, version: i64, obligations: Vec<GateObligation>) -> GateRequirement {
		GateRequirement {
			req_id: req_id.to_string(),
			version,
			obligations,
		}
	}

	fn obl(
		obligation_id: &str,
		method: &str,
		target: Option<&str>,
		threshold: Option<f64>,
	) -> GateObligation {
		GateObligation {
			obligation_id: obligation_id.to_string(),
			obligation: "test obligation".to_string(),
			method: method.to_string(),
			target: target.map(String::from),
			threshold,
			operator: None,
		}
	}

	fn key(req_id: &str, version: i64, obligation_id: &str) -> ObligationKey {
		ObligationKey::new(req_id, version, obligation_id)
	}

	fn empty_input(requirements: Vec<GateRequirement>, mode: GateMode) -> GateInput {
		GateInput {
			requirements,
			mode,
			now: "2026-04-15T00:00:00Z".to_string(),
			method_evidence: HashMap::new(),
			matching_waivers: HashMap::new(),
			quality_assessment_facts: Vec::new(),
		}
	}

	// ── arch_violations ──

	#[test]
	fn arch_violations_pass_with_zero_edges() {
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![("src/adapters".into(), 0)],
				has_any_boundary_for_target: true,
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
	}

	#[test]
	fn arch_violations_fail_when_edges_exist() {
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![
					("src/adapters".into(), 2),
					("src/cli".into(), 1),
				],
				has_any_boundary_for_target: true,
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::FAIL);
		assert_eq!(report.obligations[0].evidence["violation_count"], 3);
		assert_eq!(report.outcome.exit_code, 1);
	}

	#[test]
	fn arch_violations_missing_when_no_boundary_for_target() {
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![],
				has_any_boundary_for_target: false,
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::MISSING_EVIDENCE);
		assert_eq!(
			report.obligations[0].evidence["reason"],
			"no boundary declarations for target"
		);
	}

	// ── coverage_threshold ──

	fn measurement(key: &str, value: f64) -> PolicyMeasurement {
		PolicyMeasurement {
			target_stable_key: key.to_string(),
			value,
		}
	}

	#[test]
	fn coverage_threshold_pass_when_avg_meets_threshold() {
		let r = req("REQ-1", 1, vec![obl("o1", "coverage_threshold", Some("src/core"), Some(0.80))]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::CoverageThreshold {
				target: "src/core".into(),
				matching_measurements: vec![
					measurement("r1:src/core/a.rs:FILE", 0.90),
					measurement("r1:src/core/b.rs:FILE", 0.80),
				],
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
		assert_eq!(report.obligations[0].evidence["files_measured"], 2);
	}

	#[test]
	fn coverage_threshold_fail_when_avg_below_threshold() {
		let r = req("REQ-1", 1, vec![obl("o1", "coverage_threshold", Some("src/core"), Some(0.80))]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::CoverageThreshold {
				target: "src/core".into(),
				matching_measurements: vec![measurement("r1:src/core/a.rs:FILE", 0.50)],
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::FAIL);
	}

	#[test]
	fn coverage_threshold_missing_when_no_measurements() {
		let r = req("REQ-1", 1, vec![obl("o1", "coverage_threshold", Some("src/core"), Some(0.80))]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::CoverageThreshold {
				target: "src/core".into(),
				matching_measurements: vec![],
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::MISSING_EVIDENCE);
	}

	#[test]
	fn coverage_threshold_missing_when_target_missing() {
		let r = req("REQ-1", 1, vec![obl("o1", "coverage_threshold", None, Some(0.80))]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input
			.method_evidence
			.insert(key("REQ-1", 1, "o1"), MethodEvidence::TargetMissing);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::MISSING_EVIDENCE);
		assert_eq!(
			report.obligations[0].evidence["reason"],
			"target or threshold not specified"
		);
	}

	// Note: a compute-layer test for "malformed row" is no
	// longer possible by construction. `PolicyMeasurement`
	// carries a pre-parsed `f64`, and malformed value_json
	// cannot even be expressed in the compute input. Malformed
	// evidence is detected at the `assemble` layer where it
	// becomes `GateError::MalformedEvidence`. See
	// `assemble::tests` for that path.

	// ── complexity_threshold ──

	#[test]
	fn complexity_threshold_pass_when_max_leq_threshold() {
		let r = req("REQ-1", 1, vec![obl("o1", "complexity_threshold", Some("src/core"), Some(10.0))]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ComplexityThreshold {
				target: "src/core".into(),
				matching_measurements: vec![
					measurement("r1:src/core/a:SYMBOL:f", 5.0),
					measurement("r1:src/core/a:SYMBOL:g", 10.0),
				],
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
	}

	#[test]
	fn complexity_threshold_fail_when_max_above_threshold() {
		let r = req("REQ-1", 1, vec![obl("o1", "complexity_threshold", Some("src/core"), Some(10.0))]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ComplexityThreshold {
				target: "src/core".into(),
				matching_measurements: vec![measurement("r1:src/core/a:SYMBOL:f", 15.0)],
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::FAIL);
	}

	// ── hotspot_threshold ──

	#[test]
	fn hotspot_threshold_pass_with_no_target() {
		let r = req("REQ-1", 1, vec![obl("o1", "hotspot_threshold", None, Some(0.80))]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::HotspotThreshold {
				target: None,
				matching_inferences: vec![measurement("r1:src/a.rs:FILE", 0.5)],
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
	}

	#[test]
	fn hotspot_threshold_missing_when_threshold_missing() {
		let r = req("REQ-1", 1, vec![obl("o1", "hotspot_threshold", None, None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input
			.method_evidence
			.insert(key("REQ-1", 1, "o1"), MethodEvidence::ThresholdMissing);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::MISSING_EVIDENCE);
		// Hotspot's missing-threshold reason is distinct from
		// coverage/complexity's combined reason string.
		assert_eq!(
			report.obligations[0].evidence["reason"],
			"threshold not specified"
		);
	}

	// ── unsupported method ──

	#[test]
	fn unsupported_method_produces_unsupported_verdict() {
		let r = req("REQ-1", 1, vec![obl("o1", "bogus_method", None, None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input
			.method_evidence
			.insert(key("REQ-1", 1, "o1"), MethodEvidence::UnsupportedMethod);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::UNSUPPORTED);
		assert_eq!(
			report.obligations[0].evidence["reason"],
			"method \"bogus_method\" not yet supported"
		);
	}

	// ── waiver overlay ──

	#[test]
	fn waiver_suppresses_fail_verdict() {
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![("src/adapters".into(), 5)],
				has_any_boundary_for_target: true,
			},
		);
		input.matching_waivers.insert(
			key("REQ-1", 1, "o1"),
			vec![GateWaiver {
				waiver_uid: "w1".into(),
				reason: "temporarily accepted".into(),
				created_at: "2026-04-14T00:00:00Z".into(),
				created_by: Some("security-team".into()),
				expires_at: Some("2026-06-01T00:00:00Z".into()),
				rationale_category: Some("legacy".into()),
				policy_basis: Some("ADR-42".into()),
			}],
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::FAIL);
		assert_eq!(report.obligations[0].effective_verdict, EffectiveVerdict::WAIVED);
		assert!(report.obligations[0].waiver_basis.is_some());
		// Default-mode reduction: WAIVED is non-failing.
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
		assert_eq!(report.outcome.counts.waived, 1);
		assert_eq!(report.outcome.counts.fail, 0);
	}

	#[test]
	fn pass_obligation_stays_pass_even_with_matching_waiver() {
		// Rust-25 preserved divergence.
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![("src/adapters".into(), 0)],
				has_any_boundary_for_target: true,
			},
		);
		input.matching_waivers.insert(
			key("REQ-1", 1, "o1"),
			vec![GateWaiver {
				waiver_uid: "w1".into(),
				reason: "should not apply".into(),
				created_at: "2026-04-14T00:00:00Z".into(),
				created_by: None,
				expires_at: None,
				rationale_category: None,
				policy_basis: None,
			}],
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
		assert_eq!(report.obligations[0].effective_verdict, EffectiveVerdict::PASS);
		assert!(report.obligations[0].waiver_basis.is_none());
	}

	// ── mode reduction ──

	#[test]
	fn default_mode_incomplete_exit_2() {
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![],
				has_any_boundary_for_target: false,
			},
		);
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "incomplete");
		assert_eq!(report.outcome.exit_code, 2);
	}

	#[test]
	fn strict_mode_promotes_missing_to_fail_exit_1() {
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Strict);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![],
				has_any_boundary_for_target: false,
			},
		);
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "fail");
		assert_eq!(report.outcome.exit_code, 1);
	}

	#[test]
	fn advisory_mode_ignores_missing_exit_0() {
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Advisory);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![],
				has_any_boundary_for_target: false,
			},
		);
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
	}

	#[test]
	fn empty_requirements_default_mode_is_pass_exit_0() {
		let input = empty_input(vec![], GateMode::Default);
		let report = compute(input);
		assert!(report.obligations.is_empty());
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
		assert_eq!(report.outcome.counts.total, 0);
	}

	// ── module_violations ──

	#[test]
	fn module_violations_pass_with_zero_violations() {
		let r = req("REQ-1", 1, vec![obl("o1", "module_violations", None, None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ModuleViolations {
				violations_count: 0,
				stale_declarations_count: 0,
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
	}

	#[test]
	fn module_violations_fail_when_violations_exist() {
		let r = req("REQ-1", 1, vec![obl("o1", "module_violations", None, None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ModuleViolations {
				violations_count: 3,
				stale_declarations_count: 1,
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::FAIL);
		assert_eq!(report.obligations[0].evidence["violations_count"], 3);
		assert_eq!(report.obligations[0].evidence["stale_declarations_count"], 1);
		assert_eq!(report.outcome.exit_code, 1);
	}

	#[test]
	fn module_violations_pass_with_only_stale_declarations() {
		// Stale declarations alone do not cause FAIL.
		let r = req("REQ-1", 1, vec![obl("o1", "module_violations", None, None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ModuleViolations {
				violations_count: 0,
				stale_declarations_count: 5,
			},
		);
		let report = compute(input);
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
		assert_eq!(report.obligations[0].evidence["stale_declarations_count"], 5);
		assert_eq!(report.outcome.outcome, "pass");
	}

	#[test]
	fn module_violations_ignores_target_field_on_obligation() {
		// module_violations is repo-wide. Even if the obligation
		// has a target field, it is ignored — the storage adapter
		// evaluates all discovered-module boundaries.
		let r = req("REQ-1", 1, vec![obl("o1", "module_violations", Some("packages/app"), None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ModuleViolations {
				violations_count: 0,
				stale_declarations_count: 0,
			},
		);
		let report = compute(input);
		// Target is preserved in the evaluation output for
		// transparency, but it does not affect the verdict.
		assert_eq!(report.obligations[0].target, Some("packages/app".into()));
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
	}

	// ── quality-policy assessment tests ──

	fn quality_fact(
		policy_id: &str,
		state: GateAssessmentState,
		verdict: Option<GateAssessmentVerdict>,
		severity: GateQualityPolicySeverity,
	) -> GateQualityAssessmentFact {
		GateQualityAssessmentFact {
			policy_uid: format!("uid-{policy_id}"),
			policy_id: policy_id.to_string(),
			policy_version: 1,
			policy_kind: crate::types::GateQualityPolicyKind::AbsoluteMax,
			severity,
			assessment_state: state,
			computed_verdict: verdict,
			baseline_snapshot_uid: None,
			measurements_evaluated: Some(10),
			violations_count: if verdict == Some(GateAssessmentVerdict::Fail) {
				Some(3)
			} else {
				Some(0)
			},
		}
	}

	#[test]
	fn quality_assessment_pass_contributes_to_pass_outcome() {
		let mut input = empty_input(vec![], GateMode::Default);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Present,
			Some(GateAssessmentVerdict::Pass),
			GateQualityPolicySeverity::Fail,
		)];
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
		assert_eq!(report.outcome.quality_counts.total, 1);
		assert_eq!(report.outcome.quality_counts.pass, 1);
		assert_eq!(report.quality_assessments.len(), 1);
		assert_eq!(report.quality_assessments[0].policy_id, "QP-001");
	}

	#[test]
	fn quality_assessment_fail_blocking_causes_fail_exit_1() {
		let mut input = empty_input(vec![], GateMode::Default);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Present,
			Some(GateAssessmentVerdict::Fail),
			GateQualityPolicySeverity::Fail,
		)];
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "fail");
		assert_eq!(report.outcome.exit_code, 1);
		assert_eq!(report.outcome.quality_counts.fail, 1);
		assert_eq!(report.outcome.quality_counts.advisory_fail, 0);
	}

	#[test]
	fn quality_assessment_fail_advisory_does_not_block() {
		let mut input = empty_input(vec![], GateMode::Default);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Present,
			Some(GateAssessmentVerdict::Fail),
			GateQualityPolicySeverity::Advisory,
		)];
		let report = compute(input);
		// Advisory failures are reported but do not block gate.
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
		assert_eq!(report.outcome.quality_counts.fail, 0);
		assert_eq!(report.outcome.quality_counts.advisory_fail, 1);
	}

	#[test]
	fn quality_assessment_missing_causes_incomplete_exit_2() {
		let mut input = empty_input(vec![], GateMode::Default);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Missing,
			None,
			GateQualityPolicySeverity::Fail,
		)];
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "incomplete");
		assert_eq!(report.outcome.exit_code, 2);
		assert_eq!(report.outcome.quality_counts.missing, 1);
	}

	#[test]
	fn quality_assessment_not_comparable_causes_incomplete_exit_2() {
		let mut input = empty_input(vec![], GateMode::Default);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Present,
			Some(GateAssessmentVerdict::NotComparable),
			GateQualityPolicySeverity::Fail,
		)];
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "incomplete");
		assert_eq!(report.outcome.exit_code, 2);
		assert_eq!(report.outcome.quality_counts.not_comparable, 1);
	}

	#[test]
	fn quality_assessment_not_applicable_is_non_blocking() {
		let mut input = empty_input(vec![], GateMode::Default);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Present,
			Some(GateAssessmentVerdict::NotApplicable),
			GateQualityPolicySeverity::Fail,
		)];
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
		assert_eq!(report.outcome.quality_counts.not_applicable, 1);
	}

	#[test]
	fn quality_fail_takes_precedence_over_obligation_missing() {
		// If both obligations have missing evidence (exit 2) AND
		// quality has blocking fail (exit 1), fail wins.
		let r = req("REQ-1", 1, vec![obl("o1", "arch_violations", Some("src/core"), None)]);
		let mut input = empty_input(vec![r], GateMode::Default);
		input.method_evidence.insert(
			key("REQ-1", 1, "o1"),
			MethodEvidence::ArchViolations {
				target: "src/core".into(),
				per_boundary_counts: vec![],
				has_any_boundary_for_target: false,
			},
		);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Present,
			Some(GateAssessmentVerdict::Fail),
			GateQualityPolicySeverity::Fail,
		)];
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "fail");
		assert_eq!(report.outcome.exit_code, 1);
	}

	#[test]
	fn strict_mode_promotes_quality_incomplete_to_fail() {
		let mut input = empty_input(vec![], GateMode::Strict);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Missing,
			None,
			GateQualityPolicySeverity::Fail,
		)];
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "fail");
		assert_eq!(report.outcome.exit_code, 1);
	}

	#[test]
	fn advisory_mode_ignores_quality_incomplete() {
		let mut input = empty_input(vec![], GateMode::Advisory);
		input.quality_assessment_facts = vec![quality_fact(
			"QP-001",
			GateAssessmentState::Missing,
			None,
			GateQualityPolicySeverity::Fail,
		)];
		let report = compute(input);
		assert_eq!(report.outcome.outcome, "pass");
		assert_eq!(report.outcome.exit_code, 0);
	}
}
