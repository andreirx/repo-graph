//! Trust storage read port (dependency-inverted interface).
//!
//! Per the D-4-1 lock: the trust crate (policy) defines the
//! interface it needs. The storage crate (adapter) implements it.
//! The dependency direction is adapter → policy (outer → inner),
//! which follows the Clean Architecture dependency rule.
//!
//! This module contains:
//!   - The `TrustStorageRead` trait with the 8 narrowest read
//!     methods the trust service needs.
//!   - Supporting DTOs for the trait's return types (owned by trust,
//!     not by storage). The storage implementation maps its internal
//!     row shapes to these trust-owned DTOs.
//!
//! ── Narrow surface design ─────────────────────────────────────
//!
//! Each trait method returns exactly the data the trust service
//! reads, not the full entity shape the storage crate might have.
//! Examples:
//!   - `get_file_paths_by_repo` returns `Vec<String>` (just paths),
//!     not `Vec<TrackedFile>`. The service only extracts `.path`.
//!   - `count_active_declarations` returns `usize` (count), not
//!     `Vec<Declaration>`. The service only calls `.length`.
//!   - `TrustModuleStats` carries 5 fields, not the full
//!     `ModuleStats` shape (which has 10 fields including
//!     instability, abstractness, etc. that trust never reads).

// Re-export classification types that appear in this module's
// public DTOs and trait signatures. Consumers (e.g., the storage
// crate implementing TrustStorageRead) need these types to
// construct inputs and interpret outputs without adding a direct
// dep on repo-graph-classification.
pub use repo_graph_classification::types::{
	UnresolvedEdgeBasisCode, UnresolvedEdgeCategory, UnresolvedEdgeClassification,
};
use serde::{Deserialize, Serialize};

// ── Supporting DTOs ──────────────────────────────────────────────

/// Per-module structural metrics as seen by the trust service.
///
/// Narrowed from the full `ModuleStats` (10 fields in TS) to the
/// 5 fields the trust service actually reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustModuleStats {
	pub stable_key: String,
	/// Module path (repo-relative qualified_name).
	pub path: String,
	pub fan_in: u64,
	pub fan_out: u64,
	pub file_count: u64,
}

/// A path-prefix module cycle (ancestor → descendant).
///
/// Mirror of `PathPrefixModuleCycle` from
/// `src/core/ports/storage.ts:782`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPrefixModuleCycle {
	pub ancestor_stable_key: String,
	pub descendant_stable_key: String,
}

/// One row from a classification-grouped unresolved-edge count.
///
/// Uses `UnresolvedEdgeClassification` as the typed key instead
/// of a raw string. The trust service dispatches on the
/// classification variant (e.g., finding the
/// `ExternalLibraryCandidate` count), so type safety here
/// eliminates raw-string comparison at the call site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassificationCountRow {
	pub classification: UnresolvedEdgeClassification,
	pub count: u64,
}

/// One row from a `query_unresolved_edges` sample query.
///
/// Narrowed to the fields the trust service reads. Uses the
/// typed classification enums from `repo-graph-classification`
/// instead of raw strings, because the trust crate already
/// depends on the classification crate and the service needs to
/// dispatch on these values (e.g., calling `derive_blast_radius`
/// with a typed `UnresolvedEdgeBasisCode`).
///
/// `source_node_visibility` stays `Option<String>` because the
/// visibility vocabulary (`"export"`, `"private"`, etc.) is
/// defined in `core/model/types.ts::Visibility`, which is NOT
/// in the classification crate's ported surface. The trust
/// service passes it through to `derive_blast_radius` as a
/// string slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustUnresolvedEdgeSample {
	pub category: UnresolvedEdgeCategory,
	pub basis_code: UnresolvedEdgeBasisCode,
	pub source_node_visibility: Option<String>,
	pub metadata_json: Option<String>,
}

/// Input for `count_unresolved_edges_by_classification`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountByClassificationInput {
	pub snapshot_uid: String,
	/// Optional filter: only count edges in these categories.
	/// Empty vec means no filtering (count all categories).
	pub filter_categories: Vec<UnresolvedEdgeCategory>,
}

/// Input for `query_unresolved_edges`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryUnresolvedEdgesInput {
	pub snapshot_uid: String,
	/// Filter to this classification value.
	pub classification: UnresolvedEdgeClassification,
	pub limit: usize,
}

// ── Trait ─────────────────────────────────────────────────────────

