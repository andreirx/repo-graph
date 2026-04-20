//! Module edge derivation — pure policy core (RS-MG-2).
//!
//! Derives cross-module dependency edges from:
//! - Resolved import facts
//! - File ownership facts
//! - Module identity refs
//!
//! This is pure policy. No DB access. No side effects.
//!
//! Design decisions:
//! - Input DTOs are normalized (not storage row shapes)
//! - Duplicate file ownership is an explicit error
//! - Output is deterministically sorted
//! - Counts use u64

use std::collections::{HashMap, HashSet};

// ── Input DTOs ─────────────────────────────────────────────────────

/// A resolved import between two files.
///
/// Minimal normalized fact for derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImportFact {
	pub source_file_uid: String,
	pub target_file_uid: String,
}

/// A file ownership assignment.
///
/// Minimal normalized fact for derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOwnershipFact {
	pub file_uid: String,
	pub module_uid: String,
}

/// Module identity reference.
///
/// Minimal normalized ref for derivation output enrichment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRef {
	pub module_uid: String,
	pub canonical_path: String,
}

/// Input bundle for module edge derivation.
#[derive(Debug, Clone)]
pub struct ModuleEdgeDerivationInput {
	pub imports: Vec<ResolvedImportFact>,
	pub ownership: Vec<FileOwnershipFact>,
	pub modules: Vec<ModuleRef>,
}

// ── Output DTOs ────────────────────────────────────────────────────

/// A derived cross-module dependency edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDependencyEdge {
	pub source_module_uid: String,
	pub source_canonical_path: String,
	pub target_module_uid: String,
	pub target_canonical_path: String,
	pub import_count: u64,
	pub source_file_count: u64,
}

// ── Errors ─────────────────────────────────────────────────────────

/// Error during module edge derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleEdgeDerivationError {
	/// A file has multiple ownership assignments.
	///
	/// Module edge derivation requires single-valued ownership.
	/// Duplicate ownership produces unstable architecture facts.
	DuplicateOwnership {
		file_uid: String,
		module_uids: Vec<String>,
	},
}

impl std::fmt::Display for ModuleEdgeDerivationError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ModuleEdgeDerivationError::DuplicateOwnership {
				file_uid,
				module_uids,
			} => {
				write!(
					f,
					"file {} has duplicate ownership: {:?}",
					file_uid, module_uids
				)
			}
		}
	}
}

impl std::error::Error for ModuleEdgeDerivationError {}

// ── Derivation result ──────────────────────────────────────────────

/// Result of module edge derivation.
#[derive(Debug, Clone)]
pub struct ModuleEdgeDerivationResult {
	/// Derived cross-module edges, sorted by (source_path, target_path).
	pub edges: Vec<ModuleDependencyEdge>,
	/// Diagnostic counts.
	pub diagnostics: ModuleEdgeDiagnostics,
}

/// Diagnostic counts from derivation.
#[derive(Debug, Clone, Default)]
pub struct ModuleEdgeDiagnostics {
	/// Total resolved import facts processed.
	pub imports_total: u64,
	/// Imports where source file has no ownership (excluded).
	pub imports_source_unowned: u64,
	/// Imports where target file has no ownership (excluded).
	pub imports_target_unowned: u64,
	/// Imports within the same module (excluded).
	pub imports_intra_module: u64,
	/// Imports crossing module boundaries (included).
	pub imports_cross_module: u64,
}

// ── Pure derivation function ───────────────────────────────────────

/// Derive cross-module dependency edges from raw facts.
///
/// Algorithm:
/// 1. Build file → module ownership index (error on duplicates)
/// 2. Build module_uid → canonical_path lookup
/// 3. For each resolved import:
///    - Look up source file's module
///    - Look up target file's module
///    - If both owned AND different modules → cross-module edge
/// 4. Aggregate by (source_module, target_module)
/// 5. Sort deterministically
///
/// Returns error if any file has duplicate ownership assignments.
pub fn derive_module_dependency_edges(
	input: ModuleEdgeDerivationInput,
) -> Result<ModuleEdgeDerivationResult, ModuleEdgeDerivationError> {
	// 1. Build ownership index, detecting duplicates
	let ownership_index = build_ownership_index(&input.ownership)?;

	// 2. Build module lookup
	let module_lookup: HashMap<&str, &str> = input
		.modules
		.iter()
		.map(|m| (m.module_uid.as_str(), m.canonical_path.as_str()))
		.collect();

	// 3. Process imports and aggregate
	let mut diagnostics = ModuleEdgeDiagnostics::default();
	let mut edge_aggregates: HashMap<(&str, &str), EdgeAggregate> = HashMap::new();

	for import in &input.imports {
		diagnostics.imports_total += 1;

		// Look up source module
		let source_module = match ownership_index.get(import.source_file_uid.as_str()) {
			Some(m) => *m,
			None => {
				diagnostics.imports_source_unowned += 1;
				continue;
			}
		};

		// Look up target module
		let target_module = match ownership_index.get(import.target_file_uid.as_str()) {
			Some(m) => *m,
			None => {
				diagnostics.imports_target_unowned += 1;
				continue;
			}
		};

		// Skip intra-module imports
		if source_module == target_module {
			diagnostics.imports_intra_module += 1;
			continue;
		}

		diagnostics.imports_cross_module += 1;

		// Aggregate
		let agg = edge_aggregates
			.entry((source_module, target_module))
			.or_default();
		agg.import_count += 1;
		agg.source_files.insert(&import.source_file_uid);
	}

	// 4. Build output edges
	let mut edges: Vec<ModuleDependencyEdge> = edge_aggregates
		.into_iter()
		.filter_map(|((source_uid, target_uid), agg)| {
			let source_path = module_lookup.get(source_uid)?;
			let target_path = module_lookup.get(target_uid)?;
			Some(ModuleDependencyEdge {
				source_module_uid: source_uid.to_string(),
				source_canonical_path: (*source_path).to_string(),
				target_module_uid: target_uid.to_string(),
				target_canonical_path: (*target_path).to_string(),
				import_count: agg.import_count,
				source_file_count: agg.source_files.len() as u64,
			})
		})
		.collect();

	// 5. Sort deterministically
	edges.sort_by(|a, b| {
		a.source_canonical_path
			.cmp(&b.source_canonical_path)
			.then_with(|| a.target_canonical_path.cmp(&b.target_canonical_path))
	});

	Ok(ModuleEdgeDerivationResult { edges, diagnostics })
}

