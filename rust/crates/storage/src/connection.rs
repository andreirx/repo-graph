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
//!    migration runner, applies all 33 migrations. Returns
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

use rusqlite::{Connection, OpenFlags};

use crate::error::StorageError;
use crate::migrations::run_migrations;

/// EMBED-SEED-IMPL-1 (operator ruling 2, 2026-08-25): the WAL busy-handler bound.
///
/// The storage databases run `PRAGMA journal_mode = WAL` (set in `run_migrations`)
/// but historically set no busy handler, so a connection that could not immediately
/// acquire a contended lock failed **instantly** with `SQLITE_BUSY`
/// ("database is locked") rather than waiting. That latent defect already bit the
/// operator's real daemon: a long retention reclaim (a multi-minute writer) caused a
/// concurrent `rmap check` to return "database is locked", and the default-on
/// background seed pass makes the window routine.
///
/// `sqlite3_busy_timeout(5000)` installs SQLite's built-in busy handler: a contended
/// lock now WAITS (retrying) up to 5s before it gives up. This is honest, not a
/// hang: a writer that holds the lock **longer** than the bound still yields a real
/// `SQLITE_BUSY` error — the wait is finite, the failure is truthful, and the common
/// sub-second contention (reclaim/seed vs. a request) resolves transparently.
const BUSY_TIMEOUT_MS: u64 = 5000;

/// Install the WAL busy handler (see [`BUSY_TIMEOUT_MS`]) on a freshly-opened
/// file-backed connection. This is the single init choke point for the timeout,
/// called by [`StorageConnection::open`] and [`StorageConnection::open_existing`].
///
/// `open_in_memory` is intentionally EXCLUDED: each `:memory:` database is private
/// to its one connection (there is no second opener to contend with), so a busy
/// handler is meaningless there — and the exclusion keeps the in-memory test-path
/// timing byte-for-byte unchanged (operator ruling 2: "in-memory excluded").
fn set_busy_timeout(conn: &Connection) -> Result<(), StorageError> {
    conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS))?;
    Ok(())
}

