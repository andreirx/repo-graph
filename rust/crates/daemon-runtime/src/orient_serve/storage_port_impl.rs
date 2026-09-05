//! COHERENCE-LEAF-SERVE-IMPL-1: the `AgentStorageRead` + `GateStorageRead` impls for the orient serve
//! decorator (split from `orient_serve::mod` per the 500-line structural guardrail; mirrors the
//! focus-resolution producer's type/test split).
//!
//! The six (b) methods (focus resolution + callers/callees) SERVE from the LiveGraph when the captured
//! BOUNDED decision is on (`bounded_epoch_resident` — review-0 #1: independent of the M-2 leaf flags)
//! and the answer is `Exact`, else DELEGATE to the inner SQLite port; every other read DELEGATES
//! verbatim. The serve helpers ([`super::map_path_resolution`] etc.,
//! [`crate::callgraph_cert::lg_caller_rows`]) live beside the cert that proved them no-loss, so "what
//! the cert proved" and "what is served" are the same bytes.

use repo_graph_agent::{
    AgentBoundaryDeclaration, AgentBoundaryLinksFreshness, AgentCalleeRow, AgentCallerRow,
    AgentCancelCheck, AgentComplexityMeasurement, AgentCycle, AgentDeadNode, AgentDirectoryGroup,
    AgentDocEntry, AgentFileEntry, AgentFocusCandidate, AgentImportEdge, AgentImportEntry,
    AgentModuleSize, AgentModuleSummary, AgentPathResolution, AgentRepo, AgentRepoSummary,
    AgentSnapshot, AgentStaleFile, AgentStorageError, AgentStorageRead, AgentSymbolContext,
    AgentSymbolEntry, AgentTrustSummary, ManifestRoot,
};
use repo_graph_gate::{
    GateBoundaryDeclaration, GateImportEdge, GateInference, GateMeasurement,
    GateModuleViolationEvidence, GateQualityAssessmentFact, GateRequirement, GateStorageError,
    GateStorageRead, GateWaiver,
};
use repo_graph_trust_model::AnswerClass;

use super::{map_candidate, map_path_resolution, map_symbol_context, OrientServeDecorator};
use crate::callgraph_cert::{lg_callee_rows, lg_caller_rows};

// ── EC-M2-LEAF-SERVE-1: the M-2 leaf gates + value mappers (cycle VALUES / MODULE_SUMMARY) ───────

impl<S: AgentStorageRead + GateStorageRead + ?Sized> OrientServeDecorator<'_, S> {
    /// MODULE_SUMMARY gate: `Some(inventory)` iff the captured witness said the module-summary cert
    /// is GREEN (`m2.module_summary`), the epoch is still resident (EV-A), and the inventory answer
    /// is `Exact`. `None` ⇒ the summary methods delegate to the pinned SQLite snapshot.
    fn m2_summary_inventory(&self) -> Option<repo_graph_livegraph::StructuralInventoryAnswer> {
        if !self.m2.module_summary {
            return None;
        }
        let guard = self.livegraph.read();
        let lg = guard.as_ref()?;
        if !self.epoch_resident(lg) {
            return None;
        }
        let env = lg.structural_file_inventory();
        if env.class() != AnswerClass::Exact {
            return None;
        }
        env.data().cloned()
    }

    /// Cycle-VALUES gate (non-cancellable): `Some(qualified member lists)` iff the captured witness
    /// said the cycles cert's VALUES verdict is GREEN (`m2.cycle_values`), the epoch is still
    /// resident, and the module-cycle answer is `Exact`. `None` ⇒ delegate to SQLite.
    fn m2_module_cycles(&self) -> Option<Vec<Vec<String>>> {
        if !self.m2.cycle_values {
            return None;
        }
        let guard = self.livegraph.read();
        let lg = guard.as_ref()?;
        if !self.epoch_resident(lg) {
            return None;
        }
        let env = lg.module_import_cycles();
        if env.class() != AnswerClass::Exact {
            return None;
        }
        env.data()
            .map(|d| d.cycles.iter().map(|c| c.members.clone()).collect())
    }

    /// The cancellable sibling of [`Self::m2_module_cycles`] — threads the cooperative checkpoint
    /// into the LiveGraph SCC (DAEMON-CANCEL-1 discipline holds on the M-2 serve path too).
    /// `Err` = the peer disconnected mid-traversal (mapped to the standard cancelled storage error);
    /// `Ok(None)` = not servable ⇒ delegate.
    fn m2_module_cycles_cancellable(
        &self,
        method: &'static str,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Option<Vec<Vec<String>>>, AgentStorageError> {
        if !self.m2.cycle_values {
            return Ok(None);
        }
        let guard = self.livegraph.read();
        let Some(lg) = guard.as_ref() else {
            return Ok(None);
        };
        if !self.epoch_resident(lg) {
            return Ok(None);
        }
        let env = lg.module_import_cycles_cancellable(cancel).map_err(|_| {
            AgentStorageError::new(
                method,
                "cancelled (client disconnected during cycle computation)",
            )
        })?;
        if env.class() != AnswerClass::Exact {
            return Ok(None);
        }
        Ok(env
            .data()
            .map(|d| d.cycles.iter().map(|c| c.members.clone()).collect()))
    }
}

