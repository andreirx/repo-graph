//! Migration runner for the storage substrate.
//!
//! Hand-ports the TypeScript migration sequence from
//! `src/adapters/storage/sqlite/connection-provider.ts` and the
//! 16 individual migration files at
//! `src/adapters/storage/sqlite/migrations/`. The Rust runner is
//! the parity-faithful equivalent: same migration sequence, same
//! schema_migrations table semantics, same final database shape.
//!
//! ── Migration source-of-truth strategy (D-C6 lock) ───────────
//!
//! - **Migration 001:** SQL embedded via `include_str!` from the
//!   shared file `src/adapters/storage/sqlite/migrations/001-initial.sql`
//!   per D12. The PRAGMA statements at the top of that file are
//!   stripped before execution because the runner applies pragmas
//!   separately at the connection level (matching the TS
//!   `connection-provider.ts` behavior).
//!
//! - **Migrations 002–016:** Hand-ported as Rust functions, each
//!   in its own `migration_NNN.rs` file. Their SQL is inlined as
//!   Rust string literals matching the TS template literals.
//!   Drift between the TS and Rust migration logic is detected
//!   by the parity harness in R2-F, not by source-of-truth
//!   gymnastics (D-C6 / D3c).
//!
//! ── 001 outdated-vs-002-fills-the-gap fact (explicit) ─────────
//!
//! **Critical fact for fresh Rust initialization:**
//!
//! The shared file `001-initial.sql` is OUTDATED relative to the
//! TypeScript constant `INITIAL_MIGRATION` in `001-initial.ts`.
//! The TS constant includes two columns that the `.sql` file
//! does not:
//!
//! - `snapshots.toolchain_json` (added by migration 002 on the
//!   upgrade path)
//! - `declarations.authored_basis_json` (added by migration 002
//!   on the upgrade path)
//!
//! On the TS side, fresh installs use the constant directly so
//! these columns exist after migration 001. Migration 002 then
//! sees them already present and is a no-op (its column-existence
//! check returns true).
//!
//! On the Rust side, fresh installs use the embedded `.sql` per
//! D12, so these columns DO NOT exist after migration 001.
//! Migration 002's column-existence check returns false and the
//! ALTER TABLE statements actually run, filling the gap.
//!
//! **Both runtimes converge to the same final schema after
//! migration 002 completes.** The intermediate post-001 state
//! differs by exactly two columns. This is acceptable because the
//! cross-runtime DB-file interoperability concern (was D11) was
//! removed from the Rust-2 contract — only the final schema
//! matters for parity.
//!
//! This fact is pinned by the test
//! `migration_001_alone_does_not_have_post_002_columns_then_002_adds_them`
//! in `migration_002`'s tests. Future maintainers should NOT
//! "fix" `001-initial.sql` to include the missing columns
//! without coordinating with the parity harness expectations.
//!
//! ── Runner shape ──────────────────────────────────────────────
//!
//! The runner takes `&mut Connection` (forced by rusqlite's
//! `Connection::transaction()` API per D-C7). It:
//!
//!   1. Sets the connection-level pragmas (`journal_mode = WAL`,
//!      `foreign_keys = ON`).
//!   2. Runs migration 001 unconditionally. Migration 001 uses
//!      `CREATE TABLE IF NOT EXISTS` throughout, so re-running on
//!      an already-initialized database is safe.
//!   3. Reads `MAX(version)` from `schema_migrations`.
//!   4. Conditionally runs migrations 002–016 based on the
//!      max-version gate, mirroring the TS pattern in
//!      `connection-provider.ts`.
//!
//! This API is the minimum the migration runner needs. It does
//! NOT wrap the connection in a higher-level lifecycle struct;
//! that wrapping is R2-D's job.

use rusqlite::Connection;

use crate::error::StorageError;

