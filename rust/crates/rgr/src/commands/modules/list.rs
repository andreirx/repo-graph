//! Modules list command.
//!
//! RS-MG-12b: Module list with rollup statistics.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_modules_list` handler
//! - `ModuleListEntry` DTO
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in `repo-graph-module-queries`)
//! - rollup computation (belongs in `repo-graph-classification`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage};
use super::shared::{evaluate_violations_from_facts, load_module_graph_facts};

// ── modules list command ─────────────────────────────────────────

/// Output DTO for `modules list` command.
///
/// Dedicated CLI output shape — does not expose storage internals
/// like `snapshot_uid`, `repo_uid`, or `metadata_json`.
///
/// RS-MG-12b: Extended with rollup fields for per-module stats.
#[derive(serde::Serialize)]
struct ModuleListEntry {
	// Identity fields
	module_uid: String,
	module_key: String,
	canonical_root_path: String,
	module_kind: String,
	display_name: Option<String>,
	confidence: f64,
	// Rollup fields (RS-MG-12b)
	owned_file_count: u64,
	owned_test_file_count: u64,
	outbound_dependency_count: u64,
	outbound_import_count: u64,
	inbound_dependency_count: u64,
	inbound_import_count: u64,
	/// `None` when policy-derived rollups are unavailable (parse failure).
	/// `Some(0)` means zero violations; `None` means unknown.
	violation_count: Option<u64>,
	dead_symbol_count: u64,
	dead_test_symbol_count: u64,
}

pub(super) fn run_modules_list(args: &[String]) -> ExitCode {
	if args.len() != 2 {
		eprintln!("usage: rmap modules list <db_path> <repo_uid>");
		return ExitCode::from(1);
	}

	let db_path = Path::new(&args[0]);
	let repo_uid = &args[1];

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

	// ── Step 2: Load dead nodes (SYMBOL kind only) ────────────────
	let dead_nodes = match storage.find_dead_nodes(&snapshot.snapshot_uid, repo_uid, Some("SYMBOL"))
	{
		Ok(d) => d,
		Err(e) => {
			eprintln!("error: failed to load dead nodes: {}", e);
			return ExitCode::from(2);
		}
	};

	// ── Step 3: Evaluate violations (advisory, uses preloaded facts) ─
	let (violations_eval, violations_warning): (
		Option<repo_graph_classification::boundary_evaluator::ModuleBoundaryEvaluation>,
		Option<String>,
	) = match evaluate_violations_from_facts(&storage, repo_uid, &facts) {
		Ok(r) => (Some(r.evaluation), None),
		Err(msg) => (
			None,
			Some(format!(
				"discovered-module violation rollups unavailable: {}",
				msg
			)),
		),
	};

	// ── Step 4: Compute rollups ───────────────────────────────────
	use repo_graph_classification::module_rollup::{
		compute_module_rollups, DeadNodeFact, ModuleRollupInput, OwnedFileFact,
	};

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

	// When violations are unavailable, pass empty vec — rollups will compute
	// violation_count as 0, but we'll override to None in the output.
	let violations_for_rollup = violations_eval
		.as_ref()
		.map(|e| e.violations.clone())
		.unwrap_or_default();

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

	// ── Step 5: Build rollup lookup by module_uid ─────────────────
	use std::collections::HashMap;
	let rollup_map: HashMap<&str, &repo_graph_classification::module_rollup::ModuleRollup> =
		rollups.iter().map(|r| (r.module_uid.as_str(), r)).collect();

	// ── Step 6: Merge module identity with rollup stats ───────────
	// violation_count is None when violations_eval failed (policy unavailable)
	let violations_available = violations_eval.is_some();

	let results: Vec<ModuleListEntry> = facts
		.modules()
		.iter()
		.map(|m| {
			let rollup = rollup_map.get(m.module_candidate_uid.as_str());
			ModuleListEntry {
				module_uid: m.module_candidate_uid.clone(),
				module_key: m.module_key.clone(),
				canonical_root_path: m.canonical_root_path.clone(),
				module_kind: m.module_kind.clone(),
				display_name: m.display_name.clone(),
				confidence: m.confidence,
				// Rollup fields — default to 0 if rollup missing (shouldn't happen)
				owned_file_count: rollup.map_or(0, |r| r.owned_file_count),
				owned_test_file_count: rollup.map_or(0, |r| r.owned_test_file_count),
				outbound_dependency_count: rollup.map_or(0, |r| r.outbound_dependency_count),
				outbound_import_count: rollup.map_or(0, |r| r.outbound_import_count),
				inbound_dependency_count: rollup.map_or(0, |r| r.inbound_dependency_count),
				inbound_import_count: rollup.map_or(0, |r| r.inbound_import_count),
				// None when policy parsing failed; Some(count) when available
				violation_count: if violations_available {
					Some(rollup.map_or(0, |r| r.violation_count))
				} else {
					None
				},
				dead_symbol_count: rollup.map_or(0, |r| r.dead_symbol_count),
				dead_test_symbol_count: rollup.map_or(0, |r| r.dead_test_symbol_count),
			}
		})
		.collect();

	let count = results.len();

	// Build extra envelope fields for degradation status
	let mut extra_fields = serde_json::Map::new();
	extra_fields.insert(
		"rollups_degraded".to_string(),
		serde_json::Value::Bool(!violations_available),
	);

	let warnings: Vec<String> = violations_warning.into_iter().collect();
	extra_fields.insert(
		"warnings".to_string(),
		serde_json::to_value(&warnings).unwrap(),
	);

	let output = match build_envelope(
		&storage,
		"modules list",
		repo_uid,
		&snapshot,
		serde_json::to_value(&results).unwrap(),
		count,
		extra_fields,
	) {
		Ok(v) => v,
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
