//! Migration 014 — topology link tables.
//!
//! Pure SQL: creates `surface_config_roots` and
//! `surface_entrypoints` tables plus four indexes. Mirrors
//! `014-topology-links.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS surface_config_roots (
			surface_config_root_uid  TEXT PRIMARY KEY,
			project_surface_uid      TEXT NOT NULL REFERENCES project_surfaces(project_surface_uid) ON DELETE CASCADE,
			snapshot_uid             TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid                 TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			config_path              TEXT NOT NULL,
			config_kind              TEXT NOT NULL,
			confidence               REAL NOT NULL,
			metadata_json            TEXT,
			UNIQUE (snapshot_uid, project_surface_uid, config_path)
		);
		CREATE INDEX IF NOT EXISTS idx_surface_config_roots_surface ON surface_config_roots(project_surface_uid);
		CREATE INDEX IF NOT EXISTS idx_surface_config_roots_snapshot ON surface_config_roots(snapshot_uid);

		CREATE TABLE IF NOT EXISTS surface_entrypoints (
			surface_entrypoint_uid   TEXT PRIMARY KEY,
			project_surface_uid      TEXT NOT NULL REFERENCES project_surfaces(project_surface_uid) ON DELETE CASCADE,
			snapshot_uid             TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid                 TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			entrypoint_path          TEXT,
			entrypoint_target        TEXT,
			entrypoint_kind          TEXT NOT NULL,
			display_name             TEXT,
			confidence               REAL NOT NULL,
			metadata_json            TEXT
		);
		CREATE INDEX IF NOT EXISTS idx_surface_entrypoints_surface ON surface_entrypoints(project_surface_uid);
		CREATE INDEX IF NOT EXISTS idx_surface_entrypoints_snapshot ON surface_entrypoints(snapshot_uid);
		"#,
	)?;

	record_migration(conn, 14, "014-topology-links")?;
	Ok(())
}
