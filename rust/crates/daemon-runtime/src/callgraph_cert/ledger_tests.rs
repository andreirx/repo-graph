//! RECON-M-R1 gate tests — the witness ledger (recon-design-1 §6.1 M-R1 gate column).
//!
//! Grouped per the gate: the INSTANCE fixtures (§3.3 — the measured-empty `multiplicity`
//! sub-classes fixture-proven, exact closure), the REGIME tests (§4.2 — exclusivity +
//! exhaustiveness over the state matrix; stale scoping), CAPTURE-CONTRACT byte-parity (the GREEN
//! gate preserved through M-R1), the hand-built-`PartitionIr` COLLISION-GUARD test (R-RAT-4), the
//! `identity_suspect` detector (guard 3), GREEN/RED derivation equivalence, and the committed
//! fixture's 7/0/2/9 canonical classification + per-kind RECORD (the spike baseline).

use super::ledger::{classify_state, PinState, StateClass, WOneReason};
use super::{callgraph_cert_eligibility, callgraph_is_green, test_fixture};
use repo_graph_ir::EdgeType;

/// Run the serve-ladder build and return the stored ledger (cloned).
fn built_ledger(f: &test_fixture::Fixture) -> super::ledger::WitnessLedger {
    let _ = callgraph_is_green(&f.state, &f.snapshot_uid);
    f.state
        .witness_ledger
        .read()
        .clone()
        .expect("ledger stored by the cert build")
}

// ── The §3.3 INSTANCE fixtures (M-R1 gate: exact closure; measured-empty classes proven) ─────

#[test]
fn instance_fixture_p2_s1_yields_one_both_one_syntactic_multiplicity() {
    // P=2/S-`Calls`=1 → 1 `both` + 1 `syntactic`/`multiplicity`, dual_measured 2, agreement 50%,
    // closure exact (the R-RAT-5 instance rule: min corroborated, excess per side — the
    // disagreement is IN the rate, never absorbed).
    let f = test_fixture::build_multiplicity_fixture(2, 1);
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    assert_eq!(c.pipeline_calls, 2, "P instances (multiplicity preserved)");
    assert_eq!(c.both, 1, "min(2,1) instances corroborate");
    assert_eq!(c.both_identities, 1);
    assert_eq!(c.syntactic.multiplicity, 1, "the P-excess instance");
    assert_eq!(c.syntactic.boundary, 0);
    assert_eq!(c.syntactic.file_scope, 0);
    assert_eq!(c.syntactic.uncorroborated, 0);
    assert_eq!(c.syntactic.identities, 1);
    assert_eq!(c.semantic.total(), 0);
    assert_eq!(c.unmeasured_edges, 0);
    assert_eq!(
        c.dual_measured, 2,
        "both + syntactic — the agreement denominator"
    );
    assert_eq!(
        c.agreement_pct(),
        Some(50.0),
        "1/2 in percentage points (50, not 0.5) — the delta depresses the rate"
    );
    // Closures (§5.4, instance-exact): pipeline = both + syntactic + unmeasured; union adds semantic.
    assert_eq!(
        c.pipeline_calls,
        c.both + c.syntactic.total() + c.unmeasured_edges
    );
    assert_eq!(c.union_calls, 2);
    // The delta pair is retained with its EXACT (p, s) for doctor's enumeration (§5.1).
    assert_eq!(c.delta_pairs.len(), 1);
    assert_eq!((c.delta_pairs[0].p, c.delta_pairs[0].s_calls), (2, 1));
    // The pair record (the M-R2 serving substrate): corroborated pair -> no pair-level subclass.
    let rec = c
        .pairs
        .get(&(test_fixture::caller_key(), test_fixture::callee_key()))
        .expect("pair record");
    assert_eq!((rec.p, rec.s_calls, rec.dual_measured), (2, 1, true));
    assert_eq!(
        rec.syntactic_subclass, None,
        "sc > 0 -> multiplicity, not pair-level"
    );
}

#[test]
fn instance_fixture_p1_s2_yields_one_both_one_semantic_multiplicity() {
    // P=1/S-`Calls`=2 → 1 `both` + 1 `semantic`/`multiplicity`, union count 2, closure exact.
    // Every P occurrence is corroborated (min = p), so the P side carries NO syntactic instance.
    let f = test_fixture::build_multiplicity_fixture(1, 2);
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    assert_eq!(c.pipeline_calls, 1);
    assert_eq!(
        c.both, 1,
        "min(1,2) — the P occurrence is fully corroborated"
    );
    assert_eq!(c.syntactic.total(), 0);
    assert_eq!(c.semantic.multiplicity, 1, "the S-excess instance");
    assert_eq!(c.semantic.new_pair, 0);
    assert_eq!(c.semantic.identities, 1);
    assert_eq!(c.unmeasured_edges, 0);
    assert_eq!(
        c.union_calls, 2,
        "P 1 + S excess 1 (the served MAX rule's population)"
    );
    assert_eq!(c.dual_measured, 1);
    assert_eq!(
        c.agreement_pct(),
        Some(100.0),
        "every dual-measured P instance corroborated — 100 percentage points"
    );
    assert_eq!(c.delta_pairs.len(), 1);
    assert_eq!((c.delta_pairs[0].p, c.delta_pairs[0].s_calls), (1, 2));
    // Rollup attribution: the S-witnessed instances land on the witnessing partition.
    let rollup = c
        .rollups
        .get(&(
            repo_graph_trust_model::LanguageSupport::TypeScriptPrimary,
            "p".to_string(),
        ))
        .expect("partition rollup");
    assert_eq!(rollup.both_instances, 1);
    assert_eq!(rollup.semantic_multiplicity, 1);
}

