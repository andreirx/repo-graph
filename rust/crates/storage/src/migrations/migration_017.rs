//! Migration 017 — module discovery diagnostics table.
//!
//! Pure SQL: creates `module_discovery_diagnostics` table plus three
//! indexes. Mirrors `017-module-discovery-diagnostics.ts`.
//!
//! Diagnostics explain why certain build-system patterns were NOT
//! converted to module candidates (e.g., skipped Kconfig-gated
//! assignments in Kbuild files).

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS module_discovery_diagnostics (
			diagnostic_uid   TEXT PRIMARY KEY,
			snapshot_uid     TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			source_type      TEXT NOT NULL,
			diagnostic_kind  TEXT NOT NULL,
			file_path        TEXT NOT NULL,
			line             INTEGER,
			raw_text         TEXT,
			message          TEXT NOT NULL,
			severity         TEXT NOT NULL,
			metadata_json    TEXT
		);
		CREATE INDEX IF NOT EXISTS idx_mdd_snapshot ON module_discovery_diagnostics(snapshot_uid);
		CREATE INDEX IF NOT EXISTS idx_mdd_snapshot_source ON module_discovery_diagnostics(snapshot_uid, source_type);
		CREATE INDEX IF NOT EXISTS idx_mdd_snapshot_kind ON module_discovery_diagnostics(snapshot_uid, diagnostic_kind);
		"#,
	)?;

	record_migration(conn, 17, "017-module-discovery-diagnostics")?;
	Ok(())
}
