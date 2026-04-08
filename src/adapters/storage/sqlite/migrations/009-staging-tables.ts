/**
 * Migration 009: Staging tables for large-repo indexing.
 *
 * Two transient/derivable staging surfaces that enable the indexer
 * to persist extraction artifacts incrementally per-file rather than
 * accumulating everything in memory before resolution.
 *
 * 1. `staged_edges` — raw extracted unresolved edges persisted
 *    immediately per-file during extraction. The resolution pass
 *    reads them back in batches, resolves/classifies, and writes
 *    to the final `edges` and `unresolved_edges` tables. Staging
 *    rows are deleted after finalization.
 *
 * 2. `file_signals` — per-file classifier context (import bindings)
 *    that cannot be cheaply reconstructed from persisted nodes.
 *    Written during extraction, read during classification.
 *    Kept as long as the snapshot exists (useful for re-classification
 *    without re-extraction).
 *
 * These are NOT part of the public graph model. They are internal
 * staging artifacts used by the indexer's two-phase pipeline.
 *
 * `sameFileSymbols` (value/class/interface sets) are NOT stored here
 * because they can be rebuilt from `nodes WHERE file_uid = ?`.
 */

import type Database from "better-sqlite3";

export function runMigration009(db: Database.Database): void {
	// ── Staged edges (transient) ──────────────────────────────────

	// Staging tables use ON DELETE CASCADE because they are transient
	// snapshot artifacts. When a snapshot or repo is deleted, staging
	// rows must be cleaned up automatically.
	db.exec(`
		CREATE TABLE IF NOT EXISTS staged_edges (
			edge_uid        TEXT PRIMARY KEY,
			snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid        TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			source_node_uid TEXT NOT NULL,
			target_key      TEXT NOT NULL,
			type            TEXT NOT NULL,
			resolution      TEXT NOT NULL,
			extractor       TEXT NOT NULL,
			line_start      INTEGER,
			col_start       INTEGER,
			line_end        INTEGER,
			col_end         INTEGER,
			metadata_json   TEXT,
			source_file_uid TEXT
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_staged_edges_snapshot ON staged_edges(snapshot_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_staged_edges_source ON staged_edges(snapshot_uid, source_file_uid)",
	);

	// ── File signals (import bindings) ────────────────────────────

	db.exec(`
		CREATE TABLE IF NOT EXISTS file_signals (
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			file_uid            TEXT NOT NULL,
			import_bindings_json TEXT,
			PRIMARY KEY (snapshot_uid, file_uid)
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_file_signals_snapshot ON file_signals(snapshot_uid)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (9, '009-staging-tables', datetime('now'))",
	);
}
