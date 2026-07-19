//! RECON-M-R3a gate tests for the shared witness projection (recon-design-1 §6.1 M-R3a gate
//! column): R-0 data-driven absence; the three W-ONE reason-specific posture lines + the
//! stale∧producer-absent compound (distinct lines — review-4's defect pinned); ledger-absent /
//! superseded / build-failed rendering (unknown, never a stale number); §5.3.0 accounting +
//! coverage labels on every measured block; deterministic ordering; the R-RAT-4
//! `identity_collision` rendering (closing the M-R1 `bad69da` amendment); g2u reduction; g3u
//! new-pair file pairs; the explain union-degree attach.
//!
//! Fixtures: the M-R1/M-R2 `callgraph_cert::test_fixture` builders (faithful mirror,
//! multiplicity, collision) — the ledger is warmed through the SAME production
//! `callgraph_is_green` store path the daemon uses.

use serde_json::Value;

use super::*;
use crate::callgraph_cert::ledger::LedgerBuildFailure;
use crate::callgraph_cert::{callgraph_is_green, test_fixture};

/// Compute with the producer probe forced (tests never depend on the operator's PATH/env).
fn project(f: &test_fixture::Fixture, producer: bool) -> Option<WitnessProjection> {
    WitnessProjection::compute_with_producer(&f.state, &f.snapshot_uid, || producer)
}

fn warm_ledger(f: &test_fixture::Fixture) {
    let _ = callgraph_is_green(&f.state, &f.snapshot_uid);
    assert!(
        f.state.witness_ledger.read().is_some(),
        "the warm path stores a ledger"
    );
}

fn regime_rows(block: &Value) -> Vec<Value> {
    block["regimes"].as_array().cloned().unwrap_or_default()
}

// ── R-0: data-driven absence ────────────────────────────────────────────────────────────────

#[test]
fn r0_no_witness_evidence_projects_nothing() {
    let f = test_fixture::build_fixture(false);
    *f.state.livegraph.write() = None; // no slots, no ledger, no failure — nothing to say
    assert!(
        project(&f, true).is_none(),
        "zero witness evidence → None → every surface renders exactly today's bytes"
    );
}

#[test]
fn key_file_path_parses_canonical_shape_and_refuses_malformed() {
    assert_eq!(
        key_file_path("repo_u:src/a.ts#foo:SYMBOL:FUNCTION"),
        Some("src/a.ts")
    );
    // No path segment / missing separators → no claim.
    assert_eq!(key_file_path("repo_u:#foo"), None);
    assert_eq!(key_file_path("no-separators"), None);
    assert_eq!(key_file_path("repo_u:src/a.ts-no-hash"), None);
}

// ── The three W-ONE reasons: three DISTINCT posture lines + next actions (§4.2 ladder) ──────

#[test]
fn w_one_stale_renders_out_of_date_with_refresh_action() {
    let f = test_fixture::build_fixture(false);
    f.state.livegraph.write().as_mut().unwrap().mark_stale("p");
    let p = project(&f, true).expect("slots exist");
    let rows = regime_rows(&p.trust_block());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["regime"], "W-ONE");
    assert_eq!(rows[0]["reason"], "stale");
    let posture = rows[0]["posture"].as_str().unwrap();
    assert!(posture.contains("out of date"), "{posture}");
    assert!(
        !posture.contains("available but not loaded"),
        "stale must NEVER read as not-loaded (review-4's pinned defect): {posture}"
    );
    let next = rows[0]["next_action"].as_str().unwrap();
    assert!(next.contains("refresh `p`"), "{next}");
    assert!(rows[0].get("refresh_blocked_producer_absent").is_none());
}

#[test]
fn w_one_stale_producer_absent_compound_names_the_blocker_one_reason() {
    let f = test_fixture::build_fixture(false);
    f.state.livegraph.write().as_mut().unwrap().mark_stale("p");
    let p = project(&f, false).expect("slots exist");
    let rows = regime_rows(&p.trust_block());
    // ONE reason (`stale`), the blocker on the next action — never a fourth state.
    assert_eq!(rows[0]["reason"], "stale");
    assert_eq!(rows[0]["refresh_blocked_producer_absent"], true);
    let next = rows[0]["next_action"].as_str().unwrap();
    assert!(
        next.contains("requires `scip-typescript`, which is not provisioned"),
        "{next}"
    );
}