// ── The §4.2 REGIME tests (R-RAT-6 + the iteration-6 eligibility/activation split) ───────────

#[test]
fn stale_partition_is_w_one_and_contributes_no_classification_rows() {
    // A resident partition marked stale (`mark_stale`) → W-ONE(`stale`); the ledger holds NO
    // classification rows for it (rule e: a stale S beside a current P would mint FALSE
    // divergence describing OUR refresh lag); serving stays byte-identical pipeline (the verdict
    // is RED → the SQLite fallback, exactly today's channel).
    let f = test_fixture::build_fixture(false);
    f.state
        .livegraph
        .write()
        .as_mut()
        .expect("livegraph resident")
        .mark_stale("p");
    let l = built_ledger(&f);

    // The regime classifier maps the partition state deterministically.
    assert_eq!(
        classify_state(true, true, false, true, PinState::NoPin),
        StateClass::WOne {
            reason: WOneReason::Stale,
            refresh_blocked_producer_absent: false
        }
    );

    let c = l.classification.as_ref().expect("measured path");
    assert!(
        c.eligible.is_empty(),
        "the stale partition is NOT W-BOTH-eligible"
    );
    assert!(
        c.rollups.is_empty(),
        "no classification rows for a stale partition"
    );
    assert_eq!(
        c.s_kind_totals.calls, 0,
        "its S edges are outside the classification scope"
    );
    assert_eq!(c.both, 0);
    assert_eq!(
        c.unmeasured_edges, 1,
        "the P instance is single-witness-measured (the second witness is not eligible here)"
    );
    assert_eq!(
        c.agreement_pct(),
        None,
        "nothing dual-measured -> unknown, never 0%"
    );
    // Serving byte-identical pipeline: RED → fallback; no fingerprint captured.
    assert!(!l.derived_green());
    assert_eq!(callgraph_cert_eligibility(&f.state, &f.snapshot_uid), None);
}

#[test]
fn regime_matrix_is_exclusive_and_exhaustive_over_every_representable_cell() {
    // Drive EVERY cell of the §4.2 state matrix (covered × resident × freshness × producer × pin)
    // through the classifier: each representable cell lands in EXACTLY one classification;
    // regimes are exclusive AND exhaustive; the two transient states never classify as regimes
    // and never as W-ONE reasons (structural: they are distinct `StateClass` variants and
    // `WOneReason` has exactly three members); the ¬covered ∧ resident row is unrepresentable BY
    // DERIVATION (resident S data IS coverage evidence — the classifier derives coverage).
    for covered in [false, true] {
        for resident in [false, true] {
            for fresh in [false, true] {
                for producer in [false, true] {
                    for pin in [PinState::Match, PinState::Moved, PinState::NoPin] {
                        let got = classify_state(covered, resident, fresh, producer, pin);
                        let expect = if !covered && !resident {
                            // W-NONE: no producer exists for the language; pipeline serves (R-0).
                            StateClass::WNone
                        } else if !resident {
                            // covered ∧ ¬resident: the producer axis splits the reason.
                            StateClass::WOne {
                                reason: if producer {
                                    WOneReason::NotResident
                                } else {
                                    WOneReason::ProducerUnavailable
                                },
                                refresh_blocked_producer_absent: false,
                            }
                        } else if !fresh {
                            // resident ∧ ¬Fresh: ONE reason (stale); producer absence is the
                            // measured warm-cache COMPOUND — a named blocker, never a 4th state.
                            StateClass::WOne {
                                reason: WOneReason::Stale,
                                refresh_blocked_producer_absent: !producer,
                            }
                        } else {
                            // covered ∧ resident ∧ Fresh: W-BOTH ELIGIBLE regardless of producer
                            // (the producer-out-of-predicate cell: Fresh resident data
                            // corroborates; producers gate the NEXT refresh). The pin decides
                            // the request's ACTIVATION, never the regime.
                            match pin {
                                PinState::Match => StateClass::WBothActivated,
                                PinState::Moved => StateClass::WBothTransientPinMoved,
                                PinState::NoPin => StateClass::WBothTransientCaptureFailed,
                            }
                        };
                        assert_eq!(
                            got, expect,
                            "cell (covered={covered}, resident={resident}, fresh={fresh}, \
                             producer={producer}, pin={pin:?})"
                        );
                        // Invariant: the stale∧producer-absent compound's blocker exists ONLY
                        // beside the Stale reason.
                        if let StateClass::WOne {
                            reason,
                            refresh_blocked_producer_absent: true,
                        } = got
                        {
                            assert_eq!(reason, WOneReason::Stale);
                        }
                    }
                }
            }
        }
    }
}

