//! Path-scoped signal tests.
//!
//! Tests that the path pipeline emits all expected signals scoped
//! to the path prefix, including gate filtering by target prefix.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentBoundaryDeclaration, AgentCycle, AgentImportEdge,
	AgentPathResolution, AgentRepoSummary, Budget, LimitCode, SignalCode,
	SignalEvidence,
};
use repo_graph_gate::{GateObligation, GateRequirement};

fn seeded_with_path_focus() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	// Seed path resolution to route to path pipeline.
	fake.path_resolutions.insert(
		("snap-1".into(), "src/core".into()),
		AgentPathResolution {
			has_exact_file: false,
			file_stable_key: None,
			has_content_under_prefix: true,
			module_stable_key: Some("r1:src/core:MODULE".into()),
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

// ── DEAD_CODE scoped to path ────────────────────────────────────
// Test removed: dead-code surface withdrawn.
// See orient_repo_dead_code_reliability.rs for withdrawal regression tests.

// ── BOUNDARY_VIOLATIONS scoped to path ──────────────────────────

#[test]
fn path_focus_boundary_violations_includes_descendant_modules() {
	let mut fake = seeded_with_path_focus();
	fake.boundary_declarations_in_path.insert(
		("r1".into(), "src/core".into()),
		vec![AgentBoundaryDeclaration {
			source_module: "src/core".into(),
			forbidden_target: "src/adapters".into(),
			reason: Some("clean architecture".into()),
		}],
	);
	fake.imports_between_paths.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![AgentImportEdge {
			source_file: "src/core/service.ts".into(),
			target_file: "src/adapters/db.ts".into(),
		}],
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	let sig = find_signal(&result, SignalCode::BoundaryViolations)
		.expect("BOUNDARY_VIOLATIONS must be emitted for path focus");
	match sig.evidence() {
		SignalEvidence::BoundaryViolations(ev) => {
			assert_eq!(ev.violation_count, 1);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── IMPORT_CYCLES scoped to path ────────────────────────────────

#[test]
fn path_focus_import_cycles_includes_descendant_modules() {
	let mut fake = seeded_with_path_focus();
	fake.cycles_involving_path.insert(
		("snap-1".into(), "src/core".into()),
		vec![AgentCycle {
			length: 2,
			modules: vec!["src/core".into(), "src/core/sub".into()],
		}],
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	let sig = find_signal(&result, SignalCode::ImportCycles)
		.expect("IMPORT_CYCLES must be emitted for path focus");
	match sig.evidence() {
		SignalEvidence::ImportCycles(ev) => {
			assert_eq!(ev.cycle_count, 1);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── Gate filtered by target prefix ──────────────────────────────

#[test]
fn path_focus_gate_filters_by_target_prefix() {
	let mut fake = seeded_with_path_focus();
	fake.gate_requirements.insert(
		"r1".into(),
		vec![GateRequirement {
			req_id: "REQ-001".into(),
			version: 1,
			obligations: vec![
				GateObligation {
					obligation_id: "OBL-MATCH".into(),
					obligation: "arch violations in src/core".into(),
					method: "arch_violations".into(),
					target: Some("src/core".into()),
					threshold: None,
					operator: None,
				},
				GateObligation {
					obligation_id: "OBL-OTHER".into(),
					obligation: "arch violations in src/adapters".into(),
					method: "arch_violations".into(),
					target: Some("src/adapters".into()),
					threshold: None,
					operator: None,
				},
			],
		}],
	);
	// Seed boundary declarations for the arch_violations method
	// to find no violations (PASS).
	fake.gate_boundary_declarations.insert(
		"r1".into(),
		vec![],
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core"),
		Budget::Medium,
		common::TEST_NOW,
	)
	.unwrap();

	// Gate should have evaluated only OBL-MATCH. With no boundary
	// declarations, arch_violations produces PASS.
	let has_gate_signal = find_signal(&result, SignalCode::GatePass).is_some()
		|| find_signal(&result, SignalCode::GateFail).is_some()
		|| find_signal(&result, SignalCode::GateIncomplete).is_some();
	assert!(
		has_gate_signal,
		"gate must produce a signal when obligations target the prefix"
	);

	// The GATE_NOT_APPLICABLE_TO_FOCUS limit must NOT appear.
	assert!(
		find_limit(&result, LimitCode::GateNotApplicableToFocus).is_none(),
		"must not emit GATE_NOT_APPLICABLE_TO_FOCUS when obligations match"
	);
}

#[test]
fn path_focus_gate_not_applicable_when_no_obligations_target_prefix() {
	let mut fake = seeded_with_path_focus();
	fake.gate_requirements.insert(
		"r1".into(),
		vec![GateRequirement {
			req_id: "REQ-001".into(),
			version: 1,
			obligations: vec![GateObligation {
				obligation_id: "OBL-OTHER".into(),
				obligation: "arch violations in src/adapters".into(),
				method: "arch_violations".into(),
				target: Some("src/adapters".into()),
				threshold: None,
				operator: None,
			}],
		}],
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core"),
		Budget::Medium,
		common::TEST_NOW,
	)
	.unwrap();

	// No gate signal — no obligations target src/core.
	assert!(
		find_signal(&result, SignalCode::GatePass).is_none(),
		"must not emit gate signal when no obligations target prefix"
	);
	assert!(
		find_signal(&result, SignalCode::GateFail).is_none(),
		"must not emit gate signal when no obligations target prefix"
	);

	// The GATE_NOT_APPLICABLE_TO_FOCUS limit must appear.
	let lim = find_limit(&result, LimitCode::GateNotApplicableToFocus)
		.expect("must emit GATE_NOT_APPLICABLE_TO_FOCUS");
	assert!(lim.summary.contains("no obligations target"));
}

// ── MODULE_SUMMARY scoped to path ───────────────────────────────

#[test]
fn path_focus_module_summary_scoped_to_prefix() {
	let mut fake = seeded_with_path_focus();
	fake.path_summaries.insert(
		("snap-1".into(), "src/core".into()),
		AgentRepoSummary {
			file_count: 10,
			symbol_count: 50,
			languages: vec!["typescript".into()],
		},
	);
	let result = orient(
		&fake,
		"r1",
		Some("src/core"),
		Budget::Small,
		common::TEST_NOW,
	)
	.unwrap();

	let sig = find_signal(&result, SignalCode::ModuleSummary)
		.expect("MODULE_SUMMARY must be emitted for path focus");
	match sig.evidence() {
		SignalEvidence::ModuleSummary(ev) => {
			assert_eq!(ev.file_count, 10);
			assert_eq!(ev.symbol_count, 50);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}
