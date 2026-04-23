/**
 * Migration 018: Add identity fields to project_surfaces table.
 *
 * Adds three columns for stable surface identity:
 * - source_type TEXT: Primary evidence source type (nullable for legacy rows)
 * - source_specific_id TEXT: Source-specific identity string (nullable for legacy)
 * - stable_surface_key TEXT: Snapshot-independent identity (nullable for legacy)
 *
 * Also updates the UNIQUE constraint from the old composite key to use
 * stable_surface_key, which is the new canonical identity.
 *
 * This migration uses FK-safe table rebuild because project_surface_evidence
 * and other child tables have foreign key references to project_surfaces.
 *
 * The pattern:
 * 1. PRAGMA foreign_keys=OFF (outside transaction — cannot change within)
 * 2. Run rebuild in a transaction for atomicity:
 *    a. Create new table with updated schema
 *    b. Copy data (NULL for new identity columns on legacy rows)
 *    c. Drop old table
 *    d. Rename new table
 *    e. Recreate indexes
 * 3. PRAGMA foreign_key_check (ALL tables, not just project_surfaces)
 * 4. Record migration only after FK check passes
 * 5. PRAGMA foreign_keys=ON (in finally block to guarantee restoration)
 *
 * The full FK check validates that child tables (project_surface_evidence,
 * surface_topology_links, surface_env_dependencies, surface_fs_mutations,
 * etc.) still have valid references to the rebuilt project_surfaces table.
 */

import type Database from "better-sqlite3";

export function runMigration018(db: Database.Database): void {
	// Disable foreign keys for table rebuild.
	// Must be outside transaction — PRAGMA foreign_keys is a no-op within transactions.
	db.exec("PRAGMA foreign_keys = OFF");

	try {
		// Run the entire rebuild atomically.
		const rebuild = db.transaction(() => {
			// Create new table with identity columns and updated UNIQUE constraint.
			db.exec(`
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
			`);

			// Copy existing data. New identity columns are NULL for legacy rows.
			db.exec(`
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
			`);

			// Drop old table.
			db.exec("DROP TABLE project_surfaces");

			// Rename new table to canonical name.
			db.exec("ALTER TABLE project_surfaces_new RENAME TO project_surfaces");

			// Recreate indexes.
			db.exec(
				"CREATE INDEX IF NOT EXISTS idx_project_surfaces_snapshot ON project_surfaces(snapshot_uid)",
			);
			db.exec(
				"CREATE INDEX IF NOT EXISTS idx_project_surfaces_module ON project_surfaces(module_candidate_uid)",
			);
			// New index on stable_surface_key for cross-snapshot lookup.
			db.exec(
				"CREATE INDEX IF NOT EXISTS idx_project_surfaces_stable_key ON project_surfaces(stable_surface_key)",
			);
		});
		rebuild();

		// Verify foreign key integrity for ALL tables, not just project_surfaces.
		// This validates that child tables (project_surface_evidence, surface_topology_links,
		// surface_env_dependencies, surface_fs_mutations, etc.) still have valid references
		// to the rebuilt project_surfaces table.
		const fkCheck = db.prepare("PRAGMA foreign_key_check").all() as Array<{
			table: string;
			rowid: number;
			parent: string;
			fkid: number;
		}>;
		if (fkCheck.length > 0) {
			const summary = fkCheck
				.slice(0, 5)
				.map((v) => `${v.table}[${v.rowid}]→${v.parent}`)
				.join(", ");
			throw new Error(
				`Migration 018 FK violation: ${fkCheck.length} orphaned rows (${summary}${fkCheck.length > 5 ? "..." : ""})`,
			);
		}

		// Record migration only after FK check passes.
		db.exec(
			"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (18, '018-surface-identity', datetime('now'))",
		);
	} finally {
		// Always restore foreign key enforcement.
		db.exec("PRAGMA foreign_keys = ON");
	}
}
