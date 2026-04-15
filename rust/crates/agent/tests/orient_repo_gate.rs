//! Gate signal emission tests.
//!
//! Rust-43A added gate signal coverage to the repo-level orient
//! pipeline. These tests drive the agent through a fake that
//! implements both `AgentStorageRead` and `GateStorageRead` and
//! assert the correct signal code and evidence shape are
//! produced for each gate outcome.
//!
//! The tests seed gate inputs via the `gate_*` fields on
//! `FakeAgentStorage`. Outcomes are determined by the same gate
//! policy compute path that the `rgr-rust gate` CLI command uses,
//! so passing agent tests here are strong evidence that CLI and
//! agent consumers agree on the gate outcome.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, Budget, LimitCode, SignalCode, SignalEvidence,
};
use repo_graph_gate::{
	GateBoundaryDeclaration, GateImportEdge, GateMeasurement, GateObligation,
	GateRequirement,
};

fn seeded() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	fake
}

fn arch_requirement() -> GateRequirement {
	GateRequirement {
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
	}
}

fn coverage_requirement(threshold: f64) -> GateRequirement {
	GateRequirement {
		req_id: "REQ-2".into(),
		version: 1,
		obligations: vec![GateObligation {
			obligation_id: "o1".into(),
			obligation: "core coverage".into(),
			method: "coverage_threshold".into(),
			target: Some("src/core".into()),
			threshold: Some(threshold),
			operator: Some(">=".into()),
		}],
	}
}

fn find_signal<'a>(
	result: &'a repo_graph_agent::OrientResult,
	code: SignalCode,
) -> Option<&'a repo_graph_agent::Signal> {
	result.signals.iter().find(|s| s.code() == code)
}

// ── GATE_PASS ───────────────────────────────────────────────────

#[test]
fn gate_pass_emitted_when_arch_violations_clean() {
	let mut fake = seeded();
	fake.gate_requirements.insert("r1".into(), vec![arch_requirement()]);
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}],
	);
	// No edges seeded → zero violations → PASS.

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::GatePass)
		.expect("GATE_PASS must fire when obligations all pass");
	match sig.evidence() {
		SignalEvidence::GatePass(ev) => {
			assert_eq!(ev.pass_count, 1);
			assert_eq!(ev.total_count, 1);
			assert_eq!(ev.waived_count, 0);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── GATE_FAIL ───────────────────────────────────────────────────

#[test]
fn gate_fail_emitted_when_arch_violations_fail() {
	let mut fake = seeded();
	fake.gate_requirements.insert("r1".into(), vec![arch_requirement()]);
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}],
	);
	fake.gate_boundary_imports.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![
			GateImportEdge {
				source_file: "src/core/a.rs".into(),
				target_file: "src/adapters/b.rs".into(),
			},
			GateImportEdge {
				source_file: "src/core/c.rs".into(),
				target_file: "src/adapters/d.rs".into(),
			},
		],
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::GateFail)
		.expect("GATE_FAIL must fire when obligations fail");
	match sig.evidence() {
		SignalEvidence::GateFail(ev) => {
			assert_eq!(ev.fail_count, 1);
			assert_eq!(ev.total_count, 1);
			assert_eq!(ev.failing_obligations, vec!["REQ-1/o1".to_string()]);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

#[test]
fn gate_fail_excludes_gate_not_configured_limit() {
	let mut fake = seeded();
	fake.gate_requirements.insert("r1".into(), vec![arch_requirement()]);
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}],
	);
	fake.gate_boundary_imports.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![GateImportEdge {
			source_file: "src/core/a.rs".into(),
			target_file: "src/adapters/b.rs".into(),
		}],
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	let has = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::GateNotConfigured);
	assert!(
		!has,
		"GATE_NOT_CONFIGURED must be absent when gate ran with obligations"
	);
}

// ── GATE_INCOMPLETE ─────────────────────────────────────────────

