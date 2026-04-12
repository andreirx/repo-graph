//! Trust service — two-layer computation + assembly.
//!
//! Layer 1 (pure): `compute_trust_report` takes a fully-assembled
//! `TrustComputationInput` and returns a `TrustReport`. No storage
//! access, no I/O, no SQL knowledge.
//!
//! Layer 2 (storage-backed): `assemble_trust_report` takes a
//! `&impl TrustStorageRead` implementor and builds the input by
//! pulling raw data through the trait, then delegates to layer 1.
//!
//! Mirror of `src/core/trust/service.ts`.
//!
//! ── Lock deviation: `human_label_for_category` ───────────────
//!
//! The R4 lock listed `humanLabelForCategory` as "display-time
//! rendering, stays TS." However, the TS `computeTrustReport`
//! calls this function inside the report builder to populate
//! `TrustCategoryRow.label`. The label is part of the `TrustReport`
//! DTO parity surface, not a CLI display concern. Without it, the
//! Rust report produces different category rows than the TS report.
//!
//! The function is 8 static string mappings plus a fallback. It is
//! ported here as `pub(crate)` — a narrow DTO-completeness
//! exception, not a general invitation to port display helpers.

use repo_graph_classification::types::{BlastRadiusLevel, UnresolvedEdgeCategory};
use repo_graph_classification::derive_blast_radius;

use crate::rules::{
	self, count_suspicious_zero_connectivity_modules, group_path_prefix_cycles_by_ancestor,
	sum_unresolved_calls, sum_unresolved_imports, ModuleForSuspicionCheck, PathPrefixCycleInput,
};
use crate::storage_port::{
	ClassificationCountRow, PathPrefixModuleCycle, TrustModuleStats, TrustStorageRead,
	TrustUnresolvedEdgeSample, UnresolvedEdgeClassification,
};
use crate::types::{
	EnrichmentStatus, EnrichmentTopType, ExtractionDiagnostics, ModuleTrustRow,
	ReliabilityLevel, TrustCategoryRow, TrustClassificationRow, TrustDowngrades,
	TrustReliability, TrustReport, TrustSummary, UnknownCallsBlastRadiusBreakdown,
};

// ── Error type for assembly layer ────────────────────────────────

/// Error from the storage-backed assembly layer.
///
/// `Storage(E)` wraps errors from `TrustStorageRead` methods.
/// `JsonParse` wraps failures when parsing JSON strings
/// (diagnostics_json, toolchain_json) into typed Rust structs.
///
/// The bound `E: Display` is required only for the `Display` impl
/// on `TrustAssemblyError` itself. No `Debug` bound is imposed on
/// `E` at the type level; the `#[derive(Debug)]` works because
/// `E` is constrained to `Debug` only where the derive is used.
#[derive(Debug)]
pub enum TrustAssemblyError<E> {
	/// A `TrustStorageRead` method returned an error.
	Storage(E),
	/// A JSON field contained malformed JSON.
	JsonParse {
		field: &'static str,
		source: serde_json::Error,
	},
}

impl<E: std::fmt::Display> std::fmt::Display for TrustAssemblyError<E> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Storage(e) => write!(f, "storage error: {}", e),
			Self::JsonParse { field, source } => {
				write!(f, "failed to parse {}: {}", field, source)
			}
		}
	}
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for TrustAssemblyError<E> {}

// ── Input DTO for pure computation ───────────────────────────────

/// Fully-assembled data bundle for `compute_trust_report`.
///
/// Every field is pre-fetched and pre-parsed. The pure computation
/// layer does not do I/O, SQL, or JSON parsing of external strings
/// (except `metadata_json` on unresolved edge samples, which is
/// parsed inline with silent-ignore on failure, matching TS).
///
/// The assembly function (`assemble_trust_report`) builds this
/// from `TrustStorageRead` queries + JSON parsing.
pub struct TrustComputationInput {
	// ── Identity ──────────────────────────────────────────────
	pub snapshot_uid: String,
	pub basis_commit: Option<String>,

	// ── Pre-parsed from storage JSON strings ──────────────────
	pub toolchain: Option<serde_json::Map<String, serde_json::Value>>,
	pub diagnostics: Option<ExtractionDiagnostics>,

	// ── Storage reads (already fetched) ───────────────────────
	pub file_paths: Vec<String>,
	pub module_stats: Vec<TrustModuleStats>,
	pub path_prefix_cycles: Vec<PathPrefixModuleCycle>,
	pub active_entrypoint_count: usize,
	pub resolved_calls: u64,

	// ── Classification data (pre-fetched) ─────────────────────
	/// Classification counts filtered to CALLS-family categories.
	/// Used for Variant A reweighting (external vs internal-like).
	pub calls_classification_counts: Vec<ClassificationCountRow>,
	/// Classification counts unfiltered (all categories).
	/// Used for the classifications section of the report.
	pub all_classification_counts: Vec<ClassificationCountRow>,
	/// Unresolved edge samples with classification = "unknown".
	/// Used for blast-radius breakdown and enrichment status.
	/// Empty if `all_classification_counts` was empty (assembly
	/// skips the query in that case).
	pub unknown_calls_samples: Vec<TrustUnresolvedEdgeSample>,
}

// ── Human labels (lock deviation, DTO-completeness) ──────────────

