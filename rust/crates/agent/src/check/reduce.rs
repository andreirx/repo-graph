//! Phase 2: Verdict reduction.
//!
//! Collapses a `Vec<ConditionResult>` into a single
//! `CheckVerdict` using strict precedence:
//!
//!   Incomplete > Fail > Pass
//!
//! If any condition is Incomplete, the verdict is Incomplete.
//! Otherwise, if any condition is Fail, the verdict is Fail.
//! Otherwise, the verdict is Pass.

use super::evaluate::evaluate_conditions;
use super::types::{
	CheckInput, CheckResult, CheckVerdict, ConditionResult,
	ConditionStatus,
};

/// Reduce a slice of condition results into a single verdict.
///
/// Precedence: Incomplete > Fail > Pass.
pub fn reduce_verdict(conditions: &[ConditionResult]) -> CheckVerdict {
	let mut has_incomplete = false;
	let mut has_fail = false;

	for c in conditions {
		match c.status {
			ConditionStatus::Incomplete => {
				has_incomplete = true;
			}
			ConditionStatus::Fail => {
				has_fail = true;
			}
			ConditionStatus::Pass => {}
		}
	}

	if has_incomplete {
		CheckVerdict::Incomplete
	} else if has_fail {
		CheckVerdict::Fail
	} else {
		CheckVerdict::Pass
	}
}

