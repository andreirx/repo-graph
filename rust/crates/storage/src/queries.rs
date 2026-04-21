//! Read-side graph queries — callers, callees, etc.
//!
//! These are query methods on `StorageConnection` that serve the
//! CLI read-side surface. Separate from CRUD (write-side) and
//! from trait impls (indexer/trust port implementations).
//!
//! Rust-10: `resolve_symbol` + `find_direct_callers`.
//! Rust-11: `find_direct_callees`.
//! Rust-12: `find_dead_nodes`.
//! Rust-13: `find_cycles`.
//! Rust-14: `compute_module_stats`.
//! Rust-18: `node_exists` + `find_imports`.
//! Rust-19: `find_shortest_path`.
//! Rust-22: `get_active_boundary_declarations` + `find_imports_between_paths`.
//! Rust-24: `get_active_requirement_declarations`.
//! Rust-25: `find_active_waivers`.
//! Rust-27: `query_measurements_by_kind`.
//! Rust-30: `query_inferences_by_kind`.
//! SB-5: `resolve_resource` + `find_resource_readers` + `find_resource_writers`;
//!       `find_dead_nodes` excludes resource kinds (FS_PATH, DB_RESOURCE, BLOB, STATE+CACHE).

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::connection::StorageConnection;
use crate::error::StorageError;

// ── Query DTOs ───────────────────────────────────────────────────

/// A resolved symbol from a symbol lookup query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSymbol {
	pub stable_key: String,
	pub name: String,
	pub qualified_name: Option<String>,
	pub kind: String,
	pub subtype: Option<String>,
	pub file: Option<String>,
	pub line: Option<i64>,
	pub column: Option<i64>,
}

/// A direct caller of a symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallerResult {
	pub stable_key: String,
	pub name: String,
	pub qualified_name: Option<String>,
	pub kind: String,
	pub subtype: Option<String>,
	pub file: Option<String>,
	pub line: Option<i64>,
	pub column: Option<i64>,
	pub edge_type: String,
	pub resolution: String,
}

/// A direct callee of a symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalleeResult {
	pub stable_key: String,
	pub name: String,
	pub qualified_name: Option<String>,
	pub kind: String,
	pub subtype: Option<String>,
	pub file: Option<String>,
	pub line: Option<i64>,
	pub column: Option<i64>,
	pub edge_type: String,
	pub resolution: String,
}

/// An import result matching the TS `formatNodeResult` wire format.
///
/// Field names match the TS JSON output (snake_case via `formatNodeResult`):
/// `node_id`, `symbol`, `kind`, `subtype`, `file`, `line`, `column`,
/// `edge_type`, `resolution`, `evidence`, `depth`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportResult {
	pub node_id: String,
	/// `qualified_name` if present, else `name`.
	pub symbol: String,
	pub kind: String,
	pub subtype: Option<String>,
	/// File path, or empty string if no file.
	pub file: String,
	pub line: Option<i64>,
	pub column: Option<i64>,
	pub edge_type: Option<String>,
	pub resolution: Option<String>,
	/// Extractor evidence (e.g. `["ts-core:0.2.0"]`).
	pub evidence: Vec<String>,
	pub depth: i64,
}

/// Result of a shortest-path search between two symbols.
///
/// Matches the TS `formatPathResult` wire format (json.ts:74-86).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathResult {
	pub found: bool,
	pub path_length: i64,
	pub path: Vec<PathStep>,
}

/// A single step in a shortest-path result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathStep {
	pub node_id: String,
	/// `qualified_name` if present, else `name`.
	pub symbol: String,
	/// File path, or empty string if no file.
	pub file: String,
	pub line: Option<i64>,
	/// Edge type that led to this step. Empty string for the start node.
	pub edge_type: String,
}

/// A boundary declaration read from the declarations table.
///
/// Only the fields needed by the violations command are exposed.
/// The `value_json` is parsed into `forbids` and `reason` at the
/// storage layer so raw JSON does not leak to the CLI.
#[derive(Debug, Clone)]
pub struct BoundaryDeclaration {
	/// Module path extracted from target_stable_key.
	pub boundary_module: String,
	/// The forbidden module path (from value_json.forbids).
	pub forbids: String,
	/// Optional reason (from value_json.reason).
	pub reason: Option<String>,
}

/// An IMPORTS edge between two file path prefixes.
#[derive(Debug, Clone)]
pub struct ImportEdgeResult {
	pub source_file: String,
	pub target_file: String,
	pub line: Option<i64>,
}

/// A boundary violation: an IMPORTS edge crossing a declared boundary.
///
/// Matches the TS `formatViolationJson` wire format (arch.ts:147-156).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryViolation {
	pub boundary_module: String,
	pub forbidden_module: String,
	pub reason: Option<String>,
	pub source_file: String,
	pub target_file: String,
	pub line: Option<i64>,
}

/// A parsed requirement declaration with its verification obligations.
#[derive(Debug, Clone)]
pub struct RequirementDeclaration {
	pub req_id: String,
	pub version: i64,
	pub obligations: Vec<VerificationObligation>,
}

/// A single verification obligation from a requirement declaration.
#[derive(Debug, Clone)]
pub struct VerificationObligation {
	pub obligation_id: String,
	pub obligation: String,
	pub method: String,
	pub target: Option<String>,
	pub threshold: Option<f64>,
	pub operator: Option<String>,
}

/// An active, non-expired waiver declaration.
///
/// Parsed from the `declarations` table where `kind='waiver'`.
/// Fields are extracted from `value_json` at the storage layer so
/// raw JSON does not leak to the gate evaluator.
#[derive(Debug, Clone)]
pub struct WaiverDeclaration {
	/// The `declaration_uid` of the waiver row.
	pub declaration_uid: String,
	pub req_id: String,
	pub requirement_version: i64,
	pub obligation_id: String,
	pub reason: String,
	pub created_at: String,
	pub created_by: Option<String>,
	pub expires_at: Option<String>,
	pub rationale_category: Option<String>,
	pub policy_basis: Option<String>,
}

/// A measurement row read from the `measurements` table.
///
/// Mirrors TS `queryMeasurementsByKind` return shape:
/// `{ targetStableKey: string, valueJson: string }`.
/// The `value_json` is opaque at this layer — interpretation
/// belongs to the evaluator (gate method or CLI command).
#[derive(Debug, Clone)]
pub struct MeasurementRow {
	pub target_stable_key: String,
	pub value_json: String,
}

/// Per-file summed complexity from symbol measurements.
///
/// RS-MS-3: Joins measurements → nodes → files to aggregate
/// cyclomatic_complexity measurements by file.
#[derive(Debug, Clone)]
pub struct FileComplexityRow {
	pub file_path: String,
	pub sum_complexity: u64,
}

/// An inference row read from the `inferences` table.
///
/// Mirrors TS `InferenceRow` from `core/ports/storage.ts`, but
/// only carries the fields needed by gate evaluators.
/// The `value_json` is opaque at this layer.
#[derive(Debug, Clone)]
pub struct InferenceRow {
	pub target_stable_key: String,
	pub value_json: String,
}

/// A node with no incoming reference edges (dead code candidate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadNodeResult {
	pub stable_key: String,
	/// `qualified_name` if present, else `name`.
	pub symbol: String,
	pub kind: String,
	pub subtype: Option<String>,
	pub file: Option<String>,
	pub line: Option<i64>,
	pub line_count: Option<i64>,
	pub is_test: bool,
}

/// A single cycle (circular dependency path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleResult {
	pub cycle_id: String,
	/// Number of nodes (and edges) in the ring.
	pub length: usize,
	pub nodes: Vec<CycleNode>,
}

/// A node participating in a cycle.
///
/// The `file` field is always `None` for MODULE-level cycles
/// (MODULE nodes have no associated file). Matches TS behavior
/// where `file` is always `null` for cycle nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleNode {
	pub node_id: String,
	pub name: String,
	pub file: Option<String>,
}

/// Per-module structural metrics.
///
/// Field names match the TS CLI JSON output (snake_case).
/// Fractional metrics are rounded to 2 decimal places using
/// `(x * 100.0).round() / 100.0`, mirroring TS `Math.round(x * 100) / 100`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleStatsResult {
	/// Module path (repo-relative directory).
	pub module: String,
	pub fan_in: i64,
	pub fan_out: i64,
	pub instability: f64,
	pub abstractness: f64,
	pub distance_from_main_sequence: f64,
	pub file_count: i64,
	pub symbol_count: i64,
}

/// A resolved resource node from a resource lookup query.
///
/// SB-5: Used by `rmap resource readers/writers` commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedResource {
	pub stable_key: String,
	pub name: String,
	pub kind: String,
	pub subtype: Option<String>,
}

/// A symbol that accesses a resource (reader or writer).
///
/// SB-5: Used by `rmap resource readers/writers` commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAccessorResult {
	pub stable_key: String,
	pub name: String,
	pub qualified_name: Option<String>,
	pub kind: String,
	pub subtype: Option<String>,
	pub file: Option<String>,
	pub line: Option<i64>,
	pub column: Option<i64>,
	pub edge_type: String,
	pub resolution: String,
}

/// Error when resolving a symbol query.
#[derive(Debug)]
pub enum SymbolResolveError {
	/// No match found.
	NotFound,
	/// Multiple matches at the name/qualified_name level.
	Ambiguous(Vec<String>),
	/// Storage error.
	Storage(StorageError),
}

impl std::fmt::Display for SymbolResolveError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::NotFound => write!(f, "symbol not found"),
			Self::Ambiguous(keys) => {
				write!(f, "ambiguous symbol, matches: {}", keys.join(", "))
			}
			Self::Storage(e) => write!(f, "storage error: {}", e),
		}
	}
}

/// Error when resolving a resource query.
///
/// SB-5: Resource resolution is exact stable_key only. Unlike symbol
/// resolution, there is no name/qualified_name fallback.
#[derive(Debug)]
pub enum ResourceResolveError {
	/// No match found for the given stable_key.
	NotFound,
	/// The stable_key exists but does not refer to a resource node.
	/// Contains the actual kind found.
	NotAResource(String),
	/// Storage error.
	Storage(StorageError),
}

impl std::fmt::Display for ResourceResolveError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::NotFound => write!(f, "resource not found"),
			Self::NotAResource(kind) => {
				write!(f, "not a resource node (kind: {})", kind)
			}
			Self::Storage(e) => write!(f, "storage error: {}", e),
		}
	}
}

// ── Query methods ────────────────────────────────────────────────

