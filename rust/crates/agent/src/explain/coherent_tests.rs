//! EXPLAIN-LIVEGRAPH-IMPL: unit tests for `explain_to_coherent` — the SHARED builders + the per-leaf
//! SOURCE-MAP (provenance labelling) tests. Split out of `coherent.rs` to respect the >500-line structural
//! guardrail (`coherent.rs` keeps the pure conversion logic only). The FOLD / DEGRADATION / FRESHNESS-MEET
//! tests live in the sibling child module `coherent_fold_tests.rs` (declared at the bottom), which reuses
//! these builders via `use super::*` — keeping both test files comfortably under the guardrail.

use super::*;
use crate::dto::envelope::{Confidence, Focus, EXPLAIN_COMMAND, ORIENT_SCHEMA};
use crate::dto::limit::LimitCode;
use crate::dto::signal::{
    BoundaryViolationEvidence, CycleEvidence, ExplainBoundaryEvidence, ExplainCallerItem,
    ExplainCallersEvidence, ExplainCyclesEvidence, ExplainGateEvidence, ExplainIdentityEvidence,
    ExplainImportItem, ExplainImportsEvidence, ExplainSymbolItem, ExplainSymbolsEvidence,
    ExplainTrustEvidence,
};
use repo_graph_coherence::{
    AnswerClass, CoherenceFallbackReason, FreshnessState, LanguageSupport, QueryCompleteness,
};
use std::collections::BTreeSet;

// ── Builders (shared with the child `coherent_fold_tests` module via `use super::*`) ──

pub(super) fn ts_langs() -> BTreeSet<LanguageSupport> {
    BTreeSet::from([LanguageSupport::TypeScriptPrimary])
}

pub(super) fn lg_served() -> OrientLeafLabel {
    OrientLeafLabel::Livegraph {
        class: AnswerClass::Exact,
        completeness: QueryCompleteness::Complete,
        freshness: FreshnessState::Fresh,
        degradation_reasons: Vec::new(),
        contributing_languages: ts_langs(),
    }
}

pub(super) fn identity_symbol() -> Signal {
    Signal::explain_identity(ExplainIdentityEvidence {
        target_kind: "symbol".to_string(),
        path: Some("src/a.ts".to_string()),
        stable_key: Some("r:src/a.ts:Foo.bar:SYMBOL".to_string()),
        name: Some("bar".to_string()),
        subtype: Some("method".to_string()),
        line_start: Some(10),
        language: None,
        is_test: None,
        module_path: Some("src".to_string()),
        file_count: None,
        symbol_count: None,
    })
}

pub(super) fn callers_signal() -> Signal {
    Signal::explain_callers(ExplainCallersEvidence {
        count: 1,
        top_modules: Vec::new(),
        items: vec![ExplainCallerItem {
            stable_key: "r:src/b.ts:Caller.x:SYMBOL".to_string(),
            name: "x".to_string(),
            module: Some("src".to_string()),
        }],
        items_truncated: None,
        items_omitted_count: None,
    })
}

pub(super) fn imports_signal() -> Signal {
    Signal::explain_imports(ExplainImportsEvidence {
        count: 1,
        items: vec![ExplainImportItem {
            target_file: "src/c.ts".to_string(),
        }],
        items_truncated: None,
        items_omitted_count: None,
    })
}

pub(super) fn cycles_signal() -> Signal {
    Signal::explain_cycles(ExplainCyclesEvidence {
        count: 1,
        items: vec![CycleEvidence {
            length: 2,
            modules: vec!["a".to_string(), "b".to_string()],
            type_only: None,
        }],
        items_truncated: None,
        items_omitted_count: None,
    })
}

pub(super) fn boundary_signal() -> Signal {
    Signal::explain_boundary(ExplainBoundaryEvidence {
        violation_count: 1,
        items: vec![BoundaryViolationEvidence {
            source_module: "a".to_string(),
            target_module: "b".to_string(),
            edge_count: 1,
        }],
        items_truncated: None,
        items_omitted_count: None,
    })
}

pub(super) fn gate_signal() -> Signal {
    Signal::explain_gate(ExplainGateEvidence {
        outcome: "pass".to_string(),
        obligation_count: 1,
        items: Vec::new(),
        items_truncated: None,
        items_omitted_count: None,
    })
}

pub(super) fn symbols_signal() -> Signal {
    Signal::explain_symbols(ExplainSymbolsEvidence {
        count: 1,
        items: vec![ExplainSymbolItem {
            name: "bar".to_string(),
            subtype: Some("method".to_string()),
            line_start: Some(10),
        }],
        items_truncated: None,
        items_omitted_count: None,
    })
}

