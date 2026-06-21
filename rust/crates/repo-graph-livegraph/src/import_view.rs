//! IMPORTS-LIVEGRAPH-CLI-1: the LiveGraph IMPORT READ-MODEL DTOs (D2 -- "edges + observations, separated").
//!
//! The boundary data structures the `imports --engine livegraph` surface projects: captured FILE -> FILE
//! import EDGES (graph facts, Layer 0-1) and classified non-edge OBSERVATIONS (completeness evidence). Plain
//! DTOs -- the daemon (`livegraph_feed`) maps them to JSON + the trust envelope; this crate stays
//! serialization-free (the existing answer-type pattern). The projection method is
//! [`crate::LiveGraph::live_import_view`] (it needs the LiveGraph's private partition / overlay state).
//!
//! D5 trust invariant (enforced by construction): a benign external / asset is an OBSERVATION, NEVER an edge;
//! `workspace-local-unedgeable` is a BLOCKING observation. An edge is ONLY ever a captured
//! `ImportResolution::StaticResolved` intra-partition import or a cross-partition overlay-resolved import.

use repo_graph_ir::EdgeBasis;

/// A captured FILE -> FILE import edge (a graph fact). Produced from a resident partition's intra-partition
/// `AstImport` edge OR a cross-partition overlay edge. `src_file` / `dst_file` are repo-relative paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEdgeView {
    /// Repo-relative path of the IMPORTING file.
    pub src_file: String,
    /// Repo-relative path of the resolved TARGET file.
    pub dst_file: String,
    /// The edge's derivation basis string (which resolution path produced it). See [`import_edge_basis_label`].
    pub basis: String,
    /// The raw module specifier as written (`Some` for overlay edges and intra edges that carry import meta).
    pub raw_specifier: Option<String>,
}

/// A classified NON-EDGE import observation (completeness evidence, NOT a graph edge). The `class` is an
/// [`crate::module_cycle_cert::ObservationClass`] string and is NEVER `ResolvedEdge` (those are edges).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportObservationView {
    /// Repo-relative path of the importing file.
    pub source_file: String,
    /// The raw module specifier as written.
    pub raw_specifier: String,
    /// The observation class string (e.g. `ExternalNonLocal`, `WorkspaceLocalUnedgeable`,
    /// `UnresolvedAfterOverlay`).
    pub class: String,
    /// Whether this class BLOCKS the module-cycle completeness certificate (the five blocking classes).
    pub blocking: bool,
}

/// The LiveGraph import read-model (D2): captured EDGES (facts) + classified non-edge OBSERVATIONS (evidence),
/// separated. Built by [`crate::LiveGraph::live_import_view`]. The trust envelope (module-cycle completeness /
/// freshness / missing partitions) is added at the daemon boundary, not here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveImportView {
    /// Captured FILE -> FILE import edges (intra-partition + cross-partition overlay), sorted.
    pub edges: Vec<ImportEdgeView>,
    /// Classified non-edge observations (the completeness evidence), sorted.
    pub observations: Vec<ImportObservationView>,
}

/// IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1 (D3 precondition): the residency status of the partition that OWNS a
/// given file in the LiveGraph -- the language/residency GATE for whether the default could serve LiveGraph for
/// that file. Produced by [`crate::LiveGraph::file_partition_status`]; `None` there means the file is NOT in any
/// resident TS partition (non-resident / unknown) -> the precondition is UNMET -> SQLite fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePartitionStatus {
    /// The partition id owning the file.
    pub partition_id: String,
    /// The partition is RESIDENT (its IR is loaded). Always true when returned (the search is over resident IRs).
    pub resident: bool,
    /// The partition is Fresh (not stale / refresh-failed).
    pub fresh: bool,
    /// The partition is TypeScript-primary (the only language the LiveGraph import view covers).
    pub ts_primary: bool,
}

impl FilePartitionStatus {
    /// The D3 precondition: resident AND Fresh AND TS-primary. Only then could the default serve LiveGraph for
    /// this file (else SQLite is the sole / authoritative source).
    pub fn precondition_met(&self) -> bool {
        self.resident && self.fresh && self.ts_primary
    }
}

/// Stable string for an IMPORT edge's [`EdgeBasis`] (the read-model's JSON contract). Import edges only ever
/// carry the four import bases (intra `AstImport`; overlay relative / tsconfig-alias / dynamic); any other
/// basis (a call / reference) never appears on an import edge and maps to `"Other"`.
pub fn import_edge_basis_label(basis: EdgeBasis) -> &'static str {
    match basis {
        EdgeBasis::AstImport => "AstImport",
        EdgeBasis::AstImportFileInventoryResolved => "AstImportFileInventoryResolved",
        EdgeBasis::AstImportTsconfigPathResolved => "AstImportTsconfigPathResolved",
        EdgeBasis::AstDynamicImportResolved => "AstDynamicImportResolved",
        _ => "Other",
    }
}
