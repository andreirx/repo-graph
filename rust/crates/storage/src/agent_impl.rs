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
	AgentBoundaryDeclaration, AgentCycle, AgentDeadNode, AgentImportEdge,
	AgentReliabilityAxis, AgentReliabilityLevel, AgentRepo, AgentRepoSummary,
	AgentSnapshot, AgentStaleFile, AgentStorageError, AgentStorageRead,
	AgentTrustSummary, EnrichmentState,
};
use repo_graph_trust::service::assemble_trust_report;
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
fn map_err<E: std::fmt::Display>(
	operation: &'static str,
) -> impl FnOnce(E) -> AgentStorageError {
	move |e| AgentStorageError::new(operation, e.to_string())
}

// ── Impl ─────────────────────────────────────────────────────────

impl AgentStorageRead for StorageConnection {
	fn get_repo(
		&self,
		repo_uid: &str,
	) -> Result<Option<AgentRepo>, AgentStorageError> {
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

	fn find_module_cycles(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<AgentCycle>, AgentStorageError> {
		// Storage's `find_cycles` takes a level param; we always
		// use the module level at the agent boundary.
		let cycles = self
			.find_cycles(snapshot_uid, "module")
			.map_err(map_err("find_module_cycles"))?;
		Ok(cycles
			.into_iter()
			.map(|c| AgentCycle {
				length: c.length,
				modules: c.nodes.into_iter().map(|n| n.name).collect(),
			})
			.collect())
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
		// Delegate to the trust crate's assembly function, which
		// uses the existing `TrustStorageRead` impl on
		// `StorageConnection`. basis_commit and toolchain_json
		// are not needed by the agent projection and are passed
		// as None.
		let report = assemble_trust_report(
			self,
			repo_uid,
			snapshot_uid,
			None,
			None,
		)
		.map_err(|e| {
			AgentStorageError::new("get_trust_summary", format!("{:?}", e))
		})?;

		// Project the relevant scalars into the agent DTO.
		let resolved_calls = report.summary.resolved_calls;
		let unresolved_calls = report.summary.unresolved_calls;
		let call_resolution_rate = report.summary.call_resolution_rate;

		// Reliability axes (Rust-43 F1/F3 fix). The agent
		// crate uses `dead_code_reliability.level` as the
		// authoritative gate for the DEAD_CODE signal; it does
		// NOT re-derive thresholds. `call_graph_reliability`
		// is projected too for symmetry and future confidence
		// rules.
		let call_graph_reliability =
			map_axis(&report.summary.reliability.call_graph);
		let dead_code_reliability =
			map_axis(&report.summary.reliability.dead_code);

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
		let (enrichment_state, enrichment_eligible, enrichment_enriched) =
			match &report.enrichment_status {
				None if report.enrichment_eligible_count == 0 => {
					(EnrichmentState::NotApplicable, 0, 0)
				}
				None => (
					EnrichmentState::NotRun,
					report.enrichment_eligible_count,
					0,
				),
				Some(es) if es.eligible == 0 => {
					(EnrichmentState::NotApplicable, 0, es.enriched)
				}
				Some(es) => (EnrichmentState::Ran, es.eligible, es.enriched),
			};

		Ok(AgentTrustSummary {
			call_resolution_rate,
			resolved_calls,
			unresolved_calls,
			call_graph_reliability,
			dead_code_reliability,
			enrichment_state,
			enrichment_eligible,
			enrichment_enriched,
		})
	}
}
