//! Symbol-scoped signal emission tests.
//!
//! Tests that the symbol pipeline emits the correct signals with
//! the correct evidence and scope annotations.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentBoundaryDeclaration, AgentCalleeRow, AgentCallerRow,
	AgentCycle, AgentDeadNode, AgentFocusCandidate, AgentFocusKind,
	AgentImportEdge, AgentSymbolContext, Budget, LimitCode, SignalCode,
	SignalScope,
};

fn seeded_symbol() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	// Default: path resolution returns nothing.
	// Default: stable-key lookup returns nothing.
	// Symbol name resolution returns 1 result.
	let sk = "r1:src/core/service.ts:SYMBOL:doWork";
	fake.symbol_name_results.insert(
		("snap-1".into(), "doWork".into()),
		vec![AgentFocusCandidate {
			stable_key: sk.into(),
			kind: AgentFocusKind::Symbol,
			file: Some("src/core/service.ts".into()),
		}],
	);
	fake.symbol_contexts.insert(
		("snap-1".into(), sk.into()),
		AgentSymbolContext {
			file_path: Some("src/core/service.ts".into()),
			module_path: Some("src/core".into()),
			module_stable_key: Some("r1:src/core:MODULE".into()),
			name: "doWork".into(),
			qualified_name: Some("doWork".into()),
			subtype: Some("function".into()),
			line_start: Some(10),
		},
	);
	fake
}

fn seeded_symbol_no_module() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let sk = "r1:src/standalone.ts:SYMBOL:lonely";
	fake.symbol_name_results.insert(
		("snap-1".into(), "lonely".into()),
		vec![AgentFocusCandidate {
			stable_key: sk.into(),
			kind: AgentFocusKind::Symbol,
			file: Some("src/standalone.ts".into()),
		}],
	);
	fake.symbol_contexts.insert(
		("snap-1".into(), sk.into()),
		AgentSymbolContext {
			file_path: Some("src/standalone.ts".into()),
			module_path: None,
			module_stable_key: None,
			name: "lonely".into(),
			qualified_name: Some("lonely".into()),
			subtype: None,
			line_start: Some(1),
		},
	);
	fake
}

// ── 5. CALLERS_SUMMARY groups by module ────────────────────────

#[test]
fn callers_summary_groups_by_module() {
	let mut fake = seeded_symbol();
	let sk = "r1:src/core/service.ts:SYMBOL:doWork";

	fake.symbol_callers.insert(
		("snap-1".into(), sk.into()),
		vec![
			AgentCallerRow {
				stable_key: "r1:src/cli/run.ts:SYMBOL:main".into(),
				name: "main".into(),
				file: Some("src/cli/run.ts".into()),
				module_path: Some("src/cli".into()),
				module_stable_key: Some("r1:src/cli:MODULE".into()),
			},
			AgentCallerRow {
				stable_key: "r1:src/cli/setup.ts:SYMBOL:setup".into(),
				name: "setup".into(),
				file: Some("src/cli/setup.ts".into()),
				module_path: Some("src/cli".into()),
				module_stable_key: Some("r1:src/cli:MODULE".into()),
			},
			AgentCallerRow {
				stable_key: "r1:src/core/handler.ts:SYMBOL:handle".into(),
				name: "handle".into(),
				file: Some("src/core/handler.ts".into()),
				module_path: Some("src/core".into()),
				module_stable_key: Some("r1:src/core:MODULE".into()),
			},
		],
	);

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let callers_sig = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::CallersSummary)
		.expect("CALLERS_SUMMARY signal must be emitted");

	match callers_sig.evidence() {
		repo_graph_agent::SignalEvidence::CallersSummary(ev) => {
			assert_eq!(ev.count, 3);
			assert_eq!(ev.top_modules.len(), 2);
			// src/cli has 2 callers, should be first.
			assert_eq!(ev.top_modules[0].module, "src/cli");
			assert_eq!(ev.top_modules[0].count, 2);
			assert_eq!(ev.top_modules[1].module, "src/core");
			assert_eq!(ev.top_modules[1].count, 1);
		}
		other => panic!(
			"expected CallersSummary evidence, got: {:?}",
			other
		),
	}
}

// ── 6. CALLEES_SUMMARY groups by module ────────────────────────