// ── CAPTURE-CONTRACT byte-parity through M-R1 (§4.2/§5.1) ────────────────────────────────────

#[test]
fn divergent_fixture_captures_no_fingerprint_at_m_r1() {
    // The named M-R1 test: the ledger EXISTS on a divergent graph, but the capture contract stays
    // GREEN-gated BYTE-EXACT — a divergent fixture captures NO fingerprint (the M-R2 flip to
    // ledger-validity-gated capture is NOT this slice). Its M-R2 twin will prove the opposite.
    let f = test_fixture::build_fixture(true); // drop_calls -> divergence -> RED
    let l = built_ledger(&f);
    assert!(!l.derived_green(), "divergent -> RED");
    assert!(l.compare.is_some(), "the ledger measured the divergence");
    assert_eq!(
        callgraph_cert_eligibility(&f.state, &f.snapshot_uid),
        None,
        "RED -> NO fingerprint captured (the GREEN gate preserved through M-R1)"
    );

    // The faithful mirror still captures — the GREEN half of byte-parity.
    let g = test_fixture::build_fixture(false);
    let lg_ledger = built_ledger(&g);
    assert!(lg_ledger.derived_green());
    assert!(
        callgraph_cert_eligibility(&g.state, &g.snapshot_uid).is_some(),
        "GREEN -> the fingerprint is captured exactly as today"
    );
}

#[test]
fn derived_verdict_equals_stored_cert_verdict_and_shares_the_fingerprint_key() {
    // The GREEN/RED verdict is DERIVED from the ledger — the stored cert and the stored ledger
    // must agree on verdict AND be keyed by the SAME fingerprint (one lifecycle).
    for drop_calls in [false, true] {
        let f = test_fixture::build_fixture(drop_calls);
        let green = callgraph_is_green(&f.state, &f.snapshot_uid);
        let cert = f.state.callgraph_cert.read().clone().expect("cert stored");
        let ledger = f
            .state
            .witness_ledger
            .read()
            .clone()
            .expect("ledger stored");
        assert_eq!(green, ledger.derived_green());
        assert_eq!(cert.verdict == "GREEN", ledger.derived_green());
        assert_eq!(
            cert.fingerprint, ledger.fingerprint,
            "one fingerprint key, one lifecycle"
        );
        assert_eq!(ledger.snapshot_uid, f.snapshot_uid);
    }
}

// ── The R-RAT-4 COLLISION GUARD (§3.5 guard 2) ───────────────────────────────────────────────

#[test]
fn collision_guard_fallback_key_byte_equal_to_pipeline_key_never_corroborates() {
    // A hand-constructed `PartitionIr` holding a `ScipSynthesizedFallback` node whose key
    // byte-equals a pipeline key: the S edge is WITHHELD (never `both`), `identity_collision` is
    // counted with the colliding key retained per partition, and the P side is byte-unchanged
    // (its instance stays in the pipeline accounting; serving is untouched at M-R1). Without the
    // guard, the byte-equal keys would silently merge into a false `both` — retro-undetectable.
    let f = test_fixture::build_collision_fixture();
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    assert_eq!(
        c.both, 0,
        "NEVER `both` — merging is identity-source-conditional"
    );
    assert_eq!(
        c.semantic.total(),
        0,
        "the withheld S edge mints no semantic instance"
    );
    assert_eq!(
        c.identity_collision, 1,
        "the withheld S `Calls` instance is counted"
    );
    assert_eq!(
        c.fallback_key_count, 1,
        "the fallback-key population is real here"
    );
    let keys = c
        .colliding_keys
        .get("p")
        .expect("colliding keys per partition");
    assert!(
        keys.contains(&test_fixture::callee_key()),
        "the colliding KEY is retained (doctor)"
    );
    assert!(c
        .withheld_pairs
        .contains(&(test_fixture::caller_key(), test_fixture::callee_key())));
    // The P instance is intact and NOT corroborated — it classifies per the dual-measured rule
    // ("as if S held no matching pair"); with the fallback endpoint degrading both LG
    // projections, that is the single-witness-measured class here.
    assert_eq!(
        c.pipeline_calls, 1,
        "P rows byte-unchanged — the instance is never lost"
    );
    assert_eq!(
        c.pipeline_calls,
        c.both + c.syntactic.total() + c.unmeasured_edges
    );
    // Physical kind totals still count the withheld edge (a different, labeled population).
    assert_eq!(c.s_kind_totals.calls, 1);
    let rec = c
        .pairs
        .get(&(test_fixture::caller_key(), test_fixture::callee_key()))
        .expect("pair record");
    assert_eq!(
        rec.s_calls, 0,
        "the classification multiset excludes the withheld edge"
    );
}

// ── The §3.5 guard-3 identity_suspect detector ───────────────────────────────────────────────

#[test]
fn identity_suspect_fires_on_same_caller_same_name_different_callee_key() {
    // P: callerFn -> src/b.ts#target (syntactic — S holds no call to that key). S: callerFn -> a
    // SAME-NAMED symbol under a DIFFERENT key (semantic/new_pair). The (caller, callee-NAME)
    // signature under different keys is the wrong/missed-adoption symptom -> suspect counted.
    let f = test_fixture::build_suspect_fixture();
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");
    assert_eq!(
        c.syntactic.uncorroborated, 1,
        "P's pair: S measured, holds no such call"
    );
    assert_eq!(c.semantic.new_pair, 1, "S's pair: compiler-only call");
    assert_eq!(
        c.identity_suspect, 1,
        "the symptom signature is detected, never merged"
    );
}

