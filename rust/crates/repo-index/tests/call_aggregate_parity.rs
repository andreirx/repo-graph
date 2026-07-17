//! EC-M3A-AGG-REHOME-1 — persisted g2/g3 family parity (per-symbol CALLS
//! degrees + resolved-call file pairs).
//!
//! The slice's self-validating parity window: while every resolved CALLS
//! result materializes as an `edges` row (pre-M-6), the persisted
//! families MUST equal the live row-derived values — asserted through the
//! REAL disk-to-SQLite pipeline on BOTH a fresh index and a delta refresh
//! (copy-forward + full re-resolution exercised, not just fresh).
//!
//! Both sides of every assertion read the SQLite artifact directly
//! through a second plain connection (storage diagnostics — the
//! persisted side has no value-level production read surface by design:
//! its production consumers are the dead-liveness and map-sketch SQL
//! branches, covered by `storage/tests/call_aggregate_families.rs`).
//! A parity failure here is a slice §3 FINDING, not something to relax.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use repo_graph_repo_index::compose::{index_into_storage, refresh_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

/// Three-file TS repo:
/// - `src/util.ts` — helper() + an INTRA-file call (caller → helper):
///   g2 evidence with no file pair.
/// - `src/main.ts` — imports helper and calls it TWICE: the CROSS-file
///   CALLS shape (g3 pair main→util, multiplicity 2), riding the
///   unchanged-file copy-forward on refresh.
/// - `src/other.ts` — the churn file (changed to force a delta refresh).
fn make_calls_repo(dir: &Path) {
    fs::write(dir.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/util.ts"),
        "export function helper() { return 1; }\n\
         export function caller() { return helper(); }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/main.ts"),
        "import { helper } from './util';\n\
         export function main() { return helper() + helper(); }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/other.ts"),
        "export function other() { return 2; }\n",
    )
    .unwrap();
}

