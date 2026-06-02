//! # repo-graph-warm-cache (WARM-CACHE-1, Stage D)
//!
//! Pure support crate that **serializes + validates** the warm-cache artifacts defined by
//! `PARTITIONED-WARM-CACHE-ARCH-1`: a `PartitionIr` (the graph) plus an optional `ValueFacts`
//! **sidecar**, each wrapped in a validated manifest and written atomically.
//!
//! ## What this crate is NOT
//! - NOT wired into the daemon (a later slice does that).
//! - NO CLI, NO LiveGraph dependency, NO scip-ingest dependency, NO warm-start runtime behavior.
//! - It does NOT recompute value facts, run a producer, or decide when to load a cache. It only
//!   encodes/decodes/validates bytes.
//!
//! ## Authority (architecture invariant)
//! The warm cache is a NON-authoritative acceleration layer. It is always safe to delete and is
//! validated (manifest key + schema + checksum) before any decode. An accepted entry is complete and
//! matches its key; on ANY mismatch the entry is rejected (the caller treats the partition as needing
//! a re-index).
//!
//! ## Serialization boundary (D8)
//! `repo-graph-ir` is dependency-free by invariant, so `PartitionIr` is not serde-serializable. This
//! crate owns the serde-deriving **mirror DTOs** + `From`/`TryFrom` conversions. **No serde is added
//! to `repo-graph-ir`.**
//!
//! ## Value-fact independence (D7)
//! `ValueFact` lives in `repo-graph-livegraph`, which this crate MUST NOT depend on. The value-fact
//! DTO ([`CacheValueFactDto`]) is therefore defined **independently** (it also mirrors
//! `repo-graph-trust-model::IdentityBasis` without depending on it). The later wiring layer converts
//! LiveGraph `ValueFact` <-> [`CacheValueFactDto`]. **Partition-cache validity and ValueFacts-sidecar
//! validity are INDEPENDENT** — a sidecar failure never invalidates the partition cache.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use repo_graph_ir::{
    CanonicalKey, EdgeBasis, EdgeType, IdentitySource, ImportEdgeMeta, ImportObservation,
    ImportResolution, IrEdge, IrNode, Partition, PartitionId, PartitionIr, PartitionKind,
    Provenance, SourceRange,
};

/// Magic marker for a repo-graph warm-cache file ("RGWC").
pub const MAGIC: u32 = 0x5247_5743;
/// Cache wire-format / layout version. A change here invalidates every existing entry (D3/D4).
/// v2 (IMPORTS-MODULE-INGEST-1): `CacheIrEdgeDto` gained the optional `import` field (import edges).
/// v3 (IMPORTS-EXTRACT-COMPLETENESS-1): `CachePartitionIrDto` gained `import_observations`.
/// v4 (KEY-NAMESPACE-REPO-RELATIVE-1): node/edge KEYS are now repo-relative; the DTO shape is unchanged
/// but cached key VALUES differ, so old (partition-relative) caches MUST be discarded -> re-extract.
pub const SCHEMA_VERSION: u32 = 4;

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Errors (D4)
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// Every way a cache read/validate/decode can fail. `CacheValidationError` is an alias for the same
/// type — the WARM-CACHE-1 contract names both; one enum covers I/O, manifest validation, and decode.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Filesystem I/O failure (read/write/rename/fsync).
    #[error("cache I/O error: {0}")]
    Io(String),
    /// The opaque payload bytes did not deserialize into the expected DTO.
    #[error("cache payload decode error: {0}")]
    Decode(String),
    /// DTO -> domain conversion rejected the entry (reserved; conversions are total today).
    #[error("cache DTO conversion error: {0}")]
    Convert(String),
    /// File magic did not match [`MAGIC`].
    #[error("cache magic mismatch: found {found:#010x}, expected {expected:#010x}")]
    MagicMismatch {
        /// Expected magic ([`MAGIC`]).
        expected: u32,
        /// Magic found in the file.
        found: u32,
    },
    /// File schema version did not match [`SCHEMA_VERSION`].
    #[error("cache schema_version mismatch: found {found}, expected {expected}")]
    SchemaMismatch {
        /// Expected schema version ([`SCHEMA_VERSION`]).
        expected: u32,
        /// Schema version found in the file.
        found: u32,
    },
    /// The manifest key did not match the expected key (wrong repo/partition/hash/producer).
    #[error("cache key mismatch (entry does not match the expected key)")]
    KeyMismatch,
    /// The payload checksum did not match the manifest (corrupt or tampered content).
    #[error("cache content checksum mismatch (corrupt or tampered payload)")]
    ChecksumMismatch,
    /// The file/payload was truncated or otherwise malformed (outer decode failed, or the stated
    /// content length disagreed with the payload).
    #[error("cache payload truncated or malformed")]
    Truncated,
}

/// Alias: the WARM-CACHE-1 contract names both `CacheValidationError` and `CacheError`. Validation and
/// decode share one error type.
pub type CacheValidationError = CacheError;

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Cache key, manifest, file envelope (D3 / D4 / D5)
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// A producer's LOGICAL identity (WARM-CACHE-PRODUCER-ABSENT-1 D3): the stable name + version that
/// governs cache coherence, INDEPENDENT of the binary's install path/mtime (which over-invalidate
/// across reinstalls). A producer-absent load validates this against the manifest's stored value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerFingerprint {
    /// Producer name (e.g. `scip-typescript`).
    pub name: String,
    /// Producer logical version (e.g. `0.4.0`).
    pub version: String,
}