// ── RECON-M-R4 (§5.5): the contested detail + the case-1 semantic index ──────────────────────

#[test]
fn contested_detail_records_the_suspect_pair_consistent_with_the_count() {
    // §5.5 case 2: the identity_suspect symptom, RECORDED as detail. Computed in ONE pass over the
    // same index. For a SINGLE-candidate suspect (this fixture) the contested row fires; the
    // ≥2-candidate case is refused (`contested_refuses_ambiguous_candidates_but_still_suspects`).
    let f = test_fixture::build_suspect_fixture();
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    assert_eq!(c.identity_suspect, 1);
    assert_eq!(c.contested.len(), 1, "one (A, B) match recorded");
    let cr = &c.contested[0];
    assert_eq!(cr.caller, test_fixture::caller_key());
    assert_eq!(
        cr.name, "target",
        "the shared callee NAME (the exact-name join key)"
    );
    assert_eq!(
        cr.syntactic_key,
        format!(
            "{}:{}#target:SYMBOL:FUNCTION",
            test_fixture::REPO,
            test_fixture::CALLEE_PATH
        ),
        "syntax resolved to src/b.ts#target"
    );
    assert_eq!(
        cr.semantic_key,
        format!(
            "{}:{}#target:SYMBOL:FUNCTION",
            test_fixture::REPO,
            test_fixture::LIB_PATH
        ),
        "the compiler resolved a same-named call to lib/c.ts#target"
    );
    // The semantic target is a PROJECT symbol by construction (external bindings are dropped at
    // ingest and never reach a `semantic` edge) — the §5.5 honest scope.
    assert_ne!(
        cr.syntactic_key, cr.semantic_key,
        "distinct targets — a genuine contest"
    );
}

#[test]
fn semantic_call_targets_index_new_pair_calls_by_caller_and_name() {
    // §5.5 case 1: a SCIP-only `callerFn -> cn` call is indexed by (callerFn, "cn") for the
    // Layer-2 unresolved-site join. Exactly one target → the join can land a "likely" hint.
    let f = test_fixture::build_layer2_fixture(false);
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    let targets = c
        .semantic_call_targets
        .get(&(test_fixture::caller_key(), "cn".to_string()))
        .expect("cn indexed by (caller, name)");
    assert_eq!(targets.len(), 1, "one same-named compiler target");
    assert!(targets.contains(&test_fixture::cn_key()));
    // FULLY corroborated (s == p) — excluded: every compiler call instance on the
    // `callerFn -> calleeFn` pair (p = 1, s = 1) is P-confirmed, so NO compiler-only excess
    // exists to attribute (review-2: the exclusion rule is `s_calls == p`, NOT `p > 0` — a
    // `multiplicity` pair with s > p ≥ 1 DOES candidate, proven below).
    assert!(!c
        .semantic_call_targets
        .contains_key(&(test_fixture::caller_key(), "calleeFn".to_string())));
}

#[test]
fn semantic_multiplicity_pair_enters_the_layer2_candidate_index() {
    // review-2 #1: the S-EXCESS pair (p = 1, s = 2 → `semantic`/`multiplicity`) IS a Layer-2
    // candidate — the excess instance is a call the compiler witnessed that P did not, exactly
    // the unresolved-site class. The mechanical rule is `s_calls > p`, sub-class-blind.
    let f = test_fixture::build_multiplicity_fixture(1, 2);
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");
    assert_eq!(c.semantic.multiplicity, 1, "the S-excess instance");
    assert_eq!(
        c.semantic.new_pair, 0,
        "no new_pair — the candidate is multiplicity-class"
    );

    let targets = c
        .semantic_call_targets
        .get(&(test_fixture::caller_key(), "calleeFn".to_string()))
        .expect("the multiplicity pair candidates under (caller, name)");
    assert_eq!(targets.len(), 1, "one same-named compiler target");
    assert!(targets.contains(&test_fixture::callee_key()));

    // The mechanical boundary, other side: P-excess (p = 2, s = 1 → `syntactic`/`multiplicity`)
    // has NO compiler-only excess (`s_calls < p`) → never a candidate.
    let g = test_fixture::build_multiplicity_fixture(2, 1);
    let lg = built_ledger(&g);
    let cg = lg.classification.as_ref().expect("measured path");
    assert!(
        cg.semantic_call_targets.is_empty(),
        "P-excess mints no Layer-2 candidate: {:?}",
        cg.semantic_call_targets
    );
}

