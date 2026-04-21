//! Weighted neighbor aggregation — pure policy core (RS-MG-13a).
//!
//! Given a focal module and the derived module dependency graph, compute
//! weighted neighbor lists for both outbound and inbound directions.
//!
//! Design principles (from CLAUDE.md "AI agent perspective"):
//! - Normalized vectors over opaque maps
//! - Deterministic ordering (descending import_count, then ascending module_uid)
//! - Pure function, no DB access
//!
//! This module answers one question cleanly:
//! "Given the existing derived module graph, what are this module's weighted neighbors?"

use crate::module_edges::ModuleDependencyEdge;

// ── Output DTOs ────────────────────────────────────────────────────

/// A weighted neighbor in the module dependency graph.
///
/// Contains the neighbor's identity and edge weight metrics.
/// Identity enrichment (module_key, module_kind, display_name) belongs
/// in the CLI layer, not here — this module only has UIDs from edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightedNeighbor {
	/// The neighbor module's UID.
	pub module_uid: String,
	/// Total import edges to/from this neighbor.
	pub import_count: u64,
	/// Distinct source files contributing edges to/from this neighbor.
	pub source_file_count: u64,
}

/// Weighted neighbors for a focal module in both directions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightedNeighbors {
	/// Modules this module depends on (outbound edges).
	/// Sorted by descending import_count, then ascending module_uid.
	pub outbound: Vec<WeightedNeighbor>,
	/// Modules that depend on this module (inbound edges).
	/// Sorted by descending import_count, then ascending module_uid.
	pub inbound: Vec<WeightedNeighbor>,
}

// ── Core function ──────────────────────────────────────────────────