/// Map LiveGraph module cycles (qualified dirname members) to the REPO-level agent shape: SHORT
/// (basename) names, mirroring SQLite `find_module_cycles`' `CycleNode.name` render. The cycle
/// VALUES cert proved this exact canonical shape byte-equal (`values_verdict`); downstream the
/// agent's `canonicalize_cycles` makes the order a pure function of the set on both engines.
fn cycles_repo_shape(cycles: &[Vec<String>]) -> Vec<AgentCycle> {
    cycles
        .iter()
        .map(|members| AgentCycle {
            length: members.len(),
            modules: members
                .iter()
                .map(|m| crate::cycle_output::module_basename(m).to_string())
                .collect(),
            // ORIENT-CYCLES-DISAGREE-1: the LiveGraph module-cycle serve cannot reach the
            // stored `is_test` fact (FIXTURE-POLLUTION-1 §2.3 asymmetry), so it claims NO
            // test-only split — orient's headline then falls back to the raw total, matching
            // `cycles` on this same LiveGraph-served path.
            test_composition: None,
            type_only: None,
            // COHERENCE-3: the LiveGraph/focus serve cannot reach the intra-SCC edges — no walk
            // precomputed; orient renders the unordered form ("largest: N modules — rmap cycles").
            walk: None,
        })
        .collect()
}

