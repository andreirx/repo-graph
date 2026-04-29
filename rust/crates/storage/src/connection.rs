//! Connection lifecycle for the storage substrate.
//!
//! Rust mirror of the TypeScript `SqliteConnectionProvider` at
//! `src/adapters/storage/sqlite/connection-provider.ts`. Owns
//! the lifecycle of a single SQLite connection: opens it,
//! applies pragmas, runs migrations, exposes the inner handle
//! to crate-internal CRUD code (R2-E onwards), and closes the
//! connection on drop.
//!
//! ── API shape (R2-D sub-decisions) ────────────────────────────
//!
//! - **D-D1: file + in-memory both exposed.** `open(path)` for
//!   file-backed databases (production use, durability), and
//!   `open_in_memory()` for ephemeral databases (tests, quick
//!   experiments). Mirrors the rusqlite API convention. Both
//!   constructors return `Result<StorageConnection, StorageError>`
//!   so callers handle initialization errors at construction
//!   time, not via uninitialized state.
//!
//! - **D-D2: struct holds `Connection` directly.** Forced by
//!   the locked API shape (`open() -> Result<Self>`). The struct
//!   has no `Option<Connection>` field; if `open` succeeds, the
//!   connection is fully initialized and migrated. There is no
//!   uninitialized state to handle.
//!
//! - **D-D3: crate-internal connection access.** R2-E and beyond
//!   need to call rusqlite methods through the `StorageConnection`
//!   to implement CRUD. The struct exposes
//!   `pub(crate) fn connection(&self) -> &Connection` and
//!   `pub(crate) fn connection_mut(&mut self) -> &mut Connection`
//!   for crate-internal use. External callers cannot reach the
//!   inner handle; they must go through whatever public methods
//!   R2-E adds.
//!
//! ── Lifecycle semantics ───────────────────────────────────────
//!
//! 1. `open(path)` or `open_in_memory()` → opens the SQLite
//!    connection, applies WAL + foreign_keys pragmas via the
//!    migration runner, applies all 21 migrations. Returns
//!    `Ok(StorageConnection)` on success.
//!
//! 2. The migration runner is called via
//!    `migrations::run_migrations(&mut connection)` which itself
//!    sets the pragmas and applies migrations. So `open` does
//!    NOT need to set pragmas separately — that work is owned
//!    by the migration runner per R2-C's design.
//!
//! 3. Re-opening an existing file-backed database is safe and
//!    idempotent: the migrations runner uses `CREATE TABLE IF
//!    NOT EXISTS` and version-gated incremental migrations, so
//!    no statements re-execute against already-migrated state.
//!
//! 4. Drop closes the connection automatically via rusqlite's
//!    `Connection::Drop` implementation. No custom Drop is
//!    needed on `StorageConnection`.
//!
//! ── No explicit `close()` method ──────────────────────────────
//!
//! The TS class has a `close()` method because better-sqlite3
//! requires explicit closing for deterministic file handle
//! release. Rust's rusqlite uses RAII: `Connection::Drop` closes
//! the file handle automatically when the `Connection` is
//! dropped. By holding the `Connection` directly in
//! `StorageConnection`, dropping the `StorageConnection`
//! transitively drops and closes the inner connection. No
//! manual close is needed.
//!
//! Callers that want to control close timing can drop the
//! `StorageConnection` explicitly (`drop(conn)`) or let it go
//! out of scope.

use std::path::Path;

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::run_migrations;

/// Owned, fully-initialized connection to a storage database.
///
/// Construction via `open(path)` or `open_in_memory()` opens the
/// underlying SQLite connection AND runs all 21 migrations
/// before returning. A successfully-constructed
/// `StorageConnection` is guaranteed to be backed by a database
/// at the latest schema version. There is no uninitialized
/// intermediate state.
///
/// `Drop` closes the connection automatically via the rusqlite
/// connection's Drop. No manual `close()` is needed.
///
/// `Debug` derive is required for the `Result::unwrap_err()`
/// pattern in tests (the trait bound on `unwrap_err` requires
/// the Ok variant to be `Debug`). The derive uses
/// `rusqlite::Connection`'s own `Debug` impl, which formats as
/// a opaque handle representation.
#[derive(Debug)]
pub struct StorageConnection {
	/// The underlying rusqlite connection.
	///
	/// `#[allow(dead_code)]` because at R2-D the field is read
	/// only via the `pub(crate)` accessors below, and those
	/// accessors are themselves only called from tests within
	/// this module. R2-E will consume the accessors from CRUD
	/// methods elsewhere in the crate, at which point the
	/// `dead_code` allow becomes a no-op. The allow is targeted
	/// rather than blanket so any other unintended dead code in
	/// this module still surfaces.
	#[allow(dead_code)]
	conn: Connection,
}