pub mod migration_001;
pub mod migration_002;
pub mod migration_003;
pub mod migration_004;
pub mod migration_005;
pub mod migration_006;
pub mod migration_007;
pub mod migration_008;
pub mod migration_009;
pub mod migration_010;
pub mod migration_011;
pub mod migration_012;
pub mod migration_013;
pub mod migration_014;
pub mod migration_015;
pub mod migration_016;
pub mod migration_017;
pub mod migration_018;
pub mod migration_019;
pub mod migration_020;
pub mod migration_021;
pub mod migration_022;
pub mod migration_023;

/// Apply all 23 storage migrations to the given connection.
///
/// Sets connection-level pragmas, runs migration 001
/// unconditionally (idempotent via `CREATE TABLE IF NOT EXISTS`),
/// then reads `MAX(version)` from `schema_migrations` and
/// version-gates the application of migrations 002 through 021.
///
/// Mirrors the TypeScript `SqliteConnectionProvider.initialize()`
/// method at `connection-provider.ts:53`.
///
/// Returns `Ok(())` on success. Any underlying rusqlite error is
/// wrapped in `StorageError::Sqlite`. Migration 004 may also
/// produce `StorageError::MalformedRequirement` if it encounters
/// a structurally invalid `value_json` in any pre-existing
/// requirement declaration.
pub fn run_migrations(conn: &mut Connection) -> Result<(), StorageError> {
	// ── Step 1: connection-level pragmas ─────────────────────
	//
	// Mirrors the TS connection-provider lines 56-58:
	//
	//     this.db.pragma("journal_mode = WAL");
	//     this.db.pragma("foreign_keys = ON");
	//
	// `execute_batch` accepts pragma statements as ordinary SQL.
	// Both pragmas are idempotent: re-applying them on an already-
	// configured connection is a no-op.
	conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;

	// ── Step 2: migration 001 (always run, idempotent) ───────
	migration_001::run(conn)?;

	// ── Step 3: read current max version ─────────────────────
	//
	// After migration 001, schema_migrations has at least one row
	// (version 1). MAX returns 1 on a fresh DB; on a re-run, it
	// returns whatever the latest applied version is.
	let max_version: i64 = conn.query_row(
		"SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
		[],
		|row| row.get(0),
	)?;

	// ── Step 4: version-gated migrations 002 through 016 ─────
	//
	// Mirrors the TS pattern at connection-provider.ts:78-94:
	//
	//     if (maxVersion < 2) runMigration002(this.db!);
	//     if (maxVersion < 3) runMigration003(this.db!);
	//     ...
	//
	// Each migration is responsible for inserting its own row
	// into `schema_migrations` after applying its changes.
	if max_version < 2 {
		migration_002::run(conn)?;
	}
	if max_version < 3 {
		migration_003::run(conn)?;
	}
	if max_version < 4 {
		migration_004::run(conn)?;
	}
	if max_version < 5 {
		migration_005::run(conn)?;
	}
	if max_version < 6 {
		migration_006::run(conn)?;
	}
	if max_version < 7 {
		migration_007::run(conn)?;
	}
	if max_version < 8 {
		migration_008::run(conn)?;
	}
	if max_version < 9 {
		migration_009::run(conn)?;
	}
	if max_version < 10 {
		migration_010::run(conn)?;
	}
	if max_version < 11 {
		migration_011::run(conn)?;
	}
	if max_version < 12 {
		migration_012::run(conn)?;
	}
	if max_version < 13 {
		migration_013::run(conn)?;
	}
	if max_version < 14 {
		migration_014::run(conn)?;
	}
	if max_version < 15 {
		migration_015::run(conn)?;
	}
	if max_version < 16 {
		migration_016::run(conn)?;
	}
	if max_version < 17 {
		migration_017::run(conn)?;
	}
	if max_version < 18 {
		migration_018::run(conn)?;
	}
	if max_version < 19 {
		migration_019::run(conn)?;
	}
	if max_version < 20 {
		migration_020::run(conn)?;
	}
	if max_version < 21 {
		migration_021::run(conn)?;
	}
	if max_version < 22 {
		migration_022::run(conn)?;
	}
	if max_version < 23 {
		migration_023::run(conn)?;
	}

	Ok(())
}

// ── Shared helpers for individual migrations ──────────────────────

