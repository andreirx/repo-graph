/**
 * Migration 002: Add provenance columns.
 *
 * - snapshots.toolchain_json: snapshot-level toolchain provenance
 * - declarations.authored_basis_json: declaration identity basis
 *
 * See docs/architecture/versioning-model.txt for design.
 *
 * NOTE: This migration is run as TypeScript, not raw SQL, because
 * SQLite lacks ALTER TABLE ... ADD COLUMN IF NOT EXISTS. On fresh
 * databases where 001-initial already includes these columns, the
 * ALTER TABLE would fail with "duplicate column name". The TS
 * function checks column existence before altering.
 */

import type Database from "better-sqlite3";

export function runMigration002(db: Database.Database): void {
	// Check which columns already exist (fresh DBs from updated 001 have them)
	const snapshotCols = db
		.prepare("PRAGMA table_info(snapshots)")
		.all() as Array<{ name: string }>;
	const declCols = db
		.prepare("PRAGMA table_info(declarations)")
		.all() as Array<{ name: string }>;

	const hasToolchainJson = snapshotCols.some(
		(c) => c.name === "toolchain_json",
	);
	const hasAuthoredBasis = declCols.some(
		(c) => c.name === "authored_basis_json",
	);

	if (!hasToolchainJson) {
		db.exec("ALTER TABLE snapshots ADD COLUMN toolchain_json TEXT");
	}
	if (!hasAuthoredBasis) {
		db.exec("ALTER TABLE declarations ADD COLUMN authored_basis_json TEXT");
	}

	db.exec(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (2, '002-provenance-columns', datetime('now'))",
	);
}
