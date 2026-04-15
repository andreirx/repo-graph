//! One test per signal code that the repo-level orient pipeline
//! can emit. Tests assert both that the signal appears (or does
//! not) under the right conditions, and that its evidence fields
//! match the input data exactly.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
	orient, AgentBoundaryDeclaration, AgentCycle, AgentDeadNode,
	AgentImportEdge, AgentRepoSummary, AgentStaleFile, AgentTrustSummary,
	Budget, SignalCode, SignalEvidence,
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Medium).unwrap();
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

#[test]
fn dead_code_not_emitted_when_none_exist() {
	let fake = seeded();
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
	assert!(find_signal(&result, SignalCode::DeadCode).is_none());
}

#[test]
fn dead_code_emitted_when_threshold_met() {
	let mut fake = seeded();
	fake.dead_nodes.insert(
		"snap-1".into(),
		vec![AgentDeadNode {
			stable_key: "r1:src/foo.rs:SYMBOL:unused".into(),
			symbol: "unused".into(),
			kind: "SYMBOL".into(),
			file: Some("src/foo.rs".into()),
			line_count: Some(5),
		}],
	);
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
	let sig = find_signal(&result, SignalCode::DeadCode)
		.expect("DEAD_CODE must fire at threshold 1");
	match sig.evidence() {
		SignalEvidence::DeadCode(ev) => {
			assert_eq!(ev.dead_count, 1);
			assert_eq!(ev.top_dead.len(), 1);
			assert_eq!(ev.top_dead[0].symbol, "unused");
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

#[test]
fn dead_code_top_slice_is_sorted_by_size_descending() {
	// Regression for P3: the storage port orders dead nodes
	// alphabetically, not by size. The aggregator must sort by
	// line_count descending before slicing top N so the largest
	// dead items survive truncation.
	let mut fake = seeded();
	fake.dead_nodes.insert(
		"snap-1".into(),
		vec![
			// Alphabetically first, but smallest — should NOT
			// appear in top_dead if we had to pick between it
			// and a larger entry further down.
			AgentDeadNode {
				stable_key: "r1:a.rs:SYMBOL:aa".into(),
				symbol: "aa".into(),
				kind: "SYMBOL".into(),
				file: Some("a.rs".into()),
				line_count: Some(2),
			},
			AgentDeadNode {
				stable_key: "r1:b.rs:SYMBOL:bb".into(),
				symbol: "bb".into(),
				kind: "SYMBOL".into(),
				file: Some("b.rs".into()),
				line_count: Some(50),
			},
			AgentDeadNode {
				stable_key: "r1:c.rs:SYMBOL:cc".into(),
				symbol: "cc".into(),
				kind: "SYMBOL".into(),
				file: Some("c.rs".into()),
				line_count: Some(20),
			},
			AgentDeadNode {
				stable_key: "r1:d.rs:SYMBOL:dd".into(),
				symbol: "dd".into(),
				kind: "SYMBOL".into(),
				file: Some("d.rs".into()),
				line_count: Some(100),
			},
		],
	);
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
	let sig = find_signal(&result, SignalCode::DeadCode).unwrap();
	match sig.evidence() {
		SignalEvidence::DeadCode(ev) => {
			// dead_count reflects the full input, not the slice.
			assert_eq!(ev.dead_count, 4);
			assert_eq!(ev.top_dead.len(), 3);
			// Sorted descending by line_count: 100, 50, 20.
			assert_eq!(ev.top_dead[0].symbol, "dd");
			assert_eq!(ev.top_dead[0].line_count, Some(100));
			assert_eq!(ev.top_dead[1].symbol, "bb");
			assert_eq!(ev.top_dead[1].line_count, Some(50));
			assert_eq!(ev.top_dead[2].symbol, "cc");
			assert_eq!(ev.top_dead[2].line_count, Some(20));
			// Smallest ("aa", 2) must be dropped from the slice.
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

#[test]
fn dead_code_top_slice_pushes_unknown_sizes_to_the_tail() {
	// When line_count is None (missing line_end), the entry
	// must sort AFTER any entry with a known size, so it never
	// displaces a genuinely large dead symbol from top_dead.
	let mut fake = seeded();
	fake.dead_nodes.insert(
		"snap-1".into(),
		vec![
			AgentDeadNode {
				stable_key: "r1:a.rs:SYMBOL:unknown_size".into(),
				symbol: "unknown_size".into(),
				kind: "SYMBOL".into(),
				file: Some("a.rs".into()),
				line_count: None,
			},
			AgentDeadNode {
				stable_key: "r1:b.rs:SYMBOL:small".into(),
				symbol: "small".into(),
				kind: "SYMBOL".into(),
				file: Some("b.rs".into()),
				line_count: Some(1),
			},
		],
	);
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
	let sig = find_signal(&result, SignalCode::DeadCode).unwrap();
	match sig.evidence() {
		SignalEvidence::DeadCode(ev) => {
			assert_eq!(ev.dead_count, 2);
			assert_eq!(ev.top_dead[0].symbol, "small");
			assert_eq!(ev.top_dead[1].symbol, "unknown_size");
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

// ── BOUNDARY_VIOLATIONS ─────────────────────────────────────────

#[test]
fn boundary_violations_not_emitted_when_no_declarations() {
	let fake = seeded();
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
			enrichment_applied: true,
			enrichment_eligible: 10,
			enrichment_enriched: 9,
		},
	);
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
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
fn trust_no_enrichment_emitted_when_eligible_but_not_applied() {
	let mut fake = seeded();
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 0.70,
			resolved_calls: 70,
			unresolved_calls: 30,
			enrichment_applied: false,
			enrichment_eligible: 30,
			enrichment_enriched: 0,
		},
	);
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
	let sig = find_signal(&result, SignalCode::TrustNoEnrichment)
		.expect("TRUST_NO_ENRICHMENT must fire");
	match sig.evidence() {
		SignalEvidence::TrustNoEnrichment(ev) => {
			assert_eq!(ev.enrichment_eligible, 30);
			assert_eq!(ev.enrichment_enriched, 0);
		}
		other => panic!("wrong evidence variant: {:?}", other),
	}
}

#[test]
fn trust_no_enrichment_suppressed_when_eligible_is_zero() {
	// When there are no eligible edges, enrichment is "not
	// applicable" and must NOT emit a signal.
	let mut fake = seeded();
	fake.trust_summaries.insert(
		"snap-1".into(),
		AgentTrustSummary {
			call_resolution_rate: 1.0,
			resolved_calls: 0,
			unresolved_calls: 0,
			enrichment_applied: false,
			enrichment_eligible: 0,
			enrichment_enriched: 0,
		},
	);
	let result = orient(&fake, "r1", None, Budget::Small).unwrap();
	assert!(
		find_signal(&result, SignalCode::TrustNoEnrichment).is_none(),
		"must suppress when eligible is zero"
	);
}
