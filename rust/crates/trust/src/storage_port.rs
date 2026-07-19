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

/// The persisted snapshot-level resolved-call aggregate (EC-1 M-3b, g1).
///
/// Written by the pipeline at index/refresh finalization (supplied from
/// the resolver's full output stream) and adjusted atomically by
/// enrichment promotion; the trust service serves `resolved_calls` from
/// it instead of an eager read-time `COUNT` over CALLS rows.
///
/// An instance of this DTO is a VALIDATED claim: the adapter only
/// constructs it from a well-formed persisted state (non-negative count,
/// non-empty provenance label). Invalid persisted states — a negative
/// count, a count with no label — are NOT representable here; the adapter
/// degrades them to "no aggregate" so the labeled live-COUNT fallback
/// serves instead (a corrupt column must never surface as a measured
/// value; unknown is never zero).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCallAggregate {
    /// The snapshot's resolved CALLS-edge count as persisted by the
    /// pipeline (full resolution stream, all languages).
    pub count: u64,
    /// Explicit provenance label per the ratified interim rule (EC-1 §8
    /// clause (c)): `"pipeline"` today. Always present — an unlabeled
    /// count does not reach consumers. Future accountings (recon-design-1)
    /// will carry their own label; consumers match on the value, they do
    /// not assume `"pipeline"` is the only one.
    pub provenance: String,
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

/// One row from a basis-code-grouped unresolved-edge count.
///
/// ATTRIBUTION-1: the FINER axis behind [`ClassificationCountRow`]. The coarse
/// 4-value `UnresolvedEdgeClassification` folds third-party dependencies, the
/// standard library, and runtime globals all into one `ExternalLibraryCandidate`
/// bucket; the 17 `UnresolvedEdgeBasisCode` values keep them apart, which is exactly
/// what a reader-frame attribution breakdown needs. Typed key (the trust crate
/// already depends on `repo-graph-classification`), read from a GROUP BY over the
/// EXISTING `unresolved_edges.basis_code` column — a read, not a schema change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasisCodeCountRow {
    pub basis_code: UnresolvedEdgeBasisCode,
    pub count: u64,
}

/// One named-dependency row in [`ExternalDependencyAttribution::top`] (ATTRIBUTION-1).
///
/// `name` is the DECLARED manifest dependency a reference resolved to (`serde`,
/// `repo-graph-indexer`, `express`, `react`), reduced from whichever external-import basis
/// named it — the import specifier, or the import binding that introduced a receiver/callee
/// call. The provenance join reduces a scoped specifier (`repo_graph_indexer::types`) to the
/// manifest name and never emits a raw import path or call expression, so `name` never
/// misnames a dependency. The version is never included (the extractor does not record it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedDependencyCount {
    pub name: String,
    pub count: u64,
}

/// The reader-frame attribution of the EXTERNAL-import unresolved references (ATTRIBUTION-1
/// iteration 3 — the provenance join).
///
/// The storage join resolves each external-import unresolved reference to the DECLARED
/// dependency it maps to, across ALL three bases the classifier resolves through imports
/// (`SpecifierMatchesPackageDependency`, `ReceiverMatchesExternalImport`,
/// `CalleeMatchesExternalImport`), reusing the classifier's own reduction so a scoped
/// specifier (`repo_graph_indexer::types`) becomes the manifest name (`repo-graph-indexer`),
/// never the raw import path (the review-2 defect).
///
/// The three fields RECONCILE the "library call" class: `total_named + unidentified` equals
/// the ExternalDependency class total (every external-import reference is counted exactly
/// once — named if it resolves to a declared dependency, unidentified otherwise).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExternalDependencyAttribution {
    /// The top declared dependencies among the external-import references, count-descending
    /// then name-ascending, bounded by the read's `limit`.
    pub top: Vec<NamedDependencyCount>,
    /// ALL external-import references that resolved to a declared dependency name (the full
    /// count, not just the bounded `top`) — lets the renderer show "other declared
    /// dependencies: N" for the identified-but-unlisted remainder honestly.
    pub total_named: u64,
    /// External-import references with NO nameable declared dependency (no source file, no
    /// manifest signal, or no matching declared dependency) — the honest "dependency not
    /// identified" bucket. Never a fabricated name.
    pub unidentified: u64,
}