impl StorageConnection {
	/// Resolve a symbol query to a single `ResolvedSymbol`.
	///
	/// Resolution order (all exact match, no LIKE):
	///   1. `stable_key` — direct identity match
	///   2. `qualified_name` — exact match
	///   3. `name` — exact match
	///
	/// Returns `NotFound` if zero matches.
	/// Returns `Ambiguous` if > 1 match at steps 2 or 3.
	pub fn resolve_symbol(
		&self,
		snapshot_uid: &str,
		query: &str,
	) -> Result<ResolvedSymbol, SymbolResolveError> {
		// Step 1: exact stable_key.
		if let Some(sym) = self
			.query_symbol_by_field(snapshot_uid, "stable_key", query)
			.map_err(SymbolResolveError::Storage)?
		{
			return Ok(sym);
		}

		// Step 2: exact qualified_name.
		let by_qn = self
			.query_symbols_by_field(snapshot_uid, "qualified_name", query)
			.map_err(SymbolResolveError::Storage)?;
		if by_qn.len() == 1 {
			return Ok(by_qn.into_iter().next().unwrap());
		}
		if by_qn.len() > 1 {
			let keys: Vec<String> = by_qn.iter().map(|s| s.stable_key.clone()).collect();
			return Err(SymbolResolveError::Ambiguous(keys));
		}

		// Step 3: exact name.
		let by_name = self
			.query_symbols_by_field(snapshot_uid, "name", query)
			.map_err(SymbolResolveError::Storage)?;
		if by_name.len() == 1 {
			return Ok(by_name.into_iter().next().unwrap());
		}
		if by_name.len() > 1 {
			let keys: Vec<String> = by_name.iter().map(|s| s.stable_key.clone()).collect();
			return Err(SymbolResolveError::Ambiguous(keys));
		}

		Err(SymbolResolveError::NotFound)
	}