/// Owned, fully-initialized connection to a storage database.
///
/// Construction via `open(path)` or `open_in_memory()` opens the
/// underlying SQLite connection AND runs all 33 migrations
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
        set_busy_timeout(&conn)?;
        run_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    /// Open an EXISTING file-backed storage database WITHOUT creating it, run the
    /// idempotent migration check, and return the initialized connection.
    ///
    /// This is the NO-CREATE counterpart to [`open`](Self::open) and the constructor
    /// every serving/read path (and every writer on an already-registered DB) must
    /// use. Only the index/registration path may bring a database into existence
    /// (via [`open`](Self::open)); everywhere else, a missing file is an honest
    /// [`StorageError::DatabaseMissing`], NOT a freshly-created empty database.
    ///
    /// ## Why this exists (FORGET-REPO-1, operator ruling 2, 2026-08-24)
    ///
    /// [`open`](Self::open) passes SQLite `SQLITE_OPEN_CREATE`, so it materialises and
    /// migrates a missing file. On a serving path that is a *read that writes*: after
    /// a repo is forgotten, a request still holding a stale `RepoState` handle could
    /// reopen its deleted DB and recreate it as an unregistered orphan — the exact
    /// condition FORGET-REPO-1 must prevent. Opening without `SQLITE_OPEN_CREATE`
    /// closes that resurrection at the choke point: the stale open fails honestly and
    /// no file is written.
    ///
    /// ## Behaviour
    ///
    /// - Missing file → [`StorageError::DatabaseMissing`] (the absence fact), whether
    ///   the miss is caught by the pre-check or by SQLite's `CANTOPEN` — never a bare
    ///   SQLite/io error leaks for the missing-file case.
    /// - Present but unopenable (corruption, permissions) → [`StorageError::Sqlite`],
    ///   the true I/O fault.
    /// - Present and healthy → identical to [`open`](Self::open): the idempotent
    ///   migration check runs (no create-time DDL executes on an already-migrated DB;
    ///   this constructor does NOT introduce a fast-open that could serve an unmigrated
    ///   schema — same §14 honesty guarantee as [`open`](Self::open)).
    pub fn open_existing<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let path = path.as_ref();
        // No SQLITE_OPEN_CREATE: this is the default open-flag set MINUS create.
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        match Connection::open_with_flags(path, flags) {
            Ok(mut conn) => {
                set_busy_timeout(&conn)?;
                run_migrations(&mut conn)?;
                Ok(Self { conn })
            }
            // Map the missing-file case to the honest absence error regardless of how it
            // surfaced (this recheck also closes the pre-check TOCTOU: a file deleted between
            // check and open still reports DatabaseMissing, not a bare CANTOPEN). review-10:
            // classify with `fs::metadata`, NOT `exists()` — `exists()` collapses EVERY metadata
            // fault (permission denied, ENOTDIR on an ancestor) to `false`, which would mislabel a
            // real I/O fault as "database missing". Only a genuine NotFound is absence; any other
            // stat outcome (present, or an unstattable path) keeps the true `Sqlite` fault.
            Err(e) => match std::fs::metadata(path) {
                Err(meta_err) if meta_err.kind() == std::io::ErrorKind::NotFound => {
                    Err(StorageError::DatabaseMissing {
                        path: path.display().to_string(),
                    })
                }
                _ => Err(StorageError::Sqlite(e)),
            },
        }
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

    /// Obtain a [`StorageInterruptHandle`] — a `Send + Sync` handle another thread
    /// can use to abort whatever SQL statement THIS connection is currently running
    /// (DAEMON-CANCEL-2).
    ///
    /// ## Why this exists (the SQL-statement-granular cancel actuator)
    ///
    /// DAEMON-CANCEL-1's worker supervisor cancels a heavy *Rust* loop by polling a
    /// cooperative flag at bounded-interval checkpoints. That is useless for a single
    /// opaque `SELECT` (e.g. [`compute_module_stats`](Self::compute_module_stats)):
    /// the worker thread is blocked *inside* SQLite's C VM, with no Rust frame to
    /// poll a flag. The only way to abort such a statement from another thread is
    /// SQLite's `sqlite3_interrupt`, which rusqlite exposes as
    /// `Connection::get_interrupt_handle`. This wraps it.
    ///
    /// ## Ownership shape (B1 connection-per-operation, the slice's key design point)
    ///
    /// Obtain the handle from the OWNED connection BEFORE handing that connection to
    /// a worker thread: the daemon read handler holds its per-operation
    /// `StorageConnection` (D-S = S-A) as a local, hoists this handle out, moves the
    /// connection into the worker, and gives the handle to the supervising
    /// (transport) thread. On peer-disconnect the supervisor fires
    /// [`interrupt`](StorageInterruptHandle::interrupt); the in-flight statement
    /// aborts with `SQLITE_INTERRUPT` (`rusqlite::ErrorCode::OperationInterrupted`),
    /// surfaced as [`StorageError::Sqlite`](crate::error::StorageError::Sqlite).
    ///
    /// Firing the handle after the connection has closed is a guaranteed no-op (never
    /// a use-after-free): see [`StorageInterruptHandle`].
    pub fn interrupt_handle(&self) -> StorageInterruptHandle {
        StorageInterruptHandle {
            inner: self.conn.get_interrupt_handle(),
        }
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

    // ══════════════════════════════════════════════════════════════════
    // Test helpers — public for cross-crate integration tests
    // ══════════════════════════════════════════════════════════════════

    /// Execute raw SQL (no results). For test fixtures only.
    ///
    /// # Panics
    ///
    /// Panics on SQL error. Intended for test setup only.
    pub fn execute_raw(&self, sql: &str) -> Result<usize, crate::error::StorageError> {
        Ok(self.conn.execute(sql, [])?)
    }

    /// Query a single scalar value. For test assertions only.
    ///
    /// Returns `StorageError` if query fails or returns no rows.
    pub fn query_scalar<T: rusqlite::types::FromSql>(
        &self,
        sql: &str,
    ) -> Result<T, crate::error::StorageError> {
        Ok(self.conn.query_row(sql, [], |row| row.get(0))?)
    }
}

