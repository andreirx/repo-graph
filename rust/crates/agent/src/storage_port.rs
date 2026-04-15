//! Dependency-inverted read port for the agent use-case layer.
//!
//! The `AgentStorageRead` trait is defined here (policy side) and
//! implemented by the storage adapter crate. Orient calls port
//! methods; port methods return agent-owned DTOs; storage errors
//! are mapped to `AgentStorageError` at the adapter boundary.
//!
//! Design rules for this module:
//!
//!   1. No storage DTOs leak through the trait. Every return type
//!      is defined in this file (or imported from the agent DTO
//!      modules). The storage crate maps its internal row shapes
//!      into these agent-owned types.
//!
//!   2. Every method is read-only. No writes, no transactions, no
//!      schema mutations.
//!
//!   3. Method names mirror the domain vocabulary the use case
//!      needs, NOT the storage method names. If the storage crate
//!      renames `find_cycles`, the trait method stays
//!      `find_module_cycles`.
//!
//!   4. Each method's error branch returns `AgentStorageError`
//!      with a stable `operation: &'static str`. Callers and
//!      tests can pattern-match on that identifier without
//!      depending on any storage-crate internals.
//!
//!   5. `get_trust_summary` is intentionally a port method even
//!      though the trust data comes from a separate `trust` crate.
//!      The agent crate does not depend on `repo-graph-trust`
//!      directly. The storage adapter is responsible for calling
//!      `trust::assemble_trust_report` internally and projecting
//!      the result into the agent-owned `AgentTrustSummary` DTO.
//!      This keeps orient's public surface free of trust-crate
//!      types and keeps the trust crate's own trait surface
//!      untouched by agent concerns.

use crate::errors::AgentStorageError;

// ── Repo identity ────────────────────────────────────────────────

/// Minimal repo identity as seen by the agent layer.
///
/// Only fields the use case needs. The `name` feeds the output
/// envelope's `repo` field. `repo_uid` closes the loop for
/// follow-up commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRepo {
	pub repo_uid: String,
	pub name: String,
}

// ── Snapshot identity ────────────────────────────────────────────

/// Minimal snapshot identity as seen by the agent layer.
///
/// `scope` mirrors the `kind` column from the storage snapshots
/// table (`"full"` or `"incremental"`). `basis_commit` is the git
/// commit the snapshot was indexed against, if any.
///
/// `created_at` is ISO-8601 and carried as a plain string. The
/// agent layer does not parse timestamps; it forwards them to the
/// envelope for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSnapshot {
	pub snapshot_uid: String,
	pub repo_uid: String,
	pub scope: String,
	pub basis_commit: Option<String>,
	pub created_at: String,
	pub files_total: u64,
	pub nodes_total: u64,
	pub edges_total: u64,
}

// ── Stale file ───────────────────────────────────────────────────

/// A file whose stored parse_status is stale. This is a
/// snapshot-internal condition (the parse state recorded in
/// storage does not reflect the latest version of the file). It
/// does NOT mean "the working tree has changed since indexing" —
/// that requires a filesystem/git comparison the use-case layer
/// does not perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStaleFile {
	pub path: String,
}

// ── Module cycle ─────────────────────────────────────────────────

/// A module-level dependency cycle found in the import graph.
///
/// `modules` is ordered in ring order; the first entry is the
/// canonical smallest module UID (the cycle is deduplicated in
/// storage).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCycle {
	pub length: usize,
	pub modules: Vec<String>,
}

// ── Dead node ────────────────────────────────────────────────────

/// A node unreferenced by any edge and not marked as an entry
/// point. The agent layer summarizes these into `DEAD_CODE` signal
/// evidence; raw lists never cross the output envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDeadNode {
	pub stable_key: String,
	pub symbol: String,
	pub kind: String,
	pub file: Option<String>,
	pub line_count: Option<u64>,
}

// ── Boundary declaration ─────────────────────────────────────────

/// An active boundary declaration: "module X must not import from
/// module Y". Path prefixes are repo-relative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBoundaryDeclaration {
	pub source_module: String,
	pub forbidden_target: String,
	pub reason: Option<String>,
}

// ── Import edge (violation evidence) ─────────────────────────────

/// One import edge that crosses a forbidden boundary. Used as raw
/// input to the boundary aggregator; the aggregator summarizes
/// these into `BOUNDARY_VIOLATIONS` evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentImportEdge {
	pub source_file: String,
	pub target_file: String,
}

// ── Repo-level structural summary ────────────────────────────────

/// Repo-wide totals + language roll-up used by `MODULE_SUMMARY`.
///
/// `file_count` and `symbol_count` are counted from the snapshot
/// directly; they are not derived from module-discovery data.
/// `languages` is a sorted, deduplicated list of the language
/// column values on file_versions rows for the snapshot. It may
/// be empty when the indexer has not populated the column — in
/// that case the aggregator emits an empty list, not a limit
/// code (the contract reserves the `MODULE_DATA_UNAVAILABLE` and
/// similar limits for module discovery data, a different surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRepoSummary {
	pub file_count: u64,
	pub symbol_count: u64,
	pub languages: Vec<String>,
}

