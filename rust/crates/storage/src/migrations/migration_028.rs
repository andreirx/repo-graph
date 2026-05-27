//! Migration 028 — cache semantics columns (CACHE-SEMANTICS-1).
//!
//! Adds `derived_cache_epoch` and `retention_class` columns to the `snapshots`
//! table to support explicit cache/authority semantic separation.
//!
//! # Derived Cache Epoch
//!
//! The `derived_cache_epoch` column stores a version identifier that represents
//! the validity epoch of the snapshot's derived cache (Tier B data). When the
//! extractor or inference logic changes, the epoch changes, and snapshots with
//! mismatched epochs are considered stale.
//!
//! Format: `"<major>.<minor>"` (e.g., `"1.0"`)
//!
//! - Major bump: breaking change requiring full re-extraction
//! - Minor bump: compatible change (new optional data)
//!
//! # Retention Class
//!
//! The `retention_class` column controls snapshot retention behavior:
//!
//! - `current`: Active snapshot for this repo (always retained)
//! - `parent`: Parent of current snapshot (retained for incremental refresh)
//! - `baseline_auto`: Automatically selected comparison baseline
//! - `baseline_user`: Explicitly marked by user as baseline
//! - `prunable`: Eligible for pruning (default for old snapshots)
//!
//! # Migration Strategy
//!
//! Existing snapshots are migrated with:
//! - `derived_cache_epoch = NULL` (unknown, treated as potentially stale)
//! - `retention_class = NULL` (requires classification pass to assign)
//!
//! After migration, a classification pass should assign retention classes
//! based on snapshot relationships (current, parent, etc.).
//!
//! # Semantic Contract
//!
//! This migration establishes the semantic boundary:
//! - Tier B (extracted/derived cache) is explicitly rebuildable
//! - Retention is explicit and policy-driven
//! - Whole-snapshot invalidation on epoch mismatch
//!
//! # References
//!
//! - `docs/slices/cache-semantics-1.md`
//! - `agent_docs/storage-architecture-v2.md`

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

/// Run migration 028 against the given connection.
///
/// Idempotent: re-running on a database that already has both columns is a no-op.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    let cols = pragma_table_columns(conn, "snapshots")?;

    // Add derived_cache_epoch if not present
    if !cols.iter().any(|c| c == "derived_cache_epoch") {
        conn.execute_batch("ALTER TABLE snapshots ADD COLUMN derived_cache_epoch TEXT")?;
    }

    // Add retention_class if not present
    if !cols.iter().any(|c| c == "retention_class") {
        conn.execute_batch("ALTER TABLE snapshots ADD COLUMN retention_class TEXT")?;
    }

    record_migration(conn, 28, "028-cache-semantics")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::{migration_001, pragma_table_columns};

    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration_001::run(&mut conn).unwrap();
        // Run migrations 002-027 would be needed for a real test
        // For this test, we just need the snapshots table from 001
        conn
    }

    #[test]
    fn adds_cache_semantics_columns() {
        let mut conn = setup_db();

        // Before migration
        let cols_before = pragma_table_columns(&conn, "snapshots").unwrap();
        assert!(!cols_before.contains(&"derived_cache_epoch".to_string()));
        assert!(!cols_before.contains(&"retention_class".to_string()));

        // Run migration
        run(&mut conn).unwrap();

        // After migration
        let cols_after = pragma_table_columns(&conn, "snapshots").unwrap();
        assert!(cols_after.contains(&"derived_cache_epoch".to_string()));
        assert!(cols_after.contains(&"retention_class".to_string()));
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();

        run(&mut conn).unwrap();
        // Second run should not error
        run(&mut conn).unwrap();

        let cols = pragma_table_columns(&conn, "snapshots").unwrap();
        assert!(cols.contains(&"derived_cache_epoch".to_string()));
        assert!(cols.contains(&"retention_class".to_string()));
    }

    #[test]
    fn existing_snapshots_get_null_values() {
        let mut conn = setup_db();

        // Insert a snapshot before migration
        // Use root_path (not canonical_path) per 001-initial.sql schema
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

        // Run migration
        run(&mut conn).unwrap();

        // Check that existing snapshot has NULL values
        let (epoch, retention): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT derived_cache_epoch, retention_class FROM snapshots WHERE snapshot_uid = 's1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert!(epoch.is_none());
        assert!(retention.is_none());
    }
}