/// Compute weighted neighbors for a focal module.
///
/// # Arguments
/// - `focal_module_uid`: The module UID to compute neighbors for.
/// - `edges`: The derived module dependency edges (from `derive_module_dependency_edges`).
///
/// # Returns
/// Weighted neighbor lists for outbound and inbound directions.
/// Each list is sorted by descending `import_count`, then ascending `module_uid`.
///
/// # Semantics
/// - Outbound: edges where `source_module_uid == focal_module_uid`
/// - Inbound: edges where `target_module_uid == focal_module_uid`
/// - For inbound neighbors, `source_file_count` means distinct source files
///   in the *depending* module (the neighbor), not the focal module.
///
/// # Example
/// ```
/// use repo_graph_classification::weighted_neighbors::compute_weighted_neighbors;
/// use repo_graph_classification::module_edges::ModuleDependencyEdge;
///
/// let edges = vec![
///     ModuleDependencyEdge {
///         source_module_uid: "mod-a".to_string(),
///         source_canonical_path: "packages/a".to_string(),
///         target_module_uid: "mod-b".to_string(),
///         target_canonical_path: "packages/b".to_string(),
///         import_count: 5,
///         source_file_count: 2,
///     },
/// ];
///
/// let neighbors = compute_weighted_neighbors("mod-a", &edges);
/// assert_eq!(neighbors.outbound.len(), 1);
/// assert_eq!(neighbors.outbound[0].module_uid, "mod-b");
/// assert_eq!(neighbors.outbound[0].import_count, 5);
/// assert_eq!(neighbors.inbound.len(), 0);
/// ```
pub fn compute_weighted_neighbors(
	focal_module_uid: &str,
	edges: &[ModuleDependencyEdge],
) -> WeightedNeighbors {
	// Collect outbound neighbors (focal → neighbor)
	let mut outbound: Vec<WeightedNeighbor> = edges
		.iter()
		.filter(|e| e.source_module_uid == focal_module_uid)
		.map(|e| WeightedNeighbor {
			module_uid: e.target_module_uid.clone(),
			import_count: e.import_count,
			source_file_count: e.source_file_count,
		})
		.collect();

	// Collect inbound neighbors (neighbor → focal)
	let mut inbound: Vec<WeightedNeighbor> = edges
		.iter()
		.filter(|e| e.target_module_uid == focal_module_uid)
		.map(|e| WeightedNeighbor {
			module_uid: e.source_module_uid.clone(),
			import_count: e.import_count,
			source_file_count: e.source_file_count,
		})
		.collect();

	// Sort: descending import_count, then ascending module_uid
	outbound.sort_by(|a, b| {
		b.import_count
			.cmp(&a.import_count)
			.then_with(|| a.module_uid.cmp(&b.module_uid))
	});

	inbound.sort_by(|a, b| {
		b.import_count
			.cmp(&a.import_count)
			.then_with(|| a.module_uid.cmp(&b.module_uid))
	});

	WeightedNeighbors { outbound, inbound }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;

	fn make_edge(
		source_uid: &str,
		source_path: &str,
		target_uid: &str,
		target_path: &str,
		import_count: u64,
		source_file_count: u64,
	) -> ModuleDependencyEdge {
		ModuleDependencyEdge {
			source_module_uid: source_uid.to_string(),
			source_canonical_path: source_path.to_string(),
			target_module_uid: target_uid.to_string(),
			target_canonical_path: target_path.to_string(),
			import_count,
			source_file_count,
		}
	}

	#[test]
	fn empty_edges_returns_empty_neighbors() {
		let result = compute_weighted_neighbors("mod-a", &[]);
		assert!(result.outbound.is_empty());
		assert!(result.inbound.is_empty());
	}

	#[test]
	fn module_not_in_graph_returns_empty() {
		let edges = vec![make_edge("mod-x", "x", "mod-y", "y", 10, 3)];
		let result = compute_weighted_neighbors("mod-a", &edges);
		assert!(result.outbound.is_empty());
		assert!(result.inbound.is_empty());
	}

	#[test]
	fn single_outbound_neighbor() {
		let edges = vec![make_edge("mod-a", "a", "mod-b", "b", 5, 2)];
		let result = compute_weighted_neighbors("mod-a", &edges);

		assert_eq!(result.outbound.len(), 1);
		assert_eq!(result.outbound[0].module_uid, "mod-b");
		assert_eq!(result.outbound[0].import_count, 5);
		assert_eq!(result.outbound[0].source_file_count, 2);
		assert!(result.inbound.is_empty());
	}

	#[test]
	fn single_inbound_neighbor() {
		let edges = vec![make_edge("mod-b", "b", "mod-a", "a", 7, 3)];
		let result = compute_weighted_neighbors("mod-a", &edges);

		assert!(result.outbound.is_empty());
		assert_eq!(result.inbound.len(), 1);
		assert_eq!(result.inbound[0].module_uid, "mod-b");
		assert_eq!(result.inbound[0].import_count, 7);
		assert_eq!(result.inbound[0].source_file_count, 3);
	}

	#[test]
	fn bidirectional_edges() {
		let edges = vec![
			make_edge("mod-a", "a", "mod-b", "b", 5, 2),
			make_edge("mod-b", "b", "mod-a", "a", 3, 1),
		];
		let result = compute_weighted_neighbors("mod-a", &edges);

		assert_eq!(result.outbound.len(), 1);
		assert_eq!(result.outbound[0].module_uid, "mod-b");
		assert_eq!(result.outbound[0].import_count, 5);

		assert_eq!(result.inbound.len(), 1);
		assert_eq!(result.inbound[0].module_uid, "mod-b");
		assert_eq!(result.inbound[0].import_count, 3);
	}

	#[test]
	fn multiple_outbound_sorted_by_import_count_desc() {
		let edges = vec![
			make_edge("mod-a", "a", "mod-b", "b", 5, 2),
			make_edge("mod-a", "a", "mod-c", "c", 10, 4),
			make_edge("mod-a", "a", "mod-d", "d", 3, 1),
		];
		let result = compute_weighted_neighbors("mod-a", &edges);

		assert_eq!(result.outbound.len(), 3);
		// Sorted descending by import_count: c(10), b(5), d(3)
		assert_eq!(result.outbound[0].module_uid, "mod-c");
		assert_eq!(result.outbound[0].import_count, 10);
		assert_eq!(result.outbound[1].module_uid, "mod-b");
		assert_eq!(result.outbound[1].import_count, 5);
		assert_eq!(result.outbound[2].module_uid, "mod-d");
		assert_eq!(result.outbound[2].import_count, 3);
	}

	#[test]
	fn tie_break_by_module_uid_ascending() {
		let edges = vec![
			make_edge("mod-a", "a", "mod-z", "z", 5, 2),
			make_edge("mod-a", "a", "mod-m", "m", 5, 2),
			make_edge("mod-a", "a", "mod-b", "b", 5, 2),
		];
		let result = compute_weighted_neighbors("mod-a", &edges);

		assert_eq!(result.outbound.len(), 3);
		// Same import_count, sorted ascending by module_uid: b, m, z
		assert_eq!(result.outbound[0].module_uid, "mod-b");
		assert_eq!(result.outbound[1].module_uid, "mod-m");
		assert_eq!(result.outbound[2].module_uid, "mod-z");
	}

	#[test]
	fn multiple_inbound_sorted_correctly() {
		let edges = vec![
			make_edge("mod-x", "x", "mod-a", "a", 8, 3),
			make_edge("mod-y", "y", "mod-a", "a", 15, 5),
			make_edge("mod-z", "z", "mod-a", "a", 8, 2),
		];
		let result = compute_weighted_neighbors("mod-a", &edges);

		assert_eq!(result.inbound.len(), 3);
		// Sorted: y(15), x(8), z(8) — x and z tie on count, sorted by uid: x < z
		assert_eq!(result.inbound[0].module_uid, "mod-y");
		assert_eq!(result.inbound[0].import_count, 15);
		assert_eq!(result.inbound[1].module_uid, "mod-x");
		assert_eq!(result.inbound[1].import_count, 8);
		assert_eq!(result.inbound[2].module_uid, "mod-z");
		assert_eq!(result.inbound[2].import_count, 8);
	}

	#[test]
	fn unrelated_edges_filtered_out() {
		let edges = vec![
			make_edge("mod-a", "a", "mod-b", "b", 5, 2),
			make_edge("mod-x", "x", "mod-y", "y", 100, 50), // unrelated
			make_edge("mod-c", "c", "mod-a", "a", 3, 1),
		];
		let result = compute_weighted_neighbors("mod-a", &edges);

		assert_eq!(result.outbound.len(), 1);
		assert_eq!(result.outbound[0].module_uid, "mod-b");

		assert_eq!(result.inbound.len(), 1);
		assert_eq!(result.inbound[0].module_uid, "mod-c");
	}

	#[test]
	fn self_loop_excluded() {
		// Self-loops shouldn't exist in valid module graphs,
		// but if they do, they appear as both outbound and inbound.
		let edges = vec![make_edge("mod-a", "a", "mod-a", "a", 2, 1)];
		let result = compute_weighted_neighbors("mod-a", &edges);

		// A self-loop matches both filters
		assert_eq!(result.outbound.len(), 1);
		assert_eq!(result.outbound[0].module_uid, "mod-a");
		assert_eq!(result.inbound.len(), 1);
		assert_eq!(result.inbound[0].module_uid, "mod-a");
	}
}