// ── Trust summary (projection) ───────────────────────────────────

/// Narrow projection of the trust report for agent consumption.
///
/// The agent crate does not depend on `repo-graph-trust`. The
/// storage adapter implements this method by calling
/// `trust::assemble_trust_report` internally and projecting the
/// result into this DTO. Only fields the orient use case reads
/// are surfaced here — the full `TrustReport` carries much more.
///
/// `call_resolution_rate` is in the range `[0.0, 1.0]`.
///
/// `enrichment_applied` is the agent-facing boolean: `true` iff
/// any compiler enrichment (TypeScript TypeChecker, rust-analyzer,
/// etc.) produced at least one resolved edge. `enrichment_eligible`
/// is the count of edges that were candidates for enrichment;
/// when the eligible count is zero the agent layer treats
/// enrichment as "not applicable" rather than "missing".
#[derive(Debug, Clone, PartialEq)]
pub struct AgentTrustSummary {
	pub call_resolution_rate: f64,
	pub resolved_calls: u64,
	pub unresolved_calls: u64,
	pub enrichment_applied: bool,
	pub enrichment_eligible: u64,
	pub enrichment_enriched: u64,
}

// ── Trait ────────────────────────────────────────────────────────

/// The narrow read port the agent use-case layer needs from a
/// storage backend.
///
/// **Defined by the policy layer (agent crate). Implemented by
/// the adapter layer (storage crate).** The storage crate adds
/// `repo-graph-agent` as a dependency to import this trait and
/// implements it on `StorageConnection`.
///
/// All methods are read-only. Every method maps storage errors
/// into `AgentStorageError` at the adapter boundary so the agent
/// crate never sees rusqlite, SQL diagnostics, or table names.
pub trait AgentStorageRead {
	/// Look up a repo by its stable `repo_uid`. Returns
	/// `Ok(None)` when the repo is not registered.
	fn get_repo(
		&self,
		repo_uid: &str,
	) -> Result<Option<AgentRepo>, AgentStorageError>;

	/// Look up the latest READY snapshot for a repo. Returns
	/// `Ok(None)` when the repo exists but has never had a
	/// successfully completed index. BUILDING, STALE, and FAILED
	/// snapshots are excluded.
	fn get_latest_snapshot(
		&self,
		repo_uid: &str,
	) -> Result<Option<AgentSnapshot>, AgentStorageError>;

	/// List files whose recorded parse state is stale for a
	/// snapshot. Used as the `TRUST_STALE_SNAPSHOT` trigger.
	fn get_stale_files(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<AgentStaleFile>, AgentStorageError>;

	/// Return module-level dependency cycles for a snapshot.
	/// Canonicalized (each cycle appears once, rotated to its
	/// lexicographically smallest UID).
	fn find_module_cycles(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<AgentCycle>, AgentStorageError>;

	/// Return nodes unreferenced by any reference edge, minus
	/// declared entrypoints and framework-liveness inferences.
	/// `kind_filter`, when `Some`, restricts to nodes of that
	/// kind (e.g. `"SYMBOL"`).
	fn find_dead_nodes(
		&self,
		snapshot_uid: &str,
		repo_uid: &str,
		kind_filter: Option<&str>,
	) -> Result<Vec<AgentDeadNode>, AgentStorageError>;

	/// Return all active boundary declarations for a repo.
	/// Each declaration names a source module and a forbidden
	/// target module.
	fn get_active_boundary_declarations(
		&self,
		repo_uid: &str,
	) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError>;

	/// Return IMPORTS edges where the source file path is under
	/// `source_prefix` AND the target file path is under
	/// `target_prefix`. Used to detect boundary violations given
	/// a declaration.
	fn find_imports_between_paths(
		&self,
		snapshot_uid: &str,
		source_prefix: &str,
		target_prefix: &str,
	) -> Result<Vec<AgentImportEdge>, AgentStorageError>;

	/// Repo-level structural totals used by `MODULE_SUMMARY`.
	fn compute_repo_summary(
		&self,
		snapshot_uid: &str,
	) -> Result<AgentRepoSummary, AgentStorageError>;

	/// Assemble a narrow trust projection for the snapshot.
	///
	/// Implementation note: the storage adapter is expected to
	/// call `repo_graph_trust::assemble_trust_report` (or an
	/// equivalent) internally and project the result into
	/// `AgentTrustSummary`. The agent crate does not depend on
	/// `repo-graph-trust`; all trust policy lives on the adapter
	/// side of this method.
	fn get_trust_summary(
		&self,
		repo_uid: &str,
		snapshot_uid: &str,
	) -> Result<AgentTrustSummary, AgentStorageError>;
}
