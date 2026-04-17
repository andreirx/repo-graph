//! Check use case — three-phase pipeline.
//!
//! Phase 1: Gather substrate facts through existing ports.
//! Phase 2: Build CheckInput, call the pure `check()` reducer.
//! Phase 3: Map CheckResult into the shared OrientResult envelope.
//!
//! The use case function `run_check` is the only public entry
//! point. It is generic over storage that satisfies both
//! `AgentStorageRead` (agent port) and `GateStorageRead` (gate
//! policy port), matching the orient pattern.

pub mod evaluate;
pub mod reduce;
pub mod types;

pub use evaluate::evaluate_conditions;
pub use reduce::{check, reduce_verdict};
pub use types::*;

use repo_graph_gate::{GateMode, GateStorageRead};

use crate::confidence::derive_repo_confidence;
use crate::dto::envelope::{
	Confidence, Focus, OrientResult, CHECK_COMMAND, ORIENT_SCHEMA,
};
use crate::dto::signal::{
	CheckConditionEvidence, CheckFailEvidence, CheckIncompleteEvidence,
	CheckPassEvidence, Signal, SnapshotInfoEvidence,
};
use crate::errors::CheckError;
use crate::ranking;
use crate::storage_port::AgentStorageRead;

/// Entry point for the check use case.
///
/// Generic over a single storage handle that satisfies both
/// `AgentStorageRead` and `GateStorageRead`. Same pattern as
/// `orient()`.
///
/// `now` is an ISO 8601 timestamp used for waiver expiry
/// evaluation in the gate assembly. The check crate is
/// clock-free: callers must supply `now` explicitly.
///
/// Returns `CheckError::NoRepo` when the repo does not exist.
/// Missing snapshot is NOT an error -- it produces
/// `CHECK_INCOMPLETE` with only the `SNAPSHOT_EXISTS` condition.
pub fn run_check<S: AgentStorageRead + GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	now: &str,
) -> Result<OrientResult, CheckError> {
	// ── Phase 1: Gather ─────────────────────────────────────────

	// 1. Resolve repo identity.
	let repo = storage
		.get_repo(repo_uid)?
		.ok_or_else(|| CheckError::NoRepo {
			repo_uid: repo_uid.to_string(),
		})?;

	// 2. Try to get snapshot.
	let snapshot_opt = storage.get_latest_snapshot(repo_uid)?;

	let (input, snapshot_uid, confidence) = match snapshot_opt {
		None => {
			// No snapshot: build minimal CheckInput. The reducer
			// will produce CHECK_INCOMPLETE with only
			// SNAPSHOT_EXISTS condition.
			let input = CheckInput {
				snapshot_exists: false,
				files_total: 0,
				stale_file_count: 0,
				call_graph_reliability: None,
				dead_code_reliability: None,
				enrichment_state: None,
				gate_outcome: None,
			};
			(input, String::new(), Confidence::Low)
		}
		Some(ref snapshot) => {
			let snap_uid = snapshot.snapshot_uid.clone();

			// 3. Get stale files.
			let stale_files = storage.get_stale_files(&snap_uid)?;

			// 4. Get trust summary.
			let trust = storage.get_trust_summary(repo_uid, &snap_uid)?;

			// 5. Get gate outcome.
			let gate_outcome = gather_gate_outcome(
				storage, repo_uid, &snap_uid, now,
			);

			// Derive confidence from trust data.
			let stale = !stale_files.is_empty();
			let conf = derive_repo_confidence(&trust, stale);

			let input = CheckInput {
				snapshot_exists: true,
				files_total: snapshot.files_total,
				stale_file_count: stale_files.len() as u64,
				call_graph_reliability: Some(trust.call_graph_reliability.level),
				dead_code_reliability: Some(trust.dead_code_reliability.level),
				enrichment_state: Some(trust.enrichment_state),
				gate_outcome: Some(gate_outcome),
			};

			(input, snap_uid, conf)
		}
	};

	// ── Phase 2: Reduce ─────────────────────────────────────────

	let result = check(&input);

	// ── Phase 3: Format ─────────────────────────────────────────

	let mut signals = vec![build_verdict_signal(&result)];

	// Add SNAPSHOT_INFO if snapshot exists.
	if let Some(ref snapshot) = snapshot_opt {
		signals.push(Signal::snapshot_info(SnapshotInfoEvidence {
			snapshot_uid: snapshot.snapshot_uid.clone(),
			scope: snapshot.scope.clone(),
			basis_commit: snapshot.basis_commit.clone(),
			created_at: snapshot.created_at.clone(),
		}));
	}

	// Sort + rank (even with 1-2 signals, keeps the contract
	// consistent with orient).
	ranking::sort_and_rank(&mut signals);

	Ok(OrientResult {
		schema: ORIENT_SCHEMA,
		command: CHECK_COMMAND,
		repo: repo.name,
		snapshot: snapshot_uid,
		focus: Focus::repo(),
		confidence,

		signals,
		signals_truncated: None,
		signals_omitted_count: None,

		limits: Vec::new(),
		limits_truncated: None,
		limits_omitted_count: None,

		next: Vec::new(),
		next_truncated: None,
		next_omitted_count: None,

		truncated: false,
	})
}

