//! EXPLAIN-LIVEGRAPH-IMPL: the FOLD / DEGRADATION / FRESHNESS-MEET tests for `explain_to_coherent`.
//!
//! Child of the `coherent_tests` module (declared there via `#[path]`); reuses its builders + helpers via
//! `use super::*`. Split from the source-map tests so neither file exceeds the >500-line structural guardrail.
//!
//! Includes the review-5 STALE MULTI-SOURCE LEAF cases: a `{livegraph, sqlite}` served leaf whose SQLite
//! contributor is stale must MEET down to Stale (never stay Fresh+Exact on the LiveGraph half alone), while a
//! single-source `{livegraph}` leaf is deliberately NOT capped by SQLite-snapshot staleness.

use super::*;

#[test]
fn stale_index_makes_leaves_stale_and_caps_root() {
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol(), trust_signal()], Confidence::High),
        &ExplainLgDecisions::default(),
        None,
        true,
    );
    assert_eq!(
        leaf(&env, SignalCode::ExplainIdentity).freshness,
        FreshnessState::Stale
    );
    assert_eq!(env.freshness, FreshnessState::Stale);
    assert_ne!(env.trust.class, AnswerClass::Exact);
    assert_ne!(env.value.confidence, Confidence::High);
    assert!(has_limit(&env, LimitCode::SqliteSnapshotStale));
}

#[test]
fn identity_served_from_livegraph_is_stale_when_index_stale() {
    // review-5 #1/#2 (D-EXPLAIN-IDENTITY honesty): a SERVED {livegraph, sqlite} identity leaf whose SQLite
    // coordinate half is STALE must MEET down to Stale — it must NOT remain Fresh+Exact on the LiveGraph
    // anchor posture alone. The provenance stays multi-source (both sources genuinely build the evidence);
    // only trust + freshness are capped by the stale SQLite contributor.
    let lg = ExplainLgDecisions {
        identity: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol()], Confidence::High),
        &lg,
        None,
        true, // the backing index is stale (get_stale_files non-empty)
    );
    let id = leaf(&env, SignalCode::ExplainIdentity);
    assert_eq!(
        id.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite]),
        "served identity stays the D8 multi-source leaf even when stale (both sources build the evidence)"
    );
    assert_eq!(
        id.freshness,
        FreshnessState::Stale,
        "the stale SQLite coordinate half MEETs the served identity leaf down to Stale"
    );
    assert_ne!(
        id.trust.class,
        AnswerClass::Exact,
        "a stale multi-source leaf is never Exact (the false-freshness bug review-5 flagged)"
    );
    // The capped leaf lowers the root + confidence and emits the snapshot-stale limit.
    assert_eq!(env.freshness, FreshnessState::Stale);
    assert_ne!(env.trust.class, AnswerClass::Exact);
    assert_ne!(env.value.confidence, Confidence::High);
    assert!(has_limit(&env, LimitCode::SqliteSnapshotStale));
}

#[test]
fn callers_served_from_livegraph_is_stale_when_index_stale() {
    // The callgraph multi-source leaf caps the same way: the per-item module + top-3 grouping + SQL order are
    // SQLite-rendered, so a stale snapshot MEETs the served callers leaf down to Stale.
    let lg = ExplainLgDecisions {
        callers: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol(), callers_signal()], Confidence::High),
        &lg,
        None,
        true,
    );
    let callers = leaf(&env, SignalCode::ExplainCallers);
    assert_eq!(
        callers.provenance.source,
        BTreeSet::from([Source::Livegraph, Source::Sqlite])
    );
    assert_eq!(callers.freshness, FreshnessState::Stale);
    assert_ne!(callers.trust.class, AnswerClass::Exact);
    // Root freshness + confidence lower accordingly.
    assert_eq!(env.freshness, FreshnessState::Stale);
    assert_ne!(env.value.confidence, Confidence::High);
}

#[test]
fn imports_single_source_stays_fresh_when_index_stale() {
    // The DELIBERATE asymmetry — and why the MEET is per-leaf, not blanket. A single-source {livegraph}
    // imports leaf is served from the field-exact import cert (its value IS the LiveGraph value, NO SQLite
    // contributor), so a stale SQLite SNAPSHOT does NOT cap it: the LiveGraph reflects current state. Honesty
    // is preserved because the ROOT still MEETs down via the other (stale) SQLite leaves.
    let lg = ExplainLgDecisions {
        imports: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![imports_signal(), trust_signal()], Confidence::High),
        &lg,
        None,
        true,
    );
    let imports = leaf(&env, SignalCode::ExplainImports);
    assert_eq!(
        imports.provenance.source,
        BTreeSet::from([Source::Livegraph])
    );
    assert_eq!(
        imports.freshness,
        FreshnessState::Fresh,
        "a single-source {{livegraph}} leaf is not capped by SQLite-snapshot staleness"
    );
    assert_eq!(imports.trust.class, AnswerClass::Exact);
    // The SQLite trust leaf IS stale, so the ROOT still MEETs down to Stale — the answer stays honest.
    assert_eq!(
        leaf(&env, SignalCode::ExplainTrust).freshness,
        FreshnessState::Stale
    );
    assert_eq!(env.freshness, FreshnessState::Stale);
    assert!(has_limit(&env, LimitCode::SqliteSnapshotStale));
}

