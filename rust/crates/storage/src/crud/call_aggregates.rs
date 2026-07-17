//! Write census for the M-3a FC2a-agg families (EC-M3A-AGG-REHOME-1).
//!
//! EVERY writer of `symbol_call_degrees` (g2), `resolved_call_file_pairs`
//! (g3) and their `snapshots` presence markers lives in THIS file, so the
//! parity story stays auditable in one place (the same census discipline
//! M-3b established for the g1 columns in `crud/snapshots.rs`):
//!
//! - [`StorageConnection::persist_symbol_call_degrees`] /
//!   [`StorageConnection::persist_resolved_call_file_pairs`] — the
//!   pipeline writers, called at `run_pipeline` Phase-5 finalization
//!   (fresh index AND delta refresh share them) with values TALLIED FROM
//!   THE RESOLVER'S OUTPUT STREAM, before any storage materialization.
//!   NEVER derived from the `edges` table: after a per-language CALLS-row
//!   drop (EC-1 M-6) `edges` is a filtered subset of the stream, and any
//!   row-derived recompute would bake the undercount in — a silently
//!   false-dead liveness answer and a silently thinned dep sketch, the
//!   exact failures M-3a exists to prevent.
//! - [`adjust_symbol_call_degrees`] / [`adjust_resolved_call_file_pairs`]
//!   — the enrichment-promotion writers: delta arithmetic INSIDE the same
//!   transaction as the promotion's edge-row mutations (see
//!   `enrichment_impl::apply_promotion`), so families and rows commit or
//!   roll back together. Deltas may only describe rows the transaction
//!   itself mutates; the schema `CHECK (… >= 0)` makes any accounting bug
//!   that would drive a value negative fail the whole transaction loudly.
//!
//! # Presence markers — unknown is never zero
//!
//! The zero-rows state is ambiguous on its own (measured "no calls" vs
//! "family never persisted"), so presence is carried by the nullable
//! `snapshots` marker columns (migration 031):
//! `symbol_call_degree_provenance` / `call_file_pair_provenance`.
//! `NULL` = not persisted → readers fall back to the labeled live
//! row-derived path. Non-NULL = persisted (zero rows = measured zero),
//! stamped with the ratified interim-rule accounting label
//! ([`super::snapshots::RESOLVED_CALL_PROVENANCE_PIPELINE`], `'pipeline'`
//! — the same EXPLICITLY-TEMPORARY accounting all three FC2a-agg
//! granularities share until the reconciliation layer ships).
//!
//! The promotion adjusters are marker-gated by their caller: a
//! pre-migration snapshot (NULL marker) is NEVER seeded — seeding would
//! require deriving a base from `edges` rows, the banned accounting.

use rusqlite::Connection;

use crate::connection::StorageConnection;
use crate::crud::snapshots::RESOLVED_CALL_PROVENANCE_PIPELINE;
use crate::error::StorageError;

/// One persisted per-symbol CALLS-degree row (g2), storage-side shape.
///
/// `call_fan_in` = number of resolved CALLS edges targeting the symbol
/// (the dead-liveness input: alive ⇔ fan-in > 0 for the CALLS share);
/// `call_fan_out` = number of resolved CALLS edges originating at it
/// (§2b's other per-function skeleton column — written by the same
/// producer per the ratified M-3a row; no read consumer yet).
#[derive(Debug, Clone)]
pub struct SymbolCallDegreeRow {
    pub node_uid: String,
    pub call_fan_in: u64,
    pub call_fan_out: u64,
}

/// One persisted resolved-CALLS file-pair row (g3), storage-side shape.
///
/// `source_file`/`target_file` are repo-relative paths (`files.path`
/// values — what map's sketch renders); `call_edge_count` is the
/// CALLS-edge multiplicity behind the pair (dedup bookkeeping for the
/// promotion delta — the pair is visible while it is > 0).
#[derive(Debug, Clone)]
pub struct ResolvedCallFilePairRow {
    pub source_file: String,
    pub target_file: String,
    pub call_edge_count: u64,
}

