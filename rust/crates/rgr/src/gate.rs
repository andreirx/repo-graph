//! Gate evaluation support for `rgr-rust gate`.
//!
//! Evaluates requirement obligations against the current snapshot
//! and layers waiver overlay for governance policy.
//!
//! Supported methods: `arch_violations`, `coverage_threshold`,
//! `complexity_threshold`, `hotspot_threshold`.
//! All other methods return UNSUPPORTED.
//!
//! This module owns:
//!   - Obligation evaluation (method dispatch)
//!   - Waiver overlay (effective verdict resolution)
//!   - Gate reduction (verdict list → outcome + exit code)
//!   - Output DTOs (TS-compatible gate JSON shape)
//!
//! Waiver semantics (Rust-25, deliberate divergence from TS):
//!   - Waivers only suppress non-PASS computed verdicts.
//!   - A PASS obligation with a matching waiver remains PASS with
//!     waiver_basis = null. The waiver does not transform a passing
//!     obligation into WAIVED because no policy exception occurred.
//!   - This differs from the TS prototype which unconditionally sets
//!     effective_verdict = WAIVED when any matching waiver exists,
//!     regardless of computed verdict. The Rust model is the
//!     corrected policy model (documented in TECH-DEBT.md).
//!
//! main.rs stays a wiring layer: parse args, open storage, call
//! this module, serialize output, set exit code.

use serde::Serialize;
use repo_graph_storage::StorageConnection;
use repo_graph_storage::queries::{
	RequirementDeclaration, VerificationObligation,
};

// ── Verdicts ────────────────────────────────────────────────────

/// Four-state computed verdict (truth about the evaluation).
///
/// Variant names match TS verdict strings exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(non_camel_case_types)]
pub enum Verdict {
	PASS,
	FAIL,
	MISSING_EVIDENCE,
	UNSUPPORTED,
}

impl std::fmt::Display for Verdict {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::PASS => write!(f, "PASS"),
			Self::FAIL => write!(f, "FAIL"),
			Self::MISSING_EVIDENCE => write!(f, "MISSING_EVIDENCE"),
			Self::UNSUPPORTED => write!(f, "UNSUPPORTED"),
		}
	}
}

/// Five-state effective verdict used at the gate boundary.
///
/// WAIVED is ONLY an effective state — never a computed state.
/// Serializes to the same string values as Verdict, plus "WAIVED".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[allow(non_camel_case_types)]
pub enum EffectiveVerdict {
	PASS,
	FAIL,
	MISSING_EVIDENCE,
	UNSUPPORTED,
	WAIVED,
}

impl From<Verdict> for EffectiveVerdict {
	fn from(v: Verdict) -> Self {
		match v {
			Verdict::PASS => Self::PASS,
			Verdict::FAIL => Self::FAIL,
			Verdict::MISSING_EVIDENCE => Self::MISSING_EVIDENCE,
			Verdict::UNSUPPORTED => Self::UNSUPPORTED,
		}
	}
}

// ── Waiver basis ────────────────────────────────────────────────

/// Audit trail of the waiver that suppressed a computed verdict.
///
/// Fields mirror TS `WaiverBasis` interface. Non-null iff
/// `effective_verdict === WAIVED`.
#[derive(Debug, Clone, Serialize)]
pub struct WaiverBasis {
	pub waiver_uid: String,
	pub reason: String,
	pub created_at: String,
	pub created_by: Option<String>,
	pub expires_at: Option<String>,
	pub rationale_category: Option<String>,
	pub policy_basis: Option<String>,
}

// ── Obligation evaluation ───────────────────────────────────────

/// Evaluated obligation result (one per verification obligation).
#[derive(Debug, Clone, Serialize)]
pub struct ObligationResult {
	pub req_id: String,
	pub req_version: i64,
	pub obligation_id: String,
	pub obligation: String,
	pub method: String,
	pub target: Option<String>,
	pub threshold: Option<f64>,
	pub operator: Option<String>,
	pub computed_verdict: Verdict,
	pub effective_verdict: EffectiveVerdict,
	pub evidence: serde_json::Value,
	pub waiver_basis: Option<WaiverBasis>,
}

