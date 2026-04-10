/**
 * Migration 016: Filesystem mutation tables.
 *
 * Same identity + evidence pattern as env dependencies.
 *
 * 1. `surface_fs_mutations` — one row per (surface, target_path, mutation_kind).
 *    Identity unit. Only literal-path mutations.
 *    Same path + different kind → separate rows.
 *
 * 2. `surface_fs_mutation_evidence` — one row per source-file occurrence.
 *    Both literal and dynamic occurrences are recorded.
 *    Dynamic occurrences have surface_fs_mutation_uid = NULL.
 *
 * Both CASCADE on surface/snapshot/repo deletion.
 */

import type Database from "better-sqlite3";

export function runMigration016(db: Database.Database): void {
	db.exec(`
		CREATE TABLE IF NOT EXISTS surface_fs_mutations (
			surface_fs_mutation_uid  TEXT PRIMARY KEY,
			snapshot_uid             TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid                 TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			project_surface_uid      TEXT NOT NULL REFERENCES project_surfaces(project_surface_uid) ON DELETE CASCADE,
			target_path              TEXT NOT NULL,
			mutation_kind            TEXT NOT NULL,
			confidence               REAL NOT NULL,
			metadata_json            TEXT,
			UNIQUE (snapshot_uid, project_surface_uid, target_path, mutation_kind)
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_fs_mut_surface ON surface_fs_mutations(project_surface_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_fs_mut_snapshot ON surface_fs_mutations(snapshot_uid)",
	);

	db.exec(`
		CREATE TABLE IF NOT EXISTS surface_fs_mutation_evidence (
			surface_fs_mutation_evidence_uid  TEXT PRIMARY KEY,
			surface_fs_mutation_uid           TEXT REFERENCES surface_fs_mutations(surface_fs_mutation_uid) ON DELETE CASCADE,
			snapshot_uid                      TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid                          TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			project_surface_uid               TEXT NOT NULL REFERENCES project_surfaces(project_surface_uid) ON DELETE CASCADE,
			source_file_path                  TEXT NOT NULL,
			line_number                       INTEGER NOT NULL,
			mutation_kind                     TEXT NOT NULL,
			mutation_pattern                  TEXT NOT NULL,
			dynamic_path                      INTEGER NOT NULL,
			confidence                        REAL NOT NULL,
			metadata_json                     TEXT
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_fs_mut_evidence_identity ON surface_fs_mutation_evidence(surface_fs_mutation_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_fs_mut_evidence_surface ON surface_fs_mutation_evidence(project_surface_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_fs_mut_evidence_snapshot ON surface_fs_mutation_evidence(snapshot_uid)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (16, '016-fs-mutations', datetime('now'))",
	);
}