/// Identity of a cache entry. A change in ANY field invalidates the entry ([`CacheError::KeyMismatch`]).
///
/// The two hash axes are SEPARATED (PRODUCER-ABSENT-1 D2): `source_inputs_hash` covers ONLY the
/// source + config inputs (recomputable WITHOUT the producer binary), and `producer_fingerprint` is
/// the producer's logical identity. This lets a producer-absent load validate `source_inputs_hash`
/// while taking `producer_fingerprint` from the manifest. `repo_graph_version` (runtime identity) is
/// here; `schema_version` is NOT (a crate-owned const → [`CacheError::SchemaMismatch`]); `created_at`
/// is manifest metadata, never identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheKey {
    /// Stable repository id.
    pub repo_uid: String,
    /// Partition id (e.g. the TS package).
    pub partition_id: String,
    /// sha256 over the SOURCE inputs ONLY (sources + tsconfig/package.json/lockfile/build config) —
    /// NOT the producer. Recomputable without the producer binary (the producer-absent gate).
    pub source_inputs_hash: String,
    /// The producer's logical identity (name + version). Replaces the former `indexer_name` +
    /// `indexer_version` (same fact — not stored twice).
    pub producer_fingerprint: ProducerFingerprint,
    /// The runtime's version string. A change invalidates the entry ([`CacheError::KeyMismatch`]).
    pub repo_graph_version: String,
}

/// The validation header written at the head of every cache file (D4). `magic`, `schema_version`,
/// `content_length`, and `checksum` are filled by the `encode_*` functions; the caller supplies `key`
/// and `created_at`. `repo_graph_version` is NOT a standalone manifest field — it is part of `key`
/// (entry identity); the manifest owns only `schema_version` (cache-format identity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheManifest {
    /// File magic ([`MAGIC`]).
    pub magic: u32,
    /// Cache format version ([`SCHEMA_VERSION`]). Self-validated by this crate; mismatch ->
    /// [`CacheError::SchemaMismatch`]. Distinct axis from `key.repo_graph_version`.
    pub schema_version: u32,
    /// Entry identity (D3), including `repo_graph_version`.
    pub key: CacheKey,
    /// Caller-supplied creation timestamp (unix seconds). Metadata only — NOT part of identity. This
    /// crate does not read the clock.
    pub created_at: u64,
    /// Length of the opaque payload in bytes.
    pub content_length: u64,
    /// Hex sha256 of the opaque payload bytes.
    pub checksum: String,
}

/// On-disk envelope: a validated manifest plus the opaque bincode payload bytes.
///
/// **Recorded divergence from the WARM-CACHE-1 contract.** The contract lists `CacheFileEnvelope<T>`.
/// This realizes it as a NON-generic envelope whose `payload` is the raw bincode of the DTO `T`, and
/// carries `T` at the `encode_*`/`decode_*` function layer instead of as a struct generic. Rationale:
/// the D4 integrity checksum must cover EXACTLY the bytes on disk. Storing the payload as bytes makes
/// the checksum byte-exact and independent of bincode re-serialization determinism; a phantom generic
/// would add no integrity value. The typed contract is preserved by `encode_partition`/
/// `decode_partition` (`T = CachePartitionIrDto`) and `encode_value_facts`/`decode_value_facts`
/// (`T = CacheValueFactsDto`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheFileEnvelope {
    /// Validation header.
    pub manifest: CacheManifest,
    /// Opaque bincode of the payload DTO; the manifest checksum covers exactly these bytes.
    pub payload: Vec<u8>,
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// IR mirror DTOs (D8) + conversions
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// Mirror of `repo_graph_ir::SourceRange`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheSourceRangeDto {
    /// File path.
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

/// Mirror of `repo_graph_ir::Provenance`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheProvenanceDto {
    /// Producer name.
    pub indexer: String,
    /// Producer version.
    pub indexer_version: String,
    /// Originating SCIP symbol id, if any.
    pub scip_symbol_id: Option<String>,
    /// Build-inputs hash at production time.
    pub build_inputs_hash: String,
}

/// Mirror of `repo_graph_ir::IdentitySource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheIdentitySourceDto {
    /// `AstAdopted`.
    AstAdopted,
    /// `ScipSynthesizedFallback`.
    ScipSynthesizedFallback,
    /// `AstFileScope`.
    AstFileScope,
}

/// Mirror of `repo_graph_ir::EdgeType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheEdgeTypeDto {
    /// `Calls`.
    Calls,
    /// `References`.
    References,
    /// `Imports` (IMPORTS-MODULE-INGEST-1).
    Imports,
}

/// Mirror of `repo_graph_ir::EdgeBasis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheEdgeBasisDto {
    /// `SyntaxConfirmedCall`.
    SyntaxConfirmedCall,
    /// `DerivedReference`.
    DerivedReference,
    /// `FileScopeReference`.
    FileScopeReference,
    /// `AstImport` (IMPORTS-MODULE-INGEST-1).
    AstImport,
    /// `AstImportFileInventoryResolved` (IMPORTS-XPART-RESOLUTION-1). Present for DTO/enum compatibility;
    /// cross-partition resolved edges are NEVER persisted, so this never appears in a real cache payload
    /// (hence NO SCHEMA_VERSION bump).
    AstImportFileInventoryResolved,
}

/// Mirror of `repo_graph_ir::ImportResolution`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheImportResolutionDto {
    /// `StaticResolved`.
    StaticResolved,
    /// `StaticUnresolved` (IMPORTS-EXTRACT-COMPLETENESS-1).
    StaticUnresolved,
    /// `PackageExternal` (IMPORTS-EXTRACT-COMPLETENESS-1).
    PackageExternal,
    /// `DynamicUnsupported` (IMPORTS-EXTRACT-COMPLETENESS-1).
    DynamicUnsupported,
}

/// Mirror of `repo_graph_ir::ImportObservation` (IMPORTS-EXTRACT-COMPLETENESS-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheImportObservationDto {
    /// Raw module specifier.
    pub raw_specifier: String,
    /// Resolution class.
    pub resolution: CacheImportResolutionDto,
    /// Re-export modifier.
    pub is_re_export: bool,
    /// Type-only modifier.
    pub is_type_only: bool,
    /// Side-effect modifier.
    pub is_side_effect: bool,
}

/// Mirror of `repo_graph_ir::ImportEdgeMeta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheImportEdgeMetaDto {
    /// Raw module specifier.
    pub raw_specifier: String,
    /// Resolved partition-relative target path.
    pub resolved_path: String,
    /// Resolution class.
    pub resolution: CacheImportResolutionDto,
}

/// Mirror of `repo_graph_ir::PartitionKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CachePartitionKindDto {
    /// `TsPackage`.
    TsPackage,
}