/// Map + FILTER LiveGraph module cycles to the QUALIFIED agent shape of
/// `find_cycles_involving_path` (prefix scope: member == prefix or under `prefix/`) or
/// `find_cycles_involving_module` (exact membership) — the same predicates the SQLite
/// implementations apply to their qualified names.
fn cycles_qualified_filtered(
    cycles: &[Vec<String>],
    target: &str,
    prefix_scope: bool,
) -> Vec<AgentCycle> {
    let prefix = format!("{target}/");
    cycles
        .iter()
        .filter(|members| {
            members.iter().any(|m| {
                if prefix_scope {
                    m == target || m.starts_with(&prefix)
                } else {
                    m == target
                }
            })
        })
        .map(|members| AgentCycle {
            length: members.len(),
            modules: members.clone(),
            // ORIENT-CYCLES-DISAGREE-1: focus/path-scoped LiveGraph serve — no is_test reach
            // (§2.3) and not the repo headline; no test-only split claimed.
            test_composition: None,
            type_only: None,
            // COHERENCE-3: the LiveGraph/focus serve cannot reach the intra-SCC edges — no walk
            // precomputed; orient renders the unordered form ("largest: N modules — rmap cycles").
            walk: None,
        })
        .collect()
}

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
                if self.bounded_epoch_resident(lg) {
                    let env = lg.resolve_path(path);
                    if env.class() == AnswerClass::Exact {
                        if let Some(d) = env.data() {
                            return Ok(map_path_resolution(d));
                        }
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
                if self.bounded_epoch_resident(lg) {
                    let env = lg.resolve_stable_key(stable_key);
                    if env.class() == AnswerClass::Exact {
                        if let Some(d) = env.data() {
                            return Ok(d.as_ref().map(map_candidate));
                        }
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
                if self.bounded_epoch_resident(lg) {
                    let env = lg.resolve_symbol_name(name);
                    if env.class() == AnswerClass::Exact {
                        if let Some(d) = env.data() {
                            return Ok(d.iter().map(map_candidate).collect());
                        }
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
                if self.bounded_epoch_resident(lg) {
                    let env = lg.symbol_context(symbol_stable_key);
                    if env.class() == AnswerClass::Exact {
                        if let Some(d) = env.data() {
                            return Ok(d.as_ref().map(map_symbol_context));
                        }
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
                if self.bounded_epoch_resident(lg) {
                    if let Some(rows) = lg_caller_rows(lg, symbol_stable_key) {
                        return Ok(rows);
                    }
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
                if self.bounded_epoch_resident(lg) {
                    if let Some(rows) = lg_callee_rows(lg, symbol_stable_key) {
                        return Ok(rows);
                    }
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
        _repo_uid: &str,
    ) -> Result<Option<AgentSnapshot>, AgentStorageError> {
        // W-B-EPOCH-IMPL-1 (D-EP, explain pin — review-0 #1): return the PINNED snapshot, NOT a fresh
        // `inner` "latest" resolve. The explain use case (`run_explain`) derives its `snapshot_uid` SOLELY
        // from this call and threads it into every downstream read AND the response stamp, so returning the
        // captured `epoch.snapshot` pins explain's WHOLE request to epoch N — on BOTH the green and red paths
        // (`handle_explain` wraps this decorator whenever an epoch was captured), with no agent-crate change.
        // This is the explain analogue of orient's double-resolve removal (orient threads `&epoch.snapshot`
        // into `orient_repo` directly, so orient never reaches this method). The decorator is request-scoped
        // to one repo + epoch, so `repo_uid` is not re-resolved.
        Ok(Some(self.epoch.snapshot.clone()))
    }

    fn get_stale_files(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<AgentStaleFile>, AgentStorageError> {
        self.inner.get_stale_files(snapshot_uid)
    }

    // EC-M2-LEAF-SERVE-1 (CYCLES-B): cycle VALUES serve from the LiveGraph module-cycle SCC when
    // the cycles cert's VALUES verdict is GREEN at the pinned epoch (the canonical agent shapes
    // were proven byte-equal at cert build); otherwise DELEGATE to the pinned SQLite snapshot
    // (RATIFIED CYCLES-A posture, unchanged).

    fn find_module_cycles(&self, snapshot_uid: &str) -> Result<Vec<AgentCycle>, AgentStorageError> {
        if let Some(cycles) = self.m2_module_cycles() {
            return Ok(cycles_repo_shape(&cycles));
        }
        self.inner.find_module_cycles(snapshot_uid)
    }

    // DAEMON-CANCEL-3: the checkpoint threads into WHICHEVER Tarjan runs — the LiveGraph SCC on
    // the M-2 green serve, the SQLite SCC on delegate. Without this forward, the trait default
    // would drop the checkpoint and green-path orient/explain would run the Tarjan to completion
    // after a disconnect.
    fn find_module_cycles_cancellable(
        &self,
        snapshot_uid: &str,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        if let Some(cycles) =
            self.m2_module_cycles_cancellable("find_module_cycles", &mut *cancel)?
        {
            return Ok(cycles_repo_shape(&cycles));
        }
        self.inner
            .find_module_cycles_cancellable(snapshot_uid, cancel)
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

    // EC-M2-LEAF-SERVE-1: MODULE_SUMMARY structural counts serve from the LiveGraph structural
    // inventory when the identity-reconciliation cert is GREEN at the pinned epoch (per-file +
    // per-module + exact-totals reconciled at cert build ⇒ these computed values are byte-equal to
    // the SQLite reads); otherwise DELEGATE (the pre-M-2 posture, byte-identical).

    fn compute_repo_summary(
        &self,
        snapshot_uid: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        if let Some(inv) = self.m2_summary_inventory() {
            return Ok(crate::module_summary_cert::repo_summary_from_inventory(
                &inv,
            ));
        }
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
        if let Some(inv) = self.m2_summary_inventory() {
            return Ok(crate::module_summary_cert::path_summary_from_inventory(
                &inv,
                path_prefix,
            ));
        }
        self.inner.compute_path_summary(snapshot_uid, path_prefix)
    }

    fn compute_file_summary(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        if let Some(inv) = self.m2_summary_inventory() {
            return Ok(crate::module_summary_cert::file_summary_from_inventory(
                &inv, file_path,
            ));
        }
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
        if let Some(cycles) = self.m2_module_cycles() {
            return Ok(cycles_qualified_filtered(&cycles, path_prefix, true));
        }
        self.inner
            .find_cycles_involving_path(snapshot_uid, path_prefix)
    }

    fn find_cycles_involving_path_cancellable(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        if let Some(cycles) =
            self.m2_module_cycles_cancellable("find_cycles_involving_path", &mut *cancel)?
        {
            return Ok(cycles_qualified_filtered(&cycles, path_prefix, true));
        }
        self.inner
            .find_cycles_involving_path_cancellable(snapshot_uid, path_prefix, cancel)
    }

    fn find_cycles_involving_module(
        &self,
        snapshot_uid: &str,
        module_qualified_name: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        if let Some(cycles) = self.m2_module_cycles() {
            return Ok(cycles_qualified_filtered(
                &cycles,
                module_qualified_name,
                false,
            ));
        }
        self.inner
            .find_cycles_involving_module(snapshot_uid, module_qualified_name)
    }

    fn find_cycles_involving_module_cancellable(
        &self,
        snapshot_uid: &str,
        module_qualified_name: &str,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        if let Some(cycles) =
            self.m2_module_cycles_cancellable("find_cycles_involving_module", &mut *cancel)?
        {
            return Ok(cycles_qualified_filtered(
                &cycles,
                module_qualified_name,
                false,
            ));
        }
        self.inner.find_cycles_involving_module_cancellable(
            snapshot_uid,
            module_qualified_name,
            cancel,
        )
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

    fn query_high_complexity_symbols_cancellable(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
        limit: usize,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError> {
        self.inner.query_high_complexity_symbols_cancellable(
            snapshot_uid,
            min_threshold,
            limit,
            cancel,
        )
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

    // ORIENT-DENSITY-1: the NAMED structure headline reads per-module sizes here.
    // It is a (c)-class SQLite read (module discovery, no LiveGraph home), so —
    // like get_module_summary above — DELEGATE to the inner port. Without this
    // the decorator would fall back to the trait's empty default and the dense
    // headline would lose its module names on the LiveGraph-served path.
    fn list_module_sizes(
        &self,
        snapshot_uid: &str,
        limit: usize,
    ) -> Result<Vec<AgentModuleSize>, AgentStorageError> {
        self.inner.list_module_sizes(snapshot_uid, limit)
    }

    // MODULE-MODEL-1 D2(i): the directory-topology read backing orient's package
    // groups. Like get_module_summary / list_module_sizes above it is a (c)-class
    // SQLite read (per-directory `nodes`+OWNS, no LiveGraph home) — DELEGATE to the
    // inner port. Without this the decorator would fall back to the trait's empty
    // default and the LiveGraph-served path would lose its package groups.
    fn list_directory_groups(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<AgentDirectoryGroup>, AgentStorageError> {
        self.inner.list_directory_groups(snapshot_uid)
    }

    // MODULE-MODEL-2 §13 D4: the per-toolchain manifest roots backing orient's
    // crate/package grouping. Like list_directory_groups above it is a (c)-class
    // SQLite read (module_candidates ⋈ evidence, no LiveGraph home) — DELEGATE to
    // the inner port. Without this the decorator would fall back to the trait's
    // empty default and the LiveGraph-served path would lose per-toolchain grouping.
    fn list_manifest_roots(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ManifestRoot>, AgentStorageError> {
        self.inner.list_manifest_roots(snapshot_uid)
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
