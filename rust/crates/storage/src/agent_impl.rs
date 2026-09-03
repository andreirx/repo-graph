//! Adapter impl: `AgentStorageRead` on `StorageConnection`.
//!
//! Added in Rust-42. The agent crate (policy) defines the
//! `AgentStorageRead` trait; this file is the adapter side that
//! lets the agent use-case layer read SQLite through a storage-
//! agnostic port.
//!
//! Responsibilities:
//!
//!   1. Translate storage errors into `AgentStorageError`. The
//!      agent crate never sees `rusqlite::Error`, `StorageError`,
//!      table names, or SQL diagnostics — only the stable
//!      `operation: &'static str` identifier plus a human-
//!      readable `message` string.
//!
//!   2. Map storage row DTOs (e.g. `queries::CycleResult`) into
//!      agent-owned DTOs (e.g. `repo_graph_agent::AgentCycle`).
//!      No storage types leak through the trait.
//!
//!   3. Assemble the trust summary projection by calling
//!      `repo_graph_trust::assemble_trust_report` internally and
//!      projecting the result into `AgentTrustSummary`. The
//!      agent crate does NOT depend on `repo-graph-trust`; trust
//!      policy lives on the adapter side of this boundary.

use repo_graph_agent::{
    AgentBoundaryDeclaration, AgentBoundaryLinksFreshness, AgentCalleeRow, AgentCallerRow,
    AgentCancelCheck, AgentComplexityMeasurement, AgentCycle, AgentDeadNode, AgentDirectoryGroup,
    AgentDocEntry, AgentFileEntry, AgentFocusCandidate, AgentFocusKind, AgentImportEdge,
    AgentImportEntry, AgentModuleSize, AgentModuleSummary, AgentPathResolution,
    AgentReliabilityAxis, AgentReliabilityLevel, AgentRepo, AgentRepoSummary, AgentSnapshot,
    AgentStaleFile, AgentStorageError, AgentStorageRead, AgentSymbolContext, AgentSymbolEntry,
    AgentTrustSummary, EnrichmentState,
};
use repo_graph_trust::service::{assemble_trust_report_cancellable, TrustReportOutcome};
use repo_graph_trust::types::{
    ReliabilityAxisScore as TrustAxisScore, ReliabilityLevel as TrustLevel,
};

/// Map the trust crate's `ReliabilityLevel` into the agent-owned
/// enum. Keeps the agent public surface independent of trust.
fn map_level(level: TrustLevel) -> AgentReliabilityLevel {
    match level {
        TrustLevel::HIGH => AgentReliabilityLevel::High,
        TrustLevel::MEDIUM => AgentReliabilityLevel::Medium,
        TrustLevel::LOW => AgentReliabilityLevel::Low,
    }
}

/// Map a trust-crate reliability axis score into the agent
/// DTO. Clones the reason strings verbatim — agents see the
/// same vocabulary the trust crate produced.
fn map_axis(axis: &TrustAxisScore) -> AgentReliabilityAxis {
    AgentReliabilityAxis {
        level: map_level(axis.level),
        reasons: axis.reasons.clone(),
    }
}

use crate::connection::StorageConnection;
use crate::types::RepoRef;

// ── Small error mapping helper ───────────────────────────────────

/// Convert any `Display`-able error into an `AgentStorageError`
/// with the supplied operation identifier. The message body is
/// the error's `Display` output — storage crate diagnostics are
/// stringified at this boundary and never parsed by the agent
/// layer.
///
/// `pub(crate)` so the extracted `agent_orient_reads` discovery reads map
/// errors identically (ORIENT-DENSITY-1 review-1 #3 split).
pub(crate) fn map_err<E: std::fmt::Display>(
    operation: &'static str,
) -> impl FnOnce(E) -> AgentStorageError {
    move |e| AgentStorageError::new(operation, e.to_string())
}

// ── Agent-specific helpers ───────────────────────────────────────

impl StorageConnection {
    /// Map a dead-node row into an `AgentDeadNode` DTO. Used by the
    /// `find_dead_nodes_in_path` and `find_dead_nodes_in_file`
    /// trait implementations. Same column order as the queries in
    /// the agent impl block.
    fn map_dead_node_row_agent(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDeadNode> {
        let name: String = row.get(1)?;
        let qualified_name: Option<String> = row.get(2)?;
        let line_count: Option<i64> = row.get(7)?;
        let is_test_int: i64 = row.get(8)?;
        Ok(AgentDeadNode {
            stable_key: row.get(0)?,
            symbol: qualified_name.unwrap_or(name),
            kind: row.get(3)?,
            file: row.get(5)?,
            line_count: line_count.and_then(|n| u64::try_from(n).ok()),
            is_test: is_test_int != 0,
        })
    }
}

// ── Impl ─────────────────────────────────────────────────────────

impl AgentStorageRead for StorageConnection {
    fn get_repo(&self, repo_uid: &str) -> Result<Option<AgentRepo>, AgentStorageError> {
        let repo = self
            .get_repo(&RepoRef::Uid(repo_uid.to_string()))
            .map_err(map_err("get_repo"))?;
        Ok(repo.map(|r| AgentRepo {
            repo_uid: r.repo_uid,
            name: r.name,
        }))
    }

    fn get_latest_snapshot(
        &self,
        repo_uid: &str,
    ) -> Result<Option<AgentSnapshot>, AgentStorageError> {
        let snap = <StorageConnection>::get_latest_snapshot(self, repo_uid)
            .map_err(map_err("get_latest_snapshot"))?;
        Ok(snap.map(|s| AgentSnapshot {
            snapshot_uid: s.snapshot_uid,
            repo_uid: s.repo_uid,
            // Storage column is `kind` ("full" | "incremental");
            // the agent DTO uses `scope` (semantic rename).
            scope: s.kind,
            basis_commit: s.basis_commit,
            created_at: s.created_at,
            files_total: s.files_total.max(0) as u64,
            nodes_total: s.nodes_total.max(0) as u64,
            edges_total: s.edges_total.max(0) as u64,
        }))
    }

