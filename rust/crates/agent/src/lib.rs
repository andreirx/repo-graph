//! repo-graph-agent — use-case crate for the agent orientation
//! surface (`orient`, `check`, `explain`).
//!
//! Rust-42 scope:
//!   - crate skeleton + dependency-inverted storage port
//!   - DTO contract (`rgr.agent.v1`)
//!   - repo-level `orient(storage, repo_uid, None, budget)`
//!   - ranking, budget truncation, confidence, limits
//!   - typed signal evidence (no `serde_json::Value` escape hatch)
//!
//! Rust-43A added (gate.rs relocation):
//!   - Dependency on `repo-graph-gate`. The orient pipeline
//!     emits `GATE_PASS` / `GATE_FAIL` / `GATE_INCOMPLETE`
//!     signals by calling `repo_graph_gate::assemble_from_requirements`.
//!   - New `LimitCode::GateNotConfigured` replaces the
//!     Rust-42-era `GateUnavailable`. The limit fires when
//!     a repo has no active requirement declarations.
//!   - `orient()` trait bound widened to
//!     `S: AgentStorageRead + GateStorageRead` so a single
//!     storage handle satisfies both ports.
//!   - `orient()` takes `now: &str` as its final parameter
//!     (P2 fix). The agent crate is clock-free: callers must
//!     supply a wall-clock ISO 8601 timestamp. Used for waiver
//!     expiry comparison in the gate aggregator. Passing a
//!     far-future or far-past sentinel silently distorts
//!     waiver semantics — do not.
//!
//! Explicit deferrals (see
//! `docs/architecture/agent-orientation-contract.md` and
//! `docs/TECH-DEBT.md`):
//!   - module/path focus (Rust-44)
//!   - symbol focus (Rust-45)
//!   - `check` and `explain` use cases
//!   - CLI wiring + binary rename (Rust-43B / 43C)
//!   - Daemon socket transport
//!
//! ── Architecture (Clean Architecture policy crate) ───────────
//!
//! This crate follows the same pattern as `repo-graph-trust`:
//!
//!   * Policy layer: this crate defines `AgentStorageRead`
//!     (the narrow read port it needs) and the `OrientResult`
//!     DTO surface.
//!   * Adapter layer: `repo-graph-storage` adds this crate as a
//!     dependency and implements `AgentStorageRead` for
//!     `StorageConnection`, mapping `StorageError` into the
//!     agent-owned `AgentStorageError`.
//!
//! This crate does NOT depend on:
//!   - `repo-graph-storage` (adapter)
//!   - `repo-graph-trust` (the storage impl handles trust
//!     projection internally)
//!   - `rusqlite` or any SQL crate
//!   - any indexer or extractor crate
//!
//! External dependencies: `serde`, `serde_json`, and `repo-graph-classification` (the
//! typed `UnresolvedEdgeBasisCode` vocabulary for the exhaustive attribution mapping —
//! an inward, cycle-free edge ratified in ATTRIBUTION-1).

pub mod aggregators;
pub mod attribution;
pub mod check;
pub mod confidence;
pub mod doc_relevance;
pub mod dto;
pub mod errors;
pub mod explain;
pub mod ordering;
pub mod orient;
pub mod package_groups;
pub mod ranking;
pub mod reliability;
pub mod reliability_breakdown;
pub mod storage_port;

// ── Public surface (locked at Rust-42) ────────────────────────

pub use check::{
    check_to_coherent, run_check, run_check_cancellable, CheckInput, CheckResult, CheckVerdict,
    ConditionCode, ConditionResult, ConditionStatus, GateOutcomeForCheck,
};
pub use doc_relevance::{select_relevant_docs, DocEntry, DocFocusContext};
pub use dto::coherent::{
    to_coherent, CoherentOrientResult, OrientLeafLabel, OrientLgDecisions, COHERENT_ORIENT_SCHEMA,
};
pub use dto::{
    budget::Budget,
    envelope::{
        Confidence, DocRelevanceReason, DocumentationSection, Focus, FocusCandidate,
        FocusFailureReason, NextAction, NextKind, OrientResult, RelevantDoc, ResolvedKind,
        CHECK_COMMAND, EXPLAIN_COMMAND, ORIENT_COMMAND, ORIENT_SCHEMA,
    },
    limit::{DegradationInfo, DegradationStatus, Limit, LimitCode},
    signal::{
        BoundaryLinksSummaryEvidence,
        BoundaryViolationEvidence,
        BoundaryViolationsEvidence,
        CalleesSummaryEvidence,
        CallersSummaryEvidence,
        CheckConditionEvidence,
        CheckFailEvidence,
        CheckIncompleteEvidence,
        CheckPassEvidence,
        ComplexSymbolEvidence,
        CycleEvidence,
        // DeadCodeEvidence, DeadSymbolEvidence — removed. Surface withdrawn.
        ExplainBoundaryEvidence,
        ExplainCalleeItem,
        ExplainCalleesEvidence,
        ExplainCallerItem,
        ExplainCallersEvidence,
        ExplainCyclesEvidence,
        ExplainFileItem,
        ExplainFilesEvidence,
        ExplainGateEvidence,
        ExplainGateItem,
        ExplainIdentityEvidence,
        ExplainImportItem,
        ExplainImportsEvidence,
        ExplainMeasurementItem,
        ExplainMeasurementsEvidence,
        ExplainSymbolItem,
        ExplainSymbolsEvidence,
        ExplainTrustEvidence,
        HighComplexityEvidence,
        ImportCyclesEvidence,
        ModuleCountEvidence,
        ModuleKindBreakdown,
        ModuleSummaryEvidence,
        PackageGroupEvidence,
        Severity,
        Signal,
        SignalCategory,
        SignalCode,
        SignalEvidence,
        SignalScope,
        SnapshotInfoEvidence,
        TrustLowResolutionEvidence,
        TrustNoEnrichmentEvidence,
        TrustStaleSnapshotEvidence,
    },
    source::SourceRef,
};
pub use errors::{AgentStorageError, CheckError, ExplainError, OrientError};
pub use explain::{explain_to_coherent, run_explain, run_explain_cancellable, ExplainLgDecisions};
pub use orient::{orient, orient_cancellable};
pub use package_groups::{
    rollup_package_groups, root_manifest_limitation, DirGroup, ManifestKind, ManifestRoot,
    PackageGroup,
};
pub use storage_port::{
    AgentBoundaryDeclaration, AgentBoundaryLinksFreshness, AgentCalleeRow, AgentCallerRow,
    AgentCancelCheck, AgentComplexityMeasurement, AgentCycle, AgentDeadNode, AgentDirectoryGroup,
    AgentDocEntry, AgentFileEntry, AgentFocusCandidate, AgentFocusKind, AgentImportEdge,
    AgentImportEntry, AgentModuleSize, AgentModuleSummary, AgentPathResolution,
    AgentReliabilityAxis, AgentReliabilityLevel, AgentRepo, AgentRepoSummary, AgentSnapshot,
    AgentStaleFile, AgentStorageRead, AgentSymbolContext, AgentSymbolEntry, AgentTrustSummary,
    EnrichmentState,
};