/// Map a category key to its human-readable label.
///
/// Mirror of `humanLabelForCategory` from
/// `src/core/diagnostics/unresolved-edge-categories.ts:51`.
///
/// **Lock deviation:** the R4 lock listed this as "display-time
/// rendering, stays TS." Ported here because the TS
/// `computeTrustReport` calls it inside the report builder, making
/// the label part of the `TrustReport` DTO parity surface. This
/// is a narrow DTO-completeness exception. The function stays
/// `pub(crate)` — it is not part of the public API.
pub(crate) fn human_label_for_category(category: &str) -> String {
	match category {
		"imports_file_not_found" => "IMPORTS (file not found)".into(),
		"instantiates_class_not_found" => "INSTANTIATES (class not found)".into(),
		"implements_interface_not_found" => "IMPLEMENTS (interface not found)".into(),
		"calls_this_wildcard_method_needs_type_info" => {
			"CALLS this.*.method (needs type info)".into()
		}
		"calls_this_method_needs_class_context" => {
			"CALLS this.method (needs class context)".into()
		}
		"calls_obj_method_needs_type_info" => "CALLS obj.method (needs type info)".into(),
		"calls_function_ambiguous_or_missing" => {
			"CALLS function (ambiguous or missing)".into()
		}
		"other" => "OTHER (unclassified)".into(),
		_ => category.to_string(),
	}
}

// ── Caveats builder ──────────────────────────────────────────────

/// Build the caveats list based on reliability levels.
///
/// Mirror of `buildCaveats` from `service.ts:366`.
fn build_caveats(
	diagnostics_available: bool,
	import_graph_level: ReliabilityLevel,
	call_graph_level: ReliabilityLevel,
	dead_code_level: ReliabilityLevel,
	change_impact_level: ReliabilityLevel,
) -> Vec<String> {
	let mut caveats = Vec::new();
	if !diagnostics_available {
		caveats.push(
			"Extraction diagnostics unavailable for this snapshot. Re-index to populate."
				.into(),
		);
	}
	if call_graph_level != ReliabilityLevel::HIGH {
		caveats.push(format!(
			"Call-graph reliability is {:?} on this repo. \
			 Do not use callers/callees for safety-critical decisions without verification.",
			call_graph_level
		));
	}
	if dead_code_level != ReliabilityLevel::HIGH {
		caveats.push(format!(
			"Dead-code reliability is {:?} on this repo. \
			 Treat `graph dead` results as 'graph orphans' requiring human arbitration, \
			 not deletion candidates.",
			dead_code_level
		));
	}
	if import_graph_level != ReliabilityLevel::HIGH {
		caveats.push(format!(
			"Import-graph reliability is {:?}. \
			 Module fan-in/fan-out and change-impact propagation may undercount relationships.",
			import_graph_level
		));
	}
	if change_impact_level != ReliabilityLevel::HIGH {
		caveats.push(format!(
			"Change-impact reliability is {:?}. \
			 Impacted-module sets may be incomplete on this repo.",
			change_impact_level
		));
	}
	caveats.push(
		"Cycle payloads currently emit leaf module names only; \
		 full stable keys are not in the user-facing `graph cycles` output."
			.into(),
	);
	caveats
}

// ── Layer 1: Pure computation ────────────────────────────────────

