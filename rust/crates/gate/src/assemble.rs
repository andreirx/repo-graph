//! Storage-backed gate assembly.
//!
//! `assemble` is the thin orchestration layer that sits
//! between `GateStorageRead` (port) and `compute` (pure).
//! It:
//!
//!   1. Reads active requirements for the repo.
//!   2. For each obligation, dispatches on `method` and
//!      fetches the right evidence through the port. All
//!      per-method storage calls happen here; compute never
//!      calls storage.
//!   3. Reads active waivers for every obligation up-front —
//!      even PASS-path obligations receive a waiver lookup,
//!      because compute alone decides whether a waiver applies
//!      (PASS-not-waivable semantics) and that decision needs
//!      the waiver vector available.
//!   4. Bundles everything into a `GateInput` and calls
//!      `compute`.
//!
//! The function signature takes a storage implementor by
//! reference. No `'static` bounds, no clone requirements on
//! the storage handle.
//!
//! ── Filter policy ──────────────────────────────────────────
//!
//! Prefix filtering for coverage / complexity / hotspot uses
//! the same shape as the pre-relocation gate.rs:
//! `"{repo_uid}:{target}/"`. Byte-identical to preserve gate
//! parity.

use crate::compute::{
	compute, GateInput, MethodEvidence, ObligationKey, PolicyMeasurement,
};
use crate::errors::GateError;
use crate::storage_port::GateStorageRead;
use crate::types::{
	GateInference, GateMeasurement, GateMode, GateReport, GateRequirement,
};

// ── Measurement / inference pre-parsing ──────────────────────────
//
// Malformed value_json on coverage/complexity measurements and
// hotspot inferences must abort the gate run with a stable
// diagnostic message identifying the offending row. The
// pre-relocation gate.rs produced specific stderr strings that
// the CLI test suite pins:
//
//   "malformed coverage measurement for <key>: <json-error>"
//   "coverage measurement for <key> missing numeric \"value\" field"
//   "malformed complexity measurement for <key>: <json-error>"
//   "complexity measurement for <key> missing numeric \"value\" field"
//   "malformed hotspot inference for <key>: <json-error>"
//   "hotspot inference for <key> missing numeric \"normalized_score\" field"
//
// These strings are the contract. Preserve them verbatim here
// so the relocation is externally byte-stable.

#[derive(Debug, Clone, Copy)]
enum MeasurementKind {
	Coverage,
	Complexity,
}

impl MeasurementKind {
	fn noun(self) -> &'static str {
		match self {
			Self::Coverage => "coverage measurement",
			Self::Complexity => "complexity measurement",
		}
	}
	fn operation(self) -> &'static str {
		match self {
			Self::Coverage => "coverage_threshold",
			Self::Complexity => "complexity_threshold",
		}
	}
}

fn parse_measurements<I>(
	rows: I,
	kind: MeasurementKind,
) -> Result<Vec<PolicyMeasurement>, GateError>
where
	I: IntoIterator<Item = GateMeasurement>,
{
	let mut out = Vec::new();
	for row in rows {
		let parsed: serde_json::Value = serde_json::from_str(&row.value_json)
			.map_err(|e| GateError::MalformedEvidence {
				operation: kind.operation(),
				reason: format!(
					"malformed {} for {}: {}",
					kind.noun(),
					row.target_stable_key,
					e
				),
			})?;
		let value = parsed["value"].as_f64().ok_or_else(|| {
			GateError::MalformedEvidence {
				operation: kind.operation(),
				reason: format!(
					"{} for {} missing numeric \"value\" field",
					kind.noun(),
					row.target_stable_key
				),
			}
		})?;
		out.push(PolicyMeasurement {
			target_stable_key: row.target_stable_key,
			value,
		});
	}
	Ok(out)
}

fn parse_inferences<I>(rows: I) -> Result<Vec<PolicyMeasurement>, GateError>
where
	I: IntoIterator<Item = GateInference>,
{
	let mut out = Vec::new();
	for row in rows {
		let parsed: serde_json::Value = serde_json::from_str(&row.value_json)
			.map_err(|e| GateError::MalformedEvidence {
				operation: "hotspot_threshold",
				reason: format!(
					"malformed hotspot inference for {}: {}",
					row.target_stable_key, e
				),
			})?;
		let value = parsed["normalized_score"].as_f64().ok_or_else(|| {
			GateError::MalformedEvidence {
				operation: "hotspot_threshold",
				reason: format!(
					"hotspot inference for {} missing numeric \"normalized_score\" field",
					row.target_stable_key
				),
			}
		})?;
		out.push(PolicyMeasurement {
			target_stable_key: row.target_stable_key,
			value,
		});
	}
	Ok(out)
}

