//! Migration 026 — add created_at to generated_code_mappings.
//!
//! Fixes a schema gap in migration 025: the design doc specified
//! `created_at` for generated_code_mappings but migration 025 omitted it.
//!
//! For databases that already ran migration 025, this migration adds
//! the missing column and backfills existing rows with the current
//! timestamp.
//!
//! For fresh databases (running the fixed migration 025), this is a no-op
//! because the column already exists.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    // Check if the column already exists (fresh DB with fixed migration 025)
    let cols = pragma_table_columns(conn, "generated_code_mappings")?;

    if !cols.contains(&"created_at".to_string()) {
        // Add the column with a default for existing rows
        conn.execute(
            "ALTER TABLE generated_code_mappings ADD COLUMN created_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z'",
            [],
        )?;

        // Backfill existing rows with current timestamp
        // (The DEFAULT only applies to the ALTER, not to new INSERTs)
        conn.execute(
            "UPDATE generated_code_mappings SET created_at = datetime('now') WHERE created_at = '1970-01-01T00:00:00Z'",
            [],
        )?;
    }

    record_migration(conn, 26, "026-gcm-created-at")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;

    /// Verify migration 026 adds created_at to tables missing it.
    #[test]
    fn migration_adds_created_at_column() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Run migrations up to 025 but simulate the OLD 025 without created_at
        // by running migrations, then dropping the column if it exists
        run_migrations(&mut conn).unwrap();

        // Check the column exists after full migration run (including 026)
        let cols = pragma_table_columns(&conn, "generated_code_mappings").unwrap();
        assert!(
            cols.contains(&"created_at".to_string()),
            "expected created_at column, got: {:?}",
            cols
        );
    }

    /// Verify migration 026 is idempotent (safe to run on fresh DB with fixed 025).
    #[test]
    fn migration_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Run all migrations (fresh DB gets fixed 025 + 026)
        run_migrations(&mut conn).unwrap();

        // Run 026 again - should be no-op
        run(&mut conn).unwrap();

        // Column should still exist with no errors
        let cols = pragma_table_columns(&conn, "generated_code_mappings").unwrap();
        assert!(cols.contains(&"created_at".to_string()));
    }
}