/// Evaluate all obligations from a list of requirement declarations.
///
/// Returns `Err` if a storage read fails during evaluation.
/// Policy verdicts (PASS/FAIL/MISSING_EVIDENCE/UNSUPPORTED) are
/// returned inside `Ok`; only infrastructure failures propagate as
/// errors.
///
/// After computing each verdict, layers waiver overlay:
///   - Non-PASS computed verdicts with a matching active waiver
///     become effective_verdict = WAIVED.
///   - PASS verdicts are never transformed (no exception occurred).
///
/// `now` is an ISO 8601 timestamp for waiver expiry comparison.
pub fn evaluate_obligations(
	storage: &StorageConnection,
	snapshot_uid: &str,
	repo_uid: &str,
	requirements: &[RequirementDeclaration],
	now: &str,
) -> Result<Vec<ObligationResult>, String> {
	let mut results = Vec::new();

	for req in requirements {
		for obl in &req.obligations {
			let mut result = evaluate_single(storage, snapshot_uid, repo_uid, req, obl)?;
			apply_waiver_overlay(storage, repo_uid, req, obl, &mut result, now)?;
			results.push(result);
		}
	}

	Ok(results)
}

fn evaluate_single(
	storage: &StorageConnection,
	snapshot_uid: &str,
	repo_uid: &str,
	req: &RequirementDeclaration,
	obl: &VerificationObligation,
) -> Result<ObligationResult, String> {
	let mut result = ObligationResult {
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

	match obl.method.as_str() {
		"arch_violations" => evaluate_arch_violations(storage, snapshot_uid, repo_uid, obl, &mut result)?,
		"coverage_threshold" => evaluate_coverage_threshold(storage, snapshot_uid, repo_uid, obl, &mut result)?,
		"complexity_threshold" => evaluate_complexity_threshold(storage, snapshot_uid, repo_uid, obl, &mut result)?,
		"hotspot_threshold" => evaluate_hotspot_threshold(storage, snapshot_uid, repo_uid, obl, &mut result)?,
		_ => {
			result.computed_verdict = Verdict::UNSUPPORTED;
			result.effective_verdict = EffectiveVerdict::UNSUPPORTED;
			result.evidence = serde_json::json!({
				"reason": format!("method \"{}\" not yet supported", obl.method),
			});
		}
	}

	Ok(result)
}

/// Layer waiver overlay on a computed obligation result.
///
/// Only non-PASS computed verdicts are candidates for waiver
/// suppression. A PASS obligation stays PASS regardless of active
/// waivers (no policy exception occurred).
///
/// This is a deliberate divergence from the TS prototype, which
/// unconditionally sets effective_verdict = WAIVED for any matching
/// waiver. See module-level doc comment for rationale.
fn apply_waiver_overlay(
	storage: &StorageConnection,
	repo_uid: &str,
	req: &RequirementDeclaration,
	obl: &VerificationObligation,
	result: &mut ObligationResult,
	now: &str,
) -> Result<(), String> {
	// PASS obligations are not waivable — no exception needed.
	if result.computed_verdict == Verdict::PASS {
		return Ok(());
	}

	let waivers = storage
		.find_active_waivers(
			repo_uid,
			&req.req_id,
			req.version,
			&obl.obligation_id,
			now,
		)
		.map_err(|e| format!("failed to read waivers: {}", e))?;

	if let Some(waiver) = waivers.first() {
		result.effective_verdict = EffectiveVerdict::WAIVED;
		result.waiver_basis = Some(WaiverBasis {
			waiver_uid: waiver.declaration_uid.clone(),
			reason: waiver.reason.clone(),
			created_at: waiver.created_at.clone(),
			created_by: waiver.created_by.clone(),
			expires_at: waiver.expires_at.clone(),
			rationale_category: waiver.rationale_category.clone(),
			policy_basis: waiver.policy_basis.clone(),
		});
	}

	Ok(())
}

/// Evaluate the `arch_violations` method.
///
/// Returns `Err` on storage read failure. Returns `Ok` with a
/// MISSING_EVIDENCE verdict for legitimate policy gaps (no target,
/// no boundary declarations).
fn evaluate_arch_violations(
	storage: &StorageConnection,
	snapshot_uid: &str,
	repo_uid: &str,
	obl: &VerificationObligation,
	result: &mut ObligationResult,
) -> Result<(), String> {
	let target = match &obl.target {
		Some(t) => t,
		None => {
			result.computed_verdict = Verdict::MISSING_EVIDENCE;
			result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
			result.evidence = serde_json::json!({ "reason": "no target specified" });
			return Ok(());
		}
	};

	// Storage read: propagate errors, do not swallow.
	let all_boundaries = storage
		.get_active_boundary_declarations(repo_uid)
		.map_err(|e| format!("failed to read boundary declarations: {}", e))?;

	let target_boundaries: Vec<_> = all_boundaries
		.iter()
		.filter(|b| b.boundary_module == *target)
		.collect();

	if target_boundaries.is_empty() {
		result.computed_verdict = Verdict::MISSING_EVIDENCE;
		result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
		result.evidence = serde_json::json!({
			"reason": "no boundary declarations for target",
		});
		return Ok(());
	}

	// Count violations. Storage read errors propagate.
	let mut total_violations = 0;
	for boundary in &target_boundaries {
		let violations = storage
			.find_imports_between_paths(snapshot_uid, target, &boundary.forbids)
			.map_err(|e| format!("failed to query imports between paths: {}", e))?;
		total_violations += violations.len();
	}

	let verdict = if total_violations == 0 {
		Verdict::PASS
	} else {
		Verdict::FAIL
	};

	result.computed_verdict = verdict;
	result.effective_verdict = verdict.into();
	result.evidence = serde_json::json!({
		"violation_count": total_violations,
		"snapshot": snapshot_uid,
	});

	Ok(())
}

/// Evaluate the `coverage_threshold` method (Rust-28).
///
/// Reads `line_coverage` measurements from the measurements table,
/// filters by target path prefix, computes the average coverage,
/// and compares against the obligation's threshold.
///
/// Mirrors TS `evaluateObligation` case `coverage_threshold`
/// (core/evaluator/obligation-evaluator.ts).
///
/// Returns MISSING_EVIDENCE when:
///   - target or threshold not specified
///   - no coverage data exists for the target path
fn evaluate_coverage_threshold(
	storage: &StorageConnection,
	snapshot_uid: &str,
	repo_uid: &str,
	obl: &VerificationObligation,
	result: &mut ObligationResult,
) -> Result<(), String> {
	let target = match &obl.target {
		Some(t) => t,
		None => {
			result.computed_verdict = Verdict::MISSING_EVIDENCE;
			result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
			result.evidence = serde_json::json!({
				"reason": "target or threshold not specified",
			});
			return Ok(());
		}
	};

	let threshold = match obl.threshold {
		Some(t) => t,
		None => {
			result.computed_verdict = Verdict::MISSING_EVIDENCE;
			result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
			result.evidence = serde_json::json!({
				"reason": "target or threshold not specified",
			});
			return Ok(());
		}
	};

	let all_rows = storage
		.query_measurements_by_kind(snapshot_uid, "line_coverage")
		.map_err(|e| format!("failed to read coverage measurements: {}", e))?;

	// Filter by target path prefix: "{repo_uid}:{target}/"
	let prefix = format!("{}:{}/", repo_uid, target);
	let matching: Vec<_> = all_rows
		.iter()
		.filter(|r| r.target_stable_key.starts_with(&prefix))
		.collect();

	if matching.is_empty() {
		result.computed_verdict = Verdict::MISSING_EVIDENCE;
		result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
		result.evidence = serde_json::json!({
			"reason": "no coverage data for target path",
		});
		return Ok(());
	}

	// Compute average coverage from value_json.value fields.
	// Measurements are authored policy evidence — malformed rows
	// must fail the gate, not silently drag the average to zero.
	let mut sum = 0.0_f64;
	for row in &matching {
		let parsed: serde_json::Value = serde_json::from_str(&row.value_json)
			.map_err(|e| format!(
				"malformed coverage measurement for {}: {}",
				row.target_stable_key, e,
			))?;
		let value = parsed["value"].as_f64().ok_or_else(|| format!(
			"coverage measurement for {} missing numeric \"value\" field",
			row.target_stable_key,
		))?;
		sum += value;
	}
	let avg = sum / matching.len() as f64;

	let op = obl.operator.as_deref().unwrap_or(">=");
	let pass = compare_values(avg, op, threshold);

	let verdict = if pass { Verdict::PASS } else { Verdict::FAIL };
	result.computed_verdict = verdict;
	result.effective_verdict = verdict.into();
	result.evidence = serde_json::json!({
		"avg_coverage": (avg * 10000.0).round() / 10000.0,
		"threshold": threshold,
		"operator": op,
		"files_measured": matching.len(),
	});

	Ok(())
}

/// Evaluate the `complexity_threshold` method (Rust-29).
///
/// Reads `cyclomatic_complexity` measurements, filters by target
/// path prefix, finds the maximum complexity value, and compares
/// against the obligation's threshold.
///
/// Mirrors TS `evaluateObligation` case `complexity_threshold`
/// (core/evaluator/obligation-evaluator.ts).
///
/// TS uses `.includes()` for prefix filtering; Rust uses
/// `.starts_with()` (consistent with `coverage_threshold`). The
/// `.includes()` in TS is arguably a bug — it would match paths
/// that contain the prefix as a substring anywhere.
///
/// Evidence fields: `max_complexity`, `threshold`, `operator`,
/// `functions_measured`.
fn evaluate_complexity_threshold(
	storage: &StorageConnection,
	snapshot_uid: &str,
	repo_uid: &str,
	obl: &VerificationObligation,
	result: &mut ObligationResult,
) -> Result<(), String> {
	let target = match &obl.target {
		Some(t) => t,
		None => {
			result.computed_verdict = Verdict::MISSING_EVIDENCE;
			result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
			result.evidence = serde_json::json!({
				"reason": "target or threshold not specified",
			});
			return Ok(());
		}
	};

	let threshold = match obl.threshold {
		Some(t) => t,
		None => {
			result.computed_verdict = Verdict::MISSING_EVIDENCE;
			result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
			result.evidence = serde_json::json!({
				"reason": "target or threshold not specified",
			});
			return Ok(());
		}
	};

	let all_rows = storage
		.query_measurements_by_kind(snapshot_uid, "cyclomatic_complexity")
		.map_err(|e| format!("failed to read complexity measurements: {}", e))?;

	let prefix = format!("{}:{}/", repo_uid, target);
	let matching: Vec<_> = all_rows
		.iter()
		.filter(|r| r.target_stable_key.starts_with(&prefix))
		.collect();

	if matching.is_empty() {
		result.computed_verdict = Verdict::MISSING_EVIDENCE;
		result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
		result.evidence = serde_json::json!({
			"reason": "no complexity data for target path",
		});
		return Ok(());
	}

	// Find max complexity. Malformed rows abort — measurements are
	// authored policy evidence, not optional hints.
	let mut max_cc = f64::NEG_INFINITY;
	for row in &matching {
		let parsed: serde_json::Value = serde_json::from_str(&row.value_json)
			.map_err(|e| format!(
				"malformed complexity measurement for {}: {}",
				row.target_stable_key, e,
			))?;
		let value = parsed["value"].as_f64().ok_or_else(|| format!(
			"complexity measurement for {} missing numeric \"value\" field",
			row.target_stable_key,
		))?;
		if value > max_cc {
			max_cc = value;
		}
	}

	let op = obl.operator.as_deref().unwrap_or("<=");
	let pass = compare_values(max_cc, op, threshold);

	let verdict = if pass { Verdict::PASS } else { Verdict::FAIL };
	result.computed_verdict = verdict;
	result.effective_verdict = verdict.into();
	result.evidence = serde_json::json!({
		"max_complexity": max_cc,
		"threshold": threshold,
		"operator": op,
		"functions_measured": matching.len(),
	});

	Ok(())
}

/// Evaluate the `hotspot_threshold` method (Rust-31).
///
/// Reads `hotspot_score` inferences, optionally filters by target
/// path prefix, finds the maximum `normalized_score`, and compares
/// against the obligation's threshold.
///
/// Mirrors TS `evaluateObligation` case `hotspot_threshold`
/// (core/evaluator/obligation-evaluator.ts).
///
/// Key differences from coverage/complexity:
///   - Target is optional. If omitted, all hotspot inferences for
///     the snapshot are considered (whole-repo scope).
///   - Value field is `normalized_score`, not `value`.
///   - Evidence shape: `max_hotspot_score`, `threshold` only
///     (no operator or count fields — matches TS).
fn evaluate_hotspot_threshold(
	storage: &StorageConnection,
	snapshot_uid: &str,
	repo_uid: &str,
	obl: &VerificationObligation,
	result: &mut ObligationResult,
) -> Result<(), String> {
	let threshold = match obl.threshold {
		Some(t) => t,
		None => {
			result.computed_verdict = Verdict::MISSING_EVIDENCE;
			result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
			result.evidence = serde_json::json!({
				"reason": "threshold not specified",
			});
			return Ok(());
		}
	};

	let all_rows = storage
		.query_inferences_by_kind(snapshot_uid, "hotspot_score")
		.map_err(|e| format!("failed to read hotspot inferences: {}", e))?;

	// Filter by target prefix if target is specified.
	let matching: Vec<_> = if let Some(target) = &obl.target {
		let prefix = format!("{}:{}/", repo_uid, target);
		all_rows
			.iter()
			.filter(|r| r.target_stable_key.starts_with(&prefix))
			.collect()
	} else {
		all_rows.iter().collect()
	};

	if matching.is_empty() {
		let reason = if obl.target.is_some() {
			"no hotspot data for target path"
		} else {
			"no hotspot data"
		};
		result.computed_verdict = Verdict::MISSING_EVIDENCE;
		result.effective_verdict = EffectiveVerdict::MISSING_EVIDENCE;
		result.evidence = serde_json::json!({ "reason": reason });
		return Ok(());
	}

	// Find max normalized_score. Malformed rows abort.
	let mut max_hs = f64::NEG_INFINITY;
	for row in &matching {
		let parsed: serde_json::Value = serde_json::from_str(&row.value_json)
			.map_err(|e| format!(
				"malformed hotspot inference for {}: {}",
				row.target_stable_key, e,
			))?;
		let score = parsed["normalized_score"].as_f64().ok_or_else(|| format!(
			"hotspot inference for {} missing numeric \"normalized_score\" field",
			row.target_stable_key,
		))?;
		if score > max_hs {
			max_hs = score;
		}
	}

	let op = obl.operator.as_deref().unwrap_or("<=");
	let pass = compare_values(max_hs, op, threshold);

	let verdict = if pass { Verdict::PASS } else { Verdict::FAIL };
	result.computed_verdict = verdict;
	result.effective_verdict = verdict.into();
	result.evidence = serde_json::json!({
		"max_hotspot_score": max_hs,
		"threshold": threshold,
	});

	Ok(())
}

/// Compare a numeric value against a threshold using a string operator.
///
/// Mirrors TS `compareValues` (obligation-evaluator.ts).
/// Unknown operators return false (fail-safe).
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

// ── Gate mode ───────────────────────────────────────────────────

/// Gate reduction mode (Rust-26).
///
/// Mirrors TS `GateMode` from `core/gate/reducer.ts`.
/// Determines how non-PASS effective verdicts map to exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
	/// exit 0: all PASS/WAIVED. exit 1: any FAIL. exit 2: no FAIL
	/// but MISSING_EVIDENCE or UNSUPPORTED.
	Default,
	/// exit 0: all PASS/WAIVED. exit 1: any FAIL, MISSING_EVIDENCE,
	/// or UNSUPPORTED.
	Strict,
	/// exit 0: no FAIL (MISSING/UNSUPPORTED informational).
	/// exit 1: any FAIL.
	Advisory,
}

