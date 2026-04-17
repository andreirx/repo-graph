//! Use-case seam tests for the `check` pipeline.
//!
//! Each test seeds a `FakeAgentStorage` with specific substrate
//! conditions, calls `run_check`, and asserts the verdict signal
//! code and condition evidence in the resulting `OrientResult`
//! envelope.
//!
//! The tests drive through the full three-phase pipeline:
//!   Phase 1: gather (port calls through the fake)
//!   Phase 2: reduce (pure CheckInput -> CheckResult)
//!   Phase 3: format (CheckResult -> OrientResult envelope)

mod common;

use common::{FakeAgentStorage, TEST_NOW};
use repo_graph_agent::{
	run_check, AgentReliabilityAxis, AgentReliabilityLevel,
	AgentSnapshot, AgentStaleFile, AgentTrustSummary, CheckError,
	EnrichmentState, SignalCode, SignalEvidence, CHECK_COMMAND,
	ORIENT_SCHEMA,
};
use repo_graph_gate::{
	GateBoundaryDeclaration, GateObligation, GateRequirement,
};

fn seeded() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	// seed_minimal_repo sets files_total=0; override to
	// non-zero so INDEX_NOT_EMPTY passes by default.
	fake.snapshots.get_mut("r1").unwrap().files_total = 42;
	fake
}

fn find_signal<'a>(
	result: &'a repo_graph_agent::OrientResult,
	code: SignalCode,
) -> Option<&'a repo_graph_agent::Signal> {
	result.signals.iter().find(|s| s.code() == code)
}

// ── Envelope shape ─────────────────────────────────────────────

#[test]
fn envelope_uses_check_command_and_shared_schema() {
	let fake = seeded();
	let result = run_check(&fake, "r1", TEST_NOW).unwrap();
	assert_eq!(result.schema, ORIENT_SCHEMA);
	assert_eq!(result.command, CHECK_COMMAND);
	assert_eq!(result.repo, "my-repo");
	assert_eq!(result.snapshot, "snap-1");
}

// ── 1. no_snapshot_produces_incomplete ──────────────────────────

#[test]
fn no_snapshot_produces_incomplete() {
	let mut fake = FakeAgentStorage::new();
	fake.repos.insert(
		"r1".into(),
		repo_graph_agent::AgentRepo {
			repo_uid: "r1".into(),
			name: "my-repo".into(),
		},
	);
	// No snapshot seeded.

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckIncomplete)
		.expect("expected CHECK_INCOMPLETE signal");
	match sig.evidence() {
		SignalEvidence::CheckIncomplete(ev) => {
			assert_eq!(ev.incomplete_conditions.len(), 1);
			assert_eq!(
				ev.incomplete_conditions[0].code,
				"SNAPSHOT_EXISTS"
			);
			assert!(ev.fail_conditions.is_empty());
			assert!(ev.passing.is_empty());
		}
		other => panic!(
			"expected CheckIncomplete evidence, got {:?}",
			other
		),
	}

	// Snapshot field is empty when no snapshot exists.
	assert_eq!(result.snapshot, "");
	// Confidence is Low when no snapshot.
	assert_eq!(result.confidence, repo_graph_agent::Confidence::Low);
}

// ── 2. empty_snapshot_produces_incomplete ───────────────────────

#[test]
fn empty_snapshot_produces_incomplete() {
	let mut fake = seeded();
	// Override files_total to 0 for INDEX_NOT_EMPTY fail.
	fake.snapshots.get_mut("r1").unwrap().files_total = 0;

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckIncomplete)
		.expect("expected CHECK_INCOMPLETE signal");
	match sig.evidence() {
		SignalEvidence::CheckIncomplete(ev) => {
			let codes: Vec<&str> = ev
				.incomplete_conditions
				.iter()
				.map(|c| c.code.as_str())
				.collect();
			assert!(
				codes.contains(&"INDEX_NOT_EMPTY"),
				"expected INDEX_NOT_EMPTY in incomplete conditions: {:?}",
				codes
			);
		}
		other => panic!(
			"expected CheckIncomplete evidence, got {:?}",
			other
		),
	}
}

