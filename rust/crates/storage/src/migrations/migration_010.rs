//! Migration 010 — file_signals expansion (package deps + tsconfig aliases).
//!
//! Adds `file_signals.package_dependencies_json` and
//! `file_signals.tsconfig_aliases_json` if not present. Same
//! column-existence-check pattern as migrations 002 and 005.
//! Mirrors `010-file-signals-expansion.ts`.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
	let cols = pragma_table_columns(conn, "file_signals")?;

	if !cols.iter().any(|c| c == "package_dependencies_json") {
		conn.execute_batch(
			"ALTER TABLE file_signals ADD COLUMN package_dependencies_json TEXT",
		)?;
	}

	if !cols.iter().any(|c| c == "tsconfig_aliases_json") {
		conn.execute_batch(
			"ALTER TABLE file_signals ADD COLUMN tsconfig_aliases_json TEXT",
		)?;
	}

	record_migration(conn, 10, "010-file-signals-expansion")?;
	Ok(())
}
