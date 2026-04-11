//! Migration 001 — initial schema.
//!
//! Embeds the canonical SQL source from
//! `src/adapters/storage/sqlite/migrations/001-initial.sql` via
//! `include_str!` per R2 decision D12. The compile-time embed
//! means any change to the shared `.sql` file forces a Rust
//! rebuild — drift between the file on disk and what Rust
//! executes is impossible.
//!
//! ── PRAGMA stripping ──────────────────────────────────────────
//!
//! The shared `.sql` file starts with two PRAGMA statements:
//!
//! ```sql
//! PRAGMA journal_mode = WAL;
//! PRAGMA foreign_keys = ON;
//! ```
//!
//! The TS connection-provider strips these statements before
//! executing the rest of the migration, applying the pragmas
//! separately at the connection level (lines 56-58, 62-65 of
//! `connection-provider.ts`).
//!
//! The Rust runner does the same. PRAGMA statements at the top
//! of the embedded SQL are stripped here so the migration body
//! contains only schema-creating statements. The pragmas are
//! applied separately by `migrations::run_migrations()` before
//! this function is called.
//!
//! ── Idempotency ───────────────────────────────────────────────
//!
//! Every CREATE TABLE / CREATE INDEX statement uses
//! `IF NOT EXISTS`. The final `INSERT OR IGNORE INTO
//! schema_migrations` is also idempotent. Re-running migration
//! 001 on an already-initialized database produces no errors and
//! no state changes.
//!
//! ── Transaction wrapping ──────────────────────────────────────
//!
//! Migration 001 is wrapped in an explicit transaction that
//! commits on success and rolls back on any partial failure.
//! This mirrors the TypeScript runner at
//! `src/adapters/storage/sqlite/connection-provider.ts:65-70`:
//!
//! ```text
//! const runInitial = this.db.transaction(() => {
//!     for (const stmt of statements) {
//!         this.db!.exec(`${stmt};`);
//!     }
//! });
//! runInitial();
//! ```
//!
//! Better-sqlite3's `db.transaction(fn)` returns a function that
//! executes the body in a transaction; if the body throws, the
//! transaction is rolled back automatically. The Rust equivalent
//! uses `Connection::transaction()` which returns a `Transaction`
//! handle; calling `commit()` finalizes, and `Drop` without
//! `commit()` rolls back.
//!
//! **Why the wrap matters even though current 001-initial.sql is
//! all `IF NOT EXISTS`:** the shared `.sql` file is contract
//! input and can evolve. A future change that introduces a
//! non-idempotent statement could fail partway through, leaving
//! partial schema state committed in Rust while TS would roll
//! the entire batch back. Wrapping the Rust path defensively
//! preserves the failure-mode parity guarantee against future
//! `.sql` evolution. The regression test
//! `rollback_on_partial_failure_within_initial_migration`
//! pins this behavior.
//!
//! Migrations 002–016 do NOT use explicit transaction wraps
//! because the corresponding TS migrations also do not. Each
//! statement in those migrations runs in better-sqlite3's
//! autocommit mode (one implicit transaction per `db.exec`),
//! and the Rust ports use `execute_batch` which has the same
//! semantics. The only TS migrations that explicitly wrap are
//! 001 and 004 (the latter wraps because it has computed
//! per-row updates and a hard-fail invariant policy that needs
//! atomic rollback).
//!
//! ── Outdated-vs-final-schema fact ─────────────────────────────
//!
//! The shared `.sql` file is OUTDATED relative to the TypeScript
//! constant `INITIAL_MIGRATION` in `001-initial.ts`. Specifically,
//! the `.sql` file does NOT include:
//!
//! - `snapshots.toolchain_json`
//! - `declarations.authored_basis_json`
//!
//! The TS constant DOES include both. Migration 002 fills these
//! columns in via ALTER TABLE on the upgrade path. The Rust crate
//! always takes the upgrade path (because it always reads the
//! outdated `.sql`); the TS adapter takes the no-op path on fresh
//! installs (because the constant already has the columns).
//!
//! Both runtimes converge to the same final post-002 schema. See
//! `migrations/mod.rs` module docs for the full explanation, and
//! `migration_002.rs` tests for the explicit pinning test.

use rusqlite::Connection;

use crate::error::StorageError;