// ── 3. stale_files_produce_fail ────────────────────────────────

#[test]
fn stale_files_produce_fail() {
	let mut fake = seeded();
	fake.stale_files.insert(
		"snap-1".into(),
		vec![
			AgentStaleFile { path: "a.ts".into() },
			AgentStaleFile { path: "b.ts".into() },
		],
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckFail)
		.expect("expected CHECK_FAIL signal");
	match sig.evidence() {
		SignalEvidence::CheckFail(ev) => {
			let codes: Vec<&str> = ev
				.fail_conditions
				.iter()
				.map(|c| c.code.as_str())
				.collect();
			assert!(
				codes.contains(&"STALE_FILES"),
				"expected STALE_FILES in fail conditions: {:?}",
				codes
			);
		}
		other => panic!(
			"expected CheckFail evidence, got {:?}",
			other
		),
	}
}

// ── 4. call_graph_medium_everything_pass ───────────────────────

#[test]
fn call_graph_medium_everything_pass() {
	let mut fake = seeded();
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 0.75,
			resolved_calls: 75,
			unresolved_calls: 25,
			call_graph_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::Medium,
				reasons: Vec::new(),
			},
			dead_code_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			enrichment_state: EnrichmentState::Ran,
			enrichment_eligible: 10,
			enrichment_enriched: 9,
		},
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckPass)
		.expect("expected CHECK_PASS signal");
	match sig.evidence() {
		SignalEvidence::CheckPass(ev) => {
			// CALL_GRAPH_RELIABILITY should be pass (MEDIUM is
			// advisory in check).
			let cg = ev
				.conditions
				.iter()
				.find(|c| c.code == "CALL_GRAPH_RELIABILITY")
				.expect("expected CALL_GRAPH_RELIABILITY condition");
			assert_eq!(cg.status, "pass");
		}
		other => panic!(
			"expected CheckPass evidence, got {:?}",
			other
		),
	}
}

// ── 5. dead_code_reliability_non_high_produces_fail ────────────

#[test]
fn dead_code_reliability_non_high_produces_fail() {
	let mut fake = seeded();
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 0.90,
			resolved_calls: 90,
			unresolved_calls: 10,
			call_graph_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			dead_code_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::Low,
				reasons: vec!["test".into()],
			},
			enrichment_state: EnrichmentState::Ran,
			enrichment_eligible: 10,
			enrichment_enriched: 9,
		},
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckFail)
		.expect("expected CHECK_FAIL signal");
	match sig.evidence() {
		SignalEvidence::CheckFail(ev) => {
			let codes: Vec<&str> = ev
				.fail_conditions
				.iter()
				.map(|c| c.code.as_str())
				.collect();
			assert!(
				codes.contains(&"DEAD_CODE_RELIABILITY"),
				"expected DEAD_CODE_RELIABILITY in fail: {:?}",
				codes
			);
		}
		other => panic!(
			"expected CheckFail evidence, got {:?}",
			other
		),
	}
}

// ── 6. enrichment_not_run_produces_fail ────────────────────────

#[test]
fn enrichment_not_run_produces_fail() {
	let mut fake = seeded();
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 0.90,
			resolved_calls: 90,
			unresolved_calls: 10,
			call_graph_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			dead_code_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			enrichment_state: EnrichmentState::NotRun,
			enrichment_eligible: 0,
			enrichment_enriched: 0,
		},
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckFail)
		.expect("expected CHECK_FAIL signal");
	match sig.evidence() {
		SignalEvidence::CheckFail(ev) => {
			let codes: Vec<&str> = ev
				.fail_conditions
				.iter()
				.map(|c| c.code.as_str())
				.collect();
			assert!(
				codes.contains(&"ENRICHMENT_STATE"),
				"expected ENRICHMENT_STATE in fail: {:?}",
				codes
			);
		}
		other => panic!(
			"expected CheckFail evidence, got {:?}",
			other
		),
	}
}

