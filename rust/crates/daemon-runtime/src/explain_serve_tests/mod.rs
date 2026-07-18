//! COHERENCE-LEAF-SERVE-IMPL-2: explain's bounded (b)-leaf serve-then-fallback tests (the EXPLAIN consumer
//! of the SAME `OrientServeDecorator` + bounded cert orient IMPL-1 shipped — sibling of `orient_serve::tests`).
//!
//! These exercise the EXACT `handle_explain` GREEN sequence — bounded-cert PRECHECK
//! (`orient_bounded_cert_is_green`) -> `run_explain` through the `OrientServeDecorator` -> the existing
//! `build_explain_envelope`. The helpers/fixtures live in sibling modules per the 500-line guardrail:
//! [`spy`] (the partial storage spy) and [`fanin_fixture`] (the high-fan-in parity fixture). The single-
//! caller proofs reuse the shared `callgraph_cert::test_fixture` (GREEN by construction).
//!
//! - **V1 PARITY (high fan-in, RANKED)**: `run_explain` through the decorator == `run_explain` over bare
//!   SQLite for a SYMBOL with fan-in > the budget cap across two modules (byte/value parity of BOTH the bare
//!   `OrientResult` AND the assembled `CoherenceEnvelope`). The relevance ranking
//!   (`agent::explain::call_ranking`) sorts the FULL caller set identically whichever store served it, so
//!   the budget-truncated `items` are byte-identical — **the test that would FAIL without the ranking**
//!   (DR-EXPLAIN-CALLER-ORDER resolution, `2d6d00d`). The order-independence of the ranking itself is
//!   unit-proven in `agent::explain::call_ranking::tests`.
//! - **V2 NO-EAGER-`nodes`-READ (SYMBOL)**: a PARTIAL spy that PANICS on the four served focus-resolution
//!   `nodes` methods (+ the two callgraph methods) on the INNER SQLite port. explain SYMBOL through the
//!   decorator-over-spy completes WITHOUT panicking -> explain SYMBOL is `nodes`-FREE on green. The (c)
//!   trust read + cycles + gate/Authority reads ARE allowed (delegated to the real storage).
//! - **HONEST BOUND (FILE / PATH)**: a RECORDING spy proves the M-2-LEAVES-OFF decorator (`new()` — the
//!   pre-M-2 posture) still reads `compute_file_summary` / `list_symbols_in_file` (FILE) and
//!   `compute_path_summary` / `list_files_in_path` (PATH) — nothing silently claimed `nodes`-free there.
//!   EC-M2 NARROWED the green bound: through `with_leaf_serves` the summaries + cycle finders serve from
//!   the LiveGraph (`m2_no_eager_read_explain_file_and_path_serve_from_livegraph`), and ONLY the per-item
//!   LISTINGS remain SQLite (the DR-E3 listing half — still asserted).
//! - **RED FALLBACK**: on a RED bounded cert (callgraph diverges) the daemon runs the six (b) methods over
//!   bare SQLite; the answer is the bare SQLite answer (no LiveGraph leak) and the callgraph leaf is
//!   SQLite-LABELLED.
//! - **EC-M2 (review-0 #3)**: FILE/PATH parity through the M-2-enabled decorator (`m2_parity_explain_*`),
//!   a NON-EMPTY served explain cycle, and the FILE/PATH no-eager-read proof.

use std::sync::atomic::Ordering;

use repo_graph_agent::{AgentStorageRead, Budget, SignalCode};
use repo_graph_coherence::Source;
use repo_graph_gate::GateStorageRead;
use repo_graph_trust_model::LanguageSupport;

use crate::callgraph_cert::test_fixture;
use crate::explain_coherence::build_explain_envelope;
use crate::orient_serve::{orient_bounded_cert_is_green, OrientServeDecorator};

use self::spy::ServeSpy;

mod fanin_fixture;
mod spy;

const NOW: &str = "2026-01-01T00:00:00Z";

/// `run_explain` over a port, REPO + a target, Medium budget. The daemon floors explain to Medium.
fn run_explain<S: AgentStorageRead + GateStorageRead + ?Sized>(
    storage: &S,
    repo: &str,
    target: &str,
) -> repo_graph_agent::OrientResult {
    repo_graph_agent::run_explain(storage, repo, target, Budget::Medium, NOW)
        .expect("run_explain ok")
}

