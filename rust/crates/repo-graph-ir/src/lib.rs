#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Canonical ingestion IR for repo-graph (INGEST-CORE-1 subset).
//!
//! This crate is the repo-graph-owned domain model the LiveGraph is built from.
//! SCIP is an upstream fact *producer*, NOT the domain model: SCIP symbol ids,
//! roles, and framing are consumed at the ingestion boundary
//! (`repo-graph-scip-ingest`) and recorded only as provenance — never as identity.
//!
//! Foundational rule, enforced structurally: this crate has **zero**
//! `scip` / `sqlite` / `tree-sitter` dependencies. If SCIP types were reachable
//! from here, SCIP would have become the domain model and the slice would have
//! failed its purpose.
//!
//! Canonical identity is **value-level reuse** of the existing `ts-extractor`
//! symbol stable-key string (`repo:file#name:SYMBOL:subtype[:dupN]`) — the exact
//! value A1 governance and measurements already target. Literal unification with
//! `state_bindings::StableKey` (opaque/resource-only) is deferred. See
//! `docs/slices/ingest-core-1.md`.

// ── Canonical identity ────────────────────────────────────────────

/// Canonical symbol identity.
///
/// Holds the existing `ts-extractor` symbol stable-key value (value-level reuse).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalKey(String);

impl CanonicalKey {
    /// Wrap an existing canonical symbol stable-key value, as emitted by
    /// `ts-extractor` (primary) or by the documented SCIP-descriptor fallback.
    pub fn from_existing(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying canonical key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the underlying canonical key string.
    pub fn into_string(self) -> String {
        self.0
    }
}

/// How a node's canonical identity was obtained.
///
/// Recorded per node so the fallback rate is a measured, surfaced number
/// (exit criteria 1-2): fallback must never silently mask a weak definition join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentitySource {
    /// Adopted from a `ts-extractor` AST definition via a `(file, range)` join.
    /// The PRIMARY path.
    AstAdopted,
    /// Synthesized from SCIP global-symbol descriptors because no AST definition
    /// matched. FALLBACK only — counted and surfaced, never silent.
    ScipSynthesizedFallback,
    /// The file/module-scope structural node (`ts-extractor` FILE node). Has NO SCIP
    /// symbol; its provenance carries no `scip_symbol_id`. Materialized so file-scope
    /// reference edges (`EdgeBasis::FileScopeReference`) have a node-backed source and
    /// the partition graph has no dangling edge endpoints. It is source-file scope, not
    /// a module-architecture / boundary / runtime entity.
    AstFileScope,
}

// ── Edges ─────────────────────────────────────────────────────────

/// Edge classification carried by the IR.
///
/// INGEST-CORE-1 carried only `Calls` and `References`; `Imports` was deferred because
/// `scip-typescript` does not reliably emit import roles (spike M2). IMPORTS-MODULE-INGEST-1 adds
/// `Imports` as an **AST-derived** edge (authority = `ts-extractor`, NOT SCIP roles): a module-import
/// edge between file-scope (FILE) identities. See `EdgeBasis::AstImport` and `ImportEdgeMeta`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    /// A syntax-confirmed call.
    Calls,
    /// A resolved reference that is not a confirmed call.
    References,
    /// A module-import edge (FILE -> FILE), AST-derived. Carries `ImportEdgeMeta`.
    Imports,
}

