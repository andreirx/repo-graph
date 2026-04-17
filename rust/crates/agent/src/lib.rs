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
//! The only external dependencies are `serde` and `serde_json`.

pub mod aggregators;
pub mod confidence;
pub mod dto;
pub mod errors;
pub mod orient;
pub mod ranking;
pub mod storage_port;

// ── Public surface (locked at Rust-42) ────────────────────────

pub use dto::{
	budget::Budget,
	envelope::{
		Confidence, Focus, FocusCandidate, FocusFailureReason, NextAction,
		NextKind, OrientResult, ResolvedKind, ORIENT_COMMAND, ORIENT_SCHEMA,
	},
	limit::{Limit, LimitCode},
	signal::{
		BoundaryViolationEvidence, BoundaryViolationsEvidence,
		CallersSummaryEvidence, CalleesSummaryEvidence, CycleEvidence,
		DeadCodeEvidence, DeadSymbolEvidence, ImportCyclesEvidence,
		ModuleCountEvidence, ModuleSummaryEvidence, Severity, Signal,
		SignalCategory, SignalCode, SignalEvidence, SignalScope,
		SnapshotInfoEvidence, TrustLowResolutionEvidence,
		TrustNoEnrichmentEvidence, TrustStaleSnapshotEvidence,
	},
	source::SourceRef,
};
pub use errors::{AgentStorageError, OrientError};
pub use orient::orient;
pub use storage_port::{
	AgentBoundaryDeclaration, AgentCalleeRow, AgentCallerRow, AgentCycle,
	AgentDeadNode, AgentFocusCandidate, AgentFocusKind, AgentImportEdge,
	AgentPathResolution, AgentReliabilityAxis, AgentReliabilityLevel,
	AgentRepo, AgentRepoSummary, AgentSnapshot, AgentStaleFile,
	AgentStorageRead, AgentSymbolContext, AgentTrustSummary, EnrichmentState,
};