    fn get_stale_files(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<AgentStaleFile>, AgentStorageError> {
        let files = <StorageConnection>::get_stale_files(self, snapshot_uid)
            .map_err(map_err("get_stale_files"))?;
        Ok(files
            .into_iter()
            .map(|f| AgentStaleFile { path: f.path })
            .collect())
    }

    fn query_file_count_by_language(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<(String, u64)>, AgentStorageError> {
        // Delegates to the crate-private inherent read (DEPS-LIST-REWRITE-1 §2.2). Kept behind the
        // storage read port (operator ruling 2, 2026-08-26) rather than as a new public inherent API.
        <StorageConnection>::query_file_count_by_language(self, snapshot_uid)
            .map_err(map_err("query_file_count_by_language"))
    }

    fn find_module_cycles(&self, snapshot_uid: &str) -> Result<Vec<AgentCycle>, AgentStorageError> {
        // Storage's `find_cycles` takes a level param; we always
        // use the module level at the agent boundary.
        let cycles = self
            .find_cycles(snapshot_uid, "module")
            .map_err(map_err("find_module_cycles"))?;
        // ORIENT-CYCLES-DISAGREE-1: this is the SQLite SERVING computation for `orient`'s
        // cycles — label each with the shared test-only classification here so the headline
        // matches `cycles` (which labels at ITS serving computation via the same classifier).
        crate::agent_cycle_labeling::label_module_cycles(self, snapshot_uid, cycles)
    }

    fn find_module_cycles_cancellable(
        &self,
        snapshot_uid: &str,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        // DAEMON-CANCEL-3: identical to `find_module_cycles` but routed through the
        // CANCELLABLE Tarjan entry (`find_cycles_cancellable`, CANCEL-1). `cancel` is
        // consulted inside the SCC traversal and the per-cycle name fan-out, so a
        // disconnected peer's in-flight orient abandons the O(V+E) traversal at the
        // next checkpoint instead of running it to completion with no consumer.
        // Read-only ⇒ nothing to roll back; `StorageError::Cancelled` maps through
        // the standard adapter boundary like any other storage failure.
        let cycles = self
            .find_cycles_cancellable(snapshot_uid, "module", cancel)
            .map_err(map_err("find_module_cycles"))?;
        // ORIENT-CYCLES-DISAGREE-1: label at the serving computation (see `find_module_cycles`).
        crate::agent_cycle_labeling::label_module_cycles(self, snapshot_uid, cycles)
    }

    fn find_dead_nodes(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        kind_filter: Option<&str>,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        let rows = self
            .find_dead_nodes(snapshot_uid, repo_uid, kind_filter)
            .map_err(map_err("find_dead_nodes"))?;
        Ok(rows
            .into_iter()
            .map(|r| AgentDeadNode {
                stable_key: r.stable_key,
                symbol: r.symbol,
                kind: r.kind,
                file: r.file,
                line_count: r.line_count.and_then(|n| u64::try_from(n).ok()),
                is_test: r.is_test,
            })
            .collect())
    }

    fn get_active_boundary_declarations(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
        let rows = self
            .get_active_boundary_declarations(repo_uid)
            .map_err(map_err("get_active_boundary_declarations"))?;
        Ok(rows
            .into_iter()
            .map(|d| AgentBoundaryDeclaration {
                source_module: d.boundary_module,
                forbidden_target: d.forbids,
                reason: d.reason,
            })
            .collect())
    }

    fn find_imports_between_paths(
        &self,
        snapshot_uid: &str,
        source_prefix: &str,
        target_prefix: &str,
    ) -> Result<Vec<AgentImportEdge>, AgentStorageError> {
        let rows = self
            .find_imports_between_paths(snapshot_uid, source_prefix, target_prefix)
            .map_err(map_err("find_imports_between_paths"))?;
        Ok(rows
            .into_iter()
            .map(|e| AgentImportEdge {
                source_file: e.source_file,
                target_file: e.target_file,
            })
            .collect())
    }

    fn compute_repo_summary(
        &self,
        snapshot_uid: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        let conn = self.connection();

        // file_count: DISTINCT files present in this snapshot
        // via the file_versions join. We avoid `snapshots.files_total`
        // directly because that column is only recomputed on
        // explicit snapshot counter refreshes — the join here is
        // always up-to-date with the row data.
        let file_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT file_uid) \
				 FROM file_versions \
				 WHERE snapshot_uid = ?",
                rusqlite::params![snapshot_uid],
                |row| row.get(0),
            )
            .map_err(map_err("compute_repo_summary"))?;

        // symbol_count: kind='SYMBOL' nodes only (contract
        // clarifies that MODULE_SUMMARY reports symbols, not
        // arbitrary nodes).
        let symbol_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes \
				 WHERE snapshot_uid = ? AND kind = 'SYMBOL'",
                rusqlite::params![snapshot_uid],
                |row| row.get(0),
            )
            .map_err(map_err("compute_repo_summary"))?;

