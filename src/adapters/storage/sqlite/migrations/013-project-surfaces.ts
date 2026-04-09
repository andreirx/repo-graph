/**
 * Migration 013: Project surfaces — operational characterization of modules.
 *
 * Two tables:
 *
 * 1. `project_surfaces` — how a module is operationalized.
 *    One module can have zero, one, or many surfaces (e.g., CLI + library).
 *    Each surface describes a runnable/deployable/importable aspect.
 *    Linked to `module_candidates` via `module_candidate_uid`.
 *
 * 2. `project_surface_evidence` — per-surface evidence items.
 *    One surface may have multiple signals (bin field + framework dep, etc.).
 *
 * These are separate from module identity (module_candidates) and
 * module evidence (module_candidate_evidence). Module identity says
 * "this root is a declared module." Project surface says "this module
 * runs as X."
 *
 * All rows CASCADE on snapshot/repo deletion.
 */

import type Database from "better-sqlite3";

export function runMigration013(db: Database.Database): void {
	db.exec(`
		CREATE TABLE IF NOT EXISTS project_surfaces (
			project_surface_uid    TEXT PRIMARY KEY,
			snapshot_uid           TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid               TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			module_candidate_uid   TEXT NOT NULL REFERENCES module_candidates(module_candidate_uid) ON DELETE CASCADE,
			surface_kind           TEXT NOT NULL,
			display_name           TEXT,
			root_path              TEXT NOT NULL,
			entrypoint_path        TEXT,
			build_system           TEXT NOT NULL,
			runtime_kind           TEXT NOT NULL,
			confidence             REAL NOT NULL,
			metadata_json          TEXT,
			UNIQUE (snapshot_uid, module_candidate_uid, surface_kind, entrypoint_path)
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_project_surfaces_snapshot ON project_surfaces(snapshot_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_project_surfaces_module ON project_surfaces(module_candidate_uid)",
	);

	db.exec(`
		CREATE TABLE IF NOT EXISTS project_surface_evidence (
			project_surface_evidence_uid  TEXT PRIMARY KEY,
			project_surface_uid           TEXT NOT NULL REFERENCES project_surfaces(project_surface_uid) ON DELETE CASCADE,
			snapshot_uid                  TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
			repo_uid                      TEXT NOT NULL REFERENCES repos(repo_uid) ON DELETE CASCADE,
			source_type                   TEXT NOT NULL,
			source_path                   TEXT NOT NULL,
			evidence_kind                 TEXT NOT NULL,
			confidence                    REAL NOT NULL,
			payload_json                  TEXT
		)
	`);

	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_project_surface_evidence_surface ON project_surface_evidence(project_surface_uid)",
	);
	db.exec(
		"CREATE INDEX IF NOT EXISTS idx_project_surface_evidence_snapshot ON project_surface_evidence(snapshot_uid)",
	);

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (13, '013-project-surfaces', datetime('now'))",
	);
}
