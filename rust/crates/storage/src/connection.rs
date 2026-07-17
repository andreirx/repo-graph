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
//!    migration runner, applies all 30 migrations. Returns
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
/// underlying SQLite connection AND runs all 30 migrations
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

        // Verify all 30 migrations have been applied by checking
        // the schema_migrations table count.
        let count: i64 = storage
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("query schema_migrations");
        assert_eq!(
            count, 30,
            "open_in_memory must run all 30 migrations before returning"
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
        assert_eq!(count, 30);
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

        // Verify schema_migrations still has exactly 30 rows
        // (re-open did not duplicate any).
        let migration_count: i64 = storage_again
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            migration_count, 30,
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
        assert_eq!(count, 30);
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
}