/// A thread-crossing handle that aborts a [`StorageConnection`]'s in-flight SQL
/// statement (DAEMON-CANCEL-2). Obtain it via
/// [`StorageConnection::interrupt_handle`].
///
/// This is the SQL-statement-granular counterpart to the Rust-loop cooperative
/// checkpoint: a single opaque statement cannot be checkpointed mid-execution from
/// Rust, so a supervising thread aborts it with `sqlite3_interrupt` (which the inner
/// `rusqlite::InterruptHandle` wraps).
///
/// ## `Send + Sync`
///
/// The inner `rusqlite::InterruptHandle` is `Send + Sync` by construction, so this
/// is too. That is the whole point: the connection lives on (and is `!Sync` to) the
/// worker thread, while this handle crosses to the supervising thread.
///
/// ## Safe after the connection closes (no use-after-free)
///
/// The handle does NOT borrow the connection. Internally it shares an
/// `Arc<Mutex<*mut sqlite3>>` with the connection; the connection's close/drop
/// nulls that pointer *under the same mutex* that [`interrupt`](Self::interrupt)
/// locks (rusqlite's design). So `interrupt` either runs before close (a real
/// interrupt) or after (it observes the null pointer and no-ops) — never a dangling
/// dereference. Combined with B1's connection-per-operation model (each query opens
/// and drops its own connection, no reuse), a "late" or "no-statement-running"
/// interrupt cannot bleed into any later statement: there is none.
pub struct StorageInterruptHandle {
    inner: rusqlite::InterruptHandle,
}

