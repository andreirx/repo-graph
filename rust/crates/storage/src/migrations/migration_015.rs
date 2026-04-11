//! Migration 015 — environment variable dependency tables.
//!
//! Pure SQL: creates `surface_env_dependencies` and
//! `surface_env_evidence` tables plus four indexes. Mirrors
//! `015-env-dependencies.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS surface_env_dependencies (
			surface_env_dependency_uid  TEXT PRIMARY KEY,
			snapshot_uid                TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid                    TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			project_surface_uid         TEXT NOT NULL REFERENCES project_surfaces(project_surface_uid) ON DELETE CASCADE,
			env_name                    TEXT NOT NULL,
			access_kind                 TEXT NOT NULL,
			default_value               TEXT,
			confidence                  REAL NOT NULL,
			metadata_json               TEXT,
			UNIQUE (snapshot_uid, project_surface_uid, env_name)
		);
		CREATE INDEX IF NOT EXISTS idx_surface_env_deps_surface ON surface_env_dependencies(project_surface_uid);
		CREATE INDEX IF NOT EXISTS idx_surface_env_deps_snapshot ON surface_env_dependencies(snapshot_uid);

		CREATE TABLE IF NOT EXISTS surface_env_evidence (
			surface_env_evidence_uid     TEXT PRIMARY KEY,
			surface_env_dependency_uid   TEXT NOT NULL REFERENCES surface_env_dependencies(surface_env_dependency_uid) ON DELETE CASCADE,
			snapshot_uid                 TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid                     TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			source_file_path             TEXT NOT NULL,
			line_number                  INTEGER NOT NULL,
			access_pattern               TEXT NOT NULL,
			confidence                   REAL NOT NULL,
			metadata_json                TEXT
		);
		CREATE INDEX IF NOT EXISTS idx_surface_env_evidence_dep ON surface_env_evidence(surface_env_dependency_uid);
		CREATE INDEX IF NOT EXISTS idx_surface_env_evidence_snapshot ON surface_env_evidence(snapshot_uid);
		"#,
	)?;

	record_migration(conn, 15, "015-env-dependencies")?;
	Ok(())
}
