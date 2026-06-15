//! COHERENCE-LEAF-SERVE-IMPL-1: the `AgentStorageRead` + `GateStorageRead` impls for the orient serve
//! decorator (split from `orient_serve::mod` per the 500-line structural guardrail; mirrors the
//! focus-resolution producer's type/test split).
//!
//! The six (b) methods (focus resolution + callers/callees) SERVE from the LiveGraph when the answer is
//! `Exact`, else DELEGATE to the inner SQLite port; every other read DELEGATES verbatim. The serve
//! helpers ([`super::map_path_resolution`] etc., [`crate::callgraph_cert::lg_caller_rows`]) live beside
//! the cert that proved them no-loss, so "what the cert proved" and "what is served" are the same bytes.

use repo_graph_agent::{
    AgentBoundaryDeclaration, AgentBoundaryLinksFreshness, AgentCalleeRow, AgentCallerRow,
    AgentComplexityMeasurement, AgentCycle, AgentDeadNode, AgentDocEntry, AgentFileEntry,
    AgentFocusCandidate, AgentImportEdge, AgentImportEntry, AgentModuleSummary,
    AgentPathResolution, AgentRepo, AgentRepoSummary, AgentSnapshot, AgentStaleFile,
    AgentStorageError, AgentStorageRead, AgentSymbolContext, AgentSymbolEntry, AgentTrustSummary,
};
use repo_graph_gate::{
    GateBoundaryDeclaration, GateImportEdge, GateInference, GateMeasurement,
    GateModuleViolationEvidence, GateQualityAssessmentFact, GateRequirement, GateStorageError,
    GateStorageRead, GateWaiver,
};
use repo_graph_trust_model::AnswerClass;

use super::{map_candidate, map_path_resolution, map_symbol_context, OrientServeDecorator};
use crate::callgraph_cert::{lg_callee_rows, lg_caller_rows};

