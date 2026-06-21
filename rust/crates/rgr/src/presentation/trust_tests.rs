//! TRUST-LIVEGRAPH-IMPL: human-render tests for the hybrid `CoherenceEnvelope<CoherentTrustReport>`.
//!
//! Split out of `trust.rs` (via `#[path]`) to respect the >500-line structural guardrail. Builds the wrapper
//! through the REAL `repo_graph_trust::trust_to_coherent` so the render is exercised against the true wire
//! shape. Pins the byte-identical Half-B bullets (W2/P1), the dead_code absence (D-TRUST-5), the per-section
//! source/freshness labels, the NEW Current-State Posture section, and the cold-LiveGraph degradation (D-T6).

use super::*;
use repo_graph_coherence::{FreshnessState, QueryCompleteness, TrustPosture};
use repo_graph_trust::trust_to_coherent;
use repo_graph_trust::types::{
    DowngradeTrigger, ReliabilityAxisScore, ReliabilityLevel, TrustCategoryRow, TrustDowngrades,
    TrustReliability, TrustReport, TrustSummary,
};
use repo_graph_trust::{LiveGraphPartitionPosture, LiveGraphPosture};
use std::collections::BTreeSet;

fn axis(level: ReliabilityLevel, reasons: Vec<&str>) -> ReliabilityAxisScore {
    ReliabilityAxisScore {
        level,
        reasons: reasons.into_iter().map(|s| s.to_string()).collect(),
    }
}

fn no_downgrade() -> DowngradeTrigger {
    DowngradeTrigger {
        triggered: false,
        reasons: vec![],
    }
}

fn report() -> TrustReport {
    TrustReport {
        snapshot_uid: "snap_01kr12345678".into(),
        display_name: Some("test-repo".into()),
        basis_commit: None,
        toolchain: None,
        diagnostics_version: Some(1),
        summary: TrustSummary {
            edges_total: 100,
            edges_resolved: 100,
            unresolved_total: 20,
            resolved_calls: 50,
            unresolved_calls: 10,
            unresolved_calls_external: 5,
            unresolved_calls_internal_like: 5,
            call_resolution_rate: 0.833,
            reliability: TrustReliability {
                import_graph: axis(ReliabilityLevel::HIGH, vec![]),
                call_graph: axis(ReliabilityLevel::MEDIUM, vec!["some unresolved calls"]),
                dead_code: axis(ReliabilityLevel::LOW, vec![]),
                change_impact: axis(ReliabilityLevel::HIGH, vec![]),
            },
            triggered_downgrades: TrustDowngrades {
                framework_heavy_suspicion: no_downgrade(),
                registry_pattern_suspicion: no_downgrade(),
                missing_entrypoint_declarations: no_downgrade(),
                alias_resolution_suspicion: no_downgrade(),
            },
        },
        categories: vec![TrustCategoryRow {
            category: "imports_file_not_found".into(),
            label: "IMPORTS (file not found)".into(),
            unresolved: 5,
        }],
        classifications: vec![],
        unknown_calls_blast_radius: None,
        enrichment_status: None,
        modules: vec![],
        caveats: vec![],
        diagnostics_available: true,
        enrichment_eligible_count: 0,
    }
}

fn warm_posture() -> CoherenceEnvelope<LiveGraphPosture> {
    LiveGraphPosture {
        resident: true,
        partitions: vec![LiveGraphPartitionPosture {
            partition_id: "app".into(),
            freshness: FreshnessState::Fresh,
            typescript_primary: true,
            producer_fingerprint: "scip-typescript@0.4.0".into(),
        }],
        producer_available: true,
        migrated_answer_capability: true,
    }
    .into_leaf(
        TrustPosture {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            degradation_reasons: vec![],
            contributing_languages: BTreeSet::from([
                repo_graph_coherence::LanguageSupport::TypeScriptPrimary,
            ]),
        },
        FreshnessState::Fresh,
    )
}

fn warm_envelope() -> TrustEnvelope {
    trust_to_coherent(report(), warm_posture(), false)
}

#[test]
fn render_shows_repo_display_name() {
    let out = render_trust_envelope(&warm_envelope());
    assert!(out.contains("Trust Report: test-repo"));
}

#[test]
fn render_shows_resolution_rates_byte_identical_bullets() {
    let out = render_trust_envelope(&warm_envelope());
    // The v1 bullet text is preserved verbatim (W2 / P1).
    assert!(out.contains("Calls: 83% resolved (50 of 60)"));
    assert!(out.contains("Edges: 100% resolved (100 of 100)"));
}

#[test]
fn render_shows_reliability_levels() {
    let out = render_trust_envelope(&warm_envelope());
    assert!(out.contains("Call-graph: MEDIUM"));
    assert!(out.contains("Import-graph: HIGH"));
    // The dead_code axis must NEVER surface in the human render (D-TRUST-5).
    assert!(!out.contains("Dead-code"));
    assert!(!out.contains("dead_code"));
}

#[test]
fn render_carries_per_section_source_labels() {
    let out = render_trust_envelope(&warm_envelope());
    // Half-B sections are labelled sqlite + snapshot-scoped extraction (F5).
    assert!(out.contains("Resolution  (sqlite, snapshot-scoped extraction, Fresh)"));
    assert!(out.contains("Reliability  (sqlite, snapshot-scoped extraction, Fresh)"));
}

#[test]
fn render_shows_current_state_posture_section() {
    let out = render_trust_envelope(&warm_envelope());
    assert!(out.contains("Current-State Posture  (livegraph, current-state, Fresh)"));
    assert!(out.contains("Resident: yes (1 partition)"));
    assert!(out.contains("app: Fresh, TypeScript, producer scip-typescript@0.4.0"));
    assert!(out.contains("Producer available: yes"));
    assert!(out.contains("Migrated-answer capability: yes"));
}

#[test]
fn render_shows_overall_posture_line() {
    let out = render_trust_envelope(&warm_envelope());
    // A Fresh LiveGraph posture + Fresh snapshot -> Exact (Fresh) overall.
    assert!(out.contains("Posture: Exact (Fresh)"));
}

#[test]
fn cold_livegraph_posture_renders_unavailable_and_degrades_overall() {
    let env = trust_to_coherent(report(), LiveGraphPosture::unavailable_leaf(), false);
    let out = render_trust_envelope(&env);
    assert!(out.contains("Current-State Posture  (livegraph, current-state, Unavailable)"));
    assert!(out.contains("Resident: no"));
    // The overall posture is degraded by the cold LiveGraph even over a Fresh snapshot (D-T6).
    assert!(out.contains("Posture: Unavailable (Unavailable)"));
    // Half B is still rendered (the v1 report is available).
    assert!(out.contains("Reliability  (sqlite, snapshot-scoped extraction, Fresh)"));
}

#[test]
fn render_falls_back_to_snapshot_uid_when_no_display_name() {
    let mut r = report();
    r.display_name = None;
    let env = trust_to_coherent(r, warm_posture(), false);
    let out = render_trust_envelope(&env);
    assert!(out.contains("Trust Report: snap_01kr12345678"));
}