/// Convenience: evaluate conditions and reduce in one call.
pub fn check(input: &CheckInput) -> CheckResult {
	let conditions = evaluate_conditions(input);
	let verdict = reduce_verdict(&conditions);
	CheckResult { verdict, conditions }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use crate::check::types::{
		CheckVerdict, ConditionCode, ConditionStatus,
		GateOutcomeForCheck,
	};
	use crate::storage_port::{AgentReliabilityLevel, EnrichmentState};

	/// Helper: builds a CheckInput where everything is passing.
	fn all_pass_input() -> CheckInput {
		CheckInput {
			snapshot_exists: true,
			files_total: 42,
			stale_file_count: 0,
			call_graph_reliability: Some(AgentReliabilityLevel::High),
			dead_code_reliability: Some(AgentReliabilityLevel::High),
			enrichment_state: Some(EnrichmentState::Ran),
			gate_outcome: Some(GateOutcomeForCheck::Pass),
		}
	}

	// ── 1. all_pass ─────────────────────────────────────────

	#[test]
	fn all_pass() {
		let result = check(&all_pass_input());
		assert_eq!(result.verdict, CheckVerdict::Pass);
		assert!(
			result.conditions.iter().all(|c| c.status == ConditionStatus::Pass),
			"Expected all conditions to pass, got: {:?}",
			result.conditions,
		);
	}

	// ── 2. one_fail_stale_files ─────────────────────────────

	#[test]
	fn one_fail_stale_files() {
		let mut input = all_pass_input();
		input.stale_file_count = 5;
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Fail);
	}

	// ── 3. one_incomplete_no_snapshot ────────────────────────

	#[test]
	fn one_incomplete_no_snapshot() {
		let input = CheckInput {
			snapshot_exists: false,
			files_total: 0,
			stale_file_count: 0,
			call_graph_reliability: None,
			dead_code_reliability: None,
			enrichment_state: None,
			gate_outcome: None,
		};
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Incomplete);
	}

	// ── 4. fail_plus_incomplete ─────────────────────────────

	#[test]
	fn fail_plus_incomplete() {
		let mut input = all_pass_input();
		input.stale_file_count = 2; // Fail
		input.gate_outcome = Some(GateOutcomeForCheck::Incomplete); // Incomplete
		let result = check(&input);
		// Incomplete wins over Fail.
		assert_eq!(result.verdict, CheckVerdict::Incomplete);
	}

	// ── 5. medium_call_graph_with_everything_else_pass ──────

	#[test]
	fn medium_call_graph_with_everything_else_pass() {
		let mut input = all_pass_input();
		input.call_graph_reliability = Some(AgentReliabilityLevel::Medium);
		let result = check(&input);
		// MEDIUM call-graph is advisory → pass.
		assert_eq!(result.verdict, CheckVerdict::Pass);
		let cg = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::CallGraphReliability)
			.unwrap();
		assert_eq!(cg.status, ConditionStatus::Pass);
		assert!(
			cg.summary.contains("MEDIUM"),
			"Expected summary to mention MEDIUM, got: {}",
			cg.summary,
		);
	}

	// ── 6. gate_not_configured_with_everything_else_pass ────

	#[test]
	fn gate_not_configured_with_everything_else_pass() {
		let mut input = all_pass_input();
		input.gate_outcome = Some(GateOutcomeForCheck::NotConfigured);
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Pass);
	}

	// ── 7. dead_code_medium_is_fail ─────────────────────────

	#[test]
	fn dead_code_medium_is_fail() {
		let mut input = all_pass_input();
		input.dead_code_reliability = Some(AgentReliabilityLevel::Medium);
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Fail);
		let dc = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::DeadCodeReliability)
			.unwrap();
		assert_eq!(dc.status, ConditionStatus::Fail);
	}

	// ── 8. dead_code_low_is_fail ────────────────────────────

	#[test]
	fn dead_code_low_is_fail() {
		let mut input = all_pass_input();
		input.dead_code_reliability = Some(AgentReliabilityLevel::Low);
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Fail);
		let dc = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::DeadCodeReliability)
			.unwrap();
		assert_eq!(dc.status, ConditionStatus::Fail);
	}

	// ── 9. enrichment_not_run ───────────────────────────────

	#[test]
	fn enrichment_not_run() {
		let mut input = all_pass_input();
		input.enrichment_state = Some(EnrichmentState::NotRun);
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Fail);
	}

	// ── 10. stale_files_present ─────────────────────────────

	#[test]
	fn stale_files_present() {
		let mut input = all_pass_input();
		input.stale_file_count = 3;
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Fail);
		let sf = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::StaleFiles)
			.unwrap();
		assert_eq!(sf.status, ConditionStatus::Fail);
		assert!(
			sf.summary.contains("3"),
			"Expected summary to mention count 3, got: {}",
			sf.summary,
		);
	}

	// ── 11. empty_snapshot ──────────────────────────────────

	#[test]
	fn empty_snapshot() {
		let mut input = all_pass_input();
		input.files_total = 0;
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Incomplete);
		let idx = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::IndexNotEmpty)
			.unwrap();
		assert_eq!(idx.status, ConditionStatus::Incomplete);
	}

	// ── 12. enrichment_not_applicable_is_pass ───────────────

	#[test]
	fn enrichment_not_applicable_is_pass() {
		let mut input = all_pass_input();
		input.enrichment_state = Some(EnrichmentState::NotApplicable);
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Pass);
		let en = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::EnrichmentState)
			.unwrap();
		assert_eq!(en.status, ConditionStatus::Pass);
	}

	// ── 13. enrichment_ran_is_pass ──────────────────────────

	#[test]
	fn enrichment_ran_is_pass() {
		let mut input = all_pass_input();
		input.enrichment_state = Some(EnrichmentState::Ran);
		let result = check(&input);
		let en = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::EnrichmentState)
			.unwrap();
		assert_eq!(en.status, ConditionStatus::Pass);
		assert_eq!(result.verdict, CheckVerdict::Pass);
	}

	// ── 14. gate_fail_causes_check_fail ─────────────────────

	#[test]
	fn gate_fail_causes_check_fail() {
		let mut input = all_pass_input();
		input.gate_outcome = Some(GateOutcomeForCheck::Fail);
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Fail);
		let gs = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::GateStatus)
			.unwrap();
		assert_eq!(gs.status, ConditionStatus::Fail);
	}

	// ── 15. gate_incomplete_causes_check_incomplete ─────────

	#[test]
	fn gate_incomplete_causes_check_incomplete() {
		let mut input = all_pass_input();
		input.gate_outcome = Some(GateOutcomeForCheck::Incomplete);
		let result = check(&input);
		assert_eq!(result.verdict, CheckVerdict::Incomplete);
		let gs = result
			.conditions
			.iter()
			.find(|c| c.code == ConditionCode::GateStatus)
			.unwrap();
		assert_eq!(gs.status, ConditionStatus::Incomplete);
	}

	// ── 16. condition_codes_serialize_screaming_snake ────────

	#[test]
	fn condition_codes_serialize_screaming_snake() {
		assert_eq!(ConditionCode::SnapshotExists.as_str(), "SNAPSHOT_EXISTS");
		assert_eq!(ConditionCode::IndexNotEmpty.as_str(), "INDEX_NOT_EMPTY");
		assert_eq!(ConditionCode::StaleFiles.as_str(), "STALE_FILES");
		assert_eq!(
			ConditionCode::CallGraphReliability.as_str(),
			"CALL_GRAPH_RELIABILITY"
		);
		assert_eq!(
			ConditionCode::DeadCodeReliability.as_str(),
			"DEAD_CODE_RELIABILITY"
		);
		assert_eq!(ConditionCode::EnrichmentState.as_str(), "ENRICHMENT_STATE");
		assert_eq!(ConditionCode::GateStatus.as_str(), "GATE_STATUS");
	}

	// ── 17. no_snapshot_only_evaluates_snapshot_exists ───────

	#[test]
	fn no_snapshot_only_evaluates_snapshot_exists() {
		let input = CheckInput {
			snapshot_exists: false,
			files_total: 0,
			stale_file_count: 0,
			call_graph_reliability: None,
			dead_code_reliability: None,
			enrichment_state: None,
			gate_outcome: None,
		};
		let result = check(&input);
		assert_eq!(result.conditions.len(), 1);
		assert_eq!(result.conditions[0].code, ConditionCode::SnapshotExists);
		assert_eq!(result.conditions[0].status, ConditionStatus::Incomplete);
	}

	// ── 18. all_conditions_present_when_snapshot_exists ──────

	#[test]
	fn all_conditions_present_when_snapshot_exists() {
		let result = check(&all_pass_input());
		assert_eq!(
			result.conditions.len(),
			7,
			"Expected 7 conditions when snapshot exists, got {}",
			result.conditions.len(),
		);
		let codes: Vec<ConditionCode> =
			result.conditions.iter().map(|c| c.code).collect();
		assert_eq!(
			codes,
			vec![
				ConditionCode::SnapshotExists,
				ConditionCode::IndexNotEmpty,
				ConditionCode::StaleFiles,
				ConditionCode::CallGraphReliability,
				ConditionCode::DeadCodeReliability,
				ConditionCode::EnrichmentState,
				ConditionCode::GateStatus,
			],
		);
	}
}