/// Which M-3a family a `snapshots` presence marker describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallAggregateFamily {
    /// g2 — `symbol_call_degrees` / `symbol_call_degree_provenance`.
    SymbolCallDegrees,
    /// g3 — `resolved_call_file_pairs` / `call_file_pair_provenance`.
    ResolvedCallFilePairs,
}

impl CallAggregateFamily {
    fn marker_column(self) -> &'static str {
        match self {
            CallAggregateFamily::SymbolCallDegrees => "symbol_call_degree_provenance",
            CallAggregateFamily::ResolvedCallFilePairs => "call_file_pair_provenance",
        }
    }
}

/// Read a family's presence marker for one snapshot.
///
/// Returns `Some(label)` ONLY for a WELL-FORMED persisted state: a
/// non-NULL, non-empty label (the M-3b read-validation rule — an empty
/// label is a state no sanctioned writer produces, and a corrupt marker
/// must never present the family as measured). `None` for a missing
/// snapshot row, a NULL marker (pre-migration snapshot), or an empty
/// label — in every case the caller serves the labeled live row-derived
/// fallback.
pub(crate) fn family_marker(
    conn: &Connection,
    snapshot_uid: &str,
    family: CallAggregateFamily,
) -> Result<Option<String>, StorageError> {
    let sql = format!(
        "SELECT {} FROM snapshots WHERE snapshot_uid = ?",
        family.marker_column()
    );
    let result = conn.query_row(&sql, rusqlite::params![snapshot_uid], |row| {
        row.get::<_, Option<String>>(0)
    });
    match result {
        Ok(Some(label)) if !label.is_empty() => Ok(Some(label)),
        Ok(_) => Ok(None),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(StorageError::Sqlite(e)),
    }
}

