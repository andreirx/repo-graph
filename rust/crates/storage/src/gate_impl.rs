//! Adapter impl: `GateStorageRead` on `StorageConnection`.
//!
//! Added in Rust-43A as part of the `rgr/src/gate.rs`
//! relocation into `repo-graph-gate`. The gate crate defines
//! `GateStorageRead` (policy); this file is the adapter side
//! that lets the gate policy read SQLite through a
//! storage-agnostic port.
//!
//! Responsibilities:
//!
//!   1. Translate storage errors into `GateStorageError`. The
//!      gate crate never sees `rusqlite::Error`, `StorageError`,
//!      table names, or SQL diagnostics.
//!
//!   2. Map storage row DTOs (e.g. `queries::WaiverDeclaration`,
//!      `queries::MeasurementRow`, `queries::InferenceRow`,
//!      `queries::BoundaryDeclaration`, `queries::ImportEdgeResult`,
//!      `queries::RequirementDeclaration`) into gate-owned
//!      DTOs. No storage types leak through the trait.
//!
//! This adapter is additive. It does not remove the existing
//! storage query methods — the pre-relocation `rmap gate`
//! CLI still needed to work during the relocation commit, and
//! the CLI's `run_gate` now calls into `repo_graph_gate`
//! through this adapter.

use repo_graph_gate::{
	GateAssessmentState, GateAssessmentVerdict, GateBoundaryDeclaration, GateImportEdge,
	GateInference, GateMeasurement, GateModuleViolationEvidence, GateObligation,
	GateQualityAssessmentFact, GateQualityPolicyKind, GateQualityPolicySeverity,
	GateRequirement, GateStorageError, GateStorageRead, GateWaiver,
};
use std::collections::HashMap;

use repo_graph_classification::boundary_evaluator::evaluate_module_boundaries;
use repo_graph_classification::boundary_parser::{
	parse_discovered_module_boundaries, RawBoundaryDeclaration,
};
use repo_graph_classification::module_edges::{
	derive_module_dependency_edges, FileOwnershipFact, ModuleEdgeDerivationInput, ModuleRef,
	ResolvedImportFact,
};

use crate::connection::StorageConnection;

// ── Error mapping helper ─────────────────────────────────────────

/// Convert any `Display`-able error into a `GateStorageError`
/// tagged with the supplied operation identifier. The message
/// body is the error's `Display` output — storage diagnostics
/// are stringified at this boundary and never parsed by the
/// gate layer.
fn map_err<E: std::fmt::Display>(
	operation: &'static str,
) -> impl FnOnce(E) -> GateStorageError {
	move |e| GateStorageError::new(operation, e.to_string())
}

// ── Impl ─────────────────────────────────────────────────────────

impl GateStorageRead for StorageConnection {
	fn get_active_requirements(
		&self,
		repo_uid: &str,
	) -> Result<Vec<GateRequirement>, GateStorageError> {
		let rows = self
			.get_active_requirement_declarations(repo_uid)
			.map_err(map_err("get_active_requirements"))?;
		Ok(rows
			.into_iter()
			.map(|r| GateRequirement {
				req_id: r.req_id,
				version: r.version,
				obligations: r
					.obligations
					.into_iter()
					.map(|o| GateObligation {
						obligation_id: o.obligation_id,
						obligation: o.obligation,
						method: o.method,
						target: o.target,
						threshold: o.threshold,
						operator: o.operator,
					})
					.collect(),
			})
			.collect())
	}

	fn get_boundary_declarations(
		&self,
		repo_uid: &str,
	) -> Result<Vec<GateBoundaryDeclaration>, GateStorageError> {
		let rows = self
			.get_active_boundary_declarations(repo_uid)
			.map_err(map_err("get_boundary_declarations"))?;
		Ok(rows
			.into_iter()
			.map(|b| GateBoundaryDeclaration {
				boundary_module: b.boundary_module,
				forbids: b.forbids,
				reason: b.reason,
			})
			.collect())
	}

