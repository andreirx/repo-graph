//! Dead-code surface withdrawal regression tests.
//!
//! The `rmap dead` surface is withdrawn. These tests verify that:
//!
//!   1. No DEAD_CODE signal is emitted regardless of reliability.
//!   2. No DEAD_CODE_UNRELIABLE limit is emitted.
//!   3. Internal substrate (dead_code_reliability axis, storage
//!      queries) is preserved but not surfaced.
//!
//! See `docs/TECH-DEBT.md` for reintroduction conditions.
//!
//! Historical context: These tests originally verified the
//! reliability gate introduced in Rust-43 F1/F3 after the spike
//! `docs/spikes/2026-04-15-orient-on-repo-graph.md` found 86% of
//! symbols reported as dead on repo-graph self-index. The surface
//! was subsequently withdrawn entirely pending coverage-backed
//! reintroduction.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentDeadNode, AgentReliabilityAxis, AgentReliabilityLevel,
	AgentTrustSummary, Budget, EnrichmentState,
};

fn seed_some_dead_code(fake: &mut FakeAgentStorage) {
	fake.dead_nodes.insert(
		"snap-1".into(),
		vec![AgentDeadNode {
			stable_key: "r1:src/foo.rs:SYMBOL:unused".into(),
			symbol: "unused".into(),
			kind: "SYMBOL".into(),
			file: Some("src/foo.rs".into()),
			line_count: Some(5),
			is_test: false,
		}],
	);
}

fn make_trust(
	dead_code_level: AgentReliabilityLevel,
	dead_code_reasons: Vec<String>,
) -> AgentTrustSummary {
	AgentTrustSummary {
		call_resolution_rate: 0.90,
		resolved_calls: 90,
		unresolved_calls: 10,
		call_graph_reliability: AgentReliabilityAxis {
			level: AgentReliabilityLevel::High,
			reasons: Vec::new(),
		},
		dead_code_reliability: AgentReliabilityAxis {
			level: dead_code_level,
			reasons: dead_code_reasons,
		},
		enrichment_state: EnrichmentState::Ran,
		enrichment_eligible: 10,
		enrichment_enriched: 9,
	}
}

// ── Surface withdrawal: no dead-code signals ─────────────────────

#[test]
fn no_dead_code_signal_even_when_reliability_high_and_dead_code_exists() {
	// Pre-withdrawal, this would emit DEAD_CODE.
	// Post-withdrawal, the surface is suppressed entirely.
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	seed_some_dead_code(&mut fake);
	fake.trust_summaries.insert(
		"snap-1".into(),
		make_trust(AgentReliabilityLevel::High, Vec::new()),
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();

	// Assert: no dead-code related signal codes appear.
	let signal_codes: Vec<_> = result.signals.iter().map(|s| s.code()).collect();
	for code in &signal_codes {
		let code_str = code.as_str();
		assert!(
			!code_str.contains("DEAD"),
			"No dead-code signal should appear, found: {}",
			code_str
		);
	}
}

#[test]
fn no_dead_code_unreliable_limit_even_when_reliability_is_low() {
	// Pre-withdrawal, this would emit DEAD_CODE_UNRELIABLE limit.
	// Post-withdrawal, no dead-code limits appear.
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	seed_some_dead_code(&mut fake);
	fake.trust_summaries.insert(
		"snap-1".into(),
		make_trust(
			AgentReliabilityLevel::Low,
			vec!["missing_entrypoint_declarations".to_string()],
		),
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();

	// Assert: no dead-code related limit codes appear.
	for limit in &result.limits {
		let code_str = limit.code.as_str();
		assert!(
			!code_str.contains("DEAD"),
			"No dead-code limit should appear, found: {}",
			code_str
		);
	}
}

// ── JSON output shape ────────────────────────────────────────────

#[test]
fn json_output_contains_no_dead_code_vocabulary() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	seed_some_dead_code(&mut fake);
	fake.trust_summaries.insert(
		"snap-1".into(),
		make_trust(AgentReliabilityLevel::High, Vec::new()),
	);

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let json = serde_json::to_string(&result).unwrap();

	// The JSON output should not contain DEAD_CODE anywhere.
	assert!(
		!json.contains("DEAD_CODE"),
		"JSON output must not contain DEAD_CODE vocabulary"
	);
	assert!(
		!json.contains("dead_code"),
		"JSON output must not contain dead_code vocabulary"
	);
}
