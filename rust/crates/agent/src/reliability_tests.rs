//! Unit tests for the shared reader-frame call-reliability projection.
//!
//! These pin the DERIVATION (the in-scope rate that must exclude externals, the
//! measured-absent rules, the no-in-scope-calls honesty) and the WORDING tokens.
//! Proof that each SURFACE actually consumes THIS projection lives with each
//! surface's own tests, each asserting the exact reader-frame string only this
//! module produces:
//!
//! - `check`: `check::reduce::tests::call_graph_condition_summary_is_reader_frame_from_shared_view`.
//! - `trust`: rgr `trust_tests::render_resolution_*`, `render_names_top_external_targets_*`, and
//!   `render_reliability_zero_in_scope_calls_is_unknown_not_a_band` (the zero-denominator decision,
//!   routed through `resolution.is_none()`).
//! - `orient`: rgr `orient_tests::orient_renders_external_coverage_map_from_shared_view`,
//!   `orient_external_coverage_visible_when_call_graph_band_is_high`, and
//!   `orient_zero_in_scope_calls_is_honest_no_fabricated_rate`.
//! - `explain`: rgr `explain::tests::render_trust_*`.
//! - the `TRUST_LOW_RESOLUTION` signal: `dto::signal::tests::trust_low_resolution_summary_is_shared_reader_frame`
//!   and integration `orient_repo_signals::trust_low_resolution_total_excludes_external_via_shared_view`.
//!
//! (stats is N/A — no call-resolution surface). There is exactly ONE `derive` and
//! ONE set of wording fns (`resolved_phrase*` / `sentence_case` / `humanize_reason`);
//! the rgr copy that let trust drift was removed, and review-2's three residual
//! emitters (the aggregator in-scope total, the signal summary wording, trust's
//! zero-denominator decision) now route through THIS module.

use super::*;

#[test]
fn in_scope_rate_excludes_externals_from_denominator() {
    // 50 resolved, 5 internal-like unresolved, 80 external (NOT in denom).
    // In-scope = 50 / (50 + 5) = 90.9%, NOT 50 / (50 + 85) = 37%.
    let v = CallReliabilityView::derive(50, 5, 80, 135, vec![], Some(AgentReliabilityLevel::High));
    let r = v.resolution.expect("resolved rate present");
    assert_eq!(r.resolved, 50);
    assert_eq!(r.in_scope_or_unclassified_total, 55);
    assert!((r.pct - 90.909).abs() < 0.01, "got {}", r.pct);
}

#[test]
fn no_in_scope_calls_is_unknown_not_fabricated_full() {
    // Slice §3 / REVISE #3: zero in-scope calls is UNKNOWN, never rendered 100%.
    let v = CallReliabilityView::derive(0, 0, 0, 0, vec![], Some(AgentReliabilityLevel::High));
    assert_eq!(v.resolution, None);
    assert_eq!(v.resolved_phrase(), "no in-scope calls measured");
    // The band does NOT ride a no-calls line (a band over zero calls is vacuous).
    assert_eq!(v.resolved_with_band(), "no in-scope calls measured");
}

#[test]
fn resolved_phrase_is_reader_frame_not_pipeline_grade() {
    let v = CallReliabilityView::derive(42, 58, 0, 100, vec![], Some(AgentReliabilityLevel::Low));
    assert_eq!(v.resolved_phrase(), "your code's calls 42% resolved");
    assert_eq!(
        v.resolved_with_band(),
        "your code's calls 42% resolved (LOW)"
    );
    // Never the old grades-us vocabulary.
    assert!(!v.resolved_with_band().contains("call-graph"));
    assert!(!v.resolved_with_band().contains("call resolution rate"));
}

#[test]
fn external_share_names_the_share_and_next_action() {
    // 30 external of 130 total calls → 23%.
    let v =
        CallReliabilityView::derive(70, 30, 30, 130, vec![], Some(AgentReliabilityLevel::Medium));
    let line = v.external_line().expect("external line present");
    assert!(
        line.contains("23% of calls go into external libraries"),
        "{line}"
    );
    assert!(line.contains("follow to their crates/docs"), "{line}");
}

#[test]
fn external_share_known_zero_is_preserved_distinct_from_unknown() {
    // review-5 §2 (architecture rule 6: `0` = known-zero, `null` = unknown — never conflate).
    // KNOWN-ZERO: the heuristic ran over 130 calls and matched none. The projection PRESERVES
    // that as `Some(ExternalShare { external: 0, .. })` — a measured finding — and renders the
    // honest "none identified (heuristic)", never a fabricated "0% external" and never silence.
    let v = CallReliabilityView::derive(100, 30, 0, 130, vec![], Some(AgentReliabilityLevel::High));
    assert_eq!(
        v.external,
        Some(ExternalShare {
            external: 0,
            total_calls: 130,
            pct: 0.0,
        }),
        "known-zero external share is preserved, not collapsed to None"
    );
    let line = v
        .external_line()
        .expect("a 'none identified' line, not silence");
    assert!(
        line.contains("no external-library calls identified"),
        "{line}"
    );
    assert!(line.contains("not compiler-verified"), "{line}");
    assert!(!line.contains("0%"), "never a fabricated 0%: {line}");
    // UNKNOWN: with NO calls at all there is genuinely nothing to measure — `None`, NOT a
    // known-zero. This is the ONLY case that collapses to `None`, kept distinct from the
    // known-zero above (the rule-6 separation the test name promises).
    let empty = CallReliabilityView::derive(0, 0, 0, 0, vec![], None);
    assert_eq!(empty.external, None, "no calls at all is unknown (None)");
    assert_eq!(empty.external_line(), None);
}

