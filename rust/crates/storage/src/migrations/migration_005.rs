//! Migration 005 — extraction_diagnostics_json column.
//!
//! Adds `snapshots.extraction_diagnostics_json` if not present.
//! Same column-existence-check pattern as migration 002.
//! Mirrors `005-extraction-diagnostics.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	let snapshot_cols = pragma_table_columns(conn, "snapshots")?;

	if !snapshot_cols.iter().any(|c| c == "extraction_diagnostics_json") {
		conn.execute_batch(
			"ALTER TABLE snapshots ADD COLUMN extraction_diagnostics_json TEXT",
		)?;
	}

	record_migration(conn, 5, "005-extraction-diagnostics")?;
	Ok(())
}
