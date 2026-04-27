//! repo-graph-gate — gate policy crate.
//!
//! Obligation evaluation, waiver overlay, mode reduction, and
//! gate outcome reporting. Single source of truth for gate
//! product policy.
//!
//! Relocated from `rgr/src/gate.rs` in Rust-43A so that both
//! the `rmap gate` CLI command and the
//! `repo-graph-agent` orient aggregator can share the same
//! gate implementation. See
//! `docs/architecture/agent-orientation-contract.md` and
//! `docs/TECH-DEBT.md` for the rationale and history.
//!
//! ── Two layers ─────────────────────────────────────────────
//!
//!   * `compute(input: GateInput) -> GateReport` — pure. No
//!     I/O. No storage. No clocks beyond the caller-supplied
//!     `now` on the input. This is the policy layer proper.
//!
//!   * `assemble(storage: &impl GateStorageRead, ...)` — thin
//!     orchestration. Fetches all per-obligation evidence
//!     through the storage port, builds a `GateInput`, and
//!     delegates to `compute`. Returns `GateReport` on
//!     success, `GateError` on port failure.
//!
//! Test policy: pure compute is exercised by unit tests in
//! `compute.rs`. The storage-backed path is exercised by unit
//! tests against an in-module `FakeStorage` in `assemble.rs`
//! and by storage-side integration tests in
//! `rust/crates/storage/tests/gate_impl.rs`.
//!
//! ── Waiver semantics (preserved from Rust-25) ──────────────
//!
//! Non-PASS computed verdicts with a matching active waiver
//! become `effective_verdict = WAIVED`. PASS obligations stay
//! PASS regardless of waiver presence — no policy exception
//! occurred. This diverges intentionally from the TS prototype;
//! the divergence is preserved during relocation.
//!
//! ── Supported methods ─────────────────────────────────────
//!
//! `arch_violations`, `coverage_threshold`,
//! `complexity_threshold`, `hotspot_threshold`,
//! `module_violations`. All others return UNSUPPORTED.
//! Extending the method set is a two-location change: add a
//! `MethodEvidence` variant in `compute.rs`, extend the
//! assembly dispatch in `assemble.rs`.

pub mod assemble;
pub mod compute;
pub mod errors;
pub mod storage_port;
pub mod types;

// ── Public surface ─────────────────────────────────────────────

pub use assemble::{assemble, assemble_from_requirements};
pub use compute::{compute, GateInput, MethodEvidence, ObligationKey, PolicyMeasurement};
pub use errors::{GateError, GateStorageError};
pub use storage_port::GateStorageRead;
pub use types::{
	EffectiveVerdict, GateAssessmentState, GateAssessmentVerdict,
	GateBoundaryDeclaration, GateCounts, GateImportEdge, GateInference,
	GateMeasurement, GateMode, GateModuleViolationEvidence, GateObligation,
	GateOutcome, GateQualityAssessmentEvaluation, GateQualityAssessmentFact,
	GateQualityCounts, GateQualityPolicyKind, GateQualityPolicySeverity,
	GateReport, GateRequirement, GateWaiver, ObligationEvaluation, Verdict,
	WaiverBasis,
};
