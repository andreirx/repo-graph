//! Read-side graph queries — callers, callees, etc.
//!
//! These are query methods on `StorageConnection` that serve the
//! CLI read-side surface. Separate from CRUD (write-side) and
//! from trait impls (indexer/trust port implementations).
//!
//! Rust-10: `resolve_symbol` + `find_direct_callers`.
//! Rust-11: `find_direct_callees`.
//! Rust-12: `find_dead_nodes`.

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

	/// Find direct callers of a symbol (one hop, CALLS edges only).
	pub fn find_direct_callers(
		&self,
		snapshot_uid: &str,
		target_stable_key: &str,
	) -> Result<Vec<CallerResult>, StorageError> {
		let mut stmt = self.connection().prepare(
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
			   AND e.type = 'CALLS'
			 ORDER BY n.name ASC, f.path ASC",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, target_stable_key, snapshot_uid],
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

	/// Find direct callees of a symbol (one hop, CALLS edges only).
	///
	/// Symmetric reverse of `find_direct_callers`: the given symbol
	/// is the source node, returned nodes are the targets.
	pub fn find_direct_callees(
		&self,
		snapshot_uid: &str,
		source_stable_key: &str,
	) -> Result<Vec<CalleeResult>, StorageError> {
		let mut stmt = self.connection().prepare(
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
			   AND e.type = 'CALLS'
			 ORDER BY n.name ASC, f.path ASC",
		)?;

		let rows = stmt.query_map(
			rusqlite::params![snapshot_uid, source_stable_key, snapshot_uid],
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

	/// Find dead nodes — nodes with no incoming reference edges.
	///
	/// Mirrors the TS `findDeadNodes` algorithm exactly:
	///   1. Select all nodes in the snapshot.
	///   2. Exclude nodes that are targets of reference edges
	///      (IMPORTS, CALLS, IMPLEMENTS, INSTANTIATES, ROUTES_TO,
	///      REGISTERED_BY, TESTED_BY, COVERS).
	///   3. Exclude declared entrypoints (declarations table).
	///   4. Exclude framework-liveness inferences.
	///   5. Optional: filter by node kind (e.g., "SYMBOL").
	///   6. ORDER BY name ASC.
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
				END AS line_count
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
		Ok(DeadNodeResult {
			stable_key: row.get(0)?,
			symbol: qualified_name.unwrap_or(name),
			kind: row.get(3)?,
			subtype: row.get(4)?,
			file: row.get(5)?,
			line: row.get(6)?,
			line_count: row.get(7)?,
		})
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
}