/// Insert (or ignore) a row into `schema_migrations` recording
/// that the named migration has been applied.
///
/// Mirrors the TS pattern of every migration ending with:
///
/// ```text
/// db.exec(
///   "INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (N, 'NNN-name', datetime('now'))"
/// );
/// ```
///
/// Each migration calls this helper at the end of its body.
/// `INSERT OR IGNORE` makes the insert idempotent: if the row
/// already exists for the same version (because the migration was
/// re-run), the second insert is silently ignored.
///
/// The `applied_at` timestamp is generated by SQLite via
/// `datetime('now')`, identical to the TS code path. Per R2-C
/// decision D-C5, the parity harness must compare schema_migrations
/// rows by `(version, name)` only and ignore `applied_at`.
pub(crate) fn record_migration(
	conn: &Connection,
	version: i64,
	name: &str,
) -> Result<(), StorageError> {
	conn.execute(
		"INSERT OR IGNORE INTO schema_migrations (version, name, applied_at) VALUES (?, ?, datetime('now'))",
		rusqlite::params![version, name],
	)?;
	Ok(())
}

/// Return the column names of a SQLite table via `PRAGMA table_info`.
///
/// Used by migrations 002, 005, and 010 for the column-existence-
/// check pattern (mirroring the TS migrations that check whether
/// an ALTER TABLE column already exists before adding it). The TS
/// equivalent uses `db.prepare("PRAGMA table_info(<table>)").all()`
/// returning rows with a `name` field.
///
/// Returns the column names as `Vec<String>` in the order SQLite
/// reports them. The migrations only need set membership, not
/// order, so this is fine.
pub(crate) fn pragma_table_columns(
	conn: &Connection,
	table: &str,
) -> Result<Vec<String>, StorageError> {
	let sql = format!("PRAGMA table_info({})", table);
	let mut stmt = conn.prepare(&sql)?;
	let cols: Vec<String> = stmt
		.query_map([], |row| row.get::<_, String>("name"))?
		.collect::<Result<Vec<_>, _>>()?;
	Ok(cols)
}

#[cfg(test)]
mod tests {
	//! Integration tests for the migration runner.
	//!
	//! These tests use real `rusqlite::Connection::open_in_memory()`
	//! databases. Each test creates a fresh in-memory database,
	//! applies some subset of migrations, and verifies the
	//! resulting state.
	//!
	//! Test categories (per the R2-C contract):
	//!
	//! 1. Schema creation parity:
	//!    - `run_migrations_applies_all_sixteen_migrations`
	//!    - `schema_dump_includes_all_expected_tables`
	//!
	//! 2. Migration version progression parity:
	//!    - `schema_migrations_records_versions_one_through_sixteen_in_order`
	//!    - `re_running_runner_is_idempotent_no_duplicate_rows`
	//!
	//! 3. Computed migration behavior parity:
	//!    - `migration_001_alone_does_not_have_post_002_columns_then_002_adds_them`
	//!      (the explicit "outdated 001 + fill from 002" pinning test
	//!      per R2-C user constraint)
	//!    - `migration_002_idempotent_when_columns_already_exist`
	//!    - `migration_005_idempotent_when_column_already_exists`
	//!    - `migration_010_idempotent_when_columns_already_exist`
	//!    - `migration_012_copies_surviving_staged_edges_into_extraction_edges`

	use super::*;
	use rusqlite::Connection;

	/// Open a fresh in-memory SQLite connection for tests. Each
	/// test starts with a clean slate.
	fn fresh_conn() -> Connection {
		Connection::open_in_memory().expect("open in-memory db")
	}

	/// Read the column names of a table via PRAGMA. Test-only
	/// wrapper around the helper to avoid the `?` propagation
	/// boilerplate in tests.
	fn columns_of(conn: &Connection, table: &str) -> Vec<String> {
		pragma_table_columns(conn, table).expect("pragma table_info")
	}

	/// Check whether a table exists in `sqlite_master`.
	fn table_exists(conn: &Connection, table: &str) -> bool {
		conn.query_row(
			"SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
			rusqlite::params![table],
			|_| Ok(()),
		)
		.is_ok()
	}

