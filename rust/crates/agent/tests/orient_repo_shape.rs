//! Envelope shape + focus resolution tests for repo-level orient.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, Budget, OrientError, ORIENT_COMMAND, ORIENT_SCHEMA,
};

#[test]
fn repo_orient_envelope_has_locked_schema_and_command() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();

	assert_eq!(result.schema, ORIENT_SCHEMA);
	assert_eq!(result.schema, "rgr.agent.v1");
	assert_eq!(result.command, ORIENT_COMMAND);
	assert_eq!(result.command, "orient");
	assert_eq!(result.repo, "my-repo");
	assert_eq!(result.snapshot, "snap-1");
}

#[test]
fn repo_orient_focus_resolves_to_repo_without_input() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();

	assert!(result.focus.resolved);
	assert_eq!(
		result.focus.resolved_kind,
		Some(repo_graph_agent::ResolvedKind::Repo)
	);
	assert_eq!(result.focus.resolved_key.as_deref(), Some("r1"));
	assert!(result.focus.input.is_none());
	assert!(result.focus.candidates.is_empty());
	assert!(result.focus.reason.is_none());
}

#[test]
fn repo_orient_serializes_to_contract_shape() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	let json = serde_json::to_value(&result).unwrap();

	assert_eq!(json["schema"], "rgr.agent.v1");
	assert_eq!(json["command"], "orient");
	assert_eq!(json["repo"], "my-repo");
	assert_eq!(json["snapshot"], "snap-1");

	// Focus shape per contract.
	assert_eq!(json["focus"]["resolved"], true);
	assert_eq!(json["focus"]["resolved_kind"], "repo");
	assert!(json["focus"].get("input").is_none());
	assert!(json["focus"].get("candidates").is_none());

	// Signals and limits are always arrays, even when empty.
	assert!(json["signals"].is_array());
	assert!(json["limits"].is_array());

	// truncated is present at top level.
	assert!(json["truncated"].is_boolean());
}

#[test]
fn focus_arg_returns_error_not_focus_failure() {
	// Rust-42 does not implement module/path/symbol focus.
	// The contract requires this to surface as an error, NOT as
	// a `focus.resolved = false` degraded response.
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let err = orient(&fake, "r1", Some("src/core"), Budget::Small, common::TEST_NOW).unwrap_err();
	match err {
		OrientError::FocusNotImplementedYet { focus } => {
			assert_eq!(focus, "src/core");
		}
		other => panic!("expected FocusNotImplementedYet, got {:?}", other),
	}
}

#[test]
fn missing_repo_returns_no_repo_error() {
	let fake = FakeAgentStorage::new();
	let err = orient(&fake, "missing", None, Budget::Small, common::TEST_NOW).unwrap_err();
	match err {
		OrientError::NoRepo { repo_uid } => assert_eq!(repo_uid, "missing"),
		other => panic!("expected NoRepo, got {:?}", other),
	}
}

#[test]
fn missing_snapshot_returns_no_snapshot_error() {
	let mut fake = FakeAgentStorage::new();
	// Insert repo but NOT a snapshot.
	fake.repos.insert(
		"r1".into(),
		repo_graph_agent::AgentRepo {
			repo_uid: "r1".into(),
			name: "my-repo".into(),
		},
	);
	let err = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap_err();
	match err {
		OrientError::NoSnapshot { repo_uid } => assert_eq!(repo_uid, "r1"),
		other => panic!("expected NoSnapshot, got {:?}", other),
	}
}

#[test]
fn storage_error_propagates_through_orient() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	*fake.force_error_on.borrow_mut() = Some("find_module_cycles");

	let err = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap_err();
	match err {
		OrientError::Storage(e) => {
			assert_eq!(e.operation, "find_module_cycles");
		}
		other => panic!("expected Storage error, got {:?}", other),
	}
}
