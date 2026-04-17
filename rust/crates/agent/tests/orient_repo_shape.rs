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
	// Repo focus does not resolve to a graph node — both
	// resolved_key and resolved_path must be None. The repo
	// identity is on the envelope's top-level `repo` field.
	assert!(result.focus.resolved_key.is_none());
	assert!(result.focus.resolved_path.is_none());
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

	// Focus shape per contract. Repo focus omits input, key,
	// path, and candidates.
	assert_eq!(json["focus"]["resolved"], true);
	assert_eq!(json["focus"]["resolved_kind"], "repo");
	assert!(json["focus"].get("input").is_none());
	assert!(json["focus"].get("resolved_key").is_none(), "repo focus must not have resolved_key");
	assert!(json["focus"].get("resolved_path").is_none(), "repo focus must not have resolved_path");
	assert!(json["focus"].get("candidates").is_none());

	// Signals and limits are always arrays, even when empty.
	assert!(json["signals"].is_array());
	assert!(json["limits"].is_array());

	// truncated is present at top level.
	assert!(json["truncated"].is_boolean());
}

#[test]
fn focus_arg_with_unknown_path_returns_no_match() {
	// Rust-44 replaced the FocusNotImplementedYet error with
	// real focus resolution. An unknown path now returns a valid
	// OrientResult with focus.resolved = false and reason =
	// no_match — this is a domain outcome, not an error.
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", Some("src/core"), Budget::Small, common::TEST_NOW).unwrap();
	assert!(!result.focus.resolved, "unknown path must not resolve");
	assert_eq!(
		result.focus.reason,
		Some(repo_graph_agent::FocusFailureReason::NoMatch)
	);
	assert_eq!(result.focus.input.as_deref(), Some("src/core"));
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
