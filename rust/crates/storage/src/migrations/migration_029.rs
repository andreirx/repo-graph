//! Migration 029 — repair orphan FK references (RETENTION-POLICY-1).
//!
//! Cleans up orphan rows in tables that lack `ON DELETE CASCADE` on their
//! `snapshot_uid` foreign key. These orphan rows can cause
//! "FOREIGN KEY constraint failed" errors when pruning snapshots.
//!
//! # Background
//!
//! Some tables reference `snapshots.snapshot_uid` without `ON DELETE CASCADE`:
//! - `unresolved_edges`
//! - `boundary_provider_facts`
//! - `boundary_consumer_facts`
//! - `boundary_links`
//! - `boundary_interaction_surfaces`
//! - `boundary_interaction_links`
//!
//! The `snapshots` table also has a self-referencing FK via `parent_snapshot_uid`.
//!
//! Before the retention pruning fix (RETENTION-POLICY-1), snapshots could be
//! deleted without cleaning up these dependent rows, leaving orphan references.
//! This migration repairs those orphan references so pruning can proceed.
//!
//! # Migration Strategy
//!
//! 1. Delete rows from orphan tables where `snapshot_uid` does not exist in `snapshots`
//! 2. Clear `parent_snapshot_uid` where the referenced snapshot doesn't exist
//!
//! This is safe because:
//! - If the snapshot doesn't exist, the dependent data is meaningless
//! - Clearing parent_snapshot_uid to NULL is semantically valid (orphan snapshot)
//!
//! # Idempotence
//!
//! Safe to re-run; deletes only orphan rows, which are already absent after first run.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

/// Run migration 029 against the given connection.
///
/// Idempotent: re-running on a database with no orphan rows is a no-op.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    // Tables without ON DELETE CASCADE on snapshot_uid FK.
    // Delete rows where the referenced snapshot doesn't exist.
    let orphan_tables = [
        "unresolved_edges",
        "boundary_provider_facts",
        "boundary_consumer_facts",
        "boundary_links",
        "boundary_interaction_surfaces",
        "boundary_interaction_links",
    ];

    for table in &orphan_tables {
        let deleted = conn.execute(
            &format!(
                "DELETE FROM {} WHERE snapshot_uid NOT IN (SELECT snapshot_uid FROM snapshots)",
                table
            ),
            [],
        )?;
        if deleted > 0 {
            eprintln!(
                "migration 029: cleaned {} orphan row(s) from {}",
                deleted, table
            );
        }
    }

    // Clear parent_snapshot_uid where the referenced snapshot doesn't exist
    let cleared = conn.execute(
        "UPDATE snapshots SET parent_snapshot_uid = NULL \
         WHERE parent_snapshot_uid IS NOT NULL \
         AND parent_snapshot_uid NOT IN (SELECT snapshot_uid FROM snapshots)",
        [],
    )?;
    if cleared > 0 {
        eprintln!(
            "migration 029: cleared {} orphan parent_snapshot_uid reference(s)",
            cleared
        );
    }

    record_migration(conn, 29, "029-repair-orphan-fks")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;

    fn setup_db_through_028() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        // Run all migrations through 028 (excluding 029 which we're testing)
        run_migrations(&mut conn).unwrap();
        conn
    }

    #[test]
    fn clears_orphan_parent_snapshot_uid() {
        let mut conn = setup_db_through_028();

        // Insert repo
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
             VALUES ('r1', 'test', '/test', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Insert snapshot with orphan parent_snapshot_uid
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at, parent_snapshot_uid) \
             VALUES ('s1', 'r1', 'full', 'ready', '2025-01-01T00:00:00Z', 'nonexistent')",
            [],
        )
        .unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        // Verify orphan parent reference exists
        let parent_before: Option<String> = conn
            .query_row(
                "SELECT parent_snapshot_uid FROM snapshots WHERE snapshot_uid = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(parent_before, Some("nonexistent".to_string()));

        // Run migration (which is included in run_migrations, but the orphan was
        // inserted after, so we run it explicitly to test the cleanup)
        run(&mut conn).unwrap();

        // Orphan parent reference should be cleared
        let parent_after: Option<String> = conn
            .query_row(
                "SELECT parent_snapshot_uid FROM snapshots WHERE snapshot_uid = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(parent_after.is_none());
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db_through_028();

        // Run again (already ran during setup)
        run(&mut conn).unwrap();

        // Should not error
    }

    #[test]
    fn preserves_valid_references() {
        let mut conn = setup_db_through_028();

        // Insert repo
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
             VALUES ('r1', 'test', '/test', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Insert two snapshots with valid parent relationship
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) \
             VALUES ('s1', 'r1', 'full', 'ready', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at, parent_snapshot_uid) \
             VALUES ('s2', 'r1', 'full', 'ready', '2025-01-02T00:00:00Z', 's1')",
            [],
        )
        .unwrap();

        // Run migration
        run(&mut conn).unwrap();

        // Valid parent reference should be preserved
        let parent: Option<String> = conn
            .query_row(
                "SELECT parent_snapshot_uid FROM snapshots WHERE snapshot_uid = 's2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(parent, Some("s1".to_string()));
    }

    #[test]
    fn cleans_orphan_unresolved_edges() {
        let mut conn = setup_db_through_028();

        // Insert a repo and snapshot
        conn.execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
             VALUES ('r1', 'test', '/test', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) \
             VALUES ('s1', 'r1', 'full', 'ready', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Insert orphan unresolved_edge (references non-existent snapshot)
        // Disable FK enforcement to insert the orphan row
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn.execute(
            "INSERT INTO unresolved_edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
             target_key, type, resolution, extractor, category, classification, \
             classifier_version, basis_code, observed_at) \
             VALUES ('e1', 'nonexistent', 'r1', 'n1', 'target', 'import', 'unresolved', \
             'ts', 'external', 'unresolved', 1, 'basis', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        // Verify orphan exists
        let count_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM unresolved_edges", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count_before, 1);

        // Run migration
        run(&mut conn).unwrap();

        // Orphan should be cleaned
        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM unresolved_edges", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count_after, 0);
    }
}