/// W-B-EPOCH-IMPL-1: a captured `RequestEpoch` for the GREEN fixture — the pinned `AgentSnapshot` + the
/// build-then-peek bounded-cert eligibility (`Some(fp)` on the green mirror). `handle_explain` builds the
/// SAME epoch; the decorator's EV-A gate matches it against the (unswapped) resident fingerprint, so the
/// served (b) leaves are byte-identical to before this slice.
fn green_epoch(
    state: &crate::state::RepoState,
    snapshot_uid: &str,
    repo: &str,
) -> crate::livegraph_feed::RequestEpoch {
    let storage = state.storage().expect("storage");
    let snapshot = repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, repo)
        .expect("get_latest_snapshot ok")
        .expect("ready snapshot");
    let fingerprint = crate::orient_serve::orient_bounded_cert_eligibility(state, snapshot_uid);
    crate::livegraph_feed::RequestEpoch {
        snapshot,
        fingerprint,
    }
}

// ── V1 PARITY (high fan-in, RANKED): decorator-served explain SYMBOL == bare-SQLite explain SYMBOL ────

#[test]
fn parity_explain_symbol_high_fanin_ranked_equals_sqlite() {
    let f = fanin_fixture::build();
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    // Precondition: the bounded cert is GREEN over the high-fan-in corpus (so handle_explain picks the
    // decorator path). A faithful SQLite mirror of the resident LiveGraph -> focus-resolution ∧ callgraph
    // both GREEN.
    assert!(
        orient_bounded_cert_is_green(&f.state, &f.snapshot_uid),
        "faithful high-fan-in mirror -> bounded cert GREEN"
    );

    let target = fanin_fixture::hub_key();

    // Decorator path: focus resolution + callers served from the LiveGraph (raw IR-edge order), then
    // ranked + truncated by the agent.
    let served = {
        let epoch = green_epoch(&f.state, &f.snapshot_uid, fanin_fixture::REPO);
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &storage, &epoch);
        run_explain(&decorator, fanin_fixture::REPO, &target)
    };
    // Bare SQLite path (the RED fallback / today's eager read): callers in SQLite raw order, then ranked +
    // truncated identically.
    let plain = run_explain(&storage, fanin_fixture::REPO, &target);

    // (a) The bare `OrientResult` is byte/value-identical despite the two stores returning the caller SET in
    // different raw orders — the ranking reconciles them BEFORE truncation, so the cert-proven-equal SET
    // yields the same ranked top-N.
    assert_eq!(
        serde_json::to_value(&served).unwrap(),
        serde_json::to_value(&plain).unwrap(),
        "ranked high-fan-in explain SYMBOL is byte/value-identical SQLite-vs-LiveGraph"
    );

    // (b) The ranked + budget-truncated caller items: fan-in (ALPHA_N + BETA_N) > cap (15); the ranked top-15
    // is the ALPHA_N high-concentration `alpha` callers (name ASC) + the first (15 - ALPHA_N) `beta` callers.
    const CAP: usize = 15;
    let total = (fanin_fixture::ALPHA_N + fanin_fixture::BETA_N) as u64;
    let ev = served
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainCallers)
        .and_then(|s| s.explain_callers_evidence())
        .expect("EXPLAIN_CALLERS present for the hub SYMBOL");
    assert_eq!(ev.count, total, "the full fan-in count is preserved");
    assert_eq!(
        ev.items.len(),
        CAP,
        "items truncated to the Medium cap (15)"
    );
    assert_eq!(ev.items_truncated, Some(true), "truncation actually bit");
    assert_eq!(
        ev.items_omitted_count,
        Some(total - CAP as u64),
        "the omitted overflow count is honest"
    );
    let got: Vec<String> = ev.items.iter().map(|it| it.stable_key.clone()).collect();
    assert_eq!(
        got,
        fanin_fixture::expected_ranked_caller_keys(CAP),
        "the ranked top-N is the high-concentration alpha block then beta — deterministic across stores"
    );

    // (c) The assembled `CoherenceEnvelope` (what the daemon serializes for `rmap explain`) is also
    // byte/value-identical — the ranked item order flows through `serve_callers` unchanged (it swaps live
    // names per key but preserves the agent's row order).
    let env_served = build_explain_envelope(&f.state, fanin_fixture::REPO, served, false, false);
    let env_plain = build_explain_envelope(&f.state, fanin_fixture::REPO, plain, false, false);
    assert_eq!(
        serde_json::to_value(&env_served).unwrap(),
        serde_json::to_value(&env_plain).unwrap(),
        "the explain envelope is byte/value-identical through the decorator vs bare SQLite on green"
    );
    assert!(
        env_served
            .value
            .signals
            .iter()
            .any(|l| l.value.code() == SignalCode::ExplainCallers),
        "symbol focus emits EXPLAIN_CALLERS"
    );
}

