/**
 * Migration 014: Topology link tables.
 *
 * Narrow link tables connecting project surfaces to config roots
 * and explicit entrypoints. These persist the stable topology
 * relationships that agents need for current-repo orientation.
 *
 * 1. `surface_config_roots` — which config files govern a surface.
 * 2. `surface_entrypoints` — explicit entrypoints for a surface.
 *
 * Both CASCADE on surface/snapshot/repo deletion.
 */

import type Database from "better-sqlite3";

export function runMigration014(db: Database.Database): void {
	db.exec(`
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
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_config_roots_surface ON surface_config_roots(project_surface_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_config_roots_snapshot ON surface_config_roots(snapshot_uid)",
	);

	db.exec(`
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
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_entrypoints_surface ON surface_entrypoints(project_surface_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_surface_entrypoints_snapshot ON surface_entrypoints(snapshot_uid)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (14, '014-topology-links', datetime('now'))",
	);
}
