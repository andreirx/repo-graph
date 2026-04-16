//! Confidence derivation tests.
//!
//! Confidence is derived from raw trust data (call resolution
//! rate, stale-file state, enrichment state), NOT from the
//! emitted signals. These tests pin the tiers through real
//! orient invocations to guard against accidental reuse of
//! the lossy signal list in confidence computation.
//!
//! Rust-43 F2 fix: enrichment is modeled as a three-state enum
//! (`EnrichmentState { Ran, NotApplicable, NotRun }`). Only
//! `NotRun` penalizes confidence on the enrichment axis. Tests
//! exercise all three states.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentReliabilityAxis, AgentReliabilityLevel, AgentStaleFile,
	AgentTrustSummary, Budget, Confidence, EnrichmentState,
};

fn reliable_axis() -> AgentReliabilityAxis {
	AgentReliabilityAxis {
		level: AgentReliabilityLevel::High,
		reasons: Vec::new(),
	}
}

fn make_trust(
	rate: f64,
	enrichment_state: EnrichmentState,
	eligible: u64,
	enriched: u64,
) -> AgentTrustSummary {
	// Rust-43 F1 note: these confidence tests set the
	// reliability axes to High unconditionally. This keeps
	// the dead-code aggregator from interfering with the
	// confidence derivation under test. Reliability-gated
	// dead-code behavior is covered by its own test file.
	AgentTrustSummary {
		call_resolution_rate: rate,
		resolved_calls: (rate * 100.0) as u64,
		unresolved_calls: ((1.0 - rate) * 100.0) as u64,
		call_graph_reliability: reliable_axis(),
		dead_code_reliability: reliable_axis(),
		enrichment_state,
		enrichment_eligible: eligible,
		enrichment_enriched: enriched,
	}
}

fn with_trust(trust: AgentTrustSummary, stale: bool) -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	fake.trust_summaries.insert("snap-1".into(), trust);
	if stale {
		fake.stale_files.insert(
			"snap-1".into(),
			vec![AgentStaleFile { path: "src/a.rs".into() }],
		);
	}
	fake
}

#[test]
fn high_confidence_when_rate_high_and_enrichment_ran() {
	let fake = with_trust(
		make_trust(0.80, EnrichmentState::Ran, 10, 9),
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::High);
}

#[test]
fn medium_confidence_when_rate_in_band() {
	let fake = with_trust(
		make_trust(0.30, EnrichmentState::Ran, 10, 9),
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::Medium);
}

#[test]
fn low_confidence_when_rate_below_20_percent() {
	let fake = with_trust(
		make_trust(0.10, EnrichmentState::Ran, 10, 9),
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::Low);
}

#[test]
fn high_rate_degrades_to_medium_when_stale() {
	let fake = with_trust(
		make_trust(0.80, EnrichmentState::Ran, 10, 9),
		true,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::Medium);
}

#[test]
fn high_rate_degrades_to_medium_when_enrichment_not_run() {
	// Rust-43 F2 regression: the agent must distinguish
	// "phase never ran" from "phase ran with nothing to do".
	// This was the masked bug on the self-index spike.
	let fake = with_trust(
		make_trust(0.80, EnrichmentState::NotRun, 0, 0),
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::Medium);
}

#[test]
fn high_rate_stays_high_when_enrichment_not_applicable() {
	// `NotApplicable` = phase executed, zero eligible edges.
	// No penalty on the enrichment axis.
	let fake = with_trust(
		make_trust(0.80, EnrichmentState::NotApplicable, 0, 0),
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::High);
}

#[test]
fn high_rate_stays_high_when_enrichment_ran_with_zero_enriched() {
	// New coverage: `Ran` with `enriched == 0` means the
	// phase executed and resolved nothing. Still `Ran`, still
	// no penalty. The previous Rust-42 rule would have
	// incorrectly degraded this to Medium.
	let fake = with_trust(
		make_trust(0.80, EnrichmentState::Ran, 10, 0),
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::High);
}