/// Mirror of `repo_graph_ir::IrNode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheIrNodeDto {
    /// Canonical key string.
    pub key: String,
    /// Node subtype.
    pub subtype: String,
    /// Node name.
    pub name: String,
    /// Optional source range.
    pub range: Option<CacheSourceRangeDto>,
    /// Owning partition id string.
    pub partition_id: String,
    /// Identity source.
    pub identity_source: CacheIdentitySourceDto,
    /// Provenance.
    pub provenance: CacheProvenanceDto,
}

/// Mirror of `repo_graph_ir::IrEdge`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheIrEdgeDto {
    /// Source canonical key string.
    pub src: String,
    /// Target canonical key string.
    pub dst: String,
    /// Edge type.
    pub edge_type: CacheEdgeTypeDto,
    /// Edge basis.
    pub basis: CacheEdgeBasisDto,
    /// Provenance.
    pub provenance: CacheProvenanceDto,
    /// Import metadata — `Some` iff this is an import edge (IMPORTS-MODULE-INGEST-1).
    pub import: Option<CacheImportEdgeMetaDto>,
}

/// Mirror of `repo_graph_ir::Partition`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachePartitionDto {
    /// Partition id string.
    pub id: String,
    /// Partition kind.
    pub kind: CachePartitionKindDto,
    /// Root path.
    pub root: String,
    /// Producer name.
    pub indexer: String,
    /// Producer version.
    pub indexer_version: String,
    /// Build-inputs hash.
    pub build_inputs_hash: String,
}

/// Mirror of `repo_graph_ir::PartitionIr`. `impl From<&PartitionIr>` + `impl TryFrom<_> for
/// PartitionIr` are the D8 semantic round-trip conversions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachePartitionIrDto {
    /// Partition metadata.
    pub partition: CachePartitionDto,
    /// Nodes.
    pub nodes: Vec<CacheIrNodeDto>,
    /// Edges.
    pub edges: Vec<CacheIrEdgeDto>,
    /// Import observations (IMPORTS-EXTRACT-COMPLETENESS-1; schema v3). Old (v2) caches are rejected at
    /// the manifest `schema_version` gate (SchemaMismatch -> discard), so no backward-decode default.
    pub import_observations: Vec<CacheImportObservationDto>,
}

impl From<&SourceRange> for CacheSourceRangeDto {
    fn from(r: &SourceRange) -> Self {
        Self {
            file: r.file.clone(),
            start_line: r.start_line,
            start_col: r.start_col,
            end_line: r.end_line,
            end_col: r.end_col,
        }
    }
}
impl From<CacheSourceRangeDto> for SourceRange {
    fn from(r: CacheSourceRangeDto) -> Self {
        Self {
            file: r.file,
            start_line: r.start_line,
            start_col: r.start_col,
            end_line: r.end_line,
            end_col: r.end_col,
        }
    }
}

impl From<&Provenance> for CacheProvenanceDto {
    fn from(p: &Provenance) -> Self {
        Self {
            indexer: p.indexer.clone(),
            indexer_version: p.indexer_version.clone(),
            scip_symbol_id: p.scip_symbol_id.clone(),
            build_inputs_hash: p.build_inputs_hash.clone(),
        }
    }
}
impl From<CacheProvenanceDto> for Provenance {
    fn from(p: CacheProvenanceDto) -> Self {
        Self {
            indexer: p.indexer,
            indexer_version: p.indexer_version,
            scip_symbol_id: p.scip_symbol_id,
            build_inputs_hash: p.build_inputs_hash,
        }
    }
}

impl From<&IdentitySource> for CacheIdentitySourceDto {
    fn from(v: &IdentitySource) -> Self {
        match v {
            IdentitySource::AstAdopted => Self::AstAdopted,
            IdentitySource::ScipSynthesizedFallback => Self::ScipSynthesizedFallback,
            IdentitySource::AstFileScope => Self::AstFileScope,
        }
    }
}
impl From<CacheIdentitySourceDto> for IdentitySource {
    fn from(v: CacheIdentitySourceDto) -> Self {
        match v {
            CacheIdentitySourceDto::AstAdopted => Self::AstAdopted,
            CacheIdentitySourceDto::ScipSynthesizedFallback => Self::ScipSynthesizedFallback,
            CacheIdentitySourceDto::AstFileScope => Self::AstFileScope,
        }
    }
}

impl From<&EdgeType> for CacheEdgeTypeDto {
    fn from(v: &EdgeType) -> Self {
        match v {
            EdgeType::Calls => Self::Calls,
            EdgeType::References => Self::References,
            EdgeType::Imports => Self::Imports,
        }
    }
}
impl From<CacheEdgeTypeDto> for EdgeType {
    fn from(v: CacheEdgeTypeDto) -> Self {
        match v {
            CacheEdgeTypeDto::Calls => Self::Calls,
            CacheEdgeTypeDto::References => Self::References,
            CacheEdgeTypeDto::Imports => Self::Imports,
        }
    }
}

impl From<&ImportResolution> for CacheImportResolutionDto {
    fn from(v: &ImportResolution) -> Self {
        match v {
            ImportResolution::StaticResolved => Self::StaticResolved,
            ImportResolution::StaticUnresolved => Self::StaticUnresolved,
            ImportResolution::PackageExternal => Self::PackageExternal,
            ImportResolution::DynamicUnsupported => Self::DynamicUnsupported,
        }
    }
}
impl From<CacheImportResolutionDto> for ImportResolution {
    fn from(v: CacheImportResolutionDto) -> Self {
        match v {
            CacheImportResolutionDto::StaticResolved => Self::StaticResolved,
            CacheImportResolutionDto::StaticUnresolved => Self::StaticUnresolved,
            CacheImportResolutionDto::PackageExternal => Self::PackageExternal,
            CacheImportResolutionDto::DynamicUnsupported => Self::DynamicUnsupported,
        }
    }
}

