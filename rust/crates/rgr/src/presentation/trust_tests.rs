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
    DowngradeTrigger, EnrichmentStatus, EnrichmentTopType, ReliabilityAxisScore, ReliabilityLevel,
    TrustCategoryRow, TrustClassificationRow, TrustDowngrades, TrustReliability, TrustReport,
    TrustSummary,
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
        unresolved_calls_unknown: 0,
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
fn render_resolution_is_reader_frame_in_scope() {
    // RELIABILITY-REFRAME-1 supersedes the v1 byte-identical "Calls: X% resolved
    // (N of M)" bullet (its denominator was the external-INCLUSIVE `unresolved_calls`).
    // Fixture: resolved=50, internal-like unresolved=5, external=5, all-unresolved=10.
    //   in-scope rate = 50 / (50 + 5) = 91% (NOT 50 / (50 + 10) = 83%).
    //   external share = 5 / (50 + 10) = 8% of all calls.
    let out = render_trust_envelope(&warm_envelope());
    assert!(
        out.contains("your code's calls 91% resolved (50 of 55 in-scope or unclassified)"),
        "reader-frame in-scope resolution line (review-3 §2: denominator is in-scope OR unclassified):\n{out}"
    );
    assert!(
        out.contains("8% of calls go into external libraries — follow to their crates/docs"),
        "named external share line:\n{out}"
    );
    // Never the grades-us external-inclusive number or vocabulary.
    assert!(
        !out.contains("Calls: 83% resolved"),
        "old grades-us line survived:\n{out}"
    );
    // Edges are structural (not call-graph reliability) — unchanged.
    assert!(out.contains("Edges: 100% resolved (100 of 100)"));
}

#[test]
fn render_resolution_in_scope_low_still_reads_low() {
    // Slice §3: the reframe must NEVER hide genuine in-scope failure. With 80
    // IN-SCOPE calls unresolved (external=10, resolved=10), the in-scope rate is
    // 10 / (10 + 80) = 11% — it reads LOW plainly; the external exclusion does not
    // mask it (the external subset is only 10 of 100 calls).
    let mut r = report();
    r.summary.resolved_calls = 10;
    r.summary.unresolved_calls = 90;
    r.summary.unresolved_calls_external = 10;
    r.summary.unresolved_calls_internal_like = 80;
    r.summary.call_resolution_rate = 10.0 / 90.0;
    r.summary.reliability.call_graph = axis(
        ReliabilityLevel::LOW,
        vec!["call_resolution_rate=11.1%_below_50%"],
    );
    let out = render_trust_envelope(&trust_to_coherent(r, warm_posture(), false));
    assert!(
        out.contains("your code's calls 11% resolved (10 of 90 in-scope or unclassified)"),
        "genuine in-scope failure must read low, not hidden:\n{out}"
    );
    // The band still reads LOW, with reframed reader-frame prose.
    assert!(
        out.contains("Call-graph: LOW"),
        "band must still read LOW:\n{out}"
    );
    assert!(
        out.contains("your code's calls 11% resolved (below 50% target)"),
        "reframed band reason:\n{out}"
    );
}

#[test]
fn render_resolution_zero_external_renders_none_identified_heuristic() {
    // review-3 §2 / operator iteration-4 §2: when the heuristic identifies ZERO external
    // calls (but calls exist), render "no external-library calls identified (heuristic)" — a
    // heuristic FINDING, NOT a measured absence and NOT a fabricated "0% external" or a silent
    // omission. This is the KNOWN-ZERO case (distinct from the material-unclassified caveat).
    let mut r = report();
    r.summary.resolved_calls = 50;
    r.summary.unresolved_calls = 5;
    r.summary.unresolved_calls_external = 0;
    r.summary.unresolved_calls_internal_like = 5;
    let out = render_trust_envelope(&trust_to_coherent(r, warm_posture(), false));
    assert!(
        out.contains("your code's calls 91% resolved (50 of 55 in-scope or unclassified)"),
        "{out}"
    );
    assert!(
        out.contains("no external-library calls identified (heuristic name-set match, not compiler-verified)"),
        "zero external is a heuristic 'none identified', never a fabricated 0% or a silent omission:\n{out}"
    );
    // Still never a fabricated percentage.
    assert!(
        !out.contains("0% of calls go into external libraries"),
        "{out}"
    );
}