/// The derivation basis for an edge (D2 graded model: carried data only — there is
/// no query or trust logic in this crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeBasis {
    /// Confirmed by a call-expression range in the AST join, with a declaration-level
    /// (callable) caller. Maps to `EdgeType::Calls`. The only basis admitted into the
    /// strict call graph.
    SyntaxConfirmedCall,
    /// Resolved reference with a declaration-level caller, not call-confirmed. Maps to
    /// `EdgeType::References`.
    DerivedReference,
    /// Resolved reference whose caller is a file/module-scope source node (not a
    /// callable). Always maps to `EdgeType::References`, never `Calls`: a top-level
    /// call-expression is module-init execution, not a callable-to-callable edge, and
    /// is excluded from strict call-graph traversal by default. Carries real
    /// module-scope provenance (imports, boundary/dependency analysis).
    FileScopeReference,
    /// An AST-extracted module-import edge (IMPORTS-MODULE-INGEST-1). Maps to `EdgeType::Imports`.
    /// Authority is the `ts-extractor` AST (an `import` declaration), NOT SCIP roles or a
    /// `FileScopeReference` inference. FILE -> FILE; carries `ImportEdgeMeta`.
    AstImport,
    /// A CROSS-PARTITION module-import edge (IMPORTS-XPART-RESOLUTION-1): an AST import OBSERVATION whose
    /// target FILE was resolved against the global FILE inventory (relative + extension/index), NOT
    /// node-resolved inside the producing partition. A STRONGER inference than a raw observation but NOT
    /// identical to `AstImport`. Maps to `EdgeType::Imports`. These edges are RUNTIME/in-memory only and
    /// are NEVER persisted in a per-partition IR / warm cache (per-partition cache coherence, F1).
    AstImportFileInventoryResolved,
}

/// Resolution class of an extracted module import (IMPORTS-MODULE-INGEST-1 + IMPORTS-EXTRACT-COMPLETENESS-1).
///
/// `EdgeType::Imports` edges ONLY ever carry `StaticResolved` (a relative import node-resolved to a FILE
/// in the same partition). The other classes are produced by IMPORTS-EXTRACT-COMPLETENESS-1 for
/// [`ImportObservation`]s — completeness evidence that is NEVER a graph edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportResolution {
    /// A relative import node-resolved to a concrete FILE target in this partition (the ONLY class that
    /// becomes an `EdgeType::Imports` edge).
    StaticResolved,
    /// A relative import whose target FILE is not node-resolved in this partition (no resolvable path, or
    /// the target file is in another partition). Observation only.
    StaticUnresolved,
    /// A non-relative (package / bare) specifier (e.g. `"react"`). Observation only.
    PackageExternal,
    /// A dynamic `import()` with no static target. Observation only.
    DynamicUnsupported,
}

/// Display/dependency metadata for an `EdgeType::Imports` edge (IMPORTS-MODULE-INGEST-1).
///
/// FILE-granular only (D2): no import `kind`/`type-only` (those are binding-level facts, deferred). The
/// edge's `src`/`dst` are the file-scope FILE identities; this carries the specifier provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEdgeMeta {
    /// The raw module specifier as written (e.g. `"./foo"`).
    pub raw_specifier: String,
    /// The producer-resolved partition-relative target path (the `:FILE` target's path).
    pub resolved_path: String,
    /// Resolution class (always [`ImportResolution::StaticResolved`] for an edge).
    pub resolution: ImportResolution,
}

/// A classified module-import OBSERVATION (IMPORTS-EXTRACT-COMPLETENESS-1) — completeness evidence for an
/// import that did NOT become a graph edge (and, for honest counts, also the StaticResolved ones that
/// did). It is NEVER a graph edge: a non-node-resolved import has no FILE-node endpoint. `resolution` is
/// the mutually-exclusive class; `is_re_export`/`is_type_only`/`is_side_effect` are orthogonal modifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportObservation {
    /// The IMPORTING file's REPO-RELATIVE path (e.g. `"packages/a/src/main.ts"`) — the source endpoint a
    /// cross-partition resolver needs (IMPORTS-XPART-WIRING-1). Ingest-populated from the doc key path;
    /// repo-relative since KEY-NAMESPACE-REPO-RELATIVE-1.
    pub source_file: String,
    /// The raw module specifier as written (e.g. `"./foo"`, `"react"`).
    pub raw_specifier: String,
    /// The mutually-exclusive resolution class.
    pub resolution: ImportResolution,
    /// From an `export ... from` (a re-export), not an `import`.
    pub is_re_export: bool,
    /// `import type` / `export type` (type-only).
    pub is_type_only: bool,
    /// Bound no local identifier (e.g. `import "./x"`).
    pub is_side_effect: bool,
}

