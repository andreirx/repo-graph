//! File-scoped signal tests.
//!
//! Tests that the file pipeline emits the correct signals and
//! limits for a single-file focus, and does NOT emit signals
//! that are only meaningful at repo or path scope (boundary
//! violations, import cycles, gate).

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentBoundaryDeclaration, AgentCycle, AgentPathResolution,
	AgentRepoSummary, Budget, LimitCode, SignalCode, SignalEvidence,
};
use repo_graph_gate::{GateObligation, GateRequirement};

fn seeded_with_file_focus() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	// Seed path resolution to route to file pipeline.
	fake.path_resolutions.insert(
		("snap-1".into(), "src/core/service.ts".into()),
		AgentPathResolution {
			has_exact_file: true,
			file_stable_key: None,
			has_content_under_prefix: false,
			module_stable_key: None,
		},
	);
	fake
}

fn find_signal<'a>(
	result: &'a repo_graph_agent::OrientResult,
	code: SignalCode,
) -> Option<&'a repo_graph_agent::Signal> {
	result.signals.iter().find(|s| s.code() == code)
}

fn find_limit<'a>(
	result: &'a repo_graph_agent::OrientResult,
	code: LimitCode,
) -> Option<&'a repo_graph_agent::Limit> {
	result.limits.iter().find(|l| l.code == code)
}

// ── DEAD_CODE scoped to file ────────────────────────────────────
// Test removed: dead-code surface withdrawn.
// See orient_repo_dead_code_reliability.rs for withdrawal regression tests.

// ── MODULE_SUMMARY scoped to file ───────────────────────────────

#[test]
fn file_focus_module_summary_is_one_file() {
	let mut fake = seeded_with_file_focus();
	fake.file_summaries.insert(
		("snap-1".into(), "src/core/service.ts".into()),
		AgentRepoSummary {
			file_count: 1,
			symbol_count: 5,
			languages: vec!["typescript".into()],
		},
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core/service.ts"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	let sig = find_signal(&result, SignalCode::ModuleSummary)
		.expect("MODULE_SUMMARY must be emitted for file focus");
	match sig.evidence() {
		SignalEvidence::ModuleSummary(ev) => {
			assert_eq!(ev.file_count, 1);
			assert_eq!(ev.symbol_count, 5);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── Boundary violations NOT emitted ─────────────────────────────

#[test]
fn file_focus_does_not_emit_boundary_violations() {
	let mut fake = seeded_with_file_focus();
	// Seed boundary declarations at repo level — the file pipeline
	// must NOT look at these.
	fake.boundary_declarations.insert(
		"r1".into(),
		vec![AgentBoundaryDeclaration {
			source_module: "src/core".into(),
			forbidden_target: "src/adapters".into(),
			reason: None,
		}],
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core/service.ts"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(
		find_signal(&result, SignalCode::BoundaryViolations).is_none(),
		"file focus must not emit BOUNDARY_VIOLATIONS"
	);
}

// ── Import cycles NOT emitted ───────────────────────────────────

#[test]
fn file_focus_does_not_emit_import_cycles() {
	let mut fake = seeded_with_file_focus();
	// Seed cycles at repo level — the file pipeline must NOT
	// look at these.
	fake.cycles.insert(
		"snap-1".into(),
		vec![AgentCycle {
			length: 2,
			modules: vec!["src/core".into(), "src/adapters".into()],
		}],
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core/service.ts"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(
		find_signal(&result, SignalCode::ImportCycles).is_none(),
		"file focus must not emit IMPORT_CYCLES"
	);
}

// ── Gate signals NOT emitted ────────────────────────────────────

#[test]
fn file_focus_does_not_emit_gate_signals() {
	let mut fake = seeded_with_file_focus();
	// Seed gate requirements — the file pipeline must NOT
	// evaluate gate.
	fake.gate_requirements.insert(
		"r1".into(),
		vec![GateRequirement {
			req_id: "REQ-001".into(),
			version: 1,
			obligations: vec![GateObligation {
				obligation_id: "OBL-001".into(),
				obligation: "test obligation".into(),
				method: "arch_violations".into(),
				target: Some("src/core".into()),
				threshold: None,
				operator: None,
			}],
		}],
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core/service.ts"),
		Budget::Medium,
		common::TEST_NOW,
	)
	.unwrap();

	assert!(
		find_signal(&result, SignalCode::GatePass).is_none(),
		"file focus must not emit GATE_PASS"
	);
	assert!(
		find_signal(&result, SignalCode::GateFail).is_none(),
		"file focus must not emit GATE_FAIL"
	);
	assert!(
		find_signal(&result, SignalCode::GateIncomplete).is_none(),
		"file focus must not emit GATE_INCOMPLETE"
	);
	assert!(
		find_limit(&result, LimitCode::GateNotConfigured).is_none(),
		"file focus must not emit GATE_NOT_CONFIGURED limit"
	);
}