impl From<&ImportObservation> for CacheImportObservationDto {
    fn from(o: &ImportObservation) -> Self {
        Self {
            raw_specifier: o.raw_specifier.clone(),
            resolution: CacheImportResolutionDto::from(&o.resolution),
            is_re_export: o.is_re_export,
            is_type_only: o.is_type_only,
            is_side_effect: o.is_side_effect,
        }
    }
}
impl From<CacheImportObservationDto> for ImportObservation {
    fn from(o: CacheImportObservationDto) -> Self {
        Self {
            raw_specifier: o.raw_specifier,
            resolution: ImportResolution::from(o.resolution),
            is_re_export: o.is_re_export,
            is_type_only: o.is_type_only,
            is_side_effect: o.is_side_effect,
        }
    }
}

impl From<&ImportEdgeMeta> for CacheImportEdgeMetaDto {
    fn from(m: &ImportEdgeMeta) -> Self {
        Self {
            raw_specifier: m.raw_specifier.clone(),
            resolved_path: m.resolved_path.clone(),
            resolution: CacheImportResolutionDto::from(&m.resolution),
        }
    }
}
impl From<CacheImportEdgeMetaDto> for ImportEdgeMeta {
    fn from(m: CacheImportEdgeMetaDto) -> Self {
        Self {
            raw_specifier: m.raw_specifier,
            resolved_path: m.resolved_path,
            resolution: ImportResolution::from(m.resolution),
        }
    }
}

impl From<&EdgeBasis> for CacheEdgeBasisDto {
    fn from(v: &EdgeBasis) -> Self {
        match v {
            EdgeBasis::SyntaxConfirmedCall => Self::SyntaxConfirmedCall,
            EdgeBasis::DerivedReference => Self::DerivedReference,
            EdgeBasis::FileScopeReference => Self::FileScopeReference,
            EdgeBasis::AstImport => Self::AstImport,
            EdgeBasis::AstImportFileInventoryResolved => Self::AstImportFileInventoryResolved,
        }
    }
}
impl From<CacheEdgeBasisDto> for EdgeBasis {
    fn from(v: CacheEdgeBasisDto) -> Self {
        match v {
            CacheEdgeBasisDto::SyntaxConfirmedCall => Self::SyntaxConfirmedCall,
            CacheEdgeBasisDto::DerivedReference => Self::DerivedReference,
            CacheEdgeBasisDto::FileScopeReference => Self::FileScopeReference,
            CacheEdgeBasisDto::AstImport => Self::AstImport,
            CacheEdgeBasisDto::AstImportFileInventoryResolved => {
                Self::AstImportFileInventoryResolved
            }
        }
    }
}

impl From<&PartitionKind> for CachePartitionKindDto {
    fn from(v: &PartitionKind) -> Self {
        match v {
            PartitionKind::TsPackage => Self::TsPackage,
        }
    }
}
impl From<CachePartitionKindDto> for PartitionKind {
    fn from(v: CachePartitionKindDto) -> Self {
        match v {
            CachePartitionKindDto::TsPackage => Self::TsPackage,
        }
    }
}

impl From<&IrNode> for CacheIrNodeDto {
    fn from(n: &IrNode) -> Self {
        Self {
            key: n.key.as_str().to_string(),
            subtype: n.subtype.clone(),
            name: n.name.clone(),
            range: n.range.as_ref().map(CacheSourceRangeDto::from),
            partition_id: n.partition_id.as_str().to_string(),
            identity_source: CacheIdentitySourceDto::from(&n.identity_source),
            provenance: CacheProvenanceDto::from(&n.provenance),
        }
    }
}
impl From<CacheIrNodeDto> for IrNode {
    fn from(n: CacheIrNodeDto) -> Self {
        Self {
            key: CanonicalKey::from_existing(n.key.as_str()),
            subtype: n.subtype,
            name: n.name,
            range: n.range.map(SourceRange::from),
            partition_id: PartitionId::new(n.partition_id.as_str()),
            identity_source: IdentitySource::from(n.identity_source),
            provenance: Provenance::from(n.provenance),
        }
    }
}

impl From<&IrEdge> for CacheIrEdgeDto {
    fn from(e: &IrEdge) -> Self {
        Self {
            src: e.src.as_str().to_string(),
            dst: e.dst.as_str().to_string(),
            edge_type: CacheEdgeTypeDto::from(&e.edge_type),
            basis: CacheEdgeBasisDto::from(&e.basis),
            provenance: CacheProvenanceDto::from(&e.provenance),
            import: e.import.as_ref().map(CacheImportEdgeMetaDto::from),
        }
    }
}
impl From<CacheIrEdgeDto> for IrEdge {
    fn from(e: CacheIrEdgeDto) -> Self {
        Self {
            src: CanonicalKey::from_existing(e.src.as_str()),
            dst: CanonicalKey::from_existing(e.dst.as_str()),
            edge_type: EdgeType::from(e.edge_type),
            basis: EdgeBasis::from(e.basis),
            provenance: Provenance::from(e.provenance),
            import: e.import.map(ImportEdgeMeta::from),
        }
    }
}

impl From<&Partition> for CachePartitionDto {
    fn from(p: &Partition) -> Self {
        Self {
            id: p.id.as_str().to_string(),
            kind: CachePartitionKindDto::from(&p.kind),
            root: p.root.clone(),
            indexer: p.indexer.clone(),
            indexer_version: p.indexer_version.clone(),
            build_inputs_hash: p.build_inputs_hash.clone(),
        }
    }
}
impl From<CachePartitionDto> for Partition {
    fn from(p: CachePartitionDto) -> Self {
        Self {
            id: PartitionId::new(p.id.as_str()),
            kind: PartitionKind::from(p.kind),
            root: p.root,
            indexer: p.indexer,
            indexer_version: p.indexer_version,
            build_inputs_hash: p.build_inputs_hash,
        }
    }
}