#[test]
fn callees_summary_groups_by_module() {
	let mut fake = seeded_symbol();
	let sk = "r1:src/core/service.ts:SYMBOL:doWork";

	fake.symbol_callees.insert(
		("snap-1".into(), sk.into()),
		vec![
			AgentCalleeRow {
				stable_key: "r1:src/adapters/db.ts:SYMBOL:query".into(),
				name: "query".into(),
				file: Some("src/adapters/db.ts".into()),
				module_path: Some("src/adapters".into()),
				module_stable_key: Some("r1:src/adapters:MODULE".into()),
			},
			AgentCalleeRow {
				stable_key: "r1:src/adapters/cache.ts:SYMBOL:get".into(),
				name: "get".into(),
				file: Some("src/adapters/cache.ts".into()),
				module_path: Some("src/adapters".into()),
				module_stable_key: Some("r1:src/adapters:MODULE".into()),
			},
			AgentCalleeRow {
				stable_key: "r1:src/core/utils.ts:SYMBOL:validate".into(),
				name: "validate".into(),
				file: Some("src/core/utils.ts".into()),
				module_path: Some("src/core".into()),
				module_stable_key: Some("r1:src/core:MODULE".into()),
			},
		],
	);

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let callees_sig = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::CalleesSummary)
		.expect("CALLEES_SUMMARY signal must be emitted");

	match callees_sig.evidence() {
		repo_graph_agent::SignalEvidence::CalleesSummary(ev) => {
			assert_eq!(ev.count, 3);
			assert_eq!(ev.top_modules.len(), 2);
			assert_eq!(ev.top_modules[0].module, "src/adapters");
			assert_eq!(ev.top_modules[0].count, 2);
			assert_eq!(ev.top_modules[1].module, "src/core");
			assert_eq!(ev.top_modules[1].count, 1);
		}
		other => panic!(
			"expected CalleesSummary evidence, got: {:?}",
			other
		),
	}
}

// ── 7. Callers with unknown module grouped as "(unknown)" ──────

#[test]
fn callers_with_unknown_module_grouped_as_unknown() {
	let mut fake = seeded_symbol();
	let sk = "r1:src/core/service.ts:SYMBOL:doWork";

	fake.symbol_callers.insert(
		("snap-1".into(), sk.into()),
		vec![
			AgentCallerRow {
				stable_key: "r1:src/orphan.ts:SYMBOL:orphanFn".into(),
				name: "orphanFn".into(),
				file: Some("src/orphan.ts".into()),
				module_path: None,
				module_stable_key: None,
			},
			AgentCallerRow {
				stable_key: "r1:src/another.ts:SYMBOL:anotherFn".into(),
				name: "anotherFn".into(),
				file: Some("src/another.ts".into()),
				module_path: None,
				module_stable_key: None,
			},
		],
	);

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let callers_sig = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::CallersSummary)
		.expect("CALLERS_SUMMARY must be emitted");

	match callers_sig.evidence() {
		repo_graph_agent::SignalEvidence::CallersSummary(ev) => {
			assert_eq!(ev.count, 2);
			assert_eq!(ev.top_modules.len(), 1);
			assert_eq!(
				ev.top_modules[0].module, "(unknown)",
				"callers without module must be grouped as (unknown)"
			);
			assert_eq!(ev.top_modules[0].count, 2);
		}
		other => panic!("expected CallersSummary, got: {:?}", other),
	}
}

// ── 8. Symbol dead code fires when symbol is dead ──────────────

#[test]
fn symbol_dead_code_fires_when_symbol_is_dead() {
	let mut fake = seeded_symbol();
	let sk = "r1:src/core/service.ts:SYMBOL:doWork";

	// Seed file-level dead nodes containing our symbol.
	fake.dead_nodes_in_file.insert(
		("snap-1".into(), "src/core/service.ts".into()),
		vec![AgentDeadNode {
			stable_key: sk.into(),
			symbol: "doWork".into(),
			kind: "SYMBOL".into(),
			file: Some("src/core/service.ts".into()),
			line_count: Some(20),
			is_test: false,
		}],
	);

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let dead_sig = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::DeadCode);
	assert!(
		dead_sig.is_some(),
		"DEAD_CODE must fire when symbol is in dead list"
	);
}

// ── 9. Symbol dead code absent when symbol is alive ────────────

#[test]
fn symbol_dead_code_absent_when_symbol_is_alive() {
	let mut fake = seeded_symbol();

	// File has dead nodes but NOT our symbol.
	fake.dead_nodes_in_file.insert(
		("snap-1".into(), "src/core/service.ts".into()),
		vec![AgentDeadNode {
			stable_key: "r1:src/core/service.ts:SYMBOL:otherFn".into(),
			symbol: "otherFn".into(),
			kind: "SYMBOL".into(),
			file: Some("src/core/service.ts".into()),
			line_count: Some(5),
			is_test: false,
		}],
	);

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let dead_sig = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::DeadCode);
	assert!(
		dead_sig.is_none(),
		"DEAD_CODE must not fire when symbol is alive (not in dead list)"
	);
}

// ── 10. Inherited boundary violations have module_context scope ──

