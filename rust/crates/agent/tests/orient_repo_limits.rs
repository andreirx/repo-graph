//! Limit emission tests.
//!
//! Post-Rust-43A limit-code policy:
//!
//!   - MODULE_DATA_UNAVAILABLE — always emitted by the
//!     module_summary aggregator. Module discovery data is not
//!     queryable through the Rust storage path.
//!   - COMPLEXITY_UNAVAILABLE — always emitted by the
//!     orient_repo static append. The Rust indexer does not
//!     produce cyclomatic complexity measurements.
//!   - GATE_NOT_CONFIGURED — emitted ONLY when the repo has
//!     no active requirement declarations. Present in a default
//!     seeded fake (the fake has no seeded gate_requirements).
//!
//! GATE_UNAVAILABLE was removed in Rust-43A when gate policy was
//! relocated into `repo-graph-gate`. Tests asserting its presence
//! were rewritten to assert GATE_NOT_CONFIGURED or the absence
//! of GATE_NOT_CONFIGURED as appropriate.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{orient, Budget, LimitCode};
use repo_graph_gate::{GateObligation, GateRequirement};

fn seeded_with_requirements() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	fake.gate_requirements.insert(
		"r1".to_string(),
		vec![GateRequirement {
			req_id: "REQ-1".into(),
			version: 1,
			obligations: vec![GateObligation {
				obligation_id: "o1".into(),
				obligation: "core isolated".into(),
				method: "arch_violations".into(),
				target: Some("src/core".into()),
				threshold: None,
				operator: None,
			}],
		}],
	);
	fake
}

#[test]
fn gate_not_configured_limit_emitted_when_no_requirements() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let has = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::GateNotConfigured);
	assert!(
		has,
		"GATE_NOT_CONFIGURED must be present when no requirements exist"
	);
}

#[test]
fn gate_not_configured_limit_absent_when_requirements_exist() {
	let fake = seeded_with_requirements();
	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let has = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::GateNotConfigured);
	assert!(
		!has,
		"GATE_NOT_CONFIGURED must NOT be present when requirements exist; \
		 gate signal covers the outcome instead"
	);
}

#[test]
fn gate_not_configured_summary_mentions_requirement_declarations() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let gate = result
		.limits
		.iter()
		.find(|l| l.code == LimitCode::GateNotConfigured)
		.unwrap();
	assert!(
		gate.summary.contains("requirement declarations"),
		"wording contract: {}",
		gate.summary
	);
}

#[test]
fn repo_orient_emits_complexity_unavailable_limit() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
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

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let has = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::ModuleDataUnavailable);
	assert!(has, "MODULE_DATA_UNAVAILABLE must be present");
}

#[test]
fn repo_orient_emits_language_coverage_partial_limit() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	// LANGUAGE_COVERAGE_PARTIAL is defined but NOT emitted
	// unconditionally. The F5 review identified that emitting
	// it on every repo overclaims (a pure TS repo is fully
	// covered). The limit is deferred until actual evidence of
	// unsupported-language presence is available. Assert it is
	// absent so the deferral sticks.
	let has = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::LanguageCoveragePartial);
	assert!(
		!has,
		"LANGUAGE_COVERAGE_PARTIAL must NOT be emitted unconditionally"
	);
}

#[test]
fn limits_serialize_with_code_and_summary() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let json = serde_json::to_value(&result).unwrap();
	let limits = json["limits"].as_array().unwrap();
	assert!(!limits.is_empty());
	for l in limits {
		assert!(l["code"].is_string());
		assert!(l["summary"].is_string());
	}
}

#[test]
fn limits_fit_small_budget_cap_with_gate_not_configured() {
	// Small cap for limits is 3. Unseeded repo emits exactly
	// 3 limits: MODULE_DATA_UNAVAILABLE, GATE_NOT_CONFIGURED,
	// COMPLEXITY_UNAVAILABLE. No truncation at cap 3.
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.limits.len(), 3);
	assert_eq!(result.limits_truncated, None);
}

#[test]
fn limits_fit_small_budget_cap_without_gate_not_configured() {
	// Seeding requirements removes GATE_NOT_CONFIGURED from
	// the limit list. Remaining limits: MODULE_DATA_UNAVAILABLE,
	// COMPLEXITY_UNAVAILABLE (exactly 2).
	let fake = seeded_with_requirements();
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.limits.len(), 2);
}
