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

use crate::cycle_composition::CycleTestComposition;
use crate::cycle_type_only::CycleTypeOnly;
use crate::errors::AgentStorageError;
use crate::package_groups::ManifestRoot;

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
/// `modules` arrives from the storage port in TRAVERSAL order (a Tarjan
/// artifact — an SCC is a set, not a ring; the prior doc claim that storage
/// rotated each cycle to its smallest member was verified false,
/// EC-M2-LEAF-SERVE-1). Every rendered consumer canonicalizes via
/// `ordering::canonicalize_cycles` (members sorted; list length-DESC) before
/// truncation, so the output is a pure function of the cycle SET regardless
/// of which store produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCycle {
    pub length: usize,
    pub modules: Vec<String>,
    /// ORIENT-CYCLES-DISAGREE-1: the FIXTURE-POLLUTION-1 test-only classification of this
    /// cycle, computed at the SERVING computation from the stored `is_test` fact (the shared
    /// [`crate::cycle_composition`] classifier). `Some` on the SQLite-served path (the storage
    /// adapter reaches `is_test`), which lets `orient`'s cycle leaf report the SAME
    /// production/test-only split `cycles` renders. `None` where the serving computation
    /// cannot reach `is_test` (the LiveGraph module-cycle serve — FIXTURE-POLLUTION-1 §2.3
    /// asymmetry — and the focus/path-scoped cycle reads): the headline then falls back to the
    /// raw total, exactly as `cycles` does on those same paths. Additive: NEVER demote or
    /// classify from a `None` (STANDING HONESTY RULE #2).
    ///
    /// Why this rides the port (operator ruling cycle-count-derivation-placement, 2026-09-02):
    /// the ratified boundary crossing is the two integers on `ImportCyclesEvidence`; a
    /// storage-port contribution is permitted "only if the leaf genuinely cannot carry two
    /// integers." It cannot — the `agent` aggregator that builds that leaf is pure and reaches
    /// `is_test` ONLY through this port. This additive per-cycle field is the minimal
    /// contribution, and riding `find_module_cycles` (which the LiveGraph decorator overrides)
    /// makes the §2.3 route-conditionality automatic — no separate route check. NOT serialized
    /// (only `ImportCyclesEvidence`/`CycleEvidence` reach the JSON); it is an internal carrier,
    /// not a new query method.
    pub test_composition: Option<CycleTestComposition>,
    /// TYPE-ONLY-IMPORTS-1: the per-cycle runtime-vs-type-only verdict, computed at the SQLite
    /// SERVING computation (`agent_cycle_labeling::label_module_cycles`) from the stored
    /// per-module-edge `is_type_only` fact via the SHARED
    /// [`crate::cycle_type_only::classify_cycles_type_only`] — the SAME kernel the `cycles`
    /// command's serving computation calls, so `orient`'s cycle leaf and `cycles` cannot state a
    /// different verdict (route-agreement DoD; ORIENT-CYCLES-DISAGREE-1 "one derivation").
    ///
    /// `Some` on the SQLite-served path where the fact is reachable AND the cycle has a TS/JS
    /// member (§5); `None` where the fact is NOT reachable (the LiveGraph module-cycle serve —
    /// the packet forbids pushing `is_type_only` into the warm path — and focus/path-scoped
    /// reads) OR the cycle has no TS/JS member (§5: other languages' import edges are runtime by
    /// definition, label ABSENT not Unknown). Additive; the `None` case reaches the JSON leaf as
    /// an omitted field, byte-identical to before. NEVER classify from a `None` (RULE #2).
    pub type_only: Option<CycleTypeOnly>,
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

// ── Boundary links freshness (ACR-6) ─────────────────────────────

/// Freshness summary for `boundary_interaction_links` table.
///
/// Used by `BOUNDARY_LINKS_SUMMARY` signal to report freshness state.
/// This is the first signal backed by a freshness-tracked L2 table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBoundaryLinksFreshness {
    /// Total number of links in the snapshot.
    pub total: u64,
    /// Links with `freshness_state = 'current'`.
    pub current: u64,
    /// Links with `freshness_state = 'impacted'`.
    pub impacted: u64,
    /// Links with `freshness_state = 'unknown'`.
    pub unknown: u64,
    /// Earliest `freshness_updated_at` among impacted rows, if any.
    /// ISO-8601 timestamp as string.
    pub earliest_impacted_at: Option<String>,
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

