//! TRUST-LIVEGRAPH-IMPL: pure-layer tests for [`trust_to_coherent`] (the hybrid wrapping + MEET fold).
//!
//! Off-target (architecture.md §Off-Target Testability): NO daemon, NO LiveGraph, NO SQLite — the Half-A
//! posture leaf is constructed directly, exactly as the daemon would hand it in. Pins the slice's §5
//! parity (P1/P2/P3), hybrid-label (H1-H4), degradation (D-T4/D-T5/D-T6), and envelope (E1/E3) checks.

use std::collections::BTreeSet;

use repo_graph_coherence::{
    AnswerClass, CoherenceEnvelope, FreshnessState, LanguageSupport, QueryCompleteness, Source,
    TrustPosture,
};

use super::{trust_to_coherent, LiveGraphPartitionPosture, LiveGraphPosture};
use crate::types::{
    DowngradeTrigger, ReliabilityAxisScore, ReliabilityLevel, TrustBasisClassificationRow,
    TrustCategoryRow, TrustClassificationRow, TrustDowngrades, TrustReliability, TrustReport,
    TrustSummary,
};

// ── Builders ───────────────────────────────────────────────────────────────────────────────────

fn axis(level: ReliabilityLevel) -> ReliabilityAxisScore {
    ReliabilityAxisScore {
        level,
        reasons: vec![],
    }
}

fn no_downgrade() -> DowngradeTrigger {
    DowngradeTrigger {
        triggered: false,
        reasons: vec![],
    }
}

/// A v1 [`TrustReport`] with a couple of category/classification rows. `diagnostics_available` flips the
/// blob-presence path (D-T4).
fn report(diagnostics_available: bool) -> TrustReport {
    TrustReport {
        snapshot_uid: "snap-1".into(),
        display_name: Some("demo".into()),
        basis_commit: Some("abc123".into()),
        toolchain: None,
        diagnostics_version: if diagnostics_available { Some(1) } else { None },
        summary: TrustSummary {
            edges_total: 100,
            edges_resolved: 100,
            unresolved_total: 20,
            resolved_calls: 50,
            unresolved_calls: 10,
            unresolved_calls_external: 4,
            unresolved_calls_internal_like: 6,
            call_resolution_rate: 0.833,
            reliability: TrustReliability {
                import_graph: axis(ReliabilityLevel::HIGH),
                call_graph: axis(ReliabilityLevel::MEDIUM),
                // Internal-only axis; must NEVER serialize (P2).
                dead_code: axis(ReliabilityLevel::LOW),
                change_impact: axis(ReliabilityLevel::HIGH),
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
        classifications: vec![TrustClassificationRow {
            classification: "unknown".into(),
            count: 7,
        }],
        basis_classifications: vec![TrustBasisClassificationRow {
            basis_code: "no_supporting_signal".into(),
            count: 7,
        }],
        external_dependencies: Default::default(),
        unknown_calls_blast_radius: None,
        enrichment_status: None,
        modules: vec![],
        caveats: vec!["a caveat".into()],
        diagnostics_available,
        enrichment_eligible_count: 3,
        // RELIABILITY-REFRAME-1 (review-3 §2): 6 of the 6 internal-like calls are unclassified.
        unresolved_calls_unknown: 6,
    }
}

fn ts() -> BTreeSet<LanguageSupport> {
    BTreeSet::from([LanguageSupport::TypeScriptPrimary])
}

/// A healthy, Fresh, resident LiveGraph posture leaf (the daemon's `module_stats() == Exact/Fresh` case).
fn fresh_posture_leaf() -> CoherenceEnvelope<LiveGraphPosture> {
    LiveGraphPosture {
        resident: true,
        partitions: vec![LiveGraphPartitionPosture {
            partition_id: "pkg".into(),
            freshness: FreshnessState::Fresh,
            typescript_primary: true,
            producer_fingerprint: "scip-typescript@0.4.0".into(),
        }],
        producer_available: true,
        migrated_answer_capability: true,
        // M-R3A-TRUST-POSTURE: the served path carries both facts explicitly.
        livegraph_resident: Some(true),
        coherent_serve_eligible: Some(true),
    }
    .into_leaf(
        TrustPosture {
            class: AnswerClass::Exact,
            completeness: QueryCompleteness::Complete,
            degradation_reasons: vec![],
            contributing_languages: ts(),
        },
        FreshnessState::Fresh,
    )
}

// ── P1: Half-B payloads byte-identical to the v1 report ──────────────────────────────────────────

#[test]
fn half_b_payloads_are_byte_identical_to_v1() {
    let r = report(true);
    let reliability_before = r.summary.reliability.clone();
    let downgrades_before = r.summary.triggered_downgrades.clone();
    let categories_before = r.categories.clone();
    let classifications_before = r.classifications.clone();
    let caveats_before = r.caveats.clone();

    let env = trust_to_coherent(r, fresh_posture_leaf(), false);

    assert_eq!(env.value.reliability.value, reliability_before);
    assert_eq!(env.value.triggered_downgrades.value, downgrades_before);
    assert_eq!(env.value.categories.value, categories_before);
    assert_eq!(env.value.classifications.value, classifications_before);
    assert_eq!(env.value.caveats.value, caveats_before);
    // Resolution counts split verbatim out of the v1 summary.
    assert_eq!(env.value.resolution.value.edges_total, 100);
    assert_eq!(env.value.resolution.value.resolved_calls, 50);
    assert_eq!(env.value.resolution.value.unresolved_calls_internal_like, 6);
    // RELIABILITY-REFRAME-1 (review-3 §2): the in-process unclassified counter projects
    // onto the coherent resolution leaf (it is NOT on the v1 parity summary wire).
    assert_eq!(env.value.resolution.value.unresolved_calls_unknown, 6);
    assert_eq!(env.value.resolution.value.call_resolution_rate, 0.833);
}

// ── P2: the dead_code axis stays internal (never on the wire) ────────────────────────────────────

#[test]
fn dead_code_axis_is_never_serialized() {
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), false);
    let json = serde_json::to_string(&env).unwrap();
    assert!(
        !json.contains("dead_code"),
        "the dead_code reliability axis must stay internal (skip_serializing)"
    );
}