	/// Find direct callers of a symbol (one hop).
	///
	/// `edge_types` controls which edge kinds are traversed.
	/// Typical values: `&["CALLS"]` or `&["CALLS", "INSTANTIATES"]`.
	pub fn find_direct_callers(
		&self,
		snapshot_uid: &str,
		target_stable_key: &str,
		edge_types: &[&str],
	) -> Result<Vec<CallerResult>, StorageError> {
		let placeholders = edge_types.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
		let sql = format!(
			"SELECT
				n.stable_key, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start, n.col_start,
				e.type AS edge_type, e.resolution
			 FROM edges e
			 JOIN nodes target_n ON e.target_node_uid = target_n.node_uid
			 JOIN nodes n ON e.source_node_uid = n.node_uid
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE target_n.snapshot_uid = ?
			   AND target_n.stable_key = ?
			   AND e.snapshot_uid = ?
			   AND e.type IN ({placeholders})
			 ORDER BY n.name ASC, f.path ASC"
		);

		let mut stmt = self.connection().prepare(&sql)?;

		// Build params: snapshot_uid, target_stable_key, snapshot_uid, then edge types.
		let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
		params.push(Box::new(snapshot_uid.to_string()));
		params.push(Box::new(target_stable_key.to_string()));
		params.push(Box::new(snapshot_uid.to_string()));
		for et in edge_types {
			params.push(Box::new(et.to_string()));
		}
		let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

		let rows = stmt.query_map(
			param_refs.as_slice(),
			|row| {
				Ok(CallerResult {
					stable_key: row.get(0)?,
					name: row.get(1)?,
					qualified_name: row.get(2)?,
					kind: row.get(3)?,
					subtype: row.get(4)?,
					file: row.get(5)?,
					line: row.get(6)?,
					column: row.get(7)?,
					edge_type: row.get(8)?,
					resolution: row.get(9)?,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	/// Find direct callees of a symbol (one hop).
	///
	/// Symmetric reverse of `find_direct_callers`: the given symbol
	/// is the source node, returned nodes are the targets.
	/// `edge_types` controls which edge kinds are traversed.
	pub fn find_direct_callees(
		&self,
		snapshot_uid: &str,
		source_stable_key: &str,
		edge_types: &[&str],
	) -> Result<Vec<CalleeResult>, StorageError> {
		let placeholders = edge_types.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
		let sql = format!(
			"SELECT
				n.stable_key, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start, n.col_start,
				e.type AS edge_type, e.resolution
			 FROM edges e
			 JOIN nodes source_n ON e.source_node_uid = source_n.node_uid
			 JOIN nodes n ON e.target_node_uid = n.node_uid
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE source_n.snapshot_uid = ?
			   AND source_n.stable_key = ?
			   AND e.snapshot_uid = ?
			   AND e.type IN ({placeholders})
			 ORDER BY n.name ASC, f.path ASC"
		);

		let mut stmt = self.connection().prepare(&sql)?;

		let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
		params.push(Box::new(snapshot_uid.to_string()));
		params.push(Box::new(source_stable_key.to_string()));
		params.push(Box::new(snapshot_uid.to_string()));
		for et in edge_types {
			params.push(Box::new(et.to_string()));
		}
		let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

		let rows = stmt.query_map(
			param_refs.as_slice(),
			|row| {
				Ok(CalleeResult {
					stable_key: row.get(0)?,
					name: row.get(1)?,
					qualified_name: row.get(2)?,
					kind: row.get(3)?,
					subtype: row.get(4)?,
					file: row.get(5)?,
					line: row.get(6)?,
					column: row.get(7)?,
					edge_type: row.get(8)?,
					resolution: row.get(9)?,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	// ── Resource queries (SB-5) ──────────────────────────────────────

	/// Resolve a resource stable_key to a `ResolvedResource`.
	///
	/// Resolution is exact stable_key match only (no name fallback).
	/// Returns `NotAResource` if the node exists but is not a resource
	/// kind (FS_PATH, DB_RESOURCE, BLOB, or STATE+CACHE).
	pub fn resolve_resource(
		&self,
		snapshot_uid: &str,
		stable_key: &str,
	) -> Result<ResolvedResource, ResourceResolveError> {
		let sql = "SELECT stable_key, name, kind, subtype
		           FROM nodes
		           WHERE snapshot_uid = ?
		             AND stable_key = ?";
		let mut stmt = self
			.connection()
			.prepare(sql)
			.map_err(|e| ResourceResolveError::Storage(e.into()))?;

		let result: Option<(String, String, String, Option<String>)> = stmt
			.query_row(rusqlite::params![snapshot_uid, stable_key], |row| {
				Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
			})
			.optional()
			.map_err(|e| ResourceResolveError::Storage(e.into()))?;

		let Some((sk, name, kind, subtype)) = result else {
			return Err(ResourceResolveError::NotFound);
		};

		// Validate resource kind.
		let is_resource = matches!(kind.as_str(), "FS_PATH" | "DB_RESOURCE" | "BLOB")
			|| (kind == "STATE" && subtype.as_deref() == Some("CACHE"));

		if !is_resource {
			return Err(ResourceResolveError::NotAResource(kind));
		}

		Ok(ResolvedResource {
			stable_key: sk,
			name,
			kind,
			subtype,
		})
	}

	/// Find symbols that read from a resource (READS edges to the resource).
	///
	/// Returns symbols (source nodes) that have READS edges pointing to
	/// the given resource stable_key.
	pub fn find_resource_readers(
		&self,
		snapshot_uid: &str,
		resource_stable_key: &str,
	) -> Result<Vec<ResourceAccessorResult>, StorageError> {
		let sql = "SELECT
				n.stable_key, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start, n.col_start,
				e.type AS edge_type, e.resolution
			 FROM edges e
			 JOIN nodes target_n ON e.target_node_uid = target_n.node_uid
			 JOIN nodes n ON e.source_node_uid = n.node_uid
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE target_n.snapshot_uid = ?
			   AND target_n.stable_key = ?
			   AND e.snapshot_uid = ?
			   AND e.type = 'READS'
			   AND n.kind = 'SYMBOL'
			 ORDER BY n.name ASC, f.path ASC";

		let mut stmt = self.connection().prepare(sql)?;
		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, resource_stable_key, snapshot_uid],
			|row| {
				Ok(ResourceAccessorResult {
					stable_key: row.get(0)?,
					name: row.get(1)?,
					qualified_name: row.get(2)?,
					kind: row.get(3)?,
					subtype: row.get(4)?,
					file: row.get(5)?,
					line: row.get(6)?,
					column: row.get(7)?,
					edge_type: row.get(8)?,
					resolution: row.get(9)?,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	/// Find symbols that write to a resource (WRITES edges to the resource).
	///
	/// Returns symbols (source nodes) that have WRITES edges pointing to
	/// the given resource stable_key.
	pub fn find_resource_writers(
		&self,
		snapshot_uid: &str,
		resource_stable_key: &str,
	) -> Result<Vec<ResourceAccessorResult>, StorageError> {
		let sql = "SELECT
				n.stable_key, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start, n.col_start,
				e.type AS edge_type, e.resolution
			 FROM edges e
			 JOIN nodes target_n ON e.target_node_uid = target_n.node_uid
			 JOIN nodes n ON e.source_node_uid = n.node_uid
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE target_n.snapshot_uid = ?
			   AND target_n.stable_key = ?
			   AND e.snapshot_uid = ?
			   AND e.type = 'WRITES'
			   AND n.kind = 'SYMBOL'
			 ORDER BY n.name ASC, f.path ASC";

		let mut stmt = self.connection().prepare(sql)?;
		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, resource_stable_key, snapshot_uid],
			|row| {
				Ok(ResourceAccessorResult {
					stable_key: row.get(0)?,
					name: row.get(1)?,
					qualified_name: row.get(2)?,
					kind: row.get(3)?,
					subtype: row.get(4)?,
					file: row.get(5)?,
					line: row.get(6)?,
					column: row.get(7)?,
					edge_type: row.get(8)?,
					resolution: row.get(9)?,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	/// Find dead nodes — nodes with no incoming reference edges.
	///
	/// Based on the TS `findDeadNodes` algorithm with one intentional
	/// divergence (SB-5): resource kinds are excluded.
	///
	/// Steps:
	///   1. Select all nodes in the snapshot.
	///   2. Exclude nodes that are targets of reference edges
	///      (IMPORTS, CALLS, IMPLEMENTS, INSTANTIATES, ROUTES_TO,
	///      REGISTERED_BY, TESTED_BY, COVERS).
	///   3. Exclude declared entrypoints (declarations table).
	///   4. Exclude framework-liveness inferences.
	///   5. **SB-5 divergence**: Exclude resource kinds (FS_PATH,
	///      DB_RESOURCE, BLOB, STATE+CACHE). The TS implementation
	///      does not yet have this exclusion.
	///   6. Optional: filter by node kind (e.g., "SYMBOL").
	///   7. ORDER BY name ASC.
	///
	/// The declarations and inferences subqueries operate on tables
	/// that exist in every Rust-migrated DB (created by migration 001).
	/// When no declarations or inferences are present (typical for
	/// Rust-indexed DBs), those subqueries return empty sets and the
	/// exclusions are no-ops.
	pub fn find_dead_nodes(
		&self,
		snapshot_uid: &str,
		repo_uid: &str,
		kind_filter: Option<&str>,
	) -> Result<Vec<DeadNodeResult>, StorageError> {
		let kind_clause = if kind_filter.is_some() {
			"AND n.kind = ?3"
		} else {
			""
		};

		let sql = format!(
			"SELECT
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
			   AND n.node_uid NOT IN (
			     SELECT e.target_node_uid FROM edges e
			     WHERE e.snapshot_uid = ?1
			       AND e.type IN ('IMPORTS', 'CALLS', 'IMPLEMENTS', 'INSTANTIATES',
			                      'ROUTES_TO', 'REGISTERED_BY', 'TESTED_BY', 'COVERS')
			   )
			   {kind_clause}
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
			   AND n.kind NOT IN ('FS_PATH', 'DB_RESOURCE', 'BLOB')
			   AND NOT (n.kind = 'STATE' AND n.subtype = 'CACHE')
			 ORDER BY n.name ASC"
		);

		let mut stmt = self.connection().prepare(&sql)?;

		let rows = if let Some(kind) = kind_filter {
			stmt.query_map(
				rusqlite::params![snapshot_uid, repo_uid, kind],
				Self::map_dead_node_row,
			)?
		} else {
			stmt.query_map(
				rusqlite::params![snapshot_uid, repo_uid],
				Self::map_dead_node_row,
			)?
		};

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	fn map_dead_node_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeadNodeResult> {
		let name: String = row.get(1)?;
		let qualified_name: Option<String> = row.get(2)?;
		let is_test_int: i64 = row.get(8)?;
		Ok(DeadNodeResult {
			stable_key: row.get(0)?,
			symbol: qualified_name.unwrap_or(name),
			kind: row.get(3)?,
			subtype: row.get(4)?,
			file: row.get(5)?,
			line: row.get(6)?,
			line_count: row.get(7)?,
			is_test: is_test_int != 0,
		})
	}

	/// Find module-level dependency cycles via IMPORTS edges.
	///
	/// Mirrors the TS `findCycles` algorithm (sqlite-storage.ts:2511-2585):
	///   1. Recursive CTE walks IMPORTS edges between MODULE nodes.
	///   2. Detects when a path returns to its starting node.
	///   3. Post-processes: canonicalize each cycle by rotating to put
	///      the lexicographically smallest UID first, then deduplicate.
	///
	/// The `level` parameter selects the node kind: "module" → MODULE,
	/// "file" → FILE. Default is "module" (matching TS default).
	pub fn find_cycles(
		&self,
		snapshot_uid: &str,
		level: &str,
	) -> Result<Vec<CycleResult>, StorageError> {
		let node_kind = match level {
			"file" => "FILE",
			_ => "MODULE",
		};

		let mut stmt = self.connection().prepare(
			"WITH RECURSIVE cycle_search(start_uid, current_uid, path, is_cycle) AS (
				SELECT n.node_uid, n.node_uid, n.node_uid, 0
				FROM nodes n
				WHERE n.snapshot_uid = ? AND n.kind = ?

				UNION ALL

				SELECT cs.start_uid, e.target_node_uid,
				       cs.path || ' -> ' || e.target_node_uid,
				       CASE WHEN e.target_node_uid = cs.start_uid THEN 1 ELSE 0 END
				FROM edges e
				JOIN cycle_search cs ON e.source_node_uid = cs.current_uid
				WHERE e.snapshot_uid = ?
				  AND e.type = 'IMPORTS'
				  AND cs.is_cycle = 0
				  AND (cs.path NOT LIKE '%' || e.target_node_uid || '%'
				       OR e.target_node_uid = cs.start_uid)
			)
			SELECT DISTINCT path FROM cycle_search
			WHERE is_cycle = 1
			ORDER BY path",
		)?;

		let raw_paths: Vec<String> = stmt
			.query_map(
				rusqlite::params![snapshot_uid, node_kind, snapshot_uid],
				|row| row.get(0),
			)?
			.collect::<Result<Vec<_>, _>>()?;

		// Canonicalize and deduplicate (same algorithm as TS).
		let mut seen = std::collections::HashSet::new();
		let mut results = Vec::new();

		for path in &raw_paths {
			let uids: Vec<&str> = path.split(" -> ").collect();
			// The path includes the start node repeated at the end;
			// remove it to get the unique ring members.
			let ring = &uids[..uids.len().saturating_sub(1)];
			if ring.is_empty() {
				continue;
			}

			let canonical = canonicalize_cycle(ring);
			if !seen.insert(canonical.clone()) {
				continue;
			}

			let canonical_uids: Vec<&str> = canonical.split(',').collect();

			// Look up names for each node in the cycle.
			let nodes: Vec<CycleNode> = canonical_uids
				.iter()
				.map(|uid| {
					let name: String = self
						.connection()
						.query_row(
							"SELECT name FROM nodes WHERE node_uid = ?",
							rusqlite::params![uid],
							|row| row.get(0),
						)
						.unwrap_or_else(|_| uid.to_string());
					CycleNode {
						node_id: uid.to_string(),
						name,
						file: None,
					}
				})
				.collect();

			results.push(CycleResult {
				cycle_id: format!("cycle-{}", results.len() + 1),
				length: canonical_uids.len(),
				nodes,
			});
		}

		Ok(results)
	}

	/// Compute per-module structural metrics.
	///
	/// Mirrors the TS `computeModuleStats` (sqlite-storage.ts:2846-2964).
	/// Single SQL query with correlated subqueries for symbol/type counts,
	/// LEFT JOINs for fan-in/fan-out/file counts, and Rust-side rounding.
	///
	/// Only modules with `file_count > 0` are included (matches TS).
	/// Ordered by module path (qualified_name) ascending.
	pub fn compute_module_stats(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<ModuleStatsResult>, StorageError> {
		let mut stmt = self.connection().prepare(
			"SELECT
			   m.qualified_name AS path,
			   COALESCE(fan_in.cnt, 0) AS fan_in,
			   COALESCE(fan_out.cnt, 0) AS fan_out,
			   COALESCE(files.cnt, 0) AS file_count,
			   (SELECT COUNT(*) FROM nodes n
			    WHERE n.snapshot_uid = ?1
			      AND n.kind = 'SYMBOL' AND n.visibility = 'export'
			      AND n.file_uid IN (
			        SELECT tgt.file_uid FROM edges oe
			        JOIN nodes tgt ON oe.target_node_uid = tgt.node_uid
			        WHERE oe.snapshot_uid = ?1 AND oe.type = 'OWNS'
			          AND oe.source_node_uid = m.node_uid
			      )
			   ) AS symbol_count,
			   (SELECT COUNT(*) FROM nodes n
			    WHERE n.snapshot_uid = ?1
			      AND n.kind = 'SYMBOL'
			      AND n.subtype IN ('INTERFACE', 'TYPE_ALIAS')
			      AND n.parent_node_uid IS NULL
			      AND n.file_uid IN (
			        SELECT tgt.file_uid FROM edges oe
			        JOIN nodes tgt ON oe.target_node_uid = tgt.node_uid
			        WHERE oe.snapshot_uid = ?1 AND oe.type = 'OWNS'
			          AND oe.source_node_uid = m.node_uid
			      )
			   ) AS abstract_count,
			   (SELECT COUNT(*) FROM nodes n
			    WHERE n.snapshot_uid = ?1
			      AND n.kind = 'SYMBOL'
			      AND n.subtype IN ('INTERFACE', 'TYPE_ALIAS', 'CLASS', 'ENUM')
			      AND n.parent_node_uid IS NULL
			      AND n.file_uid IN (
			        SELECT tgt.file_uid FROM edges oe
			        JOIN nodes tgt ON oe.target_node_uid = tgt.node_uid
			        WHERE oe.snapshot_uid = ?1 AND oe.type = 'OWNS'
			          AND oe.source_node_uid = m.node_uid
			      )
			   ) AS type_count
			 FROM nodes m
			 LEFT JOIN (
			   SELECT target_node_uid AS nid, COUNT(DISTINCT source_node_uid) AS cnt
			   FROM edges
			   WHERE snapshot_uid = ?1 AND type = 'IMPORTS'
			     AND source_node_uid IN (SELECT node_uid FROM nodes WHERE snapshot_uid = ?1 AND kind = 'MODULE')
			   GROUP BY target_node_uid
			 ) fan_in ON fan_in.nid = m.node_uid
			 LEFT JOIN (
			   SELECT source_node_uid AS nid, COUNT(DISTINCT target_node_uid) AS cnt
			   FROM edges
			   WHERE snapshot_uid = ?1 AND type = 'IMPORTS'
			     AND target_node_uid IN (SELECT node_uid FROM nodes WHERE snapshot_uid = ?1 AND kind = 'MODULE')
			   GROUP BY source_node_uid
			 ) fan_out ON fan_out.nid = m.node_uid
			 LEFT JOIN (
			   SELECT source_node_uid AS nid, COUNT(*) AS cnt
			   FROM edges
			   WHERE snapshot_uid = ?1 AND type = 'OWNS'
			   GROUP BY source_node_uid
			 ) files ON files.nid = m.node_uid
			 WHERE m.snapshot_uid = ?1 AND m.kind = 'MODULE'
			   AND COALESCE(files.cnt, 0) > 0
			 ORDER BY m.qualified_name",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid],
			|row| {
				let path: String = row.get(0)?;
				let fan_in: i64 = row.get(1)?;
				let fan_out: i64 = row.get(2)?;
				let file_count: i64 = row.get(3)?;
				let symbol_count: i64 = row.get(4)?;
				let abstract_count: i64 = row.get(5)?;
				let type_count: i64 = row.get(6)?;
				Ok((path, fan_in, fan_out, file_count, symbol_count, abstract_count, type_count))
			},
		)?;

		let mut results = Vec::new();
		for row in rows {
			let (path, fan_in, fan_out, file_count, symbol_count, abstract_count, type_count) =
				row.map_err(StorageError::from)?;

			let total = fan_in + fan_out;
			let instability_raw = if total > 0 {
				fan_out as f64 / total as f64
			} else {
				0.0
			};
			let abstractness_raw = if type_count > 0 {
				abstract_count as f64 / type_count as f64
			} else {
				0.0
			};
			let distance_raw = (abstractness_raw + instability_raw - 1.0).abs();

			// Round to 2 decimal places: mirrors TS Math.round(x * 100) / 100.
			let instability = (instability_raw * 100.0).round() / 100.0;
			let abstractness = (abstractness_raw * 100.0).round() / 100.0;
			let distance = (distance_raw * 100.0).round() / 100.0;

			results.push(ModuleStatsResult {
				module: path,
				fan_in,
				fan_out,
				instability,
				abstractness,
				distance_from_main_sequence: distance,
				file_count,
				symbol_count,
			});
		}

		Ok(results)
	}

	/// Find direct imports of a file (one hop, IMPORTS edges only).
	///
	/// Returns `ImportResult` items matching the TS `formatNodeResult`
	/// wire format, including `node_id`, `symbol`, `evidence`, and `depth`.
	pub fn find_imports(
		&self,
		snapshot_uid: &str,
		source_stable_key: &str,
	) -> Result<Vec<ImportResult>, StorageError> {
		let mut stmt = self.connection().prepare(
			"SELECT
				n.node_uid, n.name, n.qualified_name, n.kind, n.subtype,
				f.path AS file_path, n.line_start, n.col_start,
				e.type AS edge_type, e.resolution, e.extractor
			 FROM edges e
			 JOIN nodes source_n ON e.source_node_uid = source_n.node_uid
			 JOIN nodes n ON e.target_node_uid = n.node_uid
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE source_n.snapshot_uid = ?
			   AND source_n.stable_key = ?
			   AND e.snapshot_uid = ?
			   AND e.type = 'IMPORTS'
			 ORDER BY n.name ASC, f.path ASC",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, source_stable_key, snapshot_uid],
			|row| {
				let name: String = row.get(1)?;
				let qualified_name: Option<String> = row.get(2)?;
				let file_path: Option<String> = row.get(5)?;
				let extractor: Option<String> = row.get(10)?;
				Ok(ImportResult {
					node_id: row.get(0)?,
					symbol: qualified_name.unwrap_or(name),
					kind: row.get(3)?,
					subtype: row.get(4)?,
					file: file_path.unwrap_or_default(),
					line: row.get(6)?,
					column: row.get(7)?,
					edge_type: row.get(8)?,
					resolution: row.get(9)?,
					evidence: extractor.into_iter().collect(),
					depth: 1,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	/// Find the shortest path between two nodes via BFS.
	///
	/// Mirrors the TS `findPath` (sqlite-storage.ts:2335-2432).
	/// Uses a recursive CTE bounded by `max_depth`. Edge types
	/// are fixed to CALLS and IMPORTS. Returns `PathResult` with
	/// `found: false` if no path exists within the depth bound.
	///
	/// Both `from_stable_key` and `to_stable_key` must be valid
	/// node stable keys (caller is responsible for resolution).
	pub fn find_shortest_path(
		&self,
		snapshot_uid: &str,
		from_stable_key: &str,
		to_stable_key: &str,
		max_depth: i64,
	) -> Result<PathResult, StorageError> {
		// Resolve stable keys to node UIDs.
		let from_uid: Option<String> = self
			.connection()
			.query_row(
				"SELECT node_uid FROM nodes WHERE snapshot_uid = ? AND stable_key = ?",
				rusqlite::params![snapshot_uid, from_stable_key],
				|row| row.get(0),
			)
			.ok();

		let to_uid: Option<String> = self
			.connection()
			.query_row(
				"SELECT node_uid FROM nodes WHERE snapshot_uid = ? AND stable_key = ?",
				rusqlite::params![snapshot_uid, to_stable_key],
				|row| row.get(0),
			)
			.ok();

		let (from_uid, to_uid) = match (from_uid, to_uid) {
			(Some(f), Some(t)) => (f, t),
			_ => {
				return Ok(PathResult {
					found: false,
					path_length: 0,
					path: Vec::new(),
				});
			}
		};

		let mut stmt = self.connection().prepare(
			"WITH RECURSIVE path_search(node_uid, depth, path_uids, path_edges) AS (
				SELECT ?, 0, ?, ''

				UNION ALL

				SELECT e.target_node_uid, p.depth + 1,
				       p.path_uids || ',' || e.target_node_uid,
				       p.path_edges || CASE WHEN p.path_edges = '' THEN '' ELSE '|' END
				         || e.source_node_uid || ':' || e.type || ':' || e.target_node_uid
				FROM edges e
				JOIN path_search p ON e.source_node_uid = p.node_uid
				WHERE e.snapshot_uid = ?
				  AND e.type IN ('CALLS', 'IMPORTS')
				  AND p.depth < ?
				  AND p.path_uids NOT LIKE '%' || e.target_node_uid || '%'
			)
			SELECT path_uids, path_edges, depth
			FROM path_search
			WHERE node_uid = ?
			ORDER BY depth ASC
			LIMIT 1",
		)?;

		let row: Option<(String, String, i64)> = stmt
			.query_row(
				rusqlite::params![&from_uid, &from_uid, snapshot_uid, max_depth, &to_uid],
				|row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
			)
			.ok();

		let (path_uids_str, path_edges_str, depth) = match row {
			Some(r) => r,
			None => {
				return Ok(PathResult {
					found: false,
					path_length: 0,
					path: Vec::new(),
				});
			}
		};

		// Parse path UIDs and look up each node.
		let path_uids: Vec<&str> = path_uids_str.split(',').filter(|s| !s.is_empty()).collect();
		let mut steps: Vec<PathStep> = Vec::new();

		for uid in &path_uids {
			let node: Option<(String, Option<String>, Option<String>, Option<i64>)> = self
				.connection()
				.query_row(
					"SELECT n.name, n.qualified_name, f.path, n.line_start
					 FROM nodes n
					 LEFT JOIN files f ON n.file_uid = f.file_uid
					 WHERE n.node_uid = ?",
					rusqlite::params![uid],
					|row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
				)
				.ok();

			if let Some((name, qualified_name, file_path, line)) = node {
				steps.push(PathStep {
					node_id: uid.to_string(),
					symbol: qualified_name.unwrap_or(name),
					file: file_path.unwrap_or_default(),
					line,
					edge_type: String::new(), // filled below
				});
			}
		}

		// Fill edge types from path_edges string.
		// Format: "source_uid:EDGE_TYPE:target_uid|source_uid:EDGE_TYPE:target_uid|..."
		let edge_segments: Vec<&str> = path_edges_str.split('|').filter(|s| !s.is_empty()).collect();
		for (i, segment) in edge_segments.iter().enumerate() {
			if i + 1 < steps.len() {
				let parts: Vec<&str> = segment.split(':').collect();
				if parts.len() >= 2 {
					steps[i + 1].edge_type = parts[1].to_string();
				}
			}
		}

		Ok(PathResult {
			found: true,
			path_length: depth,
			path: steps,
		})
	}

	/// Check whether a node with the given stable_key exists in a snapshot.
	///
	/// Used by the `imports` command to verify the FILE node exists
	/// before querying its outgoing IMPORTS edges.
	pub fn node_exists(
		&self,
		snapshot_uid: &str,
		stable_key: &str,
	) -> Result<bool, StorageError> {
		let exists: bool = self.connection().query_row(
			"SELECT EXISTS(SELECT 1 FROM nodes WHERE snapshot_uid = ? AND stable_key = ?)",
			rusqlite::params![snapshot_uid, stable_key],
			|row| row.get(0),
		)?;
		Ok(exists)
	}

	/// Load active boundary declarations for a repo.
	///
	/// Reads from the `declarations` table where `kind='boundary'`
	/// and `is_active=1`. Parses `value_json` to extract `forbids`
	/// and `reason`. Extracts the module path from
	/// `target_stable_key` (format: `{repo}:{path}:MODULE`).
	///
	/// Rows that fail to parse or lack a MODULE stable key are
	/// silently skipped (defensive, matches TS behavior where the
	/// regex match filters non-MODULE keys).
	pub fn get_active_boundary_declarations(
		&self,
		repo_uid: &str,
	) -> Result<Vec<BoundaryDeclaration>, StorageError> {
		let mut stmt = self.connection().prepare(
			"SELECT target_stable_key, value_json
			 FROM declarations
			 WHERE repo_uid = ? AND kind = 'boundary' AND is_active = 1
			 ORDER BY created_at DESC",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![repo_uid],
			|row| {
				let stable_key: String = row.get(0)?;
				let value_json: String = row.get(1)?;
				Ok((stable_key, value_json))
			},
		)?;

		let mut results = Vec::new();
		for row in rows {
			let (stable_key, value_json) = row.map_err(StorageError::from)?;

			// Extract module path from stable key: {repo}:{path}:MODULE
			let module_path = match extract_module_path(&stable_key) {
				Some(p) => p,
				None => continue,
			};

			// Parse value_json for forbids and reason.
			let parsed: serde_json::Value = match serde_json::from_str(&value_json) {
				Ok(v) => v,
				Err(_) => continue,
			};
			let forbids = match parsed["forbids"].as_str() {
				Some(f) => f.to_string(),
				None => continue,
			};
			let reason = parsed["reason"].as_str().map(|s| s.to_string());

			results.push(BoundaryDeclaration {
				boundary_module: module_path,
				forbids,
				reason,
			});
		}

		Ok(results)
	}

	/// Find IMPORTS edges where the source file is under `source_prefix`
	/// and the target file is under `target_prefix`.
	///
	/// Mirrors TS `findImportsBetweenPaths` (sqlite-storage.ts:2589-2628).
	/// Uses `LIKE '{prefix}/%'` matching on file paths.
	/// Ordered by source file path, then line number.
	/// Load active requirement declarations for a repo.
	///
	/// Reads from the `declarations` table where `kind='requirement'`
	/// and `is_active=1`. Parses `value_json` to extract `req_id`,
	/// `version`, and `verification` obligations.
	///
	/// Requirements with empty or missing `verification` arrays are
	/// skipped (matches TS: `if (!val.verification) continue`).
	///
	/// Active requirements with unparseable `value_json` or missing
	/// `req_id` are errors (not silently skipped) because they are
	/// authored governance inputs that should not vanish from the
	/// gate report.
	pub fn get_active_requirement_declarations(
		&self,
		repo_uid: &str,
	) -> Result<Vec<RequirementDeclaration>, StorageError> {
		let mut stmt = self.connection().prepare(
			"SELECT declaration_uid, value_json
			 FROM declarations
			 WHERE repo_uid = ? AND kind = 'requirement' AND is_active = 1
			 ORDER BY created_at DESC",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![repo_uid],
			|row| {
				let uid: String = row.get(0)?;
				let value_json: String = row.get(1)?;
				Ok((uid, value_json))
			},
		)?;

		let mut results = Vec::new();
		for row in rows {
			let (decl_uid, value_json) = row.map_err(StorageError::from)?;

			let parsed: serde_json::Value = serde_json::from_str(&value_json)
				.map_err(|e| StorageError::MalformedRequirement {
					declaration_uid: decl_uid.clone(),
					reason: format!("malformed value_json: {}", e),
				})?;

			let req_id = parsed["req_id"]
				.as_str()
				.ok_or_else(|| StorageError::MalformedRequirement {
					declaration_uid: decl_uid.clone(),
					reason: "missing req_id".to_string(),
				})?
				.to_string();

			let version = parsed["version"].as_i64().unwrap_or(1);

			// Empty/missing verification → skip (matches TS behavior).
			let verification = match parsed["verification"].as_array() {
				Some(arr) if !arr.is_empty() => arr,
				_ => continue,
			};

			let mut obligations = Vec::new();
			for (i, obl_val) in verification.iter().enumerate() {
				let obligation_id = obl_val["obligation_id"]
					.as_str()
					.ok_or_else(|| StorageError::MalformedRequirement {
						declaration_uid: decl_uid.clone(),
						reason: format!("obligation {} missing obligation_id", i),
					})?
					.to_string();
				let obligation = obl_val["obligation"]
					.as_str()
					.ok_or_else(|| StorageError::MalformedRequirement {
						declaration_uid: decl_uid.clone(),
						reason: format!("obligation {} missing obligation text", i),
					})?
					.to_string();
				let method = obl_val["method"]
					.as_str()
					.ok_or_else(|| StorageError::MalformedRequirement {
						declaration_uid: decl_uid.clone(),
						reason: format!("obligation {} missing method", i),
					})?
					.to_string();
				obligations.push(VerificationObligation {
					obligation_id,
					obligation,
					method,
					target: obl_val["target"].as_str().map(|s| s.to_string()),
					threshold: obl_val["threshold"].as_f64(),
					operator: obl_val["operator"].as_str().map(|s| s.to_string()),
				});
			}

			results.push(RequirementDeclaration {
				req_id,
				version,
				obligations,
			});
		}

		Ok(results)
	}

	/// Find active, non-expired waivers matching the given tuple.
	///
	/// Mirrors TS `findActiveWaivers` (sqlite-storage.ts). Uses
	/// `json_extract` on `value_json` for the 3-tuple filter
	/// (req_id, requirement_version, obligation_id) and expiry
	/// comparison.
	///
	/// Returns waivers ordered by `created_at DESC` (most recent
	/// first). When multiple waivers match, the caller should use
	/// the first entry.
	///
	/// Expiry semantics: lexicographic ISO 8601 comparison.
	/// Waivers with `expires_at IS NULL` are perpetual.
	/// Waivers with `expires_at <= now` are excluded.
	pub fn find_active_waivers(
		&self,
		repo_uid: &str,
		req_id: &str,
		requirement_version: i64,
		obligation_id: &str,
		now: &str,
	) -> Result<Vec<WaiverDeclaration>, StorageError> {
		let mut stmt = self.connection().prepare(
			"SELECT declaration_uid, value_json
			 FROM declarations
			 WHERE repo_uid = ?
			   AND kind = 'waiver'
			   AND is_active = 1
			   AND (json_extract(value_json, '$.expires_at') IS NULL
			        OR json_extract(value_json, '$.expires_at') > ?)
			   AND json_extract(value_json, '$.req_id') = ?
			   AND json_extract(value_json, '$.requirement_version') = ?
			   AND json_extract(value_json, '$.obligation_id') = ?
			 ORDER BY created_at DESC",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![repo_uid, now, req_id, requirement_version, obligation_id],
			|row| {
				let uid: String = row.get(0)?;
				let value_json: String = row.get(1)?;
				Ok((uid, value_json))
			},
		)?;

		let mut results = Vec::new();
		for row in rows {
			let (declaration_uid, value_json) = row.map_err(StorageError::from)?;

			let parsed: serde_json::Value = serde_json::from_str(&value_json)
				.map_err(|e| StorageError::MalformedWaiver {
					declaration_uid: declaration_uid.clone(),
					reason: format!("malformed value_json: {}", e),
				})?;

			let reason = parsed["reason"]
				.as_str()
				.ok_or_else(|| StorageError::MalformedWaiver {
					declaration_uid: declaration_uid.clone(),
					reason: "missing required field: reason".to_string(),
				})?
				.to_string();

			let created_at = parsed["created_at"]
				.as_str()
				.ok_or_else(|| StorageError::MalformedWaiver {
					declaration_uid: declaration_uid.clone(),
					reason: "missing required field: created_at".to_string(),
				})?
				.to_string();

			let waiver_req_id = parsed["req_id"]
				.as_str()
				.ok_or_else(|| StorageError::MalformedWaiver {
					declaration_uid: declaration_uid.clone(),
					reason: "missing required field: req_id".to_string(),
				})?
				.to_string();

			let waiver_version = parsed["requirement_version"]
				.as_i64()
				.ok_or_else(|| StorageError::MalformedWaiver {
					declaration_uid: declaration_uid.clone(),
					reason: "missing or non-integer field: requirement_version".to_string(),
				})?;

			let waiver_obl_id = parsed["obligation_id"]
				.as_str()
				.ok_or_else(|| StorageError::MalformedWaiver {
					declaration_uid: declaration_uid.clone(),
					reason: "missing required field: obligation_id".to_string(),
				})?
				.to_string();

			let created_by = parsed["created_by"].as_str().map(|s| s.to_string());
			let expires_at = parsed["expires_at"].as_str().map(|s| s.to_string());
			let rationale_category = parsed["rationale_category"].as_str().map(|s| s.to_string());
			let policy_basis = parsed["policy_basis"].as_str().map(|s| s.to_string());

			results.push(WaiverDeclaration {
				declaration_uid,
				req_id: waiver_req_id,
				requirement_version: waiver_version,
				obligation_id: waiver_obl_id,
				reason,
				created_at,
				created_by,
				expires_at,
				rationale_category,
				policy_basis,
			});
		}

		Ok(results)
	}

	/// Read measurements by kind for a snapshot.
	///
	/// Mirrors TS `queryMeasurementsByKind` (sqlite-storage.ts).
	/// Returns all measurement rows matching the snapshot and kind,
	/// with `target_stable_key` and raw `value_json` only.
	///
	/// No ordering guarantee — TS does not specify one either.
	/// Callers that need deterministic order must sort the result.
	pub fn query_measurements_by_kind(
		&self,
		snapshot_uid: &str,
		kind: &str,
	) -> Result<Vec<MeasurementRow>, StorageError> {
		let mut stmt = self.connection().prepare(
			"SELECT target_stable_key, value_json
			 FROM measurements
			 WHERE snapshot_uid = ? AND kind = ?",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, kind],
			|row| {
				Ok(MeasurementRow {
					target_stable_key: row.get(0)?,
					value_json: row.get(1)?,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	/// Read inferences by kind for a snapshot.
	///
	/// Mirrors TS `queryInferences` (sqlite-storage.ts).
	/// Returns `target_stable_key` and raw `value_json` for each
	/// matching inference row. No ordering guarantee — the TS
	/// implementation orders by `normalized_score DESC` but that is
	/// a query-side optimization, not a contract the caller depends
	/// on (the gate evaluator scans for max regardless).
	pub fn query_inferences_by_kind(
		&self,
		snapshot_uid: &str,
		kind: &str,
	) -> Result<Vec<InferenceRow>, StorageError> {
		let mut stmt = self.connection().prepare(
			"SELECT target_stable_key, value_json
			 FROM inferences
			 WHERE snapshot_uid = ? AND kind = ?",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, kind],
			|row| {
				Ok(InferenceRow {
					target_stable_key: row.get(0)?,
					value_json: row.get(1)?,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	/// Sum cyclomatic_complexity measurements per file via proper join.
	///
	/// RS-MS-3a: Joins measurements → nodes → files to aggregate
	/// symbol-level complexity measurements by containing file. This
	/// avoids parsing stable_key strings (which have format
	/// `{repo}:{path}#{symbol}:SYMBOL:{kind}`).
	///
	/// Returns only files that have at least one complexity measurement.
	/// The TS adapter uses `nodes.file_uid` for this join rather than
	/// parsing the `#` delimiter from stable keys.
	pub fn query_complexity_by_file(
		&self,
		snapshot_uid: &str,
	) -> Result<Vec<FileComplexityRow>, StorageError> {
		let mut stmt = self.connection().prepare(
			"SELECT f.path, SUM(CAST(JSON_EXTRACT(m.value_json, '$.value') AS INTEGER)) AS sum_complexity
			 FROM measurements m
			 JOIN nodes n ON m.target_stable_key = n.stable_key AND m.snapshot_uid = n.snapshot_uid
			 JOIN files f ON n.file_uid = f.file_uid
			 WHERE m.snapshot_uid = ? AND m.kind = 'cyclomatic_complexity'
			 GROUP BY f.path",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid],
			|row| {
				Ok(FileComplexityRow {
					file_path: row.get(0)?,
					sum_complexity: row.get::<_, i64>(1)? as u64,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	pub fn find_imports_between_paths(
		&self,
		snapshot_uid: &str,
		source_prefix: &str,
		target_prefix: &str,
	) -> Result<Vec<ImportEdgeResult>, StorageError> {
		// Normalize: strip trailing slashes.
		let src = source_prefix.trim_end_matches('/');
		let tgt = target_prefix.trim_end_matches('/');
		let src_pattern = format!("{}/%%", src);
		let tgt_pattern = format!("{}/%%", tgt);

		let mut stmt = self.connection().prepare(
			"SELECT
			   src_f.path AS source_file,
			   tgt_f.path AS target_file,
			   e.line_start AS line
			 FROM edges e
			 JOIN nodes src_n ON e.source_node_uid = src_n.node_uid
			 JOIN nodes tgt_n ON e.target_node_uid = tgt_n.node_uid
			 JOIN files src_f ON src_n.file_uid = src_f.file_uid
			 JOIN files tgt_f ON tgt_n.file_uid = tgt_f.file_uid
			 WHERE e.snapshot_uid = ?
			   AND e.type = 'IMPORTS'
			   AND src_f.path LIKE ?
			   AND tgt_f.path LIKE ?
			 ORDER BY src_f.path, e.line_start",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, src_pattern, tgt_pattern],
			|row| {
				Ok(ImportEdgeResult {
					source_file: row.get(0)?,
					target_file: row.get(1)?,
					line: row.get(2)?,
				})
			},
		)?;

		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}

	// ── Internal helpers ─────────────────────────────────────

	fn query_symbol_by_field(
		&self,
		snapshot_uid: &str,
		field: &str,
		value: &str,
	) -> Result<Option<ResolvedSymbol>, StorageError> {
		// Safe: field is always a compile-time literal from this module.
		// The kind = 'SYMBOL' guard keeps FILE/MODULE nodes out of
		// the callers surface — even when matched by exact stable_key.
		let sql = format!(
			"SELECT n.stable_key, n.name, n.qualified_name, n.kind, n.subtype,
			        f.path, n.line_start, n.col_start
			 FROM nodes n
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE n.snapshot_uid = ? AND n.kind = 'SYMBOL' AND n.{} = ?
			 LIMIT 1",
			field
		);
		let result = self.connection().query_row(
			&sql,
			rusqlite::params![snapshot_uid, value],
			|row| {
				Ok(ResolvedSymbol {
					stable_key: row.get(0)?,
					name: row.get(1)?,
					qualified_name: row.get(2)?,
					kind: row.get(3)?,
					subtype: row.get(4)?,
					file: row.get(5)?,
					line: row.get(6)?,
					column: row.get(7)?,
				})
			},
		);
		match result {
			Ok(sym) => Ok(Some(sym)),
			Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
			Err(e) => Err(StorageError::Sqlite(e)),
		}
	}

	fn query_symbols_by_field(
		&self,
		snapshot_uid: &str,
		field: &str,
		value: &str,
	) -> Result<Vec<ResolvedSymbol>, StorageError> {
		let sql = format!(
			"SELECT n.stable_key, n.name, n.qualified_name, n.kind, n.subtype,
			        f.path, n.line_start, n.col_start
			 FROM nodes n
			 LEFT JOIN files f ON n.file_uid = f.file_uid
			 WHERE n.snapshot_uid = ? AND n.kind = 'SYMBOL' AND n.{} = ?
			 ORDER BY n.stable_key ASC",
			field
		);
		let mut stmt = self.connection().prepare(&sql)?;
		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, value],
			|row| {
				Ok(ResolvedSymbol {
					stable_key: row.get(0)?,
					name: row.get(1)?,
					qualified_name: row.get(2)?,
					kind: row.get(3)?,
					subtype: row.get(4)?,
					file: row.get(5)?,
					line: row.get(6)?,
					column: row.get(7)?,
				})
			},
		)?;
		rows.collect::<Result<Vec<_>, _>>()
			.map_err(StorageError::from)
	}
}

/// Extract the module path from a MODULE stable key.
///
/// Format: `{repo_uid}:{path}:MODULE` → `{path}`.
/// Returns `None` if the key does not end with `:MODULE`.
fn extract_module_path(stable_key: &str) -> Option<String> {
	let suffix = ":MODULE";
	if !stable_key.ends_with(suffix) {
		return None;
	}
	let without_suffix = &stable_key[..stable_key.len() - suffix.len()];
	// Skip the repo_uid prefix (everything before the first colon).
	let colon_pos = without_suffix.find(':')?;
	Some(without_suffix[colon_pos + 1..].to_string())
}

/// Canonicalize a cycle by rotating so the lexicographically
/// smallest UID comes first. Matches TS `canonicalizeCycle`
/// (sqlite-storage.ts:3356-3365).
fn canonicalize_cycle(uids: &[&str]) -> String {
	let min_idx = uids
		.iter()
		.enumerate()
		.min_by_key(|(_, uid)| **uid)
		.map(|(i, _)| i)
		.unwrap_or(0);
	let rotated: Vec<&str> = uids[min_idx..]
		.iter()
		.chain(uids[..min_idx].iter())
		.copied()
		.collect();
	rotated.join(",")
}

// ── Storage-layer regression tests ──────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;
	use crate::StorageConnection;

	/// Insert a minimal node directly so resolve_symbol can be tested
	/// without pulling in the full indexer stack.
	fn insert_raw_node(
		storage: &StorageConnection,
		snapshot_uid: &str,
		node_uid: &str,
		stable_key: &str,
		name: &str,
		kind: &str,
	) {
		storage
			.connection()
			.execute(
				"INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, name, kind)
				 VALUES (?, ?, 'r1', ?, ?, ?)",
				rusqlite::params![node_uid, snapshot_uid, stable_key, name, kind],
			)
			.unwrap();
	}

	fn setup_db_with_snapshot() -> (StorageConnection, String) {
		let storage = StorageConnection::open_in_memory().unwrap();
		let snap_uid = "snap-test-1";

		// Minimal repo + snapshot so FK constraints are satisfied.
		storage
			.connection()
			.execute_batch(&format!(
				"INSERT INTO repos (repo_uid, name, root_path, created_at)
				 VALUES ('r1', 'test-repo', '/tmp/r1', '2024-01-01T00:00:00Z');
				 INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at)
				 VALUES ('{snap_uid}', 'r1', 'ready', 'full', '2024-01-01T00:00:00Z');"
			))
			.unwrap();

		(storage, snap_uid.to_string())
	}

	// ── P2 regression: FILE stable_key must NOT resolve ─────────

	#[test]
	fn resolve_symbol_rejects_file_node_by_stable_key() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node(
			&storage,
			&snap_uid,
			"n-file-1",
			"r1:src/index.ts:FILE",
			"index.ts",
			"FILE",
		);

		let result = storage.resolve_symbol(&snap_uid, "r1:src/index.ts:FILE");
		assert!(
			matches!(result, Err(SymbolResolveError::NotFound)),
			"FILE node stable_key must not resolve: {:?}",
			result
		);
	}

	// ── P2 regression: MODULE stable_key must NOT resolve ───────

	#[test]
	fn resolve_symbol_rejects_module_node_by_stable_key() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node(
			&storage,
			&snap_uid,
			"n-mod-1",
			"r1:src:MODULE",
			"src",
			"MODULE",
		);

		let result = storage.resolve_symbol(&snap_uid, "r1:src:MODULE");
		assert!(
			matches!(result, Err(SymbolResolveError::NotFound)),
			"MODULE node stable_key must not resolve: {:?}",
			result
		);
	}

	// ── Positive: SYMBOL stable_key resolves ────────────────────

	#[test]
	fn resolve_symbol_accepts_symbol_node_by_stable_key() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node(
			&storage,
			&snap_uid,
			"n-sym-1",
			"r1:src/server.ts#serve:SYMBOL:FUNCTION",
			"serve",
			"SYMBOL",
		);

		let result = storage.resolve_symbol(&snap_uid, "r1:src/server.ts#serve:SYMBOL:FUNCTION");
		assert!(result.is_ok(), "SYMBOL stable_key must resolve: {:?}", result);
		let sym = result.unwrap();
		assert_eq!(sym.name, "serve");
		assert_eq!(sym.kind, "SYMBOL");
	}

	// ── FILE name must NOT resolve through step 3 ───────────────

	#[test]
	fn resolve_symbol_rejects_file_node_by_name() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node(
			&storage,
			&snap_uid,
			"n-file-2",
			"r1:src/server.ts:FILE",
			"server.ts",
			"FILE",
		);

		let result = storage.resolve_symbol(&snap_uid, "server.ts");
		assert!(
			matches!(result, Err(SymbolResolveError::NotFound)),
			"FILE node name must not resolve: {:?}",
			result
		);
	}

	// ── Rust-23: boundary declaration reading ───────────────

	fn insert_declaration(
		storage: &StorageConnection,
		uid: &str,
		repo_uid: &str,
		target_stable_key: &str,
		kind: &str,
		value_json: &str,
		is_active: bool,
	) {
		storage
			.connection()
			.execute(
				"INSERT INTO declarations
				 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
				 VALUES (?, ?, ?, ?, ?, '2024-01-01T00:00:00Z', ?)",
				rusqlite::params![uid, repo_uid, target_stable_key, kind, value_json, is_active as i32],
			)
			.unwrap();
	}

	#[test]
	fn boundary_declarations_returns_active_only() {
		let (storage, _snap_uid) = setup_db_with_snapshot();

		// Active boundary.
		insert_declaration(
			&storage, "d1", "r1", "r1:src/core:MODULE", "boundary",
			r#"{"forbids":"src/adapters"}"#, true,
		);
		// Inactive boundary (deactivated).
		insert_declaration(
			&storage, "d2", "r1", "r1:src/util:MODULE", "boundary",
			r#"{"forbids":"src/core"}"#, false,
		);

		let result = storage.get_active_boundary_declarations("r1").unwrap();
		assert_eq!(result.len(), 1, "only active boundary should be returned");
		assert_eq!(result[0].boundary_module, "src/core");
		assert_eq!(result[0].forbids, "src/adapters");
	}

	#[test]
	fn boundary_declarations_skips_malformed_value_json() {
		let (storage, _snap_uid) = setup_db_with_snapshot();

		// Valid.
		insert_declaration(
			&storage, "d1", "r1", "r1:src/core:MODULE", "boundary",
			r#"{"forbids":"src/adapters"}"#, true,
		);
		// Malformed JSON.
		insert_declaration(
			&storage, "d2", "r1", "r1:src/util:MODULE", "boundary",
			"not json", true,
		);
		// Valid JSON but missing forbids field.
		insert_declaration(
			&storage, "d3", "r1", "r1:src/lib:MODULE", "boundary",
			r#"{"something":"else"}"#, true,
		);

		let result = storage.get_active_boundary_declarations("r1").unwrap();
		assert_eq!(result.len(), 1, "only valid boundary should survive");
		assert_eq!(result[0].boundary_module, "src/core");
	}

	#[test]
	fn boundary_declarations_skips_non_module_stable_key() {
		let (storage, _snap_uid) = setup_db_with_snapshot();

		// MODULE key (valid).
		insert_declaration(
			&storage, "d1", "r1", "r1:src/core:MODULE", "boundary",
			r#"{"forbids":"src/adapters"}"#, true,
		);
		// FILE key (invalid for boundary).
		insert_declaration(
			&storage, "d2", "r1", "r1:src/index.ts:FILE", "boundary",
			r#"{"forbids":"src/adapters"}"#, true,
		);
		// SYMBOL key (invalid for boundary).
		insert_declaration(
			&storage, "d3", "r1", "r1:src/core/service.ts#serve:SYMBOL:FUNCTION", "boundary",
			r#"{"forbids":"src/adapters"}"#, true,
		);

		let result = storage.get_active_boundary_declarations("r1").unwrap();
		assert_eq!(result.len(), 1, "only MODULE key should survive");
		assert_eq!(result[0].boundary_module, "src/core");
	}

	#[test]
	fn boundary_declarations_extracts_reason() {
		let (storage, _snap_uid) = setup_db_with_snapshot();

		// With reason.
		insert_declaration(
			&storage, "d1", "r1", "r1:src/core:MODULE", "boundary",
			r#"{"forbids":"src/adapters","reason":"clean architecture"}"#, true,
		);
		// Without reason.
		insert_declaration(
			&storage, "d2", "r1", "r1:src/util:MODULE", "boundary",
			r#"{"forbids":"src/core"}"#, true,
		);

		let result = storage.get_active_boundary_declarations("r1").unwrap();
		assert_eq!(result.len(), 2);

		let core = result.iter().find(|d| d.boundary_module == "src/core").unwrap();
		assert_eq!(core.reason.as_deref(), Some("clean architecture"));

		let util = result.iter().find(|d| d.boundary_module == "src/util").unwrap();
		assert!(util.reason.is_none());
	}

	#[test]
	fn boundary_declarations_ignores_non_boundary_kinds() {
		let (storage, _snap_uid) = setup_db_with_snapshot();

		// Boundary kind.
		insert_declaration(
			&storage, "d1", "r1", "r1:src/core:MODULE", "boundary",
			r#"{"forbids":"src/adapters"}"#, true,
		);
		// Entrypoint kind (not boundary).
		insert_declaration(
			&storage, "d2", "r1", "r1:src/core:MODULE", "entrypoint",
			r#"{"type":"public_export"}"#, true,
		);

		let result = storage.get_active_boundary_declarations("r1").unwrap();
		assert_eq!(result.len(), 1, "only boundary kind should be returned");
	}

	#[test]
	fn extract_module_path_parses_correctly() {
		assert_eq!(
			extract_module_path("r1:src/core:MODULE"),
			Some("src/core".to_string())
		);
		assert_eq!(
			extract_module_path("repo-graph:src/adapters/storage:MODULE"),
			Some("src/adapters/storage".to_string())
		);
		assert_eq!(extract_module_path("r1:src/index.ts:FILE"), None);
		assert_eq!(extract_module_path("r1:src/core"), None);
		assert_eq!(extract_module_path(""), None);
	}

	// ── Rust-24: storage-read failure propagation ────────────

	#[test]
	fn find_imports_between_paths_propagates_error_on_schema_corruption() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		// Drop the files table so the JOIN in find_imports_between_paths fails.
		// In-memory DB: no migration repair on reopen.
		storage
			.connection()
			.execute_batch("DROP TABLE files")
			.unwrap();

		let result = storage.find_imports_between_paths(&snap_uid, "src/core", "src/adapters");
		assert!(
			result.is_err(),
			"find_imports_between_paths must propagate SQL error, got: {:?}",
			result
		);
	}

	#[test]
	fn get_active_boundary_declarations_propagates_error_on_schema_corruption() {
		let (storage, _snap_uid) = setup_db_with_snapshot();

		// Drop the declarations table.
		storage
			.connection()
			.execute_batch("DROP TABLE declarations")
			.unwrap();

		let result = storage.get_active_boundary_declarations("r1");
		assert!(
			result.is_err(),
			"get_active_boundary_declarations must propagate SQL error, got: {:?}",
			result
		);
	}

	// ── Rust-27: query_measurements_by_kind ─────────────────────

	/// Insert a measurement row directly.
	fn insert_measurement(
		storage: &StorageConnection,
		uid: &str,
		snapshot_uid: &str,
		target_stable_key: &str,
		kind: &str,
		value_json: &str,
	) {
		storage
			.connection()
			.execute(
				"INSERT INTO measurements
				 (measurement_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, source, created_at)
				 VALUES (?, ?, 'r1', ?, ?, ?, 'test', '2024-01-01T00:00:00Z')",
				rusqlite::params![uid, snapshot_uid, target_stable_key, kind, value_json],
			)
			.unwrap();
	}

	#[test]
	fn query_measurements_by_kind_empty_on_no_rows() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		let result = storage
			.query_measurements_by_kind(&snap_uid, "line_coverage")
			.unwrap();
		assert!(result.is_empty());
	}

	#[test]
	fn query_measurements_by_kind_returns_exact_rows() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		insert_measurement(
			&storage, "m1", &snap_uid,
			"r1:src/core/service.ts:FILE", "line_coverage",
			r#"{"value":0.85}"#,
		);
		insert_measurement(
			&storage, "m2", &snap_uid,
			"r1:src/core/model.ts:FILE", "line_coverage",
			r#"{"value":0.92}"#,
		);

		let result = storage
			.query_measurements_by_kind(&snap_uid, "line_coverage")
			.unwrap();
		assert_eq!(result.len(), 2);

		let keys: Vec<&str> = result.iter().map(|r| r.target_stable_key.as_str()).collect();
		assert!(keys.contains(&"r1:src/core/service.ts:FILE"));
		assert!(keys.contains(&"r1:src/core/model.ts:FILE"));

		// value_json is opaque — verify it round-trips.
		let m1 = result.iter().find(|r| r.target_stable_key.contains("service")).unwrap();
		assert_eq!(m1.value_json, r#"{"value":0.85}"#);
	}

	#[test]
	fn query_measurements_by_kind_filters_by_kind() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		insert_measurement(
			&storage, "m1", &snap_uid,
			"r1:src/core/service.ts:FILE", "line_coverage",
			r#"{"value":0.85}"#,
		);
		insert_measurement(
			&storage, "m2", &snap_uid,
			"r1:src/core/service.ts:SYMBOL:serve", "cyclomatic_complexity",
			r#"{"value":4}"#,
		);

		let coverage = storage
			.query_measurements_by_kind(&snap_uid, "line_coverage")
			.unwrap();
		assert_eq!(coverage.len(), 1);
		assert!(coverage[0].target_stable_key.contains("service.ts:FILE"));

		let complexity = storage
			.query_measurements_by_kind(&snap_uid, "cyclomatic_complexity")
			.unwrap();
		assert_eq!(complexity.len(), 1);
		assert!(complexity[0].target_stable_key.contains("SYMBOL:serve"));
	}

	#[test]
	fn query_measurements_by_kind_filters_by_snapshot() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		// Create a second snapshot.
		storage
			.connection()
			.execute(
				"INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at)
				 VALUES ('snap-other', 'r1', 'ready', 'full', '2024-01-02T00:00:00Z')",
				[],
			)
			.unwrap();

		insert_measurement(
			&storage, "m1", &snap_uid,
			"r1:src/a.ts:FILE", "line_coverage",
			r#"{"value":0.80}"#,
		);
		insert_measurement(
			&storage, "m2", "snap-other",
			"r1:src/b.ts:FILE", "line_coverage",
			r#"{"value":0.60}"#,
		);

		let result = storage
			.query_measurements_by_kind(&snap_uid, "line_coverage")
			.unwrap();
		assert_eq!(result.len(), 1);
		assert!(result[0].target_stable_key.contains("a.ts"));

		let other = storage
			.query_measurements_by_kind("snap-other", "line_coverage")
			.unwrap();
		assert_eq!(other.len(), 1);
		assert!(other[0].target_stable_key.contains("b.ts"));
	}

	#[test]
	fn query_measurements_by_kind_propagates_error_on_schema_corruption() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		// Drop the measurements table.
		storage
			.connection()
			.execute_batch("DROP TABLE measurements")
			.unwrap();

		let result = storage.query_measurements_by_kind(&snap_uid, "line_coverage");
		assert!(
			result.is_err(),
			"query_measurements_by_kind must propagate SQL error, got: {:?}",
			result
		);
	}

	// ── RS-MS-3a: query_complexity_by_file ──────────────────────

	/// Helper: insert a node and file for complexity tests.
	fn insert_file_and_symbol(
		storage: &StorageConnection,
		snapshot_uid: &str,
		repo_uid: &str,
		file_path: &str,
		symbol_name: &str,
	) -> String {
		let file_uid = format!("{}:{}", repo_uid, file_path);
		let stable_key = format!("{}:{}#{}:SYMBOL:FUNCTION", repo_uid, file_path, symbol_name);
		let node_uid = format!("node-{}", symbol_name);

		// Insert file if not exists
		let _ = storage.connection().execute(
			"INSERT OR IGNORE INTO files (file_uid, repo_uid, path, language, is_test)
			 VALUES (?, ?, ?, 'typescript', 0)",
			rusqlite::params![file_uid, repo_uid, file_path],
		);

		// Insert node
		storage.connection().execute(
			"INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, file_uid)
			 VALUES (?, ?, ?, ?, 'SYMBOL', 'FUNCTION', ?, ?)",
			rusqlite::params![node_uid, snapshot_uid, repo_uid, stable_key, symbol_name, file_uid],
		).unwrap();

		stable_key
	}

	#[test]
	fn query_complexity_by_file_sums_symbols_per_file() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		// Two symbols in same file
		let key1 = insert_file_and_symbol(&storage, &snap_uid, "r1", "src/service.ts", "foo");
		let key2 = insert_file_and_symbol(&storage, &snap_uid, "r1", "src/service.ts", "bar");
		// One symbol in different file
		let key3 = insert_file_and_symbol(&storage, &snap_uid, "r1", "src/util.ts", "helper");

		// Insert complexity measurements
		insert_measurement(&storage, "m1", &snap_uid, &key1, "cyclomatic_complexity", r#"{"value":5}"#);
		insert_measurement(&storage, "m2", &snap_uid, &key2, "cyclomatic_complexity", r#"{"value":3}"#);
		insert_measurement(&storage, "m3", &snap_uid, &key3, "cyclomatic_complexity", r#"{"value":7}"#);

		let result = storage.query_complexity_by_file(&snap_uid).unwrap();
		assert_eq!(result.len(), 2);

		// Find each file's entry
		let service = result.iter().find(|r| r.file_path == "src/service.ts").unwrap();
		let util = result.iter().find(|r| r.file_path == "src/util.ts").unwrap();

		// src/service.ts should have 5 + 3 = 8
		assert_eq!(service.sum_complexity, 8);
		// src/util.ts should have 7
		assert_eq!(util.sum_complexity, 7);
	}

	#[test]
	fn query_complexity_by_file_empty_on_no_measurements() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		// Insert file and symbol but no measurements
		insert_file_and_symbol(&storage, &snap_uid, "r1", "src/service.ts", "foo");

		let result = storage.query_complexity_by_file(&snap_uid).unwrap();
		assert!(result.is_empty());
	}

	#[test]
	fn query_complexity_by_file_ignores_other_measurement_kinds() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		let key = insert_file_and_symbol(&storage, &snap_uid, "r1", "src/service.ts", "foo");

		// Insert complexity + coverage
		insert_measurement(&storage, "m1", &snap_uid, &key, "cyclomatic_complexity", r#"{"value":5}"#);
		insert_measurement(&storage, "m2", &snap_uid, &key, "line_coverage", r#"{"value":0.85}"#);

		let result = storage.query_complexity_by_file(&snap_uid).unwrap();
		assert_eq!(result.len(), 1);
		assert_eq!(result[0].sum_complexity, 5);
	}

	// ── Rust-30: query_inferences_by_kind ───────────────────────

	/// Insert an inference row directly.
	fn insert_inference(
		storage: &StorageConnection,
		uid: &str,
		snapshot_uid: &str,
		target_stable_key: &str,
		kind: &str,
		value_json: &str,
	) {
		storage
			.connection()
			.execute(
				"INSERT INTO inferences
				 (inference_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, confidence, basis_json, extractor, created_at)
				 VALUES (?, ?, 'r1', ?, ?, ?, 1.0, '{}', 'test', '2024-01-01T00:00:00Z')",
				rusqlite::params![uid, snapshot_uid, target_stable_key, kind, value_json],
			)
			.unwrap();
	}

	#[test]
	fn query_inferences_by_kind_empty_on_no_rows() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		let result = storage
			.query_inferences_by_kind(&snap_uid, "hotspot_score")
			.unwrap();
		assert!(result.is_empty());
	}

	#[test]
	fn query_inferences_by_kind_returns_exact_rows() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		insert_inference(
			&storage, "inf-1", &snap_uid,
			"r1:src/core/service.ts:FILE", "hotspot_score",
			r#"{"normalized_score":0.85}"#,
		);
		insert_inference(
			&storage, "inf-2", &snap_uid,
			"r1:src/core/model.ts:FILE", "hotspot_score",
			r#"{"normalized_score":0.42}"#,
		);

		let result = storage
			.query_inferences_by_kind(&snap_uid, "hotspot_score")
			.unwrap();
		assert_eq!(result.len(), 2);

		let keys: Vec<&str> = result.iter().map(|r| r.target_stable_key.as_str()).collect();
		assert!(keys.contains(&"r1:src/core/service.ts:FILE"));
		assert!(keys.contains(&"r1:src/core/model.ts:FILE"));

		let inf1 = result.iter().find(|r| r.target_stable_key.contains("service")).unwrap();
		assert_eq!(inf1.value_json, r#"{"normalized_score":0.85}"#);
	}

	#[test]
	fn query_inferences_by_kind_filters_by_kind() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		insert_inference(
			&storage, "inf-1", &snap_uid,
			"r1:src/core/service.ts:FILE", "hotspot_score",
			r#"{"normalized_score":0.85}"#,
		);
		insert_inference(
			&storage, "inf-2", &snap_uid,
			"r1:src/core/service.ts:SYMBOL:serve", "framework_entrypoint",
			r#"{"kind":"lambda"}"#,
		);

		let hotspots = storage
			.query_inferences_by_kind(&snap_uid, "hotspot_score")
			.unwrap();
		assert_eq!(hotspots.len(), 1);
		assert!(hotspots[0].target_stable_key.contains("FILE"));

		let entrypoints = storage
			.query_inferences_by_kind(&snap_uid, "framework_entrypoint")
			.unwrap();
		assert_eq!(entrypoints.len(), 1);
		assert!(entrypoints[0].target_stable_key.contains("SYMBOL"));
	}

	#[test]
	fn query_inferences_by_kind_filters_by_snapshot() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		storage
			.connection()
			.execute(
				"INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at)
				 VALUES ('snap-other', 'r1', 'ready', 'full', '2024-01-02T00:00:00Z')",
				[],
			)
			.unwrap();

		insert_inference(
			&storage, "inf-1", &snap_uid,
			"r1:src/a.ts:FILE", "hotspot_score",
			r#"{"normalized_score":0.70}"#,
		);
		insert_inference(
			&storage, "inf-2", "snap-other",
			"r1:src/b.ts:FILE", "hotspot_score",
			r#"{"normalized_score":0.30}"#,
		);

		let result = storage
			.query_inferences_by_kind(&snap_uid, "hotspot_score")
			.unwrap();
		assert_eq!(result.len(), 1);
		assert!(result[0].target_stable_key.contains("a.ts"));

		let other = storage
			.query_inferences_by_kind("snap-other", "hotspot_score")
			.unwrap();
		assert_eq!(other.len(), 1);
		assert!(other[0].target_stable_key.contains("b.ts"));
	}

	#[test]
	fn query_inferences_by_kind_propagates_error_on_schema_corruption() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		storage
			.connection()
			.execute_batch("DROP TABLE inferences")
			.unwrap();

		let result = storage.query_inferences_by_kind(&snap_uid, "hotspot_score");
		assert!(
			result.is_err(),
			"query_inferences_by_kind must propagate SQL error, got: {:?}",
			result
		);
	}

	// ── SB-5: Resource resolution tests ──────────────────────────

	fn insert_raw_node_with_subtype(
		storage: &StorageConnection,
		snapshot_uid: &str,
		node_uid: &str,
		stable_key: &str,
		name: &str,
		kind: &str,
		subtype: Option<&str>,
	) {
		storage
			.connection()
			.execute(
				"INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, name, kind, subtype)
				 VALUES (?, ?, 'r1', ?, ?, ?, ?)",
				rusqlite::params![node_uid, snapshot_uid, stable_key, name, kind, subtype],
			)
			.unwrap();
	}

	#[test]
	fn resolve_resource_accepts_fs_path_node() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node_with_subtype(
			&storage,
			&snap_uid,
			"n-res-1",
			"r1:fs:/etc/config:FS_PATH",
			"/etc/config",
			"FS_PATH",
			Some("FILE_PATH"),
		);

		let result = storage.resolve_resource(&snap_uid, "r1:fs:/etc/config:FS_PATH");
		assert!(result.is_ok(), "FS_PATH must resolve: {:?}", result);
		let res = result.unwrap();
		assert_eq!(res.kind, "FS_PATH");
		assert_eq!(res.subtype.as_deref(), Some("FILE_PATH"));
	}

	#[test]
	fn resolve_resource_accepts_db_resource_node() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node_with_subtype(
			&storage,
			&snap_uid,
			"n-res-2",
			"r1:db:postgres://host/db:DB_RESOURCE",
			"db:postgres://host/db",
			"DB_RESOURCE",
			Some("CONNECTION"),
		);

		let result = storage.resolve_resource(&snap_uid, "r1:db:postgres://host/db:DB_RESOURCE");
		assert!(result.is_ok(), "DB_RESOURCE must resolve: {:?}", result);
	}

	#[test]
	fn resolve_resource_accepts_state_cache_node() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node_with_subtype(
			&storage,
			&snap_uid,
			"n-res-3",
			"r1:state:redis-cache:STATE",
			"redis-cache",
			"STATE",
			Some("CACHE"),
		);

		let result = storage.resolve_resource(&snap_uid, "r1:state:redis-cache:STATE");
		assert!(result.is_ok(), "STATE+CACHE must resolve: {:?}", result);
	}

	#[test]
	fn resolve_resource_rejects_symbol_node() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node(
			&storage,
			&snap_uid,
			"n-sym-x",
			"r1:src/a.ts#foo:SYMBOL:FUNCTION",
			"foo",
			"SYMBOL",
		);

		let result = storage.resolve_resource(&snap_uid, "r1:src/a.ts#foo:SYMBOL:FUNCTION");
		assert!(
			matches!(result, Err(ResourceResolveError::NotAResource(_))),
			"SYMBOL must be rejected by resolve_resource: {:?}",
			result
		);
	}

	#[test]
	fn resolve_resource_rejects_state_non_cache_subtype() {
		let (storage, snap_uid) = setup_db_with_snapshot();
		insert_raw_node_with_subtype(
			&storage,
			&snap_uid,
			"n-res-4",
			"r1:state:session:STATE",
			"session",
			"STATE",
			Some("SESSION"), // Not CACHE
		);

		let result = storage.resolve_resource(&snap_uid, "r1:state:session:STATE");
		assert!(
			matches!(result, Err(ResourceResolveError::NotAResource(_))),
			"STATE+non-CACHE must be rejected: {:?}",
			result
		);
	}

	#[test]
	fn resolve_resource_returns_not_found_for_missing_key() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		let result = storage.resolve_resource(&snap_uid, "nonexistent:key");
		assert!(
			matches!(result, Err(ResourceResolveError::NotFound)),
			"missing key must return NotFound: {:?}",
			result
		);
	}

	// ── SB-5: Resource readers/writers tests ─────────────────────

	fn insert_raw_edge(
		storage: &StorageConnection,
		snapshot_uid: &str,
		edge_uid: &str,
		source_node_uid: &str,
		target_node_uid: &str,
		edge_type: &str,
	) {
		storage
			.connection()
			.execute(
				"INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, type, resolution, extractor)
				 VALUES (?, ?, 'r1', ?, ?, ?, 'static', 'test')",
				rusqlite::params![edge_uid, snapshot_uid, source_node_uid, target_node_uid, edge_type],
			)
			.unwrap();
	}

	#[test]
	fn find_resource_readers_returns_only_symbols() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		// Resource node
		insert_raw_node_with_subtype(
			&storage, &snap_uid, "n-res", "r1:fs:config:FS_PATH",
			"config", "FS_PATH", Some("FILE_PATH"),
		);
		// Symbol reader
		insert_raw_node(&storage, &snap_uid, "n-sym", "r1:a.ts#read:SYMBOL:FUNCTION", "read", "SYMBOL");
		// File node (should be excluded even if edge exists)
		insert_raw_node(&storage, &snap_uid, "n-file", "r1:a.ts:FILE", "a.ts", "FILE");

		// READS edge from symbol
		insert_raw_edge(&storage, &snap_uid, "e1", "n-sym", "n-res", "READS");
		// READS edge from file (would be bug if emitted, but test query exclusion)
		insert_raw_edge(&storage, &snap_uid, "e2", "n-file", "n-res", "READS");

		let readers = storage.find_resource_readers(&snap_uid, "r1:fs:config:FS_PATH").unwrap();
		assert_eq!(readers.len(), 1, "only SYMBOL nodes should be returned");
		assert_eq!(readers[0].kind, "SYMBOL");
		assert_eq!(readers[0].name, "read");
	}

	#[test]
	fn find_resource_writers_returns_only_symbols() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		insert_raw_node_with_subtype(
			&storage, &snap_uid, "n-res", "r1:fs:log:FS_PATH",
			"log", "FS_PATH", Some("FILE_PATH"),
		);
		insert_raw_node(&storage, &snap_uid, "n-sym", "r1:b.ts#write:SYMBOL:FUNCTION", "write", "SYMBOL");

		insert_raw_edge(&storage, &snap_uid, "e1", "n-sym", "n-res", "WRITES");

		let writers = storage.find_resource_writers(&snap_uid, "r1:fs:log:FS_PATH").unwrap();
		assert_eq!(writers.len(), 1);
		assert_eq!(writers[0].edge_type, "WRITES");
	}

	// ── SB-5: Dead-node resource exclusion tests ─────────────────

	#[test]
	fn find_dead_nodes_excludes_resource_kinds() {
		let (storage, snap_uid) = setup_db_with_snapshot();

		// FS_PATH (should be excluded)
		insert_raw_node_with_subtype(
			&storage, &snap_uid, "n-fs", "r1:fs:file:FS_PATH",
			"file", "FS_PATH", Some("FILE_PATH"),
		);
		// DB_RESOURCE (should be excluded)
		insert_raw_node_with_subtype(
			&storage, &snap_uid, "n-db", "r1:db:conn:DB_RESOURCE",
			"conn", "DB_RESOURCE", Some("CONNECTION"),
		);
		// BLOB (should be excluded)
		insert_raw_node_with_subtype(
			&storage, &snap_uid, "n-blob", "r1:blob:bucket:BLOB",
			"bucket", "BLOB", Some("BUCKET"),
		);
		// STATE+CACHE (should be excluded)
		insert_raw_node_with_subtype(
			&storage, &snap_uid, "n-cache", "r1:state:cache:STATE",
			"cache", "STATE", Some("CACHE"),
		);
		// STATE+SESSION (should NOT be excluded - only CACHE is)
		insert_raw_node_with_subtype(
			&storage, &snap_uid, "n-session", "r1:state:session:STATE",
			"session", "STATE", Some("SESSION"),
		);
		// SYMBOL (should NOT be excluded)
		insert_raw_node(&storage, &snap_uid, "n-sym", "r1:a.ts#orphan:SYMBOL:FUNCTION", "orphan", "SYMBOL");

		let dead = storage.find_dead_nodes(&snap_uid, "r1", None).unwrap();

		let kinds: Vec<&str> = dead.iter().map(|d| d.kind.as_str()).collect();
		assert!(
			!kinds.contains(&"FS_PATH"),
			"FS_PATH should be excluded from dead: {:?}",
			kinds
		);
		assert!(
			!kinds.contains(&"DB_RESOURCE"),
			"DB_RESOURCE should be excluded from dead: {:?}",
			kinds
		);
		assert!(
			!kinds.contains(&"BLOB"),
			"BLOB should be excluded from dead: {:?}",
			kinds
		);
		// STATE should only be excluded if subtype=CACHE
		let state_nodes: Vec<_> = dead.iter().filter(|d| d.kind == "STATE").collect();
		assert!(
			state_nodes.iter().all(|d| d.subtype.as_deref() != Some("CACHE")),
			"STATE+CACHE should be excluded, but other STATE subtypes included: {:?}",
			state_nodes
		);

		assert!(
			kinds.contains(&"SYMBOL"),
			"SYMBOL should be included in dead: {:?}",
			kinds
		);
	}
}