/// Fetch everything the gate policy needs from storage,
/// build a `GateInput`, and delegate to `compute`.
pub fn assemble<S: GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
	mode: GateMode,
	now: &str,
) -> Result<GateReport, GateError> {
	let requirements = storage.get_active_requirements(repo_uid)?;
	assemble_from_requirements(storage, repo_uid, snapshot_uid, mode, now, requirements)
}

/// Assemble variant that takes requirements externally. Used
/// by callers that have already fetched requirements and want
/// to reuse them — notably the agent orient aggregator, which
/// needs to know up front whether ANY requirements exist so it
/// can emit a `GATE_NOT_CONFIGURED` limit before running gate.
pub fn assemble_from_requirements<S: GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
	mode: GateMode,
	now: &str,
	requirements: Vec<GateRequirement>,
) -> Result<GateReport, GateError> {
	// Fetch quality-policy assessment facts up front.
	let quality_assessment_facts =
		storage.get_quality_assessment_facts_for_gate(repo_uid, snapshot_uid)?;

	let mut input = GateInput {
		requirements,
		mode,
		now: now.to_string(),
		method_evidence: std::collections::HashMap::new(),
		matching_waivers: std::collections::HashMap::new(),
		quality_assessment_facts,
	};

	// Pre-fetch per-method caches that are shared across
	// obligations. This is a 4-method world today; when new
	// methods land, extend this block.
	let mut cached_boundaries: Option<Vec<crate::types::GateBoundaryDeclaration>> = None;
	let mut cached_coverage: Option<Vec<crate::types::GateMeasurement>> = None;
	let mut cached_complexity: Option<Vec<crate::types::GateMeasurement>> = None;
	let mut cached_hotspots: Option<Vec<crate::types::GateInference>> = None;

	// Collect keys + evidence in a temporary vector so we can
	// drop the immutable borrow on `input.requirements` before
	// inserting into the mutable `input.method_evidence` map.
	let mut pending_evidence: Vec<(ObligationKey, MethodEvidence)> = Vec::new();

	for req in &input.requirements {
		for obl in &req.obligations {
			let key = ObligationKey::new(&req.req_id, req.version, &obl.obligation_id);

			let evidence: MethodEvidence = match obl.method.as_str() {
				"arch_violations" => {
					let target = match &obl.target {
						Some(t) => t.clone(),
						None => {
							pending_evidence.push((key, MethodEvidence::TargetMissing));
							continue;
						}
					};
					let boundaries = match cached_boundaries.clone() {
						Some(b) => b,
						None => {
							let loaded = storage.get_boundary_declarations(repo_uid)?;
							cached_boundaries = Some(loaded.clone());
							loaded
						}
					};
					let for_target: Vec<_> = boundaries
						.into_iter()
						.filter(|b| b.boundary_module == target)
						.collect();

					if for_target.is_empty() {
						MethodEvidence::ArchViolations {
							target: target.clone(),
							per_boundary_counts: Vec::new(),
							has_any_boundary_for_target: false,
						}
					} else {
						let mut per_boundary_counts = Vec::with_capacity(for_target.len());
						for b in &for_target {
							let edges = storage.find_boundary_imports(
								snapshot_uid,
								&target,
								&b.forbids,
							)?;
							per_boundary_counts.push((b.forbids.clone(), edges.len()));
						}
						MethodEvidence::ArchViolations {
							target,
							per_boundary_counts,
							has_any_boundary_for_target: true,
						}
					}
				}

				"coverage_threshold" => {
					let target = match &obl.target {
						Some(t) => t.clone(),
						None => {
							pending_evidence.push((key, MethodEvidence::TargetMissing));
							continue;
						}
					};
					if obl.threshold.is_none() {
						pending_evidence.push((key, MethodEvidence::ThresholdMissing));
						continue;
					}
					let rows = match cached_coverage.clone() {
						Some(v) => v,
						None => {
							let loaded = storage.get_coverage_measurements(snapshot_uid)?;
							cached_coverage = Some(loaded.clone());
							loaded
						}
					};
					let prefix = format!("{}:{}/", repo_uid, target);
					let matching = parse_measurements(
						rows.into_iter().filter(|r| r.target_stable_key.starts_with(&prefix)),
						MeasurementKind::Coverage,
					)?;
					MethodEvidence::CoverageThreshold {
						target,
						matching_measurements: matching,
					}
				}

				"complexity_threshold" => {
					let target = match &obl.target {
						Some(t) => t.clone(),
						None => {
							pending_evidence.push((key, MethodEvidence::TargetMissing));
							continue;
						}
					};
					if obl.threshold.is_none() {
						pending_evidence.push((key, MethodEvidence::ThresholdMissing));
						continue;
					}
					let rows = match cached_complexity.clone() {
						Some(v) => v,
						None => {
							let loaded = storage.get_complexity_measurements(snapshot_uid)?;
							cached_complexity = Some(loaded.clone());
							loaded
						}
					};
					let prefix = format!("{}:{}/", repo_uid, target);
					let matching = parse_measurements(
						rows.into_iter().filter(|r| r.target_stable_key.starts_with(&prefix)),
						MeasurementKind::Complexity,
					)?;
					MethodEvidence::ComplexityThreshold {
						target,
						matching_measurements: matching,
					}
				}

				"hotspot_threshold" => {
					if obl.threshold.is_none() {
						pending_evidence.push((key, MethodEvidence::ThresholdMissing));
						continue;
					}
					let rows = match cached_hotspots.clone() {
						Some(v) => v,
						None => {
							let loaded = storage.get_hotspot_inferences(snapshot_uid)?;
							cached_hotspots = Some(loaded.clone());
							loaded
						}
					};
					let matching = if let Some(target) = &obl.target {
						let prefix = format!("{}:{}/", repo_uid, target);
						parse_inferences(
							rows.into_iter().filter(|r| r.target_stable_key.starts_with(&prefix)),
						)?
					} else {
						parse_inferences(rows.into_iter())?
					};
					MethodEvidence::HotspotThreshold {
						target: obl.target.clone(),
						matching_inferences: matching,
					}
				}

				"module_violations" => {
					// Repo-wide only. The `target` field on the obligation
					// is ignored. threshold is not used.
					let evidence = storage.evaluate_module_violations(
						repo_uid,
						snapshot_uid,
					)?;
					MethodEvidence::ModuleViolations {
						violations_count: evidence.violations_count,
						stale_declarations_count: evidence.stale_declarations_count,
					}
				}

				_ => MethodEvidence::UnsupportedMethod,
			};

			pending_evidence.push((key, evidence));
		}
	}

	// Now that we are done iterating `input.requirements`
	// immutably, collect waivers and insert evidence.
	let mut pending_waivers: Vec<(ObligationKey, Vec<crate::types::GateWaiver>)> =
		Vec::new();
	for req in &input.requirements {
		for obl in &req.obligations {
			let key = ObligationKey::new(&req.req_id, req.version, &obl.obligation_id);
			let waivers = storage.find_waivers(
				repo_uid,
				&req.req_id,
				req.version,
				&obl.obligation_id,
				now,
			)?;
			pending_waivers.push((key, waivers));
		}
	}

	for (key, ev) in pending_evidence {
		input.method_evidence.insert(key, ev);
	}
	for (key, waivers) in pending_waivers {
		input.matching_waivers.insert(key, waivers);
	}

	Ok(compute(input))
}

