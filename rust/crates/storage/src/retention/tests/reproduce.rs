//! DAEMON-RESIDUALS-2 — retention prune reproduction & per-phase measurement.
//!
//! These are `#[ignore]`d measurement harnesses (run with `--ignored --nocapture`),
//! NOT part of the default gate. They exist to DIAGNOSE the "5h+, zero committed
//! progress" prune the operator measured on a 4.8 GB / 29-snapshot store, on a
//! controlled snapshot-heavy synthetic fixture (the spec's alternative to 6–8 real
//! re-indexes) — the SMALLEST input that reproduces the mechanism deterministically.
//!
//! What lives here:
//! - `explain_fk_child_delete_plan_scan_before_search_after` (PERMANENT GATE, not ignored):
//!   the DETERMINISTIC, size-independent ground truth, now ASSERTED. Each single-column
//!   FK-enforcement lookup SQLite issues while cascading a `nodes` delete
//!   (`edges.source_node_uid`, `edges.target_node_uid`, `unresolved_edges.source_node_uid`,
//!   `nodes.parent_node_uid`) is a full table SCAN without the migration-035 indexes and an
//!   index SEARCH with them — asserted both ways, plus that those four columns are the
//!   COMPLETE FK-to-`nodes` child set.
//! - `retention_prune_benchmark_gate` (PERMANENT GATE, not ignored): the §2.4 retention
//!   benchmark — an N-snapshot × M-row prune under a fixed wall-clock bound on the shipped
//!   schema, with the SEARCH-plan assertion as the deterministic regression guard.
//! - `measure_prune_phases` / `measure_fix_variants` / `measure_insert_path_index_cost`
//!   (`#[ignore]`d measurement harnesses; run with `--ignored --nocapture`): the per-phase
//!   diagnosis, the fix-variant comparison, and the DR-1 insert-path cost.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::connection::StorageConnection;
use crate::types::{CreateSnapshotInput, GraphEdge, GraphNode, Repo, UpdateSnapshotStatusInput};

/// Build one READY snapshot with `nodes` nodes, `2*nodes` edges (a ring so every node
/// is both a source and a target), and `unres` unresolved edges. Returns its uid.
fn build_snapshot(
    storage: &mut StorageConnection,
    repo: &str,
    parent: Option<&str>,
    nodes: usize,
    unres: usize,
) -> String {
    let snap = storage
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: repo.to_string(),
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            parent_snapshot_uid: parent.map(|s| s.to_string()),
            label: None,
            toolchain_json: None,
        })
        .unwrap();
    let uid = snap.snapshot_uid.clone();
    storage
        .update_snapshot_status(&UpdateSnapshotStatusInput {
            snapshot_uid: uid.clone(),
            status: "ready".to_string(),
            completed_at: None,
        })
        .unwrap();

    let node_rows: Vec<GraphNode> = (0..nodes)
        .map(|i| GraphNode {
            node_uid: format!("{uid}-n{i}"),
            snapshot_uid: uid.clone(),
            repo_uid: repo.to_string(),
            stable_key: format!("{repo}:{uid}:n{i}:SYMBOL"),
            kind: "SYMBOL".to_string(),
            subtype: Some("FUNCTION".to_string()),
            name: format!("n{i}"),
            qualified_name: Some(format!("mod::path::n{i}")),
            file_uid: None,
            parent_node_uid: None,
            location: None,
            signature: Some("fn f(a: usize) -> usize".to_string()),
            visibility: Some("export".to_string()),
            doc_comment: Some("padded doc comment to grow the row".to_string()),
            metadata_json: None,
        })
        .collect();
    storage.insert_nodes(&node_rows).unwrap();

    let mut edges: Vec<GraphEdge> = Vec::with_capacity(nodes * 2);
    for i in 0..nodes {
        edges.push(GraphEdge {
            edge_uid: format!("{uid}-callf{i}"),
            snapshot_uid: uid.clone(),
            repo_uid: repo.to_string(),
            source_node_uid: format!("{uid}-n{i}"),
            target_node_uid: format!("{uid}-n{}", (i + 1) % nodes),
            edge_type: "CALLS".to_string(),
            resolution: "static".to_string(),
            extractor: "test".to_string(),
            location: None,
            metadata_json: None,
        });
        edges.push(GraphEdge {
            edge_uid: format!("{uid}-refb{i}"),
            snapshot_uid: uid.clone(),
            repo_uid: repo.to_string(),
            source_node_uid: format!("{uid}-n{i}"),
            target_node_uid: format!("{uid}-n{}", (i + 7) % nodes),
            edge_type: "REFERENCES".to_string(),
            resolution: "static".to_string(),
            extractor: "test".to_string(),
            location: None,
            metadata_json: None,
        });
    }
    storage.insert_edges(&edges).unwrap();

    // unresolved_edges via raw SQL (no public bulk insert on StorageConnection).
    {
        let conn = storage.connection();
        let tx = conn.unchecked_transaction().unwrap();
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO unresolved_edges \
                     (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, \
                      resolution, extractor, category, classification, classifier_version, \
                      basis_code, observed_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 'CALLS', 'unresolved', 'test', 'external', \
                             'external_library_candidate', 1, 'specifier', '2025-01-01T00:00:00Z')",
                )
                .unwrap();
            for i in 0..unres {
                stmt.execute(rusqlite::params![
                    format!("{uid}-ue{i}"),
                    uid,
                    repo,
                    format!("{uid}-n{}", i % nodes),
                    format!("pkg#sym{i}"),
                ])
                .unwrap();
            }
        }
        tx.commit().unwrap();
    }

    uid
}

