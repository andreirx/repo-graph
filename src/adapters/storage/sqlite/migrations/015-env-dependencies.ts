/**
 * Migration 015: Environment variable dependency tables.
 *
 * Two tables following the identity + evidence pattern:
 *
 * 1. `surface_env_dependencies` — one row per surface/env var dependency.
 *    Identity: (snapshot_uid, project_surface_uid, env_name).
 *    Deduplicates: if the same env var is accessed in multiple files
 *    owned by the same surface, only one dependency row exists.
 *
 * 2. `surface_env_evidence` — one row per source-file occurrence.
 *    Multiple files in the same surface may access the same env var.
 *    Each occurrence is preserved as evidence.
 *
 * Both CASCADE on surface/snapshot/repo deletion.
 */

import type Database from "better-sqlite3";

export function runMigration015(db: Database.Database): void {
	db.exec(`
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
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_env_deps_surface ON surface_env_dependencies(project_surface_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_env_deps_snapshot ON surface_env_dependencies(snapshot_uid)",
	);

	db.exec(`
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
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_env_evidence_dep ON surface_env_evidence(surface_env_dependency_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_env_evidence_snapshot ON surface_env_evidence(snapshot_uid)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (15, '015-env-dependencies', datetime('now'))",
	);
}
