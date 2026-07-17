//! Migration 031 — persisted per-symbol CALLS degrees + resolved-call
//! file pairs (EC-M3A-AGG-REHOME-1).
//!
//! Adds the two sub-snapshot FC2a-agg families of ENGINE-CONSOLIDATION-1
//! §5.2 milestone M-3a (D-EC-7-A, g3 sub-choice A-i), extending the M-3b
//! (migration 030) producer pattern:
//!
//! - `symbol_call_degrees` (g2, per-symbol): one row per symbol that has
//!   at least one resolved CALLS endpoint — `call_fan_in` (incoming CALLS
//!   degree; the dead-liveness input) and `call_fan_out` (outgoing CALLS
//!   degree; §2b's other skeleton column). SUPPLIED by the pipeline from
//!   the resolver's full output stream (all languages, counted BEFORE
//!   storage materialization — it must survive per-language CALLS-row
//!   drops, M-6) and adjusted atomically by the enrichment-promotion
//!   transaction. A missing row for a snapshot whose marker (below) is
//!   stamped means "measured degree 0", NOT unknown.
//! - `resolved_call_file_pairs` (g3, per-file-pair): one row per DISTINCT
//!   (source_file, target_file) pair connected by at least one resolved
//!   CALLS edge whose two endpoint symbols live in different indexed
//!   files (the exact shape map's dep sketch consumes). `call_edge_count`
//!   is the CALLS-edge multiplicity behind the pair — persisted so the
//!   promotion transaction can maintain the DISTINCT pair set by lawful
//!   delta arithmetic (a bare pair set cannot be delta-maintained: without
//!   multiplicity there is no way to know when a pair's LAST edge
//!   disappears, and recomputing from `edges` is banned — the M-3b rule).
//!   A pair is visible to readers while `call_edge_count > 0`.
//!
//! Both tables `REFERENCES snapshots ON DELETE CASCADE` (the standard
//! per-snapshot family lifecycle — retention prune removes them with the
//! snapshot row; no `delete_snapshots_cascade` change needed).
//!
//! `CHECK (… >= 0)` on every degree/count column: no sanctioned writer
//! can produce a negative value, so a delta that would drive one negative
//! is an accounting bug — the constraint makes the enclosing promotion
//! transaction FAIL LOUDLY and roll back rows + families together, rather
//! than storing fabricated data (the write-side analogue of M-3b's
//! read-side negative-count degrade; here the invalid state is simply
//! unrepresentable because the tables never existed without the CHECK).
//!
//! # Presence markers (unknown is never zero)
//!
//! Two nullable `snapshots` columns, mirroring migration 030's
//! `resolved_call_provenance`:
//!
//! - `symbol_call_degree_provenance TEXT`
//! - `call_file_pair_provenance TEXT`
//!
//! `NULL` = the family was never persisted for this snapshot (a
//! pre-migration snapshot) — readers MUST fall back to the live
//! CALLS-row-derived path, never treat empty family rows as measured
//! zeros. A non-NULL label = the family is present (possibly with zero
//! rows = measured zero) and carries the ratified interim-rule accounting
//! label (`'pipeline'` — EC-1 §8 supersession (c): pipeline-derived, one
//! coherent accounting, EXPLICITLY TEMPORARY until the reconciliation
//! layer ships its own).
//!
//! # Refresh behavior
//!
//! No copy-forward: each snapshot's family rows are supplied fresh from
//! that snapshot's own resolution run (delta refresh re-resolves the full
//! extraction stream — copied-forward + fresh FC0 rows — so the supplied
//! values are full-stream and language-complete).
//!
//! # Idempotence
//!
//! Safe to re-run; tables/columns are created only when absent (mirrors
//! 030).

use rusqlite::Connection;

use crate::error::StorageError;
use crate::migrations::{pragma_table_columns, record_migration};

/// Run migration 031 against the given connection.
///
/// Idempotent: re-running on a database that already has the tables and
/// columns is a no-op.
pub fn run(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS symbol_call_degrees (
            snapshot_uid   TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            node_uid       TEXT NOT NULL,
            call_fan_in    INTEGER NOT NULL CHECK (call_fan_in >= 0),
            call_fan_out   INTEGER NOT NULL CHECK (call_fan_out >= 0),
            PRIMARY KEY (snapshot_uid, node_uid)
        );
        CREATE TABLE IF NOT EXISTS resolved_call_file_pairs (
            snapshot_uid    TEXT NOT NULL REFERENCES snapshots(snapshot_uid) ON DELETE CASCADE,
            source_file     TEXT NOT NULL,
            target_file     TEXT NOT NULL,
            call_edge_count INTEGER NOT NULL CHECK (call_edge_count >= 0),
            PRIMARY KEY (snapshot_uid, source_file, target_file)
        );",
    )?;

    let cols = pragma_table_columns(conn, "snapshots")?;
    if !cols.iter().any(|c| c == "symbol_call_degree_provenance") {
        conn.execute_batch("ALTER TABLE snapshots ADD COLUMN symbol_call_degree_provenance TEXT")?;
    }
    if !cols.iter().any(|c| c == "call_file_pair_provenance") {
        conn.execute_batch("ALTER TABLE snapshots ADD COLUMN call_file_pair_provenance TEXT")?;
    }

    record_migration(conn, 31, "031-call-aggregate-families")?;
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
    fn adds_family_tables_and_marker_columns() {
        let mut conn = setup_db();

        run(&mut conn).unwrap();

        // Tables exist with the expected columns.
        let degree_cols = pragma_table_columns(&conn, "symbol_call_degrees").unwrap();
        assert!(degree_cols.contains(&"call_fan_in".to_string()));
        assert!(degree_cols.contains(&"call_fan_out".to_string()));
        let pair_cols = pragma_table_columns(&conn, "resolved_call_file_pairs").unwrap();
        assert!(pair_cols.contains(&"call_edge_count".to_string()));

        // Presence markers on snapshots.
        let snap_cols = pragma_table_columns(&conn, "snapshots").unwrap();
        assert!(snap_cols.contains(&"symbol_call_degree_provenance".to_string()));
        assert!(snap_cols.contains(&"call_file_pair_provenance".to_string()));
    }

    #[test]
    fn idempotent_rerun() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap(); // second run must not error
    }

    #[test]
    fn existing_snapshots_get_null_markers_not_fabricated_presence() {
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

        // A pre-migration snapshot carries NO family markers — NULL, so
        // readers fall back to the live row-derived path (unknown is never
        // rendered as a measured zero).
        let (deg, pair): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT symbol_call_degree_provenance, call_file_pair_provenance \
                 FROM snapshots WHERE snapshot_uid = 's1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(deg, None);
        assert_eq!(pair, None);
    }

    #[test]
    fn negative_values_are_unrepresentable() {
        let mut conn = setup_db();
        run(&mut conn).unwrap();

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

        // A negative degree/count violates the CHECK — the write fails
        // loudly instead of storing fabricated data.
        let deg = conn.execute(
            "INSERT INTO symbol_call_degrees (snapshot_uid, node_uid, call_fan_in, call_fan_out) \
             VALUES ('s1', 'n1', -1, 0)",
            [],
        );
        assert!(deg.is_err(), "negative fan-in must be rejected");

        let pair = conn.execute(
            "INSERT INTO resolved_call_file_pairs \
             (snapshot_uid, source_file, target_file, call_edge_count) \
             VALUES ('s1', 'a.ts', 'b.ts', -1)",
            [],
        );
        assert!(pair.is_err(), "negative pair count must be rejected");
    }
}