#[test]
fn precision_pending_lg_leaf_caps_root_and_emits_limit() {
    let lg = ExplainLgDecisions {
        callers: Some(OrientLeafLabel::Livegraph {
            class: AnswerClass::Partial,
            completeness: QueryCompleteness::Degraded,
            freshness: FreshnessState::PrecisionPending,
            degradation_reasons: Vec::new(),
            contributing_languages: ts_langs(),
        }),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol(), callers_signal()], Confidence::High),
        &lg,
        None,
        false,
    );
    assert_eq!(env.freshness, FreshnessState::PrecisionPending);
    assert_ne!(env.trust.class, AnswerClass::Exact);
    assert!(has_limit(&env, LimitCode::PrecisionPending));
}

#[test]
fn producer_unavailable_limit_when_lg_unavailable() {
    let lg = ExplainLgDecisions {
        imports: Some(OrientLeafLabel::SqliteFallback {
            reason: CoherenceFallbackReason::LiveGraphUnavailable,
        }),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![imports_signal()], Confidence::High),
        &lg,
        None,
        false,
    );
    assert!(has_limit(&env, LimitCode::ProducerUnavailable));
}

#[test]
fn healthy_explain_emits_no_provenance_limits() {
    let lg = ExplainLgDecisions {
        callers: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(
            vec![identity_symbol(), callers_signal(), trust_signal()],
            Confidence::High,
        ),
        &lg,
        None,
        false,
    );
    for code in [
        LimitCode::ProducerUnavailable,
        LimitCode::LivegraphPartial,
        LimitCode::SqliteSnapshotStale,
        LimitCode::AuthorityOverlayApplied,
        LimitCode::PrecisionPending,
    ] {
        assert!(
            !has_limit(&env, code),
            "{code:?} must not fire when healthy"
        );
    }
}

#[test]
fn zero_signal_is_resolution_only_not_structural_exact() {
    let mut result = symbol_result(Vec::new(), Confidence::High);
    result.focus = Focus::no_match("does-not-exist");
    let env = explain_to_coherent(result, &ExplainLgDecisions::default(), None, false);
    assert!(env.value.signals.is_empty());
    assert_ne!(env.trust.class, AnswerClass::Exact);
    assert_eq!(env.provenance.source, BTreeSet::from([Source::Sqlite]));
    assert!(env.trust.contributing_languages.is_empty());
    assert_eq!(env.value.confidence, Confidence::High);
}

#[test]
fn trust_briefing_rides_in_value_when_present() {
    let briefing = serde_json::json!({ "caveats": ["x"] });
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol()], Confidence::High),
        &ExplainLgDecisions::default(),
        Some(briefing.clone()),
        false,
    );
    assert_eq!(env.value.trust_briefing, Some(briefing));
    // Present on the wire under value.trust_briefing.
    let json = serde_json::to_value(&env).unwrap();
    assert!(json["value"]["trust_briefing"].is_object());
}

#[test]
fn trust_briefing_absent_when_not_degraded() {
    let env = explain_to_coherent(
        symbol_result(vec![identity_symbol()], Confidence::High),
        &ExplainLgDecisions::default(),
        None,
        false,
    );
    assert!(env.value.trust_briefing.is_none());
    let json = serde_json::to_value(&env).unwrap();
    assert!(json["value"].get("trust_briefing").is_none());
}

#[test]
fn pure_conversion_does_not_widen_inner_value() {
    // The PURE layer LABELS only; it must not widen/mutate the inner Signal payload (contract D1). The
    // LiveGraph VALUE construction (the caller identity set + live IR names, served from
    // `LiveGraph::callers`/`node_display`) is the DAEMON's job, proven LG-built in
    // `daemon-runtime/src/explain_coherence_tests.rs`. Here the daemon-built value is handed through as-is.
    let served = callers_signal();
    let before = serde_json::to_value(&served).unwrap();
    let lg = ExplainLgDecisions {
        callers: Some(lg_served()),
        ..Default::default()
    };
    let env = explain_to_coherent(
        symbol_result(vec![served], Confidence::High),
        &lg,
        None,
        false,
    );
    let after = serde_json::to_value(&leaf(&env, SignalCode::ExplainCallers).value).unwrap();
    assert_eq!(
        before, after,
        "the pure conversion must not widen the Signal payload"
    );
}