impl StorageConnection {
	/// Open or create a file-backed storage database at the given
	/// path, run all migrations, and return the initialized
	/// connection.
	///
	/// If the path does not exist, SQLite creates the file. If
	/// the path exists and contains an already-initialized
	/// storage database, the migration runner detects the
	/// already-applied migrations via the `schema_migrations`
	/// table and skips them, leaving the existing data intact.
	///
	/// The path may be any type implementing `AsRef<Path>`,
	/// including `&str`, `String`, `&Path`, `PathBuf`,
	/// `&PathBuf`, etc.
	///
	/// Returns `StorageError::Sqlite` if SQLite cannot open the
	/// file (e.g., permission denied, invalid path, corrupted
	/// database). Returns `StorageError::MalformedRequirement`
	/// if migration 004 detects a structurally invalid pre-
	/// existing requirement declaration. Other migration errors
	/// also surface via `StorageError`.
	pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
		let mut conn = Connection::open(path.as_ref())?;
		run_migrations(&mut conn)?;
		Ok(Self { conn })
	}

	/// Open an in-memory storage database, run all migrations,
	/// and return the initialized connection.
	///
	/// In-memory databases live entirely in RAM. They are
	/// destroyed when the `StorageConnection` is dropped. Each
	/// call to `open_in_memory()` creates a new, isolated
	/// database — there is no shared state between in-memory
	/// connections.
	///
	/// Used primarily for tests and quick experiments where file
	/// persistence is not needed. Production usage typically
	/// uses `open(path)` for a durable file-backed database.
	///
	/// Returns the same error types as `open(path)`, though file-
	/// system errors are not possible in the in-memory case.
	pub fn open_in_memory() -> Result<Self, StorageError> {
		let mut conn = Connection::open_in_memory()?;
		run_migrations(&mut conn)?;
		Ok(Self { conn })
	}

	/// Crate-internal accessor for the underlying rusqlite
	/// connection.
	///
	/// Used by CRUD methods in the `crud` submodule to issue
	/// SQL against the connection. External callers cannot use
	/// this method because it is `pub(crate)`; they must go
	/// through the public CRUD methods on `StorageConnection`.
	///
	/// Returns a `&Connection` (immutable reference). For
	/// operations that require mutable access (e.g.,
	/// transactions), use `connection_mut()`.
	///
	/// **Why `pub(crate)` and not `pub`:** R2-D locked this as
	/// an internal accessor to keep external callers from
	/// coupling to rusqlite and bypassing the CRUD boundary.
	/// R2-F's parity harness initially required `pub` access to
	/// dump database state for comparison; that widening was
	/// corrected by introducing a narrow
	/// `diagnostic_dump()` method (see `diagnostic.rs`) that
	/// encapsulates the dump logic inside the crate and returns
	/// a `serde_json::Value`. External callers and integration
	/// tests use `diagnostic_dump()`, not the raw connection.
	pub(crate) fn connection(&self) -> &Connection {
		&self.conn
	}

	/// Crate-internal accessor for the underlying rusqlite
	/// connection (mutable).
	///
	/// Required for operations that need `&mut Connection`,
	/// notably `Connection::transaction()` which returns a
	/// `Transaction` that borrows the connection mutably for the
	/// duration of the transaction.
	///
	/// `pub(crate)` for the same reason as `connection()`:
	/// external callers go through public CRUD methods, not
	/// direct connection access. The CRUD methods
	/// (`upsert_files`, `upsert_file_versions`, `insert_nodes`,
	/// `insert_edges`, `delete_nodes_by_file`) internally manage
	/// transaction wrapping.
	pub(crate) fn connection_mut(&mut self) -> &mut Connection {
		&mut self.conn
	}

	/// Diagnostic dump of the database state, returned as a
	/// `serde_json::Value` with the canonical logical-schema
	/// representation plus per-table row data.
	///
	/// **This is the narrow diagnostic surface** that replaces
	/// R2-F's initial `pub fn connection()` widening. It is the
	/// ONLY method that exposes database-state introspection
	/// to external callers (including integration tests).
	/// External code cannot reach the raw rusqlite Connection;
	/// it can only request the canonical diagnostic dump.
	///
	/// Intended use cases:
	///
	///   1. **The R2-F parity harness** at
	///      `rust/crates/storage/tests/parity.rs` calls this
	///      method to get the database state for comparison
	///      against `expected.json` fixtures. The harness then
	///      applies its own normalization and comparison logic
	///      on top of the raw dump.
	///
	///   2. **Ad-hoc debugging** during development when a
	///      developer wants to see what state the database is
	///      in without writing SQL by hand.
	///
	/// Output shape:
	///
	/// ```json
	/// {
	///   "schema": {
	///     "tables": {
	///       "<table_name>": [
	///         { "name": "<col>", "type": "TEXT", "notnull": true, "dflt_value": null, "pk": 0 },
	///         ...
	///       ]
	///     },
	///     "indexes": ["<idx_name>", ...]
	///   },
	///   "tables": {
	///     "<table_name>": [
	///       { "<col>": <value>, ... },
	///       ...
	///     ]
	///   }
	/// }
	/// ```
	///
	/// Schema tables' columns are sorted by column name.
	/// Indexes are sorted by name. Per-table data rows are
	/// sorted by the table's identity column (see
	/// `diagnostic::sort_key_for`). Tables with no rows are
	/// omitted from the `tables` map.
	///
	/// This method does NOT perform any normalization. The
	/// caller is responsible for applying parity-specific
	/// transformations (e.g., replacing dynamic timestamps
	/// with placeholders). See the R2-F harness for the
	/// normalization contract.
	pub fn diagnostic_dump(&self) -> serde_json::Value {
		crate::diagnostic::dump_state(&self.conn)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// ── In-memory open tests ──────────────────────────────────

	#[test]
	fn open_in_memory_returns_a_fully_initialized_connection() {
		let storage = StorageConnection::open_in_memory()
			.expect("open_in_memory must succeed");

		// Verify all 21 migrations have been applied by checking
		// the schema_migrations table count.
		let count: i64 = storage
			.connection()
			.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
				row.get(0)
			})
			.expect("query schema_migrations");
		assert_eq!(
			count, 21,
			"open_in_memory must run all 21 migrations before returning"
		);
	}

	#[test]
	fn open_in_memory_creates_isolated_databases_per_call() {
		// Each open_in_memory() call returns a fresh, isolated
		// database. Inserting a row into one must not affect the
		// other.
		let storage_a = StorageConnection::open_in_memory().unwrap();
		let storage_b = StorageConnection::open_in_memory().unwrap();

		storage_a
			.connection()
			.execute(
				"INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('a', 'a', '/a', '2025-01-01T00:00:00Z')",
				[],
			)
			.unwrap();

		let count_a: i64 = storage_a
			.connection()
			.query_row("SELECT COUNT(*) FROM repos", [], |row| row.get(0))
			.unwrap();
		let count_b: i64 = storage_b
			.connection()
			.query_row("SELECT COUNT(*) FROM repos", [], |row| row.get(0))
			.unwrap();

		assert_eq!(count_a, 1, "storage_a has the inserted row");
		assert_eq!(count_b, 0, "storage_b is isolated and has no rows");
	}

	#[test]
	fn open_in_memory_schema_includes_all_expected_tables() {
		// Sanity: not just schema_migrations rows, but the
		// actual tables also exist.
		let storage = StorageConnection::open_in_memory().unwrap();
		let table_count: i64 = storage
			.connection()
			.query_row(
				"SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
				[],
				|row| row.get(0),
			)
			.unwrap();
		// 21 migrations introduce 34 tables (per the
		// schema_dump_includes_all_expected_tables test in
		// migrations/mod.rs). The exact count is not asserted
		// here to avoid duplicating that test's contract.
		assert!(
			table_count >= 30,
			"expected at least 30 tables after all migrations, got {}",
			table_count
		);
	}

	// ── File-backed open tests ────────────────────────────────

	#[test]
	fn open_creates_a_new_file_backed_database() {
		// Use tempfile to get a unique path that does not yet
		// exist. SQLite creates the file on open.
		let temp_dir = tempfile::tempdir().expect("tempdir");
		let db_path = temp_dir.path().join("test.db");

		assert!(!db_path.exists(), "db file must not exist before open");

		let storage = StorageConnection::open(&db_path).expect("open new file");

		assert!(db_path.exists(), "db file must exist after open");

		// Verify migrations ran.
		let count: i64 = storage
			.connection()
			.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
				row.get(0)
			})
			.unwrap();
		assert_eq!(count, 21);
	}

	#[test]
	fn re_opening_an_existing_file_backed_database_is_idempotent() {
		// Open, write a row, drop. Re-open and verify the row
		// is still there AND no migration errors occur on the
		// re-run.
		let temp_dir = tempfile::tempdir().expect("tempdir");
		let db_path = temp_dir.path().join("test.db");

		// First open: fresh file, runs migrations.
		{
			let storage = StorageConnection::open(&db_path).expect("first open");
			storage
				.connection()
				.execute(
					"INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('persisted', 'p', '/p', '2025-01-01T00:00:00Z')",
					[],
				)
				.unwrap();
			// Drop closes the connection.
		}

		// Second open: existing file, migrations should detect
		// already-applied state via schema_migrations and skip.
		let storage_again =
			StorageConnection::open(&db_path).expect("second open must succeed");

		// Verify the row from the first session is still present.
		let count: i64 = storage_again
			.connection()
			.query_row(
				"SELECT COUNT(*) FROM repos WHERE repo_uid = 'persisted'",
				[],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(
			count, 1,
			"row written in first session must persist across re-open"
		);

		// Verify schema_migrations still has exactly 21 rows
		// (re-open did not duplicate any).
		let migration_count: i64 = storage_again
			.connection()
			.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
				row.get(0)
			})
			.unwrap();
		assert_eq!(
			migration_count, 21,
			"re-open must not duplicate schema_migrations rows"
		);
	}

	#[test]
	fn open_returns_storage_error_on_invalid_path() {
		// A path that points to a non-existent directory cannot
		// be created by SQLite (which only creates the file, not
		// parent directories).
		let bogus_path = "/this/path/definitely/does/not/exist/and/cannot/be/created/test.db";
		let result = StorageConnection::open(bogus_path);
		assert!(result.is_err(), "open on bogus path must fail");
		// The specific error variant should be StorageError::Sqlite
		// since rusqlite is the source of the failure.
		match result.unwrap_err() {
			StorageError::Sqlite(_) => {} // expected
			other => panic!("expected Sqlite variant, got {:?}", other),
		}
	}

	// ── Connection accessor tests ─────────────────────────────

	#[test]
	fn connection_accessor_provides_immutable_access() {
		let storage = StorageConnection::open_in_memory().unwrap();
		let conn: &Connection = storage.connection();
		// Read-only operation through the immutable accessor.
		let count: i64 = conn
			.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
				row.get(0)
			})
			.unwrap();
		assert_eq!(count, 21);
	}

	#[test]
	fn connection_mut_accessor_provides_mutable_access_for_transactions() {
		let mut storage = StorageConnection::open_in_memory().unwrap();
		let conn: &mut Connection = storage.connection_mut();
		// Mutable operation: start a transaction.
		let tx = conn.transaction().expect("begin transaction");
		tx.execute(
			"INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('tx-test', 'tx', '/tx', '2025-01-01T00:00:00Z')",
			[],
		)
		.unwrap();
		tx.commit().expect("commit");

		// Verify the row was committed.
		let count: i64 = storage
			.connection()
			.query_row(
				"SELECT COUNT(*) FROM repos WHERE repo_uid = 'tx-test'",
				[],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(count, 1);
	}

	// ── Drop semantics ────────────────────────────────────────

	#[test]
	fn drop_closes_connection_and_releases_file_handle() {
		// Indirect verification: open, drop, then re-open the
		// same path. If the first connection's file handle was
		// not released, the second open would fail (or block,
		// depending on the OS). Successful re-open is evidence
		// that drop releases the handle.
		let temp_dir = tempfile::tempdir().expect("tempdir");
		let db_path = temp_dir.path().join("drop-test.db");

		{
			let _storage = StorageConnection::open(&db_path).unwrap();
			// _storage drops at the end of this block.
		}

		// Re-open succeeds → drop released the file handle.
		let _storage_2 = StorageConnection::open(&db_path)
			.expect("re-open after drop must succeed");
	}
}
