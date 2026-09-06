//! Migration 035 — single-column indexes on the FK child columns that reference
//! `nodes(node_uid)`, so the retention prune's FK cascade seeks instead of scans.
//!
//! # Why (DAEMON-RESIDUALS-2, mechanism measured cycle 1; DR-1 = B, HUMAN ruling)
//!
//! Pruning a snapshot is `DELETE FROM snapshots WHERE snapshot_uid=?`. FK
//! `ON DELETE CASCADE` on `nodes.snapshot_uid` deletes that snapshot's `nodes`
//! rows; for EACH deleted node SQLite must enforce every FK that references
//! `nodes(node_uid)`:
//!
//! - `edges.source_node_uid` / `edges.target_node_uid` (`ON DELETE CASCADE`) —
//!   cascade-delete the referencing edge rows;
//! - `unresolved_edges.source_node_uid` (NO ACTION / RESTRICT) — verify no row
//!   still references the node;
//! - `nodes.parent_node_uid` (self-ref, NO ACTION / RESTRICT) — same verification.
//!
//! Every index on those tables is COMPOSITE with `snapshot_uid` LEADING
//! (`idx_edges_snapshot_src = (snapshot_uid, source_node_uid)`, etc.), so a bare
//! single-column predicate on the child column cannot seek it — SQLite falls back
//! to a full table SCAN per deleted node. Cost is therefore O(nodes × child_rows)
//! per snapshot: on the operator's 4.8 GB / 29-snapshot store the prune ran 5 h+
//! with zero committed progress. Proven size-independently via `EXPLAIN QUERY PLAN`
//! (see `retention::tests::reproduce::explain_fk_child_delete_plan_is_a_full_scan`).
//!
//! This migration adds one bare single-column index per FK child column. With them,
//! each per-node FK enforcement is an index SEARCH, collapsing the cascade from
//! O(nodes × child_rows) to O(nodes × log child_rows). Measured on a 68 MB / 4-snapshot
//! synthetic store: 374.5 s → 0.84 s (prune of 2 snapshots), +0.29 s one-time index
//! build. The existing FK-ON cascade is UNCHANGED — SQLite still enforces referential
//! integrity; only its access path improves.
//!
//! # Insert-path cost (DR-1 measurement obligation)
//!
//! Four secondary indexes must be maintained on every `nodes`/`edges`/`unresolved_edges`
//! insert. The insert-path and store-size cost is measured and reported in the slice's
//! build report (harness before/after + one isolated repo-graph index). The human's
//! expectation is "a little"; the number is reported, not judged.
//!
//! # Completeness of the FK child-column set
//!
//! The four columns are the COMPLETE set of columns declaring a foreign key to
//! `nodes(node_uid)` across every migration (verified by grep over all `migration_*.rs`
//! and `001-initial.sql`, AND by `PRAGMA foreign_key_list` introspection in the harness —
//! the grep is a claim, the PRAGMA is the deterministic check). If a future migration
//! adds another `nodes`-referencing FK child column, it must add its own single-column
//! index (and the harness FK-child assertion will surface the gap).
//!
//! # Idempotence
//!
//! `CREATE INDEX IF NOT EXISTS` + `INSERT OR IGNORE` migration row: safe to re-run.

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::record_migration;

/// Run migration 035 against the given connection.
///
/// Idempotent: re-running on a database that already has the indexes is a no-op.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_edges_source_node_uid ON edges(source_node_uid);
         CREATE INDEX IF NOT EXISTS idx_edges_target_node_uid ON edges(target_node_uid);
         CREATE INDEX IF NOT EXISTS idx_unresolved_edges_source_node_uid \
           ON unresolved_edges(source_node_uid);
         CREATE INDEX IF NOT EXISTS idx_nodes_parent_node_uid ON nodes(parent_node_uid);",
    )?;

    record_migration(conn, 35, "035-fk-child-indexes")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::{migration_001, migration_007};

    /// A DB with the base schema (001) + `unresolved_edges` (007), the two tables
    /// carrying the FK child columns 035 indexes.
    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration_001::run(&mut conn).unwrap();
        migration_007::run(&mut conn).unwrap();
        conn
    }

    fn index_exists(conn: &Connection, index: &str) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='index' AND name=?",
            rusqlite::params![index],
            |_| Ok(()),
        )
        .is_ok()
    }

    #[test]
    fn creates_all_four_fk_child_indexes() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        for idx in [
            "idx_edges_source_node_uid",
            "idx_edges_target_node_uid",
            "idx_unresolved_edges_source_node_uid",
            "idx_nodes_parent_node_uid",
        ] {
            assert!(index_exists(&conn, idx), "missing index {idx}");
        }
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run must not error
    }

    #[test]
    fn records_version_35() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        let (version, name): (i64, String) = conn
            .query_row(
                "SELECT version, name FROM schema_migrations WHERE version = 35",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(version, 35);
        assert_eq!(name, "035-fk-child-indexes");
    }
}