impl StorageInterruptHandle {
    /// Abort the statement the originating connection is currently executing, from
    /// another thread. The aborted statement fails with `SQLITE_INTERRUPT`
    /// (`rusqlite::ErrorCode::OperationInterrupted`). Idempotent; a no-op when no
    /// statement is running or the connection has closed (see the type docs).
    pub fn interrupt(&self) {
        self.inner.interrupt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── In-memory open tests ──────────────────────────────────

    #[test]
    fn open_in_memory_returns_a_fully_initialized_connection() {
        let storage = StorageConnection::open_in_memory().expect("open_in_memory must succeed");

        // Verify all 33 migrations have been applied by checking
        // the schema_migrations table count.
        let count: i64 = storage
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("query schema_migrations");
        assert_eq!(
            count, 34,
            "open_in_memory must run all 34 migrations before returning"
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
        // 26 migrations introduce 41 tables (per the
        // schema_dump_includes_all_expected_tables test in
        // migrations/mod.rs). The exact count is not asserted
        // here to avoid duplicating that test's contract.
        assert!(
            table_count >= 31,
            "expected at least 31 tables after all migrations, got {}",
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
        assert_eq!(count, 34);
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
        let storage_again = StorageConnection::open(&db_path).expect("second open must succeed");

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

        // Verify schema_migrations still has exactly 33 rows
        // (re-open did not duplicate any).
        let migration_count: i64 = storage_again
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            migration_count, 34,
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

    // ── open_existing: NO-CREATE constructor (FORGET-REPO-1) ──

    #[test]
    fn open_existing_on_missing_file_errors_and_creates_nothing() {
        // The core FORGET-REPO-1 guarantee: a serving/read open of a DB that is not
        // there must fail honestly AND must NOT materialise the file (no read-that-
        // writes / orphan resurrection).
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("absent.db");
        assert!(!db_path.exists());

        let err = StorageConnection::open_existing(&db_path).unwrap_err();
        match err {
            StorageError::DatabaseMissing { path } => {
                assert!(path.contains("absent.db"), "path carried in error: {path}");
            }
            other => panic!("expected DatabaseMissing, got {other:?}"),
        }
        assert!(
            !db_path.exists(),
            "open_existing must NOT create the missing DB file"
        );
    }

    #[test]
    fn open_existing_maps_non_notfound_fault_to_sqlite_not_missing() {
        // review-10: a NON-NotFound open/stat fault (here ENOTDIR — a DB path that traverses a
        // regular file) must surface as the true `Sqlite` I/O fault, NEVER a false
        // `DatabaseMissing`. The missing-vs-fault classification uses `fs::metadata`, not
        // `exists()` (which collapses every metadata fault to "absent").
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let not_a_dir = temp_dir.path().join("regular-file");
        std::fs::write(&not_a_dir, b"x").expect("write file");
        // A path THROUGH a regular file: both the SQLite open and `fs::metadata` fail with ENOTDIR.
        let through_file = not_a_dir.join("inner.db");

        match StorageConnection::open_existing(&through_file).unwrap_err() {
            StorageError::Sqlite(_) => {} // expected: a real I/O fault, not absence
            other => panic!("expected Sqlite for a non-NotFound fault, got {other:?}"),
        }
    }

    #[test]
    fn open_existing_opens_a_present_migrated_db() {
        // A DB that exists (created by the index/create path) opens normally and is
        // fully migrated — parity with `open` for the healthy case.
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("present.db");
        drop(StorageConnection::open(&db_path).expect("create via the index/create path"));

        let storage =
            StorageConnection::open_existing(&db_path).expect("open_existing on present db");
        let count: i64 = storage
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 34);
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
        assert_eq!(count, 34);
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
        let _storage_2 =
            StorageConnection::open(&db_path).expect("re-open after drop must succeed");
    }

    // ── DAEMON-CANCEL-2: interrupt_handle aborts an in-flight statement ─────────
    //
    // The honest, DETERMINISTIC proof of the slice's core claim: firing the
    // `StorageInterruptHandle` from ANOTHER thread aborts a REAL, in-flight
    // `compute_module_stats` `SELECT` mid-execution — it returns SQLITE_INTERRUPT
    // rather than completing the aggregation. No wall-clock: a `progress_handler`
    // (the test-only `hooks` rusqlite feature) makes the statement provably
    // in-flight and parks it until the cross-thread interrupt has been fired, so
    // the abort is causally pinned, not raced. This is the storage-layer mechanism
    // proof; the dispatcher/transport wiring (peer-disconnect → interrupt) is proven
    // in `daemon-runtime`'s `concurrency_dispatch` suite.

    /// Build a real graph fixture so `compute_module_stats` does genuine work:
    /// `n` directory MODULEs, each OWNing one FILE node and a handful of exported
    /// SYMBOLs, wired into an IMPORTS ring (for fan-in/out). Returns the snapshot.
    fn build_stats_fixture(
        storage: &mut StorageConnection,
        n: usize,
        syms_per_file: usize,
    ) -> String {
        use crate::types::{CreateSnapshotInput, GraphEdge, GraphNode, Repo, TrackedFile};

        storage
            .add_repo(&Repo {
                repo_uid: "r1".into(),
                name: "fixture".into(),
                root_path: "/tmp/fixture".into(),
                default_branch: Some("main".into()),
                created_at: "2025-01-01T00:00:00.000Z".into(),
                metadata_json: None,
            })
            .unwrap();
        let snap = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".into(),
                kind: "full".into(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
            .snapshot_uid;

        // files (FK target for node.file_uid).
        let files: Vec<TrackedFile> = (0..n)
            .map(|i| TrackedFile {
                file_uid: format!("fu{i}"),
                repo_uid: "r1".into(),
                path: format!("mod{i}/index.ts"),
                language: Some("typescript".into()),
                is_test: false,
                is_generated: false,
                is_excluded: false,
            })
            .collect();
        storage.upsert_files(&files).unwrap();

        let mut nodes: Vec<GraphNode> = Vec::new();
        for i in 0..n {
            // MODULE node (qualified_name drives the `stats` `path`/ORDER BY).
            nodes.push(GraphNode {
                node_uid: format!("m{i}"),
                snapshot_uid: snap.clone(),
                repo_uid: "r1".into(),
                stable_key: format!("r1:mod{i}:MODULE"),
                kind: "MODULE".into(),
                subtype: None,
                name: format!("mod{i}"),
                qualified_name: Some(format!("mod{i}")),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            });
            // The OWNS target: a FILE-bearing node (file_uid IS NOT NULL).
            nodes.push(GraphNode {
                node_uid: format!("fn{i}"),
                snapshot_uid: snap.clone(),
                repo_uid: "r1".into(),
                stable_key: format!("r1:mod{i}/index.ts:FILE"),
                kind: "FILE".into(),
                subtype: None,
                name: format!("mod{i}/index.ts"),
                qualified_name: None,
                file_uid: Some(format!("fu{i}")),
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            });
            // Exported SYMBOLs in that file (drive symbol_count via the file_stats CTE).
            for k in 0..syms_per_file {
                nodes.push(GraphNode {
                    node_uid: format!("s{i}_{k}"),
                    snapshot_uid: snap.clone(),
                    repo_uid: "r1".into(),
                    stable_key: format!("r1:mod{i}/index.ts:sym{k}:SYMBOL"),
                    kind: "SYMBOL".into(),
                    subtype: Some("FUNCTION".into()),
                    name: format!("sym{k}"),
                    qualified_name: None,
                    file_uid: Some(format!("fu{i}")),
                    parent_node_uid: None,
                    location: None,
                    signature: None,
                    visibility: Some("export".into()),
                    doc_comment: None,
                    metadata_json: None,
                });
            }
        }
        storage.insert_nodes(&nodes).unwrap();

        let mut edges: Vec<GraphEdge> = Vec::new();
        for i in 0..n {
            edges.push(GraphEdge {
                edge_uid: format!("owns{i}"),
                snapshot_uid: snap.clone(),
                repo_uid: "r1".into(),
                source_node_uid: format!("m{i}"),
                target_node_uid: format!("fn{i}"),
                edge_type: "OWNS".into(),
                resolution: "static".into(),
                extractor: "test".into(),
                location: None,
                metadata_json: None,
            });
            edges.push(GraphEdge {
                edge_uid: format!("imp{i}"),
                snapshot_uid: snap.clone(),
                repo_uid: "r1".into(),
                source_node_uid: format!("m{i}"),
                target_node_uid: format!("m{}", (i + 1) % n),
                edge_type: "IMPORTS".into(),
                resolution: "static".into(),
                extractor: "test".into(),
                location: None,
                metadata_json: None,
            });
        }
        storage.insert_edges(&edges).unwrap();
        snap
    }

    #[test]
    fn interrupt_handle_aborts_in_flight_compute_module_stats() {
        use std::sync::{Arc, Condvar, Mutex};

        let mut storage = StorageConnection::open_in_memory().unwrap();
        let snap = build_stats_fixture(&mut storage, 24, 8);

        // Sanity: uninterrupted, the fixture's query produces a full, non-empty
        // result — so a later SQLITE_INTERRUPT is a real abort of real work, not an
        // empty/no-op query.
        let full = storage
            .compute_module_stats(&snap)
            .expect("uninterrupted run completes");
        assert_eq!(
            full.len(),
            24,
            "the fixture must yield all 24 modules when not interrupted"
        );

        let handle = storage.interrupt_handle();

        // Two-phase rendezvous: (announced, released). The progress handler announces
        // the statement is in-flight, then parks the worker until the test releases it
        // — AFTER firing the interrupt — so the abort is causally guaranteed.
        let barrier = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let b = Arc::clone(&barrier);
        let mut announced_once = false;
        storage.connection().progress_handler(
            64,
            Some(move || {
                if !announced_once {
                    announced_once = true;
                    let (lock, cv) = &*b;
                    let mut g = lock.lock().unwrap();
                    g.0 = true; // in-flight
                    cv.notify_all();
                    while !g.1 {
                        g = cv.wait(g).unwrap();
                    }
                }
                false // do NOT abort via the handler; OUR sqlite3_interrupt is the cause
            }),
        );

        let snap2 = snap.clone();
        let worker = std::thread::spawn(move || storage.compute_module_stats(&snap2));

        // Wait until the statement is provably executing.
        {
            let (lock, cv) = &*barrier;
            let mut g = lock.lock().unwrap();
            while !g.0 {
                g = cv.wait(g).unwrap();
            }
        }
        // Fire the interrupt from THIS thread (the worker holds the connection).
        handle.interrupt();
        // Release the parked statement; its next VM op observes the interrupt flag.
        {
            let (lock, cv) = &*barrier;
            let mut g = lock.lock().unwrap();
            g.1 = true;
            cv.notify_all();
        }

        let result = worker.join().expect("worker thread joins");
        match result {
            Err(StorageError::Sqlite(e)) => assert_eq!(
                e.sqlite_error_code(),
                Some(rusqlite::ErrorCode::OperationInterrupted),
                "the in-flight compute_module_stats SELECT must abort with SQLITE_INTERRUPT, got: {e}"
            ),
            other => panic!(
                "interrupt must abort the in-flight statement with SQLITE_INTERRUPT, got: {other:?}"
            ),
        }
    }

    // ── EMBED-SEED-IMPL-1 (operator ruling 2): WAL busy-handler contention ──────
    //
    // These prove the `busy_timeout` fix for the "database is locked" defect the
    // operator's real daemon hit (reclaim vs. `rmap check`), which the default-on
    // seed pass would make routine. They model that contention with a competing
    // writer holding the file lock while a `StorageConnection` (busy_timeout set at
    // `open`) issues its own write.

    /// A competing writer that holds the DB lock briefly must NOT make a second
    /// connection's write fail instantly — `busy_timeout` lets it WAIT then succeed.
    #[test]
    fn open_busy_timeout_lets_a_contending_writer_wait_then_succeed() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("contend.db");

        // Materialize + migrate the file (also sets WAL + busy_timeout on `victim`).
        let victim = StorageConnection::open(&db_path).expect("open new file");

        // A separate connection grabs the write lock and holds it ~300 ms.
        let holder_path = db_path.clone();
        let holder = std::thread::spawn(move || {
            let c = Connection::open(&holder_path).expect("holder open");
            c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
            c.execute_batch("BEGIN IMMEDIATE;")
                .expect("holder takes write lock");
            std::thread::sleep(std::time::Duration::from_millis(300));
            c.execute_batch("COMMIT;").expect("holder releases");
        });

        // Let the holder acquire the lock first.
        std::thread::sleep(std::time::Duration::from_millis(50));

        // The victim's write must WAIT for the lock (busy_timeout = 5 s) and then
        // succeed — never fail instantly with "database is locked".
        let res = victim
            .connection()
            .execute_batch("BEGIN IMMEDIATE; COMMIT;");
        assert!(
            res.is_ok(),
            "busy_timeout must let a contending writer wait then succeed, got: {res:?}"
        );
        holder.join().expect("holder joins");
    }

    /// The operator-named regression (review-4 #2 / operator ruling 2): a
    /// DETERMINISTIC READ must SUCCEED while a concurrent maintenance WRITER (the
    /// `enrich → seed → retention` chain, or a reclaim) holds the DB. This is the
    /// exact shape of the operator's 2026-08-24 incident (reclaim's long write made
    /// `rmap check` return "database is locked") that default-on seeding would make
    /// routine.
    ///
    /// The reader models the daemon's CACHED connection — already open (as it is in
    /// production, established at repo-load), so it never contends on open-time WAL/
    /// migration setup. Under WAL a reader proceeds against the last committed
    /// snapshot even while a writer holds the write lock; the installed `busy_timeout`
    /// absorbs any transient page contention rather than surfacing a spurious lock
    /// error. Both ends are real `StorageConnection`s (the shipped `open` path that
    /// sets WAL + `busy_timeout`) — this proves the shipped configuration, not a
    /// hand-tuned one.
    #[test]
    fn read_succeeds_while_a_maintenance_writer_holds_the_db() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("seed_read.db");

        // Both connections open FIRST (WAL + busy_timeout set on each) — the reader is
        // the daemon's already-established cached connection, the writer is the
        // background maintenance chain. Opening before the lock is taken mirrors
        // production: the daemon does not re-open per request.
        let reader = StorageConnection::open(&db_path).expect("reader (cached) opens storage");
        let writer = StorageConnection::open(&db_path).expect("writer (maintenance) opens storage");

        // The maintenance writer takes and holds the write lock (models reclaim / the
        // seed-then-retention chain actively writing).
        writer
            .connection()
            .execute_batch("BEGIN IMMEDIATE;")
            .expect("maintenance writer takes the write lock");

        // The deterministic read must complete WHILE the writer holds its lock.
        let start = std::time::Instant::now();
        let count: Result<i64, _> =
            reader
                .connection()
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                    row.get(0)
                });
        let waited = start.elapsed();
        assert!(
            count.is_ok(),
            "a deterministic read must SUCCEED while a maintenance writer holds the DB, got: {count:?}"
        );
        assert!(
            waited < std::time::Duration::from_secs(1),
            "the read must not stall (WAL reader is non-blocking): waited {waited:?}"
        );

        writer
            .connection()
            .execute_batch("COMMIT;")
            .expect("maintenance writer releases");
    }