impl StorageConnection {
    /// Persist the per-symbol CALLS-degree family (g2) for one snapshot,
    /// replacing any prior rows, and stamp the presence marker — in ONE
    /// transaction (a marker without its rows, or rows without their
    /// marker, is a state no reader should ever observe).
    ///
    /// `degrees` is SUPPLIED by the pipeline (stream-side tally); rows
    /// with both degrees zero need not be supplied — with the marker
    /// stamped, a missing row IS the measured zero. Idempotent
    /// (delete-then-insert): re-running finalization converges.
    pub fn persist_symbol_call_degrees(
        &self,
        snapshot_uid: &str,
        degrees: &[SymbolCallDegreeRow],
    ) -> Result<(), StorageError> {
        let conn = self.connection();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM symbol_call_degrees WHERE snapshot_uid = ?",
            rusqlite::params![snapshot_uid],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO symbol_call_degrees \
                 (snapshot_uid, node_uid, call_fan_in, call_fan_out) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for d in degrees {
                stmt.execute(rusqlite::params![
                    snapshot_uid,
                    d.node_uid,
                    i64::try_from(d.call_fan_in).expect("fan-in exceeds i64 — impossible"),
                    i64::try_from(d.call_fan_out).expect("fan-out exceeds i64 — impossible"),
                ])?;
            }
        }
        tx.execute(
            "UPDATE snapshots SET symbol_call_degree_provenance = ? WHERE snapshot_uid = ?",
            rusqlite::params![RESOLVED_CALL_PROVENANCE_PIPELINE, snapshot_uid],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Persist the resolved-CALLS file-pair family (g3) for one snapshot,
    /// replacing any prior rows, and stamp the presence marker — one
    /// transaction, same contract as
    /// [`Self::persist_symbol_call_degrees`].
    pub fn persist_resolved_call_file_pairs(
        &self,
        snapshot_uid: &str,
        pairs: &[ResolvedCallFilePairRow],
    ) -> Result<(), StorageError> {
        let conn = self.connection();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM resolved_call_file_pairs WHERE snapshot_uid = ?",
            rusqlite::params![snapshot_uid],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO resolved_call_file_pairs \
                 (snapshot_uid, source_file, target_file, call_edge_count) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for p in pairs {
                stmt.execute(rusqlite::params![
                    snapshot_uid,
                    p.source_file,
                    p.target_file,
                    i64::try_from(p.call_edge_count).expect("pair count exceeds i64 — impossible"),
                ])?;
            }
        }
        tx.execute(
            "UPDATE snapshots SET call_file_pair_provenance = ? WHERE snapshot_uid = ?",
            rusqlite::params![RESOLVED_CALL_PROVENANCE_PIPELINE, snapshot_uid],
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// Apply per-symbol degree deltas (g2) inside the promotion transaction.
///
/// Takes a raw [`Connection`] so the caller can pass its
/// `rusqlite::Transaction` (derefs to `Connection`) — the deltas MUST
/// commit or roll back with the exact edge-row mutations they account
/// for.
///
/// UPDATE-first, INSERT-only-when-absent (NOT `ON CONFLICT DO UPDATE`):
/// SQLite evaluates table `CHECK` constraints on the candidate INSERT
/// row BEFORE upsert conflict resolution, so a legitimate negative delta
/// against an existing row would be rejected by the candidate-row CHECK
/// without ever reaching the arithmetic UPDATE branch. The two-statement
/// form keeps the CHECK exactly where it belongs: an existing row driven
/// negative fails on the UPDATE, a decrement against an ABSENT row fails
/// on the INSERT — both are accounting bugs that abort the whole
/// promotion transaction loudly (rows and families revert together;
/// fabricated data is never stored).
///
/// Caller contract (enforced in `apply_promotion`): only invoked when
/// the g2 presence marker is stamped — a pre-migration snapshot is never
/// seeded.
pub(crate) fn adjust_symbol_call_degrees(
    conn: &Connection,
    snapshot_uid: &str,
    deltas: &[(String, i64, i64)],
) -> Result<(), StorageError> {
    let mut update = conn.prepare(
        "UPDATE symbol_call_degrees SET \
           call_fan_in = call_fan_in + ?3, \
           call_fan_out = call_fan_out + ?4 \
         WHERE snapshot_uid = ?1 AND node_uid = ?2",
    )?;
    let mut insert = conn.prepare(
        "INSERT INTO symbol_call_degrees \
         (snapshot_uid, node_uid, call_fan_in, call_fan_out) \
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (node_uid, d_in, d_out) in deltas {
        if *d_in == 0 && *d_out == 0 {
            continue;
        }
        let affected = update.execute(rusqlite::params![snapshot_uid, node_uid, d_in, d_out])?;
        if affected == 0 {
            insert.execute(rusqlite::params![snapshot_uid, node_uid, d_in, d_out])?;
        }
    }
    Ok(())
}

/// Apply file-pair count deltas (g3) inside the promotion transaction.
/// Same transaction/two-statement/CHECK contract as
/// [`adjust_symbol_call_degrees`]. A pair whose count reaches zero keeps
/// its row (count 0 = measured "no remaining CALLS multiplicity"; readers
/// filter `call_edge_count > 0`).
pub(crate) fn adjust_resolved_call_file_pairs(
    conn: &Connection,
    snapshot_uid: &str,
    deltas: &[(String, String, i64)],
) -> Result<(), StorageError> {
    let mut update = conn.prepare(
        "UPDATE resolved_call_file_pairs SET \
           call_edge_count = call_edge_count + ?4 \
         WHERE snapshot_uid = ?1 AND source_file = ?2 AND target_file = ?3",
    )?;
    let mut insert = conn.prepare(
        "INSERT INTO resolved_call_file_pairs \
         (snapshot_uid, source_file, target_file, call_edge_count) \
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (source_file, target_file, delta) in deltas {
        if *delta == 0 {
            continue;
        }
        let affected = update.execute(rusqlite::params![
            snapshot_uid,
            source_file,
            target_file,
            delta
        ])?;
        if affected == 0 {
            insert.execute(rusqlite::params![
                snapshot_uid,
                source_file,
                target_file,
                delta
            ])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::test_helpers::{fresh_storage, make_repo};
    use crate::types::CreateSnapshotInput;

    fn seed_snapshot(storage: &StorageConnection) -> String {
        storage.add_repo(&make_repo("r1")).unwrap();
        storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
            .snapshot_uid
    }

    fn degree_row(storage: &StorageConnection, snap: &str, node: &str) -> Option<(i64, i64)> {
        storage
            .connection()
            .query_row(
                "SELECT call_fan_in, call_fan_out FROM symbol_call_degrees \
                 WHERE snapshot_uid = ?1 AND node_uid = ?2",
                rusqlite::params![snap, node],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok()
    }

    fn pair_row(storage: &StorageConnection, snap: &str, src: &str, tgt: &str) -> Option<i64> {
        storage
            .connection()
            .query_row(
                "SELECT call_edge_count FROM resolved_call_file_pairs \
                 WHERE snapshot_uid = ?1 AND source_file = ?2 AND target_file = ?3",
                rusqlite::params![snap, src, tgt],
                |row| row.get(0),
            )
            .ok()
    }

    #[test]
    fn persist_symbol_call_degrees_stores_rows_and_stamps_marker() {
        let storage = fresh_storage();
        let snap = seed_snapshot(&storage);

        assert_eq!(
            family_marker(
                storage.connection(),
                &snap,
                CallAggregateFamily::SymbolCallDegrees
            )
            .unwrap(),
            None,
            "precondition: no marker before persist"
        );

        storage
            .persist_symbol_call_degrees(
                &snap,
                &[
                    SymbolCallDegreeRow {
                        node_uid: "n1".into(),
                        call_fan_in: 2,
                        call_fan_out: 0,
                    },
                    SymbolCallDegreeRow {
                        node_uid: "n2".into(),
                        call_fan_in: 0,
                        call_fan_out: 2,
                    },
                ],
            )
            .unwrap();

        assert_eq!(degree_row(&storage, &snap, "n1"), Some((2, 0)));
        assert_eq!(degree_row(&storage, &snap, "n2"), Some((0, 2)));
        assert_eq!(
            family_marker(
                storage.connection(),
                &snap,
                CallAggregateFamily::SymbolCallDegrees
            )
            .unwrap()
            .as_deref(),
            Some(RESOLVED_CALL_PROVENANCE_PIPELINE)
        );
    }

    #[test]
    fn persist_is_idempotent_replace_not_accumulate() {
        let storage = fresh_storage();
        let snap = seed_snapshot(&storage);

        let rows = [SymbolCallDegreeRow {
            node_uid: "n1".into(),
            call_fan_in: 3,
            call_fan_out: 1,
        }];
        storage.persist_symbol_call_degrees(&snap, &rows).unwrap();
        storage.persist_symbol_call_degrees(&snap, &rows).unwrap();
        assert_eq!(
            degree_row(&storage, &snap, "n1"),
            Some((3, 1)),
            "re-persist replaces, never accumulates"
        );

        let pairs = [ResolvedCallFilePairRow {
            source_file: "a.ts".into(),
            target_file: "b.ts".into(),
            call_edge_count: 2,
        }];
        storage
            .persist_resolved_call_file_pairs(&snap, &pairs)
            .unwrap();
        storage
            .persist_resolved_call_file_pairs(&snap, &pairs)
            .unwrap();
        assert_eq!(pair_row(&storage, &snap, "a.ts", "b.ts"), Some(2));
    }

    #[test]
    fn empty_family_still_stamps_marker_measured_zero_not_unknown() {
        let storage = fresh_storage();
        let snap = seed_snapshot(&storage);

        storage.persist_symbol_call_degrees(&snap, &[]).unwrap();
        storage
            .persist_resolved_call_file_pairs(&snap, &[])
            .unwrap();

        // Zero rows + stamped marker = measured zero (a snapshot with no
        // resolved calls), distinguishable from the pre-migration NULL.
        for family in [
            CallAggregateFamily::SymbolCallDegrees,
            CallAggregateFamily::ResolvedCallFilePairs,
        ] {
            assert_eq!(
                family_marker(storage.connection(), &snap, family)
                    .unwrap()
                    .as_deref(),
                Some(RESOLVED_CALL_PROVENANCE_PIPELINE),
                "{family:?}: empty family must still stamp its marker"
            );
        }
    }

    #[test]
    fn family_marker_rejects_empty_label_as_unservable() {
        let storage = fresh_storage();
        let snap = seed_snapshot(&storage);

        // An empty label is a state no sanctioned writer produces —
        // well-formedness validation degrades it to "no family" so the
        // fallback serves (mirrors the M-3b read validation).
        storage
            .connection()
            .execute(
                "UPDATE snapshots SET symbol_call_degree_provenance = '' \
                 WHERE snapshot_uid = ?",
                rusqlite::params![snap],
            )
            .unwrap();
        assert_eq!(
            family_marker(
                storage.connection(),
                &snap,
                CallAggregateFamily::SymbolCallDegrees
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn adjust_upserts_and_applies_net_deltas() {
        let storage = fresh_storage();
        let snap = seed_snapshot(&storage);
        storage
            .persist_symbol_call_degrees(
                &snap,
                &[SymbolCallDegreeRow {
                    node_uid: "n1".into(),
                    call_fan_in: 1,
                    call_fan_out: 0,
                }],
            )
            .unwrap();

        // Existing row adjusted; unseen symbol upserted with its delta.
        adjust_symbol_call_degrees(
            storage.connection(),
            &snap,
            &[
                ("n1".into(), 2, 1),
                ("n9".into(), 1, 0),
                ("nz".into(), 0, 0),
            ],
        )
        .unwrap();
        assert_eq!(degree_row(&storage, &snap, "n1"), Some((3, 1)));
        assert_eq!(degree_row(&storage, &snap, "n9"), Some((1, 0)));
        assert_eq!(
            degree_row(&storage, &snap, "nz"),
            None,
            "zero delta writes nothing"
        );

        storage
            .persist_resolved_call_file_pairs(
                &snap,
                &[ResolvedCallFilePairRow {
                    source_file: "a.ts".into(),
                    target_file: "b.ts".into(),
                    call_edge_count: 1,
                }],
            )
            .unwrap();
        adjust_resolved_call_file_pairs(
            storage.connection(),
            &snap,
            &[
                ("a.ts".into(), "b.ts".into(), -1),
                ("a.ts".into(), "c.ts".into(), 2),
            ],
        )
        .unwrap();
        // Count reaching zero keeps the row (readers filter > 0).
        assert_eq!(pair_row(&storage, &snap, "a.ts", "b.ts"), Some(0));
        assert_eq!(pair_row(&storage, &snap, "a.ts", "c.ts"), Some(2));
    }

    #[test]
    fn family_rows_cascade_with_snapshot_deletion() {
        let storage = fresh_storage();
        let snap = seed_snapshot(&storage);
        storage
            .persist_symbol_call_degrees(
                &snap,
                &[SymbolCallDegreeRow {
                    node_uid: "n1".into(),
                    call_fan_in: 1,
                    call_fan_out: 0,
                }],
            )
            .unwrap();
        storage
            .persist_resolved_call_file_pairs(
                &snap,
                &[ResolvedCallFilePairRow {
                    source_file: "a.ts".into(),
                    target_file: "b.ts".into(),
                    call_edge_count: 1,
                }],
            )
            .unwrap();

        // The CASCADE FK is the families' retention path:
        // `delete_snapshots_cascade` relies on it for every table that
        // declares it (no orphan-cleanup entry needed).
        storage
            .connection()
            .execute(
                "DELETE FROM snapshots WHERE snapshot_uid = ?1",
                rusqlite::params![snap],
            )
            .unwrap();

        for table in ["symbol_call_degrees", "resolved_call_file_pairs"] {
            let rows: i64 = storage
                .connection()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(rows, 0, "{table} rows must CASCADE with the snapshot");
        }
    }

    #[test]
    fn adjust_that_would_go_negative_fails_loudly() {
        let storage = fresh_storage();
        let snap = seed_snapshot(&storage);
        storage
            .persist_symbol_call_degrees(
                &snap,
                &[SymbolCallDegreeRow {
                    node_uid: "n1".into(),
                    call_fan_in: 1,
                    call_fan_out: 0,
                }],
            )
            .unwrap();

        // -2 on fan_in=1 would store -1: the schema CHECK rejects it —
        // an accounting bug is an error, never fabricated data.
        let result =
            adjust_symbol_call_degrees(storage.connection(), &snap, &[("n1".into(), -2, 0)]);
        assert!(result.is_err(), "negative-driving delta must fail");
        assert_eq!(
            degree_row(&storage, &snap, "n1"),
            Some((1, 0)),
            "failed adjust leaves the row untouched"
        );
    }
}
