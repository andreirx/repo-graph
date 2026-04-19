//! Indexer storage port — composed facade over narrow sub-traits.
//!
//! The indexer (policy) defines the storage interface it needs.
//! The storage crate (adapter) implements these sub-traits on
//! `StorageConnection`. The dependency direction is adapter → policy.
//!
//! Sub-traits are added progressively per substep:
//!   - R5-B: `SnapshotLifecyclePort`, `FileCatalogPort`
//!   - R5-F: `NodeStorePort`, `EdgeStorePort`, `UnresolvedEdgePort`,
//!           `FileSignalPort`
//!   - R5-H: `DeltaCopyPort`
//!
//! Each sub-trait has its own `type Error: Debug + Display`. The
//! composed `IndexerStoragePort` facade is a blanket impl for any
//! type implementing all currently-defined sub-traits.
//!
//! ── Mutability convention ────────────────────────────────────
//!
//! Write operations take `&mut self`. Read operations take `&self`.
//! The implementor decides whether internal mutability is needed
//! for `&self` writes (the Rust storage crate uses `&self` for
//! single-statement writes and `&mut self` for transaction-wrapped
//! batches). The traits use `&mut self` for writes as the safest
//! bound.

use std::collections::BTreeMap;

use repo_graph_classification::types::{
	UnresolvedEdgeBasisCode, UnresolvedEdgeCategory, UnresolvedEdgeClassification,
};

use crate::types::{
	EdgeType, ParseStatus, Resolution, SnapshotKind, SnapshotStatus,
};

// ── Snapshot lifecycle DTOs ──────────────────────────────────────

/// Input for creating a snapshot. Mirror of `CreateSnapshotInput`
/// from `src/core/ports/storage.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateSnapshotInput {
	pub repo_uid: String,
	pub kind: SnapshotKind,
	pub basis_ref: Option<String>,
	pub basis_commit: Option<String>,
	pub parent_snapshot_uid: Option<String>,
	pub label: Option<String>,
	pub toolchain_json: Option<String>,
}

/// Input for updating snapshot status. Mirror of
/// `UpdateSnapshotStatusInput` from `src/core/ports/storage.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSnapshotStatusInput {
	pub snapshot_uid: String,
	pub status: SnapshotStatus,
	pub completed_at: Option<String>,
}

/// Snapshot record. Mirror of `Snapshot` from
/// `src/core/model/snapshot.ts`. Owned by the indexer policy layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub parent_snapshot_uid: Option<String>,
	pub kind: SnapshotKind,
	pub basis_ref: Option<String>,
	pub basis_commit: Option<String>,
	pub dirty_hash: Option<String>,
	pub status: SnapshotStatus,
	pub files_total: u64,
	pub nodes_total: u64,
	pub edges_total: u64,
	pub created_at: String,
	pub completed_at: Option<String>,
	pub label: Option<String>,
	pub toolchain_json: Option<String>,
}

// ── File catalog DTOs ────────────────────────────────────────────

/// Tracked file record. Mirror of `TrackedFile` from
/// `src/core/model/file.ts`. Owned by the indexer policy layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedFile {
	pub file_uid: String,
	pub repo_uid: String,
	pub path: String,
	pub language: Option<String>,
	pub is_test: bool,
	pub is_generated: bool,
	pub is_excluded: bool,
}

/// File version record. Mirror of `FileVersion` from
/// `src/core/model/file.ts`. Owned by the indexer policy layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersion {
	pub snapshot_uid: String,
	pub file_uid: String,
	pub content_hash: String,
	pub ast_hash: Option<String>,
	pub extractor: Option<String>,
	pub parse_status: ParseStatus,
	pub size_bytes: Option<u64>,
	pub line_count: Option<u64>,
	pub indexed_at: String,
}

// ── Sub-traits ───────────────────────────────────────────────────

/// Snapshot lifecycle operations. Covers snapshot creation,
/// status transitions, count updates, and diagnostics persistence.
pub trait SnapshotLifecyclePort {
	type Error: std::fmt::Debug + std::fmt::Display;

	/// Create a new snapshot in BUILDING status.
	fn create_snapshot(
		&mut self,
		input: &CreateSnapshotInput,
	) -> Result<Snapshot, Self::Error>;

