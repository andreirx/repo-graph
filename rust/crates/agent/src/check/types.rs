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

use crate::dto::index_drift::IndexDrift;
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
    /// INDEX-BASIS-1: parse-status condition, honestly named — "N files could not
    /// be parsed". Replaces the misleading `StaleFiles` (which measured parse
    /// status, never working-tree drift).
    UnparsedFiles,
    /// INDEX-BASIS-1: DEPRECATED alias of `UnparsedFiles`, kept emitted for one
    /// release so any consumer keyed on the old `STALE_FILES` code keeps working.
    /// Same status/data as `UnparsedFiles`; suppressed from human output.
    StaleFiles,
    /// INDEX-BASIS-1: working-tree drift since the index basis commit. Informational
    /// → `Incomplete` when drifted/unknown, never `Fail` by itself.
    IndexDrift,
    CallGraphReliability,
    EnrichmentState,
    GateStatus,
}

impl ConditionCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SnapshotExists => "SNAPSHOT_EXISTS",
            Self::IndexNotEmpty => "INDEX_NOT_EMPTY",
            Self::UnparsedFiles => "UNPARSED_FILES",
            Self::StaleFiles => "STALE_FILES",
            Self::IndexDrift => "INDEX_DRIFT",
            Self::CallGraphReliability => "CALL_GRAPH_RELIABILITY",
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
    /// RELIABILITY-REFRAME-1: resolved CALLS, for the reader-frame in-scope rate
    /// in the CALL_GRAPH_RELIABILITY summary. 0 when no snapshot. Projected from
    /// the trust summary already read at the construction site (no new read).
    pub resolved_calls: u64,
    /// RELIABILITY-REFRAME-1: unresolved CALLS with the known-external subset
    /// EXCLUDED — the in-scope-rate denominator with `resolved_calls`. review-3 §2:
    /// "in-scope OR unclassified", not known-internal. 0 when no snapshot.
    pub unresolved_calls_internal_like: u64,
    /// RELIABILITY-REFRAME-1 (review-3 §1): ALL unresolved CALLS (resolved-or-not),
    /// the external-SHARE denominator with `resolved_calls` (`total_calls`). Lets
    /// `check` render the external share, not an `external=0` placeholder. 0 when no
    /// snapshot.
    pub unresolved_calls: u64,
    /// RELIABILITY-REFRAME-1 (review-3 §2): the UNCLASSIFIED (`unknown`) portion of
    /// `unresolved_calls_internal_like`, for the conservative-rate caveat. 0 when no
    /// snapshot.
    pub unresolved_calls_unknown: u64,
    /// RELIABILITY-REFRAME-1 (review-3 §1): the top named EXTERNAL receiver targets
    /// for the reader-frame coverage map, so `check` carries the FULL projection
    /// (external share + named targets + bases). Empty when no snapshot / none surfaced.
    pub external_targets: Vec<crate::reliability::ExternalTarget>,
    /// Enrichment execution state. None if no snapshot.
    pub enrichment_state: Option<EnrichmentState>,
    /// Gate outcome projection. None if no snapshot or gate not
    /// evaluated.
    pub gate_outcome: Option<GateOutcomeForCheck>,
    /// INDEX-BASIS-1: query-time working-tree drift since the index basis commit,
    /// computed by the daemon (git + storage) and handed in as pre-fetched data —
    /// the pure reducer performs no I/O. `None` when the caller did not compute
    /// drift (e.g. the simple `run_check` entry / unit tests); the `INDEX_DRIFT`
    /// condition is then OMITTED rather than fabricated. The daemon always supplies
    /// `Some`, so production `check` always evaluates it.
    pub index_drift: Option<IndexDrift>,
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