#[test]
fn contested_join_sees_a_semantic_multiplicity_competitor() {
    // review-2 #2: the compiler's competitor is `semantic`/`multiplicity` (lib/c.ts#target,
    // p = 1, s = 2), NOT a new_pair — the reversed join must still surface the disagreement on
    // the syntactic pair (syntax → src/b.ts#target vs compiler → lib/c.ts#target).
    let f = test_fixture::build_contested_multiplicity_fixture();
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    assert_eq!(
        c.semantic.multiplicity, 1,
        "the competitor is the S-excess instance"
    );
    assert_eq!(
        c.semantic.new_pair, 0,
        "no new_pair competitor exists in this fixture"
    );
    assert_eq!(c.both, 1, "P's own lib/c.ts call stays corroborated");
    assert_eq!(
        c.identity_suspect, 1,
        "the symptom detector (unchanged, full sem_index)"
    );
    assert_eq!(
        c.contested.len(),
        1,
        "the multiplicity competitor participates"
    );
    let cr = &c.contested[0];
    assert_eq!(cr.caller, test_fixture::caller_key());
    assert_eq!(cr.name, "target");
    assert_eq!(
        cr.syntactic_key,
        format!(
            "{}:{}#target:SYMBOL:FUNCTION",
            test_fixture::REPO,
            test_fixture::CALLEE_PATH
        )
    );
    assert_eq!(
        cr.semantic_key,
        format!(
            "{}:{}#target:SYMBOL:FUNCTION",
            test_fixture::REPO,
            test_fixture::LIB_PATH
        ),
        "the compiler's competing binding is the multiplicity pair's target"
    );
}

#[test]
fn refusal_spans_new_pair_and_multiplicity_candidates() {
    // review-2 #2: TWO same-named candidates, one from EACH sub-class — lib/c.ts#target
    // (`multiplicity`, p = 1, s = 2) + lib/d.ts#target (`new_pair`, p = 0, s = 1) → the
    // ambiguity guard refuses the contested join across the sub-class span (never a pick),
    // while `identity_suspect` still counts the syntactic pair.
    let f = test_fixture::build_layer2_cross_subclass_ambiguous_fixture();
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    assert_eq!(
        c.semantic.multiplicity, 1,
        "candidate 1: the S-excess sub-class"
    );
    assert_eq!(
        c.semantic.new_pair, 1,
        "candidate 2: the new_pair sub-class"
    );
    assert_eq!(
        c.semantic_call_targets
            .get(&(test_fixture::caller_key(), "target".to_string()))
            .map(|s| s.len()),
        Some(2),
        "the index holds BOTH sub-classes' candidates"
    );
    assert!(
        c.contested.is_empty(),
        "≥ 2 candidates spanning sub-classes → the contested join REFUSES: {:?}",
        c.contested
    );
    assert_eq!(c.identity_suspect, 1, "the suspicion count still fires");
}

#[test]
fn semantic_call_targets_records_two_same_named_candidates_for_ambiguity() {
    // The ambiguity substrate: two same-named `cn` targets in one caller → the read-side join
    // must REFUSE (proven at the projection layer). Here: the index holds BOTH keys.
    let f = test_fixture::build_layer2_fixture(true);
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");
    let targets = c
        .semantic_call_targets
        .get(&(test_fixture::caller_key(), "cn".to_string()))
        .expect("cn indexed");
    assert_eq!(targets.len(), 2, "two same-named candidates → ambiguous");
    assert!(targets.contains(&test_fixture::cn_key()));
    assert!(targets.contains(&test_fixture::cn2_key()));
}

#[test]
fn contested_refuses_ambiguous_candidates_but_still_suspects() {
    // review-1 #1: the REVERSED join carries the SAME ambiguity guard as case 1. One syntactic
    // `target` (P → src/b.ts#target) + TWO same-named compiler targets (lib/c.ts, lib/d.ts) →
    // the compiler resolution is itself ambiguous → NO contested row selected or emitted. The
    // wrong/missed-adoption SYMPTOM (`identity_suspect`) still fires — a distinct signal, unchanged.
    let f = test_fixture::build_contested_ambiguous_fixture();
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    assert_eq!(
        c.semantic_call_targets
            .get(&(test_fixture::caller_key(), "target".to_string()))
            .map(|s| s.len()),
        Some(2),
        "the index holds both same-named compiler candidates"
    );
    assert!(
        c.contested.is_empty(),
        "≥ 2 same-named candidates → the contested join REFUSES (never a pick): {:?}",
        c.contested
    );
    assert_eq!(
        c.identity_suspect, 1,
        "the suspicion count still fires on the one syntactic pair (value unchanged by the guard)"
    );
}

// ── The committed fixture: the spike's 7/0/2/9 + the per-kind RECORD (§3.4/§6.1) ─────────────

