//! Boundary violation evaluator — pure policy core (RS-MG-4).
//!
//! Evaluates discovered-module boundary declarations against the
//! current module dependency graph. Detects violations (forbidden
//! edges that exist) and stale declarations (boundaries referencing
//! modules that no longer exist).
//!
//! Design decisions (locked from TS MG-2b):
//! - Stale and violation are mutually exclusive
//! - Deterministic ordering explicit in code
//! - Typed enum for stale side
//! - For Both: missing_paths ordered source first, target second
//!
//! This is pure policy. No storage access. No side effects.

use std::collections::HashMap;

use crate::boundary_parser::EvaluatableBoundary;
use crate::module_edges::ModuleDependencyEdge;

// ── Stale side enum ────────────────────────────────────────────────

/// Which side of a boundary declaration is stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleSide {
	/// Source module no longer exists.
	Source,
	/// Target module no longer exists.
	Target,
	/// Both source and target modules no longer exist.
	Both,
}

// ── Output DTOs ────────────────────────────────────────────────────

/// A boundary violation: a forbidden edge exists in the module graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleBoundaryViolation {
	pub declaration_uid: String,
	pub source_canonical_path: String,
	pub target_canonical_path: String,
	pub import_count: u64,
	pub source_file_count: u64,
	pub reason: Option<String>,
}

/// A stale boundary declaration: one or both modules no longer exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleBoundaryDeclaration {
	pub declaration_uid: String,
	pub stale_side: StaleSide,
	/// Missing paths. For `Both`: source first, target second.
	pub missing_paths: Vec<String>,
}

/// Evaluation result.
#[derive(Debug, Clone)]
pub struct ModuleBoundaryEvaluation {
	/// Violations, sorted by (source_path, target_path, declaration_uid).
	pub violations: Vec<ModuleBoundaryViolation>,
	/// Stale declarations, sorted by declaration_uid.
	pub stale_declarations: Vec<StaleBoundaryDeclaration>,
}

// ── Pure evaluator ─────────────────────────────────────────────────

