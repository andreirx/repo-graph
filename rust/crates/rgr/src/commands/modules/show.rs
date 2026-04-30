//! Modules show command.
//!
//! RS-MG-12c: Single module detail view with neighbors and violations.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_modules_show` handler
//! - `ModuleIdentity`, `ModuleShowRollups`, `EnrichedNeighbor`,
//!   `ViolationTargetIdentity`, `EnrichedViolation` DTOs
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in `repo-graph-module-queries`)
//! - weighted neighbor computation (belongs in `repo-graph-classification`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, compute_trust_overlay_for_snapshot, open_storage};
use repo_graph_classification::boundary_evaluator::ModuleBoundaryEvaluation;
use super::shared::{evaluate_violations_from_facts, load_module_graph_facts};

// ── modules show DTOs ────────────────────────────────────────────

/// Module identity DTO for `modules show` output.
#[derive(serde::Serialize, Clone)]
pub(super) struct ModuleIdentity {
	pub module_uid: String,
	pub module_key: String,
	pub canonical_root_path: String,
	pub module_kind: String,
	pub display_name: Option<String>,
	pub confidence: f64,
}

/// Rollups DTO for `modules show` output.
/// Matches `modules list` rollup fields, with `violation_count` nullable.
#[derive(serde::Serialize)]
struct ModuleShowRollups {
	owned_file_count: u64,
	owned_test_file_count: u64,
	outbound_dependency_count: u64,
	outbound_import_count: u64,
	inbound_dependency_count: u64,
	inbound_import_count: u64,
	violation_count: Option<u64>,
	dead_symbol_count: u64,
	dead_test_symbol_count: u64,
}

/// Weighted neighbor DTO with full identity.
#[derive(serde::Serialize)]
struct EnrichedNeighbor {
	module_uid: String,
	module_key: String,
	canonical_root_path: String,
	module_kind: String,
	import_count: u64,
	source_file_count: u64,
}

/// Target module identity for violation output.
#[derive(serde::Serialize)]
struct ViolationTargetIdentity {
	module_uid: String,
	module_key: String,
	canonical_root_path: String,
	module_kind: String,
}

/// Violation DTO with enriched target identity.
#[derive(serde::Serialize)]
struct EnrichedViolation {
	declaration_uid: String,
	target: ViolationTargetIdentity,
	import_count: u64,
	source_file_count: u64,
	reason: Option<String>,
}

// ── modules show command ─────────────────────────────────────────