impl From<&PartitionIr> for CachePartitionIrDto {
    fn from(ir: &PartitionIr) -> Self {
        Self {
            partition: CachePartitionDto::from(&ir.partition),
            nodes: ir.nodes.iter().map(CacheIrNodeDto::from).collect(),
            edges: ir.edges.iter().map(CacheIrEdgeDto::from).collect(),
            import_observations: ir
                .import_observations
                .iter()
                .map(CacheImportObservationDto::from)
                .collect(),
        }
    }
}
impl TryFrom<CachePartitionIrDto> for PartitionIr {
    type Error = CacheError;
    /// Total today (the conversion cannot fail); `TryFrom` is the contract shape and the seam for
    /// future structural validation of untrusted input (e.g. rejecting malformed keys).
    fn try_from(dto: CachePartitionIrDto) -> Result<Self, Self::Error> {
        Ok(PartitionIr {
            partition: Partition::from(dto.partition),
            nodes: dto.nodes.into_iter().map(IrNode::from).collect(),
            edges: dto.edges.into_iter().map(IrEdge::from).collect(),
            import_observations: dto
                .import_observations
                .into_iter()
                .map(ImportObservation::from)
                .collect(),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Value-fact mirror DTOs (D7 — defined INDEPENDENTLY of LiveGraph + trust-model)
// ─────────────────────────────────────────────────────────────────────────────────────────────────

/// Mirror of `repo_graph_trust_model::IdentityBasis` (defined here to avoid a trust-model dependency).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheIdentityBasisDto {
    /// `AstAdopted`.
    AstAdopted,
    /// `ScipSynthesized`.
    ScipSynthesized,
    /// `AstFileScope`.
    AstFileScope,
    /// `DeclarationMapExact`.
    DeclarationMapExact,
    /// `NameExactUnique`.
    NameExactUnique,
    /// `RangeNameConfirmed`.
    RangeNameConfirmed,
    /// `RawAnchored`.
    RawAnchored,
}

/// Mirror of `repo_graph_livegraph::ValueFactKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheValueFactKindDto {
    /// `CyclomaticComplexity`.
    CyclomaticComplexity,
}

/// Mirror of `repo_graph_livegraph::ValueSubject`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheValueSubjectDto {
    /// Attached to a canonical symbol identity (the canonical key string).
    Symbol(String),
    /// Attached only to a source range (ownership not certified).
    RawAnchor(CacheSourceRangeDto),
}

/// Mirror of `repo_graph_livegraph::ValueFact`. Defined independently of LiveGraph/trust-model; the
/// later wiring layer converts `ValueFact` <-> `CacheValueFactDto`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheValueFactDto {
    /// What the fact is attached to.
    pub subject: CacheValueSubjectDto,
    /// The fact kind.
    pub kind: CacheValueFactKindDto,
    /// The measured value (a true observation regardless of basis).
    pub value: u32,
    /// The identity basis governing ONLY the ownership claim.
    pub basis: CacheIdentityBasisDto,
    /// The source range the fact was observed at, if known.
    pub source_range: Option<CacheSourceRangeDto>,
    /// External-producer provenance.
    pub provenance: CacheProvenanceDto,
}

/// The sidecar payload: a set of value facts (D7). Serialized under its own manifest, independent of
/// the partition cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheValueFactsDto {
    /// The value facts.
    pub facts: Vec<CacheValueFactDto>,
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Encode / decode / validate
// ─────────────────────────────────────────────────────────────────────────────────────────────────

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Validate a manifest against the expected key and the actual payload bytes (D4). Order: magic,
/// schema, key, length, checksum. The FIRST mismatch is reported.
///
/// Two distinct identity axes with distinct diagnostics: `schema_version` (cache-format identity,
/// crate-owned) mismatches as [`CacheError::SchemaMismatch`]; `repo_graph_version` (producer/runtime
/// identity, carried inside `key`) mismatches as [`CacheError::KeyMismatch`] via the key equality
/// check below.
fn validate_manifest(
    manifest: &CacheManifest,
    payload: &[u8],
    expected_key: &CacheKey,
) -> Result<(), CacheError> {
    if manifest.magic != MAGIC {
        return Err(CacheError::MagicMismatch {
            expected: MAGIC,
            found: manifest.magic,
        });
    }
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(CacheError::SchemaMismatch {
            expected: SCHEMA_VERSION,
            found: manifest.schema_version,
        });
    }
    if &manifest.key != expected_key {
        return Err(CacheError::KeyMismatch);
    }
    if manifest.content_length != payload.len() as u64 {
        return Err(CacheError::Truncated);
    }
    if manifest.checksum != sha256_hex(payload) {
        return Err(CacheError::ChecksumMismatch);
    }
    Ok(())
}

/// Wrap a serializable payload value in a validated envelope. Fills `magic`, `schema_version`,
/// `content_length`, and `checksum` on the provided manifest; the caller supplies the rest. Infallible
/// (`-> Vec<u8>`): bincode serialization of these owned DTOs cannot fail.
fn encode_typed<T: Serialize>(mut manifest: CacheManifest, payload_value: &T) -> Vec<u8> {
    let payload = bincode::serialize(payload_value)
        .expect("bincode serialization of an owned cache DTO is infallible");
    manifest.magic = MAGIC;
    manifest.schema_version = SCHEMA_VERSION;
    manifest.content_length = payload.len() as u64;
    manifest.checksum = sha256_hex(&payload);
    let envelope = CacheFileEnvelope { manifest, payload };
    bincode::serialize(&envelope).expect("bincode serialization of the envelope is infallible")
}

/// Decode + validate an envelope from file bytes into the payload DTO `T`. Used by the typed
/// `decode_*` functions; safe on untrusted bytes (validates before decoding the payload).
fn decode_typed<T: serde::de::DeserializeOwned>(
    file_bytes: &[u8],
    expected_key: &CacheKey,
) -> Result<T, CacheError> {
    let envelope: CacheFileEnvelope =
        bincode::deserialize(file_bytes).map_err(|_| CacheError::Truncated)?;
    validate_manifest(&envelope.manifest, &envelope.payload, expected_key)?;
    bincode::deserialize(&envelope.payload).map_err(|e| CacheError::Decode(e.to_string()))
}

/// Encode a `PartitionIr` into a validated cache file (manifest + payload). Infallible.
pub fn encode_partition(ir: &PartitionIr, manifest: CacheManifest) -> Vec<u8> {
    encode_typed(manifest, &CachePartitionIrDto::from(ir))
}

