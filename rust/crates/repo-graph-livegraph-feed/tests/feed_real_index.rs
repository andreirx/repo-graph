//! LIVEGRAPH-INTEGRATION-1A acceptance: REAL scip-ingest output → LiveGraph → trust-labelled answers.
//!
//! Uses the committed real index (`repo-graph-scip-ingest/tests/fixtures/synthetic/index.scip`) via
//! `ingest_partition` — NO hand-built `PartitionIr`. Single partition (callers/callees are
//! intra-partition); cross-partition real data is the named residual (LIVEGRAPH-INTEGRATION-XPART-1).

use repo_graph_ir::{EdgeType, IdentitySource};
use repo_graph_livegraph::LiveGraph;
use repo_graph_livegraph_feed::feed_partition;
use repo_graph_scip_ingest::{decode_index, ingest_partition, IngestOutcome};
use repo_graph_trust_model::{
    AnswerClass, DegradationReason, FreshnessState, Granularity, LanguageSupport,
};
use std::fs;

fn fixture_root() -> String {
    format!(
        "{}/../repo-graph-scip-ingest/tests/fixtures/synthetic",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn ingest_synthetic() -> IngestOutcome {
    let root = fixture_root();
    let scip = fs::read(format!("{root}/index.scip")).expect("read committed index.scip");
    let index = decode_index(&scip).expect("decode scip");
    ingest_partition(
        &index,
        &root,
        "synthetic",
        "synthetic",
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    )
}

fn describe_key(outcome: &IngestOutcome) -> String {
    outcome
        .ir
        .nodes
        .iter()
        .find(|n| n.key.as_str().ends_with("Circle.describe:SYMBOL:METHOD"))
        .map(|n| n.key.as_str().to_string())
        .expect("Circle.describe node in real fixture")
}

#[test]
fn real_value_facts_for_ast_adopted_symbol_is_exact() {
    let outcome = ingest_synthetic();
    let key = describe_key(&outcome);
    let expected_cx = *outcome
        .complexity
        .get(&key)
        .expect("describe complexity attached");

    let mut lg = LiveGraph::new();
    feed_partition(
        &mut lg,
        "synthetic",
        outcome,
        LanguageSupport::TypeScriptPrimary,
    );

    let a = lg.value_facts(&key);
    // Real AST-adopted symbol owns its complexity (basis SymbolOwnership-complete), fresh → Exact.
    assert_eq!(a.class(), AnswerClass::Exact);
    assert_eq!(a.freshness(), FreshnessState::Fresh);
    let facts = a.data().expect("value facts present");
    assert_eq!(facts.facts.len(), 1);
    assert_eq!(facts.facts[0].value, expected_cx);
    assert!(a
        .contributing_languages()
        .contains(&LanguageSupport::TypeScriptPrimary));
}

#[test]
fn real_callers_and_callees_over_ingested_edges() {
    let outcome = ingest_synthetic();
    // A real Calls edge from the committed index → its endpoints exercise callers/callees.
    let (caller_src, callee_dst) = outcome
        .ir
        .edges
        .iter()
        .find(|e| e.edge_type == EdgeType::Calls)
        .map(|e| (e.src.as_str().to_string(), e.dst.as_str().to_string()))
        .expect("at least one real Calls edge in the fixture");

    let mut lg = LiveGraph::new();
    feed_partition(
        &mut lg,
        "synthetic",
        outcome,
        LanguageSupport::TypeScriptPrimary,
    );

    // callers(callee): who calls it → non-empty, resident + fresh → a valid trust class (not Unavailable).
    let callers = lg.callers(&callee_dst, Granularity::CallerDetail);
    assert_ne!(callers.class(), AnswerClass::Unavailable);
    assert_eq!(callers.freshness(), FreshnessState::Fresh);
    assert!(
        !callers
            .data()
            .expect("callers data")
            .caller_identities
            .is_empty(),
        "real callers must be non-empty"
    );
    assert!(callers
        .contributing_languages()
        .contains(&LanguageSupport::TypeScriptPrimary));

    // callees(caller): whom it calls → non-empty, resolved intra-partition, fresh → valid trust class.
    let callees = lg.callees(&caller_src, Granularity::CallerDetail);
    assert_ne!(callees.class(), AnswerClass::Unavailable);
    assert_eq!(callees.freshness(), FreshnessState::Fresh);
    assert!(
        !callees
            .data()
            .expect("callees data")
            .callee_identities
            .is_empty(),
        "real callees must be non-empty"
    );
}

#[test]
fn real_file_scope_symbols_walk_both_directions_without_panic() {
    // LIVEGRAPH-PARTIAL-FIX-1 regression on REAL producer output (RECON-SPIKE-1 finding #0). The SCIP
    // producer materializes an `AstFileScope` FILE node for each top-level `import` (`main.ts:FILE`,
    // `shapes.ts:FILE`). Walking those symbols BOTH directions must NOT panic — there is deliberately NO
    // catch_unwind here, so a `finalize_envelope` panic (`partial invariant holds: PartialRequiresReasons`)
    // would abort the test — and must degrade to a reason-justified `Partial`, symmetric in both directions.
    let outcome = ingest_synthetic();

    // The `AstFileScope` FILE keys in the committed index — collected BEFORE `feed_partition` consumes it.
    let file_keys: Vec<String> = outcome
        .ir
        .nodes
        .iter()
        .filter(|n| n.identity_source == IdentitySource::AstFileScope)
        .map(|n| n.key.as_str().to_string())
        .collect();
    assert!(
        !file_keys.is_empty(),
        "the real fixture must contain AstFileScope FILE symbols (else this regression is vacuous)"
    );

    let mut lg = LiveGraph::new();
    feed_partition(
        &mut lg,
        "synthetic",
        outcome,
        LanguageSupport::TypeScriptPrimary,
    );

    for key in &file_keys {
        // Reaching these assertions at all proves the walk did not panic (no catch_unwind).
        let callers = lg.callers(key, Granularity::CallerDetail);
        assert_eq!(
            callers.class(),
            AnswerClass::Partial,
            "callers({key}): a call-graph-incomplete FILE basis degrades to Partial, never panics"
        );
        assert!(
            callers
                .degradation_reasons()
                .contains(&DegradationReason::StructuralNodeNoCallGraphContent),
            "callers({key}): the Partial must carry the honest structural-node reason \
             (the panic WAS the empty-reason case)"
        );

        let callees = lg.callees(key, Granularity::CallerDetail);
        assert_eq!(
            callees.class(),
            AnswerClass::Partial,
            "callees({key}): symmetric clean degradation, not a panic"
        );
        assert!(
            callees
                .degradation_reasons()
                .contains(&DegradationReason::StructuralNodeNoCallGraphContent),
            "callees({key}): the Partial must carry the honest structural-node reason"
        );
    }
}

#[test]
fn real_value_facts_are_epoch_bound() {
    // Feed, then re-ingest + swap WITHOUT reloading value facts → facts are from a superseded epoch →
    // value_facts reports Stale (D7), on real ingested data.
    let outcome = ingest_synthetic();
    let key = describe_key(&outcome);
    let mut lg = LiveGraph::new();
    feed_partition(
        &mut lg,
        "synthetic",
        outcome,
        LanguageSupport::TypeScriptPrimary,
    );
    assert_eq!(lg.value_facts(&key).class(), AnswerClass::Exact);

    let outcome2 = ingest_synthetic(); // deterministic re-ingest (INGEST-CORE-1 group 1)
    lg.swap_partition("synthetic", outcome2.ir); // bumps epoch; value facts NOT reloaded
    let after = lg.value_facts(&key);
    assert_eq!(
        after.class(),
        AnswerClass::Stale,
        "stale after swap without value reload"
    );
    assert!(after.data().is_some(), "last-good served, never empty");
}