// ── Module discovery summary ─────────────────────────────────────

/// Summary of discovered module candidates for a snapshot.
///
/// This is module-derived data from the `module_candidates` table,
/// NOT raw snapshot totals. When this exists, the MODULE_SUMMARY
/// signal should include module evidence and NOT emit
/// `MODULE_DATA_UNAVAILABLE`.
///
/// `module_kind` breakdown uses the three-tier vocabulary:
/// - `declared`: manifest-backed (package.json, Cargo.toml, etc.)
/// - `operational`: surface-promoted (CLI, service, web app)
/// - `inferred`: structure/build-system derived
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentModuleSummary {
    /// Total count of discovered module candidates.
    pub discovered_module_count: u64,
    /// Count of declared modules (manifest-backed).
    pub declared_count: u64,
    /// Count of operational modules (surface-promoted).
    pub operational_count: u64,
    /// Count of inferred modules (structure-derived).
    pub inferred_count: u64,
}

/// One discovered module with its owned-file count.
///
/// ORIENT-DENSITY-1: the per-module size breakdown that lets `orient` LEAD
/// with the NAMED structure ("modules: core, http, event, …") instead of a
/// bare count. `path` is the module's `canonical_root_path`; `file_count` is
/// the number of files it owns in this snapshot. Both are Layer-1 extracted
/// facts (module discovery), the SAME `module_candidates` /
/// `module_file_ownership` surface `get_module_summary` already reads — this
/// projection just carries the names + sizes the count alone discards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentModuleSize {
    /// The module's canonical root path (e.g. `src/http`).
    pub path: String,
    /// Number of files this module owns in the snapshot.
    pub file_count: u64,
    /// ORIENT-SEGMENT-2 §2.2: the module's DECLARED name (`module_candidates
    /// .display_name`, e.g. `@amodx/plugins`, `Django`), when the detector recorded
    /// one. `None` for an inferred/directory module — the row then renders by path.
    /// Carried PER ROW (not a path-keyed side map) because two modules can share a
    /// `canonical_root_path` — django declares TWO `Django` modules BOTH rooted at
    /// `.` (a root `pyproject.toml` AND a root `package.json`), distinguishable only
    /// by their manifest.
    pub name: Option<String>,
    /// ORIENT-SEGMENT-2 §2.2: the owning manifest filename (`pyproject.toml`,
    /// `package.json`, `Cargo.toml`, `settings.gradle`), derived from the
    /// `module_key` source prefix. `None` for an inferred / directory module (no
    /// manifest) — honest, never guessed.
    pub manifest: Option<String>,
}

/// One leaf directory that owns files — a row of the directory TOPOLOGY
/// (MODULE-MODEL-1 D2(i)).
///
/// `path` is the directory's `qualified_name` (a `nodes` kind=MODULE node);
/// `file_count` is its OWNS-edge count. This is a Layer-0/1 EXTRACTED fact —
/// the SAME physical per-directory facts `stats`' `compute_module_stats`
/// enumerates — and is DISTINCT from the declared/inferred `module_candidates`
/// surface (`AgentModuleSize`). The `orient` headline folds these into logical
/// package groups via `package_groups::rollup_package_groups`; the two notions
/// are then separately labelled, never collapsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDirectoryGroup {
    /// The directory's repo-relative path (e.g. `src/main/java/org/app/owner`).
    pub path: String,
    /// Number of files this directory directly owns in the snapshot.
    pub file_count: u64,
}

// ── Complexity measurement ──────────────────────────────────────

