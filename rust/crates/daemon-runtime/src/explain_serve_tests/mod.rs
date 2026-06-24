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
//! - **HONEST BOUND (FILE / PATH)**: a RECORDING spy proves explain FILE still reads `compute_file_summary` /
//!   `list_symbols_in_file` and explain PATH still reads `compute_path_summary` / `list_files_in_path` ON
//!   GREEN — this slice did NOT silently claim FILE/PATH `nodes`-free (mirrors the packet HONEST-BOUND GUARD).
//! - **RED FALLBACK**: on a RED bounded cert (callgraph diverges) the daemon declines the decorator and runs
//!   bare SQLite; the answer is the bare SQLite answer (no LiveGraph leak) and the callgraph leaf is
//!   SQLite-LABELLED.

use std::sync::atomic::Ordering;

use repo_graph_agent::{AgentStorageRead, Budget, SignalCode};
use repo_graph_coherence::Source;
use repo_graph_gate::GateStorageRead;

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
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &storage);
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
    let env_served = build_explain_envelope(&f.state, fanin_fixture::REPO, served, false);
    let env_plain = build_explain_envelope(&f.state, fanin_fixture::REPO, plain, false);
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
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &spy);
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
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &spy);
        // FILE focus: focus resolution is served (no `nodes` read for resolution), but `explain_file`
        // reads `compute_file_summary` + `list_symbols_in_file` (the identity/symbols leaves) — `nodes`
        // reads, DELEGATED to SQLite. They are NOT decorator-served (the honest bound).
        let _ = run_explain(&decorator, test_fixture::REPO, test_fixture::CALLER_PATH);
        // "src/a.ts"
    }
    assert!(
        spy.read_compute_file_summary.load(Ordering::Relaxed),
        "explain FILE STILL reads compute_file_summary (`nodes`) on green — NOT `nodes`-free (honest bound)"
    );
    assert!(
        spy.read_list_symbols_in_file.load(Ordering::Relaxed),
        "explain FILE STILL reads list_symbols_in_file (`nodes`) on green (honest bound)"
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
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &spy);
        // PATH focus: focus resolution served, but `explain_path` reads `compute_path_summary` +
        // `list_files_in_path` (the identity/files leaves) — `nodes` reads, DELEGATED to SQLite.
        let _ = run_explain(&decorator, test_fixture::REPO, test_fixture::MODULE_DIR);
        // "src"
    }
    assert!(
        spy.read_compute_path_summary.load(Ordering::Relaxed),
        "explain PATH STILL reads compute_path_summary (`nodes`) on green — NOT `nodes`-free (honest bound)"
    );
    assert!(
        spy.read_list_files_in_path.load(Ordering::Relaxed),
        "explain PATH STILL reads list_files_in_path (`nodes`) on green (honest bound)"
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
    let env = build_explain_envelope(&f.state, test_fixture::REPO, result, false);
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
