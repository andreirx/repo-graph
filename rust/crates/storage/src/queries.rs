//! Read-side graph queries — callers, callees, etc.
//!
//! These are query methods on `StorageConnection` that serve the
//! CLI read-side surface. Separate from CRUD (write-side) and
//! from trait impls (indexer/trust port implementations).
//!
//! Rust-10: `resolve_symbol` + `find_direct_callers`.

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
