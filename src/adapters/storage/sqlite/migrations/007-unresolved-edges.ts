/**
 * Migration 007: Add unresolved_edges table.
 *
 * Persists every edge the extractor emitted but could not resolve to
 * a concrete target node. Sibling to the `edges` table, not a
 * replacement: resolved facts (FK-clean, confidence 1.0) stay in
 * `edges`; symbolic observations with classification live here.
 *
 * Prior to this migration, only aggregate counts per category were
 * kept in snapshots.extraction_diagnostics_json; individual
 * observations were discarded. That precluded:
 *   - per-bucket sample retrieval (Tier 1b)
 *   - per-edge auditability of classification decisions
 *   - classifier re-runs without re-extracting — provided that the
 *     classifier's input signals (package dependencies, tsconfig
 *     path aliases, per-file import maps) remain accessible from
 *     persisted state or can be cheaply re-derived at reclassify
 *     time. The migration alone does not guarantee that property;
 *     it is a downstream constraint on the classifier implementation.
 *
 * Classification axes are orthogonal:
 *   - `category`: UnresolvedEdgeCategory (extraction failure mode)
 *   - `classification`: {external_library_candidate, internal_candidate, unknown}
 *                       (semantic meaning of the gap)
 *   - `basis_code`: the specific rule match that produced the classification
 *   - `classifier_version`: INTEGER; bumped when rules change, signals backfill
 *
 * Provenance:
 *   - `observed_at`: when this observation row was written. This is
 *     an observation stream, not an authored record — the timestamp
 *     carries observation semantics, not authorship semantics. A
 *     backfill pass rewrites `classifier_version`, `classification`,
 *     and `basis_code`, but NEVER `observed_at`.
 *
 * No FK on target: the target is symbolic (a call expression, an
 * import path, a class name), not a node identifier.
 *
 * FK policy matches `edges`: plain REFERENCES without ON DELETE
 * CASCADE. `unresolved_edges` is the unresolved sibling of `edges`
 * and shares its lifecycle discipline. Snapshot deletion is not a
 * first-class operation in v1.5; when it becomes one, both tables
 * will need coordinated delete ordering and should be revisited
 * together.
 */

import type Database from "better-sqlite3";

export function runMigration007(db: Database.Database): void {
	db.exec(`
		CREATE TABLE IF NOT EXISTS unresolved_edges (
			edge_uid            TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid),
			repo_uid            TEXT NOT NULL REFERENCES repos(repo_uid),
			source_node_uid     TEXT NOT NULL REFERENCES nodes(node_uid),
			target_key          TEXT NOT NULL,
			type                TEXT NOT NULL,
			resolution          TEXT NOT NULL,
			extractor           TEXT NOT NULL,
			line_start          INTEGER,
			col_start           INTEGER,
			line_end            INTEGER,
			col_end             INTEGER,
			metadata_json       TEXT,
			category            TEXT NOT NULL,
			classification      TEXT NOT NULL,
			classifier_version  INTEGER NOT NULL,
			basis_code          TEXT NOT NULL,
			observed_at         TEXT NOT NULL
		)
	`);

	// Tier 1a aggregate counts and Tier 1b per-bucket sample retrieval.
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_unresolved_edges_snapshot_class ON unresolved_edges(snapshot_uid, classification)",
	);
	// Cross-reference with existing trust reliability formulas (category).
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_unresolved_edges_snapshot_category ON unresolved_edges(snapshot_uid, category)",
	);
	// "Which unresolved edges originate from this file/symbol?"
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_unresolved_edges_snapshot_source ON unresolved_edges(snapshot_uid, source_node_uid)",
	);
	// "Which rows still carry old classifier semantics?" (backfill ops).
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_unresolved_edges_snapshot_classifier_version ON unresolved_edges(snapshot_uid, classifier_version)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (7, '007-unresolved-edges', datetime('now'))",
	);
}
