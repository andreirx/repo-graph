//! Ranking and sort-order tests for repo-level orient.
//!
//! These tests verify that the final signal list is ordered by
//! severity then category, and that ranks are 1-based dense
//! integers regardless of construction order inside the
//! aggregators.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentBoundaryDeclaration, AgentCycle, AgentDeadNode,
	AgentImportEdge, AgentReliabilityAxis, AgentReliabilityLevel,
	AgentTrustSummary, Budget, EnrichmentState, Severity, SignalCategory,
};

fn seed_with_all_signals() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");

	// BOUNDARY_VIOLATIONS (High severity)
	fake.boundary_declarations.insert(
		"r1".into(),
		vec![AgentBoundaryDeclaration {
			source_module: "src/core".into(),
			forbidden_target: "src/adapters".into(),
			reason: None,
		}],
	);
	fake.imports_between_paths.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![AgentImportEdge {
			source_file: "src/core/a.rs".into(),
			target_file: "src/adapters/b.rs".into(),
		}],
	);

	// TRUST_LOW_RESOLUTION (Medium, Trust category).
	// Reliability axes are seeded `High` so DEAD_CODE still
	// fires in this ranking fixture (the fake does not mirror
	// the real trust crate's rules — see common::high_confidence_trust).
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 0.10,
			resolved_calls: 1,
			unresolved_calls: 9,
			call_graph_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			dead_code_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			enrichment_state: EnrichmentState::Ran,
			enrichment_eligible: 10,
			enrichment_enriched: 9,
		},
	);

	// IMPORT_CYCLES (Medium, Structure)
	fake.cycles.insert(
		"snap-1".into(),
		vec![AgentCycle { length: 2, modules: vec!["m1".into(), "m2".into()] }],
	);

	// DEAD_CODE (Medium, Structure)
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

	fake
}

#[test]
fn ranks_are_dense_and_one_based() {
	let fake = seed_with_all_signals();
	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();
	for (i, sig) in result.signals.iter().enumerate() {
		assert_eq!(sig.rank(), (i + 1) as u32, "rank must be i+1");
	}
}

#[test]
fn severity_order_is_high_medium_low() {
	let fake = seed_with_all_signals();
	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();

	let mut prev: Option<Severity> = None;
	for sig in &result.signals {
		if let Some(p) = prev {
			// Severity must be non-increasing when iterating.
			assert!(
				sig.severity() <= p,
				"severity must not increase: saw {:?} after {:?}",
				sig.severity(),
				p
			);
		}
		prev = Some(sig.severity());
	}
}

#[test]
fn first_signal_is_boundary_violations_high() {
	let fake = seed_with_all_signals();
	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();

	let first = result.signals.first().expect("at least one signal");
	assert_eq!(first.severity(), Severity::High);
	assert_eq!(first.category(), SignalCategory::Boundary);
	assert_eq!(first.code().as_str(), "BOUNDARY_VIOLATIONS");
}

#[test]
fn within_medium_tier_trust_precedes_structure() {
	let fake = seed_with_all_signals();
	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();

	let medium_signals: Vec<_> = result
		.signals
		.iter()
		.filter(|s| s.severity() == Severity::Medium)
		.collect();
	assert!(
		!medium_signals.is_empty(),
		"fixture should produce medium-tier signals"
	);

	// Within the Medium tier, Trust (ordinal 2) must precede
	// Structure (ordinal 3).
	let mut seen_structure = false;
	for sig in &medium_signals {
		if sig.category() == SignalCategory::Structure {
			seen_structure = true;
		}
		if sig.category() == SignalCategory::Trust {
			assert!(
				!seen_structure,
				"Trust must precede Structure within the same severity tier"
			);
		}
	}
}

#[test]
fn import_cycles_ranks_before_dead_code_within_same_tier() {
	// F6: both are (Medium, Structure) — tier_priority breaks
	// the tie. IMPORT_CYCLES has priority 0, DEAD_CODE has
	// priority 1.
	let fake = seed_with_all_signals();
	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();

	let structure_medium: Vec<_> = result
		.signals
		.iter()
		.filter(|s| {
			s.severity() == Severity::Medium
				&& s.category() == SignalCategory::Structure
		})
		.collect();
	assert!(
		structure_medium.len() >= 2,
		"fixture must produce both IMPORT_CYCLES and DEAD_CODE"
	);
	let cycles_rank = structure_medium
		.iter()
		.find(|s| s.code().as_str() == "IMPORT_CYCLES")
		.expect("IMPORT_CYCLES must be present")
		.rank();
	let dead_rank = structure_medium
		.iter()
		.find(|s| s.code().as_str() == "DEAD_CODE")
		.expect("DEAD_CODE must be present")
		.rank();
	assert!(
		cycles_rank < dead_rank,
		"IMPORT_CYCLES (rank {}) must rank before DEAD_CODE (rank {})",
		cycles_rank,
		dead_rank,
	);
}

#[test]
fn informational_signals_land_at_the_tail() {
	let fake = seed_with_all_signals();
	let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();

	let tail_two: Vec<_> = result
		.signals
		.iter()
		.rev()
		.take(2)
		.map(|s| s.category())
		.collect();
	for cat in tail_two {
		assert_eq!(
			cat,
			SignalCategory::Informational,
			"MODULE_SUMMARY and SNAPSHOT_INFO must be at the tail"
		);
	}
}
