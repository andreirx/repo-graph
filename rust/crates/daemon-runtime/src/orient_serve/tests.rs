//! COHERENCE-LEAF-SERVE-IMPL-1: orient bounded (b)-leaf serve-then-fallback tests.
//!
//! Three proofs, all on the SHARED faithful fixture (`callgraph_cert::test_fixture`) where the bounded
//! orient cert is GREEN by construction:
//!
//! - **V1 PARITY**: `orient` through the decorator == `orient` through the bare SQLite storage
//!   (byte/value parity of the whole `OrientResult`) on a GREEN-cert TS fixture. The (b) leaves served
//!   from the LiveGraph are byte-identical to the SQLite leaves.
//! - **V2 NO-EAGER-(b)-READ**: a PARTIAL spy that PANICS on the six served (b) methods (focus
//!   resolution + callers/callees) and DELEGATES the rest. `orient` through the decorator-over-spy for a
//!   SYMBOL focus completes WITHOUT panicking -> the (b) leaves did NOT touch SQLite (the operational
//!   definition of "eager read eliminated"); the (c) trust read + cycles + MODULE_SUMMARY are allowed.
//! - the BOUNDED-cert gate (`orient_bounded_cert_is_green`) is GREEN on the faithful fixture and RED
//!   without a LiveGraph.
//!
//! Plus PURE unit tests for the focus-resolution native -> agent-DTO mappers.

use super::*;
use crate::callgraph_cert::test_fixture;
use repo_graph_agent::{
    AgentBoundaryDeclaration, AgentBoundaryLinksFreshness, AgentCalleeRow, AgentCallerRow,
    AgentComplexityMeasurement, AgentCycle, AgentDeadNode, AgentDocEntry, AgentFileEntry,
    AgentImportEdge, AgentImportEntry, AgentModuleSummary, AgentRepo, AgentRepoSummary,
    AgentSnapshot, AgentStaleFile, AgentStorageError, AgentSymbolEntry, AgentTrustSummary,
};
use repo_graph_gate::{
    GateBoundaryDeclaration, GateImportEdge, GateInference, GateMeasurement,
    GateModuleViolationEvidence, GateQualityAssessmentFact, GateRequirement, GateStorageError,
    GateWaiver,
};

// ── PURE focus-resolution native -> agent-DTO mapper tests ──────────────────────────────────────

#[test]
fn map_path_resolution_renames_fields() {
    let native = PathResolutionAnswer {
        has_exact_file: true,
        file_key: Some("repo:src/a.ts:FILE".into()),
        has_content_under_prefix: false,
        module_key: Some("repo:src:MODULE".into()),
    };
    let dto = map_path_resolution(&native);
    assert!(dto.has_exact_file);
    assert_eq!(dto.file_stable_key.as_deref(), Some("repo:src/a.ts:FILE"));
    assert!(!dto.has_content_under_prefix);
    assert_eq!(dto.module_stable_key.as_deref(), Some("repo:src:MODULE"));
}

#[test]
fn map_candidate_and_kind() {
    let native = FocusCandidate {
        key: "repo:src/a.ts#foo:SYMBOL:FUNCTION".into(),
        kind: FocusKind::Symbol,
        file: Some("src/a.ts".into()),
    };
    let dto = map_candidate(&native);
    assert_eq!(dto.stable_key, "repo:src/a.ts#foo:SYMBOL:FUNCTION");
    assert_eq!(dto.kind, repo_graph_agent::AgentFocusKind::Symbol);
    assert_eq!(dto.file.as_deref(), Some("src/a.ts"));
}

#[test]
fn map_symbol_context_renames_module_key() {
    let native = SymbolContext {
        file_path: Some("src/a.ts".into()),
        module_path: Some("src".into()),
        module_key: Some("repo:src:MODULE".into()),
        name: "foo".into(),
        qualified_name: Some("foo".into()),
        subtype: Some("FUNCTION".into()),
        line_start: Some(3),
    };
    let dto = map_symbol_context(&native);
    assert_eq!(dto.file_path.as_deref(), Some("src/a.ts"));
    assert_eq!(dto.module_path.as_deref(), Some("src"));
    assert_eq!(dto.module_stable_key.as_deref(), Some("repo:src:MODULE"));
    assert_eq!(dto.name, "foo");
    assert_eq!(dto.qualified_name.as_deref(), Some("foo"));
    assert_eq!(dto.subtype.as_deref(), Some("FUNCTION"));
    assert_eq!(dto.line_start, Some(3));
}