/// Embedded SQL source for migration 001.
///
/// Compile-time loaded from
/// `src/adapters/storage/sqlite/migrations/001-initial.sql`. The
/// path is relative to this source file: from
/// `rust/crates/storage/src/migrations/migration_001.rs`, five
/// `..` hops reach the repo root, then descend into the TS
/// migration directory.
///
/// If the shared `.sql` file is moved or renamed, this
/// `include_str!` fails at compile time, surfacing the contract
/// drift immediately.
const MIGRATION_001_SQL: &str = include_str!(
	"../../../../../src/adapters/storage/sqlite/migrations/001-initial.sql"
);

/// Run migration 001 against the given connection.
///
/// Delegates to `run_with_sql` using the embedded
/// `MIGRATION_001_SQL` constant. The split exists so the
/// rollback regression test can call `run_with_sql` with
/// deliberately-failing SQL.
///
/// Idempotent: safe to re-run on any database state. The current
/// `001-initial.sql` uses `IF NOT EXISTS` for every schema
/// statement and `INSERT OR IGNORE` for the schema_migrations
/// row, so re-running on an already-initialized database
/// produces no errors and no state changes.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	run_with_sql(conn, MIGRATION_001_SQL)
}

/// Apply an initial-migration SQL string to the given connection
/// inside a transaction.
///
/// Strips PRAGMA statements (the runner applies pragmas
/// separately at the connection level), then executes the
/// remaining statements inside a single SQLite transaction. If
/// any statement fails, the transaction rolls back automatically
/// when the `Transaction` is dropped without commit, leaving the
/// database in its pre-migration state.
///
/// This function is `pub(super)` rather than `pub` because it is
/// only intended to be called by `run()` (production path) and
/// by the rollback regression test in this module's `tests`
/// submodule. External callers should use `run()`.
///
/// Mirrors the TypeScript transaction wrap at
/// `connection-provider.ts:65-70`. See module-level docs for the
/// full rationale.
pub(super) fn run_with_sql(
	conn: &mut Connection,
	sql: &str,
) -> Result<(), StorageError> {
	let stripped = strip_pragmas(sql);
	let tx = conn.transaction()?;
	tx.execute_batch(&stripped)?;
	tx.commit()?;
	Ok(())
}

