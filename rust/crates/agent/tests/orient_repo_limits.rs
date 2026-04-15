//! Limit emission tests.
//!
//! The repo-level orient pipeline MUST emit three static limit
//! codes in every response (Rust-42 scope):
//!
//!   - MODULE_DATA_UNAVAILABLE (from module_summary aggregator)
//!   - GATE_UNAVAILABLE (from orient_repo static append)
//!   - COMPLEXITY_UNAVAILABLE (from orient_repo static append)
//!
//! These are unconditional because the underlying capabilities
//! are unavailable on every Rust-42 response regardless of
//! snapshot contents.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{orient, Budget, LimitCode};

#[test]
fn repo_orient_emits_gate_unavailable_limit() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large).unwrap();
	let has_gate = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::GateUnavailable);
	assert!(has_gate, "GATE_UNAVAILABLE must be present in every response");
}

#[test]
fn gate_unavailable_summary_mentions_shared_crate() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large).unwrap();
	let gate = result
		.limits
		.iter()
		.find(|l| l.code == LimitCode::GateUnavailable)
		.unwrap();
	assert!(
		gate.summary.contains("shared library crate"),
		"wording contract: {}",
		gate.summary
	);
}

#[test]
fn repo_orient_emits_complexity_unavailable_limit() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large).unwrap();
	let has = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::ComplexityUnavailable);
	assert!(has, "COMPLEXITY_UNAVAILABLE must be present");
}

#[test]
fn repo_orient_emits_module_data_unavailable_limit() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large).unwrap();
	let has = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::ModuleDataUnavailable);
	assert!(has, "MODULE_DATA_UNAVAILABLE must be present");
}

#[test]
fn limits_serialize_with_code_and_summary() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large).unwrap();
	let json = serde_json::to_value(&result).unwrap();
	let limits = json["limits"].as_array().unwrap();
	assert!(!limits.is_empty());
	for l in limits {
		assert!(l["code"].is_string());
		assert!(l["summary"].is_string());
	}
}

#[test]
fn limits_truncated_on_small_budget() {
	// Small cap for limits is 3. Rust-42 emits exactly 3 limits
	// unconditionally, so no truncation on Small.
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
	assert_eq!(result.limits.len(), 3);
	assert_eq!(result.limits_truncated, None);
}