impl GateMode {
	/// Serialize to the lowercase string used in JSON output.
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::Default => "default",
			Self::Strict => "strict",
			Self::Advisory => "advisory",
		}
	}
}

// ── Gate reduction ──────────────────────────────────────────────

/// Gate outcome after reducing all obligation verdicts.
#[derive(Debug, Clone, Serialize)]
pub struct GateResult {
	pub outcome: String,
	pub exit_code: i32,
	pub mode: String,
	pub counts: GateCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateCounts {
	pub total: usize,
	pub pass: usize,
	pub fail: usize,
	pub waived: usize,
	pub missing_evidence: usize,
	pub unsupported: usize,
}

/// Reduce obligation verdicts to a gate outcome.
///
/// WAIVED obligations are non-failing in all three modes.
/// They are counted separately in `counts.waived`.
///
/// Mode semantics (mirrors TS `reduceToGateOutcome`):
///
///   default:
///     exit 0 — all PASS or WAIVED (or empty)
///     exit 1 — any FAIL
///     exit 2 — no FAIL, but MISSING_EVIDENCE or UNSUPPORTED
///
///   strict:
///     exit 0 — all PASS or WAIVED
///     exit 1 — any FAIL, MISSING_EVIDENCE, or UNSUPPORTED
///
///   advisory:
///     exit 0 — no FAIL (MISSING/UNSUPPORTED are informational)
///     exit 1 — any FAIL
pub fn reduce_to_gate_outcome(
	obligations: &[ObligationResult],
	mode: GateMode,
) -> GateResult {
	let mut counts = GateCounts {
		total: obligations.len(),
		pass: 0,
		fail: 0,
		waived: 0,
		missing_evidence: 0,
		unsupported: 0,
	};

	for obl in obligations {
		match obl.effective_verdict {
			EffectiveVerdict::PASS => counts.pass += 1,
			EffectiveVerdict::FAIL => counts.fail += 1,
			EffectiveVerdict::MISSING_EVIDENCE => counts.missing_evidence += 1,
			EffectiveVerdict::UNSUPPORTED => counts.unsupported += 1,
			EffectiveVerdict::WAIVED => counts.waived += 1,
		}
	}

	let has_fail = counts.fail > 0;
	let has_incomplete = counts.missing_evidence > 0 || counts.unsupported > 0;

	let (outcome, exit_code) = match mode {
		GateMode::Default => {
			if has_fail {
				("fail", 1)
			} else if has_incomplete {
				("incomplete", 2)
			} else {
				("pass", 0)
			}
		}
		GateMode::Strict => {
			if has_fail || has_incomplete {
				("fail", 1)
			} else {
				("pass", 0)
			}
		}
		GateMode::Advisory => {
			if has_fail {
				("fail", 1)
			} else {
				("pass", 0)
			}
		}
	};

	GateResult {
		outcome: outcome.to_string(),
		exit_code,
		mode: mode.as_str().to_string(),
		counts,
	}
}

// Unit tests for gate evaluation are in repo-graph-storage's
// queries::tests module (where connection() is accessible for
// schema sabotage). See find_imports_between_paths_propagates_error.