#[test]
fn w_one_not_resident_renders_available_but_not_loaded() {
    let f = test_fixture::build_fixture(false);
    f.state
        .livegraph
        .write()
        .as_mut()
        .unwrap()
        .unload_partition("p");
    let p = project(&f, true).expect("summary-retained slot is evidence");
    let rows = regime_rows(&p.trust_block());
    assert_eq!(rows[0]["reason"], "not_resident");
    let posture = rows[0]["posture"].as_str().unwrap();
    assert!(posture.contains("available but not loaded"), "{posture}");
    assert!(
        rows[0]["next_action"]
            .as_str()
            .unwrap()
            .contains("load `p`"),
        "{:?}",
        rows[0]["next_action"]
    );
}

#[test]
fn w_one_producer_unavailable_renders_provision_action() {
    let f = test_fixture::build_fixture(false);
    f.state
        .livegraph
        .write()
        .as_mut()
        .unwrap()
        .unload_partition("p");
    let p = project(&f, false).expect("slot evidence");
    let rows = regime_rows(&p.trust_block());
    assert_eq!(rows[0]["reason"], "producer_unavailable");
    let posture = rows[0]["posture"].as_str().unwrap();
    assert!(
        posture.contains("no compiler analysis is loaded here"),
        "{posture}"
    );
    assert!(
        posture.contains("not provisioned"),
        "the reason names the producer state: {posture}"
    );
    assert!(rows[0]["next_action"]
        .as_str()
        .unwrap()
        .contains("provision `scip-typescript`"));
}