// ── P3: enrichment_eligible_count never reaches the wrapped output ────────────────────────────────

#[test]
fn enrichment_eligible_count_is_not_on_the_wire() {
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), false);
    let json = serde_json::to_string(&env).unwrap();
    assert!(!json.contains("enrichment_eligible_count"));
}

// ── H1: every Half-B leaf is sqlite-sourced (never livegraph) ────────────────────────────────────

#[test]
fn every_half_b_leaf_is_sqlite_sourced_never_livegraph() {
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), false);
    let v = &env.value;
    for src in [
        &v.diagnostics.provenance.source,
        &v.resolution.provenance.source,
        &v.reliability.provenance.source,
        &v.categories.provenance.source,
        &v.classifications.provenance.source,
        &v.unknown_calls_blast_radius.provenance.source,
        &v.enrichment_status.provenance.source,
        &v.modules.provenance.source,
        &v.caveats.provenance.source,
        &v.triggered_downgrades.provenance.source,
    ] {
        assert!(
            !src.contains(&Source::Livegraph),
            "a Half-B residual leaf must never claim a livegraph source (F5)"
        );
    }
}

// ── H2: current_state_posture is the ONLY livegraph-sourced leaf ─────────────────────────────────

#[test]
fn posture_is_the_only_livegraph_leaf() {
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), false);
    assert_eq!(
        env.value.current_state_posture.provenance.source,
        BTreeSet::from([Source::Livegraph])
    );
}

// ── H3: the downgrade-triggers leaf is multi-source {sqlite, declaration} even with NO downgrade fired ──

#[test]
fn downgrades_leaf_is_multi_source_even_when_no_downgrade_fires() {
    // report(true) has all four triggers `triggered: false`, yet the entrypoint Authority table was read.
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), false);
    assert_eq!(
        env.value.triggered_downgrades.provenance.source,
        BTreeSet::from([Source::Sqlite, Source::Declaration]),
        "the entrypoint Authority read makes the downgrades leaf {{sqlite, declaration}} on every report"
    );
}

// ── D-T4: diagnostics blob ABSENT → blob-derived leaves Unavailable/Unknown (NOT Fresh zeros) ────

#[test]
fn diagnostics_absent_marks_blob_leaves_unavailable() {
    let env = trust_to_coherent(report(false), fresh_posture_leaf(), false);
    for leaf_trust in [
        (
            &env.value.diagnostics.trust,
            env.value.diagnostics.freshness,
        ),
        (&env.value.resolution.trust, env.value.resolution.freshness),
        (&env.value.categories.trust, env.value.categories.freshness),
    ] {
        assert_eq!(leaf_trust.0.class, AnswerClass::Unavailable);
        assert_eq!(leaf_trust.0.completeness, QueryCompleteness::Unknown);
        assert_eq!(leaf_trust.1, FreshnessState::Unavailable);
    }
    // A non-blob leaf (reliability is always computed) stays snapshot-posture, not Unavailable.
    assert_eq!(env.value.reliability.trust.class, AnswerClass::Exact);
}

// ── D-T5: stale snapshot → Half-B leaves Stale ───────────────────────────────────────────────────

#[test]
fn stale_snapshot_marks_half_b_leaves_stale() {
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), true);
    assert_eq!(env.value.reliability.freshness, FreshnessState::Stale);
    assert_eq!(env.value.reliability.trust.class, AnswerClass::Stale);
    assert_eq!(env.value.modules.freshness, FreshnessState::Stale);
}

// ── D-T6 / RISK-T-A: epoch skew — root freshness is the MEET of both halves ──────────────────────