/// Gather the gate outcome for check input.
///
/// Calls `repo_graph_gate::assemble_from_requirements` through the
/// `GateStorageRead` port. Maps the result to `GateOutcomeForCheck`.
///
/// - No active requirements -> `NotConfigured`.
/// - Gate error -> `Incomplete` (not a check error).
/// - `outcome == "pass"` with `total > 0` -> `Pass`.
/// - `outcome == "pass"` with `total == 0` -> `NotConfigured`.
/// - `outcome == "fail"` -> `Fail`.
/// - `outcome == "incomplete"` -> `Incomplete`.
fn gather_gate_outcome<S: GateStorageRead + ?Sized>(
	storage: &S,
	repo_uid: &str,
	snapshot_uid: &str,
	now: &str,
) -> GateOutcomeForCheck {
	// Fetch requirements to detect "not configured".
	let requirements = match storage.get_active_requirements(repo_uid) {
		Ok(reqs) => reqs,
		Err(_) => return GateOutcomeForCheck::Incomplete,
	};

	if requirements.is_empty() {
		return GateOutcomeForCheck::NotConfigured;
	}

	let report = match repo_graph_gate::assemble_from_requirements(
		storage,
		repo_uid,
		snapshot_uid,
		GateMode::Default,
		now,
		requirements,
	) {
		Ok(r) => r,
		Err(_) => return GateOutcomeForCheck::Incomplete,
	};

	// Zero obligations after assembly = effectively not configured.
	if report.outcome.counts.total == 0 {
		return GateOutcomeForCheck::NotConfigured;
	}

	match report.outcome.outcome.as_str() {
		"pass" => GateOutcomeForCheck::Pass,
		"fail" => GateOutcomeForCheck::Fail,
		"incomplete" => GateOutcomeForCheck::Incomplete,
		_ => GateOutcomeForCheck::Incomplete, // defensive
	}
}

/// Build the single verdict signal from the check result.
fn build_verdict_signal(result: &CheckResult) -> Signal {
	match result.verdict {
		CheckVerdict::Pass => {
			let conditions = result
				.conditions
				.iter()
				.map(condition_to_evidence)
				.collect();
			Signal::check_pass(CheckPassEvidence { conditions })
		}
		CheckVerdict::Fail => {
			let mut fail_conditions = Vec::new();
			let mut passing = Vec::new();
			for c in &result.conditions {
				let ev = condition_to_evidence(c);
				match c.status {
					ConditionStatus::Fail => fail_conditions.push(ev),
					_ => passing.push(ev),
				}
			}
			Signal::check_fail(CheckFailEvidence {
				fail_conditions,
				passing,
			})
		}
		CheckVerdict::Incomplete => {
			let mut incomplete_conditions = Vec::new();
			let mut fail_conditions = Vec::new();
			let mut passing = Vec::new();
			for c in &result.conditions {
				let ev = condition_to_evidence(c);
				match c.status {
					ConditionStatus::Incomplete => {
						incomplete_conditions.push(ev)
					}
					ConditionStatus::Fail => fail_conditions.push(ev),
					ConditionStatus::Pass => passing.push(ev),
				}
			}
			Signal::check_incomplete(CheckIncompleteEvidence {
				incomplete_conditions,
				fail_conditions,
				passing,
			})
		}
	}
}

/// Map a `ConditionResult` to a `CheckConditionEvidence`.
fn condition_to_evidence(c: &ConditionResult) -> CheckConditionEvidence {
	CheckConditionEvidence {
		code: c.code.as_str().to_string(),
		status: match c.status {
			ConditionStatus::Pass => "pass".to_string(),
			ConditionStatus::Fail => "fail".to_string(),
			ConditionStatus::Incomplete => "incomplete".to_string(),
		},
		summary: c.summary.clone(),
	}
}
