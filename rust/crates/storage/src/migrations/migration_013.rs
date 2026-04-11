//! Migration 013 — project surfaces (operational characterization of modules).
//!
//! Pure SQL: creates `project_surfaces` and
//! `project_surface_evidence` tables plus four indexes. Mirrors
//! `013-project-surfaces.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS project_surfaces (
			project_surface_uid    TEXT PRIMARY KEY,
			snapshot_uid           TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid               TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			module_candidate_uid   TEXT NOT NULL REFERENCES module_candidates(module_candidate_uid) ON DELETE CASCADE,
			surface_kind           TEXT NOT NULL,
			display_name           TEXT,
			root_path              TEXT NOT NULL,
			entrypoint_path        TEXT,
			build_system           TEXT NOT NULL,
			runtime_kind           TEXT NOT NULL,
			confidence             REAL NOT NULL,
			metadata_json          TEXT,
			UNIQUE (snapshot_uid, module_candidate_uid, surface_kind, entrypoint_path)
		);
		CREATE INDEX IF NOT EXISTS idx_project_surfaces_snapshot ON project_surfaces(snapshot_uid);
		CREATE INDEX IF NOT EXISTS idx_project_surfaces_module ON project_surfaces(module_candidate_uid);

		CREATE TABLE IF NOT EXISTS project_surface_evidence (
			project_surface_evidence_uid  TEXT PRIMARY KEY,
			project_surface_uid           TEXT NOT NULL REFERENCES project_surfaces(project_surface_uid) ON DELETE CASCADE,
			snapshot_uid                  TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid                      TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			source_type                   TEXT NOT NULL,
			source_path                   TEXT NOT NULL,
			evidence_kind                 TEXT NOT NULL,
			confidence                    REAL NOT NULL,
			payload_json                  TEXT
		);
		CREATE INDEX IF NOT EXISTS idx_project_surface_evidence_surface ON project_surface_evidence(project_surface_uid);
		CREATE INDEX IF NOT EXISTS idx_project_surface_evidence_snapshot ON project_surface_evidence(snapshot_uid);
		"#,
	)?;

	record_migration(conn, 13, "013-project-surfaces")?;
	Ok(())
}
