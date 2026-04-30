//! Migration 023 — add return_fates table.
//!
//! Supports PF-3: RETURN_FATE policy-fact extraction.
//! See docs/slices/pf-3-return-fate.md for full specification.
//!
//! RETURN_FATE facts are snapshot-scoped: they capture what happens
//! to function return values at each call site in a specific snapshot.
//! Multiple calls on the same line are distinguished by column.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS return_fates (
            uid             TEXT PRIMARY KEY,
            snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            callee_key      TEXT,
            callee_name     TEXT NOT NULL,
            caller_key      TEXT NOT NULL,
            caller_name     TEXT NOT NULL,
            file_path       TEXT NOT NULL,
            line            INTEGER NOT NULL,
            col             INTEGER NOT NULL,
            fate            TEXT NOT NULL,
            evidence_json   TEXT NOT NULL,

            UNIQUE(snapshot_uid, caller_key, line, col)
        );

        CREATE INDEX IF NOT EXISTS idx_return_fates_snapshot
            ON return_fates(snapshot_uid);

        CREATE INDEX IF NOT EXISTS idx_return_fates_file
            ON return_fates(file_path);

        CREATE INDEX IF NOT EXISTS idx_return_fates_callee
            ON return_fates(callee_name);

        CREATE INDEX IF NOT EXISTS idx_return_fates_fate
            ON return_fates(fate);
        "#,
    )?;

    record_migration(conn, 23, "023-return-fates")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_023_creates_return_fates_table() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Bootstrap with migration 001 (creates schema_migrations table)
        crate::migrations::migration_001::run(&mut conn).unwrap();

        // Run migration 023
        run(&mut conn).unwrap();

        // Verify table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='return_fates'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify indexes exist
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_return_fates%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 4);

        // Verify migration recorded
        let version: i64 = conn
            .query_row(
                "SELECT version FROM schema_migrations WHERE name = '023-return-fates'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 23);
    }

    #[test]
    fn migration_023_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::migrations::migration_001::run(&mut conn).unwrap();

        // Run twice
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();

        // Should still have exactly one table
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='return_fates'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_023_unique_constraint_distinguishes_by_column() {
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

        // Insert two fates on the same line but different columns
        conn.execute(
            "INSERT INTO return_fates (uid, snapshot_uid, callee_key, callee_name, caller_key, caller_name, file_path, line, col, fate, evidence_json)
             VALUES ('f1', 's1', NULL, 'first', 'test:file.c#func:SYMBOL:FUNCTION', 'func', 'file.c', 10, 5, 'STORED', '{}')",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO return_fates (uid, snapshot_uid, callee_key, callee_name, caller_key, caller_name, file_path, line, col, fate, evidence_json)
             VALUES ('f2', 's1', NULL, 'second', 'test:file.c#func:SYMBOL:FUNCTION', 'func', 'file.c', 10, 20, 'STORED', '{}')",
            [],
        ).unwrap();

        // Both should exist
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM return_fates", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn migration_023_allows_null_callee_key() {
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

        // Insert a fate with NULL callee_key (unresolved callee)
        conn.execute(
            "INSERT INTO return_fates (uid, snapshot_uid, callee_key, callee_name, caller_key, caller_name, file_path, line, col, fate, evidence_json)
             VALUES ('f1', 's1', NULL, 'install_update', 'test:file.c#func:SYMBOL:FUNCTION', 'func', 'file.c', 10, 5, 'IGNORED', '{}')",
            [],
        ).unwrap();

        // Should exist with NULL key
        let callee_key: Option<String> = conn
            .query_row(
                "SELECT callee_key FROM return_fates WHERE uid = 'f1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(callee_key.is_none());
    }
}