	/// Look up a snapshot by UID. Returns `None` if not found.
	fn get_snapshot(
		&self,
		snapshot_uid: &str,
	) -> Result<Option<Snapshot>, Self::Error>;

	/// Get the latest READY snapshot for a repo. Returns `None` if
	/// no ready snapshot exists.
	fn get_latest_snapshot(
		&self,
		repo_uid: &str,
	) -> Result<Option<Snapshot>, Self::Error>;

	/// Transition a snapshot's status (e.g., BUILDING → READY).
	fn update_snapshot_status(
		&mut self,
		input: &UpdateSnapshotStatusInput,
	) -> Result<(), Self::Error>;

	/// Recompute and persist aggregate counts (files_total,
	/// nodes_total, edges_total) from the actual data.
	fn update_snapshot_counts(
		&mut self,
		snapshot_uid: &str,
	) -> Result<(), Self::Error>;

	/// Persist extraction diagnostics JSON on a snapshot.
	fn update_snapshot_extraction_diagnostics(
		&mut self,
		snapshot_uid: &str,
		diagnostics_json: &str,
	) -> Result<(), Self::Error>;
}

/// File catalog operations. Covers file tracking, file version
/// management, and stale-file detection.
pub trait FileCatalogPort {
	type Error: std::fmt::Debug + std::fmt::Display;

	/// Insert or update tracked files. Idempotent on file_uid.
	fn upsert_files(
		&mut self,
		files: &[TrackedFile],
	) -> Result<(), Self::Error>;

	/// Insert or update file versions for a snapshot.
	fn upsert_file_versions(
		&mut self,
		versions: &[FileVersion],
	) -> Result<(), Self::Error>;

	/// Get all non-excluded tracked files for a repo.
	fn get_files_by_repo(
		&self,
		repo_uid: &str,
	) -> Result<Vec<TrackedFile>, Self::Error>;

	/// Get files with stale parse status in a snapshot.
	fn get_stale_files(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<TrackedFile>, Self::Error>;

	/// Query content hashes for all file versions in a snapshot.
	/// Returns a map of file_uid → content_hash. Used by delta
	/// indexing to detect changed files.
	///
	/// `BTreeMap` for deterministic iteration (no-HashMap rule).
	fn query_file_version_hashes(
		&self,
		snapshot_uid: &str,
	) -> Result<BTreeMap<String, String>, Self::Error>;
}

// ── Node store DTOs ──────────────────────────────────────────────

// `ExtractedNode` from `types.rs` is used as the write-side input.
// `ResolverNode` from `resolver.rs` is the read-side output for
// resolution. `ResolvedEdge` is the resolved-edge write input.

// Re-export for convenience in trait signatures.
pub use crate::resolver::ResolverNode;
pub use crate::resolver::ResolvedEdge;
pub use crate::types::ExtractedNode;

// ── Extraction edge DTO ──────────────────────────────────────────

/// Persisted extraction edge — the durable form of an extractor's
/// unresolved edge, with an additional `source_file_uid` column.
/// Mirror of `ExtractionEdge` from `src/core/ports/storage.ts:1076`.
///
/// `edge_type` and `resolution` use typed enums (not raw strings)
/// so the policy layer works with validated vocabulary. The storage
/// adapter converts to/from strings at the persistence boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionEdgeRow {
	pub edge_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub source_node_uid: String,
	pub target_key: String,
	pub edge_type: EdgeType,
	pub resolution: Resolution,
	pub extractor: String,
	pub line_start: Option<i64>,
	pub col_start: Option<i64>,
	pub line_end: Option<i64>,
	pub col_end: Option<i64>,
	pub metadata_json: Option<String>,
	pub source_file_uid: Option<String>,
}

// ── Persisted unresolved edge DTO ────────────────────────────────

/// Classified unresolved edge ready for persistence. Mirrors
/// `PersistedUnresolvedEdge` from `src/core/ports/storage.ts:865`.
///
/// All vocabulary fields use typed enums. The storage adapter
/// serializes them to snake_case/SCREAMING_SNAKE_CASE strings
/// at the persistence boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedUnresolvedEdge {
	pub edge_uid: String,
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub source_node_uid: String,
	pub target_key: String,
	pub edge_type: EdgeType,
	pub resolution: Resolution,
	pub extractor: String,
	pub line_start: Option<i64>,
	pub col_start: Option<i64>,
	pub line_end: Option<i64>,
	pub col_end: Option<i64>,
	pub metadata_json: Option<String>,
	pub category: UnresolvedEdgeCategory,
	pub classification: UnresolvedEdgeClassification,
	pub classifier_version: u32,
	pub basis_code: UnresolvedEdgeBasisCode,
	pub observed_at: String,
}