fn add_repo(storage: &StorageConnection, repo: &str) {
    storage
        .add_repo(&Repo {
            repo_uid: repo.to_string(),
            name: format!("Test {repo}"),
            root_path: ".".to_string(),
            default_branch: Some("main".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            metadata_json: None,
        })
        .unwrap();
}

fn plan(conn: &rusqlite::Connection, sql: &str) -> String {
    let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    let rows = stmt
        .query_map([], |row| {
            // detail is the last column
            let detail: String = row.get(3)?;
            Ok(detail)
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect::<Vec<_>>().join(" | ")
}

/// The COMPLETE set of `(child_table, from_column)` foreign keys that reference
/// `nodes(node_uid)`, read deterministically from the live schema via
/// `PRAGMA foreign_key_list`. This is the ground-truth check behind migration 035's
/// static column list: a grep over the migrations is a claim, this PRAGMA is the check,
/// and if a future migration adds another `nodes`-referencing FK child column this set
/// grows and the completeness assertion below fails until 035's sibling index is added.
fn fk_children_of_nodes(conn: &rusqlite::Connection) -> BTreeSet<(String, String)> {
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap();
        let rows = stmt.query_map([], |row| row.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    let mut out = BTreeSet::new();
    for table in &tables {
        let mut stmt = conn
            .prepare(&format!("PRAGMA foreign_key_list({table})"))
            .unwrap();
        // columns: id, seq, table, from, to, on_update, on_delete, match
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(2)?, row.get::<_, String>(3)?))
            })
            .unwrap();
        for r in rows {
            let (referenced, from_col) = r.unwrap();
            if referenced == "nodes" {
                out.insert((table.clone(), from_col));
            }
        }
    }
    out
}

/// The four single-column FK-child indexes migration 035 ships.
const FK_CHILD_INDEXES: [&str; 4] = [
    "idx_edges_source_node_uid",
    "idx_edges_target_node_uid",
    "idx_unresolved_edges_source_node_uid",
    "idx_nodes_parent_node_uid",
];