// ── 7. gate_fail_produces_fail ─────────────────────────────────

#[test]
fn gate_fail_produces_fail() {
	let mut fake = seeded();

	// Seed a gate requirement with an arch_violations obligation
	// and create a violation.
	fake.gate_requirements.insert(
		"r1".into(),
		vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![GateObligation {
				obligation_id: "o1".into(),
				obligation: "core must not import adapters".into(),
				method: "arch_violations".into(),
				target: Some("src/core".into()),
				threshold: None,
				operator: None,
			}],
		}],
	);
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}],
	);
	// Seed a violation: an import from core to adapters.
	use repo_graph_gate::GateImportEdge;
	fake.gate_boundary_imports.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![GateImportEdge {
			source_file: "src/core/service.ts".into(),
			target_file: "src/adapters/db.ts".into(),
		}],
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckFail)
		.expect("expected CHECK_FAIL signal");
	match sig.evidence() {
		SignalEvidence::CheckFail(ev) => {
			let codes: Vec<&str> = ev
				.fail_conditions
				.iter()
				.map(|c| c.code.as_str())
				.collect();
			assert!(
				codes.contains(&"GATE_STATUS"),
				"expected GATE_STATUS in fail: {:?}",
				codes
			);
		}
		other => panic!(
			"expected CheckFail evidence, got {:?}",
			other
		),
	}
}

// ── 8. gate_incomplete_produces_incomplete ─────────────────────

#[test]
fn gate_incomplete_produces_incomplete() {
	let mut fake = seeded();

	// Seed a gate requirement with an unsupported method.
	fake.gate_requirements.insert(
		"r1".into(),
		vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![GateObligation {
				obligation_id: "o1".into(),
				obligation: "unsupported check".into(),
				method: "nonexistent_method".into(),
				target: Some("src/core".into()),
				threshold: None,
				operator: None,
			}],
		}],
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckIncomplete)
		.expect("expected CHECK_INCOMPLETE signal");
	match sig.evidence() {
		SignalEvidence::CheckIncomplete(ev) => {
			let codes: Vec<&str> = ev
				.incomplete_conditions
				.iter()
				.map(|c| c.code.as_str())
				.collect();
			assert!(
				codes.contains(&"GATE_STATUS"),
				"expected GATE_STATUS in incomplete: {:?}",
				codes
			);
		}
		other => panic!(
			"expected CheckIncomplete evidence, got {:?}",
			other
		),
	}
}

// ── 9. gate_not_configured_produces_pass ───────────────────────

#[test]
fn gate_not_configured_produces_pass() {
	let fake = seeded();
	// No gate_requirements seeded -> NotConfigured -> pass.

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::CheckPass)
		.expect("expected CHECK_PASS signal");
	match sig.evidence() {
		SignalEvidence::CheckPass(ev) => {
			let gate = ev
				.conditions
				.iter()
				.find(|c| c.code == "GATE_STATUS")
				.expect("expected GATE_STATUS condition");
			assert_eq!(gate.status, "pass");
		}
		other => panic!(
			"expected CheckPass evidence, got {:?}",
			other
		),
	}
}

// ── 10. mixed_fail_and_incomplete_produces_incomplete ──────────

