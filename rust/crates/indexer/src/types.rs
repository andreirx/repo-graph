//! Indexer domain model types.
//!
//! These types are the Rust mirror of the TS domain model at
//! `src/core/model/types.ts` plus the indexer-specific DTOs from
//! `src/core/ports/indexer.ts` and `src/core/ports/extractor.ts`.
//!
//! The indexer crate owns these types rather than importing them
//! from the storage crate, because the indexer is policy code and
//! must not depend on the storage adapter. The storage crate's
//! `GraphNode` / `GraphEdge` live in the adapter layer; these
//! extraction-output types live in the policy layer. The storage
//! port implementations map between them.
//!
//! ── Serde conventions ────────────────────────────────────────
//!
//! Enums that mirror TS SCREAMING_SNAKE_CASE string unions use
//! `#[serde(rename_all = "SCREAMING_SNAKE_CASE")]`.
//!
//! Enums that mirror TS lowercase string unions use
//! `#[serde(rename_all = "lowercase")]`.
//!
//! Struct fields use the Rust convention (snake_case) with
//! serde renames as needed for parity.

use std::collections::BTreeMap;

use repo_graph_classification::types::{ImportBinding, SourceLocation};
use serde::{Deserialize, Serialize};

// ── Edge types ──────────────────────────────────────────────────

/// The 18 canonical edge types. Mirror of `EdgeType` from
/// `src/core/model/types.ts:9`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EdgeType {
	// Structural
	Imports,
	Calls,
	Implements,
	Instantiates,
	// Data flow
	Reads,
	Writes,
	// Async / event
	Emits,
	Consumes,
	// Framework
	RoutesTo,
	RegisteredBy,
	GatedBy,
	// Relational
	DependsOn,
	Owns,
	TestedBy,
	Covers,
	// Exception flow
	Throws,
	Catches,
	// State machine
	TransitionsTo,
}

/// Edge resolution provenance. Mirror of `Resolution` from
/// `src/core/model/types.ts:41`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
	/// Deterministically resolved from source code.
	Static,
	/// Known to exist but resolved at runtime.
	Dynamic,
	/// Guessed from naming, proximity, or heuristics.
	Inferred,
}

// ── Node types ──────────────────────────────────────────────────

/// Node kind. Mirror of `NodeKind` from
/// `src/core/model/types.ts:54`.
///
/// The last three variants (`DbResource`, `FsPath`, `Blob`) were
/// added by SB-2-pre as canonical vocabulary for the state-
/// boundary slice. See
/// `docs/architecture/state-boundary-contract.txt` §4.2 for the
/// semantic definitions. These kinds are NOT emitted by any
/// extractor yet; emission begins in SB-3 once `state-extractor`
/// ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeKind {
	Module,
	File,
	Symbol,
	Endpoint,
	EventTopic,
	Table,
	ConfigKey,
	Test,
	State,
	Queue,
	Job,
	/// Logical database endpoint (driver-level identity). Added
	/// by SB-2-pre. Serialized as `"DB_RESOURCE"`.
	DbResource,
	/// Filesystem path or logical filesystem resource. Added by
	/// SB-2-pre. Serialized as `"FS_PATH"`.
	FsPath,
	/// Object-storage endpoint (S3 bucket, Azure Blob container,
	/// GCP Storage namespace). Added by SB-2-pre. Serialized as
	/// `"BLOB"`.
	Blob,
}