/// Decode + validate a partition cache file into a `PartitionIr`. Validates the manifest (magic,
/// schema, key, length, checksum) before converting.
pub fn decode_partition(bytes: &[u8], expected_key: &CacheKey) -> Result<PartitionIr, CacheError> {
    let dto: CachePartitionIrDto = decode_typed(bytes, expected_key)?;
    PartitionIr::try_from(dto)
}

/// Encode a value-facts sidecar into a validated cache file. Infallible.
pub fn encode_value_facts(facts: &[CacheValueFactDto], manifest: CacheManifest) -> Vec<u8> {
    let dto = CacheValueFactsDto {
        facts: facts.to_vec(),
    };
    encode_typed(manifest, &dto)
}

/// Decode + validate a value-facts sidecar file into its facts.
pub fn decode_value_facts(
    bytes: &[u8],
    expected_key: &CacheKey,
) -> Result<Vec<CacheValueFactDto>, CacheError> {
    let dto: CacheValueFactsDto = decode_typed(bytes, expected_key)?;
    Ok(dto.facts)
}

/// Atomically write `bytes` to `path` (D5): write a temp file in the SAME directory, fsync it, rename
/// it over the target, then best-effort fsync the parent directory. A crash leaves either the old file
/// or no file — never a partial/corrupt entry that validation would accept.
///
/// Single-writer assumption (recorded): the temp file name is `<path>.tmp` (deterministic, no clock /
/// RNG available in this crate). Concurrent writers to the SAME `path` would collide on the temp name;
/// the daemon-wiring slice owns one writer per partition, so this holds. If concurrency is later added,
/// the temp name must gain a unique suffix.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), CacheError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| CacheError::Io("target path has no parent directory".to_string()))?;

    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp_path = PathBuf::from(tmp_os);

    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .map_err(|e| CacheError::Io(format!("open temp {}: {e}", tmp_path.display())))?;
        file.write_all(bytes)
            .map_err(|e| CacheError::Io(format!("write temp: {e}")))?;
        file.sync_all()
            .map_err(|e| CacheError::Io(format!("fsync temp: {e}")))?;
    }

    std::fs::rename(&tmp_path, path)
        .map_err(|e| CacheError::Io(format!("rename temp over target: {e}")))?;

    // Best-effort durability of the rename itself. Directory fsync is not portable; ignore failure.
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Read a cache file from disk and validate its manifest envelope against `expected_key` (magic,
/// schema, key, length, checksum), returning the validated FILE bytes. This is the disk-read +
/// integrity gate; callers then decode the typed value via `decode_partition` / `decode_value_facts`
/// (which re-validate, so they are also safe on raw bytes).
pub fn read_validated(path: &Path, expected_key: &CacheKey) -> Result<Vec<u8>, CacheError> {
    let bytes =
        std::fs::read(path).map_err(|e| CacheError::Io(format!("read {}: {e}", path.display())))?;
    let envelope: CacheFileEnvelope =
        bincode::deserialize(&bytes).map_err(|_| CacheError::Truncated)?;
    validate_manifest(&envelope.manifest, &envelope.payload, expected_key)?;
    Ok(bytes)
}

/// Read the [`CacheManifest`] header from cache bytes WITHOUT accepting the entry
/// (WARM-CACHE-PRODUCER-ABSENT-1). Validates only `magic` + `schema_version` — enough to trust that
/// the manifest STRUCTURE is from a compatible format — and returns the manifest (which carries
/// `key.producer_fingerprint` + `key.source_inputs_hash`). It does NOT validate the key or the payload
/// checksum, so it is NOT acceptance: it exists solely to obtain the producer fingerprint a
/// producer-absent caller needs to construct the expected key. The caller MUST still
/// `read_validated` / `decode_partition` (full checksum + key validation) before serving the payload.
pub fn peek_manifest(bytes: &[u8]) -> Result<CacheManifest, CacheError> {
    let envelope: CacheFileEnvelope =
        bincode::deserialize(bytes).map_err(|_| CacheError::Truncated)?;
    if envelope.manifest.magic != MAGIC {
        return Err(CacheError::MagicMismatch {
            expected: MAGIC,
            found: envelope.manifest.magic,
        });
    }
    if envelope.manifest.schema_version != SCHEMA_VERSION {
        return Err(CacheError::SchemaMismatch {
            expected: SCHEMA_VERSION,
            found: envelope.manifest.schema_version,
        });
    }
    Ok(envelope.manifest)
}

// ─────────────────────────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn prov() -> Provenance {
        Provenance {
            indexer: "scip-typescript".to_string(),
            indexer_version: "0.4.0".to_string(),
            scip_symbol_id: Some("scip:typescript . . main/report().".to_string()),
            build_inputs_hash: "abc123".to_string(),
        }
    }

    fn range() -> SourceRange {
        SourceRange {
            file: "src/main.ts".to_string(),
            start_line: 1,
            start_col: 2,
            end_line: 3,
            end_col: 4,
        }
    }

    fn sample_partition_ir() -> PartitionIr {
        let partition = Partition {
            id: PartitionId::new("pkg"),
            kind: PartitionKind::TsPackage,
            root: "/repo".to_string(),
            indexer: "scip-typescript".to_string(),
            indexer_version: "0.4.0".to_string(),
            build_inputs_hash: "abc123".to_string(),
        };
        let mut ir = PartitionIr::new(partition);
        ir.nodes.push(IrNode {
            key: CanonicalKey::from_existing("repo:src/main.ts#report"),
            subtype: "function".to_string(),
            name: "report".to_string(),
            range: Some(range()),
            partition_id: PartitionId::new("pkg"),
            identity_source: IdentitySource::AstAdopted,
            provenance: prov(),
        });
        ir.nodes.push(IrNode {
            key: CanonicalKey::from_existing("repo:src/shapes.ts#Circle.describe"),
            subtype: "method".to_string(),
            name: "describe".to_string(),
            range: None,
            partition_id: PartitionId::new("pkg"),
            identity_source: IdentitySource::ScipSynthesizedFallback,
            provenance: prov(),
        });
        ir.edges.push(IrEdge {
            src: CanonicalKey::from_existing("repo:src/main.ts#report"),
            dst: CanonicalKey::from_existing("repo:src/shapes.ts#Circle.describe"),
            edge_type: EdgeType::Calls,
            basis: EdgeBasis::SyntaxConfirmedCall,
            provenance: prov(),
            import: None,
        });
        ir.edges.push(IrEdge {
            src: CanonicalKey::from_existing("repo:src/main.ts#report"),
            dst: CanonicalKey::from_existing("repo:src/main.ts#makeCircle"),
            edge_type: EdgeType::References,
            basis: EdgeBasis::DerivedReference,
            provenance: prov(),
            import: None,
        });
        // IMPORTS-MODULE-INGEST-1: an import edge so the round-trip exercises the new `import` field.
        ir.edges.push(IrEdge {
            src: CanonicalKey::from_existing("repo:src/main.ts:FILE"),
            dst: CanonicalKey::from_existing("repo:src/shapes.ts:FILE"),
            edge_type: EdgeType::Imports,
            basis: EdgeBasis::AstImport,
            provenance: prov(),
            import: Some(ImportEdgeMeta {
                raw_specifier: "./shapes".to_string(),
                resolved_path: "src/shapes.ts".to_string(),
                resolution: ImportResolution::StaticResolved,
            }),
        });
        // IMPORTS-EXTRACT-COMPLETENESS-1: observations (each non-edge class + a modifier) so the round-trip
        // exercises the v3 `import_observations` field.
        ir.import_observations.push(ImportObservation {
            raw_specifier: "react".to_string(),
            resolution: ImportResolution::PackageExternal,
            is_re_export: false,
            is_type_only: false,
            is_side_effect: false,
        });
        ir.import_observations.push(ImportObservation {
            raw_specifier: "./missing".to_string(),
            resolution: ImportResolution::StaticUnresolved,
            is_re_export: true,
            is_type_only: true,
            is_side_effect: false,
        });
        ir
    }

    fn sample_value_facts() -> Vec<CacheValueFactDto> {
        vec![
            CacheValueFactDto {
                subject: CacheValueSubjectDto::Symbol("repo:src/main.ts#report".to_string()),
                kind: CacheValueFactKindDto::CyclomaticComplexity,
                value: 7,
                basis: CacheIdentityBasisDto::RangeNameConfirmed,
                source_range: Some(CacheSourceRangeDto {
                    file: "src/main.ts".to_string(),
                    start_line: 1,
                    start_col: 0,
                    end_line: 9,
                    end_col: 1,
                }),
                provenance: CacheProvenanceDto {
                    indexer: "scip-typescript".to_string(),
                    indexer_version: "0.4.0".to_string(),
                    scip_symbol_id: None,
                    build_inputs_hash: "abc123".to_string(),
                },
            },
            CacheValueFactDto {
                subject: CacheValueSubjectDto::RawAnchor(CacheSourceRangeDto {
                    file: "src/util.ts".to_string(),
                    start_line: 4,
                    start_col: 0,
                    end_line: 4,
                    end_col: 10,
                }),
                kind: CacheValueFactKindDto::CyclomaticComplexity,
                value: 2,
                basis: CacheIdentityBasisDto::RawAnchored,
                source_range: None,
                provenance: CacheProvenanceDto {
                    indexer: "scip-typescript".to_string(),
                    indexer_version: "0.4.0".to_string(),
                    scip_symbol_id: None,
                    build_inputs_hash: "abc123".to_string(),
                },
            },
        ]
    }

    fn sample_key() -> CacheKey {
        CacheKey {
            repo_uid: "repo_01kt12m5h1jkaa9ksv80qe9fhr".to_string(),
            partition_id: "pkg".to_string(),
            source_inputs_hash: "abc123".to_string(),
            producer_fingerprint: ProducerFingerprint {
                name: "scip-typescript".to_string(),
                version: "0.4.0".to_string(),
            },
            repo_graph_version: "0.1.0".to_string(),
        }
    }

    /// A manifest with magic/schema/content_length/checksum left for `encode_*` to fill.
    fn sample_manifest(key: &CacheKey) -> CacheManifest {
        CacheManifest {
            magic: 0,
            schema_version: 0,
            key: key.clone(),
            created_at: 1_700_000_000,
            content_length: 0,
            checksum: String::new(),
        }
    }

    #[test]
    fn partition_ir_roundtrip_preserves_semantics() {
        let ir = sample_partition_ir();
        let key = sample_key();

        // Pure DTO semantic round-trip (the PARTITIONED-WARM-CACHE-ARCH-1 D8 requirement).
        let dto = CachePartitionIrDto::from(&ir);
        let back_dto = PartitionIr::try_from(dto).expect("dto -> ir");
        assert_eq!(
            ir, back_dto,
            "PartitionIr -> DTO -> PartitionIr must be equal"
        );

        // Full encode/decode round-trip through the validated envelope.
        let bytes = encode_partition(&ir, sample_manifest(&key));
        let decoded = decode_partition(&bytes, &key).expect("decode_partition");
        assert_eq!(
            ir, decoded,
            "encode -> decode must preserve the PartitionIr"
        );
    }

    #[test]
    fn value_facts_sidecar_roundtrip_preserves_semantics() {
        let facts = sample_value_facts();
        let key = sample_key();
        let bytes = encode_value_facts(&facts, sample_manifest(&key));
        let decoded = decode_value_facts(&bytes, &key).expect("decode_value_facts");
        assert_eq!(facts, decoded, "value-facts sidecar must round-trip");
    }

    #[test]
    fn manifest_key_mismatch_rejected() {
        let bytes = encode_partition(&sample_partition_ir(), sample_manifest(&sample_key()));
        let mut other = sample_key();
        other.partition_id = "a-different-partition".to_string();
        let err = decode_partition(&bytes, &other).expect_err("key mismatch must reject");
        assert!(matches!(err, CacheError::KeyMismatch), "got {err:?}");
    }

    /// `repo_graph_version` (producer/runtime identity) is part of the key: a runtime-version change
    /// with the SAME inputs invalidates the entry as `KeyMismatch` — a DISTINCT axis/diagnostic from
    /// `schema_version` (`SchemaMismatch`). Closes the WARM-CACHE-1 validation gap.
    #[test]
    fn repo_graph_version_mismatch_rejected() {
        let bytes = encode_partition(&sample_partition_ir(), sample_manifest(&sample_key()));
        let mut other = sample_key();
        other.repo_graph_version = "a-different-runtime-version".to_string();
        let err = decode_partition(&bytes, &other).expect_err("repo_graph_version must reject");
        assert!(matches!(err, CacheError::KeyMismatch), "got {err:?}");
    }

    #[test]
    fn schema_version_mismatch_rejected() {
        let key = sample_key();
        let bytes = encode_partition(&sample_partition_ir(), sample_manifest(&key));
        // Tamper the schema version in the envelope (payload + checksum untouched).
        let mut env: CacheFileEnvelope = bincode::deserialize(&bytes).unwrap();
        env.manifest.schema_version = SCHEMA_VERSION + 1;
        let tampered = bincode::serialize(&env).unwrap();
        let err = decode_partition(&tampered, &key).expect_err("schema mismatch must reject");
        assert!(
            matches!(err, CacheError::SchemaMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn checksum_mismatch_rejected() {
        let key = sample_key();
        let bytes = encode_partition(&sample_partition_ir(), sample_manifest(&key));
        // Flip a payload byte (length unchanged) -> checksum must fail.
        let mut env: CacheFileEnvelope = bincode::deserialize(&bytes).unwrap();
        assert!(!env.payload.is_empty());
        env.payload[0] ^= 0xFF;
        let tampered = bincode::serialize(&env).unwrap();
        let err = decode_partition(&tampered, &key).expect_err("checksum mismatch must reject");
        assert!(matches!(err, CacheError::ChecksumMismatch), "got {err:?}");
    }

    #[test]
    fn truncated_payload_rejected() {
        let key = sample_key();
        let bytes = encode_partition(&sample_partition_ir(), sample_manifest(&key));
        let truncated = &bytes[..bytes.len() / 2];
        let err = decode_partition(truncated, &key).expect_err("truncated must reject");
        assert!(matches!(err, CacheError::Truncated), "got {err:?}");
    }

    #[test]
    fn atomic_write_replaces_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partition.cache");

        atomic_write(&path, b"OLD-CONTENT").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"OLD-CONTENT");

        atomic_write(&path, b"NEWER-AND-LONGER-CONTENT").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"NEWER-AND-LONGER-CONTENT");

        // No leftover temp file.
        let mut tmp = path.clone().into_os_string();
        tmp.push(".tmp");
        assert!(!PathBuf::from(tmp).exists(), "temp file must not survive");
    }

    /// D7 independence: a corrupt ValueFacts sidecar does NOT invalidate the partition cache. The two
    /// artifacts are separate bytes; the partition decodes fine while the sidecar independently fails.
    #[test]
    fn invalid_value_facts_sidecar_does_not_invalidate_partition_cache() {
        let key = sample_key();
        let ir = sample_partition_ir();

        let partition_bytes = encode_partition(&ir, sample_manifest(&key));
        let sidecar_bytes = encode_value_facts(&sample_value_facts(), sample_manifest(&key));
        let corrupt_sidecar = &sidecar_bytes[..sidecar_bytes.len() / 2];

        // The partition cache remains fully valid + decodable.
        let decoded = decode_partition(&partition_bytes, &key)
            .expect("partition cache must stay valid despite a broken sidecar");
        assert_eq!(ir, decoded);

        // The sidecar is independently invalid.
        assert!(
            decode_value_facts(corrupt_sidecar, &key).is_err(),
            "the corrupt sidecar must fail on its own"
        );
    }

    /// `read_validated` is public + part of the ratified function list: read a cache file from disk,
    /// validate its manifest against the expected key, and reject a wrong key. Daemon wiring
    /// (WARM-CACHE-DAEMON-WIRING-1) depends on it, so it is covered directly.
    #[test]
    fn read_validated_reads_and_validates_written_partition_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partition.cache");
        let key = sample_key();
        let ir = sample_partition_ir();

        let bytes = encode_partition(&ir, sample_manifest(&key));
        atomic_write(&path, &bytes).unwrap();

        // Read + validate from disk: returns the exact on-disk bytes.
        let validated = read_validated(&path, &key).expect("matching key must be accepted");
        assert_eq!(
            validated, bytes,
            "read_validated must return the on-disk bytes verbatim"
        );

        // The validated bytes decode back to the original PartitionIr (semantic equality from disk).
        let decoded = decode_partition(&validated, &key).expect("decode the validated bytes");
        assert_eq!(
            ir, decoded,
            "round-trip through disk must preserve the PartitionIr"
        );

        // A wrong expected key is rejected.
        let mut wrong = key.clone();
        wrong.source_inputs_hash = "a-different-source-inputs-hash".to_string();
        let err = read_validated(&path, &wrong).expect_err("a wrong key must be rejected");
        assert!(matches!(err, CacheError::KeyMismatch), "got {err:?}");
    }

    #[test]
    fn peek_manifest_reads_key_without_accepting_corrupt_payload() {
        let key = sample_key();
        let bytes = encode_partition(&sample_partition_ir(), sample_manifest(&key));

        // Peek reads the key (incl. producer_fingerprint) from a VALID file.
        let m = peek_manifest(&bytes).expect("peek a valid file");
        assert_eq!(m.key, key);
        assert_eq!(m.key.producer_fingerprint.name, "scip-typescript");

        // Corrupt a PAYLOAD byte: peek STILL returns the manifest (it does not validate the payload),
        // but decode_partition (final acceptance) REJECTS it -> peek is read-the-key, not acceptance.
        let mut env: CacheFileEnvelope = bincode::deserialize(&bytes).unwrap();
        assert!(!env.payload.is_empty());
        env.payload[0] ^= 0xFF;
        let corrupt = bincode::serialize(&env).unwrap();
        let m2 =
            peek_manifest(&corrupt).expect("peek reads the manifest even with a corrupt payload");
        assert_eq!(m2.key, key);
        assert!(
            matches!(
                decode_partition(&corrupt, &key),
                Err(CacheError::ChecksumMismatch)
            ),
            "a corrupt payload must NOT be accepted by decode_partition"
        );
    }
}
