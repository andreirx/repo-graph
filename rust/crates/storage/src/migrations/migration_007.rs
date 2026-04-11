//! Migration 007 — unresolved_edges table.
//!
//! Pure SQL: creates the `unresolved_edges` table and four
//! indexes. Mirrors `007-unresolved-edges.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS unresolved_edges (
			edge_uid            TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
			source_node_uid     TEXT NOT NULL REFERENCES nodes(node_uid),
			target_key          TEXT NOT NULL,
			type                TEXT NOT NULL,
			resolution          TEXT NOT NULL,
			extractor           TEXT NOT NULL,
			line_start          INTEGER,
			col_start           INTEGER,
			line_end            INTEGER,
			col_end             INTEGER,
			metadata_json       TEXT,
			category            TEXT NOT NULL,
			classification      TEXT NOT NULL,
			classifier_version  INTEGER NOT NULL,
			basis_code          TEXT NOT NULL,
			observed_at         TEXT NOT NULL
		);
		CREATE INDEX IF NOT EXISTS idx_unresolved_edges_snapshot_class ON unresolved_edges(snapshot_uid, classification);
		CREATE INDEX IF NOT EXISTS idx_unresolved_edges_snapshot_category ON unresolved_edges(snapshot_uid, category);
		CREATE INDEX IF NOT EXISTS idx_unresolved_edges_snapshot_source ON unresolved_edges(snapshot_uid, source_node_uid);
		CREATE INDEX IF NOT EXISTS idx_unresolved_edges_snapshot_classifier_version ON unresolved_edges(snapshot_uid, classifier_version);
		"#,
	)?;

	record_migration(conn, 7, "007-unresolved-edges")?;
	Ok(())
}