#[test]
fn render_resolution_material_unclassified_fires_conservative_caveat() {
    // review-3 §2 / slice §2 degraded path: the in-scope denominator is "in-scope OR
    // unclassified". When a MATERIAL share is unclassified (unknown ≠ known-internal), the rate
    // is a conservative lower bound and the surface says so — the UNKNOWN case, distinct from the
    // known-zero-external "none identified" case above. Here 40 of the 90-call denominator are
    // unclassified (44% ≥ the 20% material threshold).
    let mut r = report();
    r.summary.resolved_calls = 10;
    r.summary.unresolved_calls = 90;
    r.summary.unresolved_calls_external = 10;
    r.summary.unresolved_calls_internal_like = 80;
    r.unresolved_calls_unknown = 40; // 40 of (10 + 80) = 44% unclassified → material
    let out = render_trust_envelope(&trust_to_coherent(r, warm_posture(), false));
    assert!(
        out.contains("40 of these 90 calls are unclassified"),
        "conservative-rate caveat names the unclassified count and denominator:\n{out}"
    );
    assert!(
        out.contains("the true resolved share may be higher"),
        "the caveat states the rate is a lower bound:\n{out}"
    );
}

#[test]
fn render_resolution_immaterial_unclassified_no_caveat() {
    // The caveat is gated on MATERIALITY — 1 of 90 unclassified (1.1% < 20%) does NOT fire it;
    // the "in-scope or unclassified" label alone carries the honesty. Prevents caveat noise.
    let mut r = report();
    r.summary.resolved_calls = 10;
    r.summary.unresolved_calls = 90;
    r.summary.unresolved_calls_external = 10;
    r.summary.unresolved_calls_internal_like = 80;
    r.unresolved_calls_unknown = 1;
    let out = render_trust_envelope(&trust_to_coherent(r, warm_posture(), false));
    assert!(
        !out.contains("are unclassified"),
        "an immaterial unclassified share must not fire the caveat (noise):\n{out}"
    );
}

#[test]
fn render_resolution_zero_in_scope_is_unknown_not_fabricated_full() {
    // REVISE #3 / slice §3: 0 resolved + 0 in-scope unresolved is UNKNOWN — render
    // "no in-scope calls measured", NEVER the prior fabricated "100% resolved (0 of 0)".
    // This is the surface where the `.unwrap_or(100.0)` dishonesty lived.
    let mut r = report();
    r.summary.resolved_calls = 0;
    r.summary.unresolved_calls = 0;
    r.summary.unresolved_calls_external = 0;
    r.summary.unresolved_calls_internal_like = 0;
    let out = render_trust_envelope(&trust_to_coherent(r, warm_posture(), false));
    assert!(
        out.contains("no in-scope calls measured"),
        "zero in-scope calls is unknown, not 100%:\n{out}"
    );
    assert!(
        !out.contains("100% resolved (0 of 0"),
        "no fabricated 100% for an empty call graph:\n{out}"
    );
}

#[test]
fn render_reliability_zero_in_scope_calls_is_unknown_not_a_band() {
    // REVISE #1 / review-1 §1: with NO in-scope calls the RELIABILITY section's Call-graph axis
    // must read "no in-scope calls measured", NEVER the upstream vacuous band. The stored band is
    // forced HIGH to prove the render does not trust it when there is nothing to measure.
    let mut r = report();
    r.summary.resolved_calls = 0;
    r.summary.unresolved_calls = 0;
    r.summary.unresolved_calls_external = 0;
    r.summary.unresolved_calls_internal_like = 0;
    r.summary.reliability.call_graph = axis(ReliabilityLevel::HIGH, vec![]);
    let out = render_trust_envelope(&trust_to_coherent(r, warm_posture(), false));
    assert!(
        out.contains("Call-graph: no in-scope calls measured"),
        "call-graph axis reads unknown, not a vacuous band:\n{out}"
    );
    assert!(
        !out.contains("Call-graph: HIGH"),
        "no vacuous HIGH band for a repo with nothing to measure:\n{out}"
    );
    // Import-graph / change-impact are separate axes and still render their bands.
    assert!(out.contains("Import-graph: HIGH"), "{out}");
}