    /// A writer that holds the lock LONGER than the busy bound must yield an honest
    /// `SQLITE_BUSY` error — a finite wait then a truthful failure, never an infinite
    /// hang. (The test re-sets this one connection's bound to a short value to prove
    /// the same mechanism the 5 s production value uses, without a 5 s wall-clock.)
    #[test]
    fn writer_past_busy_timeout_yields_honest_error_not_a_hang() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("timeout.db");
        let victim = StorageConnection::open(&db_path).expect("open new file");
        victim
            .connection()
            .busy_timeout(std::time::Duration::from_millis(150))
            .unwrap();

        let holder_path = db_path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let c = Connection::open(&holder_path).expect("holder open");
            c.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
            c.execute_batch("BEGIN IMMEDIATE;")
                .expect("holder takes write lock");
            tx.send(()).expect("signal lock held");
            std::thread::sleep(std::time::Duration::from_millis(800)); // > victim's 150 ms
            c.execute_batch("COMMIT;").expect("holder releases");
        });
        rx.recv().expect("wait until the lock is actually held");

        let start = std::time::Instant::now();
        let res = victim
            .connection()
            .execute_batch("BEGIN IMMEDIATE; COMMIT;");
        let waited = start.elapsed();

        match res {
            Err(rusqlite::Error::SqliteFailure(e, _)) => assert_eq!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy,
                "a lock held past busy_timeout must surface SQLITE_BUSY, got: {e:?}"
            ),
            other => panic!(
                "a lock held past busy_timeout must ERROR honestly (not succeed / not hang), got: {other:?}"
            ),
        }
        assert!(
            waited < std::time::Duration::from_millis(600),
            "the wait must be bounded by busy_timeout (no hang): waited {waited:?}"
        );
        holder.join().expect("holder joins");
    }

    /// review-5 #3 / operator ruling 2 — the ACTUAL production bound (not a shortened
    /// analogue). A writer that holds the lock LONGER than the shipped 5 s
    /// `busy_timeout` must make a contending writer that uses the SHIPPED configuration
    /// (no per-connection override) wait out the ~5 s bound and then yield an honest
    /// `SQLITE_BUSY` — never hang until release. This deliberately costs ~5 s of
    /// wall-clock: it proves the configured `BUSY_TIMEOUT_MS = 5000` value the operator
    /// ratified, which a re-set-to-150 ms victim cannot. The victim here is a real
    /// shipped `StorageConnection::open` (WAL + 5 s busy_timeout), the exact production
    /// path a deterministic writer takes while the maintenance chain holds the DB.
    #[test]
    fn writer_held_past_production_5s_bound_yields_honest_busy() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("prod_bound.db");
        // The victim uses ONLY the shipped configuration (open installs busy_timeout =
        // BUSY_TIMEOUT_MS = 5000 ms). No override — this is the production bound.
        let victim = StorageConnection::open(&db_path).expect("open (production 5 s busy_timeout)");

        let holder_path = db_path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let c = Connection::open(&holder_path).expect("holder open");
            c.busy_timeout(std::time::Duration::from_secs(30)).unwrap();
            c.execute_batch("BEGIN IMMEDIATE;")
                .expect("holder takes write lock");
            tx.send(()).expect("signal lock held");
            // Hold PAST the victim's 5 s production bound so the victim gives up on the
            // timeout, not on our release.
            std::thread::sleep(std::time::Duration::from_millis(6500));
            c.execute_batch("COMMIT;").expect("holder releases");
        });
        rx.recv().expect("wait until the lock is actually held");

        let start = std::time::Instant::now();
        let res = victim
            .connection()
            .execute_batch("BEGIN IMMEDIATE; COMMIT;");
        let waited = start.elapsed();

        match res {
            Err(rusqlite::Error::SqliteFailure(e, _)) => assert_eq!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy,
                "a lock held past the 5 s production busy_timeout must surface SQLITE_BUSY, got: {e:?}"
            ),
            other => panic!(
                "a lock held past the production bound must ERROR honestly (not succeed / not hang), got: {other:?}"
            ),
        }
        // The victim actually waited out the ~5 s production bound (it did not fail
        // instantly) ...
        assert!(
            waited >= std::time::Duration::from_millis(4500),
            "the victim must wait out the ~5 s production bound before erroring: waited {waited:?}"
        );
        // ... and gave up ON THE TIMEOUT, not by blocking until the 6.5 s holder release.
        assert!(
            waited < std::time::Duration::from_millis(6400),
            "the wait must be bounded by the 5 s busy_timeout, not block until release: waited {waited:?}"
        );
        holder.join().expect("holder joins");
    }
}
