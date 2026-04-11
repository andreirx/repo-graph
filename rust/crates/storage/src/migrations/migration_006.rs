//! Migration 006 — annotations table.
//!
//! Pure SQL: creates the `annotations` table and two indexes.
//! Mirrors `006-annotations.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS annotations (
			annotation_uid      TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			target_kind         TEXT NOT NULL,
			target_stable_key   TEXT NOT NULL,
			annotation_kind     TEXT NOT NULL,
			contract_class      TEXT NOT NULL,
			content             TEXT NOT NULL,
			content_hash        TEXT NOT NULL,
			source_file         TEXT NOT NULL,
			source_line_start   INTEGER NOT NULL,
			source_line_end     INTEGER NOT NULL,
			language            TEXT NOT NULL,
			provisional         INTEGER NOT NULL DEFAULT 1,
			extracted_at        TEXT NOT NULL
		);
		CREATE INDEX IF NOT EXISTS idx_annotations_target ON annotations(snapshot_uid, target_stable_key);
		CREATE INDEX IF NOT EXISTS idx_annotations_kind ON annotations(snapshot_uid, annotation_kind);
		"#,
	)?;

	record_migration(conn, 6, "006-annotations")?;
	Ok(())
}