// ── V2 NO-EAGER-`nodes`-READ (SYMBOL): a partial spy that PANICS on the served methods ───────────

#[test]
fn no_eager_nodes_read_explain_symbol_serves_from_livegraph() {
    let f = test_fixture::build_fixture(false);
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    assert!(orient_bounded_cert_is_green(&f.state, &f.snapshot_uid));

    // PANIC on the six served (b) methods; DELEGATE everything else (the (c) trust read, cycles,
    // gate/Authority, FS) to the real storage.
    let spy = ServeSpy::panicking(&storage);
    let target = test_fixture::callee_key();
    let result = {
        let epoch = green_epoch(&f.state, &f.snapshot_uid, test_fixture::REPO);
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &spy, &epoch);
        // If ANY served (b) method (the four focus-resolution `nodes` methods or callers/callees) hit
        // SQLite, the spy PANICS. Completing a SYMBOL-focus explain proves explain SYMBOL is `nodes`-free
        // on green: focus resolution is served from the LiveGraph. The (c) trust + cycles `edges` reads are
        // delegated (allowed).
        run_explain(&decorator, test_fixture::REPO, &target)
    };
    assert!(
        result
            .signals
            .iter()
            .any(|s| s.code() == SignalCode::ExplainCallers),
        "the EXPLAIN_CALLERS served from the LiveGraph is present"
    );
}

// ── HONEST BOUND: explain FILE / PATH STILL read `nodes` (summaries/listings) on green ───────────

#[test]
fn honest_bound_explain_file_still_reads_nodes_on_green() {
    let f = test_fixture::build_fixture(false);
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    assert!(orient_bounded_cert_is_green(&f.state, &f.snapshot_uid));

    let spy = ServeSpy::recording(&storage);
    {
        let epoch = green_epoch(&f.state, &f.snapshot_uid, test_fixture::REPO);
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &spy, &epoch);
        // FILE focus through the M-2-LEAVES-OFF decorator (`new()` — the pre-M-2 posture): focus
        // resolution is served, but `explain_file` reads `compute_file_summary` +
        // `list_symbols_in_file` — `nodes` reads, DELEGATED to SQLite. EC-M2 serves the summary
        // ONLY through `with_leaf_serves` on a GREEN module-summary cert (proven in
        // `m2_no_eager_read_explain_file_and_path_serve_from_livegraph`); the leaves-off path
        // must keep delegating byte-identically.
        let _ = run_explain(&decorator, test_fixture::REPO, test_fixture::CALLER_PATH);
        // "src/a.ts"
    }
    assert!(
        spy.read_compute_file_summary.load(Ordering::Relaxed),
        "explain FILE reads compute_file_summary (`nodes`) on the M-2-leaves-off decorator (honest bound)"
    );
    assert!(
        spy.read_list_symbols_in_file.load(Ordering::Relaxed),
        "explain FILE STILL reads list_symbols_in_file (`nodes`) on green (honest bound — ALL paths)"
    );
}

