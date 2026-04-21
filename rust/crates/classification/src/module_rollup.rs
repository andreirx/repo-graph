//! Module rollup aggregator — pure policy core (RS-MG-12a).
//!
//! Computes per-module aggregate statistics from:
//! - Owned file facts
//! - Module dependency edges
//! - Module boundary violations
//! - Dead node facts
//!
//! This is pure policy. No DB access. No side effects.
//!
//! Design decisions:
//! - Input DTOs are normalized facts, not pre-joined maps
//! - Duplicate file ownership is an explicit error (same contract as RS-MG-2)
//! - Test/non-test metrics separated for operational clarity
//! - Both breadth (dependency_count) and weight (import_count) exposed
//! - Output is deterministically sorted by canonical_path

use std::collections::{HashMap, HashSet};

use crate::boundary_evaluator::ModuleBoundaryViolation;
use crate::module_edges::{ModuleDependencyEdge, ModuleRef};

// ── Input DTOs ─────────────────────────────────────────────────────

/// A file owned by a module.
///
/// Normalized fact for rollup computation. The is_test flag comes
/// from the files table (persisted during indexing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedFileFact {
	pub file_path: String,
	pub module_uid: String,
	pub is_test: bool,
}

/// A dead node (symbol with no inbound references).
///
/// Minimal fact for rollup aggregation. The is_test flag indicates
/// whether the symbol is in a test file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadNodeFact {
	pub file_path: String,
	pub is_test: bool,
}

/// Input bundle for module rollup computation.
#[derive(Debug, Clone)]
pub struct ModuleRollupInput {
	/// Module identity refs.
	pub modules: Vec<ModuleRef>,
	/// File ownership facts (file_path → module_uid).
	pub owned_files: Vec<OwnedFileFact>,
	/// Cross-module dependency edges (from derive_module_dependency_edges).
	pub edges: Vec<ModuleDependencyEdge>,
	/// Boundary violations (from evaluate_module_boundaries).
	pub violations: Vec<ModuleBoundaryViolation>,
	/// Dead nodes with file attribution.
	pub dead_nodes: Vec<DeadNodeFact>,
}

// ── Output DTOs ────────────────────────────────────────────────────

/// Per-module aggregated statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRollup {
	pub module_uid: String,
	pub canonical_path: String,
	/// Count of owned non-test files.
	pub owned_file_count: u64,
	/// Count of owned test files.
	pub owned_test_file_count: u64,
	/// Distinct modules this module imports from (breadth).
	pub outbound_dependency_count: u64,
	/// Total import edges from this module (weight).
	pub outbound_import_count: u64,
	/// Distinct modules that import from this module (breadth).
	pub inbound_dependency_count: u64,
	/// Total import edges targeting this module (weight).
	pub inbound_import_count: u64,
	/// Boundary violations where this module is the source.
	pub violation_count: u64,
	/// Dead symbols in non-test files.
	pub dead_symbol_count: u64,
	/// Dead symbols in test files.
	pub dead_test_symbol_count: u64,
}

// ── Errors ─────────────────────────────────────────────────────────

/// Error during module rollup computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleRollupError {
	/// A file has multiple ownership assignments.
	///
	/// Same contract as RS-MG-2: duplicate ownership is an explicit
	/// error, not silent overwrite.
	DuplicateOwnership {
		file_path: String,
		module_uids: Vec<String>,
	},
}

impl std::fmt::Display for ModuleRollupError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ModuleRollupError::DuplicateOwnership {
				file_path,
				module_uids,
			} => {
				write!(
					f,
					"file {} has duplicate ownership: {:?}",
					file_path, module_uids
				)
			}
		}
	}
}

impl std::error::Error for ModuleRollupError {}

// ── Pure aggregator ────────────────────────────────────────────────