	// ── Category 1: Schema creation parity ────────────────────

	#[test]
	fn run_migrations_applies_all_twenty_three_migrations() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all migrations");

		// schema_migrations table exists and contains rows 1..=23
		let count: i64 = conn
			.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
				row.get(0)
			})
			.unwrap();
		assert_eq!(count, 23, "expected 23 migration rows after full run");
	}

	#[test]
	fn schema_dump_includes_all_expected_tables() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all migrations");

		// All tables introduced by migrations 001-018. The full
		// list pins the schema-creation contract: if any migration
		// silently fails to create one of its tables, this test
		// catches it.
		let expected_tables = [
			// 001-initial
			"repos",
			"snapshots",
			"files",
			"file_versions",
			"nodes",
			"edges",
			"declarations",
			"inferences",
			"artifacts",
			"evidence_links",
			"schema_migrations",
			// 003-measurements
			"measurements",
			// 006-annotations
			"annotations",
			// 007-unresolved-edges
			"unresolved_edges",
			// 008-boundary-facts
			"boundary_provider_facts",
			"boundary_consumer_facts",
			"boundary_links",
			// 009-staging-tables
			"staged_edges",
			"file_signals",
			// 011-module-candidates
			"module_candidates",
			"module_candidate_evidence",
			"module_file_ownership",
			// 012-extraction-edges
			"extraction_edges",
			// 013-project-surfaces
			"project_surfaces",
			"project_surface_evidence",
			// 014-topology-links
			"surface_config_roots",
			"surface_entrypoints",
			// 015-env-dependencies
			"surface_env_dependencies",
			"surface_env_evidence",
			// 016-fs-mutations
			"surface_fs_mutations",
			"surface_fs_mutation_evidence",
			// 017-module-discovery-diagnostics
			"module_discovery_diagnostics",
			// 019-quality-assessments
			"quality_assessments",
			// 020-semantic-facts
			"semantic_facts",
			// 021-status-mappings
			"status_mappings",
			// 022-behavioral-markers
			"behavioral_markers",
			// 023-return-fates
			"return_fates",
		];

		for table in expected_tables {
			assert!(
				table_exists(&conn, table),
				"expected table {} to exist after all migrations",
				table
			);
		}
	}

	// ── Category 2: Migration version progression parity ─────

	#[test]
	fn schema_migrations_records_versions_one_through_twenty_three_in_order() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all migrations");

		let mut stmt = conn
			.prepare("SELECT version, name FROM schema_migrations ORDER BY version")
			.unwrap();
		let rows: Vec<(i64, String)> = stmt
			.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
			.unwrap()
			.collect::<Result<Vec<_>, _>>()
			.unwrap();

		// Per D-C5: parity compares (version, name) only, ignoring
		// applied_at. This test enforces the contract Rust-side.
		let expected: Vec<(i64, &str)> = vec![
			(1, "001-initial"),
			(2, "002-provenance-columns"),
			(3, "003-measurements"),
			(4, "004-obligation-ids"),
			(5, "005-extraction-diagnostics"),
			(6, "006-annotations"),
			(7, "007-unresolved-edges"),
			(8, "008-boundary-facts"),
			(9, "009-staging-tables"),
			(10, "010-file-signals-expansion"),
			(11, "011-module-candidates"),
			(12, "012-extraction-edges"),
			(13, "013-project-surfaces"),
			(14, "014-topology-links"),
			(15, "015-env-dependencies"),
			(16, "016-fs-mutations"),
			(17, "017-module-discovery-diagnostics"),
			(18, "018-surface-identity"),
			(19, "019-quality-assessments"),
			(20, "020-semantic-facts"),
			(21, "021-status-mappings"),
			(22, "022-behavioral-markers"),
			(23, "023-return-fates"),
		];

		assert_eq!(rows.len(), expected.len());
		for (i, (expected_version, expected_name)) in expected.iter().enumerate() {
			assert_eq!(rows[i].0, *expected_version, "row {} version", i);
			assert_eq!(rows[i].1, *expected_name, "row {} name", i);
		}
	}

	#[test]
	fn re_running_runner_is_idempotent_no_duplicate_rows() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("first run");
		run_migrations(&mut conn).expect("second run must not error");

		// Still exactly 23 rows.
		let count: i64 = conn
			.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
				row.get(0)
			})
			.unwrap();
		assert_eq!(count, 23, "re-run must not duplicate schema_migrations rows");

		// Each version still appears exactly once.
		let mut stmt = conn
			.prepare("SELECT version, COUNT(*) FROM schema_migrations GROUP BY version HAVING COUNT(*) > 1")
			.unwrap();
		let dupes: Vec<(i64, i64)> = stmt
			.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
			.unwrap()
			.collect::<Result<Vec<_>, _>>()
			.unwrap();
		assert!(dupes.is_empty(), "duplicate migration versions: {:?}", dupes);
	}

	// ── Category 3: Computed migration behavior parity ───────

	#[test]
	fn migration_001_alone_does_not_have_post_002_columns_then_002_adds_them() {
		// THE EXPLICIT PINNING TEST per R2-C user constraint:
		//
		// "make it explicit in the module docs and tests that
		// fresh Rust initialization is:
		//   - 001 older base schema
		//   - 002 fills the gap
		//   - final post-016 schema is the contract"
		//
		// This test pins the upgrade-path-from-outdated-001 fact.
		// If a future maintainer "fixes" 001-initial.sql to include
		// the post-002 columns directly, this test fails first
		// (snapshots will already have toolchain_json after 001),
		// surfacing the contract change immediately.

		let mut conn = fresh_conn();

		// Apply pragmas (the runner normally does this).
		conn.execute_batch(
			"PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;",
		)
		.unwrap();

		// Apply migration 001 ONLY (bypass the orchestrator).
		migration_001::run(&mut conn).expect("migration 001");

		// Verify: snapshots does NOT have toolchain_json yet.
		let snapshot_cols = columns_of(&conn, "snapshots");
		assert!(
			!snapshot_cols.iter().any(|c| c == "toolchain_json"),
			"OUTDATED 001-initial.sql must NOT include toolchain_json. \
			 If this assertion fires, the .sql file was updated to match \
			 the TS constant — coordinate with R2-F parity expectations."
		);

		// Verify: declarations does NOT have authored_basis_json yet.
		let decl_cols = columns_of(&conn, "declarations");
		assert!(
			!decl_cols.iter().any(|c| c == "authored_basis_json"),
			"OUTDATED 001-initial.sql must NOT include authored_basis_json. \
			 Same coordination concern as above."
		);

		// Now apply migration 002.
		migration_002::run(&mut conn).expect("migration 002");

		// Verify: BOTH columns now exist.
		let snapshot_cols_after = columns_of(&conn, "snapshots");
		assert!(
			snapshot_cols_after.iter().any(|c| c == "toolchain_json"),
			"migration 002 must add toolchain_json to snapshots"
		);

		let decl_cols_after = columns_of(&conn, "declarations");
		assert!(
			decl_cols_after.iter().any(|c| c == "authored_basis_json"),
			"migration 002 must add authored_basis_json to declarations"
		);
	}

	#[test]
	fn migration_002_idempotent_when_columns_already_exist() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all");
		// All migrations have run; columns exist. Re-running 002
		// directly must not error.
		migration_002::run(&mut conn).expect("migration 002 re-run");
	}

	#[test]
	fn migration_005_idempotent_when_column_already_exists() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all");
		migration_005::run(&mut conn).expect("migration 005 re-run");
	}

	#[test]
	fn migration_010_idempotent_when_columns_already_exist() {
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all");
		migration_010::run(&mut conn).expect("migration 010 re-run");
	}

	#[test]
	fn migration_012_copies_surviving_staged_edges_into_extraction_edges() {
		// Pins migration 012's conditional data-copy behavior.
		//
		// Setup: apply migrations 001-011 to get the staging
		// tables (009) but NOT yet the extraction_edges table
		// (012). Insert a row into staged_edges with valid FK
		// references. Apply migration 012. Verify the row landed
		// in extraction_edges.

		let mut conn = fresh_conn();

		// Apply pragmas + migrations 001..=011 manually (bypassing
		// the orchestrator's full run so we can insert before 012).
		conn.execute_batch(
			"PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;",
		)
		.unwrap();
		migration_001::run(&mut conn).unwrap();
		migration_002::run(&mut conn).unwrap();
		migration_003::run(&mut conn).unwrap();
		migration_004::run(&mut conn).unwrap();
		migration_005::run(&mut conn).unwrap();
		migration_006::run(&mut conn).unwrap();
		migration_007::run(&mut conn).unwrap();
		migration_008::run(&mut conn).unwrap();
		migration_009::run(&mut conn).unwrap();
		migration_010::run(&mut conn).unwrap();
		migration_011::run(&mut conn).unwrap();

		// At this point: staged_edges exists, extraction_edges does NOT.
		assert!(table_exists(&conn, "staged_edges"));
		assert!(!table_exists(&conn, "extraction_edges"));

		// Insert prerequisite rows: a repo, a snapshot, and a node
		// to satisfy FK constraints on staged_edges. The FKs
		// reference repos.repo_uid, snapshots.snapshot_uid, and
		// nodes.node_uid (the last via source_node_uid being a
		// plain TEXT in staged_edges, no FK declared, but for
		// realism we still create one).
		conn.execute(
			"INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/abs', '2025-01-01T00:00:00Z')",
			[],
		)
		.unwrap();
		conn.execute(
			"INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'building', '2025-01-01T00:00:00Z')",
			[],
		)
		.unwrap();

		// Insert a staged_edges row.
		conn.execute(
			"INSERT INTO staged_edges
			 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, resolution, extractor)
			 VALUES ('staged-e1', 's1', 'r1', 'src-node', 'target.x', 'CALLS', 'static', 'ts-core:0.1.0')",
			[],
		)
		.unwrap();

		// Sanity: staged row exists.
		let staged_count: i64 = conn
			.query_row("SELECT COUNT(*) FROM staged_edges", [], |row| row.get(0))
			.unwrap();
		assert_eq!(staged_count, 1);

		// Now apply migration 012. This should:
		//   1. Create extraction_edges (table now exists)
		//   2. See staged_edges has 1 row
		//   3. INSERT OR IGNORE INTO extraction_edges SELECT * FROM staged_edges
		migration_012::run(&mut conn).expect("migration 012");

		assert!(table_exists(&conn, "extraction_edges"));

		// Verify the row landed in extraction_edges with the same UID.
		let extraction_count: i64 = conn
			.query_row("SELECT COUNT(*) FROM extraction_edges", [], |row| {
				row.get(0)
			})
			.unwrap();
		assert_eq!(
			extraction_count, 1,
			"migration 012 must copy the surviving staged_edges row"
		);

		let copied_uid: String = conn
			.query_row(
				"SELECT edge_uid FROM extraction_edges WHERE edge_uid = 'staged-e1'",
				[],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(copied_uid, "staged-e1");

		// Apply 013-016 to complete the schema (sanity that
		// nothing later in the chain breaks).
		migration_013::run(&mut conn).unwrap();
		migration_014::run(&mut conn).unwrap();
		migration_015::run(&mut conn).unwrap();
		migration_016::run(&mut conn).unwrap();
	}

	#[test]
	fn migration_012_skips_data_copy_when_staged_edges_is_empty() {
		// Counter-test to the previous: on a fresh install,
		// staged_edges is empty, the count guard returns 0,
		// the data copy is skipped. extraction_edges should
		// exist but have no rows.
		let mut conn = fresh_conn();
		run_migrations(&mut conn).expect("run all");

		assert!(table_exists(&conn, "extraction_edges"));
		let count: i64 = conn
			.query_row("SELECT COUNT(*) FROM extraction_edges", [], |row| {
				row.get(0)
			})
			.unwrap();
		assert_eq!(count, 0, "fresh extraction_edges must be empty");
	}
}