/// A symbol with high cyclomatic complexity.
///
/// Used by the `HIGH_COMPLEXITY` signal to surface complex code
/// that may benefit from refactoring or additional testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentComplexityMeasurement {
    /// The symbol's stable key.
    pub stable_key: String,
    /// Symbol name (function/method name).
    pub symbol_name: String,
    /// Owning file path (if resolvable).
    pub file_path: Option<String>,
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the symbol's `nodes.line_start` (same row as
    /// `file_path`), for the `path:line` anchor. `None` when unresolved/no stored line.
    pub line: Option<u64>,
    /// The cyclomatic complexity value.
    pub complexity: u64,
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
///
///   - `InFlight`: a background enrichment pass is QUEUED or
///     RUNNING for this repo's snapshot RIGHT NOW. This is NOT a
///     persisted-storage state (the storage adapter never
///     produces it) — it is injected by the daemon, which is the
///     only layer that knows a pass is in flight (ORIENT-FACT-
///     COHERENCE-1, operator ruling D-then-B, 2026-08-31). It
///     exists so orient/check/reliability never hand the reader
///     the stale "run `rmap enrich`" CTA / "phase did not run"
///     line while the pass that would change those figures is
///     already running. Resolution figures may still rise; the
///     honest reader action is to re-run when it completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentState {
    Ran,
    NotApplicable,
    NotRun,
    InFlight,
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
    /// Unresolved CALLS with the known-external subset EXCLUDED (`unresolved_calls`
    /// minus `unresolved_calls_external`). RELIABILITY-REFRAME-1: the reader's in-scope
    /// resolution rate is `resolved_calls / (resolved_calls + this)`. review-3 §2: this
    /// is "in-scope OR UNCLASSIFIED", NOT known-internal — it still includes `unknown`
    /// classifications; `unresolved_calls_unknown` below is that unclassified portion.
    /// The storage adapter projects both from the trust report (already computed by the
    /// Variant-A reweighting); the agent layer must NOT recompute the split from the
    /// classification axis.
    pub unresolved_calls_internal_like: u64,
    /// RELIABILITY-REFRAME-1 (review-3 §2): the UNCLASSIFIED (`unknown`) portion of
    /// `unresolved_calls_internal_like`, so `check` can fire the conservative-rate
    /// caveat from the SAME shared helper `trust`/`orient` use.
    pub unresolved_calls_unknown: u64,
    /// RELIABILITY-REFRAME-1 (review-3 §1): the top named EXTERNAL receiver targets
    /// (the trust report's `top_external_types` — external-FILTERED then truncated),
    /// so `check` renders the SAME named coverage map as `orient`/`trust`, not an
    /// `external=0` placeholder. Empty when enrichment surfaced no external receivers.
    pub external_targets: Vec<crate::reliability::ExternalTarget>,
    pub call_graph_reliability: AgentReliabilityAxis,
    pub dead_code_reliability: AgentReliabilityAxis,
    pub enrichment_state: EnrichmentState,
    pub enrichment_eligible: u64,
    pub enrichment_enriched: u64,
}

// ── Focus resolution DTOs ────────────────────────────────────────

/// What kind of graph entity a focus candidate resolved to.
///
/// Used by `resolve_stable_key_focus` to classify the matched
/// node. The orient dispatcher routes to different pipelines
/// (file vs path-area) based on this kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFocusKind {
    File,
    Module,
    Symbol,
}

/// A candidate entity returned by stable-key focus resolution.
///
/// `stable_key` is the graph-node stable key that matched.
/// `kind` classifies the node. `file` is the repo-relative file
/// path (via the files table join), if the node has a file
/// association.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFocusCandidate {
    pub stable_key: String,
    pub kind: AgentFocusKind,
    pub file: Option<String>,
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the candidate's `nodes.line_start` (same SQLite
    /// row as `file`), for the `path:line` anchor on ambiguous matches. `None` on the
    /// LiveGraph-served path (its `file` is key-derived — no same-source line), never a
    /// mixed-source pair. The focus-resolution cert compares only {key,kind,file}, so
    /// this field does not participate in the LiveGraph↔SQLite parity verdict.
    pub line: Option<u64>,
}

/// Result of resolving a path-based focus string against the
/// snapshot's file and module graph.
///
/// The dispatcher checks these flags in order:
///   1. `has_exact_file` → file pipeline
///   2. `has_content_under_prefix` → path-area pipeline
///   3. neither → fall through to stable-key resolution
///
/// `module_stable_key` is populated when a MODULE node exists
/// whose `qualified_name` matches the path prefix exactly. It is
/// `None` when there is content under the prefix but no MODULE
/// node (e.g. a subdirectory that is not a module root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPathResolution {
    pub has_exact_file: bool,
    /// When `has_exact_file` is true, the stable key of the FILE
    /// node. `None` if the file exists but the resolver could not
    /// produce a key (defensive — should not happen in practice).
    pub file_stable_key: Option<String>,
    pub has_content_under_prefix: bool,
    pub module_stable_key: Option<String>,
}