// ── Partition + provenance ────────────────────────────────────────

/// The kind of analysis partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionKind {
    /// A TypeScript workspace package.
    TsPackage,
}

/// Identifier for an analysis partition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PartitionId(String);

impl PartitionId {
    /// Construct a partition id from a string value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An analysis partition: one buildable unit indexed by one producer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    /// Partition identity.
    pub id: PartitionId,
    /// Partition kind.
    pub kind: PartitionKind,
    /// Partition root (partition-relative paths are relative to this).
    pub root: String,
    /// Producing indexer (e.g. `"scip-typescript"`).
    pub indexer: String,
    /// Producing indexer version (e.g. `"0.4.0"`).
    pub indexer_version: String,
    /// Hash of the build inputs that produced this partition's facts.
    pub build_inputs_hash: String,
    /// IMPORTS-PACKAGE-RESOLUTION-1: the package.json `name` of this partition (its WORKSPACE identity).
    /// `None` when the root has no readable package.json `name`. The union of these across loaded partitions
    /// is the workspace map that classifies a bare import as workspace-local (vs external/unresolved).
    pub package_name: Option<String>,
    /// IMPORTS-PACKAGE-RESOLUTION-1: the declared dependency NAMES (dependencies + devDependencies +
    /// peerDependencies) from this partition's package.json -- POSITIVE evidence that a bare import is an
    /// EXTERNAL package (non-cycle-relevant). NEVER inferred from absence in the workspace map.
    pub declared_dependencies: std::collections::BTreeSet<String>,
}

/// Provenance for a node or edge: the external-producer evidence (IR design R6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// Producing indexer.
    pub indexer: String,
    /// Producing indexer version.
    pub indexer_version: String,
    /// Original SCIP symbol id (substrate, non-durable identity). `None` when the
    /// fact came only from the AST.
    pub scip_symbol_id: Option<String>,
    /// Hash of the build inputs.
    pub build_inputs_hash: String,
}

// ── Source range ──────────────────────────────────────────────────

/// A source range, partition-relative. Rows are 1-based, columns 0-based,
/// matching `ts-extractor`'s convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRange {
    /// Partition-relative file path.
    pub file: String,
    /// 1-based start line.
    pub start_line: u32,
    /// 0-based start column.
    pub start_col: u32,
    /// 1-based end line.
    pub end_line: u32,
    /// 0-based end column.
    pub end_col: u32,
}

// ── Nodes + edges + container ─────────────────────────────────────

/// A node in the canonical IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrNode {
    /// Canonical identity.
    pub key: CanonicalKey,
    /// Symbol subtype as emitted by extraction (e.g. `"FUNCTION"`, `"CLASS"`).
    /// Kept as a string to avoid coupling the IR to an extractor enum.
    pub subtype: String,
    /// Symbol name.
    pub name: String,
    /// Source range, if known.
    pub range: Option<SourceRange>,
    /// Owning partition.
    pub partition_id: PartitionId,
    /// How this node's identity was obtained (primary vs fallback).
    pub identity_source: IdentitySource,
    /// Provenance.
    pub provenance: Provenance,
}

/// An edge in the canonical IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrEdge {
    /// Source (caller) canonical key.
    pub src: CanonicalKey,
    /// Target (callee / referent) canonical key.
    pub dst: CanonicalKey,
    /// Edge classification.
    pub edge_type: EdgeType,
    /// Derivation basis.
    pub basis: EdgeBasis,
    /// Provenance.
    pub provenance: Provenance,
    /// Import metadata — `Some` iff this is an `EdgeType::Imports` / `EdgeBasis::AstImport` edge,
    /// `None` for all call/reference edges (IMPORTS-MODULE-INGEST-1).
    pub import: Option<ImportEdgeMeta>,
}

