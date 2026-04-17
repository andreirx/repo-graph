//! Explain use-case tests: file target.

mod common;

use common::{FakeAgentStorage, TEST_NOW};
use repo_graph_agent::{
	run_explain, AgentImportEntry, AgentPathResolution, AgentRepoSummary,
	AgentSymbolEntry, Budget, SignalCode, EXPLAIN_COMMAND,
};

fn seed_file_repo(fake: &mut FakeAgentStorage) {
	fake.seed_minimal_repo("r1", "my-repo", "snap1");

	// Path resolution: exact file match.
	fake.path_resolutions.insert(
		("snap1".into(), "src/service.ts".into()),
		AgentPathResolution {
			has_exact_file: true,
			file_stable_key: Some("r1:src/service.ts:FILE".into()),
			has_content_under_prefix: false,
			module_stable_key: None,
		},
	);

	// File summary.
	fake.file_summaries.insert(
		("snap1".into(), "src/service.ts".into()),
		AgentRepoSummary {
			file_count: 1,
			symbol_count: 3,
			languages: vec!["typescript".into()],
		},
	);
}

#[test]
fn explain_file_has_identity_section() {
	let mut fake = FakeAgentStorage::new();
	seed_file_repo(&mut fake);

	let result = run_explain(
		&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW,
	)
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
fn explain_file_has_trust_section() {
	let mut fake = FakeAgentStorage::new();
	seed_file_repo(&mut fake);

	let result = run_explain(
		&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW,
	)
	.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	assert!(
		codes.contains(&SignalCode::ExplainTrust),
		"must have EXPLAIN_TRUST, got: {:?}",
		codes
	);
}

#[test]
fn explain_file_no_path_only_sections() {
	let mut fake = FakeAgentStorage::new();
	seed_file_repo(&mut fake);

	let result = run_explain(
		&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW,
	)
	.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	// File should NOT have callers, callees, cycles, boundary,
	// gate, files sections.
	assert!(
		!codes.contains(&SignalCode::ExplainCallers),
		"file must not have EXPLAIN_CALLERS"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainCallees),
		"file must not have EXPLAIN_CALLEES"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainCycles),
		"file must not have EXPLAIN_CYCLES"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainBoundary),
		"file must not have EXPLAIN_BOUNDARY"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainGate),
		"file must not have EXPLAIN_GATE"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainFiles),
		"file must not have EXPLAIN_FILES"
	);
}

#[test]
fn explain_file_has_imports_when_present() {
	let mut fake = FakeAgentStorage::new();
	seed_file_repo(&mut fake);

	fake.file_imports.insert(
		("snap1".into(), "src/service.ts".into()),
		vec![
			AgentImportEntry { target_file: "src/model.ts".into() },
			AgentImportEntry { target_file: "src/utils.ts".into() },
		],
	);

	let result = run_explain(
		&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW,
	)
	.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	assert!(
		codes.contains(&SignalCode::ExplainImports),
		"must have EXPLAIN_IMPORTS when imports exist"
	);
}

#[test]
fn explain_file_has_symbols_when_present() {
	let mut fake = FakeAgentStorage::new();
	seed_file_repo(&mut fake);

	fake.symbols_in_file.insert(
		("snap1".into(), "src/service.ts".into()),
		vec![
			AgentSymbolEntry {
				stable_key: "r1:src/service.ts:foo:SYMBOL".into(),
				name: "foo".into(),
				qualified_name: None,
				subtype: Some("FUNCTION".into()),
				line_start: Some(1),
			},
		],
	);

	let result = run_explain(
		&fake, "r1", "src/service.ts", Budget::Medium, TEST_NOW,
	)
	.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	assert!(
		codes.contains(&SignalCode::ExplainSymbols),
		"must have EXPLAIN_SYMBOLS when symbols exist"
	);
}

#[test]
fn explain_file_no_match_returns_empty() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap1");

	let result = run_explain(
		&fake, "r1", "nonexistent.ts", Budget::Medium, TEST_NOW,
	)
	.unwrap();

	assert_eq!(result.command, EXPLAIN_COMMAND);
	assert!(!result.focus.resolved);
	assert!(result.signals.is_empty());
}
