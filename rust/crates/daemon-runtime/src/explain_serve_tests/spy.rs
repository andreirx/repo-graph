//! COHERENCE-LEAF-SERVE-IMPL-2: the configurable PARTIAL storage spy for explain's serve proofs (split out
//! of the test module per the 500-line structural guardrail).
//!
//! A spy over the real SQLite storage. `panicking()` PANICS on the six decorator-served (b) methods — the
//! no-eager-`nodes`-read proof: on green they are served from the LiveGraph, so the panics never fire.
//! SYMBOL-IDENTITY-1 (ruling B): explain's SHARED symbol resolver `resolve_symbol` is the ONE
//! exception — it is SQLite-served (delegated, never panics), because the suffix-aware ladder cannot
//! be answered by the name-only LiveGraph resolver. `resolve_symbol_name` stays a panicking (b)
//! method (still LiveGraph-served for orient + the cert); the panicking SYMBOL proofs resolve by
//! stable key and never reach either resolver.
//! `recording()` delegates the served methods but RECORDS the four FILE/PATH summary/listing `nodes` reads
//! plus the two cycle finders (EC-M2): on the pre-M-2 `new()` decorator the summary flags fire (the
//! honest-bound proof); on the M-2-enabled `with_leaf_serves` decorator the summary + cycle flags must
//! stay FALSE (the review-0 #3 explain no-eager-read proof — those reads serve from the LiveGraph).
//! Every other read DELEGATES verbatim. Mirrors `orient_serve::tests::PartialSpy` (kept separate per the
//! orient_serve "no behavior change" scope).

use std::sync::atomic::{AtomicBool, Ordering};

use repo_graph_agent::{
    AgentBoundaryDeclaration, AgentBoundaryLinksFreshness, AgentCalleeRow, AgentCallerRow,
    AgentComplexityMeasurement, AgentCycle, AgentDeadNode, AgentDocEntry, AgentFileEntry,
    AgentFileImporter, AgentFocusCandidate, AgentImportEdge, AgentImportEntry, AgentMemberEntry,
    AgentModuleSummary, AgentPathResolution, AgentRepo, AgentRepoSummary, AgentSnapshot,
    AgentStaleFile, AgentStorageError, AgentStorageRead, AgentSymbolContext, AgentSymbolEntry,
    AgentSymbolResolution, AgentTrustSummary,
};
use repo_graph_gate::{
    GateBoundaryDeclaration, GateImportEdge, GateInference, GateMeasurement,
    GateModuleViolationEvidence, GateQualityAssessmentFact, GateRequirement, GateStorageError,
    GateStorageRead, GateWaiver,
};

pub(super) struct ServeSpy<'a, S: ?Sized> {
    inner: &'a S,
    panic_served: bool,
    /// W-B-EPOCH-IMPL-1: PANIC on `get_latest_snapshot` — the explain-pin proof. Through the epoch-pinned
    /// decorator this is never reached (the decorator returns `epoch.snapshot`), so a completing explain
    /// proves `run_explain` did NOT re-resolve "latest".
    panic_get_latest_snapshot: bool,
    pub(super) read_compute_file_summary: AtomicBool,
    pub(super) read_compute_path_summary: AtomicBool,
    pub(super) read_list_symbols_in_file: AtomicBool,
    pub(super) read_list_files_in_path: AtomicBool,
    /// EC-M2 (review-0 #3): the two explain cycle finders (the cancellable variants funnel here
    /// through the trait defaults) — must stay FALSE through the M-2-enabled decorator.
    pub(super) read_find_cycles_involving_path: AtomicBool,
    pub(super) read_find_cycles_involving_module: AtomicBool,
    /// EXPLAIN-TYPE-SECTIONS-1 (ETS-C04): the two type-focus reads are SQLite-DELEGATED on green (no
    /// LiveGraph answer class), so — unlike the six (b) methods — they DELEGATE here and are RECORDED
    /// as allowed reads. `type_focus_members_and_referenced_by_are_sqlite_delegated_on_green` asserts
    /// both fired (the reads reached SQLite, never a defaulted empty section).
    pub(super) read_list_members_of_type: AtomicBool,
    pub(super) read_find_file_importers: AtomicBool,
}

