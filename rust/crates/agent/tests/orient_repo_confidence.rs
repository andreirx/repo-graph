//! Confidence derivation tests.
//!
//! Confidence is derived from raw trust data (call resolution
//! rate, stale-file state, enrichment applicability), NOT from
//! the emitted signals. These tests pin the three tiers through
//! real orient invocations to guard against accidental reuse of
//! the lossy signal list in confidence computation.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentStaleFile, AgentTrustSummary, Budget, Confidence,
};

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
fn high_confidence_when_rate_high_and_clean() {
	let fake = with_trust(
		AgentTrustSummary {
			call_resolution_rate: 0.80,
			resolved_calls: 80,
			unresolved_calls: 20,
			enrichment_applied: true,
			enrichment_eligible: 10,
			enrichment_enriched: 9,
		},
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::High);
}

#[test]
fn medium_confidence_when_rate_in_band() {
	let fake = with_trust(
		AgentTrustSummary {
			call_resolution_rate: 0.30,
			resolved_calls: 30,
			unresolved_calls: 70,
			enrichment_applied: true,
			enrichment_eligible: 10,
			enrichment_enriched: 9,
		},
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::Medium);
}

#[test]
fn low_confidence_when_rate_below_20_percent() {
	let fake = with_trust(
		AgentTrustSummary {
			call_resolution_rate: 0.10,
			resolved_calls: 1,
			unresolved_calls: 9,
			enrichment_applied: true,
			enrichment_eligible: 10,
			enrichment_enriched: 9,
		},
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::Low);
}

#[test]
fn high_rate_degrades_to_medium_when_stale() {
	let fake = with_trust(
		AgentTrustSummary {
			call_resolution_rate: 0.80,
			resolved_calls: 80,
			unresolved_calls: 20,
			enrichment_applied: true,
			enrichment_eligible: 10,
			enrichment_enriched: 9,
		},
		true,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::Medium);
}

#[test]
fn high_rate_degrades_to_medium_when_enrichment_missing() {
	let fake = with_trust(
		AgentTrustSummary {
			call_resolution_rate: 0.80,
			resolved_calls: 80,
			unresolved_calls: 20,
			enrichment_applied: false,
			enrichment_eligible: 10,
			enrichment_enriched: 0,
		},
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::Medium);
}

#[test]
fn high_rate_stays_high_when_enrichment_not_applicable() {
	let fake = with_trust(
		AgentTrustSummary {
			call_resolution_rate: 0.80,
			resolved_calls: 80,
			unresolved_calls: 20,
			enrichment_applied: false,
			enrichment_eligible: 0,
			enrichment_enriched: 0,
		},
		false,
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert_eq!(result.confidence, Confidence::High);
}