#[test]
fn render_omits_raw_pipeline_diagnostic_sections_from_human_surface() {
    // review-1 §3 / slice §1.2 (VISION: labels speak the reader's language): the raw
    // "Unresolved Breakdown" + "Classification" sections narrated OUR extractor in internal
    // vocabulary. They are moved to the `--json` structured surface ONLY — the human render must
    // carry NONE of the raw codes even when the report is full of them.
    let mut r = report();
    r.categories = vec![TrustCategoryRow {
        category: "calls_obj_method_needs_type_info".into(),
        label: "CALLS (object method needs type info)".into(),
        unresolved: 42,
    }];
    r.classifications = vec![
        TrustClassificationRow {
            classification: "external_library_candidate".into(),
            count: 30,
        },
        TrustClassificationRow {
            classification: "internal_candidate".into(),
            count: 12,
        },
    ];
    let out = render_trust_envelope(&trust_to_coherent(r, warm_posture(), false));
    assert!(
        !out.contains("Unresolved Breakdown"),
        "raw breakdown heading must not render on the human surface:\n{out}"
    );
    assert!(
        !out.contains("Classification"),
        "raw classification heading must not render on the human surface:\n{out}"
    );
    assert!(
        !out.contains("external_library_candidate") && !out.contains("internal_candidate"),
        "raw classification codes must not leak to the reader:\n{out}"
    );
    assert!(
        !out.contains("calls_obj_method_needs_type_info"),
        "raw category code must not leak to the reader:\n{out}"
    );
}