impl<'a, S: ?Sized> ServeSpy<'a, S> {
    /// PANIC on the six served (b) methods (the no-eager-read proof).
    pub(super) fn panicking(inner: &'a S) -> Self {
        Self::with(inner, true)
    }
    /// DELEGATE the served methods, RECORD the FILE/PATH summary/listing reads (the honest-bound proof).
    pub(super) fn recording(inner: &'a S) -> Self {
        Self::with(inner, false)
    }
    /// Also PANIC on `get_latest_snapshot` (the explain snapshot-pin proof): through the epoch-pinned
    /// decorator this is never reached, so a completing explain proves no mid-request "latest" re-read.
    pub(super) fn panic_on_snapshot_resolve(mut self) -> Self {
        self.panic_get_latest_snapshot = true;
        self
    }
    fn with(inner: &'a S, panic_served: bool) -> Self {
        Self {
            inner,
            panic_served,
            panic_get_latest_snapshot: false,
            read_compute_file_summary: AtomicBool::new(false),
            read_compute_path_summary: AtomicBool::new(false),
            read_list_symbols_in_file: AtomicBool::new(false),
            read_list_files_in_path: AtomicBool::new(false),
            read_find_cycles_involving_path: AtomicBool::new(false),
            read_find_cycles_involving_module: AtomicBool::new(false),
            read_list_members_of_type: AtomicBool::new(false),
            read_find_file_importers: AtomicBool::new(false),
        }
    }
}

