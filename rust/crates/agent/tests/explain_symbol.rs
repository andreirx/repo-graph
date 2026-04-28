//! Explain use-case tests: symbol target.

mod common;

use common::{FakeAgentStorage, TEST_NOW};
use repo_graph_agent::{
	run_explain, AgentFocusCandidate, AgentFocusKind, AgentSymbolContext,
	Budget, SignalCode, EXPLAIN_COMMAND,
};

fn seed_symbol_repo(fake: &mut FakeAgentStorage) {
	fake.seed_minimal_repo("r1", "my-repo", "snap1");

	// Symbol resolution by name.
	fake.symbol_name_results.insert(
		("snap1".into(), "MyService".into()),
		vec![AgentFocusCandidate {
			stable_key: "r1:src/service.ts:MyService:SYMBOL".into(),
			kind: AgentFocusKind::Symbol,
			file: Some("src/service.ts".into()),
		}],
	);

	// Symbol context.
	fake.symbol_contexts.insert(
		("snap1".into(), "r1:src/service.ts:MyService:SYMBOL".into()),
		AgentSymbolContext {
			file_path: Some("src/service.ts".into()),
			module_path: Some("src/core".into()),
			module_stable_key: Some("r1:src/core:MODULE".into()),
			name: "MyService".into(),
			qualified_name: Some("src/service.ts:MyService".into()),
			subtype: Some("CLASS".into()),
			line_start: Some(10),
		},
	);
}

#[test]
fn explain_symbol_has_identity_section() {
	let mut fake = FakeAgentStorage::new();
	seed_symbol_repo(&mut fake);

	let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW)
		.unwrap();

	assert_eq!(result.command, EXPLAIN_COMMAND);
	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	assert!(
		codes.contains(&SignalCode::ExplainIdentity),
		"must have EXPLAIN_IDENTITY, got: {:?}",
		codes
	);
}

#[test]
fn explain_symbol_has_trust_section() {
	let mut fake = FakeAgentStorage::new();
	seed_symbol_repo(&mut fake);

	let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW)
		.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	assert!(
		codes.contains(&SignalCode::ExplainTrust),
		"must have EXPLAIN_TRUST, got: {:?}",
		codes
	);
}

#[test]
fn explain_symbol_no_file_only_sections() {
	let mut fake = FakeAgentStorage::new();
	seed_symbol_repo(&mut fake);

	let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW)
		.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	// Symbol should NOT have EXPLAIN_IMPORTS, EXPLAIN_SYMBOLS,
	// EXPLAIN_FILES (those are file/path only).
	assert!(
		!codes.contains(&SignalCode::ExplainImports),
		"symbol must not have EXPLAIN_IMPORTS"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainSymbols),
		"symbol must not have EXPLAIN_SYMBOLS"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainFiles),
		"symbol must not have EXPLAIN_FILES"
	);
}

#[test]
fn explain_symbol_no_match_returns_empty() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap1");

	let result = run_explain(&fake, "r1", "nonexistent", Budget::Medium, TEST_NOW)
		.unwrap();

	assert_eq!(result.command, EXPLAIN_COMMAND);
	assert!(!result.focus.resolved);
	assert!(result.signals.is_empty());
}

#[test]
fn explain_symbol_module_context_on_inherited_signals() {
	let mut fake = FakeAgentStorage::new();
	seed_symbol_repo(&mut fake);

	// Seed a cycle involving the owning module.
	fake.cycles_involving_module.insert(
		("snap1".into(), "src/core".into()),
		vec![repo_graph_agent::AgentCycle {
			length: 2,
			modules: vec!["src/core".into(), "src/adapters".into()],
		}],
	);

	let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW)
		.unwrap();

	let cycle_signal = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::ExplainCycles)
		.expect("must have EXPLAIN_CYCLES");

	assert_eq!(
		cycle_signal.scope(),
		repo_graph_agent::SignalScope::ModuleContext,
		"inherited cycle signal must have ModuleContext scope"
	);
}

#[test]
fn explain_symbol_callers_truncated_when_exceeding_cap() {
	let mut fake = FakeAgentStorage::new();
	seed_symbol_repo(&mut fake);

	// Seed 20 callers (cap for medium = 15).
	let callers: Vec<repo_graph_agent::AgentCallerRow> = (0..20)
		.map(|i| repo_graph_agent::AgentCallerRow {
			stable_key: format!("r1:src/c{}.ts:fn{}:SYMBOL", i, i),
			name: format!("fn{}", i),
			file: Some(format!("src/c{}.ts", i)),
			module_path: Some("src/callers".into()),
			module_stable_key: None,
		})
		.collect();
	fake.symbol_callers.insert(
		("snap1".into(), "r1:src/service.ts:MyService:SYMBOL".into()),
		callers,
	);

	let result = run_explain(&fake, "r1", "MyService", Budget::Medium, TEST_NOW)
		.unwrap();

	let callers_signal = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::ExplainCallers)
		.expect("must have EXPLAIN_CALLERS");

	let json = serde_json::to_value(callers_signal.evidence()).unwrap();
	assert_eq!(json["count"], 20);
	assert_eq!(json["items"].as_array().unwrap().len(), 15);
	assert_eq!(json["items_truncated"], true);
	assert_eq!(json["items_omitted_count"], 5);
}
