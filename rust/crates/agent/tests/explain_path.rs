//! Explain use-case tests: path target.

mod common;

use common::{FakeAgentStorage, TEST_NOW};
use repo_graph_agent::{
	run_explain, AgentFileEntry, AgentPathResolution, AgentRepoSummary,
	Budget, SignalCode, EXPLAIN_COMMAND,
};

fn seed_path_repo(fake: &mut FakeAgentStorage) {
	fake.seed_minimal_repo("r1", "my-repo", "snap1");

	// Path resolution: prefix match.
	fake.path_resolutions.insert(
		("snap1".into(), "src/core".into()),
		AgentPathResolution {
			has_exact_file: false,
			file_stable_key: None,
			has_content_under_prefix: true,
			module_stable_key: Some("r1:src/core:MODULE".into()),
		},
	);

	// Path summary.
	fake.path_summaries.insert(
		("snap1".into(), "src/core".into()),
		AgentRepoSummary {
			file_count: 5,
			symbol_count: 20,
			languages: vec!["typescript".into()],
		},
	);
}

#[test]
fn explain_path_has_identity_section() {
	let mut fake = FakeAgentStorage::new();
	seed_path_repo(&mut fake);

	let result = run_explain(&fake, "r1", "src/core", Budget::Medium, TEST_NOW)
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
fn explain_path_has_trust_section() {
	let mut fake = FakeAgentStorage::new();
	seed_path_repo(&mut fake);

	let result = run_explain(&fake, "r1", "src/core", Budget::Medium, TEST_NOW)
		.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	assert!(
		codes.contains(&SignalCode::ExplainTrust),
		"must have EXPLAIN_TRUST, got: {:?}",
		codes
	);
}

#[test]
fn explain_path_no_symbol_only_sections() {
	let mut fake = FakeAgentStorage::new();
	seed_path_repo(&mut fake);

	let result = run_explain(&fake, "r1", "src/core", Budget::Medium, TEST_NOW)
		.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	// Path should NOT have callers, callees, imports, symbols.
	assert!(
		!codes.contains(&SignalCode::ExplainCallers),
		"path must not have EXPLAIN_CALLERS"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainCallees),
		"path must not have EXPLAIN_CALLEES"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainImports),
		"path must not have EXPLAIN_IMPORTS"
	);
	assert!(
		!codes.contains(&SignalCode::ExplainSymbols),
		"path must not have EXPLAIN_SYMBOLS"
	);
}

#[test]
fn explain_path_has_files_when_present() {
	let mut fake = FakeAgentStorage::new();
	seed_path_repo(&mut fake);

	fake.files_in_path.insert(
		("snap1".into(), "src/core".into()),
		vec![
			AgentFileEntry {
				path: "src/core/model.ts".into(),
				symbol_count: 5,
				is_test: false,
			},
			AgentFileEntry {
				path: "src/core/service.ts".into(),
				symbol_count: 10,
				is_test: false,
			},
		],
	);

	let result = run_explain(&fake, "r1", "src/core", Budget::Medium, TEST_NOW)
		.unwrap();

	let codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	assert!(
		codes.contains(&SignalCode::ExplainFiles),
		"must have EXPLAIN_FILES when files exist"
	);
}

#[test]
fn explain_path_files_truncated_at_cap() {
	let mut fake = FakeAgentStorage::new();
	seed_path_repo(&mut fake);

	// Seed 20 files (cap for medium = 15).
	let files: Vec<AgentFileEntry> = (0..20)
		.map(|i| AgentFileEntry {
			path: format!("src/core/f{}.ts", i),
			symbol_count: 1,
			is_test: false,
		})
		.collect();
	fake.files_in_path.insert(
		("snap1".into(), "src/core".into()),
		files,
	);

	let result = run_explain(&fake, "r1", "src/core", Budget::Medium, TEST_NOW)
		.unwrap();

	let files_signal = result
		.signals
		.iter()
		.find(|s| s.code() == SignalCode::ExplainFiles)
		.expect("must have EXPLAIN_FILES");

	let json = serde_json::to_value(files_signal.evidence()).unwrap();
	assert_eq!(json["count"], 20);
	assert_eq!(json["items"].as_array().unwrap().len(), 15);
	assert_eq!(json["items_truncated"], true);
	assert_eq!(json["items_omitted_count"], 5);
}

#[test]
fn explain_path_no_match_returns_empty() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap1");

	let result = run_explain(
		&fake, "r1", "nonexistent/path", Budget::Medium, TEST_NOW,
	)
	.unwrap();

	assert_eq!(result.command, EXPLAIN_COMMAND);
	assert!(!result.focus.resolved);
	assert!(result.signals.is_empty());
}
