//! Migration 003 — measurements table.
//!
//! Pure SQL: creates the `measurements` table and two indexes.
//! Mirrors `003-measurements.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS measurements (
			measurement_uid     TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			target_stable_key   TEXT NOT NULL,
			kind                TEXT NOT NULL,
			value_json          TEXT NOT NULL,
			source              TEXT NOT NULL,
			created_at          TEXT NOT NULL
		);
		CREATE INDEX IF NOT EXISTS idx_measurements_target ON measurements(snapshot_uid, target_stable_key, kind);
		CREATE INDEX IF NOT EXISTS idx_measurements_kind ON measurements(snapshot_uid, kind);
		"#,
	)?;

	record_migration(conn, 3, "003-measurements")?;
	Ok(())
}