/// Compute a `TrustReport` from a fully-assembled data bundle.
///
/// This function is PURE. No storage access, no I/O, no SQL.
/// All data is pre-fetched in `TrustComputationInput`.
///
/// Mirror of `computeTrustReport` from `service.ts:63`, with the
/// storage-fetching phase factored out into `assemble_trust_report`.
pub fn compute_trust_report(input: &TrustComputationInput) -> TrustReport {
	let diagnostics_available = input.diagnostics.is_some();

	// ── Phase 1: Detection rules ─────────────────────────────
	let framework_heavy = rules::detect_framework_heavy_suspicion(&input.file_paths);

	let module_stats_for_rules: Vec<ModuleForSuspicionCheck> = input
		.module_stats
		.iter()
		.map(|m| ModuleForSuspicionCheck {
			qualified_name: m.path.clone(),
			fan_in: m.fan_in,
			fan_out: m.fan_out,
			file_count: m.file_count,
		})
		.collect();
	let suspicious_module_count =
		count_suspicious_zero_connectivity_modules(&module_stats_for_rules);

	let alias_resolution = rules::detect_alias_resolution_suspicion(suspicious_module_count);

	let cycle_inputs: Vec<PathPrefixCycleInput> = input
		.path_prefix_cycles
		.iter()
		.map(|c| PathPrefixCycleInput {
			ancestor_stable_key: c.ancestor_stable_key.clone(),
		})
		.collect();
	let cycles_by_ancestor = group_path_prefix_cycles_by_ancestor(&cycle_inputs);
	let total_cycles = input.path_prefix_cycles.len();
	let registry_pattern =
		rules::detect_registry_pattern_suspicion(&cycles_by_ancestor, total_cycles);

	let missing_entrypoints =
		rules::detect_missing_entrypoint_declarations(input.active_entrypoint_count);

	// ── Phase 2: Reliability formulas ────────────────────────
	let unresolved_calls = input
		.diagnostics
		.as_ref()
		.map(|d| sum_unresolved_calls(d))
		.unwrap_or(0);
	let unresolved_imports = input
		.diagnostics
		.as_ref()
		.map(|d| sum_unresolved_imports(d))
		.unwrap_or(0);

	// Variant A reweighting: external_library_candidate calls are
	// excluded from the internal-like denominator.
	let unresolved_calls_external = input
		.calls_classification_counts
		.iter()
		.find(|r| r.classification == UnresolvedEdgeClassification::ExternalLibraryCandidate)
		.map(|r| r.count)
		.unwrap_or(0);
	let unresolved_calls_internal_like = unresolved_calls.saturating_sub(unresolved_calls_external);

	let import_graph_reliability = rules::compute_import_graph_reliability(
		alias_resolution.triggered,
		registry_pattern.triggered,
		unresolved_imports,
	);

	let call_graph_reliability = rules::compute_call_graph_reliability(
		input.resolved_calls,
		unresolved_calls_internal_like,
	);

	let dead_code_reliability = rules::compute_dead_code_reliability(
		missing_entrypoints.triggered,
		registry_pattern.triggered,
		framework_heavy.triggered,
		call_graph_reliability.level,
	);

	let change_impact_reliability = rules::compute_change_impact_reliability(
		alias_resolution.triggered,
		registry_pattern.triggered,
		import_graph_reliability.level,
	);

	// ── Phase 3: Category rows ───────────────────────────────
	let mut categories: Vec<TrustCategoryRow> = match &input.diagnostics {
		Some(diag) => diag
			.unresolved_breakdown
			.iter()
			.map(|(category, &unresolved)| TrustCategoryRow {
				label: human_label_for_category(category),
				category: category.clone(),
				unresolved,
			})
			.collect(),
		None => vec![],
	};
	// Sort: unresolved desc, then category asc as tie-break.
	categories.sort_by(|a, b| {
		b.unresolved
			.cmp(&a.unresolved)
			.then_with(|| a.category.cmp(&b.category))
	});

	// ── Phase 4: Classification rows ─────────────────────────
	let mut classifications: Vec<TrustClassificationRow> = input
		.all_classification_counts
		.iter()
		.map(|r| {
			// Serialize the typed enum to its snake_case string for
			// the report DTO (TrustClassificationRow.classification
			// is a String, matching the TS output format).
			let classification_str = serde_json::to_value(&r.classification)
				.ok()
				.and_then(|v| match v {
					serde_json::Value::String(s) => Some(s),
					_ => None,
				})
				.unwrap_or_else(|| format!("{:?}", r.classification));
			TrustClassificationRow {
				classification: classification_str,
				count: r.count,
			}
		})
		.collect();
	// Sort: count desc, then classification asc as tie-break.
	classifications.sort_by(|a, b| {
		b.count
			.cmp(&a.count)
			.then_with(|| a.classification.cmp(&b.classification))
	});

	// ── Phase 5: Blast radius + enrichment ───────────────────
	let (unknown_calls_blast_radius, enrichment_status) =
		if !input.all_classification_counts.is_empty() {
			compute_blast_radius_and_enrichment(&input.unknown_calls_samples)
		} else {
			(None, None)
		};

	// ── Phase 6: Module rows ─────────────────────────────────
	// Explicit sort by qualified_name for deterministic output.
	// The storage SQL happens to ORDER BY qualified_name, but
	// this is a public pure function — any caller can build
	// TrustComputationInput in arbitrary order. The sort here
	// guarantees the same logical input always produces the
	// same output regardless of input ordering.
	let mut modules: Vec<ModuleTrustRow> = input
		.module_stats
		.iter()
		.map(|m| {
			let suspicious = m.fan_in == 0
				&& m.fan_out == 0
				&& m.file_count >= 2
				&& m.path != "."
				&& !m.path.is_empty();
			let mut trust_notes = Vec::new();
			if suspicious {
				trust_notes.push("alias_resolution_candidate".to_string());
			}
			ModuleTrustRow {
				module_stable_key: m.stable_key.clone(),
				qualified_name: m.path.clone(),
				fan_in: m.fan_in,
				fan_out: m.fan_out,
				file_count: m.file_count,
				suspicious_zero_connectivity: suspicious,
				trust_notes,
			}
		})
		.collect();
	modules.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

	// ── Phase 7: Caveats ─────────────────────────────────────
	let caveats = build_caveats(
		diagnostics_available,
		import_graph_reliability.level,
		call_graph_reliability.level,
		dead_code_reliability.level,
		change_impact_reliability.level,
	);

	// ── Phase 8: Call resolution rate ─────────────────────────
	let call_resolution_rate = {
		let total = input.resolved_calls + unresolved_calls_internal_like;
		if total > 0 {
			input.resolved_calls as f64 / total as f64
		} else {
			1.0
		}
	};

	// ── Assemble report ──────────────────────────────────────
	TrustReport {
		snapshot_uid: input.snapshot_uid.clone(),
		basis_commit: input.basis_commit.clone(),
		toolchain: input.toolchain.clone(),
		diagnostics_version: input.diagnostics.as_ref().map(|d| d.diagnostics_version),
		summary: TrustSummary {
			edges_total: input.diagnostics.as_ref().map(|d| d.edges_total).unwrap_or(0),
			edges_resolved: input.diagnostics.as_ref().map(|d| d.edges_total).unwrap_or(0),
			unresolved_total: input
				.diagnostics
				.as_ref()
				.map(|d| d.unresolved_total)
				.unwrap_or(0),
			resolved_calls: input.resolved_calls,
			unresolved_calls,
			unresolved_calls_external,
			unresolved_calls_internal_like,
			call_resolution_rate,
			reliability: TrustReliability {
				import_graph: import_graph_reliability,
				call_graph: call_graph_reliability,
				dead_code: dead_code_reliability,
				change_impact: change_impact_reliability,
			},
			triggered_downgrades: TrustDowngrades {
				framework_heavy_suspicion: framework_heavy,
				registry_pattern_suspicion: registry_pattern,
				missing_entrypoint_declarations: missing_entrypoints,
				alias_resolution_suspicion: alias_resolution,
			},
		},
		categories,
		classifications,
		unknown_calls_blast_radius,
		enrichment_status,
		modules,
		caveats,
		diagnostics_available,
	}
}

// ── Blast-radius + enrichment computation ────────────────────────