// ── Symbol context (Rust-45) ────────────────────────────────────

/// Context for a resolved SYMBOL node: owning file, owning
/// module (via OWNS edge), name, qualified name, subtype, and
/// line_start. The owning module is the SINGLE source of truth
/// for which module this symbol belongs to — downstream
/// boundary/gate/cycle code reads from this context and does not
/// rediscover module ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSymbolContext {
    pub file_path: Option<String>,
    pub module_path: Option<String>,
    pub module_stable_key: Option<String>,
    pub name: String,
    pub qualified_name: Option<String>,
    pub subtype: Option<String>,
    pub line_start: Option<u64>,
}

// ── Caller/callee rows (Rust-45) ────────────────────────────────

/// One caller row enriched with module ownership. Used by the
/// symbol pipeline's CALLERS_SUMMARY aggregator to group callers
/// by module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCallerRow {
    pub stable_key: String,
    pub name: String,
    pub file: Option<String>,
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the caller's `nodes.line_start` (same row as
    /// `file`), for the `path:line` anchor. `None` when the node has no stored line.
    pub line: Option<u64>,
    pub module_path: Option<String>,
    pub module_stable_key: Option<String>,
}

/// One callee row enriched with module ownership. Symmetric with
/// `AgentCallerRow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCalleeRow {
    pub stable_key: String,
    pub name: String,
    pub file: Option<String>,
    /// ANCHORS-EVERYWHERE-1 (Tier 1): the callee's `nodes.line_start` (same row as
    /// `file`), for the `path:line` anchor. `None` when the node has no stored line.
    pub line: Option<u64>,
    pub module_path: Option<String>,
    pub module_stable_key: Option<String>,
}

// ── Trait ────────────────────────────────────────────────────────

/// A cooperative cancellation checkpoint (DAEMON-CANCEL-3).
///
/// A `&mut` closure the storage adapter calls at bounded intervals inside a heavy
/// Rust loop; returning [`ControlFlow::Break`](std::ops::ControlFlow::Break) tells
/// the loop to abandon its (read-only, so discardable) work. This is the agent-crate
/// spelling of the same `&mut dyn FnMut() -> ControlFlow<()>` shape
/// `repo_graph_algorithms::CancelCheck` uses; it is aliased here so the agent crate
/// stays free of a graph-algorithms dependency (std types only). The daemon builds
/// the concrete closure from its request emitter (`cancel::loop_checkpoint`); a
/// no-op closure (`|| ControlFlow::Continue(())`) reproduces the non-cancellable
/// behavior byte-for-byte.
pub type AgentCancelCheck<'a> = &'a mut dyn FnMut() -> std::ops::ControlFlow<()>;

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
///
/// ## Cooperative cancellation (DAEMON-CANCEL-3)
///
/// A handful of port methods have a `*_cancellable` sibling taking an
/// [`AgentCancelCheck`]. These exist ONLY for the genuinely-heavy read paths the
/// daemon reaches on its transport thread (the module-cycle Tarjan and the
/// complexity FETCH_ALL materialization). The sibling threads a cooperative
/// checkpoint INTO the adapter's heavy Rust loop so a disconnected peer's
/// in-flight orient/explain abandons the work mid-traversal instead of running it
/// to completion with no consumer. Every sibling ships a DEFAULT body that IGNORES
/// the checkpoint and delegates to the non-cancellable method, so test fakes and
/// non-daemon callers are unaffected — only the real `StorageConnection` adapter
/// (and the orient serve decorator) override them. This mirrors the storage crate's
/// own `find_cycles` → `find_cycles_cancellable` pattern (CANCEL-1) one layer up.
pub trait AgentStorageRead {
    /// Look up a repo by its stable `repo_uid`. Returns
    /// `Ok(None)` when the repo is not registered.
    fn get_repo(&self, repo_uid: &str) -> Result<Option<AgentRepo>, AgentStorageError>;

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
    fn get_stale_files(&self, snapshot_uid: &str)
        -> Result<Vec<AgentStaleFile>, AgentStorageError>;