/// Evaluate module boundaries against the current module graph.
///
/// For each boundary declaration:
/// 1. Check if source and target modules exist in the current snapshot
/// 2. If either is missing → report as stale, skip violation check
/// 3. If both exist → check if a forbidden edge exists
/// 4. If edge exists → report as violation
///
/// Output is deterministically ordered:
/// - stale_declarations: sorted by declaration_uid
/// - violations: sorted by (source_path, target_path, declaration_uid)
///
/// # Arguments
///
/// * `boundaries` - Parsed discovered-module boundary declarations
/// * `edges` - Derived cross-module dependency edges
/// * `module_index` - Current snapshot module set (canonical_path → module_uid).
///   The module_uid value is opaque; only key presence is checked.
pub fn evaluate_module_boundaries(
	boundaries: &[EvaluatableBoundary],
	edges: &[ModuleDependencyEdge],
	module_index: &HashMap<String, String>,
) -> ModuleBoundaryEvaluation {
	// Build edge lookup: "source_path|target_path" → edge
	let edge_map: HashMap<(&str, &str), &ModuleDependencyEdge> = edges
		.iter()
		.map(|e| {
			(
				(
					e.source_canonical_path.as_str(),
					e.target_canonical_path.as_str(),
				),
				e,
			)
		})
		.collect();

	let mut violations: Vec<ModuleBoundaryViolation> = Vec::new();
	let mut stale_declarations: Vec<StaleBoundaryDeclaration> = Vec::new();

	for boundary in boundaries {
		let source_exists = module_index.contains_key(&boundary.source_canonical_path);
		let target_exists = module_index.contains_key(&boundary.target_canonical_path);

		// Check for staleness first — stale declarations skip violation check
		if !source_exists || !target_exists {
			let (stale_side, missing_paths) = match (source_exists, target_exists) {
				(false, false) => (
					StaleSide::Both,
					// Source first, target second — locked ordering
					vec![
						boundary.source_canonical_path.clone(),
						boundary.target_canonical_path.clone(),
					],
				),
				(false, true) => (
					StaleSide::Source,
					vec![boundary.source_canonical_path.clone()],
				),
				(true, false) => (
					StaleSide::Target,
					vec![boundary.target_canonical_path.clone()],
				),
				(true, true) => unreachable!(),
			};

			stale_declarations.push(StaleBoundaryDeclaration {
				declaration_uid: boundary.declaration_uid.clone(),
				stale_side,
				missing_paths,
			});
			continue; // Do not evaluate for violations
		}

		// Both modules exist — check if forbidden edge exists
		let edge_key = (
			boundary.source_canonical_path.as_str(),
			boundary.target_canonical_path.as_str(),
		);

		if let Some(edge) = edge_map.get(&edge_key) {
			violations.push(ModuleBoundaryViolation {
				declaration_uid: boundary.declaration_uid.clone(),
				source_canonical_path: boundary.source_canonical_path.clone(),
				target_canonical_path: boundary.target_canonical_path.clone(),
				import_count: edge.import_count,
				source_file_count: edge.source_file_count,
				reason: boundary.reason.clone(),
			});
		}
	}

	// Sort for deterministic output — explicit, not relying on input order
	stale_declarations.sort_by(|a, b| a.declaration_uid.cmp(&b.declaration_uid));

	violations.sort_by(|a, b| {
		a.source_canonical_path
			.cmp(&b.source_canonical_path)
			.then_with(|| a.target_canonical_path.cmp(&b.target_canonical_path))
			.then_with(|| a.declaration_uid.cmp(&b.declaration_uid))
	});

	ModuleBoundaryEvaluation {
		violations,
		stale_declarations,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_boundary(uid: &str, source: &str, target: &str) -> EvaluatableBoundary {
		EvaluatableBoundary {
			declaration_uid: uid.to_string(),
			source_canonical_path: source.to_string(),
			target_canonical_path: target.to_string(),
			reason: None,
		}
	}

	fn make_boundary_with_reason(
		uid: &str,
		source: &str,
		target: &str,
		reason: &str,
	) -> EvaluatableBoundary {
		EvaluatableBoundary {
			declaration_uid: uid.to_string(),
			source_canonical_path: source.to_string(),
			target_canonical_path: target.to_string(),
			reason: Some(reason.to_string()),
		}
	}

	fn make_edge(source: &str, target: &str, import_count: u64, file_count: u64) -> ModuleDependencyEdge {
		ModuleDependencyEdge {
			source_module_uid: format!("uid-{}", source.replace('/', "-")),
			source_canonical_path: source.to_string(),
			target_module_uid: format!("uid-{}", target.replace('/', "-")),
			target_canonical_path: target.to_string(),
			import_count,
			source_file_count: file_count,
		}
	}

	fn make_module_index(paths: &[&str]) -> HashMap<String, String> {
		paths
			.iter()
			.map(|p| (p.to_string(), format!("uid-{}", p.replace('/', "-"))))
			.collect()
	}

	// ── Violation detection ────────────────────────────────────────

	#[test]
	fn detects_violation_when_forbidden_edge_exists() {
		let boundaries = vec![make_boundary("decl-1", "packages/app", "packages/db")];
		let edges = vec![make_edge("packages/app", "packages/db", 3, 2)];
		let index = make_module_index(&["packages/app", "packages/db"]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert_eq!(result.violations.len(), 1);
		assert!(result.stale_declarations.is_empty());
		assert_eq!(result.violations[0].declaration_uid, "decl-1");
		assert_eq!(result.violations[0].source_canonical_path, "packages/app");
		assert_eq!(result.violations[0].target_canonical_path, "packages/db");
		assert_eq!(result.violations[0].import_count, 3);
		assert_eq!(result.violations[0].source_file_count, 2);
	}

	#[test]
	fn includes_reason_in_violation() {
		let boundaries = vec![make_boundary_with_reason(
			"decl-1",
			"packages/app",
			"packages/db",
			"UI must not access DB",
		)];
		let edges = vec![make_edge("packages/app", "packages/db", 1, 1)];
		let index = make_module_index(&["packages/app", "packages/db"]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert_eq!(result.violations[0].reason, Some("UI must not access DB".to_string()));
	}

	#[test]
	fn no_violation_when_edge_does_not_exist() {
		let boundaries = vec![make_boundary("decl-1", "packages/app", "packages/db")];
		let edges = vec![make_edge("packages/app", "packages/core", 1, 1)]; // different target
		let index = make_module_index(&["packages/app", "packages/db", "packages/core"]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert!(result.violations.is_empty());
		assert!(result.stale_declarations.is_empty());
	}

	#[test]
	fn multiple_boundaries_mixed_violations() {
		let boundaries = vec![
			make_boundary("decl-1", "packages/app", "packages/db"),
			make_boundary("decl-2", "packages/app", "packages/core"),
			make_boundary("decl-3", "packages/api", "packages/db"),
		];
		let edges = vec![
			make_edge("packages/app", "packages/db", 1, 1),  // violates decl-1
			make_edge("packages/api", "packages/db", 1, 1), // violates decl-3
		];
		let index = make_module_index(&[
			"packages/app",
			"packages/db",
			"packages/core",
			"packages/api",
		]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert_eq!(result.violations.len(), 2);
		let uids: Vec<_> = result.violations.iter().map(|v| &v.declaration_uid).collect();
		assert!(uids.contains(&&"decl-1".to_string()));
		assert!(uids.contains(&&"decl-3".to_string()));
	}

	// ── Stale detection ────────────────────────────────────────────

	#[test]
	fn detects_stale_source() {
		let boundaries = vec![make_boundary("decl-1", "packages/removed", "packages/db")];
		let edges: Vec<ModuleDependencyEdge> = vec![];
		let index = make_module_index(&["packages/db"]); // source missing

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert!(result.violations.is_empty());
		assert_eq!(result.stale_declarations.len(), 1);
		assert_eq!(result.stale_declarations[0].declaration_uid, "decl-1");
		assert_eq!(result.stale_declarations[0].stale_side, StaleSide::Source);
		assert_eq!(result.stale_declarations[0].missing_paths, vec!["packages/removed"]);
	}

	#[test]
	fn detects_stale_target() {
		let boundaries = vec![make_boundary("decl-1", "packages/app", "packages/removed")];
		let edges: Vec<ModuleDependencyEdge> = vec![];
		let index = make_module_index(&["packages/app"]); // target missing

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert!(result.violations.is_empty());
		assert_eq!(result.stale_declarations.len(), 1);
		assert_eq!(result.stale_declarations[0].stale_side, StaleSide::Target);
		assert_eq!(result.stale_declarations[0].missing_paths, vec!["packages/removed"]);
	}

	#[test]
	fn detects_stale_both_with_source_first_ordering() {
		let boundaries = vec![make_boundary("decl-1", "packages/old-app", "packages/old-db")];
		let edges: Vec<ModuleDependencyEdge> = vec![];
		let index = make_module_index(&["packages/current"]); // both missing

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert!(result.violations.is_empty());
		assert_eq!(result.stale_declarations.len(), 1);
		assert_eq!(result.stale_declarations[0].stale_side, StaleSide::Both);
		// Source first, target second
		assert_eq!(
			result.stale_declarations[0].missing_paths,
			vec!["packages/old-app", "packages/old-db"]
		);
	}

	// ── Mutual exclusion ───────────────────────────────────────────

	#[test]
	fn stale_not_reported_as_violation() {
		// Even if an edge exists matching the boundary pattern,
		// a stale declaration should NOT produce a violation.
		let boundaries = vec![make_boundary("decl-1", "packages/removed", "packages/db")];
		let edges = vec![make_edge("packages/removed", "packages/db", 1, 1)];
		let index = make_module_index(&["packages/db"]); // source missing

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert_eq!(result.stale_declarations.len(), 1);
		assert!(result.violations.is_empty()); // NOT a violation
	}

	#[test]
	fn mixed_stale_and_live_boundaries() {
		let boundaries = vec![
			make_boundary("decl-1", "packages/app", "packages/db"), // live, violated
			make_boundary("decl-2", "packages/removed", "packages/db"), // stale source
			make_boundary("decl-3", "packages/app", "packages/core"), // live, not violated
		];
		let edges = vec![
			make_edge("packages/app", "packages/db", 1, 1),
			make_edge("packages/removed", "packages/db", 1, 1),
		];
		let index = make_module_index(&["packages/app", "packages/db", "packages/core"]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert_eq!(result.violations.len(), 1);
		assert_eq!(result.violations[0].declaration_uid, "decl-1");

		assert_eq!(result.stale_declarations.len(), 1);
		assert_eq!(result.stale_declarations[0].declaration_uid, "decl-2");
	}

	// ── Deterministic ordering ─────────────────────────────────────

	#[test]
	fn stale_sorted_by_declaration_uid() {
		let boundaries = vec![
			make_boundary("decl-z", "packages/z", "packages/db"),
			make_boundary("decl-a", "packages/a", "packages/db"),
			make_boundary("decl-m", "packages/m", "packages/db"),
		];
		let edges: Vec<ModuleDependencyEdge> = vec![];
		let index = make_module_index(&["packages/db"]); // all sources missing

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		let uids: Vec<_> = result
			.stale_declarations
			.iter()
			.map(|s| s.declaration_uid.as_str())
			.collect();
		assert_eq!(uids, vec!["decl-a", "decl-m", "decl-z"]);
	}

	#[test]
	fn violations_sorted_by_source_then_target_then_uid() {
		let boundaries = vec![
			make_boundary("decl-3", "packages/b", "packages/x"),
			make_boundary("decl-1", "packages/a", "packages/y"),
			make_boundary("decl-2", "packages/a", "packages/x"),
			make_boundary("decl-4", "packages/a", "packages/x"), // same path, different uid
		];
		let edges = vec![
			make_edge("packages/a", "packages/x", 1, 1),
			make_edge("packages/a", "packages/y", 1, 1),
			make_edge("packages/b", "packages/x", 1, 1),
		];
		let index = make_module_index(&["packages/a", "packages/b", "packages/x", "packages/y"]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		let uids: Vec<_> = result
			.violations
			.iter()
			.map(|v| v.declaration_uid.as_str())
			.collect();
		// Expected order:
		// 1. packages/a → packages/x, decl-2
		// 2. packages/a → packages/x, decl-4
		// 3. packages/a → packages/y, decl-1
		// 4. packages/b → packages/x, decl-3
		assert_eq!(uids, vec!["decl-2", "decl-4", "decl-1", "decl-3"]);
	}

	// ── Edge cases ─────────────────────────────────────────────────

	#[test]
	fn empty_input() {
		let boundaries: Vec<EvaluatableBoundary> = vec![];
		let edges: Vec<ModuleDependencyEdge> = vec![];
		let index: HashMap<String, String> = HashMap::new();

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert!(result.violations.is_empty());
		assert!(result.stale_declarations.is_empty());
	}

	#[test]
	fn boundaries_with_no_edges() {
		let boundaries = vec![make_boundary("decl-1", "packages/app", "packages/db")];
		let edges: Vec<ModuleDependencyEdge> = vec![];
		let index = make_module_index(&["packages/app", "packages/db"]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert!(result.violations.is_empty());
		assert!(result.stale_declarations.is_empty());
	}

	#[test]
	fn edges_with_no_boundaries() {
		let boundaries: Vec<EvaluatableBoundary> = vec![];
		let edges = vec![make_edge("packages/app", "packages/db", 1, 1)];
		let index = make_module_index(&["packages/app", "packages/db"]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert!(result.violations.is_empty());
		assert!(result.stale_declarations.is_empty());
	}

	#[test]
	fn reverse_edge_direction_not_matched() {
		// Boundary: app must not depend on db
		// Edge: db depends on app (reverse)
		// Should NOT be a violation
		let boundaries = vec![make_boundary("decl-1", "packages/app", "packages/db")];
		let edges = vec![make_edge("packages/db", "packages/app", 1, 1)]; // reverse
		let index = make_module_index(&["packages/app", "packages/db"]);

		let result = evaluate_module_boundaries(&boundaries, &edges, &index);

		assert!(result.violations.is_empty());
	}
}
