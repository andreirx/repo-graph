//! Migration 009 — staging tables for large-repo indexing.
//!
//! Pure SQL: creates `staged_edges` and `file_signals` tables
//! plus three indexes. Mirrors `009-staging-tables.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS staged_edges (
			edge_uid        TEXT PRIMARY KEY,
			snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			source_node_uid TEXT NOT NULL,
			target_key      TEXT NOT NULL,
			type            TEXT NOT NULL,
			resolution      TEXT NOT NULL,
			extractor       TEXT NOT NULL,
			line_start      INTEGER,
			col_start       INTEGER,
			line_end        INTEGER,
			col_end         INTEGER,
			metadata_json   TEXT,
			source_file_uid TEXT
		);
		CREATE INDEX IF NOT EXISTS idx_staged_edges_snapshot ON staged_edges(snapshot_uid);
		CREATE INDEX IF NOT EXISTS idx_staged_edges_source ON staged_edges(snapshot_uid, source_file_uid);

		CREATE TABLE IF NOT EXISTS file_signals (
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			file_uid            TEXT NOT NULL,
			import_bindings_json TEXT,
			PRIMARY KEY (snapshot_uid, file_uid)
		);
		CREATE INDEX IF NOT EXISTS idx_file_signals_snapshot ON file_signals(snapshot_uid);
		"#,
	)?;

	record_migration(conn, 9, "009-staging-tables")?;
	Ok(())
}