/// RECON-M-R4 (§5.5): one PER-SITE unresolved CALL — a raw row read from `unresolved_edges`
/// (the ratified RED floor, read-only), joined to `nodes` for the caller's stable key. The
/// Layer-2 landing joins each such site against the witness ledger's `semantic` (SCIP-only)
/// call targets by `(caller_key, target expression HEAD)`. Raw boundary DTO: `target_key` is
/// verbatim from the extractor (a bare callee `cn` or a dotted `receiver.method`); head
/// extraction + the name-guard join are the reader-side surface's job, never storage's.
///
/// Abstraction ledger (review-1 #4): this DTO + [`TrustStorageRead::unresolved_call_sites`] are
/// the ONE new boundary surface M-R4 adds (an ADDED method on the EXISTING dependency-inverted
/// read port, not a new boundary). Concrete current users (2): the trust envelope assembly
/// (`trust_coherence::build_trust_envelope`, `caller_filter = None` — whole repo) and the explain
/// SYMBOL-focus dispatch (`caller_filter = Some(focus)` — bounds the read). Axis of variation: the
/// `caller_filter` (whole-repo vs one-caller). Simpler alternatives rejected: reusing
/// [`TrustUnresolvedEdgeSample`] / [`TrustStorageRead::query_unresolved_edges`] — that DTO carries
/// classification/basis fields but NOT the caller stable key or the raw target expression the
/// name-guard join needs, and widening it would burden its existing consumers with fields they do
/// not read (the narrow-surface rule); a bare `(String, String, Option<i64>, Option<i64>)` tuple
/// crossing the port — rejected, an unlabeled shape at a boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedCallSite {
    /// The CALLER's canonical stable key (`nodes.stable_key` for `source_node_uid`).
    pub caller_key: String,
    /// The raw callee expression the extractor recorded (no call parens; a bare identifier or a
    /// dotted `receiver.method` whose LAST segment is the called name).
    pub target_key: String,
    /// The call site's 1-based line, when the extractor recorded it (`unresolved_edges` DOES
    /// persist the occurrence site — unlike the served resolved edges). `None` = not recorded.
    pub line_start: Option<i64>,
    /// The call site's column, when recorded.
    pub col_start: Option<i64>,
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
    ///
    /// Since EC-1 M-3b this is NOT on the default `resolved_calls` serving
    /// path: the service reads the persisted aggregate
    /// ([`Self::get_resolved_call_aggregate`]) and uses this live COUNT only
    /// as the labeled fallback for pre-migration snapshots (no aggregate
    /// persisted, CALLS rows still present).
    fn count_edges_by_type(&self, snapshot_uid: &str, edge_type: &str) -> Result<u64, Self::Error>;

    /// Read the persisted snapshot-level resolved-call aggregate (EC-1 M-3b).
    ///
    /// Returns `Ok(None)` for snapshots without a persisted aggregate — a
    /// pre-migration snapshot or a missing snapshot — never a fabricated
    /// zero. The caller must fall back to the live CALLS-row count in that
    /// case (unknown is never zero).
    fn get_resolved_call_aggregate(
        &self,
        snapshot_uid: &str,
    ) -> Result<Option<ResolvedCallAggregate>, Self::Error>;

    /// Count active declarations of a specific kind for a repo.
    fn count_active_declarations(&self, repo_uid: &str, kind: &str) -> Result<usize, Self::Error>;

    /// Count unresolved edges grouped by the classification axis.
    fn count_unresolved_edges_by_classification(
        &self,
        input: &CountByClassificationInput,
    ) -> Result<Vec<ClassificationCountRow>, Self::Error>;

    /// Count unresolved edges grouped by the basis-code axis (ATTRIBUTION-1).
    ///
    /// The finer companion to [`Self::count_unresolved_edges_by_classification`]: a
    /// read-only GROUP BY over the existing `basis_code` column, unfiltered (the full
    /// unresolved set), so the reader surface can NAME where unresolved references go
    /// (declared dependency / standard library / runtime global / own code / dynamic
    /// dispatch / unattributed) instead of the coarse 4-value classification.
    fn count_unresolved_edges_by_basis_code(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<BasisCodeCountRow>, Self::Error>;

    /// Attribute the EXTERNAL-import unresolved references to their DECLARED dependencies
    /// (ATTRIBUTION-1 iteration 3 — the provenance join replacing the review-1 GROUP BY).
    ///
    /// A read-only join of the external-import unresolved edges (three bases:
    /// `SpecifierMatchesPackageDependency`, `Receiver`/`CalleeMatchesExternalImport`) with
    /// each source file's persisted signals (`file_signals.import_bindings_json` +
    /// `package_dependencies_json`), resolving each reference to its DECLARED dependency via
    /// the classifier's own reduction (`repo_graph_classification::
    /// resolve_external_dependency_name`) — so a scoped specifier renders as the manifest
    /// name (`repo-graph-indexer`), and receiver/callee calls are named via their import
    /// binding, not degraded. Returns the bounded `top` (count-desc, name-asc) plus the
    /// `total_named` / `unidentified` totals that reconcile the class. No schema change.
    fn attribute_external_dependencies(
        &self,
        snapshot_uid: &str,
        limit: u32,
    ) -> Result<ExternalDependencyAttribution, Self::Error>;

    /// RECON-M-R4 (§5.5): the per-site UNRESOLVED CALL rows for the Layer-2 landing — a
    /// read-only `unresolved_edges` scan (type `CALLS`) joined to `nodes` for the caller's
    /// stable key. `caller_filter`, when `Some(stable_key)`, restricts to one caller (explain
    /// SYMBOL focus — bounds the read); `None` returns every unresolved call site (trust,
    /// whole repo). Read-only over the ratified floor: touches no counter, no write path.
    fn unresolved_call_sites(
        &self,
        snapshot_uid: &str,
        caller_filter: Option<&str>,
    ) -> Result<Vec<UnresolvedCallSite>, Self::Error>;

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
    fn get_file_paths_by_repo(&self, repo_uid: &str) -> Result<Vec<String>, Self::Error>;
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