#[test]
fn honest_bound_explain_path_still_reads_nodes_on_green() {
    let f = test_fixture::build_fixture(false);
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    assert!(orient_bounded_cert_is_green(&f.state, &f.snapshot_uid));

    let spy = ServeSpy::recording(&storage);
    {
        let epoch = green_epoch(&f.state, &f.snapshot_uid, test_fixture::REPO);
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &spy, &epoch);
        // PATH focus through the M-2-LEAVES-OFF decorator (`new()` — the pre-M-2 posture):
        // `explain_path` reads `compute_path_summary` + `list_files_in_path` — DELEGATED to
        // SQLite. The M-2 serve happens only through `with_leaf_serves` (see
        // `m2_no_eager_read_explain_file_and_path_serve_from_livegraph`).
        let _ = run_explain(&decorator, test_fixture::REPO, test_fixture::MODULE_DIR);
        // "src"
    }
    assert!(
        spy.read_compute_path_summary.load(Ordering::Relaxed),
        "explain PATH reads compute_path_summary (`nodes`) on the M-2-leaves-off decorator (honest bound)"
    );
    assert!(
        spy.read_list_files_in_path.load(Ordering::Relaxed),
        "explain PATH STILL reads list_files_in_path (`nodes`) on green (honest bound — ALL paths)"
    );
}

// ── RED FALLBACK: a divergent callgraph forces the bounded cert RED -> bare SQLite, SQLite-labelled ─

#[test]
fn red_bounded_cert_falls_back_to_bare_sqlite() {
    // `drop_calls = true`: the SQLite mirror omits the CALLS edge the LiveGraph carries. Focus resolution
    // is still faithful (`nodes` untouched) so its cert is GREEN, but the callgraph diverges -> RED ->
    // bounded cert (focus-res ∧ callgraph) RED -> handle_explain declines the decorator (serve_from_lg=false).
    let f = test_fixture::build_fixture(true);
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    assert!(
        crate::focus_resolution_cert::focus_resolution_is_green(&f.state, &f.snapshot_uid),
        "focus resolution stays GREEN (nodes faithful) — only the callgraph diverges"
    );
    assert!(
        !crate::callgraph_cert::callgraph_is_green(&f.state, &f.snapshot_uid),
        "the dropped SQLite CALLS edge makes the callgraph cert RED"
    );
    assert!(
        !orient_bounded_cert_is_green(&f.state, &f.snapshot_uid),
        "focus-res GREEN ∧ callgraph RED -> bounded cert RED -> serve_from_lg = false"
    );

    // The dispatch's serve_from_lg == false branch: bare SQLite (the unchanged eager path).
    let callee = test_fixture::callee_key();
    let result = run_explain(&storage, test_fixture::REPO, &callee);

    // Non-leak: the bare answer has ZERO callers (SQLite has no CALLS edge) — the LiveGraph's caller
    // (callerFn) did NOT leak through. A decorator path would have served 1 caller.
    let callers_count = result
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainCallers)
        .and_then(|s| s.explain_callers_evidence())
        .map(|e| e.count);
    assert_eq!(
        callers_count,
        Some(0),
        "RED fallback serves the bare SQLite answer (0 callers); no LiveGraph caller leaks"
    );

    // The assembled envelope labels the callgraph leaf SQLite (the RED-path honest provenance).
    let env = build_explain_envelope(&f.state, test_fixture::REPO, result, false, false);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == SignalCode::ExplainCallers)
        .expect("EXPLAIN_CALLERS leaf present");
    assert!(
        leaf.provenance.source.contains(&Source::Sqlite)
            && !leaf.provenance.source.contains(&Source::Livegraph),
        "on the RED path the callgraph leaf is SQLite-LABELLED, never livegraph"
    );
}

// ── W-B-EPOCH-IMPL-1: explain is epoch-pinned (review-0 #1) ───────────────────────────────────────

/// (a) explain does NOT re-resolve "latest" mid-request. The inner port PANICS on `get_latest_snapshot`;
/// through the epoch-pinned decorator that call returns the captured `epoch.snapshot` instead, and the six
/// (b) leaves are LiveGraph-served on green (also panic-guarded). So a completing explain whose stamp is the
/// PINNED uid proves `run_explain` resolved the snapshot exactly once (via the decorator's pin) with no
/// delegated re-read — the explain analogue of orient's double-resolve removal.
#[test]
fn explain_pins_captured_snapshot_no_reresolve() {
    let f = test_fixture::build_fixture(false);
    let storage = f.state.storage().unwrap();
    assert!(orient_bounded_cert_is_green(&f.state, &f.snapshot_uid));

    let spy = ServeSpy::panicking(&storage).panic_on_snapshot_resolve();
    let target = test_fixture::callee_key();
    let result = {
        let epoch = green_epoch(&f.state, &f.snapshot_uid, test_fixture::REPO);
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &spy, &epoch);
        run_explain(&decorator, test_fixture::REPO, &target)
    };
    assert_eq!(
        result.snapshot, f.snapshot_uid,
        "explain stamps the captured (pinned) snapshot_uid — no mid-request 'latest' re-read"
    );
}