// ── The bounded cert gate ───────────────────────────────────────────────────────────────────────

#[test]
fn bounded_cert_green_on_faithful_fixture() {
    let f = test_fixture::build_fixture(false);
    assert!(
        orient_bounded_cert_is_green(&f.state, &f.snapshot_uid),
        "focus-resolution ∧ callgraph both GREEN on the faithful mirror"
    );
}

#[test]
fn bounded_cert_red_without_livegraph() {
    let f = test_fixture::build_fixture(false);
    *f.state.livegraph.write() = None;
    assert!(!orient_bounded_cert_is_green(&f.state, &f.snapshot_uid));
}

/// W-B-EPOCH-IMPL-1: a captured `RequestEpoch` for the GREEN faithful fixture — the pinned `AgentSnapshot`
/// plus the build-then-peek bounded-cert eligibility (`Some(fp)` on the green mirror). The decorator's
/// EV-A gate matches it against the (unswapped) resident fingerprint, so every (b) leaf serves from the
/// LiveGraph exactly as before this slice. Shared by the parity + no-eager-read decorator tests.
fn green_epoch(
    state: &crate::state::RepoState,
    snapshot_uid: &str,
) -> crate::livegraph_feed::RequestEpoch {
    let storage = state.storage().expect("storage");
    let snapshot =
        repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, test_fixture::REPO)
            .expect("get_latest_snapshot ok")
            .expect("ready snapshot");
    let fingerprint = orient_bounded_cert_eligibility(state, snapshot_uid);
    crate::livegraph_feed::RequestEpoch {
        snapshot,
        fingerprint,
    }
}

// ── V1 PARITY: decorator-served orient == bare-SQLite orient ─────────────────────────────────────

#[test]
fn parity_orient_decorator_equals_sqlite_symbol_focus() {
    let f = test_fixture::build_fixture(false);
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    // Precondition: the bounded cert is GREEN (so the daemon would pick the decorator path).
    assert!(orient_bounded_cert_is_green(&f.state, &f.snapshot_uid));

    let focus = test_fixture::callee_key();
    let now = "2026-01-01T00:00:00Z";

    // Decorator path: focus resolution + callers/callees served from the LiveGraph.
    let served = {
        let epoch = green_epoch(&f.state, &f.snapshot_uid);
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &storage, &epoch);
        repo_graph_agent::orient(
            &decorator,
            test_fixture::REPO,
            Some(focus.as_str()),
            repo_graph_agent::Budget::Small,
            now,
        )
        .expect("decorator orient ok")
    };
    // Bare SQLite path (the fallback / today's eager read).
    let plain = repo_graph_agent::orient(
        &storage,
        test_fixture::REPO,
        Some(focus.as_str()),
        repo_graph_agent::Budget::Small,
        now,
    )
    .expect("sqlite orient ok");

    assert_eq!(
        serde_json::to_value(&served).unwrap(),
        serde_json::to_value(&plain).unwrap(),
        "LiveGraph-served orient is byte/value-identical to the SQLite orient (no-loss)"
    );
    // And the served result actually carries the LG-derived CALLERS_SUMMARY (callerFn calls calleeFn).
    assert!(
        served
            .signals
            .iter()
            .any(|s| s.code() == repo_graph_agent::SignalCode::CallersSummary),
        "symbol focus emits CALLERS_SUMMARY served from the LiveGraph"
    );
}

#[test]
fn parity_orient_decorator_equals_sqlite_repo_focus() {
    // Repo focus emits no callers/callees + no focus resolution; the decorator still produces the
    // identical result (it delegates MODULE_SUMMARY / trust / cycles to SQLite verbatim).
    let f = test_fixture::build_fixture(false);
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    let now = "2026-01-01T00:00:00Z";
    let served = {
        let epoch = green_epoch(&f.state, &f.snapshot_uid);
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &storage, &epoch);
        repo_graph_agent::orient(
            &decorator,
            test_fixture::REPO,
            None,
            repo_graph_agent::Budget::Small,
            now,
        )
        .expect("decorator orient ok")
    };
    let plain = repo_graph_agent::orient(
        &storage,
        test_fixture::REPO,
        None,
        repo_graph_agent::Budget::Small,
        now,
    )
    .expect("sqlite orient ok");
    assert_eq!(
        serde_json::to_value(&served).unwrap(),
        serde_json::to_value(&plain).unwrap()
    );
}

