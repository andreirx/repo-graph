//! Migration 002 — provenance columns.
//!
//! Adds `snapshots.toolchain_json` and
//! `declarations.authored_basis_json` if they do not already
//! exist. SQLite has no `ALTER TABLE ... ADD COLUMN IF NOT
//! EXISTS`, so the migration uses `PRAGMA table_info(...)` to
//! check column presence before issuing the ALTER.
//!
//! Mirrors the TS migration at
//! `src/adapters/storage/sqlite/migrations/002-provenance-columns.ts`.
//!
//! ── Why the column-existence check matters ────────────────────
//!
//! On the TypeScript side, `001-initial.ts` already includes both
//! columns. Fresh TS installs see the columns after migration 001
//! and migration 002 becomes a no-op (the existence check returns
//! true).
//!
//! On the Rust side, the embedded `001-initial.sql` is OUTDATED
//! and does NOT include either column. Fresh Rust installs see
//! the columns missing after migration 001, and migration 002
//! actually applies the ALTER TABLE statements.
//!
//! Both paths converge to the same final schema. The intermediate
//! post-001 state differs by exactly two columns. See the
//! `migrations/mod.rs` module docs for the full explanation.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

/// Run migration 002 against the given connection.
///
/// Idempotent: re-running on a database that already has both
/// columns is a no-op (the existence checks return true and no
/// ALTER TABLE statements run).
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	let snapshot_cols = pragma_table_columns(conn, "snapshots")?;
	let decl_cols = pragma_table_columns(conn, "declarations")?;

	if !snapshot_cols.iter().any(|c| c == "toolchain_json") {
		conn.execute_batch("ALTER TABLE snapshots ADD COLUMN toolchain_json TEXT")?;
	}
	if !decl_cols.iter().any(|c| c == "authored_basis_json") {
		conn.execute_batch(
			"ALTER TABLE declarations ADD COLUMN authored_basis_json TEXT",
		)?;
	}

	record_migration(conn, 2, "002-provenance-columns")?;
	Ok(())
}
