//! One test per signal code that the repo-level orient pipeline
//! can emit. Tests assert both that the signal appears (or does
//! not) under the right conditions, and that its evidence fields
//! match the input data exactly.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentBoundaryDeclaration, AgentCycle, AgentImportEdge,
	AgentReliabilityAxis, AgentReliabilityLevel, AgentRepoSummary,
	AgentStaleFile, AgentTrustSummary, Budget, EnrichmentState,
	SignalCode, SignalEvidence,
};

fn seeded() -> FakeAgentStorage {
	let mut fake = FakeAgentStorage::new();
	fake.seed_minimal_repo("r1", "my-repo", "snap-1");
	fake
}

fn find_signal<'a>(
	result: &'a repo_graph_agent::OrientResult,
	code: SignalCode,
) -> Option<&'a repo_graph_agent::Signal> {
	result.signals.iter().find(|s| s.code() == code)
}

// ── SNAPSHOT_INFO ───────────────────────────────────────────────

#[test]
fn snapshot_info_is_always_emitted() {
	let fake = seeded();
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::SnapshotInfo)
		.expect("SNAPSHOT_INFO must be emitted");
	match sig.evidence() {
		SignalEvidence::SnapshotInfo(ev) => {
			assert_eq!(ev.snapshot_uid, "snap-1");
			assert_eq!(ev.scope, "full");
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── MODULE_SUMMARY ──────────────────────────────────────────────

#[test]
fn module_summary_is_always_emitted_with_db_counts() {
	let mut fake = seeded();
	fake.repo_summaries.insert(
		"snap-1".into(),
		AgentRepoSummary {
			file_count: 42,
			symbol_count: 307,
			languages: vec!["rust".into(), "typescript".into()],
		},
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::ModuleSummary)
		.expect("MODULE_SUMMARY must be emitted");
	match sig.evidence() {
		SignalEvidence::ModuleSummary(ev) => {
			assert_eq!(ev.file_count, 42);
			assert_eq!(ev.symbol_count, 307);
			assert_eq!(ev.languages, vec!["rust", "typescript"]);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── IMPORT_CYCLES ───────────────────────────────────────────────

#[test]
fn import_cycles_not_emitted_when_none_exist() {
	let fake = seeded();
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert!(
		find_signal(&result, SignalCode::ImportCycles).is_none(),
		"IMPORT_CYCLES must be omitted on zero-count"
	);
}

#[test]
fn import_cycles_emitted_with_top_3_evidence() {
	let mut fake = seeded();
	fake.cycles.insert(
		"snap-1".into(),
		vec![
			AgentCycle { length: 2, modules: vec!["m1".into(), "m2".into()] },
			AgentCycle { length: 3, modules: vec!["m3".into(), "m4".into(), "m5".into()] },
			AgentCycle { length: 2, modules: vec!["m6".into(), "m7".into()] },
			AgentCycle { length: 4, modules: vec!["m8".into(), "m9".into(), "m10".into(), "m11".into()] },
		],
	);
	let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::ImportCycles)
		.expect("IMPORT_CYCLES must be emitted when count > 0");
	match sig.evidence() {
		SignalEvidence::ImportCycles(ev) => {
			assert_eq!(ev.cycle_count, 4, "count is the full cycle total");
			assert_eq!(ev.cycles.len(), 3, "evidence exposes top 3 only");
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── DEAD_CODE ───────────────────────────────────────────────────
// Tests removed: dead-code surface withdrawn.
// See orient_repo_dead_code_reliability.rs for withdrawal regression tests.

// ── BOUNDARY_VIOLATIONS ─────────────────────────────────────────

#[test]
fn boundary_violations_not_emitted_when_no_declarations() {
	let fake = seeded();
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert!(find_signal(&result, SignalCode::BoundaryViolations).is_none());
}

#[test]
fn boundary_violations_not_emitted_when_declarations_have_no_edges() {
	let mut fake = seeded();
	fake.boundary_declarations.insert(
		"r1".into(),
		vec![AgentBoundaryDeclaration {
			source_module: "src/core".into(),
			forbidden_target: "src/adapters".into(),
			reason: None,
		}],
	);
	// No edges seeded — declaration exists but nothing violates.
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert!(find_signal(&result, SignalCode::BoundaryViolations).is_none());
}

#[test]
fn boundary_violations_dedupe_by_source_and_target() {
	// Regression for P2: if active declarations contain two
	// rows naming the same (source_module, forbidden_target),
	// edges must be counted once, not twice.
	let mut fake = seeded();
	fake.boundary_declarations.insert(
		"r1".into(),
		vec![
			AgentBoundaryDeclaration {
				source_module: "src/core".into(),
				forbidden_target: "src/adapters".into(),
				reason: Some("first".into()),
			},
			AgentBoundaryDeclaration {
				source_module: "src/core".into(),
				forbidden_target: "src/adapters".into(),
				reason: Some("duplicate".into()),
			},
		],
	);
	fake.imports_between_paths.insert(
		("snap-1".into(), "src/core".into(), "src/adapters".into()),
		vec![AgentImportEdge {
			source_file: "src/core/a.rs".into(),
			target_file: "src/adapters/b.rs".into(),
		}],
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::BoundaryViolations)
		.expect("BOUNDARY_VIOLATIONS must fire");
	match sig.evidence() {
		SignalEvidence::BoundaryViolations(ev) => {
			assert_eq!(
				ev.violation_count, 1,
				"duplicate declaration must not double-count edges"
			);
			assert_eq!(
				ev.top_violations.len(),
				1,
				"duplicate rule must appear once in top_violations"
			);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

#[test]
fn boundary_violations_emitted_when_edges_cross_forbidden_path() {
	let mut fake = seeded();
	fake.boundary_declarations.insert(
		"r1".into(),
		vec![AgentBoundaryDeclaration {
			source_module: "src/core".into(),
			forbidden_target: "src/adapters".into(),
			reason: Some("core must not depend on adapters".into()),
		}],
	);
	fake.imports_between_paths.insert(
		(
			"snap-1".into(),
			"src/core".into(),
			"src/adapters".into(),
		),
		vec![
			AgentImportEdge {
				source_file: "src/core/a.rs".into(),
				target_file: "src/adapters/b.rs".into(),
			},
			AgentImportEdge {
				source_file: "src/core/c.rs".into(),
				target_file: "src/adapters/d.rs".into(),
			},
		],
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::BoundaryViolations)
		.expect("BOUNDARY_VIOLATIONS must fire when edges exist");
	match sig.evidence() {
		SignalEvidence::BoundaryViolations(ev) => {
			assert_eq!(ev.violation_count, 2);
			assert_eq!(ev.top_violations.len(), 1);
			assert_eq!(ev.top_violations[0].source_module, "src/core");
			assert_eq!(ev.top_violations[0].target_module, "src/adapters");
			assert_eq!(ev.top_violations[0].edge_count, 2);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── TRUST_LOW_RESOLUTION ────────────────────────────────────────

#[test]
fn trust_low_resolution_not_emitted_when_rate_high() {
	let fake = seeded(); // default is 0.90
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert!(find_signal(&result, SignalCode::TrustLowResolution).is_none());
}

#[test]
fn trust_low_resolution_emitted_below_threshold() {
	let mut fake = seeded();
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
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::TrustLowResolution)
		.expect("TRUST_LOW_RESOLUTION must fire below 0.20");
	match sig.evidence() {
		SignalEvidence::TrustLowResolution(ev) => {
			assert!((ev.resolution_rate - 0.10).abs() < 1e-9);
			assert_eq!(ev.resolved_count, 1);
			assert_eq!(ev.total_count, 10);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── TRUST_STALE_SNAPSHOT ────────────────────────────────────────

#[test]
fn trust_stale_snapshot_not_emitted_when_stale_list_empty() {
	let fake = seeded();
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert!(find_signal(&result, SignalCode::TrustStaleSnapshot).is_none());
}

#[test]
fn trust_stale_snapshot_emitted_with_exact_wording() {
	let mut fake = seeded();
	fake.stale_files.insert(
		"snap-1".into(),
		vec![
			AgentStaleFile { path: "src/a.rs".into() },
			AgentStaleFile { path: "src/b.rs".into() },
		],
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::TrustStaleSnapshot)
		.expect("TRUST_STALE_SNAPSHOT must fire when stale files exist");

	// Wording discipline (Sub-Decision B1): the summary must
	// describe the storage-internal condition, not a filesystem
	// or git comparison.
	assert!(
		sig.summary().contains("stale file"),
		"summary must mention stale files: {}",
		sig.summary()
	);
	assert!(
		!sig.summary().to_lowercase().contains("changed since"),
		"summary must not overclaim filesystem staleness: {}",
		sig.summary()
	);

	match sig.evidence() {
		SignalEvidence::TrustStaleSnapshot(ev) => {
			assert_eq!(ev.stale_file_count, 2);
			assert_eq!(ev.snapshot_uid, "snap-1");
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── TRUST_NO_ENRICHMENT ─────────────────────────────────────────

#[test]
fn trust_no_enrichment_emitted_when_state_not_run() {
	// Rust-43 F2: the signal fires iff the enrichment phase
	// did not execute (`EnrichmentState::NotRun`). The old
	// "eligible > 0 but not applied" rule was unreachable in
	// the new mapping because eligible > 0 → Ran.
	let mut fake = seeded();
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 0.70,
			resolved_calls: 70,
			unresolved_calls: 30,
			call_graph_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			dead_code_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			enrichment_state: EnrichmentState::NotRun,
			enrichment_eligible: 0,
			enrichment_enriched: 0,
		},
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	let sig = find_signal(&result, SignalCode::TrustNoEnrichment)
		.expect("TRUST_NO_ENRICHMENT must fire on NotRun");
	match sig.evidence() {
		SignalEvidence::TrustNoEnrichment(ev) => {
			// Evidence carries the scalar counts from storage,
			// which are 0/0 in the NotRun case by mapping.
			assert_eq!(ev.enrichment_eligible, 0);
			assert_eq!(ev.enrichment_enriched, 0);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

#[test]
fn trust_no_enrichment_suppressed_when_state_not_applicable() {
	// Rust-43 F2: NotApplicable means the phase ran with zero
	// eligible edges. No penalty, no signal.
	let mut fake = seeded();
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 1.0,
			resolved_calls: 0,
			unresolved_calls: 0,
			call_graph_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			dead_code_reliability: AgentReliabilityAxis {
				level: AgentReliabilityLevel::High,
				reasons: Vec::new(),
			},
			enrichment_state: EnrichmentState::NotApplicable,
			enrichment_eligible: 0,
			enrichment_enriched: 0,
		},
	);
	let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
	assert!(
		find_signal(&result, SignalCode::TrustNoEnrichment).is_none(),
		"must suppress when eligible is zero"
	);
}