// ── V2 NO-EAGER-(b)-READ: a partial spy that PANICS on the served methods ────────────────────────

/// A partial spy over the real SQLite storage that PANICS on the SIX served (b) methods (focus
/// resolution + callers/callees) and DELEGATES everything else. On a GREEN bounded cert the decorator
/// serves those six from the LiveGraph, so the spy's panics must NEVER fire — that is the operational
/// proof of "ZERO eager `nodes`/`edges` read for the (b) leaves". The (c) trust read, cycles,
/// MODULE_SUMMARY, Authority/gate, and FS reads ARE allowed (delegated to the real storage).
struct PartialSpy<'a, S: ?Sized>(&'a S);

impl<S: AgentStorageRead + ?Sized> AgentStorageRead for PartialSpy<'_, S> {
    // ── the SIX served (b) methods: must be served from the LiveGraph, NEVER reached here ──
    fn resolve_path_focus(
        &self,
        _: &str,
        _: &str,
    ) -> Result<AgentPathResolution, AgentStorageError> {
        panic!("resolve_path_focus must be served from the LiveGraph on green")
    }
    fn resolve_stable_key_focus(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<AgentFocusCandidate>, AgentStorageError> {
        panic!("resolve_stable_key_focus must be served from the LiveGraph on green")
    }
    fn resolve_symbol_name(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Vec<AgentFocusCandidate>, AgentStorageError> {
        panic!("resolve_symbol_name must be served from the LiveGraph on green")
    }
    fn get_symbol_context(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Option<AgentSymbolContext>, AgentStorageError> {
        panic!("get_symbol_context must be served from the LiveGraph on green")
    }
    fn find_symbol_callers(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Vec<AgentCallerRow>, AgentStorageError> {
        panic!("find_symbol_callers must be served from the LiveGraph on green")
    }
    fn find_symbol_callees(
        &self,
        _: &str,
        _: &str,
    ) -> Result<Vec<AgentCalleeRow>, AgentStorageError> {
        panic!("find_symbol_callees must be served from the LiveGraph on green")
    }

    // ── everything else: DELEGATED (allowed reads) ──
    fn get_repo(&self, repo_uid: &str) -> Result<Option<AgentRepo>, AgentStorageError> {
        self.0.get_repo(repo_uid)
    }
    fn get_latest_snapshot(
        &self,
        repo_uid: &str,
    ) -> Result<Option<AgentSnapshot>, AgentStorageError> {
        self.0.get_latest_snapshot(repo_uid)
    }
    fn get_stale_files(&self, s: &str) -> Result<Vec<AgentStaleFile>, AgentStorageError> {
        self.0.get_stale_files(s)
    }
    fn find_module_cycles(&self, s: &str) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.0.find_module_cycles(s)
    }
    fn find_dead_nodes(
        &self,
        s: &str,
        r: &str,
        k: Option<&str>,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.0.find_dead_nodes(s, r, k)
    }
    fn get_active_boundary_declarations(
        &self,
        r: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
        self.0.get_active_boundary_declarations(r)
    }
    fn find_imports_between_paths(
        &self,
        s: &str,
        a: &str,
        b: &str,
    ) -> Result<Vec<AgentImportEdge>, AgentStorageError> {
        self.0.find_imports_between_paths(s, a, b)
    }
    fn compute_repo_summary(&self, s: &str) -> Result<AgentRepoSummary, AgentStorageError> {
        self.0.compute_repo_summary(s)
    }
    fn get_trust_summary(&self, r: &str, s: &str) -> Result<AgentTrustSummary, AgentStorageError> {
        self.0.get_trust_summary(r, s)
    }
    fn find_dead_nodes_in_path(
        &self,
        s: &str,
        r: &str,
        p: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.0.find_dead_nodes_in_path(s, r, p)
    }
    fn find_dead_nodes_in_file(
        &self,
        s: &str,
        r: &str,
        p: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.0.find_dead_nodes_in_file(s, r, p)
    }
    fn compute_path_summary(
        &self,
        s: &str,
        p: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        self.0.compute_path_summary(s, p)
    }
    fn compute_file_summary(
        &self,
        s: &str,
        p: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        self.0.compute_file_summary(s, p)
    }
    fn find_boundary_declarations_in_path(
        &self,
        r: &str,
        p: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
        self.0.find_boundary_declarations_in_path(r, p)
    }
    fn find_cycles_involving_path(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.0.find_cycles_involving_path(s, p)
    }
    fn find_cycles_involving_module(
        &self,
        s: &str,
        m: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.0.find_cycles_involving_module(s, m)
    }
    fn list_symbols_in_file(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentSymbolEntry>, AgentStorageError> {
        self.0.list_symbols_in_file(s, p)
    }
    fn list_files_in_path(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentFileEntry>, AgentStorageError> {
        self.0.list_files_in_path(s, p)
    }
    fn find_file_imports(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentImportEntry>, AgentStorageError> {
        self.0.find_file_imports(s, p)
    }
    fn get_doc_inventory(&self, r: &str) -> Result<Vec<AgentDocEntry>, AgentStorageError> {
        self.0.get_doc_inventory(r)
    }
    fn query_high_complexity_symbols(
        &self,
        s: &str,
        t: u64,
        l: usize,
    ) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError> {
        self.0.query_high_complexity_symbols(s, t, l)
    }
    fn has_complexity_measurements(&self, s: &str) -> Result<bool, AgentStorageError> {
        self.0.has_complexity_measurements(s)
    }
    fn count_high_complexity_symbols(&self, s: &str, t: u64) -> Result<u64, AgentStorageError> {
        self.0.count_high_complexity_symbols(s, t)
    }
    fn get_module_summary(&self, s: &str) -> Result<Option<AgentModuleSummary>, AgentStorageError> {
        self.0.get_module_summary(s)
    }
    fn get_boundary_links_freshness(
        &self,
        s: &str,
    ) -> Result<AgentBoundaryLinksFreshness, AgentStorageError> {
        self.0.get_boundary_links_freshness(s)
    }
}

impl<S: GateStorageRead + ?Sized> GateStorageRead for PartialSpy<'_, S> {
    fn get_active_requirements(&self, r: &str) -> Result<Vec<GateRequirement>, GateStorageError> {
        self.0.get_active_requirements(r)
    }
    fn get_boundary_declarations(
        &self,
        r: &str,
    ) -> Result<Vec<GateBoundaryDeclaration>, GateStorageError> {
        self.0.get_boundary_declarations(r)
    }
    fn find_boundary_imports(
        &self,
        s: &str,
        a: &str,
        b: &str,
    ) -> Result<Vec<GateImportEdge>, GateStorageError> {
        self.0.find_boundary_imports(s, a, b)
    }
    fn get_coverage_measurements(&self, s: &str) -> Result<Vec<GateMeasurement>, GateStorageError> {
        self.0.get_coverage_measurements(s)
    }
    fn get_complexity_measurements(
        &self,
        s: &str,
    ) -> Result<Vec<GateMeasurement>, GateStorageError> {
        self.0.get_complexity_measurements(s)
    }
    fn get_hotspot_inferences(&self, s: &str) -> Result<Vec<GateInference>, GateStorageError> {
        self.0.get_hotspot_inferences(s)
    }
    fn find_waivers(
        &self,
        r: &str,
        i: &str,
        v: i64,
        o: &str,
        n: &str,
    ) -> Result<Vec<GateWaiver>, GateStorageError> {
        self.0.find_waivers(r, i, v, o, n)
    }
    fn evaluate_module_violations(
        &self,
        r: &str,
        s: &str,
    ) -> Result<GateModuleViolationEvidence, GateStorageError> {
        self.0.evaluate_module_violations(r, s)
    }
    fn get_quality_assessment_facts_for_gate(
        &self,
        r: &str,
        s: &str,
    ) -> Result<Vec<GateQualityAssessmentFact>, GateStorageError> {
        self.0.get_quality_assessment_facts_for_gate(r, s)
    }
}

#[test]
fn no_eager_b_read_symbol_focus_serves_from_livegraph() {
    let f = test_fixture::build_fixture(false);
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    assert!(orient_bounded_cert_is_green(&f.state, &f.snapshot_uid));

    let spy = PartialSpy(&storage);
    let epoch = green_epoch(&f.state, &f.snapshot_uid);
    let decorator = OrientServeDecorator::new(&f.state.livegraph, &spy, &epoch);

    let focus = test_fixture::callee_key();
    // If ANY (b) leaf (focus resolution / callers / callees) were read from SQLite, the spy PANICS and
    // this test fails. Completing the symbol-focus orient proves the (b) leaves were served from the
    // LiveGraph with ZERO eager `nodes`/`edges` reads; the (c) trust read + cycles are delegated (OK).
    let result = repo_graph_agent::orient(
        &decorator,
        test_fixture::REPO,
        Some(focus.as_str()),
        repo_graph_agent::Budget::Small,
        "2026-01-01T00:00:00Z",
    )
    .expect("orient over the partial spy completes without an eager (b) read");

    assert!(
        result
            .signals
            .iter()
            .any(|s| s.code() == repo_graph_agent::SignalCode::CallersSummary),
        "the CALLERS_SUMMARY served from the LiveGraph is present"
    );
}

// ── review-1 item 1 / review-2 change 1: the FULL served path's LABEL is also zero per-call read ──

/// The COMBINED value+label served path — the exact `handle_orient` GREEN sequence: bounded-cert
/// PRECHECK -> `orient` through the `OrientServeDecorator` (value serve) -> `build_orient_envelope`
/// (label assembly) — performs ZERO per-call SQLite `find_symbol_callers`/`find_symbol_callees` read in
/// `build_orient_envelope`'s callgraph LABEL path. This is the regression guard review-1 item 1 asked
/// for: the V2 spy above proves the DECORATOR's value serve is zero-read, but `build_orient_envelope`
/// re-derives the callers/callees LEAF LABEL via `orient_*_outcome_served` (serve_from_lg = true here), which
/// must ALSO peek the callgraph cert and not re-compare per call.
///
/// `RepoState.storage` is a CONCRETE `StorageConnection`, so the panicking `PartialSpy` cannot be
/// injected into `build_orient_envelope` (it reads `repo_state.storage` directly, not a generic port).
/// The proof here is the MUTATE-AFTER-CERT mechanism: after the precheck caches the callgraph cert
/// GREEN, DELETE the SQLite `CALLS` edge so the SQLite callgraph now DIVERGES from the LiveGraph in BOTH
/// directions (`find_symbol_callers(calleeFn)` and `find_symbol_callees(callerFn)` go empty). The cached
/// GREEN cert SURVIVES the delete — its fingerprint is the LiveGraph partitions + snapshot, which a
/// SQLite-only mutation does not touch — so a cert PEEK still licenses `livegraph`; a per-call SQLite
/// compare would instead read the now-empty SQLite row set, diverge, and flip the leaf to a
/// `LiveGraphCallgraphDivergence` SQLite fallback. So a `livegraph` leaf == "the label path peeked the
/// cached cert; it did NOT re-read SQLite per call". A regression to the per-call gate fails this test.
#[test]
fn served_path_build_envelope_callgraph_label_zero_per_call_read() {
    let f = test_fixture::build_fixture(false);
    // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    let callee = test_fixture::callee_key();
    let caller = test_fixture::caller_key();
    let now = "2026-01-01T00:00:00Z";

    // 1. PRECHECK (handle_orient step 1): builds + caches the callgraph cert GREEN.
    assert!(
        orient_bounded_cert_is_green(&f.state, &f.snapshot_uid),
        "faithful fixture -> bounded cert GREEN; handle_orient takes the served path"
    );

    // 2. orient through the decorator (handle_orient step 2): the value serve. calleeFn HAS a caller
    //    (-> CALLERS_SUMMARY); callerFn HAS a callee (-> CALLEES_SUMMARY). Both served from the LiveGraph.
    let epoch = green_epoch(&f.state, &f.snapshot_uid);
    let serve = |focus: &str| {
        let decorator = OrientServeDecorator::new(&f.state.livegraph, &storage, &epoch);
        repo_graph_agent::orient(
            &decorator,
            test_fixture::REPO,
            Some(focus),
            repo_graph_agent::Budget::Small,
            now,
        )
        .expect("decorator orient ok")
    };
    let callers_result = serve(callee.as_str());
    let callees_result = serve(caller.as_str());

    // 3. MUTATE-AFTER-CERT: drop the SQLite `CALLS` edge ("ec0", from the fixture). The LiveGraph is
    //    untouched, so the cached cert fingerprint is unchanged -> the cert stays GREEN, but a per-call
    //    SQLite callers/callees compare would now see an empty row set and DIVERGE.
    storage
        .delete_edges_by_uids(&["ec0".to_string()])
        .expect("delete the SQLite CALLS edge");
    // Not vacuous: the SQLite callgraph genuinely diverges from the LiveGraph in BOTH directions now.
    assert!(
        storage
            .find_symbol_callers(&f.snapshot_uid, &callee)
            .expect("find_symbol_callers ok")
            .is_empty(),
        "post-mutation SQLite has NO caller for calleeFn -> a per-call callers compare WOULD fall back"
    );
    assert!(
        storage
            .find_symbol_callees(&f.snapshot_uid, &caller)
            .expect("find_symbol_callees ok")
            .is_empty(),
        "post-mutation SQLite has NO callee for callerFn -> a per-call callees compare WOULD fall back"
    );
    // The cached callgraph cert survives the SQLite-only mutation (fingerprint = LiveGraph + snapshot).
    assert!(
        crate::callgraph_cert::callgraph_cached_green(&f.state, &f.snapshot_uid),
        "cached callgraph cert is still GREEN after a SQLite-only edge delete"
    );

    // 4. build_orient_envelope (handle_orient step 3): the LABEL assembly. On a cached GREEN cert it PEEKS
    //    the cert; the callgraph leaves must label `livegraph` DESPITE the divergent SQLite -> ZERO per-call
    //    `find_symbol_callers`/`find_symbol_callees` read in the label path.
    assert_callgraph_leaf_livegraph(
        &f.state,
        callers_result,
        repo_graph_agent::SignalCode::CallersSummary,
    );
    assert_callgraph_leaf_livegraph(
        &f.state,
        callees_result,
        repo_graph_agent::SignalCode::CalleesSummary,
    );
}

/// Assemble `result` into the orient envelope and assert its `code` leaf is single-`livegraph`-labelled
/// with no fallback reason — the build_orient_envelope LABEL-path zero-read assertion (caller mutated the
/// SQLite callgraph to diverge, so a per-call compare would have produced a SqliteFallback label).
fn assert_callgraph_leaf_livegraph(
    state: &crate::state::RepoState,
    result: repo_graph_agent::OrientResult,
    code: repo_graph_agent::SignalCode,
) {
    // serve_from_lg = true: the caller (`served_path_build_envelope_callgraph_label_zero_per_call_read`)
    // models the SERVED path — the bounded precheck cached the callgraph cert GREEN, so the label peeks it.
    let env =
        crate::orient_coherence::build_orient_envelope(state, test_fixture::REPO, result, true);
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == code)
        .unwrap_or_else(|| panic!("{code:?} leaf present"));
    assert!(
        leaf.provenance
            .source
            .contains(&repo_graph_coherence::Source::Livegraph),
        "{code:?}: GREEN callgraph cert PEEK -> livegraph label with ZERO per-call SQLite read; a per-call \
         compare against the mutated (divergent) SQLite would have produced a SqliteFallback label"
    );
    assert!(
        leaf.provenance.fallback_reason.is_none(),
        "{code:?}: the served callgraph leaf carries no fallback reason on the green cert-peek path"
    );
}

// ── review-3 item 1: RED-fallback provenance — serve_from_lg gates the callgraph leaf LABEL ──────────

/// Seed the focus-resolution no-loss cert RED at the LIVE fingerprint. This isolates the BOUNDED-cert-RED
/// wiring without constructing a divergent focus-resolution fixture (the genuine focus-resolution compare
/// is unit-tested in `focus_resolution_cert`). A cached RED cert at the matching fingerprint makes
/// `focus_resolution_is_green` return false WITHOUT a rebuild — so the bounded cert (focus-res ∧ callgraph)
/// goes RED while the callgraph cert stays independently GREEN.
fn seed_focus_resolution_cert_red(state: &crate::state::RepoState, snapshot_uid: &str) {
    let fp = {
        let guard = state.livegraph.read();
        let lg = guard.as_ref().expect("livegraph set");
        crate::livegraph_feed::import_cert_fingerprint(&lg.live_partitions(), snapshot_uid)
    };
    *state.focus_resolution_cert.write() =
        Some(crate::focus_resolution_cert::FocusResolutionNoLossCert {
            verdict: "RED".to_string(),
            fingerprint: fp,
        });
}

/// The false-provenance regression guard (review-3 item 1). Constructs the reviewer's EXACT scenario —
/// focus-resolution cert RED ∧ callgraph parity GREEN — so the BOUNDED orient cert is RED and
/// `handle_orient` falls back to the BARE SQLite read (`serve_from_lg == false`). Even though the callgraph
/// cert is independently GREEN (a cert peek WOULD re-certify `livegraph`), the CALLERS_SUMMARY leaf MUST be
/// SQLite-LABELLED, because the value was SQLite-sourced THIS call. Proves the leaf provenance follows the
/// ACTUAL serve decision (`serve_from_lg`), not the callgraph cert state alone — the fix `build_orient_envelope`
/// must not "infer serving from the cached cert state alone".
///
/// NON-VACUITY: before forcing the fallback the test asserts the callgraph cert is cached GREEN, so a
/// regression that peeked the cert (ignoring `serve_from_lg`) WOULD mint a false `livegraph` here and fail.
#[test]
fn red_bounded_cert_labels_callgraph_sqlite_despite_green_callgraph_cert() {
    let f = test_fixture::build_fixture(false); // faithful mirror: BOTH sub-certs would be green
                                                // D-S = S-A: one per-op connection for this test (was the `repo_state.storage` field).
    let storage = f.state.storage().unwrap();
    let callee = test_fixture::callee_key();
    let now = "2026-01-01T00:00:00Z";

    // 1. Build the callgraph cert GREEN (callgraph parity holds on the faithful mirror) and confirm it is
    //    cached GREEN — the precondition that makes this a genuine guard (a cert peek would say livegraph).
    assert!(crate::callgraph_cert::callgraph_is_green(
        &f.state,
        &f.snapshot_uid
    ));
    assert!(
        crate::callgraph_cert::callgraph_cached_green(&f.state, &f.snapshot_uid),
        "callgraph parity GREEN + cached — the leaf would re-certify livegraph if the label peeked the cert alone"
    );

    // 2. Force focus-resolution RED -> the BOUNDED cert (focus-res ∧ callgraph) is RED -> handle_orient
    //    would decline the decorator and run the agent over BARE SQLite (serve_from_lg == false).
    seed_focus_resolution_cert_red(&f.state, &f.snapshot_uid);
    assert!(
        !orient_bounded_cert_is_green(&f.state, &f.snapshot_uid),
        "focus-resolution RED ∧ callgraph GREEN -> bounded cert RED -> serve_from_lg = false"
    );

    // 3. dispatch's serve_from_lg == false branch: build the OrientResult from the BARE SQLite storage
    //    (exactly what handle_orient runs when it declines the decorator), then assemble the envelope with
    //    serve_from_lg = false.
    let result = repo_graph_agent::orient(
        &storage,
        test_fixture::REPO,
        Some(callee.as_str()),
        repo_graph_agent::Budget::Small,
        now,
    )
    .expect("bare sqlite orient ok");
    assert!(
        result
            .signals
            .iter()
            .any(|s| s.code() == repo_graph_agent::SignalCode::CallersSummary),
        "symbol focus emits CALLERS_SUMMARY (SQLite-sourced this call)"
    );
    let env = crate::orient_coherence::build_orient_envelope(
        &f.state,
        test_fixture::REPO,
        result,
        false, // serve_from_lg: dispatch fell back to bare SQLite (bounded cert RED)
    );

    // 4. The callgraph leaf MUST be SQLite — NOT livegraph — DESPITE the GREEN callgraph cert.
    let leaf = env
        .value
        .signals
        .iter()
        .find(|l| l.value.code() == repo_graph_agent::SignalCode::CallersSummary)
        .expect("callers leaf present");
    assert_eq!(
        leaf.provenance.source,
        std::collections::BTreeSet::from([repo_graph_coherence::Source::Sqlite]),
        "serve_from_lg == false -> CALLERS_SUMMARY is SQLite-sourced (the bare read), never livegraph"
    );
    assert!(
        !leaf.provenance
            .source
            .contains(&repo_graph_coherence::Source::Livegraph),
        "no false livegraph provenance on the bare-SQLite fallback path despite the GREEN callgraph cert"
    );
    assert_eq!(
        leaf.provenance.fallback_reason,
        Some(repo_graph_coherence::CoherenceFallbackReason::LiveGraphBoundedServeDeclined),
        "the leaf carries the honest bounded-serve-declined reason"
    );
}