/// (b) EV-A: a mid-request LiveGraph swap makes explain fail soft to the PINNED SQLite snapshot, never the
/// swapped LiveGraph. Green serves the LiveGraph caller (1) despite an emptied SQLite; after the swap the
/// captured epoch is stale, so the caller leaf delegates to SQLite at the pinned uid (0), and the stamp stays
/// the pinned snapshot N — coherent, never a cross-epoch mix.
#[test]
fn explain_ev_a_falls_back_to_pinned_sqlite_after_swap() {
    let f = test_fixture::build_fixture(false);
    let storage = f.state.storage().unwrap();
    let epoch = green_epoch(&f.state, &f.snapshot_uid, test_fixture::REPO);
    let callee = test_fixture::callee_key();

    let callers_count = |r: &repo_graph_agent::OrientResult| -> Option<u64> {
        r.signals
            .iter()
            .find(|s| s.code() == SignalCode::ExplainCallers)
            .and_then(|s| s.explain_callers_evidence())
            .map(|e| e.count)
    };

    // Diverge SQLite from the LiveGraph so the two serve sites are distinguishable: drop the SQLite CALLS
    // edge. SQLite callers(calleeFn)=∅; the LiveGraph still has callerFn. (A SQLite-only mutation does NOT
    // move the LiveGraph fingerprint, so the captured epoch still matches — green still serves the LG.)
    storage.delete_edges_by_uids(&["ec0".to_string()]).unwrap();
    let decorator = OrientServeDecorator::new(&f.state.livegraph, &storage, &epoch);

    let served = run_explain(&decorator, test_fixture::REPO, &callee);
    assert_eq!(
        served.snapshot, f.snapshot_uid,
        "green: stamp is the pinned snapshot"
    );
    assert_eq!(
        callers_count(&served),
        Some(1),
        "green: explain serves the LiveGraph caller despite the emptied SQLite"
    );

    // EV-A: swap the LiveGraph (the resident fingerprint moves) -> the captured epoch is stale -> explain's
    // caller leaf fails soft to the PINNED SQLite snapshot (emptied), NOT the swapped LiveGraph.
    f.state.livegraph.write().as_mut().unwrap().load_partition(
        "p",
        test_fixture::build_ir(),
        LanguageSupport::TypeScriptPrimary,
    );
    let fell_back = run_explain(&decorator, test_fixture::REPO, &callee);
    assert_eq!(
        fell_back.snapshot, f.snapshot_uid,
        "after swap: the stamp STAYS the pinned snapshot N (no cross-epoch mix)"
    );
    assert_eq!(
        callers_count(&fell_back),
        Some(0),
        "after swap: explain serves SQLite@pin (∅), never the stale LiveGraph caller"
    );
}