/// The spike baseline on the COMMITTED real fixture (`repo-graph-scip-ingest`'s
/// `tests/fixtures/synthetic/index.scip`): S ingested via the REAL `ingest_partition`; P mirrored
/// to the spike's measured pipeline side (the 2 syntax-resolved calls with faithful FILE→OWNS→
/// MODULE enrichment). The canonical classification must reproduce `scip_only 7 / pipeline_only 0
/// / shared 2 / union 9` [spike §5.3], and the per-kind record must confirm the 7 SCIP-only edges
/// are ALL `References`-kind (ctor-via-`new`, field reads, incoming class/property refs — the
/// strict ingest never mints `Calls` for them), so the kind-aligned union call graph is exactly
/// the 2 corroborated calls.
#[test]
fn committed_fixture_reproduces_spike_7_0_2_9_and_records_all_references_kinds() {
    use repo_graph_scip_ingest::{decode_index, ingest_partition};
    use repo_graph_storage::types::{
        CreateSnapshotInput, GraphEdge, GraphNode, Repo, TrackedFile, UpdateSnapshotStatusInput,
    };
    use repo_graph_storage::StorageConnection;

    const REPO: &str = "synthetic";
    let root = format!(
        "{}/../repo-graph-scip-ingest/tests/fixtures/synthetic",
        env!("CARGO_MANIFEST_DIR")
    );
    let scip = std::fs::read(format!("{root}/index.scip")).expect("read committed index.scip");
    let index = decode_index(&scip).expect("decode scip");
    let outcome = ingest_partition(
        &index,
        &root,
        REPO,
        "synthetic",
        "scip-typescript",
        "0.4.0",
        "h",
        "",
    );

    // The S witness's strict-Calls pairs — the spike's 2 shared edges (P mirrors exactly these).
    let call_edges: Vec<(String, String)> = outcome
        .ir
        .edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Calls)
        .map(|e| (e.src.as_str().to_string(), e.dst.as_str().to_string()))
        .collect();
    assert_eq!(
        call_edges.len(),
        2,
        "the committed fixture's strict call graph is the 2 calls"
    );
    let node_name = |key: &str| -> String {
        outcome
            .ir
            .nodes
            .iter()
            .find(|n| n.key.as_str() == key)
            .map(|n| n.name.clone())
            .expect("call endpoint node in IR")
    };
    let key_path = |key: &str| -> String {
        // `synthetic:src/main.ts#report:SYMBOL:FUNCTION` -> `src/main.ts`.
        key.split_once(':')
            .and_then(|(_, rest)| rest.split_once('#'))
            .map(|(path, _)| path.to_string())
            .expect("key path segment")
    };

    // ── The P mirror: repo + snapshot + files + FILE/MODULE nodes + the 3 call-endpoint SYMBOLs
    //    + the 2 CALLS edges, with the FILE→OWNS→MODULE join the real writer produces (the same
    //    shape `test_fixture::build_sqlite_mirror` mirrors — reproduced here for the committed
    //    fixture's own paths).
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("repo.db");
    let mut conn = StorageConnection::open(&db_path).expect("open storage");
    conn.add_repo(&Repo {
        repo_uid: REPO.into(),
        name: REPO.into(),
        root_path: ".".into(),
        default_branch: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        metadata_json: None,
    })
    .expect("add repo");
    let snap = conn
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: REPO.into(),
            kind: "full".into(),
            basis_ref: None,
            basis_commit: None,
            parent_snapshot_uid: None,
            label: None,
            toolchain_json: None,
        })
        .expect("create snapshot");
    let snapshot_uid = snap.snapshot_uid;

    let paths = ["src/main.ts", "src/shapes.ts"];
    let file_uid = |path: &str| format!("fuid::{path}");
    let tracked: Vec<TrackedFile> = paths
        .iter()
        .map(|p| TrackedFile {
            file_uid: file_uid(p),
            repo_uid: REPO.into(),
            path: (*p).into(),
            language: Some("typescript".into()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        })
        .collect();
    conn.upsert_files(&tracked).expect("upsert files");

    let base_node = |uid: &str, key: &str, kind: &str| GraphNode {
        node_uid: uid.into(),
        snapshot_uid: snapshot_uid.clone(),
        repo_uid: REPO.into(),
        stable_key: key.into(),
        kind: kind.into(),
        subtype: None,
        name: key.into(),
        qualified_name: None,
        file_uid: None,
        parent_node_uid: None,
        location: None,
        signature: None,
        visibility: None,
        doc_comment: None,
        metadata_json: None,
    };
    let mut nodes: Vec<GraphNode> = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let mut n = base_node(&format!("nf{i}"), &format!("{REPO}:{p}:FILE"), "FILE");
        n.file_uid = Some(file_uid(p));
        nodes.push(n);
    }
    let mut module = base_node("nm0", &format!("{REPO}:src:MODULE"), "MODULE");
    module.name = "src".into();
    module.qualified_name = Some("src".into());
    nodes.push(module);
    // The 3 distinct call-endpoint symbols (caller `report` + callees `makeCircle`, `describe`).
    let mut endpoint_keys: Vec<String> = Vec::new();
    for (src, dst) in &call_edges {
        for key in [src, dst] {
            if !endpoint_keys.contains(key) {
                endpoint_keys.push(key.clone());
            }
        }
    }
    let mut symbol_uid_of = std::collections::BTreeMap::new();
    for (i, key) in endpoint_keys.iter().enumerate() {
        let uid = format!("ns{i}");
        let mut n = base_node(&uid, key, "SYMBOL");
        n.name = node_name(key);
        n.file_uid = Some(file_uid(&key_path(key)));
        nodes.push(n);
        symbol_uid_of.insert(key.clone(), uid);
    }
    conn.insert_nodes(&nodes).expect("insert nodes");

    let mut edges: Vec<GraphEdge> = Vec::new();
    for (i, _p) in paths.iter().enumerate() {
        edges.push(GraphEdge {
            edge_uid: format!("eo{i}"),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: REPO.into(),
            source_node_uid: "nm0".into(),
            target_node_uid: format!("nf{i}"),
            edge_type: "OWNS".into(),
            resolution: "resolved".into(),
            extractor: "test".into(),
            location: None,
            metadata_json: None,
        });
    }
    for (i, (src, dst)) in call_edges.iter().enumerate() {
        edges.push(GraphEdge {
            edge_uid: format!("ec{i}"),
            snapshot_uid: snapshot_uid.clone(),
            repo_uid: REPO.into(),
            source_node_uid: symbol_uid_of[src].clone(),
            target_node_uid: symbol_uid_of[dst].clone(),
            edge_type: "CALLS".into(),
            resolution: "resolved".into(),
            extractor: "test".into(),
            location: None,
            metadata_json: None,
        });
    }
    conn.insert_edges(&edges).expect("insert edges");
    conn.update_snapshot_status(&UpdateSnapshotStatusInput {
        snapshot_uid: snapshot_uid.clone(),
        status: "ready".into(),
        completed_at: None,
    })
    .expect("ready snapshot");

    // ── The per-kind RECORD input: the scip-only pairs' IR kinds, collected BEFORE the IR moves.
    let ir_kinds: Vec<(String, String, EdgeType)> = outcome
        .ir
        .edges
        .iter()
        .map(|e| {
            (
                e.src.as_str().to_string(),
                e.dst.as_str().to_string(),
                e.edge_type,
            )
        })
        .collect();

    let state = crate::state::RepoState::open(&db_path, REPO).expect("open repo state");
    let mut lg = repo_graph_livegraph::LiveGraph::new();
    lg.load_partition(
        "synthetic",
        outcome.ir,
        repo_graph_trust_model::LanguageSupport::TypeScriptPrimary,
    );
    *state.livegraph.write() = Some(lg);
    let f = test_fixture::Fixture {
        _dir: dir,
        state,
        snapshot_uid,
    };

    let l = built_ledger(&f);
    let compare = l.compare.as_ref().expect("measured path");
    let c = l.classification.as_ref().expect("measured path");

    // The spike's 7/0/2/9 canonical classification, reproduced by the LIVE ledger walk.
    assert_eq!(
        compare.canonical.livegraph_total, 9,
        "spike: LG 9 canonical instances"
    );
    assert_eq!(compare.canonical.sqlite_total, 2, "spike: pipeline 2");
    assert_eq!(compare.canonical.scip_only, 7, "spike: scip_only 7");
    assert_eq!(
        compare.canonical.pipeline_only_dual_measured + compare.canonical.pipeline_only_unmeasured,
        0,
        "spike: pipeline_only 0"
    );
    assert_eq!(compare.canonical.shared, 2, "spike: shared 2");
    assert_eq!(compare.canonical.union_edges, 9, "spike: union 9");
    // The answerability facts (post-LIVEGRAPH-PARTIAL-FIX-1: clean Partials, ZERO panics).
    assert_eq!(
        compare.unanswerable_projections, 4,
        "the 2 FILE symbols x 2 directions"
    );
    assert_eq!(
        compare.livegraph_panics, 0,
        "the fix holds — no caught panic"
    );
    assert_eq!(
        compare.field_mismatches, 0,
        "shared edges enrich byte-identically"
    );
    // Divergent (scip-only edges exist) -> derived RED, byte-compatible with the stored cert.
    assert!(!l.derived_green());
    assert_eq!(
        f.state
            .callgraph_cert
            .read()
            .as_ref()
            .map(|c| c.verdict.clone()),
        Some("RED".to_string())
    );

    // The kind-aligned classification: the union call graph is EXACTLY the 2 corroborated calls.
    assert_eq!(c.pipeline_calls, 2);
    assert_eq!(
        c.both, 2,
        "both shared pairs carry S strict-`Calls` — kind-aligned corroboration"
    );
    assert_eq!(c.both_identities, 2);
    assert_eq!(c.syntactic.total(), 0);
    assert_eq!(
        c.semantic.total(),
        0,
        "NO scip-only edge is `Calls`-kind — none joins the union"
    );
    assert_eq!(c.unmeasured_edges, 0);
    assert_eq!(c.union_calls, 2);
    assert_eq!(c.agreement_pct(), Some(100.0));
    assert_eq!(
        c.s_kind_totals.calls, 2,
        "the strict call graph is the 2 calls"
    );
    assert_eq!(c.identity_collision, 0);
    assert_eq!(c.identity_suspect, 0);

    // The RECORD (M-R1 gate: "RECORD the per-kind classification of the fixture's 7 SCIP-only
    // edges"): every S edge outside the 2 shared call pairs is `References`-kind — ctor-via-`new`
    // included (`is_call_at` does not cover new-expressions, the §3.4 answered unknown). Expected
    // all-References per the measured ctor evidence; CONFIRMED here. The RAW IR holds 9 such
    // instances; the CANONICAL scip-only count is 7 (asserted above from the live ledger) because
    // 2 instances' projections were BOTH unanswerable — `main.ts:FILE → shapes.ts:FILE` (the
    // file-scope import reference; both endpoints are FILE symbols) and `shapes.ts:FILE →
    // #label:SYMBOL:Term` (a REAL `ScipSynthesizedFallback`-keyed ambient — the fixture's own
    // guard-population member) — an unmeasured side never mints a phantom canonical edge (§3.6).
    let shared_pairs: std::collections::BTreeSet<(String, String)> =
        call_edges.iter().cloned().collect();
    let mut scip_only_kinds: std::collections::BTreeMap<(String, String), Vec<EdgeType>> =
        std::collections::BTreeMap::new();
    for (src, dst, kind) in &ir_kinds {
        if *kind == EdgeType::Imports {
            continue; // Imports enter neither the call graph nor the reference tier here.
        }
        if !shared_pairs.contains(&(src.clone(), dst.clone())) {
            scip_only_kinds
                .entry((src.clone(), dst.clone()))
                .or_default()
                .push(*kind);
        }
    }
    for (pair, kinds) in &scip_only_kinds {
        assert!(
            kinds.iter().all(|k| *k == EdgeType::References),
            "scip-only pair {pair:?} must be References-kind only (got {kinds:?})"
        );
    }
    let raw_instances: usize = scip_only_kinds.values().map(Vec::len).sum();
    assert_eq!(
        raw_instances, 9,
        "9 raw References instances beyond the call pairs = 7 canonical + 2 unmeasured-side"
    );
    assert_eq!(
        scip_only_kinds.len(),
        8,
        "8 distinct non-call pairs (6 canonical + 2 unmeasured)"
    );
    // The committed fixture's REAL fallback population: 4 distinct `ScipSynthesizedFallback` keys
    // (`#Shape:SYMBOL:Type`, `#label:SYMBOL:Term`, `#<constructor>:SYMBOL:Method`,
    // `#size:SYMBOL:Method` — one of which, `label`, appears in the edge set) — present, counted,
    // and NONE colliding (no pipeline key byte-equals them). The guard predicate runs over a real
    // nonzero fallback population even at fixture scale.
    assert_eq!(
        c.fallback_key_count, 4,
        "the fixture's real fallback-key population"
    );
}

