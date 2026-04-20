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
	AgentBoundaryDeclaration, AgentCalleeRow, AgentCallerRow, AgentCycle,
	AgentDeadNode, AgentFileEntry, AgentFocusCandidate, AgentImportEdge,
	AgentImportEntry, AgentPathResolution, AgentReliabilityAxis,
	AgentReliabilityLevel, AgentRepo, AgentRepoSummary, AgentSnapshot,
	AgentStaleFile, AgentStorageError, AgentStorageRead, AgentSymbolContext,
	AgentSymbolEntry, AgentTrustSummary, EnrichmentState,
};
use repo_graph_gate::{
	GateBoundaryDeclaration, GateImportEdge, GateInference, GateMeasurement,
	GateModuleViolationEvidence, GateRequirement, GateStorageError, GateStorageRead,
	GateWaiver,
};

/// Shared fixed "now" for all agent integration tests.
///
/// Using a constant makes waiver expiry tests deterministic and
/// prevents test drift as the real clock advances. Every test
/// that does not care about expiry passes `TEST_NOW`. Expiry
/// tests pass specific values relative to this anchor.
pub const TEST_NOW: &str = "2026-04-15T00:00:00Z";

/// Build a "high confidence" `AgentTrustSummary` fixture.
///
/// Default baseline that every existing test implicitly relies
/// on. Call resolution rate is 90%, the call-graph and
/// dead-code reliability axes are both High with empty reason
/// vectors, and the enrichment phase is `Ran` with 9-of-10
/// edges resolved. Tests that want to exercise low-reliability
/// paths construct their own `AgentTrustSummary` directly.
///
/// NOT part of the `repo-graph-agent` public surface. This is
/// a test-only helper in the shared `common` module so
/// production code does not grow fixture constructors.
pub fn high_confidence_trust() -> AgentTrustSummary {
	AgentTrustSummary {
		call_resolution_rate: 0.90,
		resolved_calls: 90,
		unresolved_calls: 10,
		call_graph_reliability: AgentReliabilityAxis {
			level: AgentReliabilityLevel::High,
			reasons: Vec::new(),
		},
		dead_code_reliability: AgentReliabilityAxis {
			level: AgentReliabilityLevel::High,
			reasons: Vec::new(),
		},
		enrichment_state: EnrichmentState::Ran,
		enrichment_eligible: 10,
		enrichment_enriched: 9,
	}
}

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

	// ── Focus-resolution seed data (Rust-44) ────────────────
	//
	// Each new port method has its own seed field keyed by
	// (snapshot_uid, path/key) or (repo_uid, prefix).
	pub path_resolutions: HashMap<(String, String), AgentPathResolution>,
	pub stable_key_candidates: HashMap<(String, String), AgentFocusCandidate>,
	pub dead_nodes_in_path: HashMap<(String, String), Vec<AgentDeadNode>>,
	pub dead_nodes_in_file: HashMap<(String, String), Vec<AgentDeadNode>>,
	pub path_summaries: HashMap<(String, String), AgentRepoSummary>,
	pub file_summaries: HashMap<(String, String), AgentRepoSummary>,
	pub boundary_declarations_in_path:
		HashMap<(String, String), Vec<AgentBoundaryDeclaration>>,
	pub cycles_involving_path: HashMap<(String, String), Vec<AgentCycle>>,

	// ── Symbol-focus seed data (Rust-45) ────────────────────
	pub symbol_name_results: HashMap<(String, String), Vec<AgentFocusCandidate>>,
	pub symbol_contexts: HashMap<(String, String), AgentSymbolContext>,
	pub symbol_callers: HashMap<(String, String), Vec<AgentCallerRow>>,
	pub symbol_callees: HashMap<(String, String), Vec<AgentCalleeRow>>,
	pub cycles_involving_module: HashMap<(String, String), Vec<AgentCycle>>,

	// ── Explain-focus seed data ─────────────────────────────
	pub symbols_in_file: HashMap<(String, String), Vec<AgentSymbolEntry>>,
	pub files_in_path: HashMap<(String, String), Vec<AgentFileEntry>>,
	pub file_imports: HashMap<(String, String), Vec<AgentImportEntry>>,

	// ── Gate-port seed data (Rust-43A) ──────────────────────
	//
	// The fake implements BOTH `AgentStorageRead` and
	// `GateStorageRead` because the orient pipeline's trait
	// bound is `AgentStorageRead + GateStorageRead`. Each gate
	// port method has its own seed field so tests can drive
	// the gate aggregator independently of the agent port
	// seeds.
	pub gate_requirements: HashMap<String, Vec<GateRequirement>>,
	pub gate_boundary_declarations: HashMap<String, Vec<GateBoundaryDeclaration>>,
	pub gate_boundary_imports:
		HashMap<(String, String, String), Vec<GateImportEdge>>,
	pub gate_coverage: HashMap<String, Vec<GateMeasurement>>,
	pub gate_complexity: HashMap<String, Vec<GateMeasurement>>,
	pub gate_hotspots: HashMap<String, Vec<GateInference>>,
	pub gate_waivers: HashMap<(String, String, i64, String), Vec<GateWaiver>>,

	/// If set to the name of a port operation, the fake returns
	/// `AgentStorageError` from that operation. Used to verify
	/// error propagation. Shared between both traits — operation
	/// identifiers are globally unique.
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
			high_confidence_trust(),
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
		// Default fallback when a test forgets to seed a trust
		// summary. Uses `high_confidence_trust()` so the
		// reliability axes are High — the Rust-43 F1 fix
		// added the reliability gate on DEAD_CODE emission,
		// and a fallback with Low reliability would break
		// every existing test that does not care about the
		// dead-code path. Tests that exercise Low reliability
		// seed their own `AgentTrustSummary` directly.
		Ok(self
			.trust_summaries
			.get(snapshot_uid)
			.cloned()
			.unwrap_or_else(high_confidence_trust))
	}

	// ── Focus-resolution methods (Rust-44) ──────────────────

	fn resolve_path_focus(
		&self,
		snapshot_uid: &str,
		path: &str,
	) -> Result<AgentPathResolution, AgentStorageError> {
		self.fail_if_forced("resolve_path_focus")?;
		let key = (snapshot_uid.to_string(), path.to_string());
		Ok(self
			.path_resolutions
			.get(&key)
			.cloned()
			.unwrap_or(AgentPathResolution {
				has_exact_file: false,
				file_stable_key: None,
				has_content_under_prefix: false,
				module_stable_key: None,
			}))
	}

	fn resolve_stable_key_focus(
		&self,
		snapshot_uid: &str,
		stable_key: &str,
	) -> Result<Option<AgentFocusCandidate>, AgentStorageError> {
		self.fail_if_forced("resolve_stable_key_focus")?;
		let key = (snapshot_uid.to_string(), stable_key.to_string());
		Ok(self.stable_key_candidates.get(&key).cloned())
	}

	fn find_dead_nodes_in_path(
		&self,
		snapshot_uid: &str,
		_repo_uid: &str,
		path_prefix: &str,
	) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
		self.fail_if_forced("find_dead_nodes_in_path")?;
		let key = (snapshot_uid.to_string(), path_prefix.to_string());
		Ok(self.dead_nodes_in_path.get(&key).cloned().unwrap_or_default())
	}

	fn find_dead_nodes_in_file(
		&self,
		snapshot_uid: &str,
		_repo_uid: &str,
		file_path: &str,
	) -> Result<Vec<AgentDeadNode>, AgentStorageError> {
		self.fail_if_forced("find_dead_nodes_in_file")?;
		let key = (snapshot_uid.to_string(), file_path.to_string());
		Ok(self.dead_nodes_in_file.get(&key).cloned().unwrap_or_default())
	}

	fn compute_path_summary(
		&self,
		snapshot_uid: &str,
		path_prefix: &str,
	) -> Result<AgentRepoSummary, AgentStorageError> {
		self.fail_if_forced("compute_path_summary")?;
		let key = (snapshot_uid.to_string(), path_prefix.to_string());
		Ok(self
			.path_summaries
			.get(&key)
			.cloned()
			.unwrap_or(AgentRepoSummary {
				file_count: 0,
				symbol_count: 0,
				languages: Vec::new(),
			}))
	}

	fn compute_file_summary(
		&self,
		snapshot_uid: &str,
		file_path: &str,
	) -> Result<AgentRepoSummary, AgentStorageError> {
		self.fail_if_forced("compute_file_summary")?;
		let key = (snapshot_uid.to_string(), file_path.to_string());
		Ok(self
			.file_summaries
			.get(&key)
			.cloned()
			.unwrap_or(AgentRepoSummary {
				file_count: 0,
				symbol_count: 0,
				languages: Vec::new(),
			}))
	}

	fn find_boundary_declarations_in_path(
		&self,
		repo_uid: &str,
		path_prefix: &str,
	) -> Result<Vec<AgentBoundaryDeclaration>, AgentStorageError> {
		self.fail_if_forced("find_boundary_declarations_in_path")?;
		let key = (repo_uid.to_string(), path_prefix.to_string());
		Ok(self
			.boundary_declarations_in_path
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	fn find_cycles_involving_path(
		&self,
		snapshot_uid: &str,
		path_prefix: &str,
	) -> Result<Vec<AgentCycle>, AgentStorageError> {
		self.fail_if_forced("find_cycles_involving_path")?;
		let key = (snapshot_uid.to_string(), path_prefix.to_string());
		Ok(self
			.cycles_involving_path
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	// ── Symbol-focus methods (Rust-45) ──────────────────────

	fn resolve_symbol_name(
		&self,
		snapshot_uid: &str,
		name: &str,
	) -> Result<Vec<AgentFocusCandidate>, AgentStorageError> {
		self.fail_if_forced("resolve_symbol_name")?;
		let key = (snapshot_uid.to_string(), name.to_string());
		Ok(self
			.symbol_name_results
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	fn get_symbol_context(
		&self,
		snapshot_uid: &str,
		symbol_stable_key: &str,
	) -> Result<Option<AgentSymbolContext>, AgentStorageError> {
		self.fail_if_forced("get_symbol_context")?;
		let key = (snapshot_uid.to_string(), symbol_stable_key.to_string());
		Ok(self.symbol_contexts.get(&key).cloned())
	}

	fn find_symbol_callers(
		&self,
		snapshot_uid: &str,
		symbol_stable_key: &str,
	) -> Result<Vec<AgentCallerRow>, AgentStorageError> {
		self.fail_if_forced("find_symbol_callers")?;
		let key = (snapshot_uid.to_string(), symbol_stable_key.to_string());
		Ok(self
			.symbol_callers
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	fn find_symbol_callees(
		&self,
		snapshot_uid: &str,
		symbol_stable_key: &str,
	) -> Result<Vec<AgentCalleeRow>, AgentStorageError> {
		self.fail_if_forced("find_symbol_callees")?;
		let key = (snapshot_uid.to_string(), symbol_stable_key.to_string());
		Ok(self
			.symbol_callees
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	fn find_cycles_involving_module(
		&self,
		snapshot_uid: &str,
		module_qualified_name: &str,
	) -> Result<Vec<AgentCycle>, AgentStorageError> {
		self.fail_if_forced("find_cycles_involving_module")?;
		let key = (snapshot_uid.to_string(), module_qualified_name.to_string());
		Ok(self
			.cycles_involving_module
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	// ── Explain-focus methods ──────────────────────────────────

	fn list_symbols_in_file(
		&self,
		snapshot_uid: &str,
		file_path: &str,
	) -> Result<Vec<AgentSymbolEntry>, AgentStorageError> {
		self.fail_if_forced("list_symbols_in_file")?;
		let key = (snapshot_uid.to_string(), file_path.to_string());
		Ok(self
			.symbols_in_file
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	fn list_files_in_path(
		&self,
		snapshot_uid: &str,
		path_prefix: &str,
	) -> Result<Vec<AgentFileEntry>, AgentStorageError> {
		self.fail_if_forced("list_files_in_path")?;
		let key = (snapshot_uid.to_string(), path_prefix.to_string());
		Ok(self
			.files_in_path
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	fn find_file_imports(
		&self,
		snapshot_uid: &str,
		file_path: &str,
	) -> Result<Vec<AgentImportEntry>, AgentStorageError> {
		self.fail_if_forced("find_file_imports")?;
		let key = (snapshot_uid.to_string(), file_path.to_string());
		Ok(self
			.file_imports
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}
}

// ── Gate port impl (Rust-43A) ────────────────────────────────────
//
// Same fake type, distinct trait impl block for clarity. Each
// method follows the same shape as the agent impl above:
// check `force_error_on`, then look up in the gate_* seed map,
// return cloned value or default empty.
//
// The gate port uses its own error type (`GateStorageError`),
// distinct from `AgentStorageError`. This mirrors production
// where the gate storage adapter and the agent storage adapter
// produce distinct error types even though they both wrap
// `StorageError` internally.

impl FakeAgentStorage {
	fn fail_if_forced_gate(
		&self,
		operation: &'static str,
	) -> Result<(), GateStorageError> {
		if let Some(op) = *self.force_error_on.borrow() {
			if op == operation {
				return Err(GateStorageError::new(
					operation,
					format!("forced failure on {}", operation),
				));
			}
		}
		Ok(())
	}
}

impl GateStorageRead for FakeAgentStorage {
	fn get_active_requirements(
		&self,
		repo_uid: &str,
	) -> Result<Vec<GateRequirement>, GateStorageError> {
		self.fail_if_forced_gate("get_active_requirements")?;
		Ok(self
			.gate_requirements
			.get(repo_uid)
			.cloned()
			.unwrap_or_default())
	}

	fn get_boundary_declarations(
		&self,
		repo_uid: &str,
	) -> Result<Vec<GateBoundaryDeclaration>, GateStorageError> {
		self.fail_if_forced_gate("get_boundary_declarations")?;
		Ok(self
			.gate_boundary_declarations
			.get(repo_uid)
			.cloned()
			.unwrap_or_default())
	}

	fn find_boundary_imports(
		&self,
		snapshot_uid: &str,
		source_prefix: &str,
		target_prefix: &str,
	) -> Result<Vec<GateImportEdge>, GateStorageError> {
		self.fail_if_forced_gate("find_boundary_imports")?;
		let key = (
			snapshot_uid.to_string(),
			source_prefix.to_string(),
			target_prefix.to_string(),
		);
		Ok(self
			.gate_boundary_imports
			.get(&key)
			.cloned()
			.unwrap_or_default())
	}

	fn get_coverage_measurements(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateMeasurement>, GateStorageError> {
		self.fail_if_forced_gate("get_coverage_measurements")?;
		Ok(self.gate_coverage.get(snapshot_uid).cloned().unwrap_or_default())
	}

	fn get_complexity_measurements(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateMeasurement>, GateStorageError> {
		self.fail_if_forced_gate("get_complexity_measurements")?;
		Ok(self
			.gate_complexity
			.get(snapshot_uid)
			.cloned()
			.unwrap_or_default())
	}

	fn get_hotspot_inferences(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<GateInference>, GateStorageError> {
		self.fail_if_forced_gate("get_hotspot_inferences")?;
		Ok(self.gate_hotspots.get(snapshot_uid).cloned().unwrap_or_default())
	}

	fn find_waivers(
		&self,
		repo_uid: &str,
		req_id: &str,
		req_version: i64,
		obligation_id: &str,
		now: &str,
	) -> Result<Vec<GateWaiver>, GateStorageError> {
		self.fail_if_forced_gate("find_waivers")?;
		let key = (
			repo_uid.to_string(),
			req_id.to_string(),
			req_version,
			obligation_id.to_string(),
		);
		let all = self
			.gate_waivers
			.get(&key)
			.cloned()
			.unwrap_or_default();

		// Apply expiry filtering exactly like the real SQL
		// path in `StorageConnection::find_active_waivers`:
		//
		//   WHERE expires_at IS NULL OR expires_at > now
		//
		// Comparison is lexicographic over ISO 8601 strings.
		// Waivers with no `expires_at` are always active.
		// Waivers with `expires_at > now` are still active.
		// Waivers with `expires_at <= now` are filtered out.
		//
		// The P2 review identified that the earlier fake
		// ignored `now` entirely, which masked the orient
		// sentinel bug. Real filtering here is how the
		// agent-side tests catch any future regression.
		let active: Vec<GateWaiver> = all
			.into_iter()
			.filter(|w| match &w.expires_at {
				None => true,
				Some(exp) => exp.as_str() > now,
			})
			.collect();
		Ok(active)
	}

	fn evaluate_module_violations(
		&self,
		_repo_uid: &str,
		_snapshot_uid: &str,
	) -> Result<GateModuleViolationEvidence, GateStorageError> {
		self.fail_if_forced_gate("evaluate_module_violations")?;
		// Default: no module violations. Tests that need violations
		// should set up explicit module data in FakeAgentStorage.
		Ok(GateModuleViolationEvidence::default())
	}
}