impl<S: AgentStorageRead + GateStorageRead + ?Sized> AgentStorageRead
    for OrientServeDecorator<'_, S>
{
    // ── (b) SERVED from the LiveGraph on green (Exact); else DELEGATED ──────────────────────────────

    fn resolve_path_focus(
        &self,
        snapshot_uid: &str,
        path: &str,
    ) -> Result<AgentPathResolution, AgentStorageError> {
        {
            let guard = self.livegraph.read();
            if let Some(lg) = guard.as_ref() {
                let env = lg.resolve_path(path);
                if env.class() == AnswerClass::Exact {
                    if let Some(d) = env.data() {
                        return Ok(map_path_resolution(d));
                    }
                }
            }
        }
        self.inner.resolve_path_focus(snapshot_uid, path)
    }

    fn resolve_stable_key_focus(
        &self,
        snapshot_uid: &str,
        stable_key: &str,
    ) -> Result<Option<AgentFocusCandidate>, AgentStorageError> {
        {
            let guard = self.livegraph.read();
            if let Some(lg) = guard.as_ref() {
                let env = lg.resolve_stable_key(stable_key);
                if env.class() == AnswerClass::Exact {
                    if let Some(d) = env.data() {
                        return Ok(d.as_ref().map(map_candidate));
                    }
                }
            }
        }
        self.inner
            .resolve_stable_key_focus(snapshot_uid, stable_key)
    }

    fn resolve_symbol_name(
        &self,
        snapshot_uid: &str,
        name: &str,
    ) -> Result<Vec<AgentFocusCandidate>, AgentStorageError> {
        {
            let guard = self.livegraph.read();
            if let Some(lg) = guard.as_ref() {
                let env = lg.resolve_symbol_name(name);
                if env.class() == AnswerClass::Exact {
                    if let Some(d) = env.data() {
                        return Ok(d.iter().map(map_candidate).collect());
                    }
                }
            }
        }
        self.inner.resolve_symbol_name(snapshot_uid, name)
    }

    fn get_symbol_context(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Option<AgentSymbolContext>, AgentStorageError> {
        {
            let guard = self.livegraph.read();
            if let Some(lg) = guard.as_ref() {
                let env = lg.symbol_context(symbol_stable_key);
                if env.class() == AnswerClass::Exact {
                    if let Some(d) = env.data() {
                        return Ok(d.as_ref().map(map_symbol_context));
                    }
                }
            }
        }
        self.inner
            .get_symbol_context(snapshot_uid, symbol_stable_key)
    }

    fn find_symbol_callers(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Vec<AgentCallerRow>, AgentStorageError> {
        {
            let guard = self.livegraph.read();
            if let Some(lg) = guard.as_ref() {
                if let Some(rows) = lg_caller_rows(lg, symbol_stable_key) {
                    return Ok(rows);
                }
            }
        }
        self.inner
            .find_symbol_callers(snapshot_uid, symbol_stable_key)
    }

    fn find_symbol_callees(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Vec<AgentCalleeRow>, AgentStorageError> {
        {
            let guard = self.livegraph.read();
            if let Some(lg) = guard.as_ref() {
                if let Some(rows) = lg_callee_rows(lg, symbol_stable_key) {
                    return Ok(rows);
                }
            }
        }
        self.inner
            .find_symbol_callees(snapshot_uid, symbol_stable_key)
    }

    // ── DELEGATED to SQLite (the (c) trust contributor, MODULE_SUMMARY, cycles, Authority, FS, …) ────

    fn get_repo(&self, repo_uid: &str) -> Result<Option<AgentRepo>, AgentStorageError> {
        self.inner.get_repo(repo_uid)
    }

    fn get_latest_snapshot(
        &self,
        repo_uid: &str,
    ) -> Result<Option<AgentSnapshot>, AgentStorageError> {
        self.inner.get_latest_snapshot(repo_uid)
    }

    fn get_stale_files(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<AgentStaleFile>, AgentStorageError> {
        self.inner.get_stale_files(snapshot_uid)
    }

    fn find_module_cycles(&self, snapshot_uid: &str) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.inner.find_module_cycles(snapshot_uid)
    }

    fn find_dead_nodes(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        kind_filter: Option<&str>,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.inner
            .find_dead_nodes(snapshot_uid, repo_uid, kind_filter)
    }

    fn get_active_boundary_declarations(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
        self.inner.get_active_boundary_declarations(repo_uid)
    }

    fn find_imports_between_paths(
        &self,
        snapshot_uid: &str,
        source_prefix: &str,
        target_prefix: &str,
    ) -> Result<Vec<AgentImportEdge>, AgentStorageError> {
        self.inner
            .find_imports_between_paths(snapshot_uid, source_prefix, target_prefix)
    }

    fn compute_repo_summary(
        &self,
        snapshot_uid: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        self.inner.compute_repo_summary(snapshot_uid)
    }

    fn get_trust_summary(
        &self,
        repo_uid: &str,
        snapshot_uid: &str,
    ) -> Result<AgentTrustSummary, AgentStorageError> {
        self.inner.get_trust_summary(repo_uid, snapshot_uid)
    }

    fn find_dead_nodes_in_path(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.inner
            .find_dead_nodes_in_path(snapshot_uid, repo_uid, path_prefix)
    }

    fn find_dead_nodes_in_file(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        self.inner
            .find_dead_nodes_in_file(snapshot_uid, repo_uid, file_path)
    }

    fn compute_path_summary(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        self.inner.compute_path_summary(snapshot_uid, path_prefix)
    }

    fn compute_file_summary(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        self.inner.compute_file_summary(snapshot_uid, file_path)
    }

    fn find_boundary_declarations_in_path(
        &self,
        repo_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
        self.inner
            .find_boundary_declarations_in_path(repo_uid, path_prefix)
    }

    fn find_cycles_involving_path(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.inner
            .find_cycles_involving_path(snapshot_uid, path_prefix)
    }

    fn find_cycles_involving_module(
        &self,
        snapshot_uid: &str,
        module_qualified_name: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        self.inner
            .find_cycles_involving_module(snapshot_uid, module_qualified_name)
    }

    fn list_symbols_in_file(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentSymbolEntry>, AgentStorageError> {
        self.inner.list_symbols_in_file(snapshot_uid, file_path)
    }

    fn list_files_in_path(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentFileEntry>, AgentStorageError> {
        self.inner.list_files_in_path(snapshot_uid, path_prefix)
    }

    fn find_file_imports(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentImportEntry>, AgentStorageError> {
        self.inner.find_file_imports(snapshot_uid, file_path)
    }

    fn get_doc_inventory(&self, repo_uid: &str) -> Result<Vec<AgentDocEntry>, AgentStorageError> {
        self.inner.get_doc_inventory(repo_uid)
    }

    fn query_high_complexity_symbols(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
        limit: usize,
    ) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError> {
        self.inner
            .query_high_complexity_symbols(snapshot_uid, min_threshold, limit)
    }

    fn has_complexity_measurements(&self, snapshot_uid: &str) -> Result<bool, AgentStorageError> {
        self.inner.has_complexity_measurements(snapshot_uid)
    }

    fn count_high_complexity_symbols(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
    ) -> Result<u64, AgentStorageError> {
        self.inner
            .count_high_complexity_symbols(snapshot_uid, min_threshold)
    }

    fn get_module_summary(
        &self,
        snapshot_uid: &str,
    ) -> Result<Option<AgentModuleSummary>, AgentStorageError> {
        self.inner.get_module_summary(snapshot_uid)
    }

    fn get_boundary_links_freshness(
        &self,
        snapshot_uid: &str,
    ) -> Result<AgentBoundaryLinksFreshness, AgentStorageError> {
        self.inner.get_boundary_links_freshness(snapshot_uid)
    }
}

impl<S: AgentStorageRead + GateStorageRead + ?Sized> GateStorageRead
    for OrientServeDecorator<'_, S>
{
    // Gate reads are Authority — NEVER served from the LiveGraph (no `nodes`/`edges` home by
    // construction). Delegate every method to the inner SQLite port.

    fn get_active_requirements(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<GateRequirement>, GateStorageError> {
        self.inner.get_active_requirements(repo_uid)
    }

    fn get_boundary_declarations(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<GateBoundaryDeclaration>, GateStorageError> {
        self.inner.get_boundary_declarations(repo_uid)
    }

    fn find_boundary_imports(
        &self,
        snapshot_uid: &str,
        source_prefix: &str,
        target_prefix: &str,
    ) -> Result<Vec<GateImportEdge>, GateStorageError> {
        self.inner
            .find_boundary_imports(snapshot_uid, source_prefix, target_prefix)
    }

    fn get_coverage_measurements(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<GateMeasurement>, GateStorageError> {
        self.inner.get_coverage_measurements(snapshot_uid)
    }

    fn get_complexity_measurements(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<GateMeasurement>, GateStorageError> {
        self.inner.get_complexity_measurements(snapshot_uid)
    }

    fn get_hotspot_inferences(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<GateInference>, GateStorageError> {
        self.inner.get_hotspot_inferences(snapshot_uid)
    }

    fn find_waivers(
        &self,
        repo_uid: &str,
        req_id: &str,
        req_version: i64,
        obligation_id: &str,
        now: &str,
    ) -> Result<Vec<GateWaiver>, GateStorageError> {
        self.inner
            .find_waivers(repo_uid, req_id, req_version, obligation_id, now)
    }

    fn evaluate_module_violations(
        &self,
        repo_uid: &str,
        snapshot_uid: &str,
    ) -> Result<GateModuleViolationEvidence, GateStorageError> {
        self.inner
            .evaluate_module_violations(repo_uid, snapshot_uid)
    }

    fn get_quality_assessment_facts_for_gate(
        &self,
        repo_uid: &str,
        snapshot_uid: &str,
    ) -> Result<Vec<GateQualityAssessmentFact>, GateStorageError> {
        self.inner
            .get_quality_assessment_facts_for_gate(repo_uid, snapshot_uid)
    }
}