        // languages: distinct, non-null, sorted ascending.
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT f.language \
				 FROM files f \
				 JOIN file_versions fv ON fv.file_uid = f.file_uid \
				 WHERE fv.snapshot_uid = ? \
				   AND f.language IS NOT NULL \
				 ORDER BY f.language ASC",
            )
            .map_err(map_err("compute_repo_summary"))?;
        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid], |row| {
                row.get::<_, String>(0)
            })
            .map_err(map_err("compute_repo_summary"))?;
        let mut languages: Vec<String> = Vec::new();
        for row in rows {
            languages.push(row.map_err(map_err("compute_repo_summary"))?);
        }

        Ok(AgentRepoSummary {
            file_count: file_count.max(0) as u64,
            symbol_count: symbol_count.max(0) as u64,
            languages,
        })
    }

    fn get_trust_summary(
        &self,
        repo_uid: &str,
        snapshot_uid: &str,
    ) -> Result<AgentTrustSummary, AgentStorageError> {
        // DAEMON-CANCEL-3: the non-cancellable entry delegates to the cancellable one
        // with a never-breaking checkpoint, so it stays byte-identical.
        self.get_trust_summary_cancellable(repo_uid, snapshot_uid, &mut || {
            std::ops::ControlFlow::Continue(())
        })
    }

    fn get_trust_summary_cancellable(
        &self,
        repo_uid: &str,
        snapshot_uid: &str,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<AgentTrustSummary, AgentStorageError> {
        // Delegate to the trust crate's CANCELLABLE assembly function, which uses the
        // existing `TrustStorageRead` impl on `StorageConnection` and threads the
        // cooperative checkpoint into its unresolved-sample loop. basis_commit and
        // toolchain_json are not needed by the agent projection and are passed as None.
        let outcome =
            assemble_trust_report_cancellable(self, repo_uid, snapshot_uid, None, None, cancel)
                .map_err(|e| AgentStorageError::new("get_trust_summary", format!("{:?}", e)))?;
        let report = match outcome {
            TrustReportOutcome::Ready(report) => *report,
            // The peer disconnected mid sample-loop. Read-only ⇒ nothing to roll back;
            // surface a "cancelled" storage error. On the daemon's check worker the
            // resulting CheckError is discarded (the supervisor already returned
            // Cancelled); for any other caller it is an honest abandonment signal.
            TrustReportOutcome::Cancelled => {
                return Err(AgentStorageError::new(
                    "get_trust_summary",
                    "cancelled (client disconnected during trust sample processing)",
                ));
            }
        };

        // Project the relevant scalars into the agent DTO.
        let resolved_calls = report.summary.resolved_calls;
        let unresolved_calls = report.summary.unresolved_calls;
        // RELIABILITY-REFRAME-1: project the in-scope (external-excluded) unresolved
        // count the trust service already computed, so the agent layer can render the
        // reader's in-scope rate without re-deriving the external split.
        let unresolved_calls_internal_like = report.summary.unresolved_calls_internal_like;
        let call_resolution_rate = report.summary.call_resolution_rate;

        // Reliability axes (Rust-43 F1/F3 fix). The agent
        // crate uses `dead_code_reliability.level` as the
        // authoritative gate for the DEAD_CODE signal; it does
        // NOT re-derive thresholds. `call_graph_reliability`
        // is projected too for symmetry and future confidence
        // rules.
        let call_graph_reliability = map_axis(&report.summary.reliability.call_graph);
        let dead_code_reliability = map_axis(&report.summary.reliability.dead_code);

        // Enrichment state (Rust-43 F2 fix, revised after the
        // spike-follow-up P2). Three-state mapping:
        //
        //   Ran:
        //     `enrichment_status == Some(_)`. The phase
        //     executed on at least one eligible sample. The
        //     `enriched` count can be zero and that still
        //     counts as Ran — the state is about phase
        //     execution, not success.
        //
        //   NotRun:
        //     `enrichment_status == None` AND
        //     `report.enrichment_eligible_count > 0`. The
        //     trust layer saw `CallsObjMethodNeedsTypeInfo`
        //     samples but found no enrichment metadata on any
        //     of them — the phase never ran on this snapshot.
        //     This is the F2 case the spike fix targets.
        //
        //   NotApplicable:
        //     `enrichment_status == None` AND
        //     `report.enrichment_eligible_count == 0`. No
        //     eligible samples. The enrichment phase has
        //     nothing to do on this snapshot, so it is not a
        //     penalty source on the confidence axis.
        //
        // The `enrichment_eligible_count` field is the
        // disambiguator added after the spike-follow-up P2
        // review: `Option<EnrichmentStatus>` alone could not
        // distinguish "no eligible samples" from "eligible
        // samples but phase did not run". Both collapsed to
        // `None`, producing a false `NotRun` on repos with
        // nothing to enrich.
        //
        // Some(es) with eligible == 0 is NOT reachable in the
        // current trust code (the compute function only
        // returns Some when `enrichment_was_run == true`,
        // which requires at least one sample with an
        // `enrichment` metadata marker, which only happens
        // when eligible_count >= 1). The match below handles
        // it defensively anyway.
        let (enrichment_state, enrichment_eligible, enrichment_enriched) = match &report
            .enrichment_status
        {
            None if report.enrichment_eligible_count == 0 => (EnrichmentState::NotApplicable, 0, 0),
            None => (EnrichmentState::NotRun, report.enrichment_eligible_count, 0),
            Some(es) if es.eligible == 0 => (EnrichmentState::NotApplicable, 0, es.enriched),
            Some(es) => (EnrichmentState::Ran, es.eligible, es.enriched),
        };

        // RELIABILITY-REFRAME-1 (review-3 §1/§2): project the reader-frame coverage facts the
        // shared `CallReliabilityView` needs — the UNCLASSIFIED portion of the in-scope
        // denominator, and the top named EXTERNAL targets (the trust service's
        // `top_external_types`, external-FILTERED then truncated, review-3 §3) mapped into the
        // agent crate's neutral `ExternalTarget` (the `repo-graph-trust` type never crosses the
        // port). Both are consumed daemon-side by `check`; no new storage read.
        let unresolved_calls_unknown = report.unresolved_calls_unknown;
        let external_targets: Vec<repo_graph_agent::reliability::ExternalTarget> = report
            .enrichment_status
            .as_ref()
            .map(|s| {
                s.top_external_types
                    .iter()
                    .map(|t| repo_graph_agent::reliability::ExternalTarget {
                        type_name: t.type_name.clone(),
                        count: t.count,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(AgentTrustSummary {
            call_resolution_rate,
            resolved_calls,
            unresolved_calls,
            unresolved_calls_internal_like,
            unresolved_calls_unknown,
            external_targets,
            call_graph_reliability,
            dead_code_reliability,
            enrichment_state,
            enrichment_eligible,
            enrichment_enriched,
        })
    }

    // ── Focus resolution (Rust-44) ──────────────────────────────

    fn resolve_path_focus(
        &self,
        snapshot_uid: &str,
        path: &str,
    ) -> Result<AgentPathResolution, AgentStorageError> {
        let conn = self.connection();

        // Check exact FILE node: a node with kind='FILE' whose
        // qualified_name matches the path, OR a file in the files
        // table with that path.
        let has_exact_file: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes n \
				 JOIN files f ON n.file_uid = f.file_uid \
				 WHERE n.snapshot_uid = ? AND n.kind = 'FILE' \
				   AND f.path = ?",
                rusqlite::params![snapshot_uid, path],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .map_err(map_err("resolve_path_focus"))?;

        // Check content under prefix: any FILE node whose file
        // path starts with "{path}/".
        let prefix_pattern = format!("{}/%", path);
        let has_content_under_prefix: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes n \
				 JOIN files f ON n.file_uid = f.file_uid \
				 WHERE n.snapshot_uid = ? AND n.kind = 'FILE' \
				   AND f.path LIKE ?",
                rusqlite::params![snapshot_uid, prefix_pattern],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .map_err(map_err("resolve_path_focus"))?;

        // When has_exact_file, resolve the FILE node's stable key.
        let file_stable_key: Option<String> = if has_exact_file {
            conn.query_row(
                "SELECT n.stable_key FROM nodes n \
				 JOIN files f ON n.file_uid = f.file_uid \
				 WHERE n.snapshot_uid = ? AND n.kind = 'FILE' \
				   AND f.path = ?",
                rusqlite::params![snapshot_uid, path],
                |row| row.get(0),
            )
            .ok()
        } else {
            None
        };

        // Check MODULE node at exact path.
        let module_stable_key: Option<String> = conn
            .query_row(
                "SELECT stable_key FROM nodes \
				 WHERE snapshot_uid = ? AND kind = 'MODULE' \
				   AND qualified_name = ?",
                rusqlite::params![snapshot_uid, path],
                |row| row.get(0),
            )
            .ok();

        Ok(AgentPathResolution {
            has_exact_file,
            file_stable_key,
            has_content_under_prefix,
            module_stable_key,
        })
    }

    fn resolve_stable_key_focus(
        &self,
        snapshot_uid: &str,
        stable_key: &str,
    ) -> Result<Option<AgentFocusCandidate>, AgentStorageError> {
        let result = self.connection().query_row(
            "SELECT n.stable_key, n.kind, f.path \
			 FROM nodes n \
			 LEFT JOIN files f ON n.file_uid = f.file_uid \
			 WHERE n.snapshot_uid = ? AND n.stable_key = ?",
            rusqlite::params![snapshot_uid, stable_key],
            |row| {
                let sk: String = row.get(0)?;
                let kind_str: String = row.get(1)?;
                let file: Option<String> = row.get(2)?;
                Ok((sk, kind_str, file))
            },
        );

        match result {
            Ok((sk, kind_str, file)) => {
                let kind = match kind_str.as_str() {
                    "FILE" => AgentFocusKind::File,
                    "MODULE" => AgentFocusKind::Module,
                    _ => AgentFocusKind::Symbol,
                };
                Ok(Some(AgentFocusCandidate {
                    stable_key: sk,
                    kind,
                    file,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(map_err("resolve_stable_key_focus")(e)),
        }
    }

    fn find_dead_nodes_in_path(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        let prefix_pattern = format!("{}/%", path_prefix);
        let sql = "SELECT
				n.stable_key, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start,
				CASE WHEN n.line_end IS NOT NULL AND n.line_start IS NOT NULL
				     THEN n.line_end - n.line_start + 1
				     ELSE NULL
				END AS line_count,
				COALESCE(f.is_test, 0) AS is_test
			 FROM nodes n
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE n.snapshot_uid = ?1
			   AND n.kind = 'SYMBOL'
			   AND (f.path LIKE ?3 OR f.path = ?4)
			   AND n.node_uid NOT IN (
			     SELECT e.target_node_uid FROM edges e
			     WHERE e.snapshot_uid = ?1
			       AND e.type IN ('IMPORTS', 'CALLS', 'IMPLEMENTS', 'INSTANTIATES',
			                      'ROUTES_TO', 'REGISTERED_BY', 'TESTED_BY', 'COVERS')
			   )
			   AND n.stable_key NOT IN (
			     SELECT d.target_stable_key FROM declarations d
			     WHERE d.repo_uid = ?2
			       AND d.kind = 'entrypoint'
			       AND d.is_active = 1
			       AND (d.snapshot_uid IS NULL OR d.snapshot_uid = ?1)
			   )
			   AND n.stable_key NOT IN (
			     SELECT i.target_stable_key FROM inferences i
			     WHERE i.snapshot_uid = ?1
			       AND i.kind IN ('framework_entrypoint', 'spring_container_managed',
			                      'pytest_test', 'pytest_fixture', 'linux_system_managed')
			   )
			 ORDER BY n.name ASC";

        let conn = self.connection();
        let mut stmt = conn
            .prepare(sql)
            .map_err(map_err("find_dead_nodes_in_path"))?;

        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, repo_uid, prefix_pattern, path_prefix],
                Self::map_dead_node_row_agent,
            )
            .map_err(map_err("find_dead_nodes_in_path"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_err("find_dead_nodes_in_path"))
    }

    fn find_dead_nodes_in_file(
        &self,
        snapshot_uid: &str,
        repo_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
        let sql = "SELECT
				n.stable_key, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start,
				CASE WHEN n.line_end IS NOT NULL AND n.line_start IS NOT NULL
				     THEN n.line_end - n.line_start + 1
				     ELSE NULL
				END AS line_count,
				COALESCE(f.is_test, 0) AS is_test
			 FROM nodes n
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE n.snapshot_uid = ?1
			   AND n.kind = 'SYMBOL'
			   AND f.path = ?3
			   AND n.node_uid NOT IN (
			     SELECT e.target_node_uid FROM edges e
			     WHERE e.snapshot_uid = ?1
			       AND e.type IN ('IMPORTS', 'CALLS', 'IMPLEMENTS', 'INSTANTIATES',
			                      'ROUTES_TO', 'REGISTERED_BY', 'TESTED_BY', 'COVERS')
			   )
			   AND n.stable_key NOT IN (
			     SELECT d.target_stable_key FROM declarations d
			     WHERE d.repo_uid = ?2
			       AND d.kind = 'entrypoint'
			       AND d.is_active = 1
			       AND (d.snapshot_uid IS NULL OR d.snapshot_uid = ?1)
			   )
			   AND n.stable_key NOT IN (
			     SELECT i.target_stable_key FROM inferences i
			     WHERE i.snapshot_uid = ?1
			       AND i.kind IN ('framework_entrypoint', 'spring_container_managed',
			                      'pytest_test', 'pytest_fixture', 'linux_system_managed')
			   )
			 ORDER BY n.name ASC";

        let conn = self.connection();
        let mut stmt = conn
            .prepare(sql)
            .map_err(map_err("find_dead_nodes_in_file"))?;

        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, repo_uid, file_path],
                Self::map_dead_node_row_agent,
            )
            .map_err(map_err("find_dead_nodes_in_file"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_err("find_dead_nodes_in_file"))
    }

    fn compute_path_summary(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        let conn = self.connection();
        let prefix_pattern = format!("{}/%", path_prefix);

        let file_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT fv.file_uid) \
				 FROM file_versions fv \
				 JOIN files f ON fv.file_uid = f.file_uid \
				 WHERE fv.snapshot_uid = ? \
				   AND (f.path LIKE ? OR f.path = ?)",
                rusqlite::params![snapshot_uid, prefix_pattern, path_prefix],
                |row| row.get(0),
            )
            .map_err(map_err("compute_path_summary"))?;

        let symbol_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes n \
				 JOIN files f ON n.file_uid = f.file_uid \
				 WHERE n.snapshot_uid = ? AND n.kind = 'SYMBOL' \
				   AND (f.path LIKE ? OR f.path = ?)",
                rusqlite::params![snapshot_uid, prefix_pattern, path_prefix],
                |row| row.get(0),
            )
            .map_err(map_err("compute_path_summary"))?;

        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT f.language \
				 FROM files f \
				 JOIN file_versions fv ON fv.file_uid = f.file_uid \
				 WHERE fv.snapshot_uid = ? \
				   AND f.language IS NOT NULL \
				   AND (f.path LIKE ? OR f.path = ?) \
				 ORDER BY f.language ASC",
            )
            .map_err(map_err("compute_path_summary"))?;
        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, prefix_pattern, path_prefix],
                |row| row.get::<_, String>(0),
            )
            .map_err(map_err("compute_path_summary"))?;
        let mut languages: Vec<String> = Vec::new();
        for row in rows {
            languages.push(row.map_err(map_err("compute_path_summary"))?);
        }

        Ok(AgentRepoSummary {
            file_count: file_count.max(0) as u64,
            symbol_count: symbol_count.max(0) as u64,
            languages,
        })
    }

    fn compute_file_summary(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<AgentRepoSummary, AgentStorageError> {
        let conn = self.connection();

        let file_count: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT fv.file_uid) \
				 FROM file_versions fv \
				 JOIN files f ON fv.file_uid = f.file_uid \
				 WHERE fv.snapshot_uid = ? \
				   AND f.path = ?",
                rusqlite::params![snapshot_uid, file_path],
                |row| row.get(0),
            )
            .map_err(map_err("compute_file_summary"))?;

        let symbol_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes n \
				 JOIN files f ON n.file_uid = f.file_uid \
				 WHERE n.snapshot_uid = ? AND n.kind = 'SYMBOL' \
				   AND f.path = ?",
                rusqlite::params![snapshot_uid, file_path],
                |row| row.get(0),
            )
            .map_err(map_err("compute_file_summary"))?;

        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT f.language \
				 FROM files f \
				 JOIN file_versions fv ON fv.file_uid = f.file_uid \
				 WHERE fv.snapshot_uid = ? \
				   AND f.language IS NOT NULL \
				   AND f.path = ? \
				 ORDER BY f.language ASC",
            )
            .map_err(map_err("compute_file_summary"))?;
        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid, file_path], |row| {
                row.get::<_, String>(0)
            })
            .map_err(map_err("compute_file_summary"))?;
        let mut languages: Vec<String> = Vec::new();
        for row in rows {
            languages.push(row.map_err(map_err("compute_file_summary"))?);
        }

        Ok(AgentRepoSummary {
            file_count: file_count.max(0) as u64,
            symbol_count: symbol_count.max(0) as u64,
            languages,
        })
    }

    fn find_boundary_declarations_in_path(
        &self,
        repo_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
        // Fetch all active boundary declarations, then filter
        // by prefix. The SQL already parses the stable key to
        // extract the module path; filtering in Rust is simpler
        // than modifying the SQL extraction.
        let all = self
            .get_active_boundary_declarations(repo_uid)
            .map_err(map_err("find_boundary_declarations_in_path"))?;

        let filtered = all
            .into_iter()
            .filter(|d| {
                d.boundary_module == path_prefix
                    || d.boundary_module.starts_with(&format!("{}/", path_prefix))
            })
            .map(|d| AgentBoundaryDeclaration {
                source_module: d.boundary_module,
                forbidden_target: d.forbids,
                reason: d.reason,
            })
            .collect();

        Ok(filtered)
    }

    fn find_cycles_involving_path(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        // Run the full cycle query, then filter to cycles where
        // at least one module's qualified_name (full path) matches
        // the prefix.
        //
        // `CycleNode.name` is the short display name (e.g.
        // `seams`). The prefix check must use the full
        // `qualified_name` (e.g. `src/core/seams`) because focus
        // strings are repo-relative paths. The Rust-44 spike
        // found that comparing short names against full paths
        // almost never matched — e.g. the module at
        // `src/core/seams` has short name `seams`, which does
        // not start with `src/core/seams/`.
        //
        // The returned `AgentCycle.modules` also carries
        // qualified_names so path-scoped cycle evidence shows
        // full paths. The repo-level aggregator continues using
        // short names through its own `find_module_cycles` path.
        let all_cycles = self
            .find_cycles(snapshot_uid, "module")
            .map_err(map_err("find_cycles_involving_path"))?;

        let conn = self.connection();
        let filtered: Vec<AgentCycle> = all_cycles
            .into_iter()
            .filter_map(|c| {
                let qualified_names: Vec<String> = c
                    .nodes
                    .iter()
                    .map(|n| {
                        conn.query_row(
                            "SELECT qualified_name FROM nodes \
							 WHERE node_uid = ?",
                            rusqlite::params![n.node_id],
                            |row| row.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| n.name.clone())
                    })
                    .collect();

                let involves_prefix = qualified_names
                    .iter()
                    .any(|qn| qn == path_prefix || qn.starts_with(&format!("{}/", path_prefix)));

                if involves_prefix {
                    Some(AgentCycle {
                        length: c.length,
                        modules: qualified_names,
                        // ORIENT-CYCLES-DISAGREE-1: focus/path-scoped cycles are not the repo
                        // headline this slice unifies — no test-only split is claimed here.
                        test_composition: None,
                        type_only: None,
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(filtered)
    }

    fn find_cycles_involving_path_cancellable(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        // DAEMON-CANCEL-3: same shape as `find_cycles_involving_path`, but (1) the
        // heavy Tarjan runs through the cancellable entry (reborrow `&mut *cancel` so
        // the checkpoint stays usable for the filter below) and (2) the per-cycle
        // qualified-name fan-out — itself a SQL lookup per cycle member, unbounded on
        // a pathological many-cycle graph — is checkpointed per cycle. Read-only ⇒ a
        // cancelled query drops its partial result with nothing to undo.
        let all_cycles = self
            .find_cycles_cancellable(snapshot_uid, "module", &mut *cancel)
            .map_err(map_err("find_cycles_involving_path"))?;

        let conn = self.connection();
        let mut filtered: Vec<AgentCycle> = Vec::new();
        for c in all_cycles {
            if cancel().is_break() {
                return Err(AgentStorageError::new(
                    "find_cycles_involving_path",
                    "cancelled (client disconnected during cycle filtering)",
                ));
            }
            let qualified_names: Vec<String> = c
                .nodes
                .iter()
                .map(|n| {
                    conn.query_row(
                        "SELECT qualified_name FROM nodes \
						 WHERE node_uid = ?",
                        rusqlite::params![n.node_id],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| n.name.clone())
                })
                .collect();

            let involves_prefix = qualified_names
                .iter()
                .any(|qn| qn == path_prefix || qn.starts_with(&format!("{}/", path_prefix)));

            if involves_prefix {
                filtered.push(AgentCycle {
                    length: c.length,
                    modules: qualified_names,
                    // ORIENT-CYCLES-DISAGREE-1: focus/path-scoped cycles are not the repo
                    // headline this slice unifies — no test-only split is claimed here.
                    test_composition: None,
                    type_only: None,
                });
            }
        }

        Ok(filtered)
    }

    // ── Symbol-focus methods (Rust-45) ──────────────────────────

    fn resolve_symbol_name(
        &self,
        snapshot_uid: &str,
        name: &str,
    ) -> Result<Vec<AgentFocusCandidate>, AgentStorageError> {
        let conn = self.connection();
        let mut stmt = conn
            .prepare(
                "SELECT n.stable_key, n.kind, f.path \
				 FROM nodes n \
				 LEFT JOIN files f ON n.file_uid = f.file_uid \
				 WHERE n.snapshot_uid = ? AND n.kind = 'SYMBOL' AND n.name = ? \
				 ORDER BY n.stable_key ASC \
				 LIMIT 5",
            )
            .map_err(map_err("resolve_symbol_name"))?;

        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid, name], |row| {
                let sk: String = row.get(0)?;
                let _kind_str: String = row.get(1)?;
                let file: Option<String> = row.get(2)?;
                Ok(AgentFocusCandidate {
                    stable_key: sk,
                    kind: AgentFocusKind::Symbol,
                    file,
                })
            })
            .map_err(map_err("resolve_symbol_name"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_err("resolve_symbol_name"))
    }

    fn get_symbol_context(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Option<AgentSymbolContext>, AgentStorageError> {
        let result = self.connection().query_row(
            "SELECT \
				n.name, n.qualified_name, n.subtype, n.line_start, \
				f.path AS file_path, \
				mod_n.qualified_name AS module_path, \
				mod_n.stable_key AS module_stable_key \
			 FROM nodes n \
			 LEFT JOIN files f ON n.file_uid = f.file_uid \
			 LEFT JOIN nodes file_node ON file_node.file_uid = n.file_uid \
				AND file_node.kind = 'FILE' \
				AND file_node.snapshot_uid = n.snapshot_uid \
			 LEFT JOIN edges own ON own.type = 'OWNS' \
				AND own.target_node_uid = file_node.node_uid \
				AND own.snapshot_uid = n.snapshot_uid \
			 LEFT JOIN nodes mod_n ON own.source_node_uid = mod_n.node_uid \
			 WHERE n.snapshot_uid = ? AND n.stable_key = ?",
            rusqlite::params![snapshot_uid, symbol_stable_key],
            |row| {
                let name: String = row.get(0)?;
                let qualified_name: Option<String> = row.get(1)?;
                let subtype: Option<String> = row.get(2)?;
                let line_start: Option<i64> = row.get(3)?;
                let file_path: Option<String> = row.get(4)?;
                let module_path: Option<String> = row.get(5)?;
                let module_stable_key: Option<String> = row.get(6)?;
                Ok(AgentSymbolContext {
                    file_path,
                    module_path,
                    module_stable_key,
                    name,
                    qualified_name,
                    subtype,
                    line_start: line_start.and_then(|n| u64::try_from(n).ok()),
                })
            },
        );

        match result {
            Ok(ctx) => Ok(Some(ctx)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(map_err("get_symbol_context")(e)),
        }
    }

    fn find_symbol_callers(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Vec<AgentCallerRow>, AgentStorageError> {
        let conn = self.connection();
        let mut stmt = conn
            .prepare(
                "SELECT \
					caller.stable_key, caller.name, \
					f.path AS file_path, \
					mod_n.qualified_name AS module_path, \
					mod_n.stable_key AS module_stable_key \
				 FROM edges e \
				 JOIN nodes caller ON e.source_node_uid = caller.node_uid \
				 LEFT JOIN files f ON caller.file_uid = f.file_uid \
				 LEFT JOIN nodes file_node ON file_node.file_uid = caller.file_uid \
					AND file_node.kind = 'FILE' \
					AND file_node.snapshot_uid = e.snapshot_uid \
				 LEFT JOIN edges own ON own.type = 'OWNS' \
					AND own.target_node_uid = file_node.node_uid \
					AND own.snapshot_uid = e.snapshot_uid \
				 LEFT JOIN nodes mod_n ON own.source_node_uid = mod_n.node_uid \
				 WHERE e.snapshot_uid = ? \
					AND e.type = 'CALLS' \
					AND e.target_node_uid = ( \
						SELECT node_uid FROM nodes \
						WHERE snapshot_uid = ? AND stable_key = ? \
						LIMIT 1 \
					)",
            )
            .map_err(map_err("find_symbol_callers"))?;

        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, snapshot_uid, symbol_stable_key,],
                |row| {
                    Ok(AgentCallerRow {
                        stable_key: row.get(0)?,
                        name: row.get(1)?,
                        file: row.get(2)?,
                        module_path: row.get(3)?,
                        module_stable_key: row.get(4)?,
                    })
                },
            )
            .map_err(map_err("find_symbol_callers"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_err("find_symbol_callers"))
    }

    fn find_symbol_callees(
        &self,
        snapshot_uid: &str,
        symbol_stable_key: &str,
    ) -> Result<Vec<AgentCalleeRow>, AgentStorageError> {
        let conn = self.connection();
        let mut stmt = conn
            .prepare(
                "SELECT \
					callee.stable_key, callee.name, \
					f.path AS file_path, \
					mod_n.qualified_name AS module_path, \
					mod_n.stable_key AS module_stable_key \
				 FROM edges e \
				 JOIN nodes callee ON e.target_node_uid = callee.node_uid \
				 LEFT JOIN files f ON callee.file_uid = f.file_uid \
				 LEFT JOIN nodes file_node ON file_node.file_uid = callee.file_uid \
					AND file_node.kind = 'FILE' \
					AND file_node.snapshot_uid = e.snapshot_uid \
				 LEFT JOIN edges own ON own.type = 'OWNS' \
					AND own.target_node_uid = file_node.node_uid \
					AND own.snapshot_uid = e.snapshot_uid \
				 LEFT JOIN nodes mod_n ON own.source_node_uid = mod_n.node_uid \
				 WHERE e.snapshot_uid = ? \
					AND e.type = 'CALLS' \
					AND e.source_node_uid = ( \
						SELECT node_uid FROM nodes \
						WHERE snapshot_uid = ? AND stable_key = ? \
						LIMIT 1 \
					)",
            )
            .map_err(map_err("find_symbol_callees"))?;

        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, snapshot_uid, symbol_stable_key,],
                |row| {
                    Ok(AgentCalleeRow {
                        stable_key: row.get(0)?,
                        name: row.get(1)?,
                        file: row.get(2)?,
                        module_path: row.get(3)?,
                        module_stable_key: row.get(4)?,
                    })
                },
            )
            .map_err(map_err("find_symbol_callees"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_err("find_symbol_callees"))
    }

    fn find_cycles_involving_module(
        &self,
        snapshot_uid: &str,
        module_qualified_name: &str,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        // Same as find_cycles_involving_path but with exact match
        // instead of prefix match.
        let all_cycles = self
            .find_cycles(snapshot_uid, "module")
            .map_err(map_err("find_cycles_involving_module"))?;

        let conn = self.connection();
        let filtered: Vec<AgentCycle> = all_cycles
            .into_iter()
            .filter_map(|c| {
                let qualified_names: Vec<String> = c
                    .nodes
                    .iter()
                    .map(|n| {
                        conn.query_row(
                            "SELECT qualified_name FROM nodes \
							 WHERE node_uid = ?",
                            rusqlite::params![n.node_id],
                            |row| row.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| n.name.clone())
                    })
                    .collect();

                // Exact match — NOT prefix matching.
                let involves = qualified_names.iter().any(|qn| qn == module_qualified_name);

                if involves {
                    Some(AgentCycle {
                        length: c.length,
                        modules: qualified_names,
                        // ORIENT-CYCLES-DISAGREE-1: focus/path-scoped cycles are not the repo
                        // headline this slice unifies — no test-only split is claimed here.
                        test_composition: None,
                        type_only: None,
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(filtered)
    }

    fn find_cycles_involving_module_cancellable(
        &self,
        snapshot_uid: &str,
        module_qualified_name: &str,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentCycle>, AgentStorageError> {
        // DAEMON-CANCEL-3: the exact-match sibling of
        // `find_cycles_involving_path_cancellable` (cancellable Tarjan + per-cycle
        // checkpointed fan-out). Read-only ⇒ a cancelled query drops its partial
        // result.
        let all_cycles = self
            .find_cycles_cancellable(snapshot_uid, "module", &mut *cancel)
            .map_err(map_err("find_cycles_involving_module"))?;

        let conn = self.connection();
        let mut filtered: Vec<AgentCycle> = Vec::new();
        for c in all_cycles {
            if cancel().is_break() {
                return Err(AgentStorageError::new(
                    "find_cycles_involving_module",
                    "cancelled (client disconnected during cycle filtering)",
                ));
            }
            let qualified_names: Vec<String> = c
                .nodes
                .iter()
                .map(|n| {
                    conn.query_row(
                        "SELECT qualified_name FROM nodes \
						 WHERE node_uid = ?",
                        rusqlite::params![n.node_id],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| n.name.clone())
                })
                .collect();

            // Exact match — NOT prefix matching.
            if qualified_names.iter().any(|qn| qn == module_qualified_name) {
                filtered.push(AgentCycle {
                    length: c.length,
                    modules: qualified_names,
                    // ORIENT-CYCLES-DISAGREE-1: focus/path-scoped cycles are not the repo
                    // headline this slice unifies — no test-only split is claimed here.
                    test_composition: None,
                    type_only: None,
                });
            }
        }

        Ok(filtered)
    }

    // ── Explain-focus methods ──────────────────────────────────────

    fn list_symbols_in_file(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentSymbolEntry>, AgentStorageError> {
        let conn = self.connection();
        let mut stmt = conn
            .prepare(
                "SELECT n.stable_key, n.name, n.qualified_name, n.subtype, n.line_start \
				 FROM nodes n \
				 JOIN files f ON n.file_uid = f.file_uid \
				 WHERE n.snapshot_uid = ? AND n.kind = 'SYMBOL' AND f.path = ? \
				 ORDER BY n.line_start ASC, n.name ASC",
            )
            .map_err(map_err("list_symbols_in_file"))?;

        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid, file_path], |row| {
                let line_start: Option<i64> = row.get(4)?;
                Ok(AgentSymbolEntry {
                    stable_key: row.get(0)?,
                    name: row.get(1)?,
                    qualified_name: row.get(2)?,
                    subtype: row.get(3)?,
                    line_start: line_start.and_then(|n| u64::try_from(n).ok()),
                })
            })
            .map_err(map_err("list_symbols_in_file"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_err("list_symbols_in_file"))
    }

    fn list_files_in_path(
        &self,
        snapshot_uid: &str,
        path_prefix: &str,
    ) -> Result<Vec<AgentFileEntry>, AgentStorageError> {
        let conn = self.connection();
        let prefix_pattern = format!("{}/%", path_prefix);

        let mut stmt = conn
            .prepare(
                "SELECT f.path, \
				   (SELECT COUNT(*) FROM nodes n2 \
				    WHERE n2.file_uid = f.file_uid \
				      AND n2.kind = 'SYMBOL' \
				      AND n2.snapshot_uid = ?1) AS symbol_count, \
				   f.is_test \
				 FROM files f \
				 JOIN file_versions fv ON fv.file_uid = f.file_uid \
				 WHERE fv.snapshot_uid = ?2 \
				   AND (f.path LIKE ?3 OR f.path = ?4) \
				 ORDER BY f.path ASC",
            )
            .map_err(map_err("list_files_in_path"))?;

        let rows = stmt
            .query_map(
                rusqlite::params![snapshot_uid, snapshot_uid, prefix_pattern, path_prefix,],
                |row| {
                    let sym_count: i64 = row.get(1)?;
                    let is_test_int: i64 = row.get(2)?;
                    Ok(AgentFileEntry {
                        path: row.get(0)?,
                        symbol_count: sym_count.max(0) as u64,
                        is_test: is_test_int != 0,
                    })
                },
            )
            .map_err(map_err("list_files_in_path"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_err("list_files_in_path"))
    }

    fn find_file_imports(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<Vec<AgentImportEntry>, AgentStorageError> {
        let conn = self.connection();
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT tgt_f.path AS target_file \
				 FROM edges e \
				 JOIN nodes src_n ON e.source_node_uid = src_n.node_uid \
				 JOIN files src_f ON src_n.file_uid = src_f.file_uid \
				 JOIN nodes tgt_n ON e.target_node_uid = tgt_n.node_uid \
				 JOIN files tgt_f ON tgt_n.file_uid = tgt_f.file_uid \
				 WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS' AND src_f.path = ? \
				 ORDER BY tgt_f.path ASC",
            )
            .map_err(map_err("find_file_imports"))?;

        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid, file_path], |row| {
                Ok(AgentImportEntry {
                    target_file: row.get(0)?,
                })
            })
            .map_err(map_err("find_file_imports"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_err("find_file_imports"))
    }

    // ── Documentation inventory (docs-primary pivot) ───────────────

    fn get_doc_inventory(&self, repo_uid: &str) -> Result<Vec<AgentDocEntry>, AgentStorageError> {
        // ORIENT-DENSITY-1 review-1 #1+#3: the body (incl. the DB-parent-relative
        // root_path resolution FIX that made orient show docs) lives in the
        // extracted `agent_orient_reads` discovery module.
        crate::agent_orient_reads::doc_inventory(self.connection(), repo_uid)
    }

    // ── Complexity measurements ─────────────────────────────────────

    fn query_high_complexity_symbols(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
        limit: usize,
    ) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError> {
        // Single SQL + materialization site lives in the cancellable variant; the
        // non-cancellable entry is that path with a never-breaking checkpoint —
        // byte-identical for every existing (test / non-daemon) caller.
        self.query_high_complexity_symbols_cancellable(
            snapshot_uid,
            min_threshold,
            limit,
            &mut || std::ops::ControlFlow::Continue(()),
        )
    }

    fn query_high_complexity_symbols_cancellable(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
        limit: usize,
        cancel: AgentCancelCheck<'_>,
    ) -> Result<Vec<AgentComplexityMeasurement>, AgentStorageError> {
        let conn = self.connection();

        // Query cyclomatic_complexity measurements joined with nodes to get
        // symbol name and file path. Filter by threshold, order by complexity
        // descending, limit to top N.
        let mut stmt = conn
            .prepare(
                "SELECT m.target_stable_key, n.name, f.path, m.value_json \
				 FROM measurements m \
				 LEFT JOIN nodes n ON m.target_stable_key = n.stable_key \
				   AND n.snapshot_uid = m.snapshot_uid \
				 LEFT JOIN files f ON n.file_uid = f.file_uid \
				 WHERE m.snapshot_uid = ? AND m.kind = 'cyclomatic_complexity' \
				 ORDER BY CAST(json_extract(m.value_json, '$.value') AS INTEGER) DESC \
				 LIMIT ?",
            )
            .map_err(map_err("query_high_complexity_symbols"))?;

        let rows = stmt
            .query_map(rusqlite::params![snapshot_uid, limit], |row| {
                let stable_key: String = row.get(0)?;
                let symbol_name: Option<String> = row.get(1)?;
                let file_path: Option<String> = row.get(2)?;
                let value_json: String = row.get(3)?;

                // Parse the complexity value from JSON {"value": N}
                let complexity: u64 = serde_json::from_str::<serde_json::Value>(&value_json)
                    .ok()
                    .and_then(|v| v.get("value").and_then(|n| n.as_u64()))
                    .unwrap_or(0);

                Ok((stable_key, symbol_name, file_path, complexity))
            })
            .map_err(map_err("query_high_complexity_symbols"))?;

        // Collect and filter by threshold.
        //
        // DAEMON-CANCEL-3 — why the COOPERATIVE checkpoint is the right tool here (review
        // iteration-1 item 5). orient calls this with `limit == FETCH_ALL` (i64::MAX), so
        // `query_map` materializes EVERY cyclomatic_complexity row into Rust. The
        // DEMONSTRATED heavy chokepoint (Codex-cited: `aggregators/complexity.rs` fetches
        // the full above-threshold set then sorts/truncates in Rust) is exactly THIS Rust
        // collect+threshold loop — and it IS what `cancel` covers: we consult it once per
        // CHUNK rows as we pull them, abandoning a huge materialization within a bounded
        // number of rows of a disconnect. Read-only ⇒ the partial `results` is dropped.
        //
        // The residual window: the SQL `ORDER BY CAST(json_extract(...) AS INTEGER)` is a
        // non-indexed expression, so SQLite computes the key + sorts the WHERE-filtered
        // set (bounded by the number of complexity measurements, i.e. O(functions))
        // BEFORE yielding row 1. A transport-thread Rust checkpoint cannot fire inside
        // that opaque sort. This is NOT a gap specific to complexity — it is the SAME
        // ratified K-A limitation as the cycles Tarjan (whose edge-load `SELECT` is
        // likewise pre-loop and uncancellable) and is consistent with §14's documented
        // "cancel latency = checkpoint spacing, and a checkpoint cannot interrupt an
        // opaque in-Rust-unreachable statement". orient runs cooperatively ON the
        // transport thread by ratified design (D-K = K-A); it does NOT run on a worker, so
        // `sqlite3_interrupt` is not available to it. Moving orient to the worker +
        // interrupt purely to cover this bounded sort window would be a far larger change
        // than this NARROW slice warrants (orient assembles many signals, not one SQL) —
        // it is the named upgrade trigger IF the sort window is ever shown to be material.
        // The bounded sort is NOT the demonstrated chokepoint; the Rust materialization
        // below is, and it is covered (proven by the dispatcher test
        // `dispatched_orient_cancels_mid_complexity_materialization_when_peer_disconnects`).
        const CHUNK: usize = 1024;
        let mut results = Vec::new();
        for (i, row) in rows.enumerate() {
            if i % CHUNK == 0 && cancel().is_break() {
                return Err(AgentStorageError::new(
                    "query_high_complexity_symbols",
                    "cancelled (client disconnected during complexity materialization)",
                ));
            }
            let (stable_key, symbol_name, file_path, complexity) =
                row.map_err(map_err("query_high_complexity_symbols"))?;

            if complexity >= min_threshold {
                results.push(AgentComplexityMeasurement {
                    stable_key,
                    symbol_name: symbol_name.unwrap_or_else(|| "unknown".to_string()),
                    file_path,
                    complexity,
                });
            }
        }

        Ok(results)
    }

    fn has_complexity_measurements(&self, snapshot_uid: &str) -> Result<bool, AgentStorageError> {
        let conn = self.connection();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM measurements \
				 WHERE snapshot_uid = ? AND kind = 'cyclomatic_complexity' \
				 LIMIT 1",
                rusqlite::params![snapshot_uid],
                |row| row.get(0),
            )
            .map_err(map_err("has_complexity_measurements"))?;

        Ok(count > 0)
    }

    fn count_high_complexity_symbols(
        &self,
        snapshot_uid: &str,
        min_threshold: u64,
    ) -> Result<u64, AgentStorageError> {
        let conn = self.connection();

        // Complexity is stored in value_json as {"value": N}.
        // Use json_extract to compare against threshold.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM measurements \
				 WHERE snapshot_uid = ? \
				   AND kind = 'cyclomatic_complexity' \
				   AND CAST(json_extract(value_json, '$.value') AS INTEGER) >= ?",
                rusqlite::params![snapshot_uid, min_threshold as i64],
                |row| row.get(0),
            )
            .map_err(map_err("count_high_complexity_symbols"))?;

        Ok(count as u64)
    }

    /// Get module summary from module_candidates table.
    ///
    /// ## Phase 4 change (2026-05-10)
    ///
    /// Prior to Phase 4, this method fell back to MODULE nodes in the `nodes`
    /// table when `module_candidates` was empty. That fallback has been removed.
    /// Empty `module_candidates` now returns `None`, surfacing repos that need
    /// module detection configured rather than silently providing degraded data.
    fn get_module_summary(
        &self,
        snapshot_uid: &str,
    ) -> Result<Option<AgentModuleSummary>, AgentStorageError> {
        let conn = self.connection();

        // Query module_candidates table (the sole source after Phase 4)
        let mut stmt = conn
            .prepare(
                "SELECT module_kind, COUNT(*) as cnt \
				 FROM module_candidates \
				 WHERE snapshot_uid = ? \
				 GROUP BY module_kind",
            )
            .map_err(map_err("get_module_summary"))?;

        let rows = stmt
            .query_map([snapshot_uid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(map_err("get_module_summary"))?;

        let mut declared: u64 = 0;
        let mut operational: u64 = 0;
        let mut inferred: u64 = 0;
        let mut total: u64 = 0;

        for row in rows {
            let (kind, count) = row.map_err(map_err("get_module_summary"))?;
            let count = count as u64;
            total += count;
            match kind.as_str() {
                "declared" => declared += count,
                "operational" => operational += count,
                "inferred" => inferred += count,
                "directory" => inferred += count,
                _ => {}
            }
        }

        if total > 0 {
            Ok(Some(AgentModuleSummary {
                discovered_module_count: total,
                declared_count: declared,
                operational_count: operational,
                inferred_count: inferred,
            }))
        } else {
            // No module data. After Phase 4, this surfaces repos that need
            // module detection rather than silently falling back to MODULE nodes.
            Ok(None)
        }
    }

    fn list_module_sizes(
        &self,
        snapshot_uid: &str,
        limit: usize,
    ) -> Result<Vec<AgentModuleSize>, AgentStorageError> {
        // ORIENT-DENSITY-1 review-1 #2+#3: the body (incl. the budget-derived,
        // bind-safe LIMIT that lets `--full` carry the COMPLETE module list)
        // lives in the extracted `agent_orient_reads` discovery module.
        crate::agent_orient_reads::module_sizes(self.connection(), snapshot_uid, limit)
    }

    fn list_directory_groups(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<AgentDirectoryGroup>, AgentStorageError> {
        // MODULE-MODEL-1 D2(i): the directory TOPOLOGY read backing the package
        // groups. Body lives beside `module_sizes` in the `agent_orient_reads`
        // discovery module (same `>500`-line guardrail rationale).
        crate::agent_orient_reads::directory_groups(self.connection(), snapshot_uid)
    }

    fn list_manifest_roots(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<repo_graph_agent::ManifestRoot>, AgentStorageError> {
        // MODULE-MODEL-2 §13 D4: per-toolchain crate/package roots (already-stored
        // module_candidates ⋈ evidence.source_type). Body beside the other
        // discovery reads in `agent_orient_reads`.
        crate::agent_orient_reads::manifest_roots(self.connection(), snapshot_uid)
    }

    fn get_boundary_links_freshness(
        &self,
        snapshot_uid: &str,
    ) -> Result<AgentBoundaryLinksFreshness, AgentStorageError> {
        let conn = self.connection();

        // Query freshness state counts from boundary_interaction_links.
        // This is a freshness-tracked L2 table (ACR-3/4/5).
        let mut stmt = conn
            .prepare(
                "SELECT freshness_state, COUNT(*) as cnt \
				 FROM boundary_interaction_links \
				 WHERE snapshot_uid = ? \
				 GROUP BY freshness_state",
            )
            .map_err(map_err("get_boundary_links_freshness"))?;

        let rows = stmt
            .query_map([snapshot_uid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(map_err("get_boundary_links_freshness"))?;

        let mut current: u64 = 0;
        let mut impacted: u64 = 0;
        let mut unknown: u64 = 0;

        for row in rows {
            let (state, count) = row.map_err(map_err("get_boundary_links_freshness"))?;
            let count = count as u64;
            match state.as_str() {
                "current" => current += count,
                "impacted" => impacted += count,
                "unknown" => unknown += count,
                "stale" => {
                    // Treat stale as impacted for summary purposes.
                    // Stale is a stronger form of impacted.
                    impacted += count;
                }
                _ => {}
            }
        }

        let total = current + impacted + unknown;

        // Query earliest impacted_at timestamp if there are impacted rows.
        let earliest_impacted_at = if impacted > 0 {
            conn.query_row(
                "SELECT MIN(freshness_updated_at) \
				 FROM boundary_interaction_links \
				 WHERE snapshot_uid = ? \
				   AND freshness_state IN ('impacted', 'stale')",
                [snapshot_uid],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(map_err("get_boundary_links_freshness"))?
        } else {
            None
        };

        Ok(AgentBoundaryLinksFreshness {
            total,
            current,
            impacted,
            unknown,
            earliest_impacted_at,
        })
    }
}