// ── File signal DTO ──────────────────────────────────────────────

/// Per-file classifier signals (import bindings, package deps,
/// tsconfig aliases). Mirror of `FileSignalRow` from
/// `src/core/ports/storage.ts:1099`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSignalRow {
	pub snapshot_uid: String,
	pub file_uid: String,
	pub import_bindings_json: Option<String>,
	pub package_dependencies_json: Option<String>,
	pub tsconfig_aliases_json: Option<String>,
}

// ── Sub-traits (R5-F batch) ──────────────────────────────────────

/// Node persistence and retrieval operations.
pub trait NodeStorePort {
	type Error: std::fmt::Debug + std::fmt::Display;

	/// Insert a batch of nodes. Transaction-wrapped.
	fn insert_nodes(
		&mut self,
		nodes: &[ExtractedNode],
	) -> Result<(), Self::Error>;

	/// Query all nodes in a snapshot (full GraphNode shape).
	fn query_all_nodes(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ExtractedNode>, Self::Error>;

	/// Query slim resolver nodes for building the ResolverIndex.
	/// Returns all nodes in the snapshot with only the fields
	/// needed for resolution.
	fn query_resolver_nodes(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ResolverNode>, Self::Error>;

	/// Delete nodes (and incident edges) for a specific file.
	fn delete_nodes_by_file(
		&mut self,
		snapshot_uid: &str,
		file_uid: &str,
	) -> Result<(), Self::Error>;
}

/// Resolved edge persistence operations.
pub trait EdgeStorePort {
	type Error: std::fmt::Debug + std::fmt::Display;

	/// Insert a batch of resolved edges. Transaction-wrapped.
	fn insert_resolved_edges(
		&mut self,
		edges: &[ResolvedEdge],
	) -> Result<(), Self::Error>;

	/// Insert a batch of extraction edges (durable unresolved
	/// edges with source_file_uid). Transaction-wrapped.
	fn insert_extraction_edges(
		&mut self,
		edges: &[ExtractionEdgeRow],
	) -> Result<(), Self::Error>;

	/// Query a batch of extraction edges using cursor pagination.
	/// Returns up to `limit` rows with `edge_uid > after_edge_uid`
	/// (or from the start if `after_edge_uid` is `None`).
	fn query_extraction_edges_batch(
		&self,
		snapshot_uid: &str,
		limit: usize,
		after_edge_uid: Option<&str>,
	) -> Result<Vec<ExtractionEdgeRow>, Self::Error>;

	/// Delete resolved edges by their UIDs.
	fn delete_edges_by_uids(
		&mut self,
		edge_uids: &[String],
	) -> Result<(), Self::Error>;
}

/// Classified unresolved edge persistence.
pub trait UnresolvedEdgePort {
	type Error: std::fmt::Debug + std::fmt::Display;

	/// Insert a batch of classified unresolved edges.
	fn insert_unresolved_edges(
		&mut self,
		edges: &[PersistedUnresolvedEdge],
	) -> Result<(), Self::Error>;
}

/// File-level classifier signal persistence and retrieval.
pub trait FileSignalPort {
	type Error: std::fmt::Debug + std::fmt::Display;

	/// Insert file signals (import bindings, package deps,
	/// tsconfig aliases) for one or more files.
	fn insert_file_signals(
		&mut self,
		signals: &[FileSignalRow],
	) -> Result<(), Self::Error>;

	/// Query file signals for a batch of files in a snapshot.
	fn query_file_signals_batch(
		&self,
		snapshot_uid: &str,
		file_uids: &[String],
	) -> Result<Vec<FileSignalRow>, Self::Error>;
}

// ── Delta copy DTOs ──────────────────────────────────────────────

/// Input for the copy-forward operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyForwardInput {
	pub from_snapshot_uid: String,
	pub to_snapshot_uid: String,
	pub repo_uid: String,
	/// File UIDs of unchanged files to copy forward.
	pub file_uids: Vec<String>,
}

/// Identity of a null-file (resource) node copied forward.
/// Used by the orchestrator to dedup hook-emitted resource nodes
/// against carried-forward ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopiedResourceNodeKey {
	/// The stable_key of the copied node.
	pub stable_key: String,
	/// The `kind` column value (e.g. `"FS_PATH"`).
	pub kind: String,
	/// The `subtype` column value (e.g. `"FILE_PATH"`).
	pub subtype: Option<String>,
	/// The `name` column value.
	pub name: String,
}

