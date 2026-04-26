/**
 * Migration 020: Add semantic_facts table.
 *
 * Supports the documentation-derived semantic facts layer.
 * This is a discovery-oriented current-state extraction model:
 * - Docs are read live from disk, not from indexed snapshots
 * - Facts are repo-scoped, not snapshot-scoped
 * - Each extraction run replaces all facts for the repo
 *
 * No snapshot_uid: this layer is explicitly not snapshot-tight.
 * The command reads from the working tree at execution time.
 */

import type Database from "better-sqlite3";

export function runMigration020(db: Database.Database): void {
	db.exec(`
		CREATE TABLE IF NOT EXISTS semantic_facts (
			fact_uid            TEXT PRIMARY KEY,
			repo_uid            TEXT NOT NULL,

			-- Fact classification
			fact_kind           TEXT NOT NULL,
			-- Valid: 'replacement_for', 'alternative_to', 'deprecated_by',
			--        'migration_path', 'environment_surface', 'operational_constraint'

			-- Subject reference (typed for query-time resolution)
			subject_ref         TEXT NOT NULL,
			subject_ref_kind    TEXT NOT NULL,
			-- Valid: 'module', 'symbol', 'file', 'environment', 'text'

			-- Object reference (nullable for some fact kinds)
			object_ref          TEXT,
			object_ref_kind     TEXT,

			-- Provenance (minimal, not full doc text)
			source_file         TEXT NOT NULL,
			source_line_start   INTEGER,
			source_line_end     INTEGER,
			source_text_excerpt TEXT,
			content_hash        TEXT NOT NULL,

			-- Quality
			extraction_method   TEXT NOT NULL,
			-- Valid: 'explicit_marker', 'keyword_pattern', 'config_parse', 'frontmatter'
			confidence          REAL NOT NULL,
			generated           INTEGER NOT NULL DEFAULT 0,
			-- 1 if from generated doc (MAP.md), 0 if authored
			doc_kind            TEXT NOT NULL,
			-- Valid: 'readme', 'architecture', 'config', 'map'

			-- Timestamps
			extracted_at        TEXT NOT NULL,

			-- Foreign key to repos table
			FOREIGN KEY (repo_uid) REFERENCES repos(repo_uid) ON DELETE CASCADE
		);

		-- Primary query patterns
		CREATE INDEX IF NOT EXISTS idx_semantic_facts_repo
			ON semantic_facts(repo_uid);

		CREATE INDEX IF NOT EXISTS idx_semantic_facts_kind
			ON semantic_facts(repo_uid, fact_kind);

		CREATE INDEX IF NOT EXISTS idx_semantic_facts_subject
			ON semantic_facts(repo_uid, subject_ref_kind, subject_ref);

		CREATE INDEX IF NOT EXISTS idx_semantic_facts_doc
			ON semantic_facts(repo_uid, generated, doc_kind);
	`);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (20, '020-semantic-facts', datetime('now'))",
	);
}