// ── Internal helpers ───────────────────────────────────────────────

/// Aggregation state for a single (source, target) module pair.
#[derive(Debug, Default)]
struct EdgeAggregate<'a> {
	import_count: u64,
	source_files: HashSet<&'a str>,
}

/// Build file → module ownership index, erroring on duplicates.
fn build_ownership_index(
	ownership: &[FileOwnershipFact],
) -> Result<HashMap<&str, &str>, ModuleEdgeDerivationError> {
	// First pass: collect all assignments per file
	let mut file_to_modules: HashMap<&str, Vec<&str>> = HashMap::new();
	for fact in ownership {
		file_to_modules
			.entry(fact.file_uid.as_str())
			.or_default()
			.push(fact.module_uid.as_str());
	}

	// Second pass: detect duplicates and build index
	let mut index: HashMap<&str, &str> = HashMap::new();
	for (file_uid, module_uids) in file_to_modules {
		if module_uids.len() > 1 {
			return Err(ModuleEdgeDerivationError::DuplicateOwnership {
				file_uid: file_uid.to_string(),
				module_uids: module_uids.into_iter().map(String::from).collect(),
			});
		}
		index.insert(file_uid, module_uids[0]);
	}

	Ok(index)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_import(source: &str, target: &str) -> ResolvedImportFact {
		ResolvedImportFact {
			source_file_uid: source.to_string(),
			target_file_uid: target.to_string(),
		}
	}

	fn make_ownership(file: &str, module: &str) -> FileOwnershipFact {
		FileOwnershipFact {
			file_uid: file.to_string(),
			module_uid: module.to_string(),
		}
	}

	fn make_module(uid: &str, path: &str) -> ModuleRef {
		ModuleRef {
			module_uid: uid.to_string(),
			canonical_path: path.to_string(),
		}
	}

	// ── Basic derivation ───────────────────────────────────────────

	#[test]
	fn empty_input_produces_empty_output() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![],
			ownership: vec![],
			modules: vec![],
		};

		let result = derive_module_dependency_edges(input).expect("derivation");

		assert!(result.edges.is_empty());
		assert_eq!(result.diagnostics.imports_total, 0);
	}

	#[test]
	fn cross_module_import_produces_edge() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![make_import("file-a", "file-b")],
			ownership: vec![
				make_ownership("file-a", "mod-app"),
				make_ownership("file-b", "mod-core"),
			],
			modules: vec![
				make_module("mod-app", "packages/app"),
				make_module("mod-core", "packages/core"),
			],
		};

		let result = derive_module_dependency_edges(input).expect("derivation");

		assert_eq!(result.edges.len(), 1);
		let edge = &result.edges[0];
		assert_eq!(edge.source_module_uid, "mod-app");
		assert_eq!(edge.source_canonical_path, "packages/app");
		assert_eq!(edge.target_module_uid, "mod-core");
		assert_eq!(edge.target_canonical_path, "packages/core");
		assert_eq!(edge.import_count, 1);
		assert_eq!(edge.source_file_count, 1);
	}

	#[test]
	fn intra_module_import_excluded() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![make_import("file-a", "file-b")],
			ownership: vec![
				make_ownership("file-a", "mod-app"),
				make_ownership("file-b", "mod-app"), // same module
			],
			modules: vec![make_module("mod-app", "packages/app")],
		};

		let result = derive_module_dependency_edges(input).expect("derivation");

		assert!(result.edges.is_empty());
		assert_eq!(result.diagnostics.imports_intra_module, 1);
		assert_eq!(result.diagnostics.imports_cross_module, 0);
	}

	#[test]
	fn unowned_source_excluded() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![make_import("unowned-file", "file-b")],
			ownership: vec![make_ownership("file-b", "mod-core")],
			modules: vec![make_module("mod-core", "packages/core")],
		};

		let result = derive_module_dependency_edges(input).expect("derivation");

		assert!(result.edges.is_empty());
		assert_eq!(result.diagnostics.imports_source_unowned, 1);
	}

	#[test]
	fn unowned_target_excluded() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![make_import("file-a", "unowned-file")],
			ownership: vec![make_ownership("file-a", "mod-app")],
			modules: vec![make_module("mod-app", "packages/app")],
		};

		let result = derive_module_dependency_edges(input).expect("derivation");

		assert!(result.edges.is_empty());
		assert_eq!(result.diagnostics.imports_target_unowned, 1);
	}

	// ── Aggregation ────────────────────────────────────────────────

	#[test]
	fn multiple_imports_aggregated() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![
				make_import("file-a1", "file-b"),
				make_import("file-a2", "file-b"),
				make_import("file-a1", "file-b"), // duplicate import from same file
			],
			ownership: vec![
				make_ownership("file-a1", "mod-app"),
				make_ownership("file-a2", "mod-app"),
				make_ownership("file-b", "mod-core"),
			],
			modules: vec![
				make_module("mod-app", "packages/app"),
				make_module("mod-core", "packages/core"),
			],
		};

		let result = derive_module_dependency_edges(input).expect("derivation");

		assert_eq!(result.edges.len(), 1);
		let edge = &result.edges[0];
		assert_eq!(edge.import_count, 3); // all 3 imports counted
		assert_eq!(edge.source_file_count, 2); // only 2 distinct source files
	}

	// ── Duplicate ownership error ──────────────────────────────────

	#[test]
	fn duplicate_ownership_returns_error() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![],
			ownership: vec![
				make_ownership("shared-file", "mod-1"),
				make_ownership("shared-file", "mod-2"), // duplicate
			],
			modules: vec![
				make_module("mod-1", "packages/mod1"),
				make_module("mod-2", "packages/mod2"),
			],
		};

		let result = derive_module_dependency_edges(input);

		assert!(result.is_err());
		match result.unwrap_err() {
			ModuleEdgeDerivationError::DuplicateOwnership {
				file_uid,
				module_uids,
			} => {
				assert_eq!(file_uid, "shared-file");
				assert_eq!(module_uids.len(), 2);
				assert!(module_uids.contains(&"mod-1".to_string()));
				assert!(module_uids.contains(&"mod-2".to_string()));
			}
		}
	}

	// ── Deterministic ordering ─────────────────────────────────────

	#[test]
	fn edges_sorted_by_source_then_target_path() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![
				make_import("file-c", "file-a"),
				make_import("file-a", "file-c"),
				make_import("file-b", "file-c"),
			],
			ownership: vec![
				make_ownership("file-a", "mod-a"),
				make_ownership("file-b", "mod-b"),
				make_ownership("file-c", "mod-c"),
			],
			modules: vec![
				make_module("mod-a", "packages/alpha"),
				make_module("mod-b", "packages/beta"),
				make_module("mod-c", "packages/charlie"),
			],
		};

		let result = derive_module_dependency_edges(input).expect("derivation");

		assert_eq!(result.edges.len(), 3);
		// Sorted by (source_path, target_path)
		assert_eq!(result.edges[0].source_canonical_path, "packages/alpha");
		assert_eq!(result.edges[0].target_canonical_path, "packages/charlie");
		assert_eq!(result.edges[1].source_canonical_path, "packages/beta");
		assert_eq!(result.edges[1].target_canonical_path, "packages/charlie");
		assert_eq!(result.edges[2].source_canonical_path, "packages/charlie");
		assert_eq!(result.edges[2].target_canonical_path, "packages/alpha");
	}

	// ── Diagnostics ────────────────────────────────────────────────

	#[test]
	fn diagnostics_counts_all_categories() {
		let input = ModuleEdgeDerivationInput {
			imports: vec![
				make_import("file-a", "file-b"),       // cross-module
				make_import("file-a", "file-a2"),      // intra-module
				make_import("unowned", "file-b"),      // source unowned
				make_import("file-a", "unowned"),      // target unowned
				make_import("file-b", "file-a"),       // cross-module
			],
			ownership: vec![
				make_ownership("file-a", "mod-app"),
				make_ownership("file-a2", "mod-app"),
				make_ownership("file-b", "mod-core"),
			],
			modules: vec![
				make_module("mod-app", "packages/app"),
				make_module("mod-core", "packages/core"),
			],
		};

		let result = derive_module_dependency_edges(input).expect("derivation");

		assert_eq!(result.diagnostics.imports_total, 5);
		assert_eq!(result.diagnostics.imports_cross_module, 2);
		assert_eq!(result.diagnostics.imports_intra_module, 1);
		assert_eq!(result.diagnostics.imports_source_unowned, 1);
		assert_eq!(result.diagnostics.imports_target_unowned, 1);
	}
}