impl<S: AgentStorageRead + ?Sized> AgentStorageRead for ServeSpy<'_, S> {
    // ── the SIX served (b) methods: served from the LiveGraph on green, so NEVER reached here ──
    fn resolve_path_focus(
        &self,
        s: &str,
        p: &str,
    ) -> Result<AgentPathResolution, AgentStorageError> {
        if self.panic_served {
            panic!("resolve_path_focus must be served from the LiveGraph on green")
        }
        self.inner.resolve_path_focus(s, p)
    }
    fn resolve_stable_key_focus(
        &self,
        s: &str,
        k: &str,
    ) -> Result<Option<AgentFocusCandidate>, AgentStorageError> {
        if self.panic_served {
            panic!("resolve_stable_key_focus must be served from the LiveGraph on green")
        }
        self.inner.resolve_stable_key_focus(s, k)
    }
    fn resolve_symbol_name(
        &self,
        s: &str,
        n: &str,
    ) -> Result<Vec<AgentFocusCandidate>, AgentStorageError> {
        if self.panic_served {
            panic!("resolve_symbol_name must be served from the LiveGraph on green")
        }
        self.inner.resolve_symbol_name(s, n)
    }
    // SYMBOL-IDENTITY-1 §2.1 (ruling EXPLAIN-RESOLVER-ROUTING = B): explain's SHARED symbol
    // resolution step is now SQLite-served (NOT one of the six LiveGraph-served (b) methods), so
    // it DELEGATES here and never panics — the decorator forwards `resolve_symbol` straight to this
    // inner port. This is the ONE explain resolution-step expectation change ruling B calls for; the
    // REST of the explain SYMBOL pipeline (callers/callees/context) stays decorator/LiveGraph-served
    // and still panics above. The `panicking()` SYMBOL proofs resolve their targets by STABLE KEY
    // (`resolve_stable_key_focus`, still LiveGraph-served), so they never reach this method — the
    // nodes-free-on-green claim for the key/name focus path is unchanged; only a qualified-suffix
    // resolution (this slice's new capability) reads SQLite here, by design.
    fn resolve_symbol(&self, s: &str, q: &str) -> Result<AgentSymbolResolution, AgentStorageError> {
        self.inner.resolve_symbol(s, q)
    }
    fn get_symbol_context(
        &self,
        s: &str,
        k: &str,
    ) -> Result<Option<AgentSymbolContext>, AgentStorageError> {
        if self.panic_served {
            panic!("get_symbol_context must be served from the LiveGraph on green")
        }
        self.inner.get_symbol_context(s, k)
    }
    fn find_symbol_callers(
        &self,
        s: &str,
        k: &str,
    ) -> Result<Vec<AgentCallerRow>, AgentStorageError> {
        if self.panic_served {
            panic!("find_symbol_callers must be served from the LiveGraph on green")
        }
        self.inner.find_symbol_callers(s, k)
    }
    fn find_symbol_callees(
        &self,
        s: &str,
        k: &str,
    ) -> Result<Vec<AgentCalleeRow>, AgentStorageError> {
        if self.panic_served {
            panic!("find_symbol_callees must be served from the LiveGraph on green")
        }
        self.inner.find_symbol_callees(s, k)
    }

    // ── the FOUR FILE/PATH summary/listing `nodes` reads: RECORDED (honest bound) + delegated ──
    fn compute_file_summary(
        &self,
        s: &str,
        p: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        self.read_compute_file_summary
            .store(true, Ordering::Relaxed);
        self.inner.compute_file_summary(s, p)
    }
    fn compute_path_summary(
        &self,
        s: &str,
        p: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        self.read_compute_path_summary
            .store(true, Ordering::Relaxed);
        self.inner.compute_path_summary(s, p)
    }
    fn list_symbols_in_file(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentSymbolEntry>, AgentStorageError> {
        self.read_list_symbols_in_file
            .store(true, Ordering::Relaxed);
        self.inner.list_symbols_in_file(s, p)
    }
    fn list_files_in_path(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentFileEntry>, AgentStorageError> {
        self.read_list_files_in_path.store(true, Ordering::Relaxed);
        self.inner.list_files_in_path(s, p)
    }

    // ── everything else: DELEGATED (allowed reads — the (c) trust, cycles, Authority, FS) ──
    fn get_repo(&self, repo_uid: &str) -> Result<Option<AgentRepo>, AgentStorageError> {
        self.inner.get_repo(repo_uid)
    }
    fn get_latest_snapshot(
        &self,
        repo_uid: &str,
    ) -> Result<Option<AgentSnapshot>, AgentStorageError> {
        if self.panic_get_latest_snapshot {
            panic!(
                "get_latest_snapshot must NOT be re-resolved on the epoch-pinned path: \
                 the decorator returns the captured epoch.snapshot"
            )
        }
        self.inner.get_latest_snapshot(repo_uid)
    }
    fn get_stale_files(&self, s: &str) -> Result<Vec<AgentStaleFile>, AgentStorageError> {
        self.inner.get_stale_files(s)
    }
    fn find_module_cycles(&self, s: &str) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.inner.find_module_cycles(s)
    }
    fn find_dead_nodes(
        &self,
        s: &str,
        r: &str,
        k: Option<&str>,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.inner.find_dead_nodes(s, r, k)
    }
    fn get_active_boundary_declarations(
        &self,
        r: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
        self.inner.get_active_boundary_declarations(r)
    }
    fn find_imports_between_paths(
        &self,
        s: &str,
        a: &str,
        b: &str,
    ) -> Result<Vec<AgentImportEdge>, AgentStorageError> {
        self.inner.find_imports_between_paths(s, a, b)
    }
    fn compute_repo_summary(&self, s: &str) -> Result<AgentRepoSummary, AgentStorageError> {
        self.inner.compute_repo_summary(s)
    }
    fn get_trust_summary(&self, r: &str, s: &str) -> Result<AgentTrustSummary, AgentStorageError> {
        self.inner.get_trust_summary(r, s)
    }
    fn find_dead_nodes_in_path(
        &self,
        s: &str,
        r: &str,
        p: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.inner.find_dead_nodes_in_path(s, r, p)
    }
    fn find_dead_nodes_in_file(
        &self,
        s: &str,
        r: &str,
        p: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.inner.find_dead_nodes_in_file(s, r, p)
    }
    fn find_boundary_declarations_in_path(
        &self,
        r: &str,
        p: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
        self.inner.find_boundary_declarations_in_path(r, p)
    }
    fn find_cycles_involving_path(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.read_find_cycles_involving_path
            .store(true, Ordering::Relaxed);
        self.inner.find_cycles_involving_path(s, p)
    }
    fn find_cycles_involving_module(
        &self,
        s: &str,
        m: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.read_find_cycles_involving_module
            .store(true, Ordering::Relaxed);
        self.inner.find_cycles_involving_module(s, m)
    }
    fn find_file_imports(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentImportEntry>, AgentStorageError> {
        self.inner.find_file_imports(s, p)
    }
    // EXPLAIN-TYPE-SECTIONS-1 (ETS-C04): explicit delegating overrides — SQLite-served (recorded as
    // allowed reads), NEVER panicking like the six (b) methods.
    fn list_members_of_type(
        &self,
        s: &str,
        q: &str,
    ) -> Result<Vec<AgentMemberEntry>, AgentStorageError> {
        self.read_list_members_of_type
            .store(true, Ordering::Relaxed);
        self.inner.list_members_of_type(s, q)
    }
    fn find_file_importers(
        &self,
        s: &str,
        p: &str,
    ) -> Result<Vec<AgentFileImporter>, AgentStorageError> {
        self.read_find_file_importers.store(true, Ordering::Relaxed);
        self.inner.find_file_importers(s, p)
    }
    fn get_doc_inventory(&self, r: &str) -> Result<Vec<AgentDocEntry>, AgentStorageError> {
        self.inner.get_doc_inventory(r)
    }
    fn query_high_complexity_symbols(
        &self,
        s: &str,
        t: u64,
        l: usize,
    ) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError> {
        self.inner.query_high_complexity_symbols(s, t, l)
    }
    fn has_complexity_measurements(&self, s: &str) -> Result<bool, AgentStorageError> {
        self.inner.has_complexity_measurements(s)
    }
    fn count_high_complexity_symbols(&self, s: &str, t: u64) -> Result<u64, AgentStorageError> {
        self.inner.count_high_complexity_symbols(s, t)
    }
    fn get_module_summary(&self, s: &str) -> Result<Option<AgentModuleSummary>, AgentStorageError> {
        self.inner.get_module_summary(s)
    }
    fn get_boundary_links_freshness(
        &self,
        s: &str,
    ) -> Result<AgentBoundaryLinksFreshness, AgentStorageError> {
        self.inner.get_boundary_links_freshness(s)
    }
}

impl<S: GateStorageRead + ?Sized> GateStorageRead for ServeSpy<'_, S> {
    fn get_active_requirements(&self, r: &str) -> Result<Vec<GateRequirement>, GateStorageError> {
        self.inner.get_active_requirements(r)
    }
    fn get_boundary_declarations(
        &self,
        r: &str,
    ) -> Result<Vec<GateBoundaryDeclaration>, GateStorageError> {
        self.inner.get_boundary_declarations(r)
    }
    fn find_boundary_imports(
        &self,
        s: &str,
        a: &str,
        b: &str,
    ) -> Result<Vec<GateImportEdge>, GateStorageError> {
        self.inner.find_boundary_imports(s, a, b)
    }
    fn get_coverage_measurements(&self, s: &str) -> Result<Vec<GateMeasurement>, GateStorageError> {
        self.inner.get_coverage_measurements(s)
    }
    fn get_complexity_measurements(
        &self,
        s: &str,
    ) -> Result<Vec<GateMeasurement>, GateStorageError> {
        self.inner.get_complexity_measurements(s)
    }
    fn get_hotspot_inferences(&self, s: &str) -> Result<Vec<GateInference>, GateStorageError> {
        self.inner.get_hotspot_inferences(s)
    }
    fn find_waivers(
        &self,
        r: &str,
        i: &str,
        v: i64,
        o: &str,
        n: &str,
    ) -> Result<Vec<GateWaiver>, GateStorageError> {
        self.inner.find_waivers(r, i, v, o, n)
    }
    fn evaluate_module_violations(
        &self,
        r: &str,
        s: &str,
    ) -> Result<GateModuleViolationEvidence, GateStorageError> {
        self.inner.evaluate_module_violations(r, s)
    }
    fn get_quality_assessment_facts_for_gate(
        &self,
        r: &str,
        s: &str,
    ) -> Result<Vec<GateQualityAssessmentFact>, GateStorageError> {
        self.inner.get_quality_assessment_facts_for_gate(r, s)
    }
}