/// The ingested IR for a single partition. In-memory only (D1: the warm cache is a
/// later, separate projection; this crate has no serialization).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionIr {
    /// The partition this IR belongs to.
    pub partition: Partition,
    /// Nodes.
    pub nodes: Vec<IrNode>,
    /// Edges.
    pub edges: Vec<IrEdge>,
    /// Classified module-import observations (IMPORTS-EXTRACT-COMPLETENESS-1) — completeness evidence for
    /// imports that are NOT graph edges (unresolved/package/dynamic) plus the resolved ones (for counts).
    pub import_observations: Vec<ImportObservation>,
}

impl PartitionIr {
    /// Create an empty IR for a partition.
    pub fn new(partition: Partition) -> Self {
        Self {
            partition,
            nodes: Vec::new(),
            edges: Vec::new(),
            import_observations: Vec::new(),
        }
    }

    /// Find a node by canonical key.
    pub fn node(&self, key: &CanonicalKey) -> Option<&IrNode> {
        self.nodes.iter().find(|n| &n.key == key)
    }

    /// Edges whose source is `key` (outgoing).
    pub fn outgoing(&self, key: &CanonicalKey) -> Vec<&IrEdge> {
        self.edges.iter().filter(|e| &e.src == key).collect()
    }

    /// Edges whose target is `key` (incoming) — the basis for `callers`.
    pub fn incoming(&self, key: &CanonicalKey) -> Vec<&IrEdge> {
        self.edges.iter().filter(|e| &e.dst == key).collect()
    }

    /// Number of nodes whose identity was synthesized via the SCIP-descriptor
    /// fallback (exit criteria 1-2: fallback must be surfaced, not silent).
    pub fn fallback_node_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.identity_source == IdentitySource::ScipSynthesizedFallback)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prov() -> Provenance {
        Provenance {
            indexer: "scip-typescript".into(),
            indexer_version: "0.4.0".into(),
            scip_symbol_id: None,
            build_inputs_hash: "h".into(),
        }
    }

    fn part() -> Partition {
        Partition {
            id: PartitionId::new("p"),
            kind: PartitionKind::TsPackage,
            root: "/x".into(),
            indexer: "scip-typescript".into(),
            indexer_version: "0.4.0".into(),
            build_inputs_hash: "h".into(),
            package_name: None,
            declared_dependencies: std::collections::BTreeSet::new(),
        }
    }

    #[test]
    fn canonical_key_roundtrip() {
        let k = CanonicalKey::from_existing("repo:src/a.ts#f:SYMBOL:FUNCTION");
        assert_eq!(k.as_str(), "repo:src/a.ts#f:SYMBOL:FUNCTION");
        assert_eq!(k.clone().into_string(), "repo:src/a.ts#f:SYMBOL:FUNCTION");
    }

    #[test]
    fn fallback_count_is_surfaced() {
        let mut ir = PartitionIr::new(part());
        ir.nodes.push(IrNode {
            key: CanonicalKey::from_existing("k1"),
            subtype: "FUNCTION".into(),
            name: "f".into(),
            range: None,
            partition_id: PartitionId::new("p"),
            identity_source: IdentitySource::AstAdopted,
            provenance: prov(),
        });
        ir.nodes.push(IrNode {
            key: CanonicalKey::from_existing("k2"),
            subtype: "FUNCTION".into(),
            name: "g".into(),
            range: None,
            partition_id: PartitionId::new("p"),
            identity_source: IdentitySource::ScipSynthesizedFallback,
            provenance: prov(),
        });
        assert_eq!(ir.fallback_node_count(), 1);
        assert!(ir.node(&CanonicalKey::from_existing("k1")).is_some());
    }

    #[test]
    fn incoming_is_callers_basis() {
        let mut ir = PartitionIr::new(part());
        let caller = CanonicalKey::from_existing("caller");
        let callee = CanonicalKey::from_existing("callee");
        ir.edges.push(IrEdge {
            src: caller.clone(),
            dst: callee.clone(),
            edge_type: EdgeType::Calls,
            basis: EdgeBasis::SyntaxConfirmedCall,
            provenance: prov(),
            import: None,
        });
        let callers = ir.incoming(&callee);
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].src, caller);
    }
}
