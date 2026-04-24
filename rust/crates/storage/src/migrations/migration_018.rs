//! Migration 018 — add identity fields to project_surfaces table.
//!
//! Adds three columns for stable surface identity:
//! - source_type TEXT: Primary evidence source type (nullable for legacy rows)
//! - source_specific_id TEXT: Source-specific identity string (nullable for legacy)
//! - stable_surface_key TEXT: Snapshot-independent identity (nullable for legacy)
//!
//! Also updates the UNIQUE constraint from the old composite key to use
//! stable_surface_key, which is the new canonical identity.
//!
//! This migration uses FK-safe table rebuild because project_surface_evidence
//! and other child tables have foreign key references to project_surfaces.
//!
//! The pattern:
//! 1. PRAGMA foreign_keys=OFF (outside transaction)
//! 2. Run rebuild in a transaction for atomicity:
//!    a. Create new table with updated schema
//!    b. Copy data (NULL for new identity columns on legacy rows)
//!    c. Drop old table
//!    d. Rename new table
//!    e. Recreate indexes
//! 3. PRAGMA foreign_key_check (ALL tables)
//! 4. Record migration only after FK check passes
//! 5. PRAGMA foreign_keys=ON
//!
//! Mirrors `018-surface-identity.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	// Disable foreign keys for table rebuild.
	// Must be outside transaction — PRAGMA foreign_keys is a no-op within transactions.
	conn.execute_batch("PRAGMA foreign_keys = OFF")?;

	// Run the entire rebuild atomically within a closure that restores FK on exit.
	let result = (|| -> Result<(), StorageError> {
		let tx = conn.transaction()?;

		// Create new table with identity columns and updated UNIQUE constraint.
		tx.execute_batch(
			r#"
			CREATE TABLE project_surfaces_new (
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
				source_type            TEXT,
				source_specific_id     TEXT,
				stable_surface_key     TEXT,
				UNIQUE (snapshot_uid, stable_surface_key)
			)
			"#,
		)?;

		// Copy existing data. New identity columns are NULL for legacy rows.
		tx.execute_batch(
			r#"
			INSERT INTO project_surfaces_new (
				project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid,
				surface_kind, display_name, root_path, entrypoint_path,
				build_system, runtime_kind, confidence, metadata_json,
				source_type, source_specific_id, stable_surface_key
			)
			SELECT
				project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid,
				surface_kind, display_name, root_path, entrypoint_path,
				build_system, runtime_kind, confidence, metadata_json,
				NULL, NULL, NULL
			FROM project_surfaces
			"#,
		)?;

		// Drop old table.
		tx.execute_batch("DROP TABLE project_surfaces")?;

		// Rename new table to canonical name.
		tx.execute_batch("ALTER TABLE project_surfaces_new RENAME TO project_surfaces")?;

		// Recreate indexes.
		tx.execute_batch(
			r#"
			CREATE INDEX IF NOT EXISTS idx_project_surfaces_snapshot ON project_surfaces(snapshot_uid);
			CREATE INDEX IF NOT EXISTS idx_project_surfaces_module ON project_surfaces(module_candidate_uid);
			CREATE INDEX IF NOT EXISTS idx_project_surfaces_stable_key ON project_surfaces(stable_surface_key);
			"#,
		)?;

		tx.commit()?;
		Ok(())
	})();

	// Always restore foreign key enforcement before returning.
	let fk_restore = conn.execute_batch("PRAGMA foreign_keys = ON");

	// If rebuild failed, return that error (FK restore error is secondary).
	result?;

	// If FK restore failed after successful rebuild, return that error.
	fk_restore?;

	// Verify foreign key integrity for ALL tables.
	// This validates that child tables still have valid references.
	let fk_violations: Vec<(String, i64, String, i64)> = {
		let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
		let rows = stmt.query_map([], |row| {
			Ok((
				row.get::<_, String>(0)?,
				row.get::<_, i64>(1)?,
				row.get::<_, String>(2)?,
				row.get::<_, i64>(3)?,
			))
		})?;
		rows.collect::<Result<Vec<_>, _>>()?
	};

	if !fk_violations.is_empty() {
		let summary: String = fk_violations
			.iter()
			.take(5)
			.map(|(table, rowid, parent, _)| format!("{}[{}]->{}", table, rowid, parent))
			.collect::<Vec<_>>()
			.join(", ");
		let suffix = if fk_violations.len() > 5 { "..." } else { "" };
		return Err(StorageError::Migration(format!(
			"Migration 018 FK violation: {} orphaned rows ({}{})",
			fk_violations.len(),
			summary,
			suffix
		)));
	}

	record_migration(conn, 18, "018-surface-identity")?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::migrations::{migration_001, run_migrations};
	use rusqlite::Connection;

	fn fresh_conn() -> Connection {
		Connection::open_in_memory().expect("open in-memory db")
	}

	#[test]
	fn migration_018_adds_identity_columns() {
		let mut conn = fresh_conn();
		// Apply all migrations through 017, then 018.
		run_migrations(&mut conn).expect("run all migrations");

		// Verify the three new columns exist.
		let cols: Vec<String> = {
			let mut stmt = conn.prepare("PRAGMA table_info(project_surfaces)").unwrap();
			stmt.query_map([], |row| row.get::<_, String>("name"))
				.unwrap()
				.collect::<Result<Vec<_>, _>>()
				.unwrap()
		};

		assert!(cols.contains(&"source_type".to_string()));
		assert!(cols.contains(&"source_specific_id".to_string()));
		assert!(cols.contains(&"stable_surface_key".to_string()));
	}

	// NOTE: Removed test `migration_018_idempotent_when_columns_already_exist`.
	// That test called `run()` after `run_migrations()` had already recorded
	// migration 18, which documents behavior that should never happen in the
	// runner (version-gating prevents re-entry). The "idempotent" label was
	// misleading since the rebuild is destructive, not skip-if-done. Testing
	// migration re-run safety is not operational behavior worth covering.

	#[test]
	fn migration_018_preserves_legacy_rows_with_null_identity() {
		let mut conn = fresh_conn();
		conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();

		// Apply migrations 001-017 only.
		migration_001::run(&mut conn).unwrap();
		for v in 2..=17 {
			let max: i64 = conn
				.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))
				.unwrap();
			if max < v {
				match v {
					2 => crate::migrations::migration_002::run(&mut conn).unwrap(),
					3 => crate::migrations::migration_003::run(&mut conn).unwrap(),
					4 => crate::migrations::migration_004::run(&mut conn).unwrap(),
					5 => crate::migrations::migration_005::run(&mut conn).unwrap(),
					6 => crate::migrations::migration_006::run(&mut conn).unwrap(),
					7 => crate::migrations::migration_007::run(&mut conn).unwrap(),
					8 => crate::migrations::migration_008::run(&mut conn).unwrap(),
					9 => crate::migrations::migration_009::run(&mut conn).unwrap(),
					10 => crate::migrations::migration_010::run(&mut conn).unwrap(),
					11 => crate::migrations::migration_011::run(&mut conn).unwrap(),
					12 => crate::migrations::migration_012::run(&mut conn).unwrap(),
					13 => crate::migrations::migration_013::run(&mut conn).unwrap(),
					14 => crate::migrations::migration_014::run(&mut conn).unwrap(),
					15 => crate::migrations::migration_015::run(&mut conn).unwrap(),
					16 => crate::migrations::migration_016::run(&mut conn).unwrap(),
					17 => crate::migrations::migration_017::run(&mut conn).unwrap(),
					_ => {}
				}
			}
		}

		// Insert prerequisite rows for FK constraints.
		conn.execute(
			"INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/abs', '2025-01-01T00:00:00Z')",
			[],
		).unwrap();
		conn.execute(
			"INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'building', '2025-01-01T00:00:00Z')",
			[],
		).unwrap();
		conn.execute(
			"INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, module_key, module_kind, canonical_root_path, confidence) VALUES ('mc1', 's1', 'r1', 'npm:test', 'npm_package', 'packages/test', 1.0)",
			[],
		).unwrap();

		// Insert a legacy surface row (no identity columns yet).
		conn.execute(
			"INSERT INTO project_surfaces (project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid, surface_kind, root_path, build_system, runtime_kind, confidence) VALUES ('ps1', 's1', 'r1', 'mc1', 'cli_tool', 'packages/test', 'npm', 'node', 1.0)",
			[],
		).unwrap();

		// Apply migration 018.
		run(&mut conn).expect("migration 018");

		// Verify the row still exists with NULL identity columns.
		let (uid, source_type, source_specific_id, stable_key): (String, Option<String>, Option<String>, Option<String>) = conn
			.query_row(
				"SELECT project_surface_uid, source_type, source_specific_id, stable_surface_key FROM project_surfaces WHERE project_surface_uid = 'ps1'",
				[],
				|row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
			)
			.unwrap();

		assert_eq!(uid, "ps1");
		assert!(source_type.is_none(), "legacy row source_type must be NULL");
		assert!(source_specific_id.is_none(), "legacy row source_specific_id must be NULL");
		assert!(stable_key.is_none(), "legacy row stable_surface_key must be NULL");
	}
}
