/**
 * Migration 005: Add extraction_diagnostics_json column to snapshots.
 *
 * Persists snapshot-level extraction diagnostics that are otherwise
 * lost after the indexer returns. Used by the trust reporting surface
 * (rgr trust <repo>) to answer:
 *   - how many unresolved edges by category
 *   - edges_total at index time
 *   - diagnostics_version for future schema evolution
 *
 * The payload uses machine-stable keys (see
 * src/core/diagnostics/unresolved-edge-categories.ts). Human-readable
 * labels are rendered at display time.
 *
 * Column naming: extraction_diagnostics_json is broader than
 * "unresolved breakdown" — the payload is expected to grow with
 * additional diagnostic facts as the trust surface evolves.
 *
 * The column is nullable. Snapshots created before this migration
 * have NULL diagnostics; the trust command surfaces this as
 * "diagnostics unavailable, re-index to populate."
 */

import type Database from "better-sqlite3";

export function runMigration005(db: Database.Database): void {
	const snapshotCols = db
		.prepare("PRAGMA table_info(snapshots)")
		.all() as Array<{ name: string }>;

	const hasColumn = snapshotCols.some(
		(c) => c.name === "extraction_diagnostics_json",
	);

	if (!hasColumn) {
		db.exec(
			"ALTER TABLE snapshots ADD COLUMN extraction_diagnostics_json TEXT",
		);
	}

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (5, '005-extraction-diagnostics', datetime('now'))",
	);
}