pub(super) fn run_modules_show(args: &[String]) -> ExitCode {
	if args.len() != 3 {
		eprintln!("usage: rmap modules show <db_path> <repo_uid> <module>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];
	let module_arg = &args[2];

	let storage = match open_storage(db_path) {
		Ok(s) => s,
		Err(msg) => {
			eprintln!("error: {}", msg);
			return ExitCode::from(2);
		}
	};

	let snapshot = match storage.get_latest_snapshot(repo_uid) {
		Ok(Some(snap)) => snap,
		Ok(None) => {
			eprintln!("error: no snapshot found for repo '{}'", repo_uid);
			return ExitCode::from(2);
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Step 1: Load module graph facts (single load) ─────────────
	let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
		Ok(f) => f,
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Step 2: Resolve module argument ───────────────────────────
	// Resolution: canonical_root_path exact → module_key exact → module_uid exact → exit 1
	let resolved_module = match facts.resolve_module(module_arg) {
		Some(m) => m.clone(),
		None => {
			eprintln!("error: module not found: {}", module_arg);
			return ExitCode::from(1); // Exit 1 for resolution failure
		}
	};

	// Build module identity lookup for enrichment
	let module_identity_map: std::collections::HashMap<&str, ModuleIdentity> = facts
		.modules()
		.iter()
		.map(|m| {
			(
				m.canonical_root_path.as_str(),
				ModuleIdentity {
					module_uid: m.module_candidate_uid.clone(),
					module_key: m.module_key.clone(),
					canonical_root_path: m.canonical_root_path.clone(),
					module_kind: m.module_kind.clone(),
					display_name: m.display_name.clone(),
					confidence: m.confidence,
				},
			)
		})
		.collect();

	// ── Step 3: Load dead nodes (SYMBOL kind only) ────────────────
	let dead_nodes = match storage.find_dead_nodes(&snapshot.snapshot_uid, repo_uid, Some("SYMBOL"))
	{
		Ok(d) => d,
		Err(e) => {
			eprintln!("error: failed to load dead nodes: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Step 4: Evaluate violations (advisory, uses preloaded facts) ─
	let (violations_eval, violations_warning): (Option<ModuleBoundaryEvaluation>, Option<String>) =
		match evaluate_violations_from_facts(&storage, repo_uid, &facts) {
			Ok(r) => (Some(r.evaluation), None),
			Err(msg) => (
				None,
				Some(format!(
					"discovered-module violation rollups unavailable: {}",
					msg
				)),
			),
		};

	// ── Step 5: Compute rollup for this module ────────────────────
	use repo_graph_classification::module_rollup::{
		compute_module_rollups, DeadNodeFact, ModuleRollupInput, OwnedFileFact,
	};

	let violations_for_rollup = violations_eval
		.as_ref()
		.map(|e| e.violations.clone())
		.unwrap_or_default();

	let owned_file_facts: Vec<OwnedFileFact> = facts
		.owned_files()
		.iter()
		.map(|f| OwnedFileFact {
			file_path: f.file_path.clone(),
			module_uid: f.module_candidate_uid.clone(),
			is_test: f.is_test,
		})
		.collect();

	let dead_node_facts: Vec<DeadNodeFact> = dead_nodes
		.into_iter()
		.filter_map(|d| {
			d.file.map(|file_path| DeadNodeFact {
				file_path,
				is_test: d.is_test,
			})
		})
		.collect();

	let rollup_input = ModuleRollupInput {
		modules: facts.module_refs.clone(),
		owned_files: owned_file_facts,
		edges: facts.edges.clone(),
		violations: violations_for_rollup,
		dead_nodes: dead_node_facts,
	};

	let rollups = match compute_module_rollups(&rollup_input) {
		Ok(r) => r,
		Err(e) => {
			eprintln!("error: failed to compute rollups: {}", e);
			return ExitCode::from(2);
		}
	};

	// Find this module's rollup
	let module_rollup = rollups
		.iter()
		.find(|r| r.module_uid == resolved_module.module_candidate_uid);

	let violations_available = violations_eval.is_some();

	let rollups_output = ModuleShowRollups {
		owned_file_count: module_rollup.map_or(0, |r| r.owned_file_count),
		owned_test_file_count: module_rollup.map_or(0, |r| r.owned_test_file_count),
		outbound_dependency_count: module_rollup.map_or(0, |r| r.outbound_dependency_count),
		outbound_import_count: module_rollup.map_or(0, |r| r.outbound_import_count),
		inbound_dependency_count: module_rollup.map_or(0, |r| r.inbound_dependency_count),
		inbound_import_count: module_rollup.map_or(0, |r| r.inbound_import_count),
		violation_count: if violations_available {
			Some(module_rollup.map_or(0, |r| r.violation_count))
		} else {
			None
		},
		dead_symbol_count: module_rollup.map_or(0, |r| r.dead_symbol_count),
		dead_test_symbol_count: module_rollup.map_or(0, |r| r.dead_test_symbol_count),
	};

	// ── Step 6: Compute weighted neighbors ────────────────────────
	use repo_graph_classification::weighted_neighbors::compute_weighted_neighbors;

	let weighted = compute_weighted_neighbors(&resolved_module.module_candidate_uid, &facts.edges);

	// Enrich outbound neighbors with identity
	let outbound_dependencies: Vec<EnrichedNeighbor> = weighted
		.outbound
		.iter()
		.filter_map(|n| {
			// Find module by UID, then get identity from path lookup
			let module_path = facts.edges
				.iter()
				.find(|e| e.target_module_uid == n.module_uid)
				.map(|e| e.target_canonical_path.as_str())?;
			let identity = module_identity_map.get(module_path)?;
			Some(EnrichedNeighbor {
				module_uid: identity.module_uid.clone(),
				module_key: identity.module_key.clone(),
				canonical_root_path: identity.canonical_root_path.clone(),
				module_kind: identity.module_kind.clone(),
				import_count: n.import_count,
				source_file_count: n.source_file_count,
			})
		})
		.collect();

	// Enrich inbound neighbors with identity
	let inbound_dependencies: Vec<EnrichedNeighbor> = weighted
		.inbound
		.iter()
		.filter_map(|n| {
			let module_path = facts.edges
				.iter()
				.find(|e| e.source_module_uid == n.module_uid)
				.map(|e| e.source_canonical_path.as_str())?;
			let identity = module_identity_map.get(module_path)?;
			Some(EnrichedNeighbor {
				module_uid: identity.module_uid.clone(),
				module_key: identity.module_key.clone(),
				canonical_root_path: identity.canonical_root_path.clone(),
				module_kind: identity.module_kind.clone(),
				import_count: n.import_count,
				source_file_count: n.source_file_count,
			})
		})
		.collect();

	// ── Step 7: Filter and enrich violations ──────────────────────
	// Only source-side violations (where this module is the source)
	let violations_output: Option<Vec<EnrichedViolation>> = if violations_available {
		let source_violations: Vec<EnrichedViolation> = violations_eval
			.as_ref()
			.unwrap()
			.violations
			.iter()
			.filter(|v| v.source_canonical_path == resolved_module.canonical_root_path)
			.filter_map(|v| {
				let target_identity = module_identity_map.get(v.target_canonical_path.as_str())?;
				Some(EnrichedViolation {
					declaration_uid: v.declaration_uid.clone(),
					target: ViolationTargetIdentity {
						module_uid: target_identity.module_uid.clone(),
						module_key: target_identity.module_key.clone(),
						canonical_root_path: target_identity.canonical_root_path.clone(),
						module_kind: target_identity.module_kind.clone(),
					},
					import_count: v.import_count,
					source_file_count: v.source_file_count,
					reason: v.reason.clone(),
				})
			})
			.collect();
		Some(source_violations)
	} else {
		None // null when policy unavailable
	};

	// ── Step 8: Build output ──────────────────────────────────────
	let module_identity = ModuleIdentity {
		module_uid: resolved_module.module_candidate_uid.clone(),
		module_key: resolved_module.module_key.clone(),
		canonical_root_path: resolved_module.canonical_root_path.clone(),
		module_kind: resolved_module.module_kind.clone(),
		display_name: resolved_module.display_name.clone(),
		confidence: resolved_module.confidence,
	};

	let warnings: Vec<String> = violations_warning.into_iter().collect();

	let mut extra_fields = serde_json::Map::new();
	extra_fields.insert(
		"module".to_string(),
		serde_json::to_value(&module_identity).unwrap(),
	);
	extra_fields.insert(
		"rollups".to_string(),
		serde_json::to_value(&rollups_output).unwrap(),
	);
	extra_fields.insert(
		"outbound_dependencies".to_string(),
		serde_json::to_value(&outbound_dependencies).unwrap(),
	);
	extra_fields.insert(
		"inbound_dependencies".to_string(),
		serde_json::to_value(&inbound_dependencies).unwrap(),
	);
	extra_fields.insert(
		"violations".to_string(),
		serde_json::to_value(&violations_output).unwrap(),
	);
	extra_fields.insert(
		"rollups_degraded".to_string(),
		serde_json::Value::Bool(!violations_available),
	);
	extra_fields.insert(
		"warnings".to_string(),
		serde_json::to_value(&warnings).unwrap(),
	);

	// Trust overlay (Option A: only when repo has degradations).
	// Module dependencies are import-based, so graph_basis = "IMPORTS".
	if let Some(trust) = compute_trust_overlay_for_snapshot(&storage, repo_uid, &snapshot, "IMPORTS") {
		if trust.has_degradation() || !trust.caveats.is_empty() {
			extra_fields.insert("trust".to_string(), serde_json::to_value(&trust).unwrap());
		}
	}

	// Build envelope (no results array for show — module is the main content)
	let output = match build_envelope(
		&storage,
		"modules show",
		repo_uid,
		&snapshot,
		serde_json::Value::Null, // No results array
		0,                       // count not applicable
		extra_fields,
	) {
		Ok(mut v) => {
			// Remove the results/count fields since show doesn't use them
			if let serde_json::Value::Object(ref mut map) = v {
				map.remove("results");
				map.remove("count");
			}
			v
		}
		Err(e) => {
			eprintln!("error: {}", e);
			return ExitCode::from(2);
		}
	};

	match serde_json::to_string_pretty(&output) {
		Ok(json) => {
			println!("{}", json);
			ExitCode::SUCCESS
		}
		Err(e) => {
			eprintln!("error: {}", e);
			ExitCode::from(2)
		}
	}
}
