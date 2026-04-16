//! Dead-code reliability gate tests (Rust-43 F1/F3 fix).
//!
//! The dead-code aggregator reads the trust layer's composite
//! `dead_code_reliability` axis and suppresses the DEAD_CODE
//! signal when the level is not High. In the suppressed case,
//! it emits a `DEAD_CODE_UNRELIABLE` limit carrying the trust
//! layer's reason strings verbatim.
//!
//! These tests pin:
//!
//!   1. Reliable path: High reliability → DEAD_CODE signal
//!      fires as before.
//!   2. Low path via missing entrypoints (the exact condition
//!      the repo-graph self-index hit during the spike).
//!   3. Medium path via call-graph reliability propagation
//!      (the next-most-common low-reliability case).
//!   4. Reason pass-through: the trust layer's reason vector
//!      appears verbatim on the emitted limit.
//!   5. JSON shape: the `reasons` field is serialized as a
//!      non-empty array on the limit record.
//!   6. Fallback reason: if the trust layer reports a non-High
//!      level with an empty reason vector (defensive case),
//!      the agent emits the stable fallback string.
//!
//! Motivated by
//! `docs/spikes/2026-04-15-orient-on-repo-graph.md`, which
//! found 86% of symbols reported as dead on the self-index
//! because the Rust indexer produces no framework-liveness
//! inferences and the agent did not gate emission on trust
//! reliability.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentDeadNode, AgentReliabilityAxis, AgentReliabilityLevel,
	AgentTrustSummary, Budget, EnrichmentState, LimitCode, SignalCode,
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

// ── 1. Reliable path ─────────────────────────────────────────────

#[test]
fn dead_code_signal_fires_when_reliability_is_high() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	seed_some_dead_code(&mut fake);
	fake.trust_summaries.insert(
		"snap-1".into(),
		make_trust(AgentReliabilityLevel::High, Vec::new()),
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();

	let has_signal = result
		.signals
		.iter()
		.any(|s| s.code() == SignalCode::DeadCode);
	assert!(
		has_signal,
		"DEAD_CODE signal must fire when reliability is High"
	);
	let has_limit = result
		.limits
		.iter()
		.any(|l| l.code == LimitCode::DeadCodeUnreliable);
	assert!(
		!has_limit,
		"DEAD_CODE_UNRELIABLE limit must NOT appear when reliability is High"
	);
}

// ── 2. Low via missing entrypoints ──────────────────────────────

#[test]
fn dead_code_suppressed_when_reliability_low_missing_entrypoints() {
	// Exact self-index case: no framework-liveness inferences,
	// no entrypoint declarations → trust reports
	// `dead_code.level = LOW` with reason
	// `"missing_entrypoint_declarations"`.
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

	let has_signal = result
		.signals
		.iter()
		.any(|s| s.code() == SignalCode::DeadCode);
	assert!(
		!has_signal,
		"DEAD_CODE signal must be suppressed when reliability is Low"
	);
	let limit = result
		.limits
		.iter()
		.find(|l| l.code == LimitCode::DeadCodeUnreliable)
		.expect("DEAD_CODE_UNRELIABLE limit must appear on Low reliability");
	assert_eq!(
		limit.reasons,
		vec!["missing_entrypoint_declarations".to_string()],
		"reasons must pass through verbatim from trust"
	);
}

// ── 3. Medium via call-graph reliability propagation ────────────

#[test]
fn dead_code_suppressed_when_reliability_medium_call_graph_propagation() {
	// Case: call_graph_reliability is Low (rate < 50%) →
	// dead_code_reliability degrades to Medium with reason
	// "call_graph_reliability_low" (that is the reason string
	// trust produces).
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	seed_some_dead_code(&mut fake);
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 0.30,
			resolved_calls: 30,
			unresolved_calls: 70,
			call_graph_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::Low,
				reasons: vec!["call_resolution_rate=30.0%_below_50%".into()],
			},
			dead_code_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::Medium,
				reasons: vec!["call_graph_reliability_low".into()],
			},
			enrichment_state: EnrichmentState::Ran,
			enrichment_eligible: 100,
			enrichment_enriched: 30,
		},
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();

	let has_signal = result
		.signals
		.iter()
		.any(|s| s.code() == SignalCode::DeadCode);
	assert!(
		!has_signal,
		"DEAD_CODE must be suppressed on Medium reliability too — the gate is \
		 strictly `level == High`, not `level != Low`"
	);
	let limit = result
		.limits
		.iter()
		.find(|l| l.code == LimitCode::DeadCodeUnreliable)
		.expect("DEAD_CODE_UNRELIABLE must appear on Medium");
	assert_eq!(limit.reasons, vec!["call_graph_reliability_low".to_string()]);
}