/// Compute the blast-radius breakdown and enrichment status from
/// unknown-classified CALLS samples.
///
/// Mirrors the inner loop in `computeTrustReport` at
/// service.ts:214-288. Uses typed `UnresolvedEdgeCategory` and
/// `derive_blast_radius` from the classification crate — no raw
/// string comparisons where typed enums exist.
fn compute_blast_radius_and_enrichment(
	samples: &[TrustUnresolvedEdgeSample],
) -> (
	Option<UnknownCallsBlastRadiusBreakdown>,
	Option<EnrichmentStatus>,
) {
	let mut breakdown = UnknownCallsBlastRadiusBreakdown {
		low: 0,
		medium: 0,
		high: 0,
	};
	let mut enriched_count: u64 = 0;
	let mut eligible_count: u64 = 0;
	let mut enrichment_was_run = false;

	// BTreeMap for deterministic ordering of type counts.
	let mut type_counts: std::collections::BTreeMap<String, (u64, bool)> =
		std::collections::BTreeMap::new();

	for sample in samples {
		// Filter to CALLS-family only (typed check, not string).
		if !sample.category.is_calls_category() {
			continue;
		}

		// Derive blast radius per-row.
		let assessment = derive_blast_radius(
			sample.category,
			sample.basis_code,
			sample.source_node_visibility.as_deref(),
		);
		match assessment.blast_radius {
			BlastRadiusLevel::Low => breakdown.low += 1,
			BlastRadiusLevel::Medium => breakdown.medium += 1,
			BlastRadiusLevel::High => breakdown.high += 1,
			BlastRadiusLevel::NotApplicable => {}
		}

		// Enrichment status: only for calls_obj_method_needs_type_info.
		if sample.category == UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo {
			eligible_count += 1;
			if let Some(ref meta_str) = sample.metadata_json {
				// Silent-ignore on malformed JSON, matching TS try-catch.
				if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
					if let Some(enrichment) = meta.get("enrichment") {
						enrichment_was_run = true;
						if enrichment.get("receiverType").is_some() {
							enriched_count += 1;
							let type_name = enrichment
								.get("typeDisplayName")
								.and_then(|v| v.as_str())
								.or_else(|| {
									enrichment.get("receiverType").and_then(|v| v.as_str())
								})
								.unwrap_or("")
								.to_string();
							let is_ext = enrichment
								.get("isExternalType")
								.and_then(|v| v.as_bool())
								.unwrap_or(false);
							let entry = type_counts.entry(type_name).or_insert((0, is_ext));
							entry.0 += 1;
						}
					}
				}
			}
		}
	}

	let blast_radius = Some(breakdown);

	// Build enrichment status only if enrichment was actually run.
	// Null = no enrichment markers found.
	// Populated with enriched=0 = enrichment ran but resolved zero types.
	let enrichment = if enrichment_was_run {
		let mut top_types: Vec<EnrichmentTopType> = type_counts
			.into_iter()
			.map(|(type_name, (count, is_external))| EnrichmentTopType {
				type_name,
				count,
				is_external,
			})
			.collect();
		// Sort: count desc, then type_name asc as tie-break.
		top_types.sort_by(|a, b| {
			b.count
				.cmp(&a.count)
				.then_with(|| a.type_name.cmp(&b.type_name))
		});
		top_types.truncate(15);
		Some(EnrichmentStatus {
			eligible: eligible_count,
			enriched: enriched_count,
			top_types,
		})
	} else {
		None
	};

	(blast_radius, enrichment)
}

// ── Layer 2: Storage-backed assembly ─────────────────────────────