#[test]
fn three_w_one_posture_lines_are_pairwise_distinct() {
    // stale
    let f1 = test_fixture::build_fixture(false);
    f1.state.livegraph.write().as_mut().unwrap().mark_stale("p");
    let stale = regime_rows(&project(&f1, true).unwrap().trust_block())[0]["posture"]
        .as_str()
        .unwrap()
        .to_string();
    // not_resident
    let f2 = test_fixture::build_fixture(false);
    f2.state
        .livegraph
        .write()
        .as_mut()
        .unwrap()
        .unload_partition("p");
    let not_resident = regime_rows(&project(&f2, true).unwrap().trust_block())[0]["posture"]
        .as_str()
        .unwrap()
        .to_string();
    // producer_unavailable
    let producer_unavailable = regime_rows(&project(&f2, false).unwrap().trust_block())[0]
        ["posture"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(stale, not_resident, "stale ≠ 'available but not loaded'");
    assert_ne!(stale, producer_unavailable);
    assert_ne!(not_resident, producer_unavailable);
}

#[test]
fn w_both_fresh_partition_renders_corroboration_active() {
    let f = test_fixture::build_fixture(false);
    let p = project(&f, true).expect("slot evidence");
    let rows = regime_rows(&p.trust_block());
    assert_eq!(rows[0]["regime"], "W-BOTH");
    assert!(rows[0].get("reason").is_none(), "W-BOTH carries no reason");
}

// ── Ledger-absent / superseded / build-failed: unknown, NEVER a stale number ────────────────

#[test]
fn trust_block_renders_unknown_when_ledger_never_built() {
    let f = test_fixture::build_fixture(false);
    let p = project(&f, true).unwrap();
    let block = p.trust_block();
    assert!(block["measured"].is_null(), "no number without a ledger");
    assert!(block["unknown_reason"]
        .as_str()
        .unwrap()
        .contains("not yet measured"));
    assert!(p.g1u_block().is_none(), "no g1u line without a ledger");
    let doctor = p.doctor_block();
    assert_eq!(doctor["ledger"]["present"], false);
    assert!(doctor["ledger"]["last_build_outcome"]
        .as_str()
        .unwrap()
        .contains("not yet attempted"));
}

#[test]
fn superseded_ledger_renders_unknown_never_the_old_numbers() {
    let f = test_fixture::build_fixture(false);
    warm_ledger(&f);
    // Witness movement: reload the partition (epoch bump) — the stored ledger's fingerprint
    // no longer matches the resident state.
    f.state.livegraph.write().as_mut().unwrap().load_partition(
        "p",
        test_fixture::build_ir(),
        repo_graph_trust_model::LanguageSupport::TypeScriptPrimary,
    );
    let p = project(&f, true).unwrap();
    let block = p.trust_block();
    assert!(
        block["measured"].is_null(),
        "a superseded ledger must never serve its numbers"
    );
    assert!(block["unknown_reason"]
        .as_str()
        .unwrap()
        .contains("superseded"));
    let doctor = p.doctor_block();
    assert_eq!(doctor["ledger"]["present"], true);
    assert_eq!(doctor["ledger"]["current"], false);
    assert!(p.g1u_block().is_none());
    assert!(p.unref_reduction(["k"]).is_none());
    assert!(p.g3u_new_call_file_pairs().is_none());
}

// ── Review-1 item 1: the witness-currency race under W-B refresh concurrency ────────────────

/// Review-1 (iteration 2): W-B admits reads during refresh, and the refresh swap takes
/// `livegraph.write()` (`livegraph_refresh.rs`). Iteration 1 released the read guard between
/// the fingerprint capture and the ledger selection; a swap in that window could pair the
/// pre-swap fingerprint with the retained pre-swap ledger → false `Measured` (a stale number
/// rendered as current). The fix holds ONE read guard across both reads. This test forces the
/// question at the exact seam: the write-lock acquisition a swap needs must FAIL there.
#[test]
fn livegraph_swap_cannot_interleave_fingerprint_capture_and_ledger_selection() {
    let f = test_fixture::build_fixture(false);
    warm_ledger(&f);
    let mut seam_probed = false;
    let p = WitnessProjection::compute_with_seam_probe(
        &f.state,
        &f.snapshot_uid,
        || true,
        || {
            assert!(
                f.state.livegraph.try_write().is_none(),
                "a LiveGraph swap must be excluded between the fingerprint capture and the \
                 ledger selection — review-1's race window"
            );
            seam_probed = true;
        },
    );
    assert!(seam_probed, "the seam probe ran");
    // With the guard held the pairing is consistent: the warmed ledger IS current here.
    assert!(!p.unwrap().trust_block()["measured"].is_null());
}

/// The seam half that CAN still move: `witness_ledger` is its own lock, so a concurrent
/// builder's store may land between the two reads (build under `livegraph.read()` completes,
/// store happens guard-free — `build_and_store_callgraph_cert`). Whatever the selection reads,
/// it must classify against the fingerprint PINNED by the held guard — a mismatched ledger
/// renders Superseded (unknown), never Measured with the moved figures.
#[test]
fn ledger_movement_at_the_seam_renders_superseded_never_measured() {
    let f = test_fixture::build_fixture(false);
    warm_ledger(&f);
    let p = WitnessProjection::compute_with_seam_probe(
        &f.state,
        &f.snapshot_uid,
        || true,
        || {
            // A concurrent store lands mid-projection, keyed at ANOTHER fingerprint (its own
            // observation of a different graph state) — deterministic movement at the seam.
            let mut stored = f.state.witness_ledger.write();
            stored.as_mut().expect("warmed ledger present").fingerprint =
                "fp_moved_at_the_seam".to_string();
        },
    );
    let block = p.expect("slot evidence exists").trust_block();
    assert!(
        block["measured"].is_null(),
        "a ledger that moved mid-projection must never serve figures as current"
    );
    assert!(block["unknown_reason"]
        .as_str()
        .unwrap()
        .contains("superseded"));
}

/// Review-0 defect (a): the REALISTIC old-ledger + current-build-failure state — a failed
/// rebuild RETAINS the old ledger (the store clears the failure only on success), so rendering
/// only "superseded" would mask the failure. Both facts must render, on trust AND doctor.
#[test]
fn superseded_ledger_does_not_mask_the_latest_build_failure() {
    let f = test_fixture::build_fixture(false);
    warm_ledger(&f);
    // Witness movement: the stored ledger's fingerprint is now superseded…
    f.state.livegraph.write().as_mut().unwrap().load_partition(
        "p",
        test_fixture::build_ir(),
        repo_graph_trust_model::LanguageSupport::TypeScriptPrimary,
    );
    // …and the rebuild attempt at the NEW fingerprint failed (the production writer's exact
    // state: failure recorded, old ledger left in place — callgraph_cert/mod.rs).
    *f.state.witness_ledger_build_failure.write() = Some(LedgerBuildFailure {
        fingerprint: "fp_new_failed".into(),
        reason: "sqlite_error_during_ledger_walk".into(),
    });

    let p = project(&f, true).unwrap();
    let block = p.trust_block();
    assert!(block["measured"].is_null(), "never the old numbers");
    let reason = block["unknown_reason"].as_str().unwrap();
    assert!(reason.contains("superseded"), "{reason}");
    assert!(
        reason.contains("failed (sqlite_error_during_ledger_walk)"),
        "the masked-failure defect: the failure must render beside superseded: {reason}"
    );

    let doctor = p.doctor_block();
    assert_eq!(doctor["ledger"]["present"], true);
    assert_eq!(doctor["ledger"]["current"], false);
    assert_eq!(doctor["ledger"]["last_build_outcome"], "failed");
    assert_eq!(
        doctor["ledger"]["failure_reason"],
        "sqlite_error_during_ledger_walk"
    );
    assert_eq!(doctor["ledger"]["failed_fingerprint"], "fp_new_failed");
}

#[test]
fn doctor_reports_the_last_build_failure_with_its_reason() {
    let f = test_fixture::build_fixture(false);
    *f.state.witness_ledger_build_failure.write() = Some(LedgerBuildFailure {
        fingerprint: "fp_failed".into(),
        reason: "sqlite_error_during_ledger_walk".into(),
    });
    let p = project(&f, true).unwrap();
    let doctor = p.doctor_block();
    assert_eq!(doctor["ledger"]["present"], false);
    assert_eq!(doctor["ledger"]["last_build_outcome"], "failed");
    assert_eq!(
        doctor["ledger"]["failure_reason"],
        "sqlite_error_during_ledger_walk"
    );
    assert_eq!(doctor["ledger"]["failed_fingerprint"], "fp_failed");
    let block = p.trust_block();
    assert!(block["measured"].is_null());
    assert!(block["unknown_reason"]
        .as_str()
        .unwrap()
        .contains("failed (sqlite_error_during_ledger_walk)"));
}

// ── Measured blocks: §5.3.0 labels + closure fields + deterministic ordering ────────────────

#[test]
fn measured_blocks_carry_union_accounting_and_coverage_labels() {
    let f = test_fixture::build_fixture(false);
    warm_ledger(&f);
    let p = project(&f, true).unwrap();

    let trust = p.trust_block();
    let m = &trust["measured"];
    assert_eq!(m["accounting"], "union");
    assert_eq!(m["coverage"]["languages"][0], "TypeScript");
    assert_eq!(m["coverage"]["partitions"][0], "p");
    assert!(m["coverage"]["fingerprint"].as_str().unwrap().len() > 4);
    // The faithful mirror: 1 P call, corroborated by the 1 S call.
    assert_eq!(m["pipeline_calls"], 1);
    assert_eq!(m["union_calls"], 1);
    assert_eq!(m["both"]["instances"], 1);
    // Defect (b): unit-labeled object — instances (the ledger's unit) + distinct pairs.
    assert_eq!(m["identity_collision"]["instances"], 0);
    assert_eq!(m["identity_collision"]["identities"], 0);
    // The reference tier + projections are separately-named populations.
    assert!(m.get("references").is_some());
    assert!(m["projections"]["total"].as_u64().unwrap() > 0);

    let g1u = p.g1u_block().expect("measured → g1u present");
    assert_eq!(g1u["accounting"], "union");
    assert_eq!(g1u["pipeline_calls"], 1);
    assert_eq!(g1u["union_calls"], 1);

    let doctor = p.doctor_block();
    assert_eq!(doctor["ledger"]["current"], true);
    assert_eq!(doctor["measured"]["adoption"]["p"]["adopted"], 2);
    assert_eq!(doctor["measured"]["adoption"]["p"]["file_scope"], 3);
}

#[test]
fn agreement_pct_is_null_when_nothing_dual_measured_never_zero() {
    // The multiplicity fixture with p=0, s=0 has no calls at all → dual_measured 0.
    let f = test_fixture::build_multiplicity_fixture(0, 0);
    warm_ledger(&f);
    let p = project(&f, true).unwrap();
    let m = &p.trust_block()["measured"];
    assert!(
        m["agreement_pct"].is_null(),
        "unknown rate renders null, never 0%: {m}"
    );
}

#[test]
fn deterministic_ordering_two_computes_render_identical_json() {
    let f = test_fixture::build_fixture(false);
    warm_ledger(&f);
    let a = project(&f, true).unwrap();
    let b = project(&f, true).unwrap();
    assert_eq!(a.trust_block().to_string(), b.trust_block().to_string());
    assert_eq!(a.doctor_block().to_string(), b.doctor_block().to_string());
    assert_eq!(
        a.g1u_block().map(|v| v.to_string()),
        b.g1u_block().map(|v| v.to_string())
    );
}

// ── Collision rendering (closes the M-R1 bad69da amendment) ─────────────────────────────────

#[test]
fn collision_fixture_renders_identity_collision_on_trust_and_doctor() {
    let f = test_fixture::build_collision_fixture();
    warm_ledger(&f);
    let p = project(&f, true).unwrap();

    let m = &p.trust_block()["measured"];
    // Defect (b) — unit truth: the withheld S call renders in the ledger's ACTUAL units —
    // instances (what `identity_collision` counts) + distinct withheld pairs.
    assert_eq!(
        m["identity_collision"]["instances"], 1,
        "the withheld S call INSTANCE is counted on trust's block, beside the closure"
    );
    assert_eq!(m["identity_collision"]["identities"], 1);

    let doctor = p.doctor_block();
    let colliding = &doctor["measured"]["colliding_keys"]["p"];
    assert_eq!(colliding[0], test_fixture::callee_key(), "the KEY is named");
    let line = doctor["measured"]["collision_line"].as_str().unwrap();
    // Both populations, each with its own unit: 1 colliding KEY, 1 withheld INSTANCE.
    assert!(line.contains("1 symbol identity collides"), "{line}");
    assert!(
        line.contains("1 compiler-witnessed call instance withheld"),
        "{line}"
    );
    assert!(line.contains("never merged"), "{line}");
}

// ── R-1: covered+uncovered MIXED-repo scoping through the M-R3a surfaces (review-0 defect c) ─

/// Review-0 defect (c): the covered+uncovered mixed-repo fixture driven through the M-R3a READ
/// SURFACES' scoping (the M-R2 mixed tests cover serving, not the projection). Fixture
/// (`build_pipeline_only_fixture`): TWO covered TS partitions resident+Fresh in S; a Rust file
/// whose symbols exist ONLY in the pipeline (uncovered language — no S partition, no producer).
/// The R-1 contract on every projection surface: witness claims scope to the COVERED partitions;
/// the uncovered language yields NO phantom regime row, NO divergence claim (its pipeline-only
/// pair lands `unmeasured` — coverage, never divergence), and NO g2u/g3u leakage.
#[test]
fn mixed_repo_projection_scopes_to_covered_partitions_never_the_uncovered_language() {
    let f = test_fixture::build_pipeline_only_fixture();
    warm_ledger(&f);
    let p = project(&f, true).unwrap();

    let block = p.trust_block();
    // Regime rows: exactly the TWO covered TS partitions (deterministic order) — the uncovered
    // Rust language has no partition-level fact to state, so NO phantom row (unknown unstated).
    let rows = regime_rows(&block);
    assert_eq!(rows.len(), 2, "exactly the two TS partitions: {rows:?}");
    assert_eq!(rows[0]["partition"], "p1");
    assert_eq!(rows[1]["partition"], "p2");
    assert!(rows.iter().all(|r| r["language"] == "TypeScript"));
    assert!(rows.iter().all(|r| r["regime"] == "W-BOTH"));

    // The measured block's coverage basis names ONLY the covered partitions/language.
    let m = &block["measured"];
    assert_eq!(
        m["coverage"]["languages"],
        serde_json::json!(["TypeScript"])
    );
    assert_eq!(m["coverage"]["partitions"], serde_json::json!(["p1", "p2"]));

    // Scoping of the classification itself (the fixture's ratified shape): 4 pipeline calls;
    // the uncovered-pair instance (`rustCaller -> rustFn`) is UNMEASURED — coverage, never
    // divergence — and the endpoint-absent TS-caller pair is dual-measured `syntactic`. No
    // union-only (semantic) instances exist: S holds no call edges.
    assert_eq!(m["pipeline_calls"], 4);
    assert_eq!(m["union_calls"], 4, "union adds nothing here");
    assert_eq!(m["unmeasured_edges"]["instances"], 1, "the rust-only pair");
    assert_eq!(
        m["syntactic_only"]["boundary"].as_u64().unwrap()
            + m["syntactic_only"]["file_scope"].as_u64().unwrap()
            + m["syntactic_only"]["uncorroborated"].as_u64().unwrap()
            + m["syntactic_only"]["multiplicity"].as_u64().unwrap(),
        3,
        "the three TS-caller pairs are dual-measured divergence, never the rust pair: {m}"
    );

    // g2u scoping: the reduction never claims a compiler witness for uncovered-language symbols
    // (rustFn has pipeline callers but NO S-witnessed incoming edge).
    let rust_fn = test_fixture::rust_fn_key();
    assert_eq!(
        p.unref_reduction([rust_fn.as_str()]),
        Some(0),
        "no compiler-witness claim about the uncovered language"
    );
    assert!(p.unref_reduction_block([rust_fn.as_str()]).is_none());

    // g3u scoping: no semantic instances → no phantom sketch pairs.
    assert!(p.g3u_new_call_file_pairs().unwrap().is_empty());

    // Doctor: adoption rows exist for the covered partitions ONLY.
    let doctor = p.doctor_block();
    let adoption = doctor["measured"]["adoption"].as_object().unwrap();
    assert_eq!(
        adoption.keys().collect::<Vec<_>>(),
        vec!["p1", "p2"],
        "adoption is a per-producer, per-partition fact — covered partitions only"
    );
}

// ── g2u: the reduction-only unref overlay ───────────────────────────────────────────────────

#[test]
fn unref_reduction_counts_only_compiler_witnessed_flagged_keys() {
    let f = test_fixture::build_fixture(false);
    warm_ledger(&f);
    let p = project(&f, true).unwrap();
    let callee = test_fixture::callee_key();
    let caller = test_fixture::caller_key();
    // The callee has a compiler-witnessed incoming call; the caller has none.
    assert_eq!(
        p.unref_reduction([callee.as_str(), caller.as_str()]),
        Some(1)
    );
    // Zero reduction → NO block (absence, never a zero line).
    assert!(p.unref_reduction_block([caller.as_str()]).is_none());
    let block = p
        .unref_reduction_block([callee.as_str()])
        .expect("nonzero reduction renders");
    assert_eq!(block["fewer_flagged"], 1);
    assert_eq!(block["accounting"], "union");
    assert_eq!(block["basis"], "compiler-verified references found");
}

// ── g2u-b + g3u: union degree + new-pair file pairs ─────────────────────────────────────────

#[test]
fn union_degree_second_figure_only_where_it_differs() {
    // S-excess (p=1, s=2): the callee's union fan-in is 2 vs pipeline 1.
    let f = test_fixture::build_multiplicity_fixture(1, 2);
    warm_ledger(&f);
    let p = project(&f, true).unwrap();
    let callee = test_fixture::callee_key();
    let caller = test_fixture::caller_key();
    assert_eq!(p.union_fan_in(&callee), Some((1, 2)));
    assert_eq!(p.union_fan_out(&caller), Some((1, 2)));
    // Agreeing degrees → None ("a labeled second figure WHERE IT DIFFERS").
    let faithful = test_fixture::build_fixture(false);
    warm_ledger(&faithful);
    let fp = project(&faithful, true).unwrap();
    assert_eq!(fp.union_fan_in(&callee), None);
}

#[test]
fn g3u_semantic_new_pair_yields_the_cross_file_pair() {
    // p=0, s=1: an S-only (semantic/new_pair) call caller→callee across files.
    let f = test_fixture::build_multiplicity_fixture(0, 1);
    warm_ledger(&f);
    let p = project(&f, true).unwrap();
    let pairs = p.g3u_new_call_file_pairs().expect("measured");
    assert!(
        pairs.contains(&("src/a.ts".to_string(), "src/b.ts".to_string())),
        "the new_pair file pair is derived from the canonical keys: {pairs:?}"
    );
    assert!(p.g3u_label().unwrap()["coverage"]["fingerprint"].is_string());
    // A corroborated pair (p≥1) adds NO sketch pair — new_pair only (§5.3.4).
    let corroborated = test_fixture::build_fixture(false);
    warm_ledger(&corroborated);
    let cp = project(&corroborated, true).unwrap();
    assert!(cp.g3u_new_call_file_pairs().unwrap().is_empty());
}

// ── The explain union-degree attach (g2u-b end-to-end shape) ────────────────────────────────

#[test]
fn explain_attach_adds_union_object_where_it_differs() {
    let f = test_fixture::build_multiplicity_fixture(1, 2);
    warm_ledger(&f);
    let callee = test_fixture::callee_key();
    let mut response = serde_json::json!({
        "value": {
            "focus": { "resolved_kind": "symbol", "resolved_key": callee },
            "signals": [
                { "value": { "code": "EXPLAIN_CALLERS", "evidence": { "count": 1 } } },
                { "value": { "code": "EXPLAIN_CYCLES", "evidence": { "count": 0 } } },
            ],
        },
    });
    WitnessProjection::attach_explain_union_degrees(&f.state, &f.snapshot_uid, &mut response);
    let ev = &response["value"]["signals"][0]["value"]["evidence"];
    assert_eq!(ev["union"]["count"], 2);
    assert_eq!(ev["union"]["pipeline_count"], 1);
    assert_eq!(ev["union"]["accounting"], "union");
    assert!(ev["union"]["coverage"]["fingerprint"].is_string());
    assert_eq!(ev["count"], 1, "the pipeline figure is never touched");
    // The non-callgraph signal is untouched.
    assert!(response["value"]["signals"][1]["value"]["evidence"]
        .get("union")
        .is_none());
}

#[test]
fn explain_attach_is_a_no_op_without_a_ledger_or_on_non_symbol_focus() {
    let f = test_fixture::build_fixture(false); // no ledger warmed
    let mut response = serde_json::json!({
        "value": {
            "focus": { "resolved_kind": "symbol", "resolved_key": test_fixture::callee_key() },
            "signals": [ { "value": { "code": "EXPLAIN_CALLERS", "evidence": { "count": 1 } } } ],
        },
    });
    let before = response.to_string();
    WitnessProjection::attach_explain_union_degrees(&f.state, &f.snapshot_uid, &mut response);
    assert_eq!(response.to_string(), before, "no ledger → byte-identical");

    warm_ledger(&f);
    let mut repo_focus = serde_json::json!({
        "value": {
            "focus": { "resolved_kind": "repo" },
            "signals": [ { "value": { "code": "EXPLAIN_CALLERS", "evidence": { "count": 1 } } } ],
        },
    });
    let before = repo_focus.to_string();
    WitnessProjection::attach_explain_union_degrees(&f.state, &f.snapshot_uid, &mut repo_focus);
    assert_eq!(before, repo_focus.to_string(), "non-symbol focus → no-op");
}

// ── RECON-M-R3b: the reference tier (recon-design-1 §5.2 / §6.1 M-R3b gate) ──────────────────

use serde_json::json;

#[test]
fn reference_tier_incoming_renders_labeled_truncated_and_collision_withheld() {
    let f = test_fixture::build_reference_tier_fixture();
    warm_ledger(&f);
    let block = WitnessProjection::reference_tier_block(
        &f.state,
        &f.snapshot_uid,
        &test_fixture::callee_key(),
        ReferenceDirection::Incoming,
    )
    .expect("W-BOTH with a measured ledger renders the tier");

    // §5.3.0 labeling: accounting + a complete coverage basis.
    assert_eq!(block["accounting"], "union");
    assert_eq!(block["direction"], "incoming");
    assert!(block["coverage"]["fingerprint"].is_string());
    assert_eq!(block["coverage"]["languages"], json!(["TypeScript"]));
    assert_eq!(block["coverage"]["partitions"], json!(["p"]));

    // 31 incoming reference edges (30 ref{i} + the collision referrer) minus the WITHHELD
    // collision (§3.5 guard 2) = 30; the self-reference is excluded. Budget 25 ⇒ NAMED truncation.
    assert_eq!(
        block["total"], 30,
        "collision referrer withheld, self-ref excluded"
    );
    assert_eq!(block["shown"], 25, "the budget bounds the listing");
    assert_eq!(block["truncated"], 5, "named truncation — never silent");

    let items = block["references"].as_array().unwrap();
    assert_eq!(items.len(), 25);
    let keys: Vec<&str> = items
        .iter()
        .filter_map(|i| i["stable_key"].as_str())
        .collect();
    assert!(
        !keys.contains(&test_fixture::caller_key().as_str()),
        "the collision-withheld referrer (callerFn's colliding key) never surfaces: {keys:?}"
    );
    // Every listed item carries a resolved name + file (the reader's orientation anchor).
    assert!(items[0]["name"].as_str().unwrap().starts_with("ref"));
    assert!(items[0]["file"].as_str().unwrap().ends_with("refs.ts"));
}

#[test]
fn reference_tier_outgoing_lists_referenced_symbols() {
    let f = test_fixture::build_reference_tier_fixture();
    warm_ledger(&f);
    let block = WitnessProjection::reference_tier_block(
        &f.state,
        &f.snapshot_uid,
        &test_fixture::callee_key(),
        ReferenceDirection::Outgoing,
    )
    .expect("outgoing references render");
    assert_eq!(block["direction"], "outgoing");
    // calleeFn -> {outA, outB}; the self-reference is excluded. No truncation (2 ≤ 25).
    assert_eq!(block["total"], 2);
    assert_eq!(block["truncated"], 0);
    let names: Vec<&str> = block["references"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["name"].as_str())
        .collect();
    assert!(
        names.contains(&"outA") && names.contains(&"outB"),
        "{names:?}"
    );
}

#[test]
fn reference_tier_absent_without_a_current_ledger_r0() {
    // No ledger warmed → the tier is absent (R-0: zero-SCIP / not-yet-measured renders nothing).
    let f = test_fixture::build_reference_tier_fixture();
    assert!(
        WitnessProjection::reference_tier_block(
            &f.state,
            &f.snapshot_uid,
            &test_fixture::callee_key(),
            ReferenceDirection::Incoming,
        )
        .is_none(),
        "no ledger → tier absent"
    );
    // No LiveGraph at all → absent.
    let f2 = test_fixture::build_fixture(false);
    *f2.state.livegraph.write() = None;
    assert!(WitnessProjection::reference_tier_block(
        &f2.state,
        &f2.snapshot_uid,
        &test_fixture::callee_key(),
        ReferenceDirection::Incoming,
    )
    .is_none());
}

#[test]
fn reference_tier_scopes_to_covered_partitions_r1() {
    // A stale second partition q ALSO references calleeFn, but is not W-BOTH-eligible — its
    // reference must NOT count and the coverage basis lists only the covered partition (R-1).
    let f = test_fixture::build_reference_tier_mixed_fixture();
    warm_ledger(&f);
    let block = WitnessProjection::reference_tier_block(
        &f.state,
        &f.snapshot_uid,
        &test_fixture::callee_key(),
        ReferenceDirection::Incoming,
    )
    .expect("the eligible partition still renders");
    assert_eq!(
        block["total"], 30,
        "the stale partition q's reference is excluded — covered-partition scoping"
    );
    assert_eq!(
        block["coverage"]["partitions"],
        json!(["p"]),
        "coverage lists only the eligible partition"
    );
    let keys: Vec<&str> = block["references"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["stable_key"].as_str())
        .collect();
    assert!(
        !keys.contains(&test_fixture::stale_ref_key().as_str()),
        "the stale partition's referrer never surfaces: {keys:?}"
    );
}

#[test]
fn reference_tier_absent_on_witness_movement() {
    // Never a stale row: a witness movement (the partition goes stale) supersedes the warmed
    // ledger, so the currency peek fails and the tier is absent rather than serving old edges.
    let f = test_fixture::build_reference_tier_fixture();
    warm_ledger(&f);
    f.state.livegraph.write().as_mut().unwrap().mark_stale("p");
    assert!(
        WitnessProjection::reference_tier_block(
            &f.state,
            &f.snapshot_uid,
            &test_fixture::callee_key(),
            ReferenceDirection::Incoming,
        )
        .is_none(),
        "witness movement supersedes the ledger → tier absent (never a stale row)"
    );
}

#[test]
fn attach_explain_reference_tier_on_symbol_focus_only() {
    let f = test_fixture::build_reference_tier_fixture();
    warm_ledger(&f);
    let mut response = json!({
        "value": { "focus": { "resolved_kind": "symbol", "resolved_key": test_fixture::callee_key() } },
    });
    WitnessProjection::attach_explain_reference_tier(&f.state, &f.snapshot_uid, &mut response);
    let block = &response["value"]["references"];
    assert_eq!(block["direction"], "incoming");
    assert_eq!(block["total"], 30);

    // Non-symbol focus → no-op (byte-identical).
    let mut repo_focus = json!({ "value": { "focus": { "resolved_kind": "repo" } } });
    let before = repo_focus.to_string();
    WitnessProjection::attach_explain_reference_tier(&f.state, &f.snapshot_uid, &mut repo_focus);
    assert_eq!(before, repo_focus.to_string(), "non-symbol focus → no-op");
}
