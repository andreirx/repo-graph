//! Gate evaluation support for `rgr-rust gate`.
//!
//! Narrow gate evaluator: evaluates requirement obligations against
//! the current snapshot. Only `arch_violations` is supported in
//! Rust-24; all other methods return UNSUPPORTED.
//!
//! This module owns:
//!   - Obligation evaluation (method dispatch)
//!   - Gate reduction (verdict list → outcome + exit code)
//!   - Output DTOs (matching TS gate JSON shape)
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
	/// Same as computed_verdict in Rust-24 (no waivers yet).
	pub effective_verdict: Verdict,
	pub evidence: serde_json::Value,
	/// Always null in Rust-24 (no waivers yet).
	pub waiver_basis: Option<()>,
}

/// Evaluate all obligations from a list of requirement declarations.
///
/// Returns `Err` if a storage read fails during evaluation.
/// Policy verdicts (PASS/FAIL/MISSING_EVIDENCE/UNSUPPORTED) are
/// returned inside `Ok`; only infrastructure failures propagate as
/// errors.
pub fn evaluate_obligations(
	storage: &StorageConnection,
	snapshot_uid: &str,
	repo_uid: &str,
	requirements: &[RequirementDeclaration],
) -> Result<Vec<ObligationResult>, String> {
	let mut results = Vec::new();

	for req in requirements {
		for obl in &req.obligations {
			let result = evaluate_single(storage, snapshot_uid, repo_uid, req, obl)?;
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
		effective_verdict: Verdict::UNSUPPORTED,
		evidence: serde_json::json!({}),
		waiver_basis: None,
	};

	match obl.method.as_str() {
		"arch_violations" => evaluate_arch_violations(storage, snapshot_uid, repo_uid, obl, &mut result)?,
		_ => {
			result.computed_verdict = Verdict::UNSUPPORTED;
			result.effective_verdict = Verdict::UNSUPPORTED;
			result.evidence = serde_json::json!({
				"reason": format!("method \"{}\" not yet supported", obl.method),
			});
		}
	}

	Ok(result)
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
			result.effective_verdict = Verdict::MISSING_EVIDENCE;
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
		result.effective_verdict = Verdict::MISSING_EVIDENCE;
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
	result.effective_verdict = verdict;
	result.evidence = serde_json::json!({
		"violation_count": total_violations,
		"snapshot": snapshot_uid,
	});

	Ok(())
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

/// Reduce obligation verdicts to a gate outcome (default mode only).
///
/// Mirrors TS `reduceToGateOutcome` (core/gate/reducer.ts) for
/// default mode:
///   - exit 0: all PASS (or empty)
///   - exit 1: any FAIL
///   - exit 2: no FAIL, but MISSING_EVIDENCE or UNSUPPORTED
pub fn reduce_to_gate_outcome(obligations: &[ObligationResult]) -> GateResult {
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
			Verdict::PASS => counts.pass += 1,
			Verdict::FAIL => counts.fail += 1,
			Verdict::MISSING_EVIDENCE => counts.missing_evidence += 1,
			Verdict::UNSUPPORTED => counts.unsupported += 1,
		}
	}

	let has_fail = counts.fail > 0;
	let has_incomplete = counts.missing_evidence > 0 || counts.unsupported > 0;

	let (outcome, exit_code) = if has_fail {
		("fail", 1)
	} else if has_incomplete {
		("incomplete", 2)
	} else {
		("pass", 0)
	};

	GateResult {
		outcome: outcome.to_string(),
		exit_code,
		mode: "default".to_string(),
		counts,
	}
}

// Unit tests for gate evaluation are in repo-graph-storage's
// queries::tests module (where connection() is accessible for
// schema sabotage). See find_imports_between_paths_propagates_error.
