/**
 * Migration 018 tests — surface identity columns.
 *
 * Verifies:
 * 1. Child rows (project_surface_evidence) survive the FK-safe table rebuild
 *    when upgrading from a pre-018 database with existing data
 * 2. FK check validates ALL tables, not just project_surfaces
 * 3. New columns are nullable (legacy compatibility)
 * 4. New index exists on stable_surface_key
 * 5. Foreign key enforcement is restored after migration
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import Database from "better-sqlite3";
import { randomUUID } from "crypto";
import { tmpdir } from "os";
import { join } from "path";
import { unlinkSync, existsSync } from "fs";
import { SqliteConnectionProvider } from "../../../../src/adapters/storage/sqlite/connection-provider.js";
import { SqliteStorage } from "../../../../src/adapters/storage/sqlite/sqlite-storage.js";
import { INITIAL_MIGRATION } from "../../../../src/adapters/storage/sqlite/migrations/001-initial.js";
import { runMigration002 } from "../../../../src/adapters/storage/sqlite/migrations/002-provenance-columns.js";
import { runMigration003 } from "../../../../src/adapters/storage/sqlite/migrations/003-measurements.js";
import { runMigration004 } from "../../../../src/adapters/storage/sqlite/migrations/004-obligation-ids.js";
import { runMigration005 } from "../../../../src/adapters/storage/sqlite/migrations/005-extraction-diagnostics.js";
import { runMigration006 } from "../../../../src/adapters/storage/sqlite/migrations/006-annotations.js";
import { runMigration007 } from "../../../../src/adapters/storage/sqlite/migrations/007-unresolved-edges.js";
import { runMigration008 } from "../../../../src/adapters/storage/sqlite/migrations/008-boundary-facts.js";
import { runMigration009 } from "../../../../src/adapters/storage/sqlite/migrations/009-staging-tables.js";
import { runMigration010 } from "../../../../src/adapters/storage/sqlite/migrations/010-file-signals-expansion.js";
import { runMigration011 } from "../../../../src/adapters/storage/sqlite/migrations/011-module-candidates.js";
import { runMigration012 } from "../../../../src/adapters/storage/sqlite/migrations/012-extraction-edges.js";
import { runMigration013 } from "../../../../src/adapters/storage/sqlite/migrations/013-project-surfaces.js";
import { runMigration014 } from "../../../../src/adapters/storage/sqlite/migrations/014-topology-links.js";
import { runMigration015 } from "../../../../src/adapters/storage/sqlite/migrations/015-env-dependencies.js";
import { runMigration016 } from "../../../../src/adapters/storage/sqlite/migrations/016-fs-mutations.js";
import { runMigration017 } from "../../../../src/adapters/storage/sqlite/migrations/017-module-discovery-diagnostics.js";
import { runMigration018 } from "../../../../src/adapters/storage/sqlite/migrations/018-surface-identity.js";

/**
 * Create a database at migration 017 level (pre-018).
 * Returns the raw Database handle.
 */
function createDatabaseAtMigration017(dbPath: string): Database.Database {
	const db = new Database(dbPath);
	db.pragma("journal_mode = WAL");
	db.pragma("foreign_keys = ON");

	// Run migration 001 (initial schema)
	const statements = INITIAL_MIGRATION.split(";")
		.map((s) => s.trim())
		.filter((s) => s.length > 0 && !s.startsWith("PRAGMA"));
	for (const stmt of statements) {
		db.exec(`${stmt};`);
	}

	// Run migrations 002-017
	runMigration002(db);
	runMigration003(db);
	runMigration004(db);
	runMigration005(db);
	runMigration006(db);
	runMigration007(db);
	runMigration008(db);
	runMigration009(db);
	runMigration010(db);
	runMigration011(db);
	runMigration012(db);
	runMigration013(db);
	runMigration014(db);
	runMigration015(db);
	runMigration016(db);
	runMigration017(db);

	// Verify we're at migration 017
	const version = db.prepare("SELECT MAX(version) as v FROM schema_migrations").get() as { v: number };
	if (version.v !== 17) {
		throw new Error(`Expected migration 017, got ${version.v}`);
	}

	return db;
}

