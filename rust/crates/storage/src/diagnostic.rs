//! Diagnostic dump helpers for the storage substrate.
//!
//! This module provides the narrow diagnostic surface used by
//! the R2-F parity harness (and any future debugging code) to
//! inspect the raw database state without being given the
//! rusqlite `Connection` handle directly.
//!
//! The only public entry point is exposed through
//! `StorageConnection::diagnostic_dump()`. This module's items
//! are `pub(crate)` internal helpers that encapsulate the SQL
//! shapes for schema introspection and per-table row dumping.
//!
//! ── Why this module exists (R2-F correction) ──────────────────
//!
//! R2-F's first implementation widened `StorageConnection::connection()`
//! from `pub(crate)` to `pub` so the integration parity test
//! could access the underlying rusqlite Connection directly.
//! That widening leaked rusqlite as part of the public API
//! surface of the storage substrate crate — any external caller
//! could open ad-hoc transactions, run arbitrary SQL, and bypass
//! the locked CRUD boundary. The R2-D architectural lock
//! explicitly required the connection accessor to remain
//! crate-internal.
//!
//! The corrected shape (applied in this module) is:
//!
//!   1. `connection()` and `connection_mut()` stay `pub(crate)`,
//!      preserving the R2-D lock.
//!
//!   2. A narrow `diagnostic_dump()` method on `StorageConnection`
//!      returns a `serde_json::Value` containing the canonical
//!      schema + per-table row data. This is the ONLY way
//!      external code can introspect database state.
//!
//!   3. The dump logic lives here, in this crate-internal
//!      module. It uses `pub(crate) fn` internally and only the
//!      public `StorageConnection::diagnostic_dump()` entry
//!      point is visible to integration tests.
//!
//! External callers therefore cannot couple to rusqlite at all.
//! They get a `serde_json::Value` containing a snapshot of
//! canonical diagnostic state — nothing more.
//!
//! ── Schema dump format (PRAGMA-based, not sqlite_master text) ─
//!
//! The schema dump uses `PRAGMA table_info(<table>)` per table,
//! NOT `sqlite_master.sql` text. This is essential for
//! cross-runtime parity:
//!
//!   - Rust embeds the outdated `001-initial.sql` per R2-C D12,
//!     so migrations 002/005/010 add columns via `ALTER TABLE`.
//!     The resulting `sqlite_master.sql` text shows the columns
//!     appended with ALTER-style formatting.
//!
//!   - TS uses the up-to-date `001-initial.ts` constant, so the
//!     columns are inline in the original `CREATE TABLE` body.
//!
//!   - Both end states are logically identical; the stored
//!     `sqlite_master.sql` text differs.
//!
//! Using PRAGMA table_info gives a canonical column-shape
//! representation independent of creation path. Both runtimes
//! produce byte-equal output for the same logical schema.

use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{json, Map, Value};

/// Assemble the full diagnostic dump: `{schema, tables}`.
///
/// Called by `StorageConnection::diagnostic_dump()`.
pub(crate) fn dump_state(conn: &Connection) -> Value {
	json!({
		"schema": dump_schema(conn),
		"tables": dump_tables(conn),
	})
}