/// Node subtype. Mirror of `NodeSubtype` from
/// `src/core/model/types.ts:72`. Exactly matches the TS
/// contract — no Rust-only additions, no omissions.
///
/// The enum is a single flat vocabulary; context for ambiguous
/// variants comes from the parent `NodeKind`. For example,
/// `Namespace` is valid as a MODULE subtype AND as a BLOB
/// subtype (GCP Storage namespace); readers interpret it per
/// the containing node's kind.
///
/// The state-boundary slice-1 additions (SB-2-pre-2) are at the
/// end of the enum: `Connection`, `FilePath`, `DirectoryPath`,
/// `Logical`, `Cache`, `Bucket`, `Container`. The pre-existing
/// `Namespace` variant is reused for the BLOB-namespace case
/// (single serialized form `"NAMESPACE"` in both MODULE and
/// BLOB contexts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeSubtype {
	// SYMBOL subtypes
	Function,
	Class,
	Method,
	Interface,
	Struct,
	TypeAlias,
	Variable,
	Constant,
	Enum,
	EnumMember,
	Property,
	Constructor,
	Getter,
	Setter,
	// FILE subtypes
	Source,
	TestFile,
	Config,
	Migration,
	Schema,
	// MODULE subtypes
	Package,
	Namespace,
	Directory,
	// ENDPOINT subtypes
	Route,
	RpcMethod,
	GraphqlResolver,
	WebsocketHandler,
	// TEST subtypes
	TestSuite,
	TestCase,
	// ── State-boundary slice 1 (SB-2-pre-2) ─────────────────
	// Resource-kind subtypes. Not emitted by any extractor
	// under slice 1 directly — emission ships in SB-2 via the
	// `state-extractor` crate. Canonical vocabulary is pinned
	// here so state-extractor can select from a stable set.
	/// DB_RESOURCE subtype — logical database connection /
	/// data source. Serialized as `"CONNECTION"`.
	Connection,
	/// FS_PATH subtype — literal filesystem file path.
	/// Serialized as `"FILE_PATH"`.
	FilePath,
	/// FS_PATH subtype — literal filesystem directory path.
	/// Serialized as `"DIRECTORY_PATH"`.
	DirectoryPath,
	/// FS_PATH subtype — logical (config/env-derived) FS
	/// resource name rather than a concrete path.
	/// Serialized as `"LOGICAL"`.
	Logical,
	/// STATE subtype — cache endpoint (Redis, Memcached, etc.).
	/// Serialized as `"CACHE"`. The pre-existing STATE
	/// subtypes (`STATE_VALUE`, `STATE_FIELD`) remain reserved.
	Cache,
	/// BLOB subtype — object-storage bucket (S3 bucket, GCP
	/// Storage bucket). Serialized as `"BUCKET"`.
	Bucket,
	/// BLOB subtype — Azure Blob Storage container (and
	/// similarly-named providers). Serialized as `"CONTAINER"`.
	Container,
}

/// Visibility of a symbol. Mirror of `Visibility` from
/// `src/core/model/types.ts:197`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
	Public,
	Private,
	Protected,
	Internal,
	Export,
}

// ── Snapshot types ───────────────────────────────────────────────

/// Snapshot kind. Mirror of `SnapshotKind` from
/// `src/core/model/types.ts:111`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotKind {
	Full,
	Refresh,
	Working,
	Sealed,
}

/// Snapshot status. Mirror of `SnapshotStatus` from
/// `src/core/model/types.ts:120`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotStatus {
	Building,
	Ready,
	Stale,
	Failed,
}

/// File parse status. Mirror of `ParseStatus` from
/// `src/core/model/types.ts:132`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParseStatus {
	Parsed,
	Skipped,
	Failed,
	Stale,
}

// ── Extraction output types ─────────────────────────────────────

/// A node produced by an extractor. Mirrors `GraphNode` from
/// `src/core/model/node.ts` but owned by the indexer policy layer.
///
/// The storage port maps this to whatever row shape the adapter
/// needs for persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedNode {
	pub node_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub stable_key: String,
	pub kind: NodeKind,
	pub subtype: Option<NodeSubtype>,
	pub name: String,
	pub qualified_name: Option<String>,
	pub file_uid: Option<String>,
	pub parent_node_uid: Option<String>,
	pub location: Option<SourceLocation>,
	pub signature: Option<String>,
	pub visibility: Option<Visibility>,
	pub doc_comment: Option<String>,
	pub metadata_json: Option<String>,
}