/// (c) ALL-OFF witness: explain STILL pins, and the always-wrap change is transparent. `handle_explain`
/// wraps the decorator even when NO leaf serves (fingerprint `None` — e.g. no resident LiveGraph, or
/// every leaf cert RED), so the `epoch_resident` short-circuit makes every leaf delegate to SQLite at
/// the pinned uid while `get_latest_snapshot` returns the pinned snapshot. The decorator-served answer
/// is therefore byte/value-identical to bare-SQLite explain — the pin adds coherence without changing
/// the output. (Constructed here via the BOUNDED-only eligibility, which is `None` on this fixture;
/// under EC-M2 review-0 #1 the REAL dispatch witness on this fixture would mint a fingerprint from the
/// still-GREEN module-summary leaf and serve THAT leaf — the all-off shape this test pins remains real
/// for the no-LiveGraph / all-RED states.)
#[test]
fn explain_red_epoch_pins_and_is_transparent() {
    // drop_calls = true: the SQLite mirror omits the CALLS edge -> callgraph diverges -> bounded cert RED.
    let f = test_fixture::build_fixture(true);
    let storage = f.state.storage().unwrap();
    assert!(!orient_bounded_cert_is_green(&f.state, &f.snapshot_uid));

    // The BOUNDED eligibility carries no witness on a RED fold (`fingerprint == None`) — the all-off epoch.
    let epoch = green_epoch(&f.state, &f.snapshot_uid, test_fixture::REPO);
    assert!(
        epoch.fingerprint.is_none(),
        "RED bounded cert -> no BOUNDED eligibility witness (fingerprint None)"
    );

    let callee = test_fixture::callee_key();
    let via_decorator = {
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &storage, &epoch);
        run_explain(&decorator, test_fixture::REPO, &callee)
    };
    let bare = run_explain(&storage, test_fixture::REPO, &callee);

    assert_eq!(
        serde_json::to_value(&via_decorator).unwrap(),
        serde_json::to_value(&bare).unwrap(),
        "RED explain through the epoch-pinned decorator == bare SQLite explain (transparent)"
    );
    assert_eq!(
        via_decorator.snapshot, f.snapshot_uid,
        "RED path still stamps the pinned snapshot (get_latest_snapshot returns epoch.snapshot)"
    );
}

// ── EC-M2-LEAF-SERVE-1 (review-0 #3): explain FILE/PATH parity + no-eager-read through the
//    M-2-enabled decorator — the missing GREEN-path coverage for the newly served methods
//    (`compute_{file,path}_summary`, `find_cycles_involving_path`). Sibling of the orient proofs in
//    `orient_serve::tests` (`m2_parity_*`), through `run_explain` — the changed explain surface. ──

/// The EXACT `handle_explain` capture sequence: resolve the READY snapshot, capture the FULL serve
/// witness, pin the epoch at the witness fingerprint. Returns both so tests can assert the witness
/// decisions and construct the decorator dispatch would.
fn witness_epoch(
    state: &crate::state::RepoState,
    repo: &str,
) -> (
    crate::livegraph_feed::RequestEpoch,
    crate::orient_serve::OrientServeWitness,
) {
    let storage = state.storage().expect("storage");
    let snapshot = repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, repo)
        .expect("get_latest_snapshot ok")
        .expect("ready snapshot");
    let w = crate::orient_serve::orient_serve_witness(state, &snapshot.snapshot_uid);
    (
        crate::livegraph_feed::RequestEpoch {
            snapshot,
            fingerprint: w.fingerprint.clone(),
        },
        w,
    )
}

/// explain FILE-focus parity: `run_explain("src/a.ts")` through the M-2-enabled decorator (the
/// FILE identity counts served from the LiveGraph inventory) is byte/value-identical to bare
/// SQLite. Non-vacuous: the FILE identity leaf is present.
#[test]
fn m2_parity_explain_file_focus_equals_sqlite() {
    let f = test_fixture::build_fixture(false);
    let storage = f.state.storage().unwrap();
    let (epoch, w) = witness_epoch(&f.state, test_fixture::REPO);
    assert!(w.bounded && w.m2.module_summary && w.m2.cycle_values);

    let served = {
        let decorator = OrientServeDecorator::with_leaf_serves(
            &f.state.livegraph,
            &storage,
            &epoch,
            w.bounded,
            w.m2,
        );
        run_explain(&decorator, test_fixture::REPO, test_fixture::CALLER_PATH)
    };
    let plain = run_explain(&storage, test_fixture::REPO, test_fixture::CALLER_PATH);
    assert_eq!(
        serde_json::to_value(&served).unwrap(),
        serde_json::to_value(&plain).unwrap(),
        "M-2 GREEN explain FILE focus is byte/value-identical decorator-vs-SQLite"
    );
    assert!(
        served
            .signals
            .iter()
            .any(|s| s.code() == SignalCode::ExplainIdentity),
        "FILE focus emits EXPLAIN_IDENTITY (its counts are the compute_file_summary leaf)"
    );
}

