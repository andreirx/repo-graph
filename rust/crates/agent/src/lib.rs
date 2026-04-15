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
//! Explicit deferrals (see
//! `docs/architecture/agent-orientation-contract.md` and
//! `docs/TECH-DEBT.md`):
//!   - module/path focus (Rust-44)
//!   - symbol focus (Rust-45)
//!   - `check` and `explain` use cases
//!   - CLI wiring + binary rename (Rust-43)
//!   - GATE_* signal emission — blocked on gate.rs relocation.
//!     Rust-42 emits `GATE_UNAVAILABLE` limit in every response.
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
		BoundaryViolationEvidence, BoundaryViolationsEvidence, CycleEvidence,
		DeadCodeEvidence, DeadSymbolEvidence, ImportCyclesEvidence,
		ModuleSummaryEvidence, Severity, Signal, SignalCategory, SignalCode,
		SignalEvidence, SnapshotInfoEvidence, TrustLowResolutionEvidence,
		TrustNoEnrichmentEvidence, TrustStaleSnapshotEvidence,
	},
	source::SourceRef,
};
pub use errors::{AgentStorageError, OrientError};
pub use orient::orient;
pub use storage_port::{
	AgentBoundaryDeclaration, AgentCycle, AgentDeadNode, AgentImportEdge,
	AgentRepo, AgentRepoSummary, AgentSnapshot, AgentStaleFile,
	AgentStorageRead, AgentTrustSummary,
};
