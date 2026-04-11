//! Migration 011 — module candidate discovery tables.
//!
//! Pure SQL: creates `module_candidates`,
//! `module_candidate_evidence`, and `module_file_ownership`
//! tables plus five indexes. Mirrors `011-module-candidates.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	conn.execute_batch(
		r#"
		CREATE TABLE IF NOT EXISTS module_candidates (
			module_candidate_uid  TEXT PRIMARY KEY,
			snapshot_uid          TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid              TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			module_key            TEXT NOT NULL,
			module_kind           TEXT NOT NULL,
			canonical_root_path   TEXT NOT NULL,
			confidence            REAL NOT NULL,
			display_name          TEXT,
			metadata_json         TEXT,
			UNIQUE (snapshot_uid, module_key)
		);
		CREATE INDEX IF NOT EXISTS idx_module_candidates_snapshot ON module_candidates(snapshot_uid);

		CREATE TABLE IF NOT EXISTS module_candidate_evidence (
			evidence_uid           TEXT PRIMARY KEY,
			module_candidate_uid   TEXT NOT NULL REFERENCES module_candidates(module_candidate_uid) ON DELETE CASCADE,
			snapshot_uid           TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid               TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			source_type            TEXT NOT NULL,
			source_path            TEXT NOT NULL,
			evidence_kind          TEXT NOT NULL,
			confidence             REAL NOT NULL,
			payload_json           TEXT
		);
		CREATE INDEX IF NOT EXISTS idx_module_evidence_candidate ON module_candidate_evidence(module_candidate_uid);
		CREATE INDEX IF NOT EXISTS idx_module_evidence_snapshot ON module_candidate_evidence(snapshot_uid);

		CREATE TABLE IF NOT EXISTS module_file_ownership (
			snapshot_uid           TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid               TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			file_uid               TEXT NOT NULL,
			module_candidate_uid   TEXT NOT NULL REFERENCES module_candidates(module_candidate_uid) ON DELETE CASCADE,
			assignment_kind        TEXT NOT NULL,
			confidence             REAL NOT NULL,
			basis_json             TEXT,
			UNIQUE (snapshot_uid, file_uid, module_candidate_uid)
		);
		CREATE INDEX IF NOT EXISTS idx_module_ownership_snapshot ON module_file_ownership(snapshot_uid);
		CREATE INDEX IF NOT EXISTS idx_module_ownership_file ON module_file_ownership(snapshot_uid, file_uid);
		CREATE INDEX IF NOT EXISTS idx_module_ownership_candidate ON module_file_ownership(module_candidate_uid);
		"#,
	)?;

	record_migration(conn, 11, "011-module-candidates")?;
	Ok(())
}