/// An unresolved edge produced by an extractor. The `target_key`
/// is a symbolic reference (not a resolved node UID). Mirrors
/// `UnresolvedEdge` from `src/core/ports/extractor.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedEdge {
	pub edge_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub source_node_uid: String,
	/// Symbolic target reference. NOT a node UID.
	pub target_key: String,
	#[serde(rename = "type")]
	pub edge_type: EdgeType,
	pub resolution: Resolution,
	pub extractor: String,
	pub location: Option<SourceLocation>,
	pub metadata_json: Option<String>,
}

/// Per-function metrics produced by an extractor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedMetrics {
	pub cyclomatic_complexity: u32,
	pub parameter_count: u32,
	pub max_nesting_depth: u32,
}

/// Classification of a call site's positional argument 0 payload.
///
/// Added by SB-3-pre for state-boundary integration. The variant
/// carries the extracted value directly: the state-boundary layer
/// uses the payload plus the binding table to build the target
/// resource's stable key.
///
/// Slice-scoped on purpose: this covers only the argument-0 forms
/// SB-3 ships (string literal; `process.env.NAME` member read).
/// It is NOT the final universal call-argument model; object-
/// property extraction, constructor configs, and positional
/// beyond index 0 are reserved for later slices with their own
/// types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Arg0Payload {
	/// Argument 0 is a string literal. Carries the literal text
	/// (quotes stripped).
	StringLiteral {
		/// The literal string value.
		value: String,
	},
	/// Argument 0 is a `process.env.NAME` member read. Carries
	/// the env-key name (the `NAME` part of `process.env.NAME`).
	EnvKeyRead {
		/// The env-key identifier (e.g. `"DATABASE_URL"`).
		key_name: String,
	},
}

/// A call-site fact with the callee resolved to a (module,
/// symbol) pair via the file's import bindings, plus a
/// classification of argument 0's payload.
///
/// Added by SB-3-pre. Produced by the ts-extractor for every call
/// expression whose callee resolves via an import binding AND
/// whose arg 0 matches one of the patterns captured by
/// `Arg0Payload`. Consumed by `state-extractor` downstream; no
/// state-boundary-specific filtering in the extractor itself
/// (filtering happens at the binding-table match layer).
///
/// Narrow by design: only the fields SB-3 actually needs. The
/// generic `CALLS` edge on `ExtractedEdge` is unchanged and
/// remains the cross-runtime parity surface for call-graph
/// analysis; `ResolvedCallsite` is a Rust-only extractor-output
/// channel under the Fork-1 posture (TS population is deferred).
///
/// Slice-1 limitation: top-level calls at module scope do NOT
/// produce `ResolvedCallsite` facts. The `enclosing_symbol_node_uid`
/// field is typed as "enclosing SYMBOL node UID" by contract; the
/// call-extraction pipeline uses the FILE node UID as the caller
/// for top-level statements, which would violate the contract.
/// Top-level state touches remain visible as generic CALLS edges.
/// Widening the contract to accept file-rooted sources is deferred
/// pending a real consumer need for file-rooted state-boundary
/// edges (currently none).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCallsite {
	/// `node_uid` of the symbol containing the call site.
	pub enclosing_symbol_node_uid: String,
	/// Resolved source module of the callee, via import
	/// bindings. Matches the `specifier` of one of the file's
	/// `import_bindings`.
	pub resolved_module: String,
	/// Resolved exported-symbol path within `resolved_module`.
	/// For named imports with an alias, this is the ORIGINAL
	/// exported name (via `ImportBinding.imported_name`), not
	/// the local alias.
	pub resolved_symbol: String,
	/// Classification of argument 0's payload.
	pub arg0_payload: Arg0Payload,
	/// Source location of the call expression.
	pub source_location: SourceLocation,
}