describe("migration 018: surface identity columns", () => {
	let dbPath: string;
	let db: Database.Database;

	beforeEach(() => {
		dbPath = join(tmpdir(), `rgr-m018-${randomUUID()}.db`);
	});

	afterEach(() => {
		if (db) db.close();
		if (existsSync(dbPath)) unlinkSync(dbPath);
	});

	it("preserves child rows (project_surface_evidence) after table rebuild from pre-018 database", () => {
		// Create database at migration 017 level (pre-018)
		db = createDatabaseAtMigration017(dbPath);

		// Insert test data using the pre-018 schema (no identity columns)
		const repoUid = "test-repo-001";
		const snapshotUid = "test-snap-001";
		const moduleCandidateUid = "test-mc-001";
		const surfaceUid = "test-surface-001";
		const evidenceUid = "test-evidence-001";

		db.exec(`
			INSERT INTO repos (repo_uid, name, root_path, default_branch, created_at)
			VALUES ('${repoUid}', 'test-repo', '/tmp/test', 'main', '2025-01-01T00:00:00.000Z');

			INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at, files_total, nodes_total, edges_total)
			VALUES ('${snapshotUid}', '${repoUid}', 'full', 'ready', '2025-01-01T00:00:00.000Z', 0, 0, 0);

			INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, canonical_root_path, display_name, module_key, module_kind, confidence)
			VALUES ('${moduleCandidateUid}', '${snapshotUid}', '${repoUid}', 'src', 'test-module', 'test-module', 'npm_package', 0.9);

			INSERT INTO project_surfaces (project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid, surface_kind, root_path, build_system, runtime_kind, confidence)
			VALUES ('${surfaceUid}', '${snapshotUid}', '${repoUid}', '${moduleCandidateUid}', 'cli', 'src', 'typescript_tsc', 'node', 0.95);

			INSERT INTO project_surface_evidence (project_surface_evidence_uid, project_surface_uid, snapshot_uid, repo_uid, source_type, source_path, evidence_kind, confidence)
			VALUES ('${evidenceUid}', '${surfaceUid}', '${snapshotUid}', '${repoUid}', 'package_json_bin', 'package.json', 'binary_entrypoint', 0.95);
		`);

		// Verify evidence row exists BEFORE migration
		const evidenceBefore = db.prepare(
			"SELECT * FROM project_surface_evidence WHERE project_surface_evidence_uid = ?",
		).get(evidenceUid) as Record<string, unknown> | undefined;
		expect(evidenceBefore).toBeDefined();
		expect(evidenceBefore?.project_surface_uid).toBe(surfaceUid);

		// Verify pre-018 schema does NOT have identity columns
		const columnsBefore = db.prepare("PRAGMA table_info(project_surfaces)").all() as Array<{ name: string }>;
		expect(columnsBefore.find((c) => c.name === "source_type")).toBeUndefined();
		expect(columnsBefore.find((c) => c.name === "stable_surface_key")).toBeUndefined();

		// Run migration 018
		runMigration018(db);

		// Verify migration 018 was recorded
		const version = db.prepare("SELECT MAX(version) as v FROM schema_migrations").get() as { v: number };
		expect(version.v).toBe(18);

		// Verify evidence row STILL EXISTS after migration
		const evidenceAfter = db.prepare(
			"SELECT * FROM project_surface_evidence WHERE project_surface_evidence_uid = ?",
		).get(evidenceUid) as Record<string, unknown> | undefined;
		expect(evidenceAfter).toBeDefined();
		expect(evidenceAfter?.project_surface_uid).toBe(surfaceUid);

		// Verify surface row survived with NULL identity columns
		const surfaceRow = db.prepare(
			"SELECT * FROM project_surfaces WHERE project_surface_uid = ?",
		).get(surfaceUid) as Record<string, unknown> | undefined;
		expect(surfaceRow).toBeDefined();
		expect(surfaceRow?.module_candidate_uid).toBe(moduleCandidateUid);
		expect(surfaceRow?.source_type).toBeNull();
		expect(surfaceRow?.source_specific_id).toBeNull();
		expect(surfaceRow?.stable_surface_key).toBeNull();

		// Verify FK relationship is intact (evidence → surface)
		const fkCheck = db.prepare("PRAGMA foreign_key_check").all();
		expect(fkCheck).toEqual([]);
	});

	it("adds nullable identity columns for legacy compatibility", () => {
		const provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		db = provider.getDatabase();

		// Check schema for new columns
		const columns = db.prepare("PRAGMA table_info(project_surfaces)").all() as Array<{
			name: string;
			type: string;
			notnull: number;
			dflt_value: unknown;
		}>;

		const sourceType = columns.find((c) => c.name === "source_type");
		const sourceSpecificId = columns.find((c) => c.name === "source_specific_id");
		const stableSurfaceKey = columns.find((c) => c.name === "stable_surface_key");

		// All new columns must exist
		expect(sourceType).toBeDefined();
		expect(sourceSpecificId).toBeDefined();
		expect(stableSurfaceKey).toBeDefined();

		// All new columns must be nullable (notnull = 0)
		expect(sourceType?.notnull).toBe(0);
		expect(sourceSpecificId?.notnull).toBe(0);
		expect(stableSurfaceKey?.notnull).toBe(0);

		// All new columns must be TEXT type
		expect(sourceType?.type).toBe("TEXT");
		expect(sourceSpecificId?.type).toBe("TEXT");
		expect(stableSurfaceKey?.type).toBe("TEXT");

		provider.close();
	});

	it("creates index on stable_surface_key", () => {
		const provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		db = provider.getDatabase();

		const indexes = db.prepare(
			"SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_project_surfaces_stable_key'",
		).all() as Array<{ name: string }>;

		expect(indexes.length).toBe(1);
		expect(indexes[0].name).toBe("idx_project_surfaces_stable_key");

		provider.close();
	});

	it("foreign key enforcement is restored after migration", () => {
		const provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		db = provider.getDatabase();

		// Check that foreign keys are ON
		const fkStatus = db.prepare("PRAGMA foreign_keys").get() as { foreign_keys: number };
		expect(fkStatus.foreign_keys).toBe(1);

		// Attempting to insert evidence with non-existent surface should fail
		expect(() => {
			db.exec(`
				INSERT INTO project_surface_evidence (project_surface_evidence_uid, project_surface_uid, snapshot_uid, repo_uid, source_type, source_path, evidence_kind, confidence)
				VALUES ('orphan-ev', 'non-existent-surface', 'non-existent-snap', 'non-existent-repo', 'package_json_bin', 'package.json', 'binary_entrypoint', 0.95)
			`);
		}).toThrow();

		provider.close();
	});

	it("legacy rows have NULL identity columns", () => {
		// Create database with pre-018 schema, insert surface, then upgrade
		const Database = require("better-sqlite3");
		db = new Database(dbPath);

		// Manually bootstrap to migration 017 level (simplified)
		// We can't easily do this without running all migrations, so we'll
		// use the full init and then verify NULL values work correctly.

		db.close();

		const provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		db = provider.getDatabase();

		// Insert a surface WITHOUT identity columns (they should default to NULL)
		const repoUid = "legacy-repo-001";
		const snapshotUid = "legacy-snap-001";
		const moduleCandidateUid = "legacy-mc-001";
		const surfaceUid = "legacy-surface-001";

		db.exec(`
			INSERT INTO repos (repo_uid, name, root_path, default_branch, created_at)
			VALUES ('${repoUid}', 'legacy-repo', '/tmp/legacy', 'main', '2025-01-01T00:00:00.000Z');

			INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at, files_total, nodes_total, edges_total)
			VALUES ('${snapshotUid}', '${repoUid}', 'full', 'ready', '2025-01-01T00:00:00.000Z', 0, 0, 0);

			INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, canonical_root_path, display_name, module_key, module_kind, confidence)
			VALUES ('${moduleCandidateUid}', '${snapshotUid}', '${repoUid}', 'src', 'legacy-module', 'legacy-module', 'npm_package', 0.9);
		`);

		// Insert surface with only required columns (identity columns will be NULL)
		db.prepare(`
			INSERT INTO project_surfaces (
				project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid,
				surface_kind, root_path, build_system, runtime_kind, confidence
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
		`).run(
			surfaceUid, snapshotUid, repoUid, moduleCandidateUid,
			"library", "src", "typescript_tsc", "node", 0.85
		);

		// Verify identity columns are NULL
		const row = db.prepare(
			"SELECT source_type, source_specific_id, stable_surface_key FROM project_surfaces WHERE project_surface_uid = ?",
		).get(surfaceUid) as {
			source_type: string | null;
			source_specific_id: string | null;
			stable_surface_key: string | null;
		};

		expect(row.source_type).toBeNull();
		expect(row.source_specific_id).toBeNull();
		expect(row.stable_surface_key).toBeNull();

		provider.close();
	});

	it("preserves ALL child tables (evidence, topology, env, fs) after table rebuild", () => {
		// Create database at migration 017 level
		db = createDatabaseAtMigration017(dbPath);

		// Insert test data chain
		const repoUid = "multi-child-repo";
		const snapshotUid = "multi-child-snap";
		const moduleCandidateUid = "multi-child-mc";
		const surfaceUid = "multi-child-surface";

		db.exec(`
			INSERT INTO repos (repo_uid, name, root_path, default_branch, created_at)
			VALUES ('${repoUid}', 'multi-repo', '/tmp/multi', 'main', '2025-01-01T00:00:00.000Z');

			INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at, files_total, nodes_total, edges_total)
			VALUES ('${snapshotUid}', '${repoUid}', 'full', 'ready', '2025-01-01T00:00:00.000Z', 0, 0, 0);

			INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, canonical_root_path, display_name, module_key, module_kind, confidence)
			VALUES ('${moduleCandidateUid}', '${snapshotUid}', '${repoUid}', 'src', 'multi-module', 'multi-module', 'npm_package', 0.9);

			INSERT INTO project_surfaces (project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid, surface_kind, root_path, build_system, runtime_kind, confidence)
			VALUES ('${surfaceUid}', '${snapshotUid}', '${repoUid}', '${moduleCandidateUid}', 'cli', 'src', 'typescript_tsc', 'node', 0.95);
		`);

		// Insert rows in ALL child tables that reference project_surfaces

		// 1. project_surface_evidence (FK to project_surfaces)
		db.exec(`
			INSERT INTO project_surface_evidence (project_surface_evidence_uid, project_surface_uid, snapshot_uid, repo_uid, source_type, source_path, evidence_kind, confidence)
			VALUES ('evidence-001', '${surfaceUid}', '${snapshotUid}', '${repoUid}', 'package_json_bin', 'package.json', 'binary_entrypoint', 0.95)
		`);

		// 2. surface_config_roots (FK to project_surfaces)
		db.exec(`
			INSERT INTO surface_config_roots (surface_config_root_uid, project_surface_uid, snapshot_uid, repo_uid, config_path, config_kind, confidence)
			VALUES ('config-001', '${surfaceUid}', '${snapshotUid}', '${repoUid}', 'tsconfig.json', 'tsconfig', 0.95)
		`);

		// 3. surface_entrypoints (FK to project_surfaces)
		db.exec(`
			INSERT INTO surface_entrypoints (surface_entrypoint_uid, project_surface_uid, snapshot_uid, repo_uid, entrypoint_path, entrypoint_kind, confidence)
			VALUES ('entrypoint-001', '${surfaceUid}', '${snapshotUid}', '${repoUid}', 'src/index.ts', 'main', 0.95)
		`);

		// 4. surface_env_dependencies (FK to project_surfaces)
		db.exec(`
			INSERT INTO surface_env_dependencies (surface_env_dependency_uid, snapshot_uid, repo_uid, project_surface_uid, env_name, access_kind, confidence)
			VALUES ('env-dep-001', '${snapshotUid}', '${repoUid}', '${surfaceUid}', 'DATABASE_URL', 'read', 0.9)
		`);

		// 5. surface_env_evidence (FK to surface_env_dependencies)
		db.exec(`
			INSERT INTO surface_env_evidence (surface_env_evidence_uid, surface_env_dependency_uid, snapshot_uid, repo_uid, source_file_path, line_number, access_pattern, confidence)
			VALUES ('env-evidence-001', 'env-dep-001', '${snapshotUid}', '${repoUid}', 'src/config.ts', 10, 'process.env.DATABASE_URL', 0.85)
		`);

		// 6. surface_fs_mutations (FK to project_surfaces)
		db.exec(`
			INSERT INTO surface_fs_mutations (surface_fs_mutation_uid, snapshot_uid, repo_uid, project_surface_uid, target_path, mutation_kind, confidence)
			VALUES ('fs-mut-001', '${snapshotUid}', '${repoUid}', '${surfaceUid}', '/tmp/output.log', 'write', 0.8)
		`);

		// 7. surface_fs_mutation_evidence (FK to surface_fs_mutations and project_surfaces)
		db.exec(`
			INSERT INTO surface_fs_mutation_evidence (surface_fs_mutation_evidence_uid, surface_fs_mutation_uid, snapshot_uid, repo_uid, project_surface_uid, source_file_path, line_number, mutation_kind, mutation_pattern, dynamic_path, confidence)
			VALUES ('fs-evidence-001', 'fs-mut-001', '${snapshotUid}', '${repoUid}', '${surfaceUid}', 'src/logger.ts', 25, 'write', 'fs.writeFileSync', 0, 0.8)
		`);

		// Run migration 018
		runMigration018(db);

		// Verify ALL child rows still exist and point to the preserved surface

		// project_surface_evidence
		const evidence = db.prepare("SELECT * FROM project_surface_evidence WHERE project_surface_evidence_uid = 'evidence-001'").get();
		expect(evidence).toBeDefined();

		// surface_config_roots
		const config = db.prepare("SELECT * FROM surface_config_roots WHERE surface_config_root_uid = 'config-001'").get();
		expect(config).toBeDefined();

		// surface_entrypoints
		const entrypoint = db.prepare("SELECT * FROM surface_entrypoints WHERE surface_entrypoint_uid = 'entrypoint-001'").get();
		expect(entrypoint).toBeDefined();

		// surface_env_dependencies
		const envDep = db.prepare("SELECT * FROM surface_env_dependencies WHERE surface_env_dependency_uid = 'env-dep-001'").get();
		expect(envDep).toBeDefined();

		// surface_env_evidence
		const envEvidence = db.prepare("SELECT * FROM surface_env_evidence WHERE surface_env_evidence_uid = 'env-evidence-001'").get();
		expect(envEvidence).toBeDefined();

		// surface_fs_mutations
		const fsMut = db.prepare("SELECT * FROM surface_fs_mutations WHERE surface_fs_mutation_uid = 'fs-mut-001'").get();
		expect(fsMut).toBeDefined();

		// surface_fs_mutation_evidence
		const fsEvidence = db.prepare("SELECT * FROM surface_fs_mutation_evidence WHERE surface_fs_mutation_evidence_uid = 'fs-evidence-001'").get();
		expect(fsEvidence).toBeDefined();

		// Full FK check passes
		const fkCheck = db.prepare("PRAGMA foreign_key_check").all();
		expect(fkCheck).toEqual([]);
	});

	it("restores FK enforcement and does not record migration on failure", () => {
		// Create database at migration 017 level
		db = createDatabaseAtMigration017(dbPath);

		// Insert test data that will cause FK violation AFTER the rebuild.
		// We simulate this by inserting an evidence row with a surface_uid,
		// then deleting the surface before running the migration.
		const repoUid = "failure-repo";
		const snapshotUid = "failure-snap";
		const moduleCandidateUid = "failure-mc";
		const surfaceUid = "failure-surface";

		db.exec(`
			INSERT INTO repos (repo_uid, name, root_path, default_branch, created_at)
			VALUES ('${repoUid}', 'failure-repo', '/tmp/failure', 'main', '2025-01-01T00:00:00.000Z');

			INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at, files_total, nodes_total, edges_total)
			VALUES ('${snapshotUid}', '${repoUid}', 'full', 'ready', '2025-01-01T00:00:00.000Z', 0, 0, 0);

			INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, canonical_root_path, display_name, module_key, module_kind, confidence)
			VALUES ('${moduleCandidateUid}', '${snapshotUid}', '${repoUid}', 'src', 'failure-module', 'failure-module', 'npm_package', 0.9);

			INSERT INTO project_surfaces (project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid, surface_kind, root_path, build_system, runtime_kind, confidence)
			VALUES ('${surfaceUid}', '${snapshotUid}', '${repoUid}', '${moduleCandidateUid}', 'cli', 'src', 'typescript_tsc', 'node', 0.95);

			INSERT INTO project_surface_evidence (project_surface_evidence_uid, project_surface_uid, snapshot_uid, repo_uid, source_type, source_path, evidence_kind, confidence)
			VALUES ('orphan-evidence', '${surfaceUid}', '${snapshotUid}', '${repoUid}', 'package_json_bin', 'package.json', 'binary_entrypoint', 0.95);
		`);

		// Temporarily disable FK enforcement to delete the surface without cascade
		db.exec("PRAGMA foreign_keys = OFF");
		db.exec(`DELETE FROM project_surfaces WHERE project_surface_uid = '${surfaceUid}'`);
		db.exec("PRAGMA foreign_keys = ON");

		// Now we have an orphaned evidence row that points to a non-existent surface.
		// The migration's FK check should catch this and fail.

		// Expect migration to throw due to FK violation
		expect(() => runMigration018(db)).toThrow(/FK violation/);

		// Verify FK enforcement is restored (the finally block should have run)
		const fkStatus = db.prepare("PRAGMA foreign_keys").get() as { foreign_keys: number };
		expect(fkStatus.foreign_keys).toBe(1);

		// Verify migration 018 was NOT recorded
		const version = db.prepare("SELECT MAX(version) as v FROM schema_migrations").get() as { v: number };
		expect(version.v).toBe(17); // Still at 017, not 018
	});

	it("rejects new surfaces with null identity fields in insertProjectSurfaces", () => {
		const provider = new SqliteConnectionProvider(dbPath);
		provider.initialize();
		db = provider.getDatabase();
		const storage = new SqliteStorage(db);

		// Set up required parent data
		const repoUid = "null-identity-repo";
		const snapshotUid = "null-identity-snap";
		const moduleCandidateUid = "null-identity-mc";

		db.exec(`
			INSERT INTO repos (repo_uid, name, root_path, default_branch, created_at)
			VALUES ('${repoUid}', 'test-repo', '/tmp/test', 'main', '2025-01-01T00:00:00.000Z');

			INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at, files_total, nodes_total, edges_total)
			VALUES ('${snapshotUid}', '${repoUid}', 'full', 'ready', '2025-01-01T00:00:00.000Z', 0, 0, 0);

			INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, canonical_root_path, display_name, module_key, module_kind, confidence)
			VALUES ('${moduleCandidateUid}', '${snapshotUid}', '${repoUid}', 'src', 'test-module', 'test-module', 'npm_package', 0.9);
		`);

		// Attempt to insert surface with null sourceType
		expect(() => {
			storage.insertProjectSurfaces([{
				projectSurfaceUid: "bad-surface-1",
				snapshotUid,
				repoUid,
				moduleCandidateUid,
				surfaceKind: "cli",
				displayName: "test",
				rootPath: "src",
				entrypointPath: null,
				buildSystem: "typescript_tsc",
				runtimeKind: "node",
				confidence: 0.9,
				metadataJson: null,
				sourceType: null, // Invalid
				sourceSpecificId: "test:src/index.js",
				stableSurfaceKey: "abc123",
			}]);
		}).toThrow(/null sourceType/);

		// Attempt to insert surface with null sourceSpecificId
		expect(() => {
			storage.insertProjectSurfaces([{
				projectSurfaceUid: "bad-surface-2",
				snapshotUid,
				repoUid,
				moduleCandidateUid,
				surfaceKind: "cli",
				displayName: "test",
				rootPath: "src",
				entrypointPath: null,
				buildSystem: "typescript_tsc",
				runtimeKind: "node",
				confidence: 0.9,
				metadataJson: null,
				sourceType: "package_json_bin",
				sourceSpecificId: null, // Invalid
				stableSurfaceKey: "abc123",
			}]);
		}).toThrow(/null sourceSpecificId/);

		// Attempt to insert surface with null stableSurfaceKey
		expect(() => {
			storage.insertProjectSurfaces([{
				projectSurfaceUid: "bad-surface-3",
				snapshotUid,
				repoUid,
				moduleCandidateUid,
				surfaceKind: "cli",
				displayName: "test",
				rootPath: "src",
				entrypointPath: null,
				buildSystem: "typescript_tsc",
				runtimeKind: "node",
				confidence: 0.9,
				metadataJson: null,
				sourceType: "package_json_bin",
				sourceSpecificId: "test:src/index.js",
				stableSurfaceKey: null, // Invalid
			}]);
		}).toThrow(/null stableSurfaceKey/);

		// Valid surface should insert successfully
		expect(() => {
			storage.insertProjectSurfaces([{
				projectSurfaceUid: "good-surface",
				snapshotUid,
				repoUid,
				moduleCandidateUid,
				surfaceKind: "cli",
				displayName: "test",
				rootPath: "src",
				entrypointPath: null,
				buildSystem: "typescript_tsc",
				runtimeKind: "node",
				confidence: 0.9,
				metadataJson: null,
				sourceType: "package_json_bin",
				sourceSpecificId: "test:src/index.js",
				stableSurfaceKey: "abc123def456",
			}]);
		}).not.toThrow();

		provider.close();
	});
});