	fn find_boundary_imports(
		&self,
		snapshot_uid: &str,
		source_prefix: &str,
		target_prefix: &str,
	) -> Result<Vec<GateImportEdge>, GateStorageError> {
		let rows = self
			.find_imports_between_paths(snapshot_uid, source_prefix, target_prefix)
			.map_err(map_err("find_boundary_imports"))?;
		Ok(rows
			.into_iter()
			.map(|e| GateImportEdge {
				source_file: e.source_file,
				target_file: e.target_file,
			})
			.collect())
	}

	fn get_coverage_measurements(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateMeasurement>, GateStorageError> {
		let rows = self
			.query_measurements_by_kind(snapshot_uid, "line_coverage")
			.map_err(map_err("get_coverage_measurements"))?;
		Ok(rows
			.into_iter()
			.map(|m| GateMeasurement {
				target_stable_key: m.target_stable_key,
				value_json: m.value_json,
			})
			.collect())
	}

	fn get_complexity_measurements(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateMeasurement>, GateStorageError> {
		let rows = self
			.query_measurements_by_kind(snapshot_uid, "cyclomatic_complexity")
			.map_err(map_err("get_complexity_measurements"))?;
		Ok(rows
			.into_iter()
			.map(|m| GateMeasurement {
				target_stable_key: m.target_stable_key,
				value_json: m.value_json,
			})
			.collect())
	}

	fn get_hotspot_inferences(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateInference>, GateStorageError> {
		let rows = self
			.query_inferences_by_kind(snapshot_uid, "hotspot_score")
			.map_err(map_err("get_hotspot_inferences"))?;
		Ok(rows
			.into_iter()
			.map(|i| GateInference {
				target_stable_key: i.target_stable_key,
				value_json: i.value_json,
			})
			.collect())
	}

	fn find_waivers(
		&self,
		repo_uid: &str,
		req_id: &str,
		req_version: i64,
		obligation_id: &str,
		now: &str,
	) -> Result<Vec<GateWaiver>, GateStorageError> {
		let rows = self
			.find_active_waivers(repo_uid, req_id, req_version, obligation_id, now)
			.map_err(map_err("find_waivers"))?;
		Ok(rows
			.into_iter()
			.map(|w| GateWaiver {
				waiver_uid: w.declaration_uid,
				reason: w.reason,
				created_at: w.created_at,
				created_by: w.created_by,
				expires_at: w.expires_at,
				rationale_category: w.rationale_category,
				policy_basis: w.policy_basis,
			})
			.collect())
	}

	fn evaluate_module_violations(
		&self,
		repo_uid: &str,
		snapshot_uid: &str,
	) -> Result<GateModuleViolationEvidence, GateStorageError> {
		// RS-MG-8: Evaluate discovered-module boundary violations.
		//
		// Always repo-wide. Orchestrates RS-MG-1 through RS-MG-4:
		// 1. Build module identity index
		// 2. Derive cross-module edges
		// 3. Parse boundary declarations
		// 4. Evaluate violations

		// RS-MG-1: Load module candidates and build identity index
		let module_candidates = self
			.get_module_candidates_for_snapshot(snapshot_uid)
			.map_err(map_err("evaluate_module_violations"))?;

		let module_index = self
			.build_module_index_by_canonical_path(snapshot_uid)
			.map_err(map_err("evaluate_module_violations"))?;

		// RS-MG-2: Load imports + ownership and derive edges
		let imports = self
			.get_resolved_imports_for_snapshot(snapshot_uid)
			.map_err(map_err("evaluate_module_violations"))?;

		let ownership = self
			.get_file_ownership_for_snapshot(snapshot_uid)
			.map_err(map_err("evaluate_module_violations"))?;

		let derivation_input = ModuleEdgeDerivationInput {
			imports: imports
				.into_iter()
				.map(|i| ResolvedImportFact {
					source_file_uid: i.source_file_uid,
					target_file_uid: i.target_file_uid,
				})
				.collect(),
			ownership: ownership
				.into_iter()
				.map(|o| FileOwnershipFact {
					file_uid: o.file_uid,
					module_uid: o.module_candidate_uid,
				})
				.collect(),
			modules: module_candidates
				.iter()
				.map(|m| ModuleRef {
					module_uid: m.module_candidate_uid.clone(),
					canonical_path: m.canonical_root_path.clone(),
				})
				.collect(),
		};

		let edges = derive_module_dependency_edges(derivation_input)
			.map_err(|e| GateStorageError::new("evaluate_module_violations", e.to_string()))?;

		// RS-MG-3: Load and parse boundary declarations
		let raw_boundaries = self
			.get_active_boundary_declarations_for_repo(repo_uid)
			.map_err(map_err("evaluate_module_violations"))?;

		let raw_boundary_decls: Vec<RawBoundaryDeclaration> = raw_boundaries
			.into_iter()
			.map(|b| RawBoundaryDeclaration {
				declaration_uid: b.declaration_uid,
				value_json: b.value_json,
			})
			.collect();

		let parsed_boundaries = parse_discovered_module_boundaries(&raw_boundary_decls)
			.map_err(|e| GateStorageError::new("evaluate_module_violations", e.to_string()))?;

		// RS-MG-4: Evaluate violations (repo-wide, no filtering)
		let evaluation = evaluate_module_boundaries(&parsed_boundaries, &edges.edges, &module_index);

		Ok(GateModuleViolationEvidence {
			violations_count: evaluation.violations.len(),
			stale_declarations_count: evaluation.stale_declarations.len(),
		})
	}

	fn get_quality_assessment_facts_for_gate(
		&self,
		repo_uid: &str,
		snapshot_uid: &str,
	) -> Result<Vec<GateQualityAssessmentFact>, GateStorageError> {
		// Step 1: Load active quality-policy declarations (typed).
		let policies = self
			.get_active_quality_policy_declarations(repo_uid)
			.map_err(map_err("get_quality_assessment_facts_for_gate"))?;

		// Step 2: Load assessment rows for this snapshot.
		let assessments = self
			.get_quality_assessments_for_snapshot(snapshot_uid)
			.map_err(map_err("get_quality_assessment_facts_for_gate"))?;

		// Step 3: Build lookup of assessments by policy_uid.
		//
		// Invariant: For gate purposes, exactly one assessment should exist
		// per (policy_uid, snapshot_uid). Multiple rows indicate:
		//   - Storage corruption
		//   - Multiple baselines computed without cleanup
		//   - Assessment pipeline bug
		//
		// Rather than silently pick one and hide ambiguity, fail fast so the
		// problem is surfaced. This is an enforcement surface.
		let mut assessment_lookup: HashMap<&str, &crate::types::QualityAssessmentRow> =
			HashMap::new();
		for assessment in &assessments {
			if assessment_lookup.contains_key(assessment.policy_uid.as_str()) {
				return Err(GateStorageError::new(
					"get_quality_assessment_facts_for_gate",
					format!(
						"multiple assessment rows for policy_uid={} in snapshot={}; \
						 gate requires exactly one assessment per policy",
						assessment.policy_uid, snapshot_uid
					),
				));
			}
			assessment_lookup.insert(&assessment.policy_uid, assessment);
		}

		// Step 4: Build one fact per active policy.
		let mut facts = Vec::with_capacity(policies.len());
		for policy in &policies {
			let assessment = assessment_lookup.get(policy.declaration_uid.as_str());

			// Map policy_kind from storage to gate-owned enum.
			let policy_kind = map_policy_kind(&policy.payload.policy_kind);

			// Map severity from storage to gate-owned enum.
			let severity = map_severity(&policy.payload.severity);

			if let Some(assessment) = assessment {
				// Assessment exists - map to fact with Present state.
				// Validate stored data; malformed rows are storage corruption.
				let computed_verdict = map_verdict(&assessment.computed_verdict)
					.map_err(|reason| {
						GateStorageError::new(
							"get_quality_assessment_facts_for_gate",
							format!(
								"malformed assessment row {}: {}",
								assessment.assessment_uid, reason
							),
						)
					})?;

				let violations_count = parse_violations_count(&assessment.violations_json)
					.map_err(|reason| {
						GateStorageError::new(
							"get_quality_assessment_facts_for_gate",
							format!(
								"malformed assessment row {}: {}",
								assessment.assessment_uid, reason
							),
						)
					})?;

				facts.push(GateQualityAssessmentFact {
					policy_uid: policy.declaration_uid.clone(),
					policy_id: policy.payload.policy_id.clone(),
					policy_version: policy.payload.version,
					policy_kind,
					severity,
					assessment_state: GateAssessmentState::Present,
					computed_verdict: Some(computed_verdict),
					baseline_snapshot_uid: assessment.baseline_snapshot_uid.clone(),
					measurements_evaluated: Some(assessment.measurements_evaluated),
					violations_count: Some(violations_count),
				});
			} else {
				// No assessment - fact with Missing state.
				facts.push(GateQualityAssessmentFact {
					policy_uid: policy.declaration_uid.clone(),
					policy_id: policy.payload.policy_id.clone(),
					policy_version: policy.payload.version,
					policy_kind,
					severity,
					assessment_state: GateAssessmentState::Missing,
					computed_verdict: None,
					baseline_snapshot_uid: None,
					measurements_evaluated: None,
					violations_count: None,
				});
			}
		}

		// Sort by policy_id for deterministic output.
		facts.sort_by(|a, b| a.policy_id.cmp(&b.policy_id));

		Ok(facts)
	}
}

// ── Mapping helpers for quality-policy types ────────────────────────

fn map_policy_kind(kind: &crate::types::QualityPolicyKind) -> GateQualityPolicyKind {
	match kind {
		crate::types::QualityPolicyKind::AbsoluteMax => GateQualityPolicyKind::AbsoluteMax,
		crate::types::QualityPolicyKind::AbsoluteMin => GateQualityPolicyKind::AbsoluteMin,
		crate::types::QualityPolicyKind::NoNew => GateQualityPolicyKind::NoNew,
		crate::types::QualityPolicyKind::NoWorsened => GateQualityPolicyKind::NoWorsened,
	}
}

fn map_severity(severity: &crate::types::QualityPolicySeverity) -> GateQualityPolicySeverity {
	match severity {
		crate::types::QualityPolicySeverity::Fail => GateQualityPolicySeverity::Fail,
		crate::types::QualityPolicySeverity::Advisory => GateQualityPolicySeverity::Advisory,
	}
}

/// Parse verdict string into gate enum. Returns error for unknown values.
///
/// Gate is an enforcement surface. Malformed stored data must fail
/// loudly, not be silently normalized into plausible values.
fn map_verdict(verdict_str: &str) -> Result<GateAssessmentVerdict, String> {
	match verdict_str {
		"PASS" => Ok(GateAssessmentVerdict::Pass),
		"FAIL" => Ok(GateAssessmentVerdict::Fail),
		"NOT_APPLICABLE" => Ok(GateAssessmentVerdict::NotApplicable),
		"NOT_COMPARABLE" => Ok(GateAssessmentVerdict::NotComparable),
		other => Err(format!("unknown computed_verdict '{}'", other)),
	}
}

/// Parse violations JSON array and return count. Returns error for malformed JSON.
///
/// Gate is an enforcement surface. Malformed stored data must fail
/// loudly, not be silently normalized into plausible values.
fn parse_violations_count(violations_json: &str) -> Result<usize, String> {
	serde_json::from_str::<Vec<serde_json::Value>>(violations_json)
		.map(|arr| arr.len())
		.map_err(|e| format!("invalid violations_json: {}", e))
}