#[test]
fn gate_incomplete_emitted_when_obligation_has_missing_data() {
	// Coverage obligation with no matching measurements →
	// MISSING_EVIDENCE → default-mode outcome is "incomplete".
	let mut fake = seeded();
	fake.gate_requirements
		.insert("r1".into(), vec![coverage_requirement(0.80)]);
	// No gate_coverage seed → no measurements.

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::GateIncomplete)
		.expect("GATE_INCOMPLETE must fire on missing coverage data");
	match sig.evidence() {
		SignalEvidence::GateIncomplete(ev) => {
			assert_eq!(ev.missing_count, 1);
			assert_eq!(ev.unsupported_count, 0);
			assert_eq!(ev.total_count, 1);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

#[test]
fn gate_incomplete_when_unsupported_method() {
	let mut fake = seeded();
	fake.gate_requirements.insert(
		"r1".into(),
		vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![GateObligation {
				obligation_id: "o1".into(),
				obligation: "mystery method".into(),
				method: "non_existent_method".into(),
				target: None,
				threshold: None,
				operator: None,
			}],
		}],
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::GateIncomplete)
		.expect("GATE_INCOMPLETE must fire on unsupported method");
	match sig.evidence() {
		SignalEvidence::GateIncomplete(ev) => {
			assert_eq!(ev.unsupported_count, 1);
			assert_eq!(ev.missing_count, 0);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── GATE_NOT_CONFIGURED limit vs. gate signals ──────────────────

#[test]
fn no_gate_signal_emitted_when_no_requirements() {
	let fake = seeded();
	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	assert!(find_signal(&result, SignalCode::GatePass).is_none());
	assert!(find_signal(&result, SignalCode::GateFail).is_none());
	assert!(find_signal(&result, SignalCode::GateIncomplete).is_none());
}

// ── Waiver overlay passes through orient ────────────────────────

#[test]
fn waiver_overlay_turns_gate_fail_into_gate_pass() {
	let mut fake = seeded();
	fake.gate_requirements.insert("r1".into(), vec![arch_requirement()]);
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}],
	);
	fake.gate_boundary_imports.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![GateImportEdge {
			source_file: "src/core/a.rs".into(),
			target_file: "src/adapters/b.rs".into(),
		}],
	);
	// Active waiver on the obligation.
	fake.gate_waivers.insert(
		("r1".into(), "REQ-1".into(), 1, "o1".into()),
		vec![repo_graph_gate::GateWaiver {
			waiver_uid: "w1".into(),
			reason: "accepted".into(),
			created_at: "2026-04-14T00:00:00Z".into(),
			created_by: None,
			expires_at: None,
			rationale_category: None,
			policy_basis: None,
		}],
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::GatePass)
		.expect("waiver should flip GATE_FAIL to GATE_PASS at orient level");
	match sig.evidence() {
		SignalEvidence::GatePass(ev) => {
			assert_eq!(ev.waived_count, 1);
			assert_eq!(ev.pass_count, 0);
			assert_eq!(ev.total_count, 1);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── Gate signal ranks highest severity ──────────────────────────

#[test]
fn gate_fail_ranks_at_the_top_of_the_signal_list() {
	let mut fake = seeded();
	fake.gate_requirements.insert("r1".into(), vec![arch_requirement()]);
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}],
	);
	fake.gate_boundary_imports.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![GateImportEdge {
			source_file: "src/core/a.rs".into(),
			target_file: "src/adapters/b.rs".into(),
		}],
	);
	// Also seed an unrelated boundary violation so we know GATE
	// ranks ahead of BOUNDARY_VIOLATIONS in the tie-break order.
	fake.boundary_declarations.insert(
		"r1".into(),
		vec![repo_graph_agent::AgentBoundaryDeclaration {
			source_module: "src/other".into(),
			forbidden_target: "src/forbid".into(),
			reason: None,
		}],
	);
	fake.imports_between_paths.insert(
		("snap-1".into(), "src/other".into(), "src/forbid".into()),
		vec![repo_graph_agent::AgentImportEdge {
			source_file: "src/other/a.rs".into(),
			target_file: "src/forbid/b.rs".into(),
		}],
	);

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let first = result.signals.first().expect("at least one signal");
	// Ranking: severity High, then category order Gate > Boundary.
	assert_eq!(
		first.code(),
		SignalCode::GateFail,
		"GATE_FAIL must outrank BOUNDARY_VIOLATIONS within High severity"
	);
}

// ── Waiver expiry regression (P2 fix) ───────────────────────────
//
// The P2 review identified that the Rust-43A initial shipping
// used a far-future sentinel for `now`, which (lexicographically
// compared) made every finite-expiry waiver appear already
// expired. The fix threaded a real `now` through orient; these
// tests lock the correct expiry semantics so the bug cannot
// reappear silently.
//
// Fixture shape: one failing arch_violations obligation with a
// single waiver. The waiver has a finite `expires_at`. Calls
// before expiry must WAIVE the failure; calls after expiry must
// leave it as FAIL. A separate perpetual-waiver test pins the
// `expires_at = None` path.

fn seed_fail_with_finite_waiver(expires_at: &str) -> FakeAgentStorage {
	let mut fake = seeded();
	fake.gate_requirements.insert("r1".into(), vec![arch_requirement()]);
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}],
	);
	fake.gate_boundary_imports.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![GateImportEdge {
			source_file: "src/core/a.rs".into(),
			target_file: "src/adapters/b.rs".into(),
		}],
	);
	fake.gate_waivers.insert(
		("r1".into(), "REQ-1".into(), 1, "o1".into()),
		vec![repo_graph_gate::GateWaiver {
			waiver_uid: "w1".into(),
			reason: "temporary exception".into(),
			created_at: "2026-01-01T00:00:00Z".into(),
			created_by: None,
			expires_at: Some(expires_at.to_string()),
			rationale_category: None,
			policy_basis: None,
		}],
	);
	fake
}

