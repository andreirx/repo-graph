//! Budget truncation tests.
//!
//! Verifies that signal and limit lists are truncated to the
//! correct caps per budget tier, and that truncation metadata
//! is populated correctly on both the affected section and the
//! top-level `truncated` boolean.

mod common;

use common::FakeAgentStorage;
use repo_graph_agent::{
    orient, AgentBoundaryDeclaration, AgentCycle, AgentImportEdge, AgentReliabilityAxis,
    AgentReliabilityLevel, AgentStaleFile, AgentTrustSummary, Budget, EnrichmentState,
};

fn seed_many_signals() -> FakeAgentStorage {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap-1");

    // Signal-producing conditions to exceed the small cap (5).
    //
    // Dead-code surface is withdrawn, so we need 7 signals total:
    // - BOUNDARY_VIOLATIONS (1)
    // - TRUST_LOW_RESOLUTION (1)
    // - TRUST_NO_ENRICHMENT (1)
    // - TRUST_STALE_SNAPSHOT (1)
    // - IMPORT_CYCLES (1)
    // - MODULE_SUMMARY (1)
    // - SNAPSHOT_INFO (1)
    // Total = 7.

    // Boundary (1)
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
    // Trust low-resolution (fires TRUST_LOW_RESOLUTION) plus
    // NotRun enrichment (fires TRUST_NO_ENRICHMENT) plus
    // stale files below (fires TRUST_STALE_SNAPSHOT).
    fake.trust_summaries.insert(
        "snap-1".into(),
        AgentTrustSummary {
            call_resolution_rate: 0.10,
            resolved_calls: 1,
            unresolved_calls: 9,
            unresolved_calls_internal_like: 9,
            unresolved_calls_unknown: 0,
            external_targets: Vec::new(),
            call_graph_reliability: AgentReliabilityAxis {
                level: AgentReliabilityLevel::High,
                reasons: Vec::new(),
            },
            dead_code_reliability: AgentReliabilityAxis {
                level: AgentReliabilityLevel::High,
                reasons: Vec::new(),
            },
            enrichment_state: EnrichmentState::NotRun,
            enrichment_eligible: 10,
            enrichment_enriched: 0,
        },
    );
    fake.stale_files.insert(
        "snap-1".into(),
        vec![AgentStaleFile {
            path: "src/a.rs".into(),
        }],
    );
    // Cycles (1)
    fake.cycles.insert(
        "snap-1".into(),
        vec![AgentCycle {
            length: 2,
            modules: vec!["m1".into(), "m2".into()],
            test_composition: None,
        }],
    );
    // Dead code surface withdrawn — no dead_nodes seeding.
    // MODULE_SUMMARY and SNAPSHOT_INFO always emit → +2.
    // Total emitted = 1 + 3 + 1 + 2 = 7.

    fake
}

#[test]
fn small_budget_keeps_headline_signals_unstripped() {
    // ORIENT-DENSITY-1 §2: a small budget trades DEPTH, never the load-bearing
    // facts. This seed emits 7 signals; 3 are protected headline codes
    // (BOUNDARY_VIOLATIONS alert, IMPORT_CYCLES, MODULE_SUMMARY) and 4 are
    // unprotected. The cap (5) applies only to the unprotected tail — 4 ≤ 5 — so
    // NOTHING is dropped and the structure / cycles / alert all survive at small.
    // (Before this slice the flat cap dropped 2 signals, stripping MODULE_SUMMARY
    // — the exact "small budget = thin meta" inversion the slice fixes.)
    let fake = seed_many_signals();
    let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();

    let codes: Vec<&str> = result.signals.iter().map(|s| s.code().as_str()).collect();
    assert!(
        codes.contains(&"MODULE_SUMMARY"),
        "structure must survive a small budget: {codes:?}"
    );
    assert!(
        codes.contains(&"IMPORT_CYCLES"),
        "cycles must survive a small budget: {codes:?}"
    );
    assert!(
        codes.contains(&"BOUNDARY_VIOLATIONS"),
        "the boundary alert must survive a small budget: {codes:?}"
    );
    assert_eq!(
        result.signals.len(),
        7,
        "4 unprotected ≤ small cap (5) ⇒ nothing truncated"
    );
    assert_eq!(result.signals_truncated, None);
    assert!(!result.truncated, "headline-only response is not truncated");
}

#[test]
fn medium_budget_fits_all_seven_signals() {
    let fake = seed_many_signals();
    let result = orient(&fake, "r1", None, Budget::Medium, common::TEST_NOW).unwrap();

    assert_eq!(result.signals.len(), 7);
    assert_eq!(result.signals_truncated, None);
    assert_eq!(result.signals_omitted_count, None);
}

#[test]
fn large_budget_fits_all_signals_and_all_limits() {
    let fake = seed_many_signals();
    let result = orient(&fake, "r1", None, Budget::Large, common::TEST_NOW).unwrap();

    assert_eq!(result.signals.len(), 7);
    // 3 limits: MODULE_DATA_UNAVAILABLE from module_summary,
    // COMPLEXITY_UNAVAILABLE from orient_repo's static append,
    // and GATE_NOT_CONFIGURED from the gate aggregator (this
    // seeded fake has no gate_requirements).
    assert_eq!(result.limits.len(), 3);
    assert!(!result.truncated);
}

#[test]
fn truncated_sections_preserve_highest_ranked_signals() {
    let fake = seed_many_signals();
    let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();

    // The highest-ranked signal must survive truncation.
    let first = result.signals.first().unwrap();
    assert_eq!(first.rank(), 1);
    // ORIENT-DENSITY-1: with headline protection the 4 unprotected signals fit
    // under the small cap (5), so all 7 survive — including the low-priority
    // informational MODULE_SUMMARY (the structure the dense headline needs). The
    // ranks stay dense because truncation only ever REMOVES the unprotected tail,
    // never reorders.
    for (i, s) in result.signals.iter().enumerate() {
        assert_eq!(s.rank(), (i + 1) as u32);
    }
}

#[test]
fn untruncated_response_has_no_truncation_metadata() {
    let mut fake = FakeAgentStorage::new();
    fake.seed_minimal_repo("r1", "my-repo", "snap-1");

    let result = orient(&fake, "r1", None, Budget::Small, common::TEST_NOW).unwrap();
    // Only MODULE_SUMMARY + SNAPSHOT_INFO will fire — 2 signals, under cap.
    // 3 limits (MODULE_DATA_UNAVAILABLE, GATE_NOT_CONFIGURED,
    // COMPLEXITY_UNAVAILABLE), all under Small cap (3).
    assert_eq!(result.signals.len(), 2);
    assert_eq!(result.signals_truncated, None);
    assert_eq!(result.signals_omitted_count, None);
    assert!(!result.truncated);
}
