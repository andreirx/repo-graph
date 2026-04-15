//! Shared helpers for `repo-graph-agent` integration tests.
//!
//! Provides a `FakeAgentStorage` that implements
//! `AgentStorageRead` entirely in-memory with hand-seeded data.
//! Use-case tests drive the orient pipeline through this fake
//! rather than through a real `StorageConnection`, so the tests:
//!
//!   - exercise the port boundary (no leaking of storage-crate
//!     types)
//!   - run without a SQLite dependency (no temp files, no
//!     rusqlite bundled into the agent dev dependencies)
//!   - stay deterministic and fast (no migrations, no index
//!     orchestration, no live pipeline)
//!
//! The fake supports every method on the trait. Each method
//! either returns the seeded vector for that key or returns
//! empty / None / the default summary. Errors can be injected
//! by setting `force_error_on` to the operation name.

use std::cell::RefCell;
use std::collections::HashMap;

use repo_graph_agent::{
	AgentBoundaryDeclaration, AgentCycle, AgentDeadNode, AgentImportEdge,
	AgentRepo, AgentRepoSummary, AgentSnapshot, AgentStaleFile,
	AgentStorageError, AgentStorageRead, AgentTrustSummary,
};

#[derive(Default)]
pub struct FakeAgentStorage {
	pub repos: HashMap<String, AgentRepo>,
	pub snapshots: HashMap<String, AgentSnapshot>,
	pub stale_files: HashMap<String, Vec<AgentStaleFile>>,
	pub cycles: HashMap<String, Vec<AgentCycle>>,
	pub dead_nodes: HashMap<String, Vec<AgentDeadNode>>,
	pub boundary_declarations: HashMap<String, Vec<AgentBoundaryDeclaration>>,
	pub imports_between_paths:
		HashMap<(String, String, String), Vec<AgentImportEdge>>,
	pub repo_summaries: HashMap<String, AgentRepoSummary>,
	pub trust_summaries: HashMap<String, AgentTrustSummary>,
	/// If set to the name of a port operation, the fake returns
	/// `AgentStorageError` from that operation. Used to verify
	/// error propagation.
	pub force_error_on: RefCell<Option<&'static str>>,
}

impl FakeAgentStorage {
	pub fn new() -> Self {
		Self::default()
	}

	/// Convenience: seed a minimal repo + snapshot pair with all
	/// aggregator inputs set to neutral defaults. Returns the
	/// `snapshot_uid` for the caller to reference.
	pub fn seed_minimal_repo(
		&mut self,
		repo_uid: &str,
		repo_name: &str,
		snapshot_uid: &str,
	) {
		self.repos.insert(
			repo_uid.to_string(),
			AgentRepo {
				repo_uid: repo_uid.to_string(),
				name: repo_name.to_string(),
			},
		);
		self.snapshots.insert(
			repo_uid.to_string(),
			AgentSnapshot {
				snapshot_uid: snapshot_uid.to_string(),
				repo_uid: repo_uid.to_string(),
				scope: "full".to_string(),
				basis_commit: None,
				created_at: "2026-04-15T00:00:00Z".to_string(),
				files_total: 0,
				nodes_total: 0,
				edges_total: 0,
			},
		);
		self.repo_summaries.insert(
			snapshot_uid.to_string(),
			AgentRepoSummary {
				file_count: 0,
				symbol_count: 0,
				languages: Vec::new(),
			},
		);
		self.trust_summaries.insert(
			snapshot_uid.to_string(),
			AgentTrustSummary {
				// Default: high resolution, enrichment applied.
				call_resolution_rate: 0.90,
				resolved_calls: 90,
				unresolved_calls: 10,
				enrichment_applied: true,
				enrichment_eligible: 10,
				enrichment_enriched: 9,
			},
		);
	}

	fn fail_if_forced(
		&self,
		operation: &'static str,
	) -> Result<(), AgentStorageError> {
		if let Some(op) = *self.force_error_on.borrow() {
			if op == operation {
				return Err(AgentStorageError::new(
					operation,
					format!("forced failure on {}", operation),
				));
			}
		}
		Ok(())
	}
}

impl AgentStorageRead for FakeAgentStorage {
	fn get_repo(
		&self,
		repo_uid: &str,
	) -> Result<Option<AgentRepo>, AgentStorageError> {
		self.fail_if_forced("get_repo")?;
		Ok(self.repos.get(repo_uid).cloned())
	}

	fn get_latest_snapshot(
		&self,
		repo_uid: &str,
	) -> Result<Option<AgentSnapshot>, AgentStorageError> {
		self.fail_if_forced("get_latest_snapshot")?;
		Ok(self.snapshots.get(repo_uid).cloned())
	}

	fn get_stale_files(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<AgentStaleFile>, AgentStorageError> {
		self.fail_if_forced("get_stale_files")?;
		Ok(self.stale_files.get(snapshot_uid).cloned().unwrap_or_default())
	}

	fn find_module_cycles(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<AgentCycle>, AgentStorageError> {
		self.fail_if_forced("find_module_cycles")?;
		Ok(self.cycles.get(snapshot_uid).cloned().unwrap_or_default())
	}

	fn find_dead_nodes(
		&self,
		snapshot_uid: &str,
		_repo_uid: &str,
		_kind_filter: Option<&str>,
	) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
		self.fail_if_forced("find_dead_nodes")?;
		Ok(self.dead_nodes.get(snapshot_uid).cloned().unwrap_or_default())
	}

	fn get_active_boundary_declarations(
		&self,
		repo_uid: &str,
	) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
		self.fail_if_forced("get_active_boundary_declarations")?;
		Ok(self
			.boundary_declarations
			.get(repo_uid)
			.cloned()
			.unwrap_or_default())
	}

	fn find_imports_between_paths(
		&self,
		snapshot_uid: &str,
		source_prefix: &str,
		target_prefix: &str,
	) -> Result<Vec<AgentImportEdge>, AgentStorageError> {
		self.fail_if_forced("find_imports_between_paths")?;
		let key = (
			snapshot_uid.to_string(),
			source_prefix.to_string(),
			target_prefix.to_string(),
		);
		Ok(self.imports_between_paths.get(&key).cloned().unwrap_or_default())
	}

	fn compute_repo_summary(
		&self,
		snapshot_uid: &str,
	) -> Result<AgentRepoSummary, AgentStorageError> {
		self.fail_if_forced("compute_repo_summary")?;
		Ok(self
			.repo_summaries
			.get(snapshot_uid)
			.cloned()
			.unwrap_or(AgentRepoSummary {
				file_count: 0,
				symbol_count: 0,
				languages: Vec::new(),
			}))
	}

	fn get_trust_summary(
		&self,
		_repo_uid: &str,
		snapshot_uid: &str,
	) -> Result<AgentTrustSummary, AgentStorageError> {
		self.fail_if_forced("get_trust_summary")?;
		Ok(self
			.trust_summaries
			.get(snapshot_uid)
			.cloned()
			.unwrap_or(AgentTrustSummary {
				call_resolution_rate: 1.0,
				resolved_calls: 0,
				unresolved_calls: 0,
				enrichment_applied: false,
				enrichment_eligible: 0,
				enrichment_enriched: 0,
			}))
	}
}
