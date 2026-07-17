//! Migration 030 — persisted resolved-call aggregate (EC-M3B-TRUST-AGG-1).
//!
//! Adds the snapshot-level g1 aggregate columns to `snapshots`
//! (ENGINE-CONSOLIDATION-1 §5.2 milestone M-3b, D-EC-7-A):
//!
//! - `resolved_call_count INTEGER` — the snapshot's resolved CALLS-edge
//!   count, SUPPLIED by the pipeline at index/refresh finalization from
//!   the resolver's full output stream (counted before storage
//!   materialization — it must survive per-language CALLS-row drops,
//!   M-6) and adjusted atomically by the enrichment-promotion
//!   transaction's net CALLS delta. `NULL` means "not persisted" (a
//!   pre-migration snapshot) — NEVER a measured zero. `0` means the
//!   pipeline measured zero resolved calls. Unknown is never zero.
//! - `resolved_call_provenance TEXT` — the explicit provenance label the
//!   ratified interim rule requires (EC-1 §8 supersession (c)): the value
//!   is PIPELINE-derived (one coherent accounting, matching the trust
//!   denominator), EXPLICITLY TEMPORARY until the reconciliation layer
//!   (recon-design-1) ships its own accounting. Both writers live in
//!   `crud/snapshots.rs` (the write census); the label stamped is
//!   `'pipeline'`.
//!
//! # Why columns on `snapshots`, not a `measurements` row
//!
//! The snapshot row already carries the pipeline-written snapshot-level
//! counters (`files_total`/`nodes_total`/`edges_total`) — this is the
//! same artifact class, narrowed to CALLS and given honest unknown
//! semantics (nullable). A `measurements` row was rejected: the
//! registered Measurements family contract declares file-local,
//! source-file-provenance, copy-forward-on-refresh semantics
//! (artifact-contracts registry), all three of which are false for a
//! snapshot-scoped pipeline aggregate.
//!
//! # Refresh behavior
//!
//! No copy-forward: each snapshot's aggregate is supplied fresh from that
//! snapshot's own resolution run (delta refresh re-resolves the full
//! extraction stream — copied-forward + fresh FC0 rows — so the supplied
//! value is full-stream and language-complete).
//!
//! # Migration Strategy
//!
//! Existing snapshots get `NULL` in both columns: they carry no persisted
//! aggregate, and the trust core falls back to the live CALLS-row COUNT
//! for them (labeled fallback — see `trust::service`).
//!
//! # Idempotence
//!
//! Safe to re-run; columns are added only when absent (mirrors 028).

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

/// Run migration 030 against the given connection.
///
/// Idempotent: re-running on a database that already has both columns is a no-op.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    let cols = pragma_table_columns(conn, "snapshots")?;

    if !cols.iter().any(|c| c == "resolved_call_count") {
        conn.execute_batch("ALTER TABLE snapshots ADD COLUMN resolved_call_count INTEGER")?;
    }

    if !cols.iter().any(|c| c == "resolved_call_provenance") {
        conn.execute_batch("ALTER TABLE snapshots ADD COLUMN resolved_call_provenance TEXT")?;
    }

    record_migration(conn, 30, "030-resolved-call-aggregate")?;
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
    fn adds_resolved_call_aggregate_columns() {
        let mut conn = setup_db();

        let cols_before = pragma_table_columns(&conn, "snapshots").unwrap();
        assert!(!cols_before.contains(&"resolved_call_count".to_string()));
        assert!(!cols_before.contains(&"resolved_call_provenance".to_string()));

        run(&mut conn).unwrap();

        let cols_after = pragma_table_columns(&conn, "snapshots").unwrap();
        assert!(cols_after.contains(&"resolved_call_count".to_string()));
        assert!(cols_after.contains(&"resolved_call_provenance".to_string()));
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run must not error
    }

    #[test]
    fn existing_rows_get_null_not_zero() {
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

        run(&mut conn).unwrap();

        // A pre-migration snapshot has NO aggregate — NULL, never a
        // fabricated 0 (unknown is never zero).
        let (count, provenance): (Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT resolved_call_count, resolved_call_provenance \
                 FROM snapshots WHERE snapshot_uid = 's1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, None);
        assert_eq!(provenance, None);
    }
}