/// The narrow read port the trust service needs from a storage
/// backend.
///
/// **This trait is defined by the policy layer (trust crate) and
/// implemented by the adapter layer (storage crate).** The storage
/// crate adds `repo-graph-trust` as a dependency to import this
/// trait and implements it on `StorageConnection`.
///
/// All methods are read-only. No writes, no transactions, no
/// schema mutations.
///
/// **Error handling:** each method returns `Result<T, Self::Error>`
/// so that real storage errors (locked DB, malformed schema, SQL
/// bugs) propagate to the trust service instead of being silently
/// coerced to zero/empty. The TS `StoragePort` methods throw on
/// SQL errors; the Rust trait matches that by making failures
/// explicit in the return type. The associated `Error` type is
/// provided by the implementor (e.g., `StorageError` in the
/// storage crate).
pub trait TrustStorageRead {
	/// The error type for storage operations. Provided by the
	/// implementor. Must be `Debug + Display` so callers can
	/// format diagnostic messages without knowing the concrete
	/// type.
	type Error: std::fmt::Debug + std::fmt::Display;

	/// Read the extraction diagnostics JSON payload for a snapshot.
	/// Returns `Ok(None)` for snapshots indexed before migration 005
	/// or for snapshots that don't exist. Returns `Err` on actual
	/// SQL errors.
	fn get_snapshot_extraction_diagnostics(
		&self,
		snapshot_uid: &str,
	) -> Result<Option<String>, Self::Error>;

	/// Count resolved edges of a specific type in a snapshot.
	fn count_edges_by_type(
		&self,
		snapshot_uid: &str,
		edge_type: &str,
	) -> Result<u64, Self::Error>;

	/// Count active declarations of a specific kind for a repo.
	fn count_active_declarations(
		&self,
		repo_uid: &str,
		kind: &str,
	) -> Result<usize, Self::Error>;

	/// Count unresolved edges grouped by the classification axis.
	fn count_unresolved_edges_by_classification(
		&self,
		input: &CountByClassificationInput,
	) -> Result<Vec<ClassificationCountRow>, Self::Error>;

	/// Query unresolved edge samples filtered by classification.
	fn query_unresolved_edges(
		&self,
		input: &QueryUnresolvedEdgesInput,
	) -> Result<Vec<TrustUnresolvedEdgeSample>, Self::Error>;

	/// Find path-prefix module cycles for a snapshot.
	fn find_path_prefix_module_cycles(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<PathPrefixModuleCycle>, Self::Error>;

	/// Compute per-module structural metrics for a snapshot.
	fn compute_module_stats(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<TrustModuleStats>, Self::Error>;

	/// Get file paths for a repo (excluding is_excluded files).
	fn get_file_paths_by_repo(
		&self,
		repo_uid: &str,
	) -> Result<Vec<String>, Self::Error>;
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn trust_module_stats_serializes_camel_case() {
		let ms = TrustModuleStats {
			stable_key: "r1:src/core:MODULE".into(),
			path: "src/core".into(),
			fan_in: 5,
			fan_out: 3,
			file_count: 12,
		};
		let s = serde_json::to_string(&ms).unwrap();
		assert!(s.contains("\"stableKey\":"));
		assert!(s.contains("\"fanIn\":5"));
		assert!(s.contains("\"fanOut\":3"));
		assert!(s.contains("\"fileCount\":12"));
		assert!(!s.contains("\"stable_key\""));
		assert!(!s.contains("\"fan_in\""));
	}

	#[test]
	fn path_prefix_module_cycle_serializes_camel_case() {
		let c = PathPrefixModuleCycle {
			ancestor_stable_key: "r1:src:MODULE".into(),
			descendant_stable_key: "r1:src/api:MODULE".into(),
		};
		let s = serde_json::to_string(&c).unwrap();
		assert!(s.contains("\"ancestorStableKey\":"));
		assert!(s.contains("\"descendantStableKey\":"));
		assert!(!s.contains("\"ancestor_stable_key\""));
	}

	#[test]
	fn trust_unresolved_edge_sample_uses_typed_enums() {
		let sample = TrustUnresolvedEdgeSample {
			category: UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
			basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
			source_node_visibility: Some("export".into()),
			metadata_json: None,
		};
		let s = serde_json::to_string(&sample).unwrap();
		// Typed enums serialize as their snake_case string values.
		assert!(s.contains("\"category\":\"calls_obj_method_needs_type_info\""));
		assert!(s.contains("\"basisCode\":\"no_supporting_signal\""));
		assert!(s.contains("\"sourceNodeVisibility\":\"export\""));
	}

	#[test]
	fn trust_unresolved_edge_sample_roundtrips_from_json() {
		let json = r#"{
			"category": "calls_function_ambiguous_or_missing",
			"basisCode": "callee_matches_same_file_symbol",
			"sourceNodeVisibility": null,
			"metadataJson": null
		}"#;
		let parsed: TrustUnresolvedEdgeSample = serde_json::from_str(json).unwrap();
		assert_eq!(
			parsed.category,
			UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
		);
		assert_eq!(
			parsed.basis_code,
			UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol
		);
	}
}
