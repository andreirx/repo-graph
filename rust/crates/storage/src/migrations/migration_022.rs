//! Migration 022 — add behavioral_markers table.
//!
//! Supports PF-2: BEHAVIORAL_MARKER policy-fact extraction.
//! See docs/slices/pf-2-behavioral-marker.md for full specification.
//!
//! BEHAVIORAL_MARKER facts are snapshot-scoped: they capture behavioral
//! patterns (retry loops, resume offsets) present in a specific snapshot.
//! A function can have multiple markers (e.g., both RETRY_LOOP and
//! RESUME_OFFSET in the same function).

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS behavioral_markers (
            uid             TEXT PRIMARY KEY,
            snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            symbol_key      TEXT NOT NULL,
            function_name   TEXT NOT NULL,
            file_path       TEXT NOT NULL,
            line_start      INTEGER NOT NULL,
            line_end        INTEGER NOT NULL,
            kind            TEXT NOT NULL,
            evidence_json   TEXT NOT NULL,

            UNIQUE(snapshot_uid, symbol_key, kind, line_start)
        );

        CREATE INDEX IF NOT EXISTS idx_behavioral_markers_snapshot
            ON behavioral_markers(snapshot_uid);

        CREATE INDEX IF NOT EXISTS idx_behavioral_markers_file
            ON behavioral_markers(file_path);

        CREATE INDEX IF NOT EXISTS idx_behavioral_markers_kind
            ON behavioral_markers(kind);
        "#,
    )?;

    record_migration(conn, 22, "022-behavioral-markers")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_022_creates_behavioral_markers_table() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Bootstrap with migration 001 (creates schema_migrations table)
        crate::migrations::migration_001::run(&mut conn).unwrap();

        // Run migration 022
        run(&mut conn).unwrap();

        // Verify table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='behavioral_markers'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify indexes exist
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_behavioral_markers%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 3);

        // Verify migration recorded
        let version: i64 = conn
            .query_row(
                "SELECT version FROM schema_migrations WHERE name = '022-behavioral-markers'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 22);
    }

    #[test]
    fn migration_022_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();

        // Run twice
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Should still have exactly one table
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='behavioral_markers'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_022_unique_constraint_allows_multiple_markers_per_function() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Create prerequisite rows
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/abs', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'building', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        // Insert two markers for the same function at different lines
        conn.execute(
            "INSERT INTO behavioral_markers (uid, snapshot_uid, symbol_key, function_name, file_path, line_start, line_end, kind, evidence_json)
             VALUES ('m1', 's1', 'test:file.c#func:SYMBOL:FUNCTION', 'func', 'file.c', 10, 20, 'RETRY_LOOP', '{}')",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO behavioral_markers (uid, snapshot_uid, symbol_key, function_name, file_path, line_start, line_end, kind, evidence_json)
             VALUES ('m2', 's1', 'test:file.c#func:SYMBOL:FUNCTION', 'func', 'file.c', 15, 15, 'RESUME_OFFSET', '{}')",
            [],
        ).unwrap();

        // Both should exist
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM behavioral_markers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