/// explain PATH-focus parity with a NON-EMPTY cycle: `run_explain("src")` through the M-2-enabled
/// decorator serves `compute_path_summary` + `find_cycles_involving_path` from the LiveGraph,
/// byte/value-identical to bare SQLite — and the answer carries the REAL `src` ↔ `lib` cycle
/// (canonical qualified members), so the cycle-VALUES serve is exercised non-vacuously.
#[test]
fn m2_parity_explain_path_focus_equals_sqlite_with_nonempty_cycle() {
    let f = test_fixture::build_fixture(false);
    let storage = f.state.storage().unwrap();
    let (epoch, w) = witness_epoch(&f.state, test_fixture::REPO);
    assert!(w.bounded && w.m2.module_summary && w.m2.cycle_values);

    let served = {
        let decorator = OrientServeDecorator::with_leaf_serves(
            &f.state.livegraph,
            &storage,
            &epoch,
            w.bounded,
            w.m2,
        );
        run_explain(&decorator, test_fixture::REPO, test_fixture::MODULE_DIR)
    };
    let plain = run_explain(&storage, test_fixture::REPO, test_fixture::MODULE_DIR);
    assert_eq!(
        serde_json::to_value(&served).unwrap(),
        serde_json::to_value(&plain).unwrap(),
        "M-2 GREEN explain PATH focus is byte/value-identical decorator-vs-SQLite"
    );
    let cycles = served
        .signals
        .iter()
        .find(|s| s.code() == SignalCode::ExplainCycles)
        .expect("EXPLAIN_CYCLES present (the src <-> lib cycle)");
    let v = serde_json::to_value(cycles).unwrap();
    assert_eq!(
        v["evidence"]["items"][0]["modules"],
        serde_json::json!(["lib", "src"]),
        "the served explain cycle is the canonicalized (member-sorted) QUALIFIED ring — non-empty"
    );
}

/// The review-0 #3 explain NO-EAGER-READ proof: through the M-2-enabled decorator, explain FILE
/// must NOT read `compute_file_summary` from SQLite and explain PATH must NOT read
/// `compute_path_summary` / `find_cycles_involving_path` (all LiveGraph-served; the cancellable
/// cycle variant funnels through the recorded non-cancellable default). The per-item LISTING reads
/// (`list_symbols_in_file` / `list_files_in_path`) still delegate — the DR-E3 honest bound,
/// asserted here so this slice's claim stays bounded.
#[test]
fn m2_no_eager_read_explain_file_and_path_serve_from_livegraph() {
    let f = test_fixture::build_fixture(false);
    let storage = f.state.storage().unwrap();
    let (epoch, w) = witness_epoch(&f.state, test_fixture::REPO);
    assert!(w.bounded && w.m2.module_summary && w.m2.cycle_values);

    let spy = ServeSpy::recording(&storage);
    {
        let decorator = OrientServeDecorator::with_leaf_serves(
            &f.state.livegraph,
            &spy,
            &epoch,
            w.bounded,
            w.m2,
        );
        let _ = run_explain(&decorator, test_fixture::REPO, test_fixture::CALLER_PATH);
        let _ = run_explain(&decorator, test_fixture::REPO, test_fixture::MODULE_DIR);
    }
    assert!(
        !spy.read_compute_file_summary.load(Ordering::Relaxed),
        "explain FILE served compute_file_summary from the LiveGraph — zero SQLite read (M-2)"
    );
    assert!(
        !spy.read_compute_path_summary.load(Ordering::Relaxed),
        "explain PATH served compute_path_summary from the LiveGraph — zero SQLite read (M-2)"
    );
    assert!(
        !spy.read_find_cycles_involving_path.load(Ordering::Relaxed),
        "explain PATH served find_cycles_involving_path from the LiveGraph SCC — zero SQLite read"
    );
    // The honest bound is UNCHANGED: the per-item listings still read SQLite on green.
    assert!(
        spy.read_list_symbols_in_file.load(Ordering::Relaxed)
            && spy.read_list_files_in_path.load(Ordering::Relaxed),
        "the FILE/PATH per-item LISTINGS still delegate to SQLite (the DR-E3 honest bound)"
    );
}