#[test]
fn mixed_fail_and_incomplete_produces_incomplete() {
	let mut fake = seeded();

	// Stale files -> STALE_FILES fail.
	fake.stale_files.insert(
		"snap-1".into(),
		vec![AgentStaleFile { path: "a.ts".into() }],
	);

	// Gate with unsupported method -> GATE_STATUS incomplete.
	fake.gate_requirements.insert(
		"r1".into(),
		vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![GateObligation {
				obligation_id: "o1".into(),
				obligation: "check".into(),
				method: "nonexistent_method".into(),
				target: Some("src/core".into()),
				threshold: None,
				operator: None,
			}],
		}],
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	// Incomplete wins over Fail.
	let sig = find_signal(&result, SignalCode::CheckIncomplete)
		.expect("expected CHECK_INCOMPLETE signal");
	match sig.evidence() {
		SignalEvidence::CheckIncomplete(ev) => {
			let incomplete_codes: Vec<&str> = ev
				.incomplete_conditions
				.iter()
				.map(|c| c.code.as_str())
				.collect();
			let fail_codes: Vec<&str> = ev
				.fail_conditions
				.iter()
				.map(|c| c.code.as_str())
				.collect();
			assert!(
				incomplete_codes.contains(&"GATE_STATUS"),
				"expected GATE_STATUS in incomplete: {:?}",
				incomplete_codes
			);
			assert!(
				fail_codes.contains(&"STALE_FILES"),
				"expected STALE_FILES in fail: {:?}",
				fail_codes
			);
		}
		other => panic!(
			"expected CheckIncomplete evidence, got {:?}",
			other
		),
	}
}

// ── Error cases ────────────────────────────────────────────────

#[test]
fn no_repo_returns_error() {
	let fake = FakeAgentStorage::new();
	let result = run_check(&fake, "nonexistent", TEST_NOW);
	match result {
		Err(CheckError::NoRepo { repo_uid }) => {
			assert_eq!(repo_uid, "nonexistent");
		}
		other => panic!("expected NoRepo error, got {:?}", other),
	}
}

// ── SNAPSHOT_INFO signal ───────────────────────────────────────

#[test]
fn snapshot_info_emitted_when_snapshot_exists() {
	let fake = seeded();
	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	let sig = find_signal(&result, SignalCode::SnapshotInfo)
		.expect("expected SNAPSHOT_INFO signal");
	match sig.evidence() {
		SignalEvidence::SnapshotInfo(ev) => {
			assert_eq!(ev.snapshot_uid, "snap-1");
		}
		other => panic!(
			"expected SnapshotInfo evidence, got {:?}",
			other
		),
	}
}

#[test]
fn snapshot_info_not_emitted_when_no_snapshot() {
	let mut fake = FakeAgentStorage::new();
	fake.repos.insert(
		"r1".into(),
		repo_graph_agent::AgentRepo {
			repo_uid: "r1".into(),
			name: "my-repo".into(),
		},
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	assert!(
		find_signal(&result, SignalCode::SnapshotInfo).is_none(),
		"SNAPSHOT_INFO should not be emitted when no snapshot"
	);
}

// ── Signal count discipline ────────────────────────────────────

#[test]
fn check_emits_exactly_two_signals_when_snapshot_exists() {
	let fake = seeded();
	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	// ONE verdict signal + SNAPSHOT_INFO = 2 signals.
	assert_eq!(
		result.signals.len(),
		2,
		"expected exactly 2 signals (verdict + snapshot_info), got {:?}",
		result
			.signals
			.iter()
			.map(|s| s.code())
			.collect::<Vec<_>>()
	);
}

#[test]
fn check_emits_exactly_one_signal_when_no_snapshot() {
	let mut fake = FakeAgentStorage::new();
	fake.repos.insert(
		"r1".into(),
		repo_graph_agent::AgentRepo {
			repo_uid: "r1".into(),
			name: "my-repo".into(),
		},
	);

	let result = run_check(&fake, "r1", TEST_NOW).unwrap();

	// ONE verdict signal only (no SNAPSHOT_INFO).
	assert_eq!(
		result.signals.len(),
		1,
		"expected exactly 1 signal (verdict), got {:?}",
		result
			.signals
			.iter()
			.map(|s| s.code())
			.collect::<Vec<_>>()
	);
}