#[test]
fn unclassified_caveat_fires_only_when_material() {
    // review-3 §2: the conservative-rate caveat is gated on materiality. 30 of 100 (30%) fires;
    // 10 of 100 (10% < 20%) does not; 0 unclassified or 0 denominator never fires.
    let c = unclassified_caveat(30, 100).expect("material unclassified fires");
    assert!(c.contains("30 of these 100 calls are unclassified"), "{c}");
    assert!(c.contains("true resolved share may be higher"), "{c}");
    assert_eq!(
        unclassified_caveat(10, 100),
        None,
        "immaterial share is silent"
    );
    assert_eq!(
        unclassified_caveat(0, 100),
        None,
        "no unclassified is silent"
    );
    assert_eq!(unclassified_caveat(50, 0), None, "no denominator is silent");
}

#[test]
fn named_target_line_singular_and_plural() {
    let many = ExternalTarget {
        type_name: "Value".into(),
        count: 425,
    };
    let one = ExternalTarget {
        type_name: "Once".into(),
        count: 1,
    };
    assert_eq!(
        CallReliabilityView::named_target_line(&many),
        "call on likely-external receiver `Value` (425 calls)"
    );
    assert_eq!(
        CallReliabilityView::named_target_line(&one),
        "call on likely-external receiver `Once` (1 call)"
    );
}

#[test]
fn named_coverage_map_line_caps_and_summarises_the_tail() {
    let v = CallReliabilityView::derive(
        10,
        10,
        50,
        100,
        vec![
            ExternalTarget {
                type_name: "Value".into(),
                count: 30,
            },
            ExternalTarget {
                type_name: "Vec".into(),
                count: 12,
            },
            ExternalTarget {
                type_name: "Once".into(),
                count: 3,
            },
        ],
        Some(AgentReliabilityLevel::Low),
    );
    let line = v.named_coverage_map_line(2).expect("named map present");
    assert!(line.contains("`Value` (30)"), "{line}");
    assert!(line.contains("`Vec` (12)"), "{line}");
    // Third target is summarised, not listed (compressed surface).
    assert!(!line.contains("`Once`"), "{line}");
    assert!(line.contains("+1 more"), "{line}");
    assert!(line.contains("follow to their crates/docs"), "{line}");
    // Empty when nothing external is named.
    let none = CallReliabilityView::derive(10, 10, 0, 20, vec![], None);
    assert_eq!(none.named_coverage_map_line(2), None);
}

#[test]
fn humanize_call_resolution_is_reader_frame() {
    assert_eq!(
        humanize_reason("call_resolution_rate=42.0%_below_50%"),
        "your code's calls 42% resolved (below 50% target)"
    );
}

#[test]
fn humanize_other_reasons_unchanged() {
    assert_eq!(
        humanize_reason("unresolved_imports=944"),
        "944 unresolved imports"
    );
    assert_eq!(
        humanize_reason("alias_resolution_suspicion"),
        "alias resolution suspected"
    );
    assert_eq!(
        humanize_reason("missing_entrypoint_declarations"),
        "no entrypoints declared"
    );
    assert_eq!(
        humanize_reason("registry_pattern_suspicion"),
        "registry/factory patterns detected"
    );
    assert_eq!(humanize_reason("some_unknown_token"), "some unknown token");
}

#[test]
fn band_label_is_uppercase_reader_facing() {
    assert_eq!(band_label(AgentReliabilityLevel::Low), "LOW");
    assert_eq!(band_label(AgentReliabilityLevel::Medium), "MEDIUM");
    assert_eq!(band_label(AgentReliabilityLevel::High), "HIGH");
}

#[test]
fn resolved_phrase_with_band_is_the_one_band_convention() {
    // The banded phrase used by check (typed band) and orient (wire-string band)
    // is one string here — the same body `resolved_with_band` produces.
    assert_eq!(
        resolved_phrase_with_band(42.0, "LOW"),
        "your code's calls 42% resolved (LOW)"
    );
    let v = CallReliabilityView::derive(42, 58, 0, 100, vec![], Some(AgentReliabilityLevel::Low));
    assert_eq!(
        v.resolved_with_band(),
        resolved_phrase_with_band(42.0, "LOW")
    );
}

#[test]
fn sentence_case_capitalizes_first_char_only() {
    // Consolidated from `check::evaluate` (review-2 §2): the reader-frame phrase is capitalised
    // for surfaces that emit it as a standalone sentence (check condition, TRUST_LOW_RESOLUTION
    // signal). Only the first char changes; the rest — including the "(LOW)" band — is untouched.
    assert_eq!(
        sentence_case("your code's calls 42% resolved (LOW)"),
        "Your code's calls 42% resolved (LOW)"
    );
    assert_eq!(
        sentence_case(NO_IN_SCOPE_CALLS),
        "No in-scope calls measured"
    );
    assert_eq!(sentence_case(""), "");
}

#[test]
fn band_from_wire_maps_serialized_levels_case_insensitively() {
    assert_eq!(band_from_wire("LOW"), Some(AgentReliabilityLevel::Low));
    assert_eq!(
        band_from_wire("Medium"),
        Some(AgentReliabilityLevel::Medium)
    );
    assert_eq!(band_from_wire("high"), Some(AgentReliabilityLevel::High));
    // Unknown token → None (unknown is never a fabricated band).
    assert_eq!(band_from_wire("BOGUS"), None);
}
