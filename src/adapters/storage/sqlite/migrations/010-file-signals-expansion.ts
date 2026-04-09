/**
 * Migration 010: Expand file_signals with package dependencies and
 * tsconfig aliases.
 *
 * These are file-scoped classifier inputs computed once during
 * extraction (from nearest-owning manifest files). Persisting them
 * here allows the resolution/classification phase to load them
 * per-batch from DB instead of holding a snapshot-wide in-memory
 * cache.
 *
 * NOT stored here: sameFile{Value,Class,Interface}Symbols.
 * Those are derivable from persisted nodes via
 * `querySymbolsByFile(snapshotUid, fileUid)` and would be
 * redundant schema pollution.
 *
 * Column semantics:
 *   package_dependencies_json — JSON-serialized PackageDependencySet.
 *     Nearest-owning package.json/Cargo.toml/build.gradle/pyproject.toml
 *     dependency names. Used by the classifier to distinguish
 *     external_library_candidate from unknown.
 *   tsconfig_aliases_json — JSON-serialized TsconfigAliases.
 *     Nearest-owning tsconfig.json path aliases (compilerOptions.paths).
 *     Used by the classifier to identify project_alias imports.
 */

import type Database from "better-sqlite3";

export function runMigration010(db: Database.Database): void {
	// Add columns to the existing file_signals table.
	// SQLite ALTER TABLE ADD COLUMN is always safe (appends, no rewrite).
	// Guard against re-run: check if columns already exist (can happen
	// if a test DB was created at current schema then markers deleted).
	const columns = db.pragma("table_info(file_signals)") as Array<{ name: string }>;
	const columnNames = new Set(columns.map((c) => c.name));

	if (!columnNames.has("package_dependencies_json")) {
		db.exec(`
			ALTER TABLE file_signals
			ADD COLUMN package_dependencies_json TEXT
		`);
	}

	if (!columnNames.has("tsconfig_aliases_json")) {
		db.exec(`
			ALTER TABLE file_signals
			ADD COLUMN tsconfig_aliases_json TEXT
		`);
	}

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (10, '010-file-signals-expansion', datetime('now'))",
	);
}