/// Live row-derived per-symbol degrees: node_uid → (fan_in, fan_out),
/// only nodes with any CALLS involvement (mirrors the sparse family).
fn live_degrees(conn: &rusqlite::Connection, snap: &str) -> BTreeMap<String, (i64, i64)> {
    let mut map: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut fan_in = conn
        .prepare(
            "SELECT target_node_uid, COUNT(*) FROM edges \
             WHERE snapshot_uid = ?1 AND type = 'CALLS' GROUP BY target_node_uid",
        )
        .unwrap();
    let rows = fan_in
        .query_map([snap], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .unwrap();
    for row in rows {
        let (uid, n) = row.unwrap();
        map.entry(uid).or_insert((0, 0)).0 = n;
    }
    let mut fan_out = conn
        .prepare(
            "SELECT source_node_uid, COUNT(*) FROM edges \
             WHERE snapshot_uid = ?1 AND type = 'CALLS' GROUP BY source_node_uid",
        )
        .unwrap();
    let rows = fan_out
        .query_map([snap], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .unwrap();
    for row in rows {
        let (uid, n) = row.unwrap();
        map.entry(uid).or_insert((0, 0)).1 = n;
    }
    map
}

fn persisted_degrees(conn: &rusqlite::Connection, snap: &str) -> BTreeMap<String, (i64, i64)> {
    let mut stmt = conn
        .prepare(
            "SELECT node_uid, call_fan_in, call_fan_out FROM symbol_call_degrees \
             WHERE snapshot_uid = ?1",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?),
            ))
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// Live row-derived cross-file CALLS pairs with multiplicity (the exact
/// join semantics of the pre-M-3a map sketch, plus COUNT).
fn live_pairs(conn: &rusqlite::Connection, snap: &str) -> Vec<(String, String, i64)> {
    let mut stmt = conn
        .prepare(
            "SELECT src_f.path, tgt_f.path, COUNT(*) FROM edges e \
             JOIN nodes sn ON e.source_node_uid = sn.node_uid \
             JOIN files src_f ON sn.file_uid = src_f.file_uid \
             JOIN nodes tn ON e.target_node_uid = tn.node_uid \
             JOIN files tgt_f ON tn.file_uid = tgt_f.file_uid \
             WHERE e.snapshot_uid = ?1 AND e.type = 'CALLS' \
               AND src_f.path <> tgt_f.path \
             GROUP BY src_f.path, tgt_f.path \
             ORDER BY src_f.path, tgt_f.path",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn persisted_pairs(conn: &rusqlite::Connection, snap: &str) -> Vec<(String, String, i64)> {
    let mut stmt = conn
        .prepare(
            "SELECT source_file, target_file, call_edge_count \
             FROM resolved_call_file_pairs \
             WHERE snapshot_uid = ?1 AND call_edge_count > 0 \
             ORDER BY source_file, target_file",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn marker(conn: &rusqlite::Connection, snap: &str, column: &str) -> Option<String> {
    conn.query_row(
        &format!("SELECT {column} FROM snapshots WHERE snapshot_uid = ?1"),
        [snap],
        |r| r.get::<_, Option<String>>(0),
    )
    .unwrap()
}

/// Assert full g2 + g3 parity for one snapshot; returns
/// (degree-row count, cross-file pair count) for non-vacuity guards.
fn assert_parity(db_path: &Path, snapshot_uid: &str, context: &str) -> (usize, usize) {
    let conn = rusqlite::Connection::open(db_path).unwrap();

    // Presence markers stamped with the ratified interim-rule label.
    for column in ["symbol_call_degree_provenance", "call_file_pair_provenance"] {
        assert_eq!(
            marker(&conn, snapshot_uid, column).as_deref(),
            Some("pipeline"),
            "{context}: {column} must carry the interim-rule label"
        );
    }

    let live_deg = live_degrees(&conn, snapshot_uid);
    let pers_deg = persisted_degrees(&conn, snapshot_uid);
    assert_eq!(
        pers_deg, live_deg,
        "{context}: persisted g2 degrees must equal live row-derived degrees \
         (parity window violated — slice §3 FINDING)"
    );

    let live_p = live_pairs(&conn, snapshot_uid);
    let pers_p = persisted_pairs(&conn, snapshot_uid);
    assert_eq!(
        pers_p, live_p,
        "{context}: persisted g3 pairs must equal live row-derived pairs \
         (parity window violated — slice §3 FINDING)"
    );

    (pers_deg.len(), pers_p.len())
}

#[test]
fn fresh_index_persists_parity_equal_families() {
    let dir = tempfile::tempdir().unwrap();
    make_calls_repo(dir.path());
    let db_path = dir.path().join("index.db");

    let mut storage = StorageConnection::open(&db_path).unwrap();
    let result =
        index_into_storage(dir.path(), &mut storage, "r1", &ComposeOptions::default()).unwrap();

    let (degree_rows, pair_rows) = assert_parity(&db_path, &result.snapshot_uid, "fresh index");
    // Guards against vacuous empty == empty passes: the fixture must
    // produce per-symbol degrees AND at least one CROSS-FILE pair.
    assert!(
        degree_rows > 0,
        "fixture must produce at least one symbol with CALLS degree"
    );
    assert!(
        pair_rows > 0,
        "fixture must produce at least one cross-file CALLS pair \
         (main.ts → util.ts) — otherwise the g3 parity assertion is vacuous"
    );

    // The cross-file multiplicity is genuinely > 1 (two helper() calls
    // collapsed into one pair) — pinning the dedup-with-multiplicity
    // shape end-to-end.
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let pairs = persisted_pairs(&conn, &result.snapshot_uid);
    assert!(
        pairs
            .iter()
            .any(|(s, t, n)| s == "src/main.ts" && t == "src/util.ts" && *n == 2),
        "expected the (src/main.ts → src/util.ts) pair with multiplicity 2, got {pairs:?}"
    );
}

#[test]
fn delta_refresh_recomputes_families_with_copy_forward_exercised() {
    let dir = tempfile::tempdir().unwrap();
    make_calls_repo(dir.path());
    let db_path = dir.path().join("index.db");

    let mut storage = StorageConnection::open(&db_path).unwrap();

    // Phase 1: full index.
    let r1 =
        index_into_storage(dir.path(), &mut storage, "r1", &ComposeOptions::default()).unwrap();
    let (deg1, pairs1) = assert_parity(&db_path, &r1.snapshot_uid, "fresh index (pre-refresh)");
    assert!(deg1 > 0 && pairs1 > 0, "fixture must be non-vacuous");

    // Phase 2: change ONLY other.ts — util.ts and main.ts (the CALLS
    // carriers) stay unchanged, so their extraction edges ride the delta
    // copy-forward and are re-resolved into the child snapshot.
    fs::write(
        dir.path().join("src/other.ts"),
        "export function other() { return 3; }\n",
    )
    .unwrap();

    // Phase 3: delta refresh.
    let r2 =
        refresh_into_storage(dir.path(), &mut storage, "r1", &ComposeOptions::default()).unwrap();

    // Prove this exercised the DELTA path, not the no-parent full-index
    // fallback (else the copy-forward claim below is untested).
    let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
    assert_eq!(snap2.kind, "refresh", "delta path must be exercised");
    assert_eq!(
        snap2.parent_snapshot_uid,
        Some(r1.snapshot_uid.clone()),
        "refresh snapshot must link to the parent"
    );

    // The child snapshot's families: recomputed from the full
    // re-resolved stream (copied-forward + fresh), parity-equal.
    let (deg2, pairs2) = assert_parity(&db_path, &r2.snapshot_uid, "delta refresh");
    assert_eq!(
        (deg2, pairs2),
        (deg1, pairs1),
        "unchanged CALLS carriers ⇒ same family cardinalities across refresh"
    );

    // The PARENT snapshot's families are untouched by the refresh (its
    // rows remain readable by pinned uid — the W-B rule).
    assert_parity(&db_path, &r1.snapshot_uid, "parent after refresh");
}
