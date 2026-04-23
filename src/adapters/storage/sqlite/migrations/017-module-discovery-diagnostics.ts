/**
 * Migration 017: Module discovery diagnostics table.
 *
 * Persists audit evidence from module discovery detectors.
 * Diagnostics explain why certain build-system patterns were NOT
 * converted to module candidates (e.g., skipped Kconfig-gated
 * assignments in Kbuild files).
 *
 * Diagnostics are snapshot-scoped and CASCADE on snapshot deletion.
 *
 * UID is deterministic: hash of (snapshotUid, sourceType, diagnosticKind,
 * filePath, line, rawText). Duplicate inserts are no-ops.
 */

import type Database from "better-sqlite3";

export function runMigration017(db: Database.Database): void {
	db.exec(`
		CREATE TABLE IF NOT EXISTS module_discovery_diagnostics (
			diagnostic_uid   TEXT PRIMARY KEY,
			snapshot_uid     TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			source_type      TEXT NOT NULL,
			diagnostic_kind  TEXT NOT NULL,
			file_path        TEXT NOT NULL,
			line             INTEGER,
			raw_text         TEXT,
			message          TEXT NOT NULL,
			severity         TEXT NOT NULL,
			metadata_json    TEXT
		)
	`);

	// Primary query path: all diagnostics for a snapshot
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_mdd_snapshot ON module_discovery_diagnostics(snapshot_uid)",
	);

	// Filter by source type (e.g., show only kbuild diagnostics)
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_mdd_snapshot_source ON module_discovery_diagnostics(snapshot_uid, source_type)",
	);

	// Filter by diagnostic kind (e.g., show only skipped_config_gated)
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_mdd_snapshot_kind ON module_discovery_diagnostics(snapshot_uid, diagnostic_kind)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (17, '017-module-discovery-diagnostics', datetime('now'))",
	);
}
