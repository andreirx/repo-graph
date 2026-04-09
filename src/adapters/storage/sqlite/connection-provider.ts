/**
 * SqliteConnectionProvider — shared SQLite infrastructure.
 *
 * Owns the lifecycle of a single SQLite connection and the schema
 * migration sequence that applies to it. Multiple SQLite-backed
 * adapters (SqliteStorage, SqliteAnnotationsStorage, future read
 * models) share one Database instance through this provider.
 *
 * Rationale: migrations, pragmas, and connection lifecycle are
 * infrastructure concerns that belong to the outer layer. Without
 * a shared provider, each adapter would open its own connection,
 * duplicate pragma/WAL assumptions, and split transaction
 * boundaries — a structural smell documented in the annotations
 * slice architecture decision (C over A/B).
 *
 * Usage (composition root):
 *
 *   const provider = new SqliteConnectionProvider(dbPath);
 *   provider.initialize();
 *   const storage = new SqliteStorage(provider.getDatabase());
 *   const annotations = new SqliteAnnotationsStorage(provider.getDatabase());
 *   // ... use adapters ...
 *   provider.close();
 */

import Database from "better-sqlite3";
import { INITIAL_MIGRATION } from "./migrations/001-initial.js";
import { runMigration002 } from "./migrations/002-provenance-columns.js";
import { runMigration003 } from "./migrations/003-measurements.js";
import { runMigration004 } from "./migrations/004-obligation-ids.js";
import { runMigration005 } from "./migrations/005-extraction-diagnostics.js";
import { runMigration006 } from "./migrations/006-annotations.js";
import { runMigration007 } from "./migrations/007-unresolved-edges.js";
import { runMigration008 } from "./migrations/008-boundary-facts.js";
import { runMigration009 } from "./migrations/009-staging-tables.js";
import { runMigration010 } from "./migrations/010-file-signals-expansion.js";
import { runMigration011 } from "./migrations/011-module-candidates.js";
import { runMigration012 } from "./migrations/012-extraction-edges.js";
import { runMigration013 } from "./migrations/013-project-surfaces.js";
import { runMigration014 } from "./migrations/014-topology-links.js";

export class SqliteConnectionProvider {
	private db: Database.Database | null = null;

	constructor(private readonly dbPath: string) {}

	/**
	 * Open the SQLite connection, set pragmas, and run all migrations.
	 * Idempotent: subsequent calls are no-ops.
	 */
	initialize(): void {
		if (this.db !== null) return;

		this.db = new Database(this.dbPath);
		this.db.pragma("journal_mode = WAL");
		this.db.pragma("foreign_keys = ON");

		// Migration 001: initial schema (CREATE TABLE IF NOT EXISTS — safe to replay)
		const statements = INITIAL_MIGRATION.split(";")
			.map((s) => s.trim())
			.filter((s) => s.length > 0 && !s.startsWith("PRAGMA"));

		const runInitial = this.db.transaction(() => {
			for (const stmt of statements) {
				this.db!.exec(`${stmt};`);
			}
		});
		runInitial();

		// Migrations 002+: incremental, check schema_migrations before acting.
		const runIncremental = this.db.transaction(() => {
			const maxVersion = (
				this.db!
					.prepare("SELECT MAX(version) as v FROM schema_migrations")
					.get() as { v: number }
			).v;
			if (maxVersion < 2) runMigration002(this.db!);
			if (maxVersion < 3) runMigration003(this.db!);
			if (maxVersion < 4) runMigration004(this.db!);
			if (maxVersion < 5) runMigration005(this.db!);
			if (maxVersion < 6) runMigration006(this.db!);
			if (maxVersion < 7) runMigration007(this.db!);
			if (maxVersion < 8) runMigration008(this.db!);
			if (maxVersion < 9) runMigration009(this.db!);
			if (maxVersion < 10) runMigration010(this.db!);
			if (maxVersion < 11) runMigration011(this.db!);
			if (maxVersion < 12) runMigration012(this.db!);
			if (maxVersion < 13) runMigration013(this.db!);
			if (maxVersion < 14) runMigration014(this.db!);
		});
		runIncremental();
	}

	/**
	 * Return the underlying Database handle for adapters to use.
	 * Throws if initialize() has not been called.
	 *
	 * Access is restricted to SQLite adapters in the outer layer.
	 * Core ports MUST NOT depend on Database directly.
	 */
	getDatabase(): Database.Database {
		if (this.db === null) {
			throw new Error(
				"SqliteConnectionProvider not initialized. Call initialize() first.",
			);
		}
		return this.db;
	}

	/**
	 * Close the connection. Safe to call multiple times.
	 */
	close(): void {
		if (this.db !== null) {
			this.db.close();
			this.db = null;
		}
	}
}
