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
}

/// Node subtype. Mirror of `NodeSubtype` from
/// `src/core/model/types.ts:72`. Exactly matches the TS
/// contract — no Rust-only additions, no omissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeSubtype {
	// SYMBOL subtypes
	Function,
	Class,
	Method,
	Interface,
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

/// The result of extracting a single file. Mirrors
/// `ExtractionResult` from `src/core/ports/extractor.ts`.
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