/// DETERMINISTIC (size-independent) GATE, not just a print: the migration-035 indexes
/// convert every per-node FK-enforcement lookup the prune cascade issues from a full
/// table SCAN into an index SEARCH.
///
/// This is a permanent (non-ignored) gate because it encodes the DR-1 diagnosis as an
/// assertion — a future schema change that drops or fails to serve one of these indexes
/// re-introduces the O(nodes × child_rows) prune and fails here. It asserts, per the four
/// FK child predicates: SCAN with the 035 index dropped ("before"), SEARCH with it
/// present ("after"), plus that the four indexed columns are the COMPLETE FK-to-`nodes`
/// child set.
#[test]
fn explain_fk_child_delete_plan_scan_before_search_after() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("plan.db");
    let mut storage = StorageConnection::open(&db_path).unwrap();
    add_repo(&storage, "r1");
    // A little data so the planner has stats; the plan is what matters, not the size.
    build_snapshot(&mut storage, "r1", None, 200, 40);
    let conn = storage.connection();

    // Completeness: the columns 035 indexes are exactly the FK-to-`nodes` child set.
    let expected: BTreeSet<(String, String)> = [
        ("edges", "source_node_uid"),
        ("edges", "target_node_uid"),
        ("unresolved_edges", "source_node_uid"),
        ("nodes", "parent_node_uid"),
    ]
    .iter()
    .map(|(t, c)| (t.to_string(), c.to_string()))
    .collect();
    assert_eq!(
        fk_children_of_nodes(conn),
        expected,
        "FK-to-nodes child set changed; migration 035's index list must cover it"
    );

    // (predicate SQL, which 035 index serves it). Each is one FK-enforcement lookup
    // SQLite issues per deleted node while cascading `DELETE FROM nodes`.
    let predicates = [
        ("DELETE FROM edges WHERE source_node_uid = 'x'", 0usize),
        ("DELETE FROM edges WHERE target_node_uid = 'x'", 1),
        (
            "SELECT 1 FROM unresolved_edges WHERE source_node_uid = 'x'",
            2,
        ),
        ("SELECT 1 FROM nodes WHERE parent_node_uid = 'x'", 3),
    ];

    // AFTER (migration applied): every predicate SEARCHes via its single-column index.
    for (sql, _idx) in &predicates {
        let p = plan(conn, sql);
        assert!(
            p.contains("SEARCH") && !p.contains("SCAN"),
            "expected SEARCH (index seek) for `{sql}` with 035 indexes present, got: {p}"
        );
    }

    // BEFORE (035 indexes dropped): every predicate falls back to a full-table SCAN,
    // because the remaining indexes are all composite with snapshot_uid leading.
    for idx in FK_CHILD_INDEXES {
        conn.execute_batch(&format!("DROP INDEX {idx};")).unwrap();
    }
    for (sql, _idx) in &predicates {
        let p = plan(conn, sql);
        assert!(
            p.contains("SCAN"),
            "expected SCAN (full scan) for `{sql}` without 035 indexes, got: {p}"
        );
    }

    // Contrast (informational): the snapshot_uid-keyed deletes always SEARCH via the
    // pre-existing composite indexes — the mechanism is specific to the bare FK child
    // column, not to snapshot_uid.
    eprintln!(
        "edges by snapshot_uid : {}",
        plan(conn, "DELETE FROM edges WHERE snapshot_uid = 'x'")
    );
}

