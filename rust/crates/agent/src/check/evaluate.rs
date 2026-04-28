//! Phase 1: Condition evaluation.
//!
//! A pure function that takes `CheckInput` and produces a
//! `Vec<ConditionResult>`, one per condition code. When no
//! snapshot exists, only `SNAPSHOT_EXISTS` is evaluated (with
//! status Incomplete); conditions 2-7 are omitted entirely.

use crate::storage_port::{AgentReliabilityLevel, EnrichmentState};

use super::types::{
	CheckInput, ConditionCode, ConditionResult, ConditionStatus,
	GateOutcomeForCheck,
};

/// Evaluate all applicable conditions from the pre-fetched input.
///
/// Returns one `ConditionResult` per evaluated condition code.
/// When `input.snapshot_exists` is false, only the
/// `SNAPSHOT_EXISTS` condition is returned.
pub fn evaluate_conditions(input: &CheckInput) -> Vec<ConditionResult> {
	let mut results = Vec::new();

	// ── 1. SNAPSHOT_EXISTS ───────────────────────────────────
	if input.snapshot_exists {
		results.push(ConditionResult {
			code: ConditionCode::SnapshotExists,
			status: ConditionStatus::Pass,
			summary: "READY snapshot available.".to_string(),
		});
	} else {
		results.push(ConditionResult {
			code: ConditionCode::SnapshotExists,
			status: ConditionStatus::Incomplete,
			summary: "No READY snapshot. Index the repo first.".to_string(),
		});
		// No snapshot → conditions 2-7 are not evaluated.
		return results;
	}

	// ── 2. INDEX_NOT_EMPTY ──────────────────────────────────
	if input.files_total > 0 {
		results.push(ConditionResult {
			code: ConditionCode::IndexNotEmpty,
			status: ConditionStatus::Pass,
			summary: format!("{} files indexed.", input.files_total),
		});
	} else {
		results.push(ConditionResult {
			code: ConditionCode::IndexNotEmpty,
			status: ConditionStatus::Incomplete,
			summary: "Snapshot has zero indexed files.".to_string(),
		});
	}

	// ── 3. STALE_FILES ──────────────────────────────────────
	if input.stale_file_count == 0 {
		results.push(ConditionResult {
			code: ConditionCode::StaleFiles,
			status: ConditionStatus::Pass,
			summary: "No stale files.".to_string(),
		});
	} else {
		results.push(ConditionResult {
			code: ConditionCode::StaleFiles,
			status: ConditionStatus::Fail,
			summary: format!(
				"{} stale files recorded in storage.",
				input.stale_file_count
			),
		});
	}

	// ── 4. CALL_GRAPH_RELIABILITY ───────────────────────────
	//
	// **POLICY NOTE:** MEDIUM -> pass is a check-specific
	// interpretation. The trust crate defines MEDIUM as
	// 50-85% resolution. Check treats this as "safe enough to
	// act on." This is NOT inherited from the trust contract.
	match input.call_graph_reliability {
		Some(AgentReliabilityLevel::High) => {
			results.push(ConditionResult {
				code: ConditionCode::CallGraphReliability,
				status: ConditionStatus::Pass,
				summary: "Call graph reliability is HIGH.".to_string(),
			});
		}
		Some(AgentReliabilityLevel::Medium) => {
			results.push(ConditionResult {
				code: ConditionCode::CallGraphReliability,
				status: ConditionStatus::Pass,
				summary: "Call graph reliability is MEDIUM (advisory).".to_string(),
			});
		}
		Some(AgentReliabilityLevel::Low) => {
			results.push(ConditionResult {
				code: ConditionCode::CallGraphReliability,
				status: ConditionStatus::Fail,
				summary: "Call graph reliability is LOW.".to_string(),
			});
		}
		None => {
			results.push(ConditionResult {
				code: ConditionCode::CallGraphReliability,
				status: ConditionStatus::Incomplete,
				summary: "Call graph reliability data unavailable.".to_string(),
			});
		}
	}

	// ── 5. ENRICHMENT_STATE ─────────────────────────────────
	match input.enrichment_state {
		Some(EnrichmentState::Ran) => {
			results.push(ConditionResult {
				code: ConditionCode::EnrichmentState,
				status: ConditionStatus::Pass,
				summary: "Enrichment phase executed.".to_string(),
			});
		}
		Some(EnrichmentState::NotApplicable) => {
			results.push(ConditionResult {
				code: ConditionCode::EnrichmentState,
				status: ConditionStatus::Pass,
				summary: "No eligible edges for enrichment.".to_string(),
			});
		}
		Some(EnrichmentState::NotRun) => {
			results.push(ConditionResult {
				code: ConditionCode::EnrichmentState,
				status: ConditionStatus::Fail,
				summary: "Enrichment phase did not run.".to_string(),
			});
		}
		None => {
			results.push(ConditionResult {
				code: ConditionCode::EnrichmentState,
				status: ConditionStatus::Incomplete,
				summary: "Enrichment state data unavailable.".to_string(),
			});
		}
	}

	// ── 7. GATE_STATUS ──────────────────────────────────────
	//
	// **POLICY NOTE:** NotConfigured -> pass is a check-specific
	// interpretation. "No policy = no violation." If the product
	// later wants policy-coverage as a concern, it would be a
	// separate condition code.
	match input.gate_outcome {
		Some(GateOutcomeForCheck::Pass) => {
			results.push(ConditionResult {
				code: ConditionCode::GateStatus,
				status: ConditionStatus::Pass,
				summary: "Gate passes.".to_string(),
			});
		}
		Some(GateOutcomeForCheck::Fail) => {
			results.push(ConditionResult {
				code: ConditionCode::GateStatus,
				status: ConditionStatus::Fail,
				summary: "Gate fails.".to_string(),
			});
		}
		Some(GateOutcomeForCheck::Incomplete) => {
			results.push(ConditionResult {
				code: ConditionCode::GateStatus,
				status: ConditionStatus::Incomplete,
				summary: "Gate incomplete: missing evidence.".to_string(),
			});
		}
		Some(GateOutcomeForCheck::NotConfigured) => {
			results.push(ConditionResult {
				code: ConditionCode::GateStatus,
				status: ConditionStatus::Pass,
				summary: "No gate policy configured.".to_string(),
			});
		}
		None => {
			results.push(ConditionResult {
				code: ConditionCode::GateStatus,
				status: ConditionStatus::Incomplete,
				summary: "Gate status data unavailable.".to_string(),
			});
		}
	}

	results
}