/// Result counts from the copy-forward operation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CopyForwardResult {
	/// Count of file-owned nodes copied.
	pub nodes_copied: u64,
	/// Count of extraction edges copied.
	pub extraction_edges_copied: u64,
	/// Count of file signals copied.
	pub file_signals_copied: u64,
	/// Count of file versions copied.
	pub file_versions_copied: u64,
	/// Resource nodes (file_uid IS NULL) copied from the parent
	/// snapshot. Populated by SB-4-pre Fix B so the orchestrator
	/// can dedup hook-emitted resource nodes against them.
	pub copied_resource_node_keys: Vec<CopiedResourceNodeKey>,
}

// ── DeltaCopyPort (R5-H) ────────────────────────────────────────

/// Delta copy-forward operations for refresh indexing. Owns
/// composite transaction semantics: the copy-forward of nodes,
/// extraction edges, file signals, and file versions happens as
/// a single atomic operation inside the storage adapter. No
/// transaction handles cross the policy boundary.
pub trait DeltaCopyPort {
	type Error: std::fmt::Debug + std::fmt::Display;

	/// Copy all artifacts for unchanged files from parent to child
	/// snapshot in a single transaction.
	///
	/// Handles:
	///   - nodes (new node_uids, preserving stable_keys)
	///   - extraction_edges (new edge_uids, remapped source_node_uids)
	///   - file_signals
	///   - file_versions
	///
	/// Returns counts per artifact type for delta trust metadata.
	fn copy_forward_unchanged_files(
		&mut self,
		input: &CopyForwardInput,
	) -> Result<CopyForwardResult, Self::Error>;
}

// ── Composed facade ──────────────────────────────────────────────

/// Composed storage facade for the indexer. Unifies the error
/// type across all sub-traits so orchestration functions can
/// return a single `Result<T, S::StorageError>`.
///
/// The sub-trait set grows per substep:
///   - R5-B: SnapshotLifecyclePort + FileCatalogPort
///   - R5-F: + NodeStorePort + EdgeStorePort + UnresolvedEdgePort
///           + FileSignalPort
///   - R5-H: + DeltaCopyPort
///
/// Each sub-trait declares its own `type Error`, but the facade
/// constrains them all to be the same concrete type via the
/// `StorageError` associated type. This gives the orchestrator
/// one coherent error path: `Result<T, <S as IndexerStoragePort>::StorageError>`.
pub trait IndexerStoragePort:
	SnapshotLifecyclePort<Error = <Self as IndexerStoragePort>::StorageError>
	+ FileCatalogPort<Error = <Self as IndexerStoragePort>::StorageError>
	+ NodeStorePort<Error = <Self as IndexerStoragePort>::StorageError>
	+ EdgeStorePort<Error = <Self as IndexerStoragePort>::StorageError>
	+ UnresolvedEdgePort<Error = <Self as IndexerStoragePort>::StorageError>
	+ FileSignalPort<Error = <Self as IndexerStoragePort>::StorageError>
	+ DeltaCopyPort<Error = <Self as IndexerStoragePort>::StorageError>
{
	/// The unified error type for all storage operations.
	type StorageError: std::fmt::Debug + std::fmt::Display;
}

impl<T, E> IndexerStoragePort for T
where
	T: SnapshotLifecyclePort<Error = E>
		+ FileCatalogPort<Error = E>
		+ NodeStorePort<Error = E>
		+ EdgeStorePort<Error = E>
		+ UnresolvedEdgePort<Error = E>
		+ FileSignalPort<Error = E>
		+ DeltaCopyPort<Error = E>,
	E: std::fmt::Debug + std::fmt::Display,
{
	type StorageError = E;
}