/// Dump the logical schema as `{tables: {<name>: [columns]}, indexes: [<name>...]}`.
///
/// Columns per table are sorted by column name.
/// Indexes are sorted by index name.
pub(crate) fn dump_schema(conn: &Connection) -> Value {
	let mut table_stmt = conn
		.prepare(
			"SELECT name FROM sqlite_master WHERE type = 'table' \
			 AND name NOT LIKE 'sqlite_%' ORDER BY name",
		)
		.expect("prepare schema table list");
	let table_names: Vec<String> = table_stmt
		.query_map([], |row| row.get::<_, String>(0))
		.expect("query table names")
		.map(|r| r.expect("table name"))
		.collect();

	let mut tables_out = Map::new();
	for table_name in table_names {
		let sql = format!("PRAGMA table_info({})", table_name);
		let mut stmt = conn
			.prepare(&sql)
			.unwrap_or_else(|e| panic!("prepare pragma for {}: {e}", table_name));
		let mut cols: Vec<Value> = stmt
			.query_map([], |row| {
				Ok(json!({
					"name": row.get::<_, String>("name")?,
					"type": row.get::<_, String>("type")?,
					"notnull": row.get::<_, i64>("notnull")? != 0,
					"dflt_value": row.get::<_, Option<String>>("dflt_value")?,
					"pk": row.get::<_, i64>("pk")?,
				}))
			})
			.expect("query pragma")
			.map(|r| r.expect("pragma row"))
			.collect();
		cols.sort_by(|a, b| {
			let an = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
			let bn = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
			an.cmp(bn)
		});
		tables_out.insert(table_name, Value::Array(cols));
	}

	let mut idx_stmt = conn
		.prepare(
			"SELECT name FROM sqlite_master WHERE type = 'index' \
			 AND name NOT LIKE 'sqlite_%' ORDER BY name",
		)
		.expect("prepare index list");
	let index_names: Vec<Value> = idx_stmt
		.query_map([], |row| row.get::<_, String>(0))
		.expect("query indexes")
		.map(|r| Value::String(r.expect("index name")))
		.collect();

	json!({
		"tables": Value::Object(tables_out),
		"indexes": Value::Array(index_names),
	})
}

/// Dump per-table row data. Only tables with at least one row
/// are included. Each table's rows are sorted by the known
/// identity key.
pub(crate) fn dump_tables(conn: &Connection) -> Value {
	let mut stmt = conn
		.prepare(
			"SELECT name FROM sqlite_master WHERE type = 'table' \
			 AND name NOT LIKE 'sqlite_%' ORDER BY name",
		)
		.expect("prepare table names");
	let table_names: Vec<String> = stmt
		.query_map([], |row| row.get::<_, String>(0))
		.expect("query table names")
		.map(|r| r.expect("table name"))
		.collect();

	let mut out = Map::new();
	for table_name in table_names {
		let sort_key = sort_key_for(&table_name);
		let sql = format!("SELECT * FROM \"{}\" ORDER BY {}", table_name, sort_key);
		let mut stmt = conn
			.prepare(&sql)
			.unwrap_or_else(|e| panic!("prepare dump for {}: {e}", table_name));

		let col_count = stmt.column_count();
		let col_names: Vec<String> = (0..col_count)
			.map(|i| stmt.column_name(i).expect("column name").to_string())
			.collect();

		let rows: Vec<Value> = stmt
			.query_map([], |row| {
				let mut obj = Map::new();
				for (i, col_name) in col_names.iter().enumerate() {
					let v = row_value_to_json(row, i);
					obj.insert(col_name.clone(), v);
				}
				Ok(Value::Object(obj))
			})
			.expect("query dump")
			.map(|r| r.expect("dump row"))
			.collect();

		if !rows.is_empty() {
			out.insert(table_name, Value::Array(rows));
		}
	}
	Value::Object(out)
}

fn row_value_to_json(row: &rusqlite::Row, col: usize) -> Value {
	match row.get_ref(col).expect("column ref") {
		ValueRef::Null => Value::Null,
		ValueRef::Integer(i) => Value::from(i),
		ValueRef::Real(f) => Value::from(f),
		ValueRef::Text(bytes) => {
			Value::String(String::from_utf8_lossy(bytes).to_string())
		}
		ValueRef::Blob(_) => Value::String("<BLOB>".to_string()),
	}
}

/// Identity-column SQL fragment for ORDER BY clauses, per table.
///
/// Matches the fixture-format README's "Canonical ordering"
/// section. Tables outside the D4-Core set fall back to
/// `rowid` ordering, which is defensive — no R2-F v1 fixture
/// writes to such tables, but the dump does not crash if a
/// future fixture does.
pub(crate) fn sort_key_for(table: &str) -> &'static str {
	match table {
		"repos" => "repo_uid",
		"snapshots" => "snapshot_uid",
		"files" => "file_uid",
		"file_versions" => "snapshot_uid, file_uid",
		"nodes" => "node_uid",
		"edges" => "edge_uid",
		"schema_migrations" => "version",
		_ => "rowid",
	}
}