pub(super) fn trust_signal() -> Signal {
    Signal::explain_trust(ExplainTrustEvidence {
        call_resolution_rate: 0.9,
        call_graph_reliability: "high".to_string(),
        enrichment_state: "ran".to_string(),
        // In-scope-or-unclassified counts consistent with the 0.9 rate (90 / 100).
        resolved_in_scope: 90,
        in_scope_or_unclassified_total: 100,
    })
}

pub(super) fn symbol_result(signals: Vec<Signal>, confidence: Confidence) -> OrientResult {
    OrientResult {
        schema: ORIENT_SCHEMA,
        command: EXPLAIN_COMMAND,
        repo: "demo".to_string(),
        display_name: Some("demo".to_string()),
        snapshot: "snap-1".to_string(),
        focus: Focus::symbol("Foo.bar", "r:src/a.ts:Foo.bar:SYMBOL", Some("src/a.ts")),
        confidence,
        documentation: None,
        signals,
        signals_truncated: None,
        signals_omitted_count: None,
        limits: Vec::new(),
        limits_truncated: None,
        limits_omitted_count: None,
        next: Vec::new(),
        next_truncated: None,
        next_omitted_count: None,
        truncated: false,
    }
}

pub(super) fn leaf(
    env: &CoherenceEnvelope<CoherentOrientResult>,
    code: SignalCode,
) -> &CoherenceEnvelope<Signal> {
    env.value
        .signals
        .iter()
        .find(|l| l.value.code() == code)
        .expect("leaf present")
}

pub(super) fn has_limit(env: &CoherenceEnvelope<CoherentOrientResult>, code: LimitCode) -> bool {
    env.value.limits.iter().any(|l| l.code == code)
}

// ── Per-leaf SOURCE MAP (provenance labelling) ──

#[test]
fn identity_without_decision_collapses_to_sqlite() {
    // NO daemon decision = the daemon made no LiveGraph ATTEMPT for this leaf. In production this is the
    // FILE/PATH-focus listings identity (D-EXPLAIN-LISTINGS) — no symbol anchor, so no attempt. The proven
    // SQLite primary, honestly UNLABELLED: single-source {sqlite}, NO fallback reason (no LG was tried). A
    // FAILED symbol-focus attempt is the SEPARATE labelled case below.
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol()], Confidence::High),
        &ExplainLgDecisions::default(),
        None,
        false,
    );
    let id = leaf(&env, SignalCode::ExplainIdentity);
    assert_eq!(id.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert!(!id.provenance.source.contains(&Source::Livegraph));
    assert!(id.provenance.fallback_reason.is_none());
    assert_eq!(id.freshness, FreshnessState::Fresh);
}

#[test]
fn identity_failed_attempt_is_labelled_sqlite_fallback() {
    // A COMMITTED symbol-focus LG-first attempt that could not serve the live anchor (no LiveGraph /
    // non-resident / stale / non-TS / no live node) arrives as a `SqliteFallback { reason }` decision → the
    // proven SQLite identity primary, LABELLED with the cert-ladder reason (review-7: a failed attempt is
    // NEVER an unlabelled {sqlite} leaf — that would hide a real degradation as the proven primary).
    let lg = ExplainLgDecisions {
        identity: Some(OrientLeafLabel::SqliteFallback {
            reason: CoherenceFallbackReason::LiveGraphUnavailable,
        }),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol()], Confidence::High),
        &lg,
        None,
        false,
    );
    let id = leaf(&env, SignalCode::ExplainIdentity);
    assert_eq!(id.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert!(!id.provenance.source.contains(&Source::Livegraph));
    assert_eq!(
        id.provenance.fallback_reason,
        Some(CoherenceFallbackReason::LiveGraphUnavailable),
        "a failed symbol-focus identity attempt is a LABELLED {{sqlite}} fallback"
    );
}

#[test]
fn identity_served_from_livegraph_is_multi_source() {
    // The daemon served the anchor (name/subtype) from current-state LiveGraph IR; coordinates SQLite.
    let lg = ExplainLgDecisions {
        identity: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol()], Confidence::High),
        &lg,
        None,
        false,
    );
    let id = leaf(&env, SignalCode::ExplainIdentity);
    assert_eq!(
        id.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "served identity is the D8 {{livegraph, sqlite}} leaf (anchor LG + coordinates SQLite)"
    );
    assert!(id.provenance.fallback_reason.is_none());
}