#[test]
fn render_names_top_external_targets_in_order_with_heuristic_basis() {
    // Slice §2: the external share renders as a NAMED coverage map — top targets
    // (from receiver-type facts) in the given count-desc order, with honest
    // heuristic-basis markers (EY1-A), never a Layer-0 claim. Internal receivers
    // are excluded from the external map.
    let mut r = report();
    r.enrichment_status = Some(EnrichmentStatus {
        eligible: 47,
        enriched: 47,
        top_types: vec![
            EnrichmentTopType {
                type_name: "Value".into(),
                count: 30,
                is_external: true,
            },
            EnrichmentTopType {
                type_name: "Vec".into(),
                count: 12,
                is_external: true,
            },
            EnrichmentTopType {
                type_name: "LocalThing".into(),
                count: 5,
                is_external: false,
            },
        ],
        // review-3 §3: the render reads `top_external_types` (external-FILTERED then truncated) —
        // the externals only, count-desc, as the service produces them.
        top_external_types: vec![
            EnrichmentTopType {
                type_name: "Value".into(),
                count: 30,
                is_external: true,
            },
            EnrichmentTopType {
                type_name: "Vec".into(),
                count: 12,
                is_external: true,
            },
        ],
    });
    let out = render_trust_envelope(&trust_to_coherent(r, warm_posture(), false));
    let value_at = out.find("`Value`").expect("names external receiver Value");
    let vec_at = out.find("`Vec`").expect("names external receiver Vec");
    assert!(value_at < vec_at, "top targets in count-desc order:\n{out}");
    assert!(
        !out.contains("LocalThing"),
        "internal receiver excluded from the external coverage map:\n{out}"
    );
    // Honest heuristic basis (EY1-A) — never a Layer-0 certainty claim.
    assert!(out.contains("not compiler-verified"), "{out}");
    assert!(
        out.contains("orientation only, not resolved call-graph edges"),
        "{out}"
    );
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

// ── ENRICH-YIELD-2 EY1-A: likely-external receiver-call orientation projection ─────────────────

/// A report whose enrichment surfaced external receiver types (like the ~36% gate-4 class measured on
/// the self-index) plus one INTERNAL resolved type — the internal one must not leak into the
/// external-only section. Ordered count-desc as the trust service emits it.
fn report_with_external_receivers() -> TrustReport {
    let mut r = report();
    r.enrichment_status = Some(EnrichmentStatus {
        eligible: 100,
        enriched: 74,
        top_types: vec![
            EnrichmentTopType {
                type_name: "Value".into(),
                count: 425,
                is_external: true,
            },
            EnrichmentTopType {
                type_name: "TempDir".into(),
                count: 203,
                is_external: true,
            },
            // A language primitive — EY1-B classifies primitives external in the Rust resolver, so
            // `str` receivers now surface in this projection too (the measured `str`=512 class).
            EnrichmentTopType {
                type_name: "str".into(),
                count: 512,
                is_external: true,
            },
            EnrichmentTopType {
                type_name: "Engine".into(),
                count: 12,
                is_external: false, // internal resolved type — belongs to promotion, not this section
            },
            EnrichmentTopType {
                type_name: "Once".into(),
                count: 1,
                is_external: true, // exercises singular "call"
            },
        ],
        // review-3 §3: externals only, count-desc (Engine internal excluded) — what the service's
        // filter-then-truncate emits, and what the render now reads.
        top_external_types: vec![
            EnrichmentTopType {
                type_name: "str".into(),
                count: 512,
                is_external: true,
            },
            EnrichmentTopType {
                type_name: "Value".into(),
                count: 425,
                is_external: true,
            },
            EnrichmentTopType {
                type_name: "TempDir".into(),
                count: 203,
                is_external: true,
            },
            EnrichmentTopType {
                type_name: "Once".into(),
                count: 1,
                is_external: true,
            },
        ],
    });
    r
}

#[test]
fn render_shows_likely_external_receiver_projection_with_separate_bases() {
    let env = trust_to_coherent(report_with_external_receivers(), warm_posture(), false);
    let out = render_trust_envelope(&env);

    // The Layer-2 section renders, labelled like its Half-B siblings (source/scope/freshness).
    assert!(
        out.contains("Likely-External Receiver Calls  (sqlite, snapshot-scoped extraction, Fresh)"),
        "section heading with honesty label: {out}"
    );
    // The ratified reader label, per external type, with call counts (plural + singular).
    assert!(
        out.contains("call on likely-external receiver `Value` (425 calls)"),
        "{out}"
    );
    assert!(
        out.contains("call on likely-external receiver `TempDir` (203 calls)"),
        "{out}"
    );
    assert!(
        out.contains("call on likely-external receiver `Once` (1 call)"),
        "singular call count: {out}"
    );
    // A primitive receiver renders here too (EY1-B classifies primitives external).
    assert!(
        out.contains("call on likely-external receiver `str` (512 calls)"),
        "primitive receiver surfaces in the projection: {out}"
    );

    // STRUCTURAL separation of the two bases: the receiver-type basis and the
    // external-classification basis are on DISTINCT bullet lines, each its own labelled line — not
    // merged into one "basis:" claim. Locate each line and assert they are different lines and each
    // is its own bullet.
    let receiver_line = out
        .lines()
        .find(|l| l.contains("receiver-type basis:"))
        .expect("a dedicated receiver-type basis line");
    let external_line = out
        .lines()
        .find(|l| l.contains("external-classification basis:"))
        .expect("a dedicated external-classification basis line");
    assert_ne!(
        receiver_line, external_line,
        "the two bases must be on separate lines, not one combined bullet"
    );
    assert!(
        receiver_line.trim_start().starts_with("- ")
            && external_line.trim_start().starts_with("- "),
        "each basis is its own bullet: {receiver_line:?} / {external_line:?}"
    );
    // The receiver-type basis line carries ONLY the hover provenance; the external line carries the
    // name-set + not-compiler-verified honesty. Neither leaks into the other.
    assert!(
        receiver_line.contains("language-server type hover") && !receiver_line.contains("name-set"),
        "receiver-type basis line is about the hover, not the name-set: {receiver_line:?}"
    );
    assert!(
        external_line.contains("static name-set")
            && external_line.contains("not compiler-verified"),
        "external basis line carries the name-set + not-compiler-verified honesty: {external_line:?}"
    );
    // The Layer-2 framing is its own line too.
    assert!(
        out.contains("orientation only, not resolved call-graph edges"),
        "Layer-2 framing present: {out}"
    );

    // An INTERNAL resolved type must NOT appear in this external-only section (no false external claim).
    assert!(
        !out.contains("likely-external receiver `Engine`"),
        "internal type must not render as external: {out}"
    );
    // Internal-constant names stay off the reader surface (VISION: labels speak the reader's language).
    assert!(
        !out.contains("STD_TYPES") && !out.contains("PRIMITIVES"),
        "internal constant names must not leak to the reader: {out}"
    );
}

#[test]
fn render_omits_external_section_when_enrichment_absent() {
    // warm_envelope() has enrichment_status = None → honest absence, never a phantom/measured-zero.
    let out = render_trust_envelope(&warm_envelope());
    assert!(
        !out.contains("Likely-External Receiver Calls"),
        "no enrichment → no section: {out}"
    );
}

#[test]
fn render_omits_external_section_when_no_external_types() {
    // Enrichment ran but every resolved type is internal → measured-absent, still no section.
    let mut r = report();
    r.enrichment_status = Some(EnrichmentStatus {
        eligible: 10,
        enriched: 8,
        top_types: vec![EnrichmentTopType {
            type_name: "Engine".into(),
            count: 8,
            is_external: false,
        }],
        // No external receivers → the filter-then-truncate list is empty.
        top_external_types: vec![],
    });
    let env = trust_to_coherent(r, warm_posture(), false);
    let out = render_trust_envelope(&env);
    assert!(
        !out.contains("Likely-External Receiver Calls"),
        "enrichment ran but no external receivers → no section: {out}"
    );
}
