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
	pub is_test: bool,
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

// ── Reliability axis (projection of trust axis scores) ──────────

/// Three-state reliability level, mirror of `trust::ReliabilityLevel`.
///
/// The agent crate keeps its own enum (rather than re-exporting
/// trust's) so the public surface of `repo-graph-agent` is
/// independent of the trust crate. Storage adapters map trust's
/// enum into this one at the port boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentReliabilityLevel {
	Low,
	Medium,
	High,
}

/// One reliability axis score: a level plus human-readable
/// reasons. Mirrors `trust::ReliabilityAxisScore`. Reasons are
/// arbitrary free-form strings produced by the trust rules (e.g.
/// `"missing_entrypoint_declarations"`,
/// `"call_resolution_rate=22.2%_below_50%"`).
///
/// Downstream signal / limit emitters that carry reasons to the
/// output envelope must copy them verbatim; the reason vocabulary
/// is controlled by the trust crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentReliabilityAxis {
	pub level: AgentReliabilityLevel,
	pub reasons: Vec<String>,
}

// ── Enrichment state ─────────────────────────────────────────────

/// Three-state enrichment execution model.
///
/// Replaces the Rust-42-era `enrichment_applied: bool` field,
/// which collapsed two distinct states ("phase never ran" vs
/// "phase ran but had nothing to do") into one. The Rust indexer
/// does not run a compiler enrichment phase; TS indexers may or
/// may not, and may or may not have eligible edges. This enum
/// distinguishes all three.
///
/// Variants:
///
///   - `Ran`: the enrichment phase executed with at least one
///     eligible edge. The scalar `enrichment_enriched` count
///     indicates how many were actually resolved (it may be
///     zero — `Ran` is about phase execution, not success). Do
///     NOT rename this variant to `Applied`: "applied" implies
///     successful resolution, which is stronger than what the
///     storage adapter can claim at this boundary.
///
///   - `NotApplicable`: the enrichment phase executed with
///     zero eligible edges. Nothing to do. Confidence is NOT
///     penalized on the enrichment axis in this state.
///
///   - `NotRun`: the enrichment phase did NOT execute. The
///     indexer did not report any enrichment status at all. The
///     confidence axis penalizes this state because the caller
///     has no evidence that the call graph was ever enriched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentState {
	Ran,
	NotApplicable,
	NotRun,
}

// ── Trust summary (projection) ───────────────────────────────────

/// Narrow projection of the trust report for agent consumption.
///
/// The agent crate does not depend on `repo-graph-trust`. The
/// storage adapter implements `get_trust_summary` by calling
/// `trust::assemble_trust_report` internally and projecting the
/// result into this DTO. Only fields the orient use case reads
/// are surfaced here — the full `TrustReport` carries much more.
///
/// ── Reliability axes (Rust-43 F1/F3 fix) ────────────────────
///
/// `call_graph_reliability` and `dead_code_reliability` are
/// projections of the trust crate's composite reliability axes.
/// The agent orient pipeline uses `dead_code_reliability.level`
/// as the single authoritative gate for whether the DEAD_CODE
/// signal can be emitted. The trust layer already composes
/// call-graph reliability, entrypoint declarations, registry
/// pattern suspicion, and framework-heavy suspicion into this
/// axis; the agent crate must NOT re-derive those rules.
///
/// ── Enrichment state (Rust-43 F2 fix) ───────────────────────
///
/// `enrichment_state` replaces the earlier boolean. See
/// `EnrichmentState` docs for the three-state model.
/// `enrichment_eligible` and `enrichment_enriched` remain as
/// scalar counts for signal evidence (they are NOT the
/// authoritative state — the enum is).
#[derive(Debug, Clone, PartialEq)]
pub struct AgentTrustSummary {
	pub call_resolution_rate: f64,
	pub resolved_calls: u64,
	pub unresolved_calls: u64,
	pub call_graph_reliability: AgentReliabilityAxis,
	pub dead_code_reliability: AgentReliabilityAxis,
	pub enrichment_state: EnrichmentState,
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