#[test]
fn inherited_boundary_violations_have_module_context_scope() {
	let mut fake = seeded_symbol();

	// Seed boundary declaration for the owning module.
	fake.boundary_declarations.insert(
		"r1".into(),
		vec![AgentBoundaryDeclaration {
			source_module: "src/core".into(),
			forbidden_target: "src/adapters".into(),
			reason: Some("clean arch".into()),
		}],
	);
	// Seed violating edges.
	fake.imports_between_paths.insert(
		(
			"snap-1".into(),
			"src/core".into(),
			"src/adapters".into(),
		),
		vec![AgentImportEdge {
			source_file: "src/core/service.ts".into(),
			target_file: "src/adapters/db.ts".into(),
		}],
	);

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let boundary_sig = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::BoundaryViolations)
		.expect("BOUNDARY_VIOLATIONS must be emitted");

	assert_eq!(
		boundary_sig.scope(),
		SignalScope::ModuleContext,
		"boundary violations at symbol scope must have ModuleContext scope"
	);
}

// ── 11. Inherited import cycles have module_context scope ──────

#[test]
fn inherited_import_cycles_have_module_context_scope() {
	let mut fake = seeded_symbol();

	// Seed cycle involving the owning module (exact match).
	fake.cycles_involving_module.insert(
		("snap-1".into(), "src/core".into()),
		vec![AgentCycle {
			length: 2,
			modules: vec!["src/core".into(), "src/adapters".into()],
		}],
	);

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let cycle_sig = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::ImportCycles)
		.expect("IMPORT_CYCLES must be emitted");

	assert_eq!(
		cycle_sig.scope(),
		SignalScope::ModuleContext,
		"import cycles at symbol scope must have ModuleContext scope"
	);
}

// ── 12. Direct callers_summary has no scope field in JSON ──────

#[test]
fn direct_callers_summary_has_no_scope_field_in_json() {
	let mut fake = seeded_symbol();
	let sk = "r1:src/core/service.ts:SYMBOL:doWork";

	fake.symbol_callers.insert(
		("snap-1".into(), sk.into()),
		vec![AgentCallerRow {
			stable_key: "r1:src/cli/main.ts:SYMBOL:run".into(),
			name: "run".into(),
			file: Some("src/cli/main.ts".into()),
			module_path: Some("src/cli".into()),
			module_stable_key: Some("r1:src/cli:MODULE".into()),
		}],
	);

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let callers_sig = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::CallersSummary)
		.expect("CALLERS_SUMMARY must exist");

	// Serialize to JSON and verify "scope" is absent.
	let json = serde_json::to_value(callers_sig).unwrap();
	assert!(
		json.get("scope").is_none(),
		"Direct (default) scope must NOT appear in JSON: {:?}",
		json
	);
}

// ── 13. No inherited signals when module context is missing ────

#[test]
fn no_inherited_signals_when_module_context_missing() {
	let mut fake = seeded_symbol_no_module();

	// Seed boundary declarations and cycles that WOULD fire if
	// the symbol had a module context.
	fake.boundary_declarations.insert(
		"r1".into(),
		vec![AgentBoundaryDeclaration {
			source_module: "src/standalone".into(),
			forbidden_target: "src/adapters".into(),
			reason: None,
		}],
	);
	fake.cycles_involving_module.insert(
		("snap-1".into(), "src/standalone".into()),
		vec![AgentCycle {
			length: 2,
			modules: vec![
				"src/standalone".into(),
				"src/other".into(),
			],
		}],
	);

	let result = orient(
		&fake,
		"r1",
		Some("lonely"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let boundary = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::BoundaryViolations);
	let cycles = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::ImportCycles);
	let gate = result
		.signals
		.iter()
		.find(|s| {
			matches!(
				s.code(),
				SignalCode::GatePass
					| SignalCode::GateFail
					| SignalCode::GateIncomplete
			)
		});

	assert!(
		boundary.is_none(),
		"boundary violations must not fire without module context"
	);
	assert!(
		cycles.is_none(),
		"import cycles must not fire without module context"
	);
	assert!(
		gate.is_none(),
		"gate signals must not fire without module context"
	);

	// Also no GATE_NOT_APPLICABLE_TO_FOCUS limit.
	let gate_limit = result
		.limits
		.iter()
		.find(|l| l.code == LimitCode::GateNotApplicableToFocus);
	assert!(
		gate_limit.is_none(),
		"GATE_NOT_APPLICABLE_TO_FOCUS must not fire without module context"
	);
}

// ── 14. MODULE_SUMMARY not emitted at symbol scope ─────────────

#[test]
fn module_summary_not_emitted_at_symbol_scope() {
	let fake = seeded_symbol();

	let result = orient(
		&fake,
		"r1",
		Some("doWork"),
		Budget::Large,
		common::TEST_NOW,
	)
	.unwrap();

	let mod_summary = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::ModuleSummary);
	assert!(
		mod_summary.is_none(),
		"MODULE_SUMMARY must not be emitted at symbol scope"
	);
}