/// Fetch data from storage, parse JSON, and delegate to
/// `compute_trust_report`.
///
/// This is the thin orchestration layer. It calls
/// `TrustStorageRead` methods, parses the JSON strings, and
/// builds the `TrustComputationInput`. The actual report
/// computation is pure and happens in `compute_trust_report`.
///
/// Mirrors `computeTrustReport` from `service.ts:63` — the
/// storage-fetching parts only.
pub fn assemble_trust_report<S: TrustStorageRead>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
	basis_commit: Option<&str>,
	toolchain_json: Option<&str>,
) -> Result<TrustReport, TrustAssemblyError<S::Error>> {
	// ── Parse JSON strings ───────────────────────────────────
	let diagnostics: Option<ExtractionDiagnostics> = {
		let json_str = storage
			.get_snapshot_extraction_diagnostics(snapshot_uid)
			.map_err(TrustAssemblyError::Storage)?;
		match json_str {
			Some(s) => {
				let parsed = serde_json::from_str(&s).map_err(|e| {
					TrustAssemblyError::JsonParse {
						field: "extraction_diagnostics_json",
						source: e,
					}
				})?;
				Some(parsed)
			}
			None => None,
		}
	};

	let toolchain: Option<serde_json::Map<String, serde_json::Value>> = match toolchain_json {
		Some(s) => {
			let parsed = serde_json::from_str(s).map_err(|e| TrustAssemblyError::JsonParse {
				field: "toolchain_json",
				source: e,
			})?;
			Some(parsed)
		}
		None => None,
	};

	// ── Fetch from storage ───────────────────────────────────
	let file_paths = storage
		.get_file_paths_by_repo(repo_uid)
		.map_err(TrustAssemblyError::Storage)?;

	let module_stats = storage
		.compute_module_stats(snapshot_uid)
		.map_err(TrustAssemblyError::Storage)?;

	let path_prefix_cycles = storage
		.find_path_prefix_module_cycles(snapshot_uid)
		.map_err(TrustAssemblyError::Storage)?;

	let active_entrypoint_count = storage
		.count_active_declarations(repo_uid, "entrypoint")
		.map_err(TrustAssemblyError::Storage)?;

	let resolved_calls = storage
		.count_edges_by_type(snapshot_uid, "CALLS")
		.map_err(TrustAssemblyError::Storage)?;

	// CALLS-family classification counts (Variant A reweighting).
	let calls_filter = vec![
		UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo,
		UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext,
		UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
		UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
	];
	let calls_classification_counts = storage
		.count_unresolved_edges_by_classification(
			&crate::storage_port::CountByClassificationInput {
				snapshot_uid: snapshot_uid.into(),
				filter_categories: calls_filter,
			},
		)
		.map_err(TrustAssemblyError::Storage)?;

	// All classification counts (no filter).
	let all_classification_counts = storage
		.count_unresolved_edges_by_classification(
			&crate::storage_port::CountByClassificationInput {
				snapshot_uid: snapshot_uid.into(),
				filter_categories: vec![],
			},
		)
		.map_err(TrustAssemblyError::Storage)?;

	// Unknown CALLS samples (conditional: only if there are
	// classification counts).
	let unknown_calls_samples = if !all_classification_counts.is_empty() {
		storage
			.query_unresolved_edges(&crate::storage_port::QueryUnresolvedEdgesInput {
				snapshot_uid: snapshot_uid.into(),
				classification: UnresolvedEdgeClassification::Unknown,
				limit: 100_000,
			})
			.map_err(TrustAssemblyError::Storage)?
	} else {
		vec![]
	};

	// ── Delegate to pure computation ─────────────────────────
	let input = TrustComputationInput {
		snapshot_uid: snapshot_uid.to_string(),
		basis_commit: basis_commit.map(|s| s.to_string()),
		toolchain,
		diagnostics,
		file_paths,
		module_stats,
		path_prefix_cycles,
		active_entrypoint_count,
		resolved_calls,
		calls_classification_counts,
		all_classification_counts,
		unknown_calls_samples,
	};

	Ok(compute_trust_report(&input))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use crate::storage_port::{
		ClassificationCountRow, CountByClassificationInput, PathPrefixModuleCycle,
		QueryUnresolvedEdgesInput, TrustModuleStats, TrustUnresolvedEdgeSample,
		UnresolvedEdgeBasisCode, UnresolvedEdgeClassification,
	};
	use std::collections::BTreeMap;

	/// Build a minimal input with all-empty data. Produces a report
	/// with HIGH reliability everywhere (no downgrades, no caveats
	/// except the permanent cycle caveat).
	fn minimal_input() -> TrustComputationInput {
		TrustComputationInput {
			snapshot_uid: "snap1".into(),
			basis_commit: None,
			toolchain: None,
			diagnostics: None,
			file_paths: vec![],
			module_stats: vec![],
			path_prefix_cycles: vec![],
			active_entrypoint_count: 1, // >0 to avoid missing_entrypoint trigger
			resolved_calls: 0,
			calls_classification_counts: vec![],
			all_classification_counts: vec![],
			unknown_calls_samples: vec![],
		}
	}

	// ── Pure computation: minimal ────────────────────────────

	#[test]
	fn minimal_input_produces_all_high_reliability() {
		let report = compute_trust_report(&minimal_input());
		assert_eq!(report.snapshot_uid, "snap1");
		assert_eq!(report.summary.reliability.import_graph.level, ReliabilityLevel::HIGH);
		assert_eq!(report.summary.reliability.call_graph.level, ReliabilityLevel::HIGH);
		assert_eq!(report.summary.reliability.dead_code.level, ReliabilityLevel::HIGH);
		assert_eq!(report.summary.reliability.change_impact.level, ReliabilityLevel::HIGH);
		assert!(!report.diagnostics_available);
		assert_eq!(report.categories.len(), 0);
		assert_eq!(report.classifications.len(), 0);
		assert!(report.unknown_calls_blast_radius.is_none());
		assert!(report.enrichment_status.is_none());
	}

	#[test]
	fn minimal_input_has_permanent_cycle_caveat() {
		let report = compute_trust_report(&minimal_input());
		assert!(report.caveats.iter().any(|c| c.contains("Cycle payloads")));
		// Only the permanent caveat (no diagnostics caveat because
		// diagnostics_available is false, which adds its own).
		// diagnostics=None → diagnostics_available=false → caveat added.
		assert!(report.caveats.iter().any(|c| c.contains("Extraction diagnostics unavailable")));
	}

	// ── Framework heavy → dead code LOW ──────────────────────

	#[test]
	fn framework_heavy_triggers_dead_code_low() {
		let mut input = minimal_input();
		// 5 tsx files out of 10 → 50% ratio, well above 20% threshold.
		input.file_paths = (0..5)
			.map(|i| format!("src/component_{}.tsx", i))
			.chain((0..5).map(|i| format!("src/util_{}.ts", i)))
			.collect();
		let report = compute_trust_report(&input);
		assert!(report.summary.triggered_downgrades.framework_heavy_suspicion.triggered);
		assert_eq!(report.summary.reliability.dead_code.level, ReliabilityLevel::LOW);
	}

	// ── Variant A reweighting ────────────────────────────────

	#[test]
	fn variant_a_reweighting_excludes_external_calls() {
		let mut input = minimal_input();
		let mut breakdown = BTreeMap::new();
		breakdown.insert("calls_obj_method_needs_type_info".into(), 100);
		input.diagnostics = Some(ExtractionDiagnostics {
			diagnostics_version: 1,
			edges_total: 200,
			unresolved_total: 100,
			unresolved_breakdown: breakdown,
		});
		input.resolved_calls = 50;
		// 80 of the 100 unresolved are external_library_candidate.
		input.calls_classification_counts = vec![
			ClassificationCountRow {
				classification: UnresolvedEdgeClassification::ExternalLibraryCandidate,
				count: 80,
			},
			ClassificationCountRow {
				classification: UnresolvedEdgeClassification::Unknown,
				count: 20,
			},
		];

		let report = compute_trust_report(&input);
		// Internal-like = 100 - 80 = 20. Rate = 50 / (50 + 20) ≈ 0.714.
		assert_eq!(report.summary.unresolved_calls_external, 80);
		assert_eq!(report.summary.unresolved_calls_internal_like, 20);
		// 71.4% is between 50% and 85% → MEDIUM.
		assert_eq!(
			report.summary.reliability.call_graph.level,
			ReliabilityLevel::MEDIUM
		);
	}

	// ── Category rows sorted by unresolved desc ──────────────

	#[test]
	fn category_rows_sorted_by_unresolved_desc() {
		let mut input = minimal_input();
		let mut breakdown = BTreeMap::new();
		breakdown.insert("calls_obj_method_needs_type_info".into(), 5);
		breakdown.insert("imports_file_not_found".into(), 20);
		breakdown.insert("other".into(), 1);
		input.diagnostics = Some(ExtractionDiagnostics {
			diagnostics_version: 1,
			edges_total: 100,
			unresolved_total: 26,
			unresolved_breakdown: breakdown,
		});

		let report = compute_trust_report(&input);
		assert_eq!(report.categories.len(), 3);
		assert_eq!(report.categories[0].category, "imports_file_not_found");
		assert_eq!(report.categories[0].unresolved, 20);
		assert_eq!(report.categories[0].label, "IMPORTS (file not found)");
		assert_eq!(report.categories[1].category, "calls_obj_method_needs_type_info");
		assert_eq!(report.categories[1].unresolved, 5);
		assert_eq!(report.categories[2].category, "other");
		assert_eq!(report.categories[2].unresolved, 1);
	}

	// ── Classification rows sorted by count desc ─────────────

	#[test]
	fn classification_rows_sorted_by_count_desc_then_key_asc() {
		let mut input = minimal_input();
		input.all_classification_counts = vec![
			ClassificationCountRow {
				classification: UnresolvedEdgeClassification::Unknown,
				count: 10,
			},
			ClassificationCountRow {
				classification: UnresolvedEdgeClassification::ExternalLibraryCandidate,
				count: 10,
			},
			ClassificationCountRow {
				classification: UnresolvedEdgeClassification::InternalCandidate,
				count: 5,
			},
		];

		let report = compute_trust_report(&input);
		assert_eq!(report.classifications.len(), 3);
		// Same count (10) → sorted by classification asc.
		assert_eq!(report.classifications[0].classification, "external_library_candidate");
		assert_eq!(report.classifications[1].classification, "unknown");
		assert_eq!(report.classifications[2].classification, "internal_candidate");
		assert_eq!(report.classifications[2].count, 5);
	}

	// ── Blast radius breakdown ───────────────────────────────

	#[test]
	fn blast_radius_counts_per_level() {
		let mut input = minimal_input();
		input.all_classification_counts = vec![ClassificationCountRow {
			classification: UnresolvedEdgeClassification::Unknown,
			count: 3,
		}];
		input.unknown_calls_samples = vec![
			// External import → low blast radius.
			TrustUnresolvedEdgeSample {
				category: UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
				basis_code: UnresolvedEdgeBasisCode::CalleeMatchesExternalImport,
				source_node_visibility: Some("export".into()),
				metadata_json: None,
			},
			// Same-file symbol, exported → low blast radius.
			TrustUnresolvedEdgeSample {
				category: UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
				basis_code: UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol,
				source_node_visibility: Some("export".into()),
				metadata_json: None,
			},
			// Internal import → medium blast radius.
			TrustUnresolvedEdgeSample {
				category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
				basis_code: UnresolvedEdgeBasisCode::ReceiverMatchesInternalImport,
				source_node_visibility: Some("export".into()),
				metadata_json: None,
			},
		];

		let report = compute_trust_report(&input);
		let br = report.unknown_calls_blast_radius.unwrap();
		assert_eq!(br.low, 2);
		assert_eq!(br.medium, 1);
		assert_eq!(br.high, 0);
	}

	// ── Enrichment status ────────────────────────────────────

	#[test]
	fn enrichment_null_when_no_enrichment_markers() {
		let mut input = minimal_input();
		input.all_classification_counts = vec![ClassificationCountRow {
			classification: UnresolvedEdgeClassification::Unknown,
			count: 1,
		}];
		input.unknown_calls_samples = vec![TrustUnresolvedEdgeSample {
			category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
			source_node_visibility: None,
			metadata_json: None, // No metadata → no enrichment marker.
		}];

		let report = compute_trust_report(&input);
		assert!(report.enrichment_status.is_none());
	}

	#[test]
	fn enrichment_populated_when_markers_present() {
		let mut input = minimal_input();
		input.all_classification_counts = vec![ClassificationCountRow {
			classification: UnresolvedEdgeClassification::Unknown,
			count: 2,
		}];
		input.unknown_calls_samples = vec![
			TrustUnresolvedEdgeSample {
				category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
				basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
				source_node_visibility: None,
				metadata_json: Some(
					r#"{"enrichment":{"receiverType":"Map","typeDisplayName":"Map","isExternalType":true}}"#.into(),
				),
			},
			TrustUnresolvedEdgeSample {
				category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
				basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
				source_node_visibility: None,
				metadata_json: Some(
					r#"{"enrichment":{"receiverType":"MyClass","isExternalType":false}}"#.into(),
				),
			},
		];

		let report = compute_trust_report(&input);
		let es = report.enrichment_status.unwrap();
		assert_eq!(es.eligible, 2);
		assert_eq!(es.enriched, 2);
		assert_eq!(es.top_types.len(), 2);
		// Sorted by count desc, then type_name asc.
		// Both have count=1, so sorted alphabetically: Map, MyClass.
		assert_eq!(es.top_types[0].type_name, "Map");
		assert!(es.top_types[0].is_external);
		assert_eq!(es.top_types[1].type_name, "MyClass");
		assert!(!es.top_types[1].is_external);
	}

	#[test]
	fn enrichment_populated_with_zero_enriched_when_enrichment_ran_but_no_types() {
		let mut input = minimal_input();
		input.all_classification_counts = vec![ClassificationCountRow {
			classification: UnresolvedEdgeClassification::Unknown,
			count: 1,
		}];
		input.unknown_calls_samples = vec![TrustUnresolvedEdgeSample {
			category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
			source_node_visibility: None,
			// Enrichment key exists but no receiverType → enrichment ran, resolved 0.
			metadata_json: Some(r#"{"enrichment":{}}"#.into()),
		}];

		let report = compute_trust_report(&input);
		let es = report.enrichment_status.unwrap();
		assert_eq!(es.eligible, 1);
		assert_eq!(es.enriched, 0);
		assert_eq!(es.top_types.len(), 0);
	}

	// ── Module suspicious flag ───────────────────────────────

	#[test]
	fn module_suspicious_zero_connectivity_flagged() {
		let mut input = minimal_input();
		input.module_stats = vec![
			TrustModuleStats {
				stable_key: "r1:src/orphan:MODULE".into(),
				path: "src/orphan".into(),
				fan_in: 0,
				fan_out: 0,
				file_count: 3,
			},
			TrustModuleStats {
				stable_key: "r1:src/connected:MODULE".into(),
				path: "src/connected".into(),
				fan_in: 2,
				fan_out: 1,
				file_count: 5,
			},
		];

		let report = compute_trust_report(&input);
		assert_eq!(report.modules.len(), 2);
		// Sorted by qualified_name: src/connected, src/orphan.
		assert_eq!(report.modules[0].qualified_name, "src/connected");
		assert!(!report.modules[0].suspicious_zero_connectivity);
		assert!(report.modules[0].trust_notes.is_empty());
		assert_eq!(report.modules[1].qualified_name, "src/orphan");
		assert!(report.modules[1].suspicious_zero_connectivity);
		assert_eq!(report.modules[1].trust_notes, vec!["alias_resolution_candidate"]);
	}

	#[test]
	fn root_module_not_flagged_suspicious() {
		let mut input = minimal_input();
		input.module_stats = vec![TrustModuleStats {
			stable_key: "r1:.:MODULE".into(),
			path: ".".into(),
			fan_in: 0,
			fan_out: 0,
			file_count: 10,
		}];

		let report = compute_trust_report(&input);
		assert!(!report.modules[0].suspicious_zero_connectivity);
	}

	#[test]
	fn module_rows_sorted_by_qualified_name_regardless_of_input_order() {
		let mut input = minimal_input();
		// Feed modules in reverse alphabetical order.
		input.module_stats = vec![
			TrustModuleStats {
				stable_key: "r1:src/z:MODULE".into(),
				path: "src/z".into(),
				fan_in: 0,
				fan_out: 0,
				file_count: 1,
			},
			TrustModuleStats {
				stable_key: "r1:src/a:MODULE".into(),
				path: "src/a".into(),
				fan_in: 0,
				fan_out: 0,
				file_count: 1,
			},
			TrustModuleStats {
				stable_key: "r1:src/m:MODULE".into(),
				path: "src/m".into(),
				fan_in: 0,
				fan_out: 0,
				file_count: 1,
			},
		];

		let report = compute_trust_report(&input);
		assert_eq!(report.modules.len(), 3);
		assert_eq!(report.modules[0].qualified_name, "src/a");
		assert_eq!(report.modules[1].qualified_name, "src/m");
		assert_eq!(report.modules[2].qualified_name, "src/z");
	}

	// ── Caveats ──────────────────────────────────────────────

	#[test]
	fn caveats_for_non_high_levels() {
		let mut input = minimal_input();
		// Force LOW call graph by having lots of unresolved, few resolved.
		let mut breakdown = BTreeMap::new();
		breakdown.insert("calls_function_ambiguous_or_missing".into(), 100);
		input.diagnostics = Some(ExtractionDiagnostics {
			diagnostics_version: 1,
			edges_total: 110,
			unresolved_total: 100,
			unresolved_breakdown: breakdown,
		});
		input.resolved_calls = 10;
		// No entrypoints → missing_entrypoint triggered.
		input.active_entrypoint_count = 0;

		let report = compute_trust_report(&input);
		assert!(report.caveats.iter().any(|c| c.contains("Call-graph reliability is LOW")));
		assert!(report.caveats.iter().any(|c| c.contains("Dead-code reliability is LOW")));
	}

	// ── Call resolution rate edge case ────────────────────────

	#[test]
	fn call_resolution_rate_is_one_when_no_calls() {
		let input = minimal_input();
		let report = compute_trust_report(&input);
		assert_eq!(report.summary.call_resolution_rate, 1.0);
	}

	// ── human_label_for_category ─────────────────────────────

	#[test]
	fn human_labels_match_ts_labels() {
		assert_eq!(
			human_label_for_category("imports_file_not_found"),
			"IMPORTS (file not found)"
		);
		assert_eq!(
			human_label_for_category("calls_obj_method_needs_type_info"),
			"CALLS obj.method (needs type info)"
		);
		assert_eq!(
			human_label_for_category("other"),
			"OTHER (unclassified)"
		);
		// Fallback: unknown category returns itself.
		assert_eq!(
			human_label_for_category("something_new"),
			"something_new"
		);
	}

	// ── Assembly layer tests ─────────────────────────────────
	//
	// These test the assembly function directly using a mock
	// TrustStorageRead implementation. They pin:
	//   - storage error propagation
	//   - malformed toolchain JSON
	//   - malformed diagnostics JSON

	/// Mock storage that returns configurable results.
	struct MockStorage {
		diagnostics_json: Option<String>,
		file_paths: Vec<String>,
		module_stats: Vec<TrustModuleStats>,
		path_prefix_cycles: Vec<PathPrefixModuleCycle>,
		active_entrypoint_count: usize,
		resolved_calls: u64,
		calls_classification_counts: Vec<ClassificationCountRow>,
		all_classification_counts: Vec<ClassificationCountRow>,
		unknown_calls_samples: Vec<TrustUnresolvedEdgeSample>,
		/// If set, all methods return this error.
		force_error: Option<String>,
	}

	impl MockStorage {
		fn ok() -> Self {
			Self {
				diagnostics_json: None,
				file_paths: vec![],
				module_stats: vec![],
				path_prefix_cycles: vec![],
				active_entrypoint_count: 1,
				resolved_calls: 0,
				calls_classification_counts: vec![],
				all_classification_counts: vec![],
				unknown_calls_samples: vec![],
				force_error: None,
			}
		}

		fn err_result<T>(&self) -> Result<T, String> {
			Err(self.force_error.clone().unwrap())
		}
	}

	impl TrustStorageRead for MockStorage {
		type Error = String;

		fn get_snapshot_extraction_diagnostics(
			&self,
			_snapshot_uid: &str,
		) -> Result<Option<String>, String> {
			if self.force_error.is_some() {
				return self.err_result();
			}
			Ok(self.diagnostics_json.clone())
		}

		fn count_edges_by_type(
			&self,
			_snapshot_uid: &str,
			_edge_type: &str,
		) -> Result<u64, String> {
			if self.force_error.is_some() {
				return self.err_result();
			}
			Ok(self.resolved_calls)
		}

		fn count_active_declarations(
			&self,
			_repo_uid: &str,
			_kind: &str,
		) -> Result<usize, String> {
			if self.force_error.is_some() {
				return self.err_result();
			}
			Ok(self.active_entrypoint_count)
		}

		fn count_unresolved_edges_by_classification(
			&self,
			input: &CountByClassificationInput,
		) -> Result<Vec<ClassificationCountRow>, String> {
			if self.force_error.is_some() {
				return self.err_result();
			}
			if input.filter_categories.is_empty() {
				Ok(self.all_classification_counts.clone())
			} else {
				Ok(self.calls_classification_counts.clone())
			}
		}

		fn query_unresolved_edges(
			&self,
			_input: &QueryUnresolvedEdgesInput,
		) -> Result<Vec<TrustUnresolvedEdgeSample>, String> {
			if self.force_error.is_some() {
				return self.err_result();
			}
			Ok(self.unknown_calls_samples.clone())
		}

		fn find_path_prefix_module_cycles(
			&self,
			_snapshot_uid: &str,
		) -> Result<Vec<PathPrefixModuleCycle>, String> {
			if self.force_error.is_some() {
				return self.err_result();
			}
			Ok(self.path_prefix_cycles.clone())
		}

		fn compute_module_stats(
			&self,
			_snapshot_uid: &str,
		) -> Result<Vec<TrustModuleStats>, String> {
			if self.force_error.is_some() {
				return self.err_result();
			}
			Ok(self.module_stats.clone())
		}

		fn get_file_paths_by_repo(
			&self,
			_repo_uid: &str,
		) -> Result<Vec<String>, String> {
			if self.force_error.is_some() {
				return self.err_result();
			}
			Ok(self.file_paths.clone())
		}
	}

	#[test]
	fn assembly_produces_report_from_mock_storage() {
		let mock = MockStorage::ok();
		let result = assemble_trust_report(&mock, "r1", "snap1", None, None);
		let report = result.unwrap();
		assert_eq!(report.snapshot_uid, "snap1");
		assert!(report.diagnostics_available == false);
	}

	#[test]
	fn assembly_propagates_storage_error() {
		let mock = MockStorage {
			force_error: Some("database locked".into()),
			..MockStorage::ok()
		};
		let result = assemble_trust_report(&mock, "r1", "snap1", None, None);
		match result {
			Err(TrustAssemblyError::Storage(msg)) => {
				assert_eq!(msg, "database locked");
			}
			other => panic!("expected Storage error, got {:?}", other),
		}
	}

	#[test]
	fn assembly_errors_on_malformed_toolchain_json() {
		let mock = MockStorage::ok();
		let result =
			assemble_trust_report(&mock, "r1", "snap1", None, Some("{invalid json"));
		match result {
			Err(TrustAssemblyError::JsonParse { field, .. }) => {
				assert_eq!(field, "toolchain_json");
			}
			other => panic!("expected JsonParse for toolchain, got {:?}", other),
		}
	}

	#[test]
	fn assembly_errors_on_malformed_diagnostics_json() {
		let mut mock = MockStorage::ok();
		mock.diagnostics_json = Some("{not valid json!!}".into());
		let result = assemble_trust_report(&mock, "r1", "snap1", None, None);
		match result {
			Err(TrustAssemblyError::JsonParse { field, .. }) => {
				assert_eq!(field, "extraction_diagnostics_json");
			}
			other => panic!(
				"expected JsonParse for diagnostics, got {:?}",
				other
			),
		}
	}
}