/// Compute per-module rollup statistics.
///
/// # Arguments
///
/// * `input` - Input bundle with modules, owned files, edges, violations, dead nodes
///
/// # Returns
///
/// * `Ok(Vec<ModuleRollup>)` - Per-module stats, sorted by canonical_path
/// * `Err(ModuleRollupError)` - If duplicate file ownership detected
///
/// # Errors
///
/// Returns `ModuleRollupError::DuplicateOwnership` if any file appears
/// in multiple ownership facts with different module_uids.
pub fn compute_module_rollups(
	input: &ModuleRollupInput,
) -> Result<Vec<ModuleRollup>, ModuleRollupError> {
	// ── Step 1: Build file_path → (module_uid, is_test) index ──────
	// Detect duplicates explicitly per RS-MG-2 contract.
	let mut file_to_module: HashMap<&str, (&str, bool)> = HashMap::new();
	let mut duplicates: HashMap<&str, Vec<&str>> = HashMap::new();

	for fact in &input.owned_files {
		let path = fact.file_path.as_str();
		let module = fact.module_uid.as_str();

		if let Some((existing_module, _)) = file_to_module.get(path) {
			if *existing_module != module {
				// Different module owns the same file — collect duplicates
				duplicates
					.entry(path)
					.or_insert_with(|| vec![*existing_module])
					.push(module);
			}
			// Same module, same file — idempotent, skip
		} else {
			file_to_module.insert(path, (module, fact.is_test));
		}
	}

	// Report first duplicate found (consistent with module_edges.rs behavior)
	if let Some((path, modules)) = duplicates.into_iter().next() {
		return Err(ModuleRollupError::DuplicateOwnership {
			file_path: path.to_string(),
			module_uids: modules.into_iter().map(String::from).collect(),
		});
	}

	// ── Step 2: Initialize rollup accumulators per module ──────────
	struct Accumulator {
		canonical_path: String,
		owned_file_count: u64,
		owned_test_file_count: u64,
		outbound_modules: HashSet<String>,
		outbound_import_count: u64,
		inbound_modules: HashSet<String>,
		inbound_import_count: u64,
		violation_count: u64,
		dead_symbol_count: u64,
		dead_test_symbol_count: u64,
	}

	let mut accumulators: HashMap<&str, Accumulator> = input
		.modules
		.iter()
		.map(|m| {
			(
				m.module_uid.as_str(),
				Accumulator {
					canonical_path: m.canonical_path.clone(),
					owned_file_count: 0,
					owned_test_file_count: 0,
					outbound_modules: HashSet::new(),
					outbound_import_count: 0,
					inbound_modules: HashSet::new(),
					inbound_import_count: 0,
					violation_count: 0,
					dead_symbol_count: 0,
					dead_test_symbol_count: 0,
				},
			)
		})
		.collect();

	// ── Step 3: Count owned files (test vs non-test) ───────────────
	// Use file_to_module index to get deduplicated counts.
	// file_to_module already has idempotent entries (same file, same module = one entry).
	for (_, (module_uid, is_test)) in &file_to_module {
		if let Some(acc) = accumulators.get_mut(*module_uid) {
			if *is_test {
				acc.owned_test_file_count += 1;
			} else {
				acc.owned_file_count += 1;
			}
		}
		// Files owned by unknown modules are ignored (orphan files)
	}

	// ── Step 4: Aggregate dependency edges ─────────────────────────
	for edge in &input.edges {
		// Outbound: source module imports from target module
		if let Some(acc) = accumulators.get_mut(edge.source_module_uid.as_str()) {
			acc.outbound_modules
				.insert(edge.target_module_uid.clone());
			acc.outbound_import_count += edge.import_count;
		}

		// Inbound: target module is imported by source module
		if let Some(acc) = accumulators.get_mut(edge.target_module_uid.as_str()) {
			acc.inbound_modules
				.insert(edge.source_module_uid.clone());
			acc.inbound_import_count += edge.import_count;
		}
	}

	// ── Step 5: Count violations by source module ──────────────────
	for violation in &input.violations {
		// Find module by canonical_path (violations use paths, not UIDs)
		if let Some((_, acc)) = accumulators
			.iter_mut()
			.find(|(_, a)| a.canonical_path == violation.source_canonical_path)
		{
			acc.violation_count += 1;
		}
	}

	// ── Step 6: Attribute dead symbols to modules via file path ────
	for dead in &input.dead_nodes {
		if let Some((module_uid, _)) = file_to_module.get(dead.file_path.as_str()) {
			if let Some(acc) = accumulators.get_mut(*module_uid) {
				if dead.is_test {
					acc.dead_test_symbol_count += 1;
				} else {
					acc.dead_symbol_count += 1;
				}
			}
		}
		// Dead symbols in unowned files are ignored (orphan symbols)
	}

	// ── Step 7: Build output, sorted by canonical_path ─────────────
	let mut rollups: Vec<ModuleRollup> = accumulators
		.into_iter()
		.map(|(module_uid, acc)| ModuleRollup {
			module_uid: module_uid.to_string(),
			canonical_path: acc.canonical_path,
			owned_file_count: acc.owned_file_count,
			owned_test_file_count: acc.owned_test_file_count,
			outbound_dependency_count: acc.outbound_modules.len() as u64,
			outbound_import_count: acc.outbound_import_count,
			inbound_dependency_count: acc.inbound_modules.len() as u64,
			inbound_import_count: acc.inbound_import_count,
			violation_count: acc.violation_count,
			dead_symbol_count: acc.dead_symbol_count,
			dead_test_symbol_count: acc.dead_test_symbol_count,
		})
		.collect();

	rollups.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path));

	Ok(rollups)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_module(uid: &str, path: &str) -> ModuleRef {
		ModuleRef {
			module_uid: uid.to_string(),
			canonical_path: path.to_string(),
		}
	}

	fn make_owned_file(path: &str, module_uid: &str, is_test: bool) -> OwnedFileFact {
		OwnedFileFact {
			file_path: path.to_string(),
			module_uid: module_uid.to_string(),
			is_test,
		}
	}

	fn make_edge(
		source_uid: &str,
		source_path: &str,
		target_uid: &str,
		target_path: &str,
		import_count: u64,
	) -> ModuleDependencyEdge {
		ModuleDependencyEdge {
			source_module_uid: source_uid.to_string(),
			source_canonical_path: source_path.to_string(),
			target_module_uid: target_uid.to_string(),
			target_canonical_path: target_path.to_string(),
			import_count,
			source_file_count: 1,
		}
	}

	fn make_violation(source_path: &str, target_path: &str) -> ModuleBoundaryViolation {
		ModuleBoundaryViolation {
			declaration_uid: format!("decl-{}-{}", source_path, target_path),
			source_canonical_path: source_path.to_string(),
			target_canonical_path: target_path.to_string(),
			import_count: 1,
			source_file_count: 1,
			reason: None,
		}
	}

	fn make_dead_node(file_path: &str, is_test: bool) -> DeadNodeFact {
		DeadNodeFact {
			file_path: file_path.to_string(),
			is_test,
		}
	}

	// ── Empty input ────────────────────────────────────────────────

	#[test]
	fn empty_input_returns_empty_rollups() {
		let input = ModuleRollupInput {
			modules: vec![],
			owned_files: vec![],
			edges: vec![],
			violations: vec![],
			dead_nodes: vec![],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert!(result.is_empty());
	}

	// ── Module with no data ────────────────────────────────────────

	#[test]
	fn module_with_no_files_or_edges_has_zero_counts() {
		let input = ModuleRollupInput {
			modules: vec![make_module("mc-app", "packages/app")],
			owned_files: vec![],
			edges: vec![],
			violations: vec![],
			dead_nodes: vec![],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert_eq!(result.len(), 1);

		let rollup = &result[0];
		assert_eq!(rollup.module_uid, "mc-app");
		assert_eq!(rollup.canonical_path, "packages/app");
		assert_eq!(rollup.owned_file_count, 0);
		assert_eq!(rollup.owned_test_file_count, 0);
		assert_eq!(rollup.outbound_dependency_count, 0);
		assert_eq!(rollup.outbound_import_count, 0);
		assert_eq!(rollup.inbound_dependency_count, 0);
		assert_eq!(rollup.inbound_import_count, 0);
		assert_eq!(rollup.violation_count, 0);
		assert_eq!(rollup.dead_symbol_count, 0);
		assert_eq!(rollup.dead_test_symbol_count, 0);
	}

	// ── Owned file counts ──────────────────────────────────────────

	#[test]
	fn counts_owned_files_separately_from_test_files() {
		let input = ModuleRollupInput {
			modules: vec![make_module("mc-app", "packages/app")],
			owned_files: vec![
				make_owned_file("packages/app/index.ts", "mc-app", false),
				make_owned_file("packages/app/util.ts", "mc-app", false),
				make_owned_file("packages/app/index.test.ts", "mc-app", true),
			],
			edges: vec![],
			violations: vec![],
			dead_nodes: vec![],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert_eq!(result.len(), 1);

		let rollup = &result[0];
		assert_eq!(rollup.owned_file_count, 2);
		assert_eq!(rollup.owned_test_file_count, 1);
	}

	// ── Duplicate ownership error ──────────────────────────────────

	#[test]
	fn duplicate_ownership_returns_error() {
		let input = ModuleRollupInput {
			modules: vec![
				make_module("mc-app", "packages/app"),
				make_module("mc-lib", "packages/lib"),
			],
			owned_files: vec![
				make_owned_file("shared/utils.ts", "mc-app", false),
				make_owned_file("shared/utils.ts", "mc-lib", false), // duplicate
			],
			edges: vec![],
			violations: vec![],
			dead_nodes: vec![],
		};

		let result = compute_module_rollups(&input);
		assert!(result.is_err());

		let err = result.unwrap_err();
		match err {
			ModuleRollupError::DuplicateOwnership {
				file_path,
				module_uids,
			} => {
				assert_eq!(file_path, "shared/utils.ts");
				assert!(module_uids.contains(&"mc-app".to_string()));
				assert!(module_uids.contains(&"mc-lib".to_string()));
			}
		}
	}

	#[test]
	fn same_file_same_module_is_idempotent() {
		let input = ModuleRollupInput {
			modules: vec![make_module("mc-app", "packages/app")],
			owned_files: vec![
				make_owned_file("packages/app/index.ts", "mc-app", false),
				make_owned_file("packages/app/index.ts", "mc-app", false), // duplicate same module
			],
			edges: vec![],
			violations: vec![],
			dead_nodes: vec![],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert_eq!(result.len(), 1);
		// Count should be 1, not 2 (idempotent)
		assert_eq!(result[0].owned_file_count, 1);
	}

	// ── Dependency counts ──────────────────────────────────────────

	#[test]
	fn counts_outbound_and_inbound_dependencies() {
		let input = ModuleRollupInput {
			modules: vec![
				make_module("mc-app", "packages/app"),
				make_module("mc-core", "packages/core"),
				make_module("mc-util", "packages/util"),
			],
			owned_files: vec![],
			edges: vec![
				// app imports from core (5 imports)
				make_edge("mc-app", "packages/app", "mc-core", "packages/core", 5),
				// app imports from util (3 imports)
				make_edge("mc-app", "packages/app", "mc-util", "packages/util", 3),
				// core imports from util (2 imports)
				make_edge("mc-core", "packages/core", "mc-util", "packages/util", 2),
			],
			violations: vec![],
			dead_nodes: vec![],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert_eq!(result.len(), 3);

		// Find modules by path (sorted)
		let app = result.iter().find(|r| r.canonical_path == "packages/app").unwrap();
		let core = result.iter().find(|r| r.canonical_path == "packages/core").unwrap();
		let util = result.iter().find(|r| r.canonical_path == "packages/util").unwrap();

		// app: outbound to core + util
		assert_eq!(app.outbound_dependency_count, 2);
		assert_eq!(app.outbound_import_count, 8); // 5 + 3
		assert_eq!(app.inbound_dependency_count, 0);
		assert_eq!(app.inbound_import_count, 0);

		// core: outbound to util, inbound from app
		assert_eq!(core.outbound_dependency_count, 1);
		assert_eq!(core.outbound_import_count, 2);
		assert_eq!(core.inbound_dependency_count, 1);
		assert_eq!(core.inbound_import_count, 5);

		// util: no outbound, inbound from app + core
		assert_eq!(util.outbound_dependency_count, 0);
		assert_eq!(util.outbound_import_count, 0);
		assert_eq!(util.inbound_dependency_count, 2);
		assert_eq!(util.inbound_import_count, 5); // 3 + 2
	}

	// ── Violation counts ───────────────────────────────────────────

	#[test]
	fn counts_violations_by_source_module() {
		let input = ModuleRollupInput {
			modules: vec![
				make_module("mc-adapters", "packages/adapters"),
				make_module("mc-core", "packages/core"),
			],
			owned_files: vec![],
			edges: vec![],
			violations: vec![
				// adapters violates boundary to core (twice)
				make_violation("packages/adapters", "packages/core"),
				make_violation("packages/adapters", "packages/core"),
			],
			dead_nodes: vec![],
		};

		let result = compute_module_rollups(&input).unwrap();

		let adapters = result.iter().find(|r| r.canonical_path == "packages/adapters").unwrap();
		let core = result.iter().find(|r| r.canonical_path == "packages/core").unwrap();

		// Violations counted against source module only
		assert_eq!(adapters.violation_count, 2);
		assert_eq!(core.violation_count, 0);
	}

	// ── Dead symbol counts ─────────────────────────────────────────

	#[test]
	fn counts_dead_symbols_separately_from_test_dead_symbols() {
		let input = ModuleRollupInput {
			modules: vec![make_module("mc-app", "packages/app")],
			owned_files: vec![
				make_owned_file("packages/app/service.ts", "mc-app", false),
				make_owned_file("packages/app/service.test.ts", "mc-app", true),
			],
			edges: vec![],
			violations: vec![],
			dead_nodes: vec![
				// 3 dead symbols in non-test file
				make_dead_node("packages/app/service.ts", false),
				make_dead_node("packages/app/service.ts", false),
				make_dead_node("packages/app/service.ts", false),
				// 2 dead symbols in test file
				make_dead_node("packages/app/service.test.ts", true),
				make_dead_node("packages/app/service.test.ts", true),
			],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert_eq!(result.len(), 1);

		let rollup = &result[0];
		assert_eq!(rollup.dead_symbol_count, 3);
		assert_eq!(rollup.dead_test_symbol_count, 2);
	}

	#[test]
	fn dead_symbols_in_unowned_files_are_ignored() {
		let input = ModuleRollupInput {
			modules: vec![make_module("mc-app", "packages/app")],
			owned_files: vec![make_owned_file("packages/app/index.ts", "mc-app", false)],
			edges: vec![],
			violations: vec![],
			dead_nodes: vec![
				// Dead symbol in owned file
				make_dead_node("packages/app/index.ts", false),
				// Dead symbol in unowned file (should be ignored)
				make_dead_node("packages/other/orphan.ts", false),
			],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert_eq!(result[0].dead_symbol_count, 1); // Only the owned one
	}

	// ── Sorting ────────────────────────────────────────────────────

	#[test]
	fn output_sorted_by_canonical_path() {
		let input = ModuleRollupInput {
			modules: vec![
				make_module("mc-z", "packages/zebra"),
				make_module("mc-a", "packages/alpha"),
				make_module("mc-m", "packages/middle"),
			],
			owned_files: vec![],
			edges: vec![],
			violations: vec![],
			dead_nodes: vec![],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert_eq!(result.len(), 3);
		assert_eq!(result[0].canonical_path, "packages/alpha");
		assert_eq!(result[1].canonical_path, "packages/middle");
		assert_eq!(result[2].canonical_path, "packages/zebra");
	}

	// ── Full integration scenario ──────────────────────────────────

	#[test]
	fn full_rollup_scenario() {
		let input = ModuleRollupInput {
			modules: vec![
				make_module("mc-app", "packages/app"),
				make_module("mc-core", "packages/core"),
			],
			owned_files: vec![
				make_owned_file("packages/app/index.ts", "mc-app", false),
				make_owned_file("packages/app/service.ts", "mc-app", false),
				make_owned_file("packages/app/index.test.ts", "mc-app", true),
				make_owned_file("packages/core/lib.ts", "mc-core", false),
			],
			edges: vec![
				// app depends on core with 10 imports
				make_edge("mc-app", "packages/app", "mc-core", "packages/core", 10),
			],
			violations: vec![
				// app violates a boundary
				make_violation("packages/app", "packages/core"),
			],
			dead_nodes: vec![
				// 2 dead in app non-test
				make_dead_node("packages/app/index.ts", false),
				make_dead_node("packages/app/service.ts", false),
				// 1 dead in app test
				make_dead_node("packages/app/index.test.ts", true),
				// 1 dead in core
				make_dead_node("packages/core/lib.ts", false),
			],
		};

		let result = compute_module_rollups(&input).unwrap();
		assert_eq!(result.len(), 2);

		// packages/app comes first (sorted)
		let app = &result[0];
		assert_eq!(app.canonical_path, "packages/app");
		assert_eq!(app.owned_file_count, 2);
		assert_eq!(app.owned_test_file_count, 1);
		assert_eq!(app.outbound_dependency_count, 1);
		assert_eq!(app.outbound_import_count, 10);
		assert_eq!(app.inbound_dependency_count, 0);
		assert_eq!(app.inbound_import_count, 0);
		assert_eq!(app.violation_count, 1);
		assert_eq!(app.dead_symbol_count, 2);
		assert_eq!(app.dead_test_symbol_count, 1);

		// packages/core
		let core = &result[1];
		assert_eq!(core.canonical_path, "packages/core");
		assert_eq!(core.owned_file_count, 1);
		assert_eq!(core.owned_test_file_count, 0);
		assert_eq!(core.outbound_dependency_count, 0);
		assert_eq!(core.outbound_import_count, 0);
		assert_eq!(core.inbound_dependency_count, 1);
		assert_eq!(core.inbound_import_count, 10);
		assert_eq!(core.violation_count, 0);
		assert_eq!(core.dead_symbol_count, 1);
		assert_eq!(core.dead_test_symbol_count, 0);
	}
}
