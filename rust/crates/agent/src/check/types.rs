//! Check condition DTOs and verdict types.
//!
//! These types are the data vocabulary of the two-phase check
//! reducer. Phase 1 (`evaluate_conditions`) produces a
//! `Vec<ConditionResult>` from a `CheckInput`. Phase 2
//! (`reduce_verdict`) collapses those results into a single
//! `CheckVerdict`. Neither phase touches storage or I/O.
//!
//! Serialization (serde derives) is deliberately omitted here.
//! The use-case layer (step 3) adds Serialize when the envelope
//! shape is finalized.

use crate::storage_port::{AgentReliabilityLevel, EnrichmentState};

// ── Verdicts ────────────────────────────────────────────────────

/// The three possible check verdicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckVerdict {
	Pass,
	Fail,
	Incomplete,
}

/// Status of a single condition within a check evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionStatus {
	Pass,
	Fail,
	Incomplete,
}

// ── Condition codes ─────────────────────────────────────────────

/// Enumeration of all condition codes that check evaluates.
/// Each code has a stable string representation for the JSON
/// contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionCode {
	SnapshotExists,
	IndexNotEmpty,
	StaleFiles,
	CallGraphReliability,
	DeadCodeReliability,
	EnrichmentState,
	GateStatus,
}

impl ConditionCode {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::SnapshotExists => "SNAPSHOT_EXISTS",
			Self::IndexNotEmpty => "INDEX_NOT_EMPTY",
			Self::StaleFiles => "STALE_FILES",
			Self::CallGraphReliability => "CALL_GRAPH_RELIABILITY",
			Self::DeadCodeReliability => "DEAD_CODE_RELIABILITY",
			Self::EnrichmentState => "ENRICHMENT_STATE",
			Self::GateStatus => "GATE_STATUS",
		}
	}
}

// ── Condition result ────────────────────────────────────────────

/// One evaluated condition result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionResult {
	pub code: ConditionCode,
	pub status: ConditionStatus,
	pub summary: String,
}

// ── Input ───────────────────────────────────────────────────────

/// Input data for check evaluation. All fields are pre-fetched
/// by the use-case layer and handed to the pure reducer.
/// No storage, no I/O.
#[derive(Debug, Clone)]
pub struct CheckInput {
	/// Whether a READY snapshot exists.
	pub snapshot_exists: bool,
	/// Total files in the snapshot. 0 if no snapshot.
	pub files_total: u64,
	/// Number of stale files. 0 if none or no snapshot.
	pub stale_file_count: u64,
	/// Trust call-graph reliability level. None if no snapshot.
	pub call_graph_reliability: Option<AgentReliabilityLevel>,
	/// Trust dead-code reliability level. None if no snapshot.
	pub dead_code_reliability: Option<AgentReliabilityLevel>,
	/// Enrichment execution state. None if no snapshot.
	pub enrichment_state: Option<EnrichmentState>,
	/// Gate outcome projection. None if no snapshot or gate not
	/// evaluated.
	pub gate_outcome: Option<GateOutcomeForCheck>,
}

// ── Gate outcome projection ─────────────────────────────────────

/// Minimal gate outcome projection for the check reducer.
/// Does not carry the full GateReport — only what check needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOutcomeForCheck {
	/// Gate evaluated, all pass/waived.
	Pass,
	/// Gate evaluated, at least one obligation failed.
	Fail,
	/// Gate evaluated, missing evidence or unsupported methods.
	Incomplete,
	/// No active requirements — no policy to evaluate.
	NotConfigured,
}

// ── Result ──────────────────────────────────────────────────────

/// The full check result produced by the two-phase reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
	pub verdict: CheckVerdict,
	pub conditions: Vec<ConditionResult>,
}
