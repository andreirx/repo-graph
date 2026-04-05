/**
 * Migration 006: Add annotations table.
 *
 * Provisional author claims extracted from source artifacts (JSDoc,
 * package descriptions, file headers, READMEs). Stored separately
 * from declarations, measurements, inferences, and nodes/edges
 * because of their distinct trust semantics:
 *
 *   - provisional (not verified against behavior)
 *   - source-attributable (carries exact source span)
 *   - never read by computed-truth surfaces
 *
 * Normative contract: docs/architecture/annotations-contract.txt.
 *
 * Isolation: this table is accessed ONLY through AnnotationsPort
 * (src/core/ports/annotations.ts), which has a grep-based
 * architecture test preventing imports from policy/evaluator/gate/
 * trust/impact paths.
 */

import type Database from "better-sqlite3";

export function runMigration006(db: Database.Database): void {
	db.exec(`
		CREATE TABLE IF NOT EXISTS annotations (
			annotation_uid      TEXT PRIMARY KEY,
			snapshot_uid        TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			target_kind         TEXT NOT NULL,
			target_stable_key   TEXT NOT NULL,
			annotation_kind     TEXT NOT NULL,
			contract_class      TEXT NOT NULL,
			content             TEXT NOT NULL,
			content_hash        TEXT NOT NULL,
			source_file         TEXT NOT NULL,
			source_line_start   INTEGER NOT NULL,
			source_line_end     INTEGER NOT NULL,
			language            TEXT NOT NULL,
			provisional         INTEGER NOT NULL DEFAULT 1,
			extracted_at        TEXT NOT NULL
		)
	`);

	// Query by target stable_key (exact-target retrieval per contract §6)
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_annotations_target ON annotations(snapshot_uid, target_stable_key)",
	);
	// Query by kind (diagnostic summaries, future filters)
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_annotations_kind ON annotations(snapshot_uid, annotation_kind)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (6, '006-annotations', datetime('now'))",
	);
}