// ── assemble tests (use a fake storage) ──────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use crate::errors::GateStorageError;
	use crate::storage_port::GateStorageRead;
	use crate::types::{
		GateBoundaryDeclaration, GateImportEdge, GateInference, GateMeasurement,
		GateModuleViolationEvidence, GateObligation, GateRequirement, GateWaiver,
		Verdict,
	};
	use std::cell::RefCell;

	#[derive(Default)]
	struct FakeStorage {
		pub requirements: Vec<GateRequirement>,
		pub boundaries: Vec<GateBoundaryDeclaration>,
		pub boundary_edges: std::collections::HashMap<(String, String), Vec<GateImportEdge>>,
		pub coverage: Vec<GateMeasurement>,
		pub complexity: Vec<GateMeasurement>,
		pub hotspots: Vec<GateInference>,
		pub waivers: std::collections::HashMap<(String, i64, String), Vec<GateWaiver>>,
		pub module_violations: GateModuleViolationEvidence,
		pub force_error_on: RefCell<Option<&'static str>>,
	}

	impl FakeStorage {
		fn fail(&self, op: &'static str) -> Result<(), GateStorageError> {
			if let Some(target) = *self.force_error_on.borrow() {
				if target == op {
					return Err(GateStorageError::new(op, format!("forced failure on {op}")));
				}
			}
			Ok(())
		}
	}

	impl GateStorageRead for FakeStorage {
		fn get_active_requirements(
			&self,
			_repo_uid: &str,
		) -> Result<Vec<GateRequirement>, GateStorageError> {
			self.fail("get_active_requirements")?;
			Ok(self.requirements.clone())
		}
		fn get_boundary_declarations(
			&self,
			_repo_uid: &str,
		) -> Result<Vec<GateBoundaryDeclaration>, GateStorageError> {
			self.fail("get_boundary_declarations")?;
			Ok(self.boundaries.clone())
		}
		fn find_boundary_imports(
			&self,
			_snapshot_uid: &str,
			source: &str,
			target: &str,
		) -> Result<Vec<GateImportEdge>, GateStorageError> {
			self.fail("find_boundary_imports")?;
			Ok(self
				.boundary_edges
				.get(&(source.to_string(), target.to_string()))
				.cloned()
				.unwrap_or_default())
		}
		fn get_coverage_measurements(
			&self,
			_snapshot_uid: &str,
		) -> Result<Vec<GateMeasurement>, GateStorageError> {
			self.fail("get_coverage_measurements")?;
			Ok(self.coverage.clone())
		}
		fn get_complexity_measurements(
			&self,
			_snapshot_uid: &str,
		) -> Result<Vec<GateMeasurement>, GateStorageError> {
			self.fail("get_complexity_measurements")?;
			Ok(self.complexity.clone())
		}
		fn get_hotspot_inferences(
			&self,
			_snapshot_uid: &str,
		) -> Result<Vec<GateInference>, GateStorageError> {
			self.fail("get_hotspot_inferences")?;
			Ok(self.hotspots.clone())
		}
		fn find_waivers(
			&self,
			_repo_uid: &str,
			req_id: &str,
			req_version: i64,
			obligation_id: &str,
			_now: &str,
		) -> Result<Vec<GateWaiver>, GateStorageError> {
			self.fail("find_waivers")?;
			Ok(self
				.waivers
				.get(&(req_id.to_string(), req_version, obligation_id.to_string()))
				.cloned()
				.unwrap_or_default())
		}

		fn evaluate_module_violations(
			&self,
			_repo_uid: &str,
			_snapshot_uid: &str,
		) -> Result<GateModuleViolationEvidence, GateStorageError> {
			self.fail("evaluate_module_violations")?;
			Ok(self.module_violations.clone())
		}

		fn get_quality_assessment_facts_for_gate(
			&self,
			_repo_uid: &str,
			_snapshot_uid: &str,
		) -> Result<Vec<crate::types::GateQualityAssessmentFact>, GateStorageError> {
			self.fail("get_quality_assessment_facts_for_gate")?;
			// FakeStorage returns empty by default — tests that need
			// quality assessments must set up the storage explicitly.
			Ok(vec![])
		}
	}

	fn make_obl(id: &str, method: &str, target: Option<&str>, threshold: Option<f64>) -> GateObligation {
		GateObligation {
			obligation_id: id.to_string(),
			obligation: "test".to_string(),
			method: method.to_string(),
			target: target.map(String::from),
			threshold,
			operator: None,
		}
	}

	#[test]
	fn assemble_empty_requirements_returns_pass_pass_zero_counts() {
		let fake = FakeStorage::default();
		let report =
			assemble(&fake, "r1", "snap-1", GateMode::Default, "2026-04-15T00:00:00Z")
				.unwrap();
		assert!(report.obligations.is_empty());
		assert_eq!(report.outcome.counts.total, 0);
		assert_eq!(report.outcome.exit_code, 0);
	}

	#[test]
	fn assemble_arch_violations_fails_on_violating_edge() {
		let mut fake = FakeStorage::default();
		fake.requirements = vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![make_obl("o1", "arch_violations", Some("src/core"), None)],
		}];
		fake.boundaries = vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}];
		fake.boundary_edges.insert(
			("src/core".into(), "src/adapters".into()),
			vec![GateImportEdge {
				source_file: "src/core/a.rs".into(),
				target_file: "src/adapters/b.rs".into(),
			}],
		);

		let report =
			assemble(&fake, "r1", "snap-1", GateMode::Default, "2026-04-15T00:00:00Z")
				.unwrap();
		assert_eq!(report.obligations[0].computed_verdict, Verdict::FAIL);
		assert_eq!(report.outcome.outcome, "fail");
	}

	#[test]
	fn assemble_coverage_threshold_filters_by_target_prefix() {
		let mut fake = FakeStorage::default();
		fake.requirements = vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![make_obl(
				"o1",
				"coverage_threshold",
				Some("src/core"),
				Some(0.80),
			)],
		}];
		fake.coverage = vec![
			// Matches prefix
			GateMeasurement {
				target_stable_key: "r1:src/core/a.rs:FILE".into(),
				value_json: r#"{"value":0.90}"#.into(),
			},
			// Does NOT match prefix (different dir)
			GateMeasurement {
				target_stable_key: "r1:src/adapters/a.rs:FILE".into(),
				value_json: r#"{"value":0.10}"#.into(),
			},
		];

		let report =
			assemble(&fake, "r1", "snap-1", GateMode::Default, "2026-04-15T00:00:00Z")
				.unwrap();
		// Average over the only matching row (0.90) >= 0.80 → PASS.
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
		assert_eq!(report.obligations[0].evidence["files_measured"], 1);
	}

	#[test]
	fn assemble_waiver_overlay_applies_from_find_waivers() {
		let mut fake = FakeStorage::default();
		fake.requirements = vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![make_obl("o1", "arch_violations", Some("src/core"), None)],
		}];
		fake.boundaries = vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}];
		fake.boundary_edges.insert(
			("src/core".into(), "src/adapters".into()),
			vec![GateImportEdge {
				source_file: "src/core/a.rs".into(),
				target_file: "src/adapters/b.rs".into(),
			}],
		);
		fake.waivers.insert(
			("REQ-1".into(), 1, "o1".into()),
			vec![GateWaiver {
				waiver_uid: "w1".into(),
				reason: "accepted".into(),
				created_at: "2026-04-14T00:00:00Z".into(),
				created_by: None,
				expires_at: None,
				rationale_category: None,
				policy_basis: None,
			}],
		);

		let report =
			assemble(&fake, "r1", "snap-1", GateMode::Default, "2026-04-15T00:00:00Z")
				.unwrap();
		assert_eq!(report.obligations[0].computed_verdict, Verdict::FAIL);
		assert_eq!(
			report.obligations[0].effective_verdict,
			crate::types::EffectiveVerdict::WAIVED
		);
		assert_eq!(report.outcome.outcome, "pass");
	}

	#[test]
	fn assemble_propagates_storage_error() {
		let mut fake = FakeStorage::default();
		fake.requirements = vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![make_obl("o1", "arch_violations", Some("src/core"), None)],
		}];
		*fake.force_error_on.borrow_mut() = Some("get_boundary_declarations");

		let err =
			assemble(&fake, "r1", "snap-1", GateMode::Default, "2026-04-15T00:00:00Z")
				.unwrap_err();
		match err {
			GateError::Storage(e) => assert_eq!(e.operation, "get_boundary_declarations"),
			other => panic!("expected Storage, got {:?}", other),
		}
	}

	// ── module_violations ──

	#[test]
	fn assemble_module_violations_pass_with_zero_violations() {
		let mut fake = FakeStorage::default();
		fake.requirements = vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![make_obl("o1", "module_violations", None, None)],
		}];
		fake.module_violations = GateModuleViolationEvidence {
			violations_count: 0,
			stale_declarations_count: 0,
		};

		let report =
			assemble(&fake, "r1", "snap-1", GateMode::Default, "2026-04-15T00:00:00Z")
				.unwrap();
		assert_eq!(report.obligations[0].computed_verdict, Verdict::PASS);
		assert_eq!(report.outcome.outcome, "pass");
	}

	#[test]
	fn assemble_module_violations_fail_when_violations_exist() {
		let mut fake = FakeStorage::default();
		fake.requirements = vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![make_obl("o1", "module_violations", None, None)],
		}];
		fake.module_violations = GateModuleViolationEvidence {
			violations_count: 2,
			stale_declarations_count: 1,
		};

		let report =
			assemble(&fake, "r1", "snap-1", GateMode::Default, "2026-04-15T00:00:00Z")
				.unwrap();
		assert_eq!(report.obligations[0].computed_verdict, Verdict::FAIL);
		assert_eq!(report.obligations[0].evidence["violations_count"], 2);
		assert_eq!(report.outcome.outcome, "fail");
	}

	#[test]
	fn assemble_module_violations_propagates_storage_error() {
		let mut fake = FakeStorage::default();
		fake.requirements = vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![make_obl("o1", "module_violations", None, None)],
		}];
		*fake.force_error_on.borrow_mut() = Some("evaluate_module_violations");

		let err =
			assemble(&fake, "r1", "snap-1", GateMode::Default, "2026-04-15T00:00:00Z")
				.unwrap_err();
		match err {
			GateError::Storage(e) => assert_eq!(e.operation, "evaluate_module_violations"),
			other => panic!("expected Storage, got {:?}", other),
		}
	}
}