/// The result of extracting a single file. Mirrors
/// `ExtractionResult` from `src/core/ports/extractor.ts`.
///
/// Cross-runtime parity: every field of this struct has a
/// corresponding field on the TS `ExtractionResult` interface.
/// `resolved_callsites` / `resolvedCallsites` was added by
/// SB-3-pre and is structurally present on both runtimes; only
/// the Rust `ts-extractor` populates it today, but the port
/// contract remains unified (TS extractors return empty arrays
/// under the Fork-1 posture). See
/// `docs/TECH-DEBT.md` for the TS-side deferred population note.
pub struct ExtractionResult {
	/// FILE node + all symbol nodes found in this file.
	pub nodes: Vec<ExtractedNode>,
	/// Unresolved edges (symbolic targets) found in this file.
	pub edges: Vec<ExtractedEdge>,
	/// Per-function metrics keyed by stable_key. BTreeMap for
	/// deterministic iteration (no-HashMap API rule).
	pub metrics: BTreeMap<String, ExtractedMetrics>,
	/// Import statement bindings found in this file.
	pub import_bindings: Vec<ImportBinding>,
	/// Resolved callsite facts for state-boundary integration
	/// (SB-3-pre). Present on both runtimes' `ExtractionResult`
	/// contract; populated by the Rust `ts-extractor` under
	/// Fork 1, empty for all other extractors. See
	/// `ResolvedCallsite` docs.
	pub resolved_callsites: Vec<ResolvedCallsite>,
}

// ── Indexer input/output types ──────────────────────────────────

/// Progress callback type. Receives an `IndexProgressEvent` for
/// each phase transition during indexing. Mirror of the TS
/// `onProgress` callback in `IndexOptions`.
pub type ProgressCallback = Box<dyn FnMut(&IndexProgressEvent) + Send>;

/// Options for an indexing operation. Mirror of `IndexOptions`
/// from `src/core/ports/indexer.ts`.
///
/// Not `Clone` or `Debug` because `on_progress` is a boxed
/// closure. Use `IndexOptions::default()` for tests.
pub struct IndexOptions {
	/// Glob patterns to exclude from scanning.
	pub exclude: Vec<String>,
	/// Glob patterns to include (if non-empty, only matching files
	/// are indexed).
	pub include: Vec<String>,
	/// Git commit SHA to record as the snapshot basis.
	pub basis_commit: Option<String>,
	/// Batch size for edge resolution (default 10,000).
	pub edge_batch_size: Option<usize>,
	/// Optional progress callback. Called for each phase transition.
	pub on_progress: Option<ProgressCallback>,
}

impl Default for IndexOptions {
	fn default() -> Self {
		Self {
			exclude: Vec::new(),
			include: Vec::new(),
			basis_commit: None,
			edge_batch_size: None,
			on_progress: None,
		}
	}
}

/// Result of an indexing operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexResult {
	pub snapshot_uid: String,
	pub files_total: u64,
	pub nodes_total: u64,
	pub edges_total: u64,
	pub edges_unresolved: u64,
	pub unresolved_breakdown: BTreeMap<String, u64>,
	pub duration_ms: u64,
	pub orphaned_declarations: u64,
	/// Per-symbol metrics from extraction.
	///
	/// RS-MS-3c-prereq: Accumulated from all extraction results.
	/// Keyed by symbol stable_key, containing cyclomatic_complexity,
	/// parameter_count, and max_nesting_depth. Persisted by the
	/// compose layer after index_repo returns.
	pub metrics: BTreeMap<String, ExtractedMetrics>,
}

/// Progress event during indexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexProgressEvent {
	pub phase: IndexPhase,
	pub current: u64,
	pub total: u64,
	pub file: Option<String>,
}

/// Indexing phase for progress reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexPhase {
	Scanning,
	Extracting,
	Resolving,
	Persisting,
}
