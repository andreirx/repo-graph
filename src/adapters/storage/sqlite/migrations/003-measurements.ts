/**
 * Migration 003: Add measurements table.
 *
 * Deterministic values computed from graph, AST, or imported evidence.
 * See docs/architecture/measurement-model.txt.
 */

import type Database from "better-sqlite3";

export function runMigration003(db: Database.Database): void {
	db.exec(`
		CREATE TABLE IF NOT EXISTS measurements (
			measurement_uid     TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			target_stable_key   TEXT NOT NULL,
			kind                TEXT NOT NULL,
			value_json          TEXT NOT NULL,
			source              TEXT NOT NULL,
			created_at          TEXT NOT NULL
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_measurements_target ON measurements(snapshot_uid, target_stable_key, kind)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_measurements_kind ON measurements(snapshot_uid, kind)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (3, '003-measurements', datetime('now'))",
	);
}
