//! Migration 012 — durable extraction edges + conditional data copy.
//!
//! Creates `extraction_edges` (durable analog of `staged_edges`),
//! plus three indexes. Then conditionally copies surviving
//! `staged_edges` rows into `extraction_edges` to handle
//! incomplete runs from before this migration. Mirrors
//! `012-extraction-edges.ts`.
//!
//! ── Conditional data copy ─────────────────────────────────────
//!
//! The TS migration uses a `COUNT(*)` guard before issuing the
//! `INSERT OR IGNORE INTO extraction_edges SELECT * FROM
//! staged_edges` statement. The guard avoids the insert when
//! there are no rows to copy. The Rust port mirrors this exactly.
//!
//! On a fresh Rust install, `staged_edges` has zero rows
//! (migration 009 created the table empty), so the guard returns
//! 0 and the data copy is skipped. The migration just creates
//! the table and indexes.
//!
//! `INSERT OR IGNORE` provides idempotency: if some
//! `staged_edges` rows happen to share `edge_uid` with existing
//! `extraction_edges` rows, the duplicates are silently dropped.
//! `SELECT *` is safe here because both tables have identical
//! column shapes (verified by reading the schemas in migrations
//! 009 and 012).

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	// Create the extraction_edges table and indexes.
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS extraction_edges (
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
		CREATE INDEX IF NOT EXISTS idx_extraction_edges_snapshot ON extraction_edges(snapshot_uid);
		CREATE INDEX IF NOT EXISTS idx_extraction_edges_source_file ON extraction_edges(snapshot_uid, source_file_uid);
		CREATE INDEX IF NOT EXISTS idx_extraction_edges_cursor ON extraction_edges(snapshot_uid, edge_uid);
		"#,
	)?;

	// Conditional copy of any surviving staged_edges rows.
	let staged_count: i64 =
		conn.query_row("SELECT COUNT(*) FROM staged_edges", [], |row| row.get(0))?;

	if staged_count > 0 {
		conn.execute_batch(
			"INSERT OR IGNORE INTO extraction_edges SELECT * FROM staged_edges",
		)?;
	}

	record_migration(conn, 12, "012-extraction-edges")?;
	Ok(())
}
