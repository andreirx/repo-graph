//! COHERENCE-LEAF-SERVE-IMPL-1 (review-3 item 3): orient per-leaf decision PURE unit tests, extracted
//! from `orient_lg_decisions.rs` (structural guardrail — the file held ~1045 lines of tests). These are
//! the no-`RepoState` proofs: the `orient_outcome_from_env` Fresh->Exact->TS ladder + the
//! `gate_callgraph_no_loss` per-symbol value-equivalence proof. `super` is `orient_lg_decisions`, so the
//! private functions remain reachable exactly as when this module was inline.

use super::*;

// ── ORIENT-LIVEGRAPH-IMPL: the orient LG-leaf decision ladder (orient_outcome_from_env). ──
// Pure over an AnswerEnvelope (no RepoState): proves the Fresh -> Exact -> TS-only gating and the
// posture projection that the orient cycles/callers/callees leaf decisions rely on.

fn ts_set() -> std::collections::BTreeSet<LanguageSupport> {
    std::collections::BTreeSet::from([LanguageSupport::TypeScriptPrimary])
}

#[test]
fn orient_outcome_exact_fresh_ts_is_livegraph() {
    let env = AnswerEnvelope::exact(
        1u32,
        QueryCompleteness::Complete,
        FreshnessState::Fresh,
        vec![],
        ts_set(),
    )
    .unwrap();
    match orient_outcome_from_env(&env) {
        OrientLgOutcome::Livegraph {
            class,
            completeness,
            freshness,
            contributing_languages,
            ..
        } => {
            assert_eq!(class, AnswerClass::Exact);
            assert_eq!(completeness, QueryCompleteness::Complete);
            assert_eq!(freshness, FreshnessState::Fresh);
            assert_eq!(contributing_languages, ts_set());
        }
        OrientLgOutcome::Fallback { .. } => panic!("expected Livegraph for Exact+Fresh+TS"),
    }
}

#[test]
fn orient_outcome_partial_falls_back_partial() {
    let env = AnswerEnvelope::partial(
        Some(1u32),
        vec![DegradationReason::ScipFallbackIdentity],
        vec![],
        FreshnessState::Fresh,
        vec![],
        ts_set(),
    )
    .unwrap();
    assert!(matches!(
        orient_outcome_from_env(&env),
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphPartial
        }
    ));
}

#[test]
fn orient_outcome_stale_falls_back_stale_before_class() {
    // Freshness is checked BEFORE class, so a Stale answer reports LiveGraphStale (not Partial).
    let env = AnswerEnvelope::stale(
        1u32,
        FreshnessState::Stale,
        vec![],
        vec![],
        vec![],
        ts_set(),
    )
    .unwrap();
    assert!(matches!(
        orient_outcome_from_env(&env),
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphStale
        }
    ));
}

#[test]
fn orient_outcome_non_ts_falls_back_unsupported_language() {
    let langs = std::collections::BTreeSet::from([LanguageSupport::RustPartialBeta]);
    let env = AnswerEnvelope::exact(
        1u32,
        QueryCompleteness::Complete,
        FreshnessState::Fresh,
        vec![],
        langs,
    )
    .unwrap();
    assert!(matches!(
        orient_outcome_from_env(&env),
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphUnsupportedLanguage
        }
    ));
}

// ── ORIENT-LIVEGRAPH-IMPL: the callers/callees VALUE-EQUIVALENCE proof (gate_callgraph_no_loss). ──
// PURE over a ladder outcome + the LG key set + a SQLite-keys closure. A panicking closure proves the
// already-fallback path SKIPS the SQLite read; set-equality is order-independent; any divergence or
// read error NEVER yields a `livegraph` label.

fn lg_ladder() -> OrientLgOutcome {
    OrientLgOutcome::Livegraph {
        class: AnswerClass::Exact,
        completeness: QueryCompleteness::Complete,
        freshness: FreshnessState::Fresh,
        degradation_reasons: vec![],
        contributing_languages: ts_set(),
    }
}

fn keyset(keys: &[&str]) -> std::collections::BTreeSet<String> {
    keys.iter().map(|s| s.to_string()).collect()
}

#[test]
fn callgraph_no_loss_equal_sets_keeps_livegraph() {
    // LG key set == SQLite key set (order-independent) -> the proven `livegraph` label is kept.
    let out = gate_callgraph_no_loss(lg_ladder(), keyset(&["a", "b"]), || {
        Ok::<_, ()>(keyset(&["b", "a"]))
    });
    assert!(matches!(out, OrientLgOutcome::Livegraph { .. }));
}

#[test]
fn callgraph_no_loss_divergent_sets_falls_back_callgraph_divergence() {
    // A divergence (SQLite has `c`, LiveGraph has `b`) -> labelled SQLite fallback, never `livegraph`.
    let out = gate_callgraph_no_loss(lg_ladder(), keyset(&["a", "b"]), || {
        Ok::<_, ()>(keyset(&["a", "c"]))
    });
    assert!(matches!(
        out,
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphCallgraphDivergence
        }
    ));
}

#[test]
fn callgraph_no_loss_storage_error_falls_back_error() {
    // Cannot prove equivalence (SQLite read errored) -> safe SQLite fallback, never `livegraph`.
    let out = gate_callgraph_no_loss(lg_ladder(), keyset(&["a"]), || {
        Err::<std::collections::BTreeSet<String>, _>(())
    });
    assert!(matches!(
        out,
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphError
        }
    ));
}

#[test]
fn callgraph_no_loss_already_fallback_skips_sqlite_read() {
    // The ladder already fell back (non-resident partition) -> the SQLite compare read is SKIPPED.
    let ladder = OrientLgOutcome::Fallback {
        reason: FallbackReason::LiveGraphStale,
    };
    let out = gate_callgraph_no_loss(
        ladder,
        keyset(&["a"]),
        || -> Result<std::collections::BTreeSet<String>, ()> {
            panic!("SQLite caller/callee keys must NOT be read when the ladder already fell back")
        },
    );
    assert!(matches!(
        out,
        OrientLgOutcome::Fallback {
            reason: FallbackReason::LiveGraphStale
        }
    ));
}
