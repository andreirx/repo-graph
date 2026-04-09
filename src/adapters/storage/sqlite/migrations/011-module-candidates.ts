/**
 * Migration 011: Module candidate discovery tables.
 *
 * Three tables for machine-derived module discovery facts:
 *
 * 1. `module_candidates` — one row per discovered module root.
 *    Identity anchored by repo-relative root path, not package name.
 *    Snapshot-scoped.
 *
 * 2. `module_candidate_evidence` — one row per evidence item.
 *    A single candidate may have multiple evidence items from
 *    different manifest sources.
 *
 * 3. `module_file_ownership` — one row per file-to-module assignment.
 *    Maps files to their owning discovered module.
 *
 * These are separate from:
 * - `declarations` (human-authored policy)
 * - `inferences` (node-targeted derived facts)
 * - `nodes WHERE kind = 'MODULE'` (directory-derived MODULE nodes)
 *
 * The boundary-facts tables are the architectural precedent:
 * separate fact surfaces for distinct categories of derived knowledge.
 *
 * All three tables cascade on snapshot/repo deletion.
 */

import type Database from "better-sqlite3";

export function runMigration011(db: Database.Database): void {
	// ── Module candidates ─────────────────────────────────────────

	db.exec(`
		CREATE TABLE IF NOT EXISTS module_candidates (
			module_candidate_uid  TEXT PRIMARY KEY,
			snapshot_uid          TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid              TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			module_key            TEXT NOT NULL,
			module_kind           TEXT NOT NULL,
			canonical_root_path   TEXT NOT NULL,
			confidence            REAL NOT NULL,
			display_name          TEXT,
			metadata_json         TEXT,
			UNIQUE (snapshot_uid, module_key)
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_module_candidates_snapshot ON module_candidates(snapshot_uid)",
	);

	// ── Module candidate evidence ─────────────────────────────────

	db.exec(`
		CREATE TABLE IF NOT EXISTS module_candidate_evidence (
			evidence_uid           TEXT PRIMARY KEY,
			module_candidate_uid   TEXT NOT NULL REFERENCES module_candidates(module_candidate_uid) ON DELETE CASCADE,
			snapshot_uid           TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid               TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			source_type            TEXT NOT NULL,
			source_path            TEXT NOT NULL,
			evidence_kind          TEXT NOT NULL,
			confidence             REAL NOT NULL,
			payload_json           TEXT
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_module_evidence_candidate ON module_candidate_evidence(module_candidate_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_module_evidence_snapshot ON module_candidate_evidence(snapshot_uid)",
	);

	// ── Module file ownership ─────────────────────────────────────

	db.exec(`
		CREATE TABLE IF NOT EXISTS module_file_ownership (
			snapshot_uid           TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid               TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			file_uid               TEXT NOT NULL,
			module_candidate_uid   TEXT NOT NULL REFERENCES module_candidates(module_candidate_uid) ON DELETE CASCADE,
			assignment_kind        TEXT NOT NULL,
			confidence             REAL NOT NULL,
			basis_json             TEXT,
			UNIQUE (snapshot_uid, file_uid, module_candidate_uid)
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_module_ownership_snapshot ON module_file_ownership(snapshot_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_module_ownership_file ON module_file_ownership(snapshot_uid, file_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_module_ownership_candidate ON module_file_ownership(module_candidate_uid)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (11, '011-module-candidates', datetime('now'))",
	);
}