#[test]
fn callers_served_from_livegraph_is_multi_source() {
    let lg = ExplainLgDecisions {
        callers: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol(), callers_signal()], Confidence::High),
        &lg,
        None,
        false,
    );
    let callers = leaf(&env, SignalCode::ExplainCallers);
    assert_eq!(
        callers.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "callers served from LiveGraph is jointly sourced (identity set + per-item name from LiveGraph; \
             per-item module + grouping rendered from SQLite)"
    );
    assert_eq!(callers.trust.class, AnswerClass::Exact);
    assert!(callers.provenance.fallback_reason.is_none());
    // The root provenance UNION carries both.
    assert!(env.provenance.source.contains(&Source::Livegraph));
    assert!(env.provenance.source.contains(&Source::Sqlite));
}

#[test]
fn imports_served_from_livegraph_is_single_source() {
    // The daemon BUILT the value from `live_import_view` under the field-exact import cert -> {livegraph}.
    let lg = ExplainLgDecisions {
        imports: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![imports_signal()], Confidence::High),
        &lg,
        None,
        false,
    );
    let imports = leaf(&env, SignalCode::ExplainImports);
    assert_eq!(
        imports.provenance.source,
        BTreeSet::from([Source::Livegraph]),
        "imports served from the field-exact import cert is single-source {{livegraph}}"
    );
}

#[test]
fn cycles_served_from_livegraph_is_single_source() {
    // Served from `module_import_cycles` under the field-exact module-cycle cert -> {livegraph}.
    let lg = ExplainLgDecisions {
        cycles: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![cycles_signal()], Confidence::High),
        &lg,
        None,
        false,
    );
    let cycles = leaf(&env, SignalCode::ExplainCycles);
    assert_eq!(
        cycles.provenance.source,
        BTreeSet::from([Source::Livegraph]),
        "cycles served from the field-exact module-cycle cert is single-source {{livegraph}}"
    );
}

#[test]
fn callers_fallback_is_sqlite_with_reason() {
    let lg = ExplainLgDecisions {
        callers: Some(OrientLeafLabel::SqliteFallback {
            reason: CoherenceFallbackReason::LiveGraphCallgraphDivergence,
        }),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![callers_signal()], Confidence::High),
        &lg,
        None,
        false,
    );
    let callers = leaf(&env, SignalCode::ExplainCallers);
    assert_eq!(callers.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert!(!callers.provenance.source.contains(&Source::Livegraph));
    assert_eq!(
        callers.provenance.fallback_reason,
        Some(CoherenceFallbackReason::LiveGraphCallgraphDivergence)
    );
}

#[test]
fn lg_first_without_decision_defaults_to_sqlite_no_fallback() {
    // No daemon decision (e.g. no populated LiveGraph) -> the proven SQLite primary, no fallback reason.
    let env = explain_to_coherent(
        symbol_result(vec![callers_signal()], Confidence::High),
        &ExplainLgDecisions::default(),
        None,
        false,
    );
    let callers = leaf(&env, SignalCode::ExplainCallers);
    assert_eq!(callers.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert!(callers.provenance.fallback_reason.is_none());
}

#[test]
fn boundary_is_multi_source_and_gate_is_declaration() {
    let env = explain_to_coherent(
        symbol_result(vec![boundary_signal(), gate_signal()], Confidence::High),
        &ExplainLgDecisions::default(),
        None,
        false,
    );
    let boundary = leaf(&env, SignalCode::ExplainBoundary);
    assert_eq!(
        boundary.provenance.source,
        BTreeSet::from([Source::Sqlite, Source::Declaration])
    );
    let gate = leaf(&env, SignalCode::ExplainGate);
    assert_eq!(
        gate.provenance.source,
        BTreeSet::from([Source::Declaration])
    );
    // AUTHORITY_OVERLAY_APPLIED fires when a declaration source is present.
    assert!(has_limit(&env, LimitCode::AuthorityOverlayApplied));
}

#[test]
fn symbols_and_trust_are_sqlite() {
    let env = explain_to_coherent(
        symbol_result(vec![symbols_signal(), trust_signal()], Confidence::High),
        &ExplainLgDecisions::default(),
        None,
        false,
    );
    assert_eq!(
        leaf(&env, SignalCode::ExplainSymbols).provenance.source,
        BTreeSet::from([Source::Sqlite])
    );
    assert_eq!(
        leaf(&env, SignalCode::ExplainTrust).provenance.source,
        BTreeSet::from([Source::Sqlite])
    );
}

// The fold / degradation / freshness-MEET tests (incl. the review-5 stale multi-source leaf cases) live in
// this sibling child module so neither test file exceeds the >500-line structural guardrail. It reuses the
// builders above via `use super::*`.
#[path = "coherent_fold_tests.rs"]
mod fold_tests;