/// Strip top-level `PRAGMA` statements from a multi-statement SQL
/// string, leaving the remaining statements joined by `;\n`.
///
/// Mirrors the TS pattern at `connection-provider.ts:62-65`:
///
/// ```text
/// const statements = INITIAL_MIGRATION.split(";")
///   .map((s) => s.trim())
///   .filter((s) => s.length > 0 && !s.startsWith("PRAGMA"));
/// ```
///
/// Same naive split-on-semicolon approach. This is safe for the
/// migration 001 source because no string literal in the SQL
/// contains a semicolon. If a future migration adds a string
/// literal with a semicolon, this naive approach would break,
/// but no such case exists in the current corpus.
///
/// `PRAGMA` is matched case-insensitively to be tolerant of
/// future SQL formatting changes.
fn strip_pragmas(sql: &str) -> String {
	let mut out = String::with_capacity(sql.len());
	for stmt in sql.split(';') {
		let trimmed = stmt.trim();
		if trimmed.is_empty() {
			continue;
		}
		if trimmed.to_uppercase().starts_with("PRAGMA") {
			continue;
		}
		out.push_str(trimmed);
		out.push_str(";\n");
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn strip_pragmas_removes_top_level_pragma_statements() {
		let sql = "PRAGMA journal_mode = WAL;\nPRAGMA foreign_keys = ON;\n\nCREATE TABLE foo (id INTEGER);\nCREATE TABLE bar (id INTEGER);";
		let stripped = strip_pragmas(sql);
		assert!(!stripped.to_uppercase().contains("PRAGMA"));
		assert!(stripped.contains("CREATE TABLE foo"));
		assert!(stripped.contains("CREATE TABLE bar"));
	}

	#[test]
	fn strip_pragmas_preserves_non_pragma_statements_in_order() {
		let sql = "CREATE TABLE a (x INTEGER); CREATE TABLE b (y INTEGER); CREATE TABLE c (z INTEGER);";
		let stripped = strip_pragmas(sql);
		let a_pos = stripped.find("CREATE TABLE a").unwrap();
		let b_pos = stripped.find("CREATE TABLE b").unwrap();
		let c_pos = stripped.find("CREATE TABLE c").unwrap();
		assert!(a_pos < b_pos);
		assert!(b_pos < c_pos);
	}

	#[test]
	fn strip_pragmas_handles_empty_segments_between_statements() {
		let sql = "CREATE TABLE a (x INTEGER);;;CREATE TABLE b (y INTEGER);";
		let stripped = strip_pragmas(sql);
		assert!(stripped.contains("CREATE TABLE a"));
		assert!(stripped.contains("CREATE TABLE b"));
	}

	#[test]
	fn strip_pragmas_is_case_insensitive_on_pragma_keyword() {
		let sql = "pragma journal_mode = WAL; PRAGMA foreign_keys = ON; Pragma cache_size = 1000;\nCREATE TABLE foo (id INTEGER);";
		let stripped = strip_pragmas(sql);
		assert!(!stripped.to_lowercase().contains("pragma"));
		assert!(stripped.contains("CREATE TABLE foo"));
	}

	// ── Transaction wrapping (rollback regression) ────────────

	#[test]
	fn rollback_on_partial_failure_within_initial_migration() {
		// Pins the TS-parity transaction wrap.
		//
		// The current `001-initial.sql` uses `CREATE TABLE IF NOT
		// EXISTS` everywhere, so a partial failure within real
		// migration 001 SQL is not reachable today. But the shared
		// `.sql` file is contract input and can evolve. A future
		// change that introduces a non-idempotent statement could
		// fail partway through.
		//
		// This test calls `run_with_sql` with a synthetic SQL
		// string containing two valid CREATE TABLE statements
		// followed by an invalid statement. The transaction wrap
		// must roll back the first two CREATE TABLEs when the
		// third fails. Without the wrap, `execute_batch` would
		// commit the first two before failing on the third,
		// leaving partial schema state.
		//
		// If a future maintainer removes the transaction wrap from
		// `run_with_sql`, this test fails immediately, surfacing
		// the parity regression.
		let sql = "CREATE TABLE rollback_t1 (id INTEGER); CREATE TABLE rollback_t2 (id INTEGER); GARBAGE_NOT_VALID_SQL;";

		let mut conn = rusqlite::Connection::open_in_memory()
			.expect("open in-memory db for rollback test");

		// Apply pragmas (the runner normally does this).
		conn.execute_batch(
			"PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;",
		)
		.unwrap();

		let result = run_with_sql(&mut conn, sql);
		assert!(
			result.is_err(),
			"GARBAGE_NOT_VALID_SQL must produce an error"
		);

		// Verify neither rollback_t1 nor rollback_t2 exist after
		// the failure (they were rolled back).
		let t1_exists = conn
			.query_row(
				"SELECT 1 FROM sqlite_master WHERE type='table' AND name='rollback_t1'",
				[],
				|_| Ok(()),
			)
			.is_ok();
		let t2_exists = conn
			.query_row(
				"SELECT 1 FROM sqlite_master WHERE type='table' AND name='rollback_t2'",
				[],
				|_| Ok(()),
			)
			.is_ok();

		assert!(
			!t1_exists,
			"rollback_t1 must be rolled back when a later statement in the batch fails"
		);
		assert!(
			!t2_exists,
			"rollback_t2 must be rolled back when a later statement in the batch fails"
		);
	}

	#[test]
	fn run_with_sql_commits_on_success() {
		// Counter-test to the rollback test: when all statements
		// succeed, the transaction commits and the resulting
		// tables persist after the function returns.
		let sql = "CREATE TABLE commit_t1 (id INTEGER); CREATE TABLE commit_t2 (id INTEGER);";

		let mut conn = rusqlite::Connection::open_in_memory().unwrap();
		conn.execute_batch(
			"PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;",
		)
		.unwrap();

		run_with_sql(&mut conn, sql).expect("clean SQL must succeed");

		// Verify both tables exist after the function returns.
		let t1_exists = conn
			.query_row(
				"SELECT 1 FROM sqlite_master WHERE type='table' AND name='commit_t1'",
				[],
				|_| Ok(()),
			)
			.is_ok();
		let t2_exists = conn
			.query_row(
				"SELECT 1 FROM sqlite_master WHERE type='table' AND name='commit_t2'",
				[],
				|_| Ok(()),
			)
			.is_ok();

		assert!(t1_exists, "commit_t1 must be committed on success");
		assert!(t2_exists, "commit_t2 must be committed on success");
	}
}
