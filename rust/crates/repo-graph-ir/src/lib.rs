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
/// INGEST-CORE-1 carries only `Calls` and `References`. `Imports` is intentionally
/// absent: `scip-typescript` does not reliably emit import roles (spike M2), so
/// import edges are deferred to AST-derived classification in a later slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    /// A syntax-confirmed call.
    Calls,
    /// A resolved reference that is not a confirmed call.
    References,
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
}

impl PartitionIr {
    /// Create an empty IR for a partition.
    pub fn new(partition: Partition) -> Self {
        Self {
            partition,
            nodes: Vec::new(),
            edges: Vec::new(),
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
        });
        let callers = ir.incoming(&callee);
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].src, caller);
    }
}