#[test]
fn finite_waiver_applies_before_expiry_at_orient_level() {
	// expires_at = 2027-01-01, now = 2026-04-15
	// expires_at > now → waiver active → FAIL becomes WAIVED
	// → gate outcome PASS with waived_count = 1.
	let fake = seed_fail_with_finite_waiver("2027-01-01T00:00:00Z");
	let result = orient(&fake, "r1", None, Budget::Medium, "2026-04-15T00:00:00Z")
		.unwrap();
	let sig = find_signal(&result, SignalCode::GatePass).expect(
		"active waiver before expiry must suppress FAIL and yield GATE_PASS",
	);
	match sig.evidence() {
		SignalEvidence::GatePass(ev) => {
			assert_eq!(ev.waived_count, 1);
			assert_eq!(ev.pass_count, 0);
			assert_eq!(ev.total_count, 1);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

#[test]
fn finite_waiver_does_not_apply_after_expiry_at_orient_level() {
	// expires_at = 2026-06-01, now = 2026-09-01
	// expires_at <= now → waiver expired → FAIL stays FAIL.
	let fake = seed_fail_with_finite_waiver("2026-06-01T00:00:00Z");
	let result = orient(&fake, "r1", None, Budget::Medium, "2026-09-01T00:00:00Z")
		.unwrap();
	let sig = find_signal(&result, SignalCode::GateFail).expect(
		"expired waiver must NOT suppress FAIL; GATE_FAIL must be reported",
	);
	match sig.evidence() {
		SignalEvidence::GateFail(ev) => {
			assert_eq!(ev.fail_count, 1);
			assert_eq!(ev.total_count, 1);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
	// A paranoid pin: the corresponding PASS signal must NOT
	// be present. This guards against a future refactor that
	// accidentally emits both.
	assert!(
		find_signal(&result, SignalCode::GatePass).is_none(),
		"GATE_PASS must not coexist with GATE_FAIL"
	);
}

#[test]
fn perpetual_waiver_applies_regardless_of_now() {
	// expires_at = None → always active, whatever `now` is.
	let mut fake = seeded();
	fake.gate_requirements.insert("r1".into(), vec![arch_requirement()]);
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![GateBoundaryDeclaration {
			boundary_module: "src/core".into(),
			forbids: "src/adapters".into(),
			reason: None,
		}],
	);
	fake.gate_boundary_imports.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![GateImportEdge {
			source_file: "src/core/a.rs".into(),
			target_file: "src/adapters/b.rs".into(),
		}],
	);
	fake.gate_waivers.insert(
		("r1".into(), "REQ-1".into(), 1, "o1".into()),
		vec![repo_graph_gate::GateWaiver {
			waiver_uid: "w1".into(),
			reason: "permanent exception".into(),
			created_at: "2020-01-01T00:00:00Z".into(),
			created_by: None,
			expires_at: None,
			rationale_category: None,
			policy_basis: None,
		}],
	);

	// Call twice with wildly different `now` values. Both must
	// yield GATE_PASS with waived_count = 1.
	for now in &["2020-06-01T00:00:00Z", "9999-01-01T00:00:00Z"] {
		let result = orient(&fake, "r1", None, Budget::Medium, now).unwrap();
		let sig = find_signal(&result, SignalCode::GatePass).unwrap_or_else(|| {
			panic!("perpetual waiver must apply at now={}", now)
		});
		match sig.evidence() {
			SignalEvidence::GatePass(ev) => assert_eq!(ev.waived_count, 1),
			other => panic!("wrong evidence variant at now={}: {:?}", now, other),
		}
	}
}

// ── Coverage obligation pass path ───────────────────────────────

#[test]
fn gate_pass_with_coverage_threshold_met() {
	let mut fake = seeded();
	fake.gate_requirements
		.insert("r1".into(), vec![coverage_requirement(0.80)]);
	fake.gate_coverage.insert(
		"snap-1".into(),
		vec![
			GateMeasurement {
				target_stable_key: "r1:src/core/a.rs:FILE".into(),
				value_json: r#"{"value":0.90}"#.into(),
			},
			GateMeasurement {
				target_stable_key: "r1:src/core/b.rs:FILE".into(),
				value_json: r#"{"value":0.85}"#.into(),
			},
		],
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::GatePass)
		.expect("GATE_PASS must fire when coverage threshold met");
	match sig.evidence() {
		SignalEvidence::GatePass(ev) => {
			assert_eq!(ev.pass_count, 1);
			assert_eq!(ev.total_count, 1);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}