// ── 4. Reason pass-through with multiple reasons ────────────────

#[test]
fn dead_code_unreliable_limit_preserves_multiple_trust_reasons() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	seed_some_dead_code(&mut fake);
	fake.trust_summaries.insert(
		"snap-1".into(),
		make_trust(
			AgentReliabilityLevel::Low,
			vec![
				"missing_entrypoint_declarations".into(),
				"registry_pattern_suspicion".into(),
				"framework_heavy_suspicion".into(),
			],
		),
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();

	let limit = result
		.limits
		.iter()
		.find(|l| l.code == LimitCode::DeadCodeUnreliable)
		.expect("DEAD_CODE_UNRELIABLE must appear");
	assert_eq!(limit.reasons.len(), 3);
	assert_eq!(limit.reasons[0], "missing_entrypoint_declarations");
	assert_eq!(limit.reasons[1], "registry_pattern_suspicion");
	assert_eq!(limit.reasons[2], "framework_heavy_suspicion");
}

// ── 5. JSON shape ───────────────────────────────────────────────

#[test]
fn dead_code_unreliable_limit_serializes_with_reasons_array() {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	seed_some_dead_code(&mut fake);
	fake.trust_summaries.insert(
		"snap-1".into(),
		make_trust(
			AgentReliabilityLevel::Low,
			vec!["missing_entrypoint_declarations".into()],
		),
	);

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let json = serde_json::to_value(&result).unwrap();

	let limits = json["limits"].as_array().unwrap();
	let dc_limit = limits
		.iter()
		.find(|l| l["code"] == "DEAD_CODE_UNRELIABLE")
		.expect("DEAD_CODE_UNRELIABLE must appear in JSON output");

	assert_eq!(dc_limit["code"], "DEAD_CODE_UNRELIABLE");
	assert!(dc_limit["summary"].is_string());
	let reasons = dc_limit["reasons"]
		.as_array()
		.expect("reasons must serialize as an array");
	assert_eq!(reasons.len(), 1);
	assert_eq!(reasons[0], "missing_entrypoint_declarations");
}

#[test]
fn limits_without_reasons_omit_the_reasons_field_in_json() {
	// Pins the `skip_serializing_if = "Vec::is_empty"`
	// behavior on the `reasons` field: limits that do not
	// carry reasons must not emit the field at all (keeps
	// the JSON output shape unchanged for pre-Rust-43
	// consumers).
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	// No dead code, no gate config, default high reliability.
	// The three static limits (MODULE_DATA_UNAVAILABLE,
	// GATE_NOT_CONFIGURED, COMPLEXITY_UNAVAILABLE) carry no
	// reasons.

	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	let json = serde_json::to_value(&result).unwrap();

	let limits = json["limits"].as_array().unwrap();
	for l in limits {
		// reasons field must be absent (not empty array, not
		// null) for limits that carry no reasons.
		assert!(
			l.get("reasons").is_none(),
			"limit {} must not have a reasons field when empty",
			l["code"]
		);
	}
}

// ── 6. Fallback reason ──────────────────────────────────────────

#[test]
fn dead_code_unreliable_falls_back_to_stable_reason_when_trust_empty() {
	// Defensive: if the trust layer somehow reports
	// `level != High` with an empty reason vector, the agent
	// emits a stable fallback reason so the limit's reason
	// list is always non-empty at that level.
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	seed_some_dead_code(&mut fake);
	fake.trust_summaries.insert(
		"snap-1".into(),
		make_trust(AgentReliabilityLevel::Medium, Vec::new()),
	);

	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();

	let limit = result
		.limits
		.iter()
		.find(|l| l.code == LimitCode::DeadCodeUnreliable)
		.expect("DEAD_CODE_UNRELIABLE must appear");
	assert_eq!(limit.reasons, vec!["dead_code_reliability_not_high".to_string()]);
}