#[test]
fn fresh_posture_over_stale_snapshot_yields_stale_root() {
    // Half A Fresh (warm LiveGraph), Half B Stale (stale index) → root Stale (MEET), never Fresh.
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), true);
    assert_eq!(env.freshness, FreshnessState::Stale);
    assert_ne!(env.trust.class, AnswerClass::Exact);
    // Half A itself is still Fresh — the two halves carry independent, honest freshness.
    assert_eq!(
        env.value.current_state_posture.freshness,
        FreshnessState::Fresh
    );
}

#[test]
fn cold_livegraph_over_fresh_snapshot_degrades_the_root() {
    // Half A Unavailable (cold LiveGraph), Half B Fresh → root Unavailable (MEET, D-T6). The Half-B leaves
    // are still individually Fresh/sqlite-served — the v1 report is fully available, honestly labelled.
    let env = trust_to_coherent(report(true), LiveGraphPosture::unavailable_leaf(), false);
    assert_eq!(env.freshness, FreshnessState::Unavailable);
    assert_eq!(env.trust.class, AnswerClass::Unavailable);
    assert!(!env.value.current_state_posture.value.resident);
    // F3: Unavailable is NOT empty-as-known-zero — the posture value reports resident=false explicitly.
    assert_eq!(
        env.value.current_state_posture.trust.class,
        AnswerClass::Unavailable
    );
    // Half B stays served + Fresh + sqlite-labelled.
    assert_eq!(env.value.reliability.freshness, FreshnessState::Fresh);
    assert_eq!(
        env.value.reliability.provenance.source,
        BTreeSet::from([Source::Sqlite])
    );
}

// ── E1: the MEET is monotone — no fold manufactures an Exact/Fresh root from a non-Exact leaf ────

#[test]
fn root_never_exceeds_the_weakest_leaf() {
    // A healthy report + Fresh posture → Exact/Fresh root (the only all-Exact case).
    let healthy = trust_to_coherent(report(true), fresh_posture_leaf(), false);
    assert_eq!(healthy.trust.class, AnswerClass::Exact);
    assert_eq!(healthy.freshness, FreshnessState::Fresh);

    // Any degraded half pulls the root below Exact.
    let stale = trust_to_coherent(report(true), fresh_posture_leaf(), true);
    assert_ne!(stale.trust.class, AnswerClass::Exact);
    let cold = trust_to_coherent(report(true), LiveGraphPosture::unavailable_leaf(), false);
    assert_ne!(cold.trust.class, AnswerClass::Exact);
}

// ── E3: the root provenance is the exact set-UNION {livegraph, sqlite, declaration} ──────────────

#[test]
fn root_provenance_is_the_three_source_union() {
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), false);
    assert_eq!(
        env.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite, Source::Declaration])
    );
    // No Half-B axis is LG-first this slice → no cert fallback at the root.
    assert!(env.provenance.fallback_reason.is_none());
}

// ── W1: the wire shape is CoherenceEnvelope<CoherentTrustReport> with the posture leaf present ────

#[test]
fn wire_shape_carries_the_wrapper_and_the_posture_leaf() {
    let env = trust_to_coherent(report(true), fresh_posture_leaf(), false);
    let json = serde_json::to_value(&env).unwrap();
    assert!(json.get("value").is_some());
    assert!(json.get("provenance").is_some());
    assert!(json.get("trust").is_some());
    assert!(json.get("freshness").is_some());
    assert!(json["value"].get("current_state_posture").is_some());
    assert!(json["value"]["current_state_posture"]["value"]["resident"].as_bool() == Some(true));
}

// ── M-R3A-TRUST-POSTURE (ratified 2026-07-19): the two-fact wire contract ────────────────────────

/// The COLD leaf serializes WITHOUT the amendment fields — `resident: false` is the complete
/// truth there and the zero-SCIP wire stays byte-identical to the pre-amendment shape (R-0).
#[test]
fn cold_posture_wire_omits_the_amendment_fields() {
    let cold = trust_to_coherent(report(true), LiveGraphPosture::unavailable_leaf(), false);
    let posture = &serde_json::to_value(&cold).unwrap()["value"]["current_state_posture"]["value"];
    assert_eq!(posture["resident"].as_bool(), Some(false));
    assert!(posture.get("livegraph_resident").is_none(), "{posture}");
    assert!(
        posture.get("coherent_serve_eligible").is_none(),
        "{posture}"
    );
}

/// The RESIDENT-BUT-WITHHELD leaf carries BOTH facts explicitly labeled — residency true,
/// eligibility false — while the legacy serve fact (`resident`) and the Unavailable class are
/// unchanged (the epoch invariant; values stay withheld).
#[test]
fn resident_withheld_posture_wire_states_both_facts() {
    let env = trust_to_coherent(
        report(true),
        LiveGraphPosture::resident_withheld_leaf(),
        false,
    );
    let posture = &serde_json::to_value(&env).unwrap()["value"]["current_state_posture"]["value"];
    assert_eq!(posture["resident"].as_bool(), Some(false));
    assert_eq!(posture["livegraph_resident"].as_bool(), Some(true));
    assert_eq!(posture["coherent_serve_eligible"].as_bool(), Some(false));
    assert_eq!(posture["partitions"].as_array().map(Vec::len), Some(0));
}