/// TIMING with PER-PHASE, PER-TABLE attribution (§2.1): reproduces the SLOW cascade prune
/// (the operator's mechanism) by dropping the migration-035 indexes, then hand-runs the SAME
/// cascade production does and records, item-by-item, the §2.1 measurement list:
/// - **page-cache behaviour**: the connection cache is sized to the store exactly as the
///   production maintenance prune does (`maintenance_cache_kib`), and the effective
///   `cache_size` + its coverage of the store is reported, so the measurement shows the
///   unindexed cascade is dominated by the missing FK-child index, not by page-cache misses;
/// - the **two FK-RESTRICT (NO ACTION) checks** SQLite issues per deleted node —
///   `nodes.parent_node_uid` and `unresolved_edges.source_node_uid` — timed in isolation as
///   the exact enforcement SELECT, with lookups/s (the CASCADE FKs `edges.*` are not RESTRICT);
/// - **per orphan (non-CASCADE) table**: the statement's table, rows deleted, wall time, rows/s;
/// - the **`parent_snapshot_uid`** self-ref update (rows + time);
/// - the **snapshot-row delete → FK cascade** into nodes/edges (dominant when unindexed),
///   nodes deleted, cascade rows/s;
/// - **WAL-file growth** during the prune.
///
/// `#[ignore]` (slow by design: at the default size the SCAN path is minutes). Run with
/// `cargo test -p repo-graph-storage retention::tests::reproduce::measure_prune_phases \
///  -- --ignored --nocapture`. `REPRO_NODES` / `REPRO_SNAPS` / `REPRO_KEEP_INDEXES=1`
/// (measure the FIXED path instead) tune it.
#[test]
#[ignore = "diagnosis harness; run with --ignored --nocapture"]
fn measure_prune_phases() {
    let nodes: usize = std::env::var("REPRO_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8_000);
    let snaps: usize = std::env::var("REPRO_SNAPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let keep_indexes = std::env::var("REPRO_KEEP_INDEXES").is_ok();

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("measure.db");
    let wal_path = db_path.with_extension("db-wal");
    let unres = nodes / 5;

    // Build `snaps` chained snapshots; keep the last two (current + parent), prune the rest.
    let mut prunable: Vec<String> = Vec::new();
    {
        let mut storage = StorageConnection::open(&db_path).unwrap();
        add_repo(&storage, "r1");
        let mut parent: Option<String> = None;
        for k in 0..snaps {
            let uid = build_snapshot(&mut storage, "r1", parent.as_deref(), nodes, unres);
            if k < snaps.saturating_sub(2) {
                prunable.push(uid.clone());
            }
            parent = Some(uid);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    } // drop → WAL checkpoint folds data into the main file
    let file_mb = std::fs::metadata(&db_path).unwrap().len() as f64 / (1024.0 * 1024.0);

    let storage = StorageConnection::open(&db_path).unwrap();
    let conn = storage.connection();
    if !keep_indexes {
        // Reproduce the operator's SLOW mechanism: without the 035 single-column FK-child
        // indexes, each per-node FK enforcement is a full child-table SCAN.
        for idx in FK_CHILD_INDEXES {
            conn.execute_batch(&format!("DROP INDEX IF EXISTS {idx};"))
                .unwrap();
        }
    }

    eprintln!("── fixture ──");
    eprintln!(
        "snapshots={snaps} nodes/snap={nodes} edges/snap={} unres/snap={unres} file={file_mb:.1} MB",
        nodes * 2
    );
    eprintln!(
        "035 FK-child indexes: {}",
        if keep_indexes {
            "PRESENT (fixed path)"
        } else {
            "DROPPED (reproduced slow path)"
        }
    );
    eprintln!("prunable snapshots = {}", prunable.len());

    // ── §2.1 "page-cache behaviour": size the cache to the store exactly as the production
    // maintenance prune does, and record the effective setting + coverage. A store-covering
    // cache that STILL leaves the unindexed cascade dominant isolates the cost to the missing
    // FK-child index (the §6 mechanism), not to page-cache misses. ──
    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap();
    let store_bytes = (page_count.max(0) as i128) * (page_size.max(0) as i128);
    let cache_kib = (store_bytes / 1024).clamp(2_000, 512 * 1024) as i64;
    conn.execute_batch(&format!("PRAGMA cache_size = -{cache_kib};"))
        .unwrap();
    let eff_cache: i64 = conn
        .query_row("PRAGMA cache_size", [], |r| r.get(0))
        .unwrap();
    let cache_bytes = if eff_cache < 0 {
        (-eff_cache as i128) * 1024
    } else {
        (eff_cache as i128) * (page_size.max(0) as i128)
    };
    eprintln!("── §2.1 page-cache behaviour ──");
    eprintln!(
        "store = {page_count} pages × {page_size} B = {:.1} MB",
        store_bytes as f64 / (1024.0 * 1024.0)
    );
    eprintln!(
        "effective cache_size = {eff_cache} (neg = KiB) = {:.1} MB → coverage {:.0}% of the store \
         (production sizes the same, clamped to 512 MiB on a multi-GB store)",
        cache_bytes as f64 / (1024.0 * 1024.0),
        (cache_bytes as f64 / (store_bytes.max(1) as f64)) * 100.0
    );

    // ── §2.1 "FK RESTRICT checks on non-cascade children": the TWO NO-ACTION FKs to `nodes`
    // (`nodes.parent_node_uid`, `unresolved_edges.source_node_uid`) SQLite must verify per
    // deleted node while cascading the snapshot delete. Timed in isolation as the exact
    // enforcement SELECT on a sample of the first prunable snapshot's nodes (read-only). The
    // `edges.*` FKs are ON DELETE CASCADE, not RESTRICT, so they are covered by phase C below. ──
    const RESTRICT_SAMPLE: usize = 50;
    let sample: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT node_uid FROM nodes WHERE snapshot_uid = ?1 LIMIT ?2")
            .unwrap();
        stmt.query_map(
            rusqlite::params![prunable[0], RESTRICT_SAMPLE as i64],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    };
    let time_restrict_check = |sql: &str| -> Duration {
        let mut stmt = conn.prepare(sql).unwrap();
        let t = Instant::now();
        for nid in &sample {
            // `.next()` drives the enforcement lookup: full child-table SCAN unindexed.
            let mut rows = stmt.query(rusqlite::params![nid]).unwrap();
            let _ = rows.next().unwrap();
        }
        t.elapsed()
    };
    let parent_check = time_restrict_check("SELECT 1 FROM nodes WHERE parent_node_uid = ?1");
    let unres_check =
        time_restrict_check("SELECT 1 FROM unresolved_edges WHERE source_node_uid = ?1");
    let per_lookup = |d: Duration| d.as_secs_f64() / sample.len().max(1) as f64;
    eprintln!(
        "── §2.1 FK-RESTRICT (NO ACTION) checks, isolated over {} sample node(s) ──",
        sample.len()
    );
    eprintln!(
        "nodes.parent_node_uid check            : {:.4}s total, {:.1} µs/lookup, {:.0} lookups/s",
        parent_check.as_secs_f64(),
        per_lookup(parent_check) * 1e6,
        1.0 / per_lookup(parent_check).max(1e-9)
    );
    eprintln!(
        "unresolved_edges.source_node_uid check : {:.4}s total, {:.1} µs/lookup, {:.0} lookups/s",
        unres_check.as_secs_f64(),
        per_lookup(unres_check) * 1e6,
        1.0 / per_lookup(unres_check).max(1e-9)
    );

    let orphan_tables = [
        "unresolved_edges",
        "boundary_provider_facts",
        "boundary_consumer_facts",
        "boundary_links",
        "boundary_interaction_surfaces",
        "boundary_interaction_links",
    ];

    let wal_before = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
    // §2.1 "which table / rows/s": per-orphan-table (summed duration, summed rows deleted).
    let mut per_table: BTreeMap<&str, (Duration, i64)> = BTreeMap::new();
    let mut phase_parent = Duration::ZERO;
    let mut parent_rows: i64 = 0;
    let mut phase_snapshot = Duration::ZERO;
    let mut wal_peak = wal_before;

    for uid in &prunable {
        let tx = conn.unchecked_transaction().unwrap();

        // Phase A — orphan (non-CASCADE) tables, deleted by snapshot_uid, timed PER TABLE.
        for tbl in orphan_tables {
            let t = Instant::now();
            let n = tx
                .execute(
                    &format!("DELETE FROM {tbl} WHERE snapshot_uid = ?1"),
                    rusqlite::params![uid],
                )
                .unwrap();
            let e = per_table.entry(tbl).or_default();
            e.0 += t.elapsed();
            e.1 += n as i64;
        }

        // Phase B — clear self-referencing parent_snapshot_uid links.
        let t = Instant::now();
        parent_rows += tx
            .execute(
                "UPDATE snapshots SET parent_snapshot_uid = NULL WHERE parent_snapshot_uid = ?1",
                rusqlite::params![uid],
            )
            .unwrap() as i64;
        phase_parent += t.elapsed();

        // Phase C — the snapshot-row delete: this triggers the FK CASCADE into `nodes`,
        // and per deleted node the FK enforcement on edges/unresolved_edges/nodes.parent
        // (the dominant phase when the 035 indexes are absent).
        let t = Instant::now();
        tx.execute(
            "DELETE FROM snapshots WHERE snapshot_uid = ?1",
            rusqlite::params![uid],
        )
        .unwrap();
        phase_snapshot += t.elapsed();

        tx.commit().unwrap();
        if let Ok(m) = std::fs::metadata(&wal_path) {
            wal_peak = wal_peak.max(m.len());
        }
    }

    let phase_orphans: Duration = per_table.values().map(|(d, _)| *d).sum();
    let total = phase_orphans + phase_parent + phase_snapshot;
    let deleted_nodes = (prunable.len() * nodes) as f64;
    eprintln!(
        "── CURRENT cascade prune, per phase (summed over {} snapshot(s)) ──",
        prunable.len()
    );
    eprintln!("A orphan-table deletes (by snapshot_uid), PER TABLE:");
    for (tbl, (d, rows)) in &per_table {
        eprintln!(
            "    {tbl:<32} : {:.3}s, {rows} rows, {:.0} rows/s",
            d.as_secs_f64(),
            *rows as f64 / d.as_secs_f64().max(1e-9)
        );
    }
    eprintln!(
        "  A subtotal                             : {:.3}s",
        phase_orphans.as_secs_f64()
    );
    eprintln!(
        "B parent_snapshot_uid update             : {:.3}s ({parent_rows} rows)",
        phase_parent.as_secs_f64()
    );
    eprintln!(
        "C snapshot delete → FK cascade to nodes  : {:.3}s  <-- dominant when unindexed",
        phase_snapshot.as_secs_f64()
    );
    eprintln!(
        "TOTAL prune                              : {:.3}s ({:.3}s/snapshot)",
        total.as_secs_f64(),
        total.as_secs_f64() / prunable.len().max(1) as f64
    );
    eprintln!(
        "cascade rows/s (nodes deleted / phase C) : {:.0} nodes/s",
        deleted_nodes / phase_snapshot.as_secs_f64().max(1e-9)
    );
    eprintln!(
        "WAL growth during prune                  : {:.1} MB (peak {:.1} MB)",
        (wal_peak.saturating_sub(wal_before)) as f64 / (1024.0 * 1024.0),
        wal_peak as f64 / (1024.0 * 1024.0)
    );
}

/// Measure candidate FIXES on independent copies of ONE built fixture, so each
/// variant sees the same starting state. Prints per-variant seconds.
#[test]
#[ignore = "diagnosis harness; run with --ignored --nocapture"]
fn measure_fix_variants() {
    let nodes: usize = std::env::var("REPRO_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8_000);
    let snaps: usize = std::env::var("REPRO_SNAPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let unres = nodes / 5;

    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.db");
    let mut prunable: Vec<String> = Vec::new();
    {
        let mut storage = StorageConnection::open(&base_path).unwrap();
        add_repo(&storage, "r1");
        let mut parent: Option<String> = None;
        for k in 0..snaps {
            let uid = build_snapshot(&mut storage, "r1", parent.as_deref(), nodes, unres);
            if k < snaps - 2 {
                prunable.push(uid.clone()); // keep the last two (current + parent)
            }
            parent = Some(uid);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let file_mb = std::fs::metadata(&base_path).unwrap().len() as f64 / (1024.0 * 1024.0);
    eprintln!("── fixture: snaps={snaps} nodes/snap={nodes} edges/snap={} unres/snap={unres} file={file_mb:.1} MB ──",
        nodes * 2);
    eprintln!("prunable snapshots = {}", prunable.len());

    // Copy the base file so each variant starts fresh.
    let copy_fixture = |name: &str| -> std::path::PathBuf {
        let p = dir.path().join(name);
        std::fs::copy(&base_path, &p).unwrap();
        p
    };

    // Node-referencing tables to clear by snapshot_uid (FK OFF) before deleting nodes.
    let node_ref_tables = [
        "edges",
        "unresolved_edges",
        "extraction_edges",
        "staged_edges",
        "symbol_call_degrees",
        "resolved_call_file_pairs",
    ];

    // V2: FK OFF on the maintenance connection; delete node-ref tables + nodes + snapshot by snapshot_uid.
    {
        let p = copy_fixture("v2.db");
        let storage = StorageConnection::open(&p).unwrap();
        storage
            .connection()
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .unwrap();
        let t = Instant::now();
        for uid in &prunable {
            let conn = storage.connection();
            let tx = conn.unchecked_transaction().unwrap();
            for tbl in &node_ref_tables {
                tx.execute(
                    &format!("DELETE FROM {tbl} WHERE snapshot_uid = ?1"),
                    rusqlite::params![uid],
                )
                .unwrap();
            }
            tx.execute(
                "DELETE FROM nodes WHERE snapshot_uid = ?1",
                rusqlite::params![uid],
            )
            .unwrap();
            tx.execute(
                "DELETE FROM snapshots WHERE snapshot_uid = ?1",
                rusqlite::params![uid],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        eprintln!(
            "V2 FK=OFF explicit deletes by snapshot_uid : {:.2}s",
            t.elapsed().as_secs_f64()
        );
    }

    // V3: FK ON, but add single-column FK indexes first, then the CURRENT cascade prune.
    {
        let p = copy_fixture("v3.db");
        let storage = StorageConnection::open(&p).unwrap();
        let ti = Instant::now();
        storage
            .connection()
            .execute_batch(
                "CREATE INDEX IF NOT EXISTS tmp_edges_src ON edges(source_node_uid);\
             CREATE INDEX IF NOT EXISTS tmp_edges_dst ON edges(target_node_uid);\
             CREATE INDEX IF NOT EXISTS tmp_unres_src ON unresolved_edges(source_node_uid);\
             CREATE INDEX IF NOT EXISTS tmp_nodes_parent ON nodes(parent_node_uid);",
            )
            .unwrap();
        let index_secs = ti.elapsed().as_secs_f64();
        // Mark the intended prunable snapshots prunable so the cascade runs on them.
        for uid in &prunable {
            storage
                .connection()
                .execute(
                    "UPDATE snapshots SET retention_class = 'prunable' WHERE snapshot_uid = ?1",
                    rusqlite::params![uid],
                )
                .unwrap();
        }
        let t = Instant::now();
        let pruned = storage.prune_prunable_snapshots("r1").unwrap();
        eprintln!("V3 FK=ON  + single-col FK indexes: build idx {index_secs:.2}s, cascade prune {:.2}s (pruned {pruned})",
            t.elapsed().as_secs_f64());
    }
}

/// RETENTION BENCHMARK GATE (§2.4) — a permanent (non-ignored) test that fails if the
/// prune ever regresses to the O(nodes × child_rows) cascade.
///
/// It builds an N-snapshot × M-row fixture on the SHIPPED schema (migration 035 indexes
/// present), classifies, and prunes via the real `prune_prunable_snapshots`, asserting:
///
/// 1. the plan for each FK-child predicate is SEARCH, not SCAN — the DETERMINISTIC,
///    machine-independent regression guard (the mechanism itself); and
/// 2. the prune completes under a fixed wall-clock bound — the §2.4 literal requirement, a
///    coarse secondary guard set with wide margin over the measured indexed path so it
///    never false-fails on the fast path but a scan regression (seconds→minutes at this
///    size) blows it.
///
/// Sized to run in the default suite in ~1–2 s while a scan regression at the same size is
/// tens of seconds. In-memory: the mechanism is CPU-bound (scanning cached child rows), so
/// it reproduces without disk, and the fixture always carries the migrated schema.
#[test]
fn retention_prune_benchmark_gate() {
    // Fixed fixture (the gate's "N snapshots × M rows").
    const SNAPS: usize = 5;
    const NODES: usize = 1_200;
    const UNRES: usize = NODES / 5;
    // Wide-margin wall-clock bound. Indexed prune of 3 snapshots at this size is well under
    // a second in debug; a scan regression is tens of seconds. 10 s tolerates a slow CI host
    // on the fast path without admitting the regression.
    const BOUND: std::time::Duration = std::time::Duration::from_secs(10);

    let mut storage = StorageConnection::open_in_memory().unwrap();
    add_repo(&storage, "r1");
    let mut parent: Option<String> = None;
    for _ in 0..SNAPS {
        let uid = build_snapshot(&mut storage, "r1", parent.as_deref(), NODES, UNRES);
        parent = Some(uid);
    }

    // Deterministic mechanism guard: the shipped schema must SEARCH, not SCAN.
    {
        let conn = storage.connection();
        for sql in [
            "DELETE FROM edges WHERE source_node_uid = 'x'",
            "DELETE FROM edges WHERE target_node_uid = 'x'",
            "SELECT 1 FROM unresolved_edges WHERE source_node_uid = 'x'",
            "SELECT 1 FROM nodes WHERE parent_node_uid = 'x'",
        ] {
            let p = plan(conn, sql);
            assert!(
                p.contains("SEARCH") && !p.contains("SCAN"),
                "retention prune regressed to a full SCAN for `{sql}`: {p}"
            );
        }
    }

    // classify marks all but current+parent prunable; prune under the bound.
    storage.classify_repo_retention("r1").unwrap();
    let stats = storage.get_retention_stats("r1").unwrap();
    assert!(stats.prunable >= 1, "fixture must have prunable snapshots");

    let t = Instant::now();
    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    let elapsed = t.elapsed();
    assert_eq!(
        pruned, stats.prunable,
        "all prunable snapshots must be pruned"
    );
    assert!(
        elapsed < BOUND,
        "retention prune of {pruned} snapshot(s) ({NODES} nodes each) took {:.2}s, over the {}s gate bound — \
         the O(nodes × child_rows) cascade has likely regressed",
        elapsed.as_secs_f64(),
        BOUND.as_secs()
    );
    eprintln!(
        "retention gate: pruned {pruned} snapshot(s) of {NODES} nodes in {:.3}s (bound {}s)",
        elapsed.as_secs_f64(),
        BOUND.as_secs()
    );
}

/// DR-1 INSERT-PATH COST (§6, human obligation "the insert-path cost is measured"):
/// times building the SAME fixture WITH the migration-035 indexes present vs a variant
/// with them dropped before inserting, and reports the wall-time delta and the store-size
/// delta the four indexes add. `#[ignore]` — a measurement, not a gate. Run with
/// `-- --ignored --nocapture`. The authoritative synthetic number; the build report also
/// carries one isolated real repo-graph index before/after.
#[test]
#[ignore = "insert-path cost measurement; run with --ignored --nocapture"]
fn measure_insert_path_index_cost() {
    let nodes: usize = std::env::var("REPRO_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8_000);
    let snaps: usize = std::env::var("REPRO_SNAPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let unres = nodes / 5;

    let build = |with_indexes: bool| -> (f64, u64) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("insert.db");
        let mut storage = StorageConnection::open(&db_path).unwrap();
        if !with_indexes {
            for idx in FK_CHILD_INDEXES {
                storage
                    .connection()
                    .execute_batch(&format!("DROP INDEX IF EXISTS {idx};"))
                    .unwrap();
            }
        }
        add_repo(&storage, "r1");
        let t = Instant::now();
        let mut parent: Option<String> = None;
        for _ in 0..snaps {
            let uid = build_snapshot(&mut storage, "r1", parent.as_deref(), nodes, unres);
            parent = Some(uid);
        }
        let secs = t.elapsed().as_secs_f64();
        drop(storage); // checkpoint WAL into the main file before sizing
        let bytes = std::fs::metadata(&db_path).unwrap().len();
        (secs, bytes)
    };

    let (with_secs, with_bytes) = build(true);
    let (without_secs, without_bytes) = build(false);
    let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
    eprintln!("── insert-path cost of the four 035 FK-child indexes ──");
    eprintln!(
        "fixture: snaps={snaps} nodes/snap={nodes} edges/snap={} unres/snap={unres}",
        nodes * 2
    );
    eprintln!(
        "insert WITH indexes    : {with_secs:.3}s, file {:.1} MB",
        mb(with_bytes)
    );
    eprintln!(
        "insert WITHOUT indexes : {without_secs:.3}s, file {:.1} MB",
        mb(without_bytes)
    );
    eprintln!(
        "insert-path delta      : {:+.3}s ({:+.1}%)",
        with_secs - without_secs,
        (with_secs - without_secs) / without_secs.max(1e-9) * 100.0
    );
    eprintln!(
        "store-size delta       : {:+.1} MB ({:+.1}%)",
        mb(with_bytes) - mb(without_bytes),
        (with_bytes as f64 - without_bytes as f64) / (without_bytes.max(1) as f64) * 100.0
    );
}