// ── RECON-M-R2: the ADDED pipeline-only fixture (classification layer) ───────────────────────

#[test]
fn pipeline_only_fixture_classifies_boundary_uncorroborated_and_unmeasured() {
    // The M-R2 gate's ADDED fixture — a P row absent from S (the committed fixture cannot
    // produce the shape; the amodx artifacts inform it): one BOUNDARY pair (endpoints in two
    // compiler runs, partition sets disjoint), two UNCORROBORATED pairs (same partition; and an
    // endpoint absent from S entirely), and one UNMEASURED pair (both endpoints outside S — the
    // coverage-not-divergence rule, §3.6).
    let f = test_fixture::build_pipeline_only_fixture();
    let l = built_ledger(&f);
    let c = l.classification.as_ref().expect("measured path");

    assert_eq!(c.pipeline_calls, 4);
    assert_eq!(c.both, 0, "S corroborates nothing (no S call edges)");
    assert_eq!(c.semantic.total(), 0);
    assert_eq!(c.syntactic.boundary, 1, "callerFn(p1) -> calleeFn(p2)");
    assert_eq!(c.syntactic.file_scope, 0);
    assert_eq!(
        c.syntactic.uncorroborated, 2,
        "same-partition (otherFn) + endpoint-absent-from-S (rustFn)"
    );
    assert_eq!(c.syntactic.multiplicity, 0);
    assert_eq!(c.syntactic.identities, 3);
    assert_eq!(
        c.unmeasured_edges, 1,
        "rustCaller -> rustFn: coverage, never divergence (§3.6)"
    );
    assert_eq!(c.unmeasured_identities, 1);
    assert_eq!(c.dual_measured, 3);
    assert_eq!(
        c.union_calls, 4,
        "closure: both 0 + syntactic 3 + semantic 0 + unmeasured 1"
    );
    assert_eq!(
        c.agreement_pct(),
        Some(0.0),
        "0/3 dual-measured corroborate"
    );
    // Two eligible TS partitions (the boundary sub-class's substrate).
    assert_eq!(c.eligible.len(), 2);
    assert!(c.eligible.contains_key("p1") && c.eligible.contains_key("p2"));
    // Pair records (the serving substrate) carry the sub-classes.
    use super::ledger::PairSubclass;
    let rec = |a: String, b: String| c.pairs.get(&(a, b)).expect("pair record").clone();
    assert_eq!(
        rec(test_fixture::caller_key(), test_fixture::callee_key()).syntactic_subclass,
        Some(PairSubclass::Boundary)
    );
    assert_eq!(
        rec(test_fixture::caller_key(), test_fixture::other_key()).syntactic_subclass,
        Some(PairSubclass::Uncorroborated)
    );
    assert_eq!(
        rec(test_fixture::caller_key(), test_fixture::rust_fn_key()).syntactic_subclass,
        Some(PairSubclass::Uncorroborated)
    );
    assert!(
        !rec(test_fixture::rust_caller_key(), test_fixture::rust_fn_key()).dual_measured,
        "the uncovered pair is unmeasured — no witness class exists for it"
    );
}
