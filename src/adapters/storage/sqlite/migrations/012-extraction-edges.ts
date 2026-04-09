/**
 * Migration 012: Durable extraction edges.
 *
 * Creates `extraction_edges` — the durable raw-edge fact table that
 * replaces `staged_edges` as the canonical store of extracted
 * unresolved edges.
 *
 * Why this exists:
 *   `staged_edges` was designed as transient plumbing: write during
 *   extraction, read during resolution, delete after finalization.
 *   Delta indexing requires raw edge facts to survive finalization
 *   so unchanged files' edges can be copied forward to a new
 *   snapshot without re-extraction.
 *
 *   `extraction_edges` is the durable version. Same columns as
 *   `staged_edges` plus an index on `(snapshot_uid, source_file_uid)`
 *   for per-file copy-forward queries.
 *
 * Migration behavior:
 *   1. Create extraction_edges table
 *   2. Copy any surviving staged_edges rows (e.g., from incomplete runs)
 *   3. staged_edges is NOT dropped in this migration — cleanup is
 *      deferred until the codebase no longer references it
 *
 * Retention policy:
 *   extraction_edges rows are snapshot-scoped and CASCADE on
 *   snapshot/repo deletion. They are NOT deleted after finalization.
 *   They are the durable raw extraction facts.
 */

import type Database from "better-sqlite3";

export function runMigration012(db: Database.Database): void {
	db.exec(`
		CREATE TABLE IF NOT EXISTS extraction_edges (
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

	// Primary access patterns:
	// 1. Batch cursor pagination during resolution: ORDER BY edge_uid
	// 2. Per-file copy-forward during delta indexing: WHERE source_file_uid IN (...)
	// 3. Snapshot-scoped queries: WHERE snapshot_uid = ?
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_extraction_edges_snapshot ON extraction_edges(snapshot_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_extraction_edges_source_file ON extraction_edges(snapshot_uid, source_file_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_extraction_edges_cursor ON extraction_edges(snapshot_uid, edge_uid)",
	);

	// Copy any surviving staged_edges rows into extraction_edges.
	// This handles incomplete runs where staged_edges were not yet deleted.
	const hasStagedEdges = db.prepare(
		"SELECT COUNT(*) AS cnt FROM staged_edges",
	).get() as { cnt: number };
	if (hasStagedEdges.cnt > 0) {
		db.exec(`
			INSERT OR IGNORE INTO extraction_edges
			SELECT * FROM staged_edges
		`);
	}

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (12, '012-extraction-edges', datetime('now'))",
	);
}