    /// Return module-level dependency cycles for a snapshot.
    /// Canonicalized (each cycle appears once, rotated to its
    /// lexicographically smallest UID).
    fn find_module_cycles(&self, snapshot_uid: &str) -> Result<Vec<AgentCycle>, AgentStorageError>;

    /// Cancellable variant of [`find_module_cycles`](Self::find_module_cycles)
    /// (DAEMON-CANCEL-3). The adapter consults `cancel` inside the Tarjan SCC
    /// traversal and the per-cycle name fan-out; on
    /// [`ControlFlow::Break`](std::ops::ControlFlow::Break) it abandons the work.
    /// DEFAULT: ignore `cancel`, delegate to the non-cancellable method.
    fn find_module_cycles_cancellable(
        &self,
        snapshot_uid: &str,
        _cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.find_module_cycles(snapshot_uid)
    }

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

    /// Per-language indexed-file counts for a snapshot, sorted count-DESC then language-ASC
    /// (DEPS-LIST-REWRITE-1 §2.2 — `deps list` selects the dependency-manifest ecosystem by the
    /// DOMINANT indexed language, which needs counts, not the DISTINCT-language list that
    /// [`compute_repo_summary`](Self::compute_repo_summary) exposes). Sole current caller: the
    /// `deps list` dispatch arm. DEFAULT: a loud error — an adapter that has not implemented this
    /// read must fail honestly, never silently return "no languages" (which would misselect the
    /// ecosystem). The real storage adapter overrides it.
    fn query_file_count_by_language(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<(String, u64)>, AgentStorageError> {
        let _ = snapshot_uid;
        Err(AgentStorageError::new(
            "query_file_count_by_language",
            "not implemented by this storage adapter",
        ))
    }

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

    /// Cancellable variant of [`get_trust_summary`](Self::get_trust_summary)
    /// (DAEMON-CANCEL-3). The adapter threads `cancel` into the trust assembly's
    /// up-to-100_000-row unresolved-sample loop (the demonstrated heavy path `check`
    /// inherits via this method); on [`ControlFlow::Break`](std::ops::ControlFlow::Break)
    /// it abandons the work and returns a "cancelled" storage error. DEFAULT: ignore
    /// `cancel`, delegate to the non-cancellable method — so test fakes and non-daemon
    /// callers are unaffected. (The trust `compute_module_stats` SQL the same path
    /// reaches is opaque to a Rust checkpoint; the daemon runs `check` under
    /// `sqlite3_interrupt` to abort it — the two mechanisms compose, see `check`.)
    fn get_trust_summary_cancellable(
        &self,
        repo_uid: &str,
        snapshot_uid: &str,
        _cancel: AgentCancelCheck<'_>,
    ) -> Result<AgentTrustSummary, AgentStorageError> {
        self.get_trust_summary(repo_uid, snapshot_uid)
    }

    // ── Focus resolution (Rust-44) ──────────────────────────────

    /// Resolve a path-based focus string against the snapshot.
    ///
    /// Checks (1) whether the path names an exact FILE node,
    /// (2) whether any file content exists under the prefix,
    /// (3) whether a MODULE node exists at that path.
    fn resolve_path_focus(
        &self,
        snapshot_uid: &str,
        path: &str,
    ) -> Result<AgentPathResolution, AgentStorageError>;

    /// Resolve a stable-key focus string against the snapshot.
    ///
    /// Returns the matching node (with its kind and file) when
    /// exactly one node has the given `stable_key`. Returns
    /// `Ok(None)` when no match.
    fn resolve_stable_key_focus(
        &self,
        snapshot_uid: &str,
        stable_key: &str,
    ) -> Result<Option<AgentFocusCandidate>, AgentStorageError>;

    /// Return dead nodes scoped to files under a path prefix.
    ///
    /// Same exclusion layers as `find_dead_nodes` (incoming edges,
    /// entrypoint declarations, framework-liveness inferences).
    /// Path matching uses `{prefix}/` with trailing slash to avoid
    /// prefix collisions.
    fn find_dead_nodes_in_path(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError>;

    /// Return dead nodes scoped to a single exact file.
    ///
    /// Same exclusion layers as `find_dead_nodes`.
    fn find_dead_nodes_in_file(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError>;

    /// Structural summary scoped to files under a path prefix.
    fn compute_path_summary(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError>;

    /// Structural summary scoped to a single file.
    fn compute_file_summary(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError>;

    /// Return active boundary declarations where the source module
    /// is under the given path prefix.
    fn find_boundary_declarations_in_path(
        &self,
        repo_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError>;

    /// Return module-level dependency cycles that involve at least
    /// one module under the given path prefix.
    fn find_cycles_involving_path(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError>;

    /// Cancellable variant of
    /// [`find_cycles_involving_path`](Self::find_cycles_involving_path)
    /// (DAEMON-CANCEL-3). The adapter consults `cancel` inside the Tarjan SCC
    /// traversal and the per-cycle prefix-filter fan-out. DEFAULT: ignore `cancel`,
    /// delegate to the non-cancellable method.
    fn find_cycles_involving_path_cancellable(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
        _cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.find_cycles_involving_path(snapshot_uid, path_prefix)
    }

    // ── Symbol-focus methods (Rust-45) ──────────────────────────

    /// Resolve a symbol name to candidate nodes. Returns up to 5
    /// candidates matching `name` with `kind = 'SYMBOL'`, sorted
    /// by `stable_key` ascending.
    fn resolve_symbol_name(
        &self,
        snapshot_uid: &str,
        name: &str,
    ) -> Result<Vec<AgentFocusCandidate>, AgentStorageError>;

    /// Get context for a resolved SYMBOL node: file, module
    /// ownership (via OWNS edges), name, subtype, line_start.
    fn get_symbol_context(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Option<AgentSymbolContext>, AgentStorageError>;

    /// Return direct callers of a symbol (CALLS edges only),
    /// enriched with module ownership.
    fn find_symbol_callers(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Vec<AgentCallerRow>, AgentStorageError>;

    /// Return direct callees of a symbol (CALLS edges only),
    /// enriched with module ownership.
    fn find_symbol_callees(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Vec<AgentCalleeRow>, AgentStorageError>;

    /// Return module-level dependency cycles that involve the
    /// given module (exact qualified_name match, NOT prefix).
    fn find_cycles_involving_module(
        &self,
        snapshot_uid: &str,
        module_qualified_name: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError>;

    /// Cancellable variant of
    /// [`find_cycles_involving_module`](Self::find_cycles_involving_module)
    /// (DAEMON-CANCEL-3). The adapter consults `cancel` inside the Tarjan SCC
    /// traversal and the per-cycle match fan-out. DEFAULT: ignore `cancel`, delegate
    /// to the non-cancellable method.
    fn find_cycles_involving_module_cancellable(
        &self,
        snapshot_uid: &str,
        module_qualified_name: &str,
        _cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.find_cycles_involving_module(snapshot_uid, module_qualified_name)
    }

    // ── Explain-focus methods ───────────────────────────────────

    /// List SYMBOL nodes in a specific file, ordered by line_start
    /// ascending then name ascending.
    fn list_symbols_in_file(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentSymbolEntry>, AgentStorageError>;

    /// List files under a path prefix (or at exact path), ordered
    /// by path ascending. Each entry includes a symbol count and
    /// is_test flag.
    fn list_files_in_path(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentFileEntry>, AgentStorageError>;

    /// Return distinct target file paths imported by a source file
    /// via IMPORTS edges.
    fn find_file_imports(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentImportEntry>, AgentStorageError>;

    // ── Documentation inventory (docs-primary pivot) ────────────────

    /// Discover documentation files from the repo's filesystem.
    ///
    /// Implementation: the storage adapter reads `repo_path` from
    /// the repos table, then calls
    /// `repo_graph_doc_facts::discover_doc_inventory(repo_path, false)`
    /// and projects entries into `AgentDocEntry`.
    ///
    /// Returns an empty vector when the repo path is inaccessible
    /// or the repo has no documentation files. Does NOT return an
    /// error for missing files — docs are optional and their absence
    /// is valid (the orient contract says "works on repos with zero
    /// semantic hints").
    fn get_doc_inventory(&self, repo_uid: &str) -> Result<Vec<AgentDocEntry>, AgentStorageError>;

    // ── Complexity measurements ─────────────────────────────────────

    /// Query symbols with cyclomatic complexity above a threshold.
    ///
    /// Returns the top N symbols (by complexity descending) where
    /// complexity exceeds `min_threshold`. Used by the HIGH_COMPLEXITY
    /// signal to surface code that may need refactoring attention.
    ///
    /// Returns an empty vector when no measurements exist or none
    /// exceed the threshold — this is valid, not an error.
    fn query_high_complexity_symbols(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
        limit: usize,
    ) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError>;

    /// Cancellable variant of
    /// [`query_high_complexity_symbols`](Self::query_high_complexity_symbols)
    /// (DAEMON-CANCEL-3). The `FETCH_ALL` orient call materializes every complexity
    /// row (the SQL `ORDER BY ... LIMIT i64::MAX` plus a Rust collect/threshold
    /// loop); the adapter consults `cancel` inside that collect loop. DEFAULT: ignore
    /// `cancel`, delegate to the non-cancellable method.
    fn query_high_complexity_symbols_cancellable(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
        limit: usize,
        _cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError> {
        self.query_high_complexity_symbols(snapshot_uid, min_threshold, limit)
    }

    /// Check whether any complexity measurements exist for a snapshot.
    ///
    /// Used to determine whether to emit COMPLEXITY_UNAVAILABLE limit.
    /// Returns true if at least one cyclomatic_complexity measurement
    /// exists, false otherwise.
    fn has_complexity_measurements(&self, snapshot_uid: &str) -> Result<bool, AgentStorageError>;

    /// Count symbols with complexity exceeding the threshold.
    ///
    /// Used by HIGH_COMPLEXITY signal to report the true count of
    /// violating symbols (separate from the top-N sample returned
    /// by `query_high_complexity_symbols`).
    fn count_high_complexity_symbols(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
    ) -> Result<u64, AgentStorageError>;

    // ── Module discovery ────────────────────────────────────────────

    /// Query module discovery summary for a snapshot.
    ///
    /// Returns `Ok(Some(summary))` when the snapshot has discovered
    /// module candidates (from the `module_candidates` table).
    /// Returns `Ok(None)` when no module candidates exist — this is
    /// the trigger for fallback behavior and `MODULE_DATA_UNAVAILABLE`.
    ///
    /// The summary includes total count and breakdown by module kind
    /// (declared/operational/inferred).
    fn get_module_summary(
        &self,
        snapshot_uid: &str,
    ) -> Result<Option<AgentModuleSummary>, AgentStorageError>;

    /// List discovered modules with their owned-file counts, ordered
    /// by size descending (then path, then uid — a total, source-
    /// independent order), capped at `limit` rows.
    ///
    /// ORIENT-DENSITY-1: feeds the NAMED structure headline. Returns
    /// only modules that own at least one file in the snapshot. Empty
    /// when module discovery data is unavailable (same condition that
    /// makes `get_module_summary` return `None`). A read of the same
    /// Layer-1 module-discovery surface — no new architectural boundary.
    ///
    /// `limit` is the budget-derived cap (ORIENT-DENSITY-1 §5, review-1
    /// #2): `small`/`medium` request a bounded headline set, `large`/
    /// `--full` request the COMPLETE list (`usize::MAX`) so the `--full`
    /// breakdown is genuinely full. The adapter clamps `usize::MAX` to a
    /// SQLite-safe `LIMIT` (rusqlite binds `i64`); the caller's
    /// `discovered_module_count` still reports the true total, so a
    /// bounded cap never overclaims completeness.
    ///
    /// Default impl returns empty so the many `AgentStorageRead` test
    /// fakes need no stub; the real adapter overrides it.
    fn list_module_sizes(
        &self,
        _snapshot_uid: &str,
        _limit: usize,
    ) -> Result<Vec<AgentModuleSize>, AgentStorageError> {
        Ok(Vec::new())
    }

    /// List leaf directories that own ≥1 file, with their owned-file counts —
    /// the directory TOPOLOGY (`nodes` kind=MODULE ⋈ OWNS) the `orient`
    /// structure headline folds into package groups (MODULE-MODEL-1 D2(i)).
    ///
    /// A Layer-0/1 EXTRACTED fact, DISTINCT from the declared/inferred
    /// `module_candidates` surface (`list_module_sizes` / `get_module_summary`):
    /// it is present whenever files were indexed (the indexer materializes a
    /// MODULE node + OWNS edges per directory), even on repos where
    /// `module_candidates` is empty. Order is by path (a total order); the
    /// caller folds + re-sorts via `rollup_package_groups`. This is the SAME
    /// per-directory set `stats` reads through `compute_module_stats`, so the
    /// two commands' topology numbers cannot diverge.
    ///
    /// Default impl returns empty so the many `AgentStorageRead` test fakes need
    /// no stub; the real adapter overrides it.
    fn list_directory_groups(
        &self,
        _snapshot_uid: &str,
    ) -> Result<Vec<AgentDirectoryGroup>, AgentStorageError> {
        Ok(Vec::new())
    }

    /// List the manifest-declared package boundaries (crate / workspace-package
    /// roots) for a snapshot — the per-toolchain grouping facts the package-group
    /// fold uses to name Rust crates and TS packages instead of raw directory
    /// fragments (MODULE-MODEL-2 §13 D4).
    ///
    /// A Layer-0/1 EXTRACTED fact: reads the ALREADY-STORED `module_candidates`
    /// ⋈ `module_candidate_evidence` surface — `canonical_root_path` +
    /// `source_type` (the toolchain marker: `cargo_toml` → Rust, `package_json` /
    /// `pnpm_workspace_yaml` → TS). NOT `module_kind`, which is provenance
    /// (`declared`/`inferred`), not toolchain. No new scan; no new subsystem.
    ///
    /// Empty when no manifest facts are indexed (e.g. a C/C++ or manifest-less
    /// tree, or the raw-indexer path where `module_candidates` is unpopulated) —
    /// the fold then degrades HONESTLY to directory/JVM grouping (the delivered
    /// shape). Only Rust + TS manifests are surfaced; JVM/Python manifests keep
    /// the directory/JVM heuristic per the ratified D4.
    ///
    /// Default impl returns empty so the many `AgentStorageRead` test fakes need
    /// no stub; the real adapter overrides it.
    fn list_manifest_roots(
        &self,
        _snapshot_uid: &str,
    ) -> Result<Vec<ManifestRoot>, AgentStorageError> {
        Ok(Vec::new())
    }

    // ── Boundary links freshness (ACR-6) ────────────────────────────

    /// Query freshness summary for boundary_interaction_links.
    ///
    /// Returns counts by freshness state for the snapshot. Used by
    /// `BOUNDARY_LINKS_SUMMARY` signal — the first signal backed by
    /// a freshness-tracked L2 table.
    ///
    /// Returns zero counts when the table has no rows for this
    /// snapshot (which is a valid "no links discovered" state, not
    /// an error).
    fn get_boundary_links_freshness(
        &self,
        snapshot_uid: &str,
    ) -> Result<AgentBoundaryLinksFreshness, AgentStorageError>;
}

// ── Explain DTOs ────────────────────────────────────────────────

/// One SYMBOL node entry in a file listing (explain surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSymbolEntry {
    pub stable_key: String,
    pub name: String,
    pub qualified_name: Option<String>,
    pub subtype: Option<String>,
    pub line_start: Option<u64>,
}

/// One file entry under a path prefix (explain surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFileEntry {
    pub path: String,
    pub symbol_count: u64,
    pub is_test: bool,
}

/// One import target file (explain surface).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentImportEntry {
    pub target_file: String,
}

// ── Documentation inventory ─────────────────────────────────────────

/// A documentation file from live filesystem discovery.
///
/// Docs are primary orientation data. This struct comes from
/// `discover_doc_inventory` in the doc-facts crate, projected
/// through the storage adapter. The agent crate does not access
/// the filesystem directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDocEntry {
    /// Path relative to repo root.
    pub path: String,
    /// Document kind: "readme", "architecture", "config", "map".
    pub kind: String,
    /// Whether this is a generated document (e.g., MAP.md from rgistr).
    pub generated: bool,
}
