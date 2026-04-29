//! Migration 021 — add status_mappings table.
//!
//! Supports PF-1: STATUS_MAPPING policy-fact extraction.
//! See docs/slices/pf-1-status-mapping.md for full specification.
//!
//! STATUS_MAPPING facts are snapshot-scoped: they capture the status
//! translation functions present in a specific snapshot of the codebase.
//! Each extraction run for a snapshot replaces all STATUS_MAPPING facts
//! for that snapshot.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS status_mappings (
            uid             TEXT PRIMARY KEY,
            snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            symbol_key      TEXT NOT NULL,
            function_name   TEXT NOT NULL,
            file_path       TEXT NOT NULL,
            line_start      INTEGER NOT NULL,
            line_end        INTEGER NOT NULL,
            source_type     TEXT NOT NULL,
            target_type     TEXT NOT NULL,
            mappings_json   TEXT NOT NULL,
            default_output  TEXT,

            UNIQUE(snapshot_uid, symbol_key)
        );

        CREATE INDEX IF NOT EXISTS idx_status_mappings_snapshot
            ON status_mappings(snapshot_uid);

        CREATE INDEX IF NOT EXISTS idx_status_mappings_file
            ON status_mappings(file_path);

        CREATE INDEX IF NOT EXISTS idx_status_mappings_types
            ON status_mappings(source_type, target_type);
        "#,
    )?;

    record_migration(conn, 21, "021-status-mappings")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_021_creates_status_mappings_table() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Bootstrap with migration 001 (creates schema_migrations table)
        crate::migrations::migration_001::run(&mut conn).unwrap();

        // Run migration 021
        run(&mut conn).unwrap();

        // Verify table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='status_mappings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify indexes exist
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_status_mappings%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 3);

        // Verify migration recorded
        let version: i64 = conn
            .query_row(
                "SELECT version FROM schema_migrations WHERE name = '021-status-mappings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 21);
    }

    #[test]
    fn migration_021_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();

        // Run twice
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Should still have exactly one table
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='status_mappings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
