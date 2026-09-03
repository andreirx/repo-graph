//! Migration 032 — per-edge `is_type_only` on the resolved `edges` table
//! (TYPE-ONLY-IMPORTS-1).
//!
//! Adds ONE additive, nullable column to `edges`:
//!
//! - `is_type_only INTEGER` — the type-only disposition of an IMPORTS edge
//!   (value domain = [`TypeOnlyDisposition`] codes). `1` = type-only (a TS/JS
//!   `import type` / `export type … from` — an edge that VANISHES at runtime);
//!   `0` = a runtime import edge; `2` = the fact's carrier was CORRUPT/unreadable
//!   when computed (a distinct truth from an absent one — operator ruling
//!   2026-09-03 item 2a); `NULL` = the fact was not computed for this row.
//!
//! [`TypeOnlyDisposition`]: repo_graph_indexer::storage_port::TypeOnlyDisposition
//!
//! # Unknown is never zero (honesty rule)
//!
//! `NULL` is the ONLY representation of an ABSENT fact and is what a pre-migration
//! snapshot's edges carry (the column is added empty; old rows are never
//! back-filled). A reader that finds `NULL` on a MODULE→MODULE IMPORTS edge
//! MUST render it as Unknown-with-reason ("indexed before type-only tracking"),
//! never demote it to `0`/runtime. `0` is a MEASURED runtime edge; `2` is a
//! MEASURED-but-corrupt fact (its own Unknown reason, "type-only fact
//! unreadable"); `NULL` is the absence of the measurement. All three
//! non-runtime states stay distinct — none collapses into `0`.
//!
//! # Who writes it
//!
//! Only the indexer's module-edge derivation writes non-NULL values, and only on
//! MODULE→MODULE IMPORTS edges, via `EdgeStorePort::set_edge_type_only`. The value
//! is the CONJUNCTIVE aggregate over the contributing file-level import
//! observations: a module edge is type-only iff EVERY contributing file import is
//! type-only (any runtime contributor ⇒ `0`; any unknown-and-no-runtime
//! contributor ⇒ the edge is left `NULL`). File-level and non-IMPORTS edge rows
//! keep `NULL` — nothing reads their disposition today (the `cycles` serve reads
//! MODULE edges only), so leaving them unmeasured is honest, not a false zero.
//!
//! # Refresh behavior
//!
//! No copy-forward of this column is needed: resolved `edges` (including the
//! MODULE→MODULE IMPORTS rows) are REBUILT every snapshot from the full
//! extraction stream (fresh + copied-forward `extraction_edges`), and the
//! aggregate is re-derived and re-written on each run. The per-file
//! `import type` fact itself is carried forward inside `extraction_edges`'
//! `metadata_json` (an existing copied column), so a copied-forward file's
//! disposition survives without any schema change here.
//!
//! # Idempotence
//!
//! Safe to re-run; the column is added only when absent (mirrors the
//! `pragma_table_columns` guard used by migrations 030/031).

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

/// Run migration 032 against the given connection.
///
/// Idempotent: re-running on a database that already has the column is a no-op.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    let cols = pragma_table_columns(conn, "edges")?;
    if !cols.iter().any(|c| c == "is_type_only") {
        conn.execute_batch("ALTER TABLE edges ADD COLUMN is_type_only INTEGER")?;
    }

    record_migration(conn, 32, "032-edge-is-type-only")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::{migration_001, pragma_table_columns};

    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migration_001::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn adds_is_type_only_column() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        let cols = pragma_table_columns(&conn, "edges").unwrap();
        assert!(cols.contains(&"is_type_only".to_string()));
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run must not error
    }

    #[test]
    fn existing_edges_get_null_not_fabricated_zero() {
        let mut conn = setup_db();

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
        // A resolved edge inserted BEFORE the column exists.
        conn.execute(
            "INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, name) \
             VALUES ('na','s1','r1','r1:a:MODULE','MODULE','a'), \
                    ('nb','s1','r1','r1:b:MODULE','MODULE','b')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
             target_node_uid, type, resolution, extractor) \
             VALUES ('e1','s1','r1','na','nb','IMPORTS','static','x')",
            [],
        )
        .unwrap();

        run(&mut conn).unwrap();

        // The pre-migration edge carries NULL (unknown), never a fabricated 0.
        let v: Option<i64> = conn
            .query_row(
                "SELECT is_type_only FROM edges WHERE edge_uid = 'e1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(v, None);
    }
}
