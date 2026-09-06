//! DAEMON-RESIDUALS-2 — retention prune reproduction & per-phase measurement.
//!
//! These are `#[ignore]`d measurement harnesses (run with `--ignored --nocapture`),
//! NOT part of the default gate. They exist to DIAGNOSE the "5h+, zero committed
//! progress" prune the operator measured on a 4.8 GB / 29-snapshot store, on a
//! controlled snapshot-heavy synthetic fixture (the spec's alternative to 6–8 real
//! re-indexes) — the SMALLEST input that reproduces the mechanism deterministically.
//!
//! What they prove:
//! - `explain_fk_child_delete_plan_is_a_full_scan`: the DETERMINISTIC ground truth.
//!   The single-column FK-enforcement lookups SQLite issues while cascading a
//!   `nodes` delete (`edges.source_node_uid`, `edges.target_node_uid`,
//!   `unresolved_edges.source_node_uid` → `nodes`) resolve to a table SCAN, because
//!   every index on those tables is COMPOSITE with `snapshot_uid` leading and cannot
//!   serve a bare child-column predicate. This is size-independent.
//! - `measure_prune_phases`: times the CURRENT `prune_prunable_snapshots` cascade vs a
//!   snapshot_uid-keyed pre-delete of the node-referencing tables, on a file-backed
//!   fixture, so the per-phase cost is attributable.

use std::time::Instant;

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

/// DETERMINISTIC (size-independent): show the query plan SQLite uses for the
/// single-column FK-enforcement lookups triggered when a `nodes` row is deleted.
/// If these SCAN, every parent-row deletion pays a full child-table scan — the
/// O(nodes × child_rows) mechanism.
#[test]
#[ignore = "diagnosis harness; run with --ignored --nocapture"]
fn explain_fk_child_delete_plan_is_a_full_scan() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("plan.db");
    let mut storage = StorageConnection::open(&db_path).unwrap();
    add_repo(&storage, "r1");
    // A little data so the planner has stats; the plan is what matters, not the size.
    build_snapshot(&mut storage, "r1", None, 200, 40);
    let conn = storage.connection();

    // What FK CASCADE on edges.{source,target}_node_uid → nodes does per deleted node.
    let edges_src = plan(conn, "DELETE FROM edges WHERE source_node_uid = 'x'");
    let edges_dst = plan(conn, "DELETE FROM edges WHERE target_node_uid = 'x'");
    // What FK RESTRICT (NO ACTION) on unresolved_edges.source_node_uid → nodes does.
    let unres_src = plan(
        conn,
        "SELECT 1 FROM unresolved_edges WHERE source_node_uid = 'x'",
    );
    // The self-ref NO-ACTION check on nodes.parent_node_uid → nodes.
    let nodes_parent = plan(conn, "SELECT 1 FROM nodes WHERE parent_node_uid = 'x'");
    // For contrast: the snapshot_uid-keyed delete the fix uses (should SEARCH via index).
    let edges_by_snap = plan(conn, "DELETE FROM edges WHERE snapshot_uid = 'x'");
    let unres_by_snap = plan(
        conn,
        "DELETE FROM unresolved_edges WHERE snapshot_uid = 'x'",
    );

    eprintln!("── FK-enforcement lookup plans (single child column) ──");
    eprintln!("edges.source_node_uid : {edges_src}");
    eprintln!("edges.target_node_uid : {edges_dst}");
    eprintln!("unresolved.source_node: {unres_src}");
    eprintln!("nodes.parent_node_uid : {nodes_parent}");
    eprintln!("── snapshot_uid-keyed deletes (the fix's access path) ──");
    eprintln!("edges by snapshot_uid : {edges_by_snap}");
    eprintln!("unres by snapshot_uid : {unres_by_snap}");
}

/// TIMING: current cascade prune vs a snapshot_uid-keyed pre-delete, on a file-backed
/// snapshot-heavy fixture. Prints per-phase seconds so the dominant phase is named.
#[test]
#[ignore = "diagnosis harness; run with --ignored --nocapture"]
fn measure_prune_phases() {
    // Tunable via env so the harness can be scaled up toward the operator's store.
    let nodes: usize = std::env::var("REPRO_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8_000);
    let snaps: usize = std::env::var("REPRO_SNAPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let cache_kib: i64 = std::env::var("REPRO_CACHE_KIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("measure.db");
    let unres = nodes / 5;

    // Build `snaps` chained snapshots; keep current+parent, prune the rest.
    let mut prunable_uids: Vec<String> = Vec::new();
    {
        let mut storage = StorageConnection::open(&db_path).unwrap();
        add_repo(&storage, "r1");
        let mut parent: Option<String> = None;
        for _ in 0..snaps {
            let uid = build_snapshot(&mut storage, "r1", parent.as_deref(), nodes, unres);
            prunable_uids.push(uid.clone());
            parent = Some(uid);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    } // drop → WAL checkpoint folds data into the main file
    let file_mb = std::fs::metadata(&db_path).unwrap().len() as f64 / (1024.0 * 1024.0);

    let storage = StorageConnection::open(&db_path).unwrap();
    if cache_kib != 0 {
        storage
            .connection()
            .execute_batch(&format!("PRAGMA cache_size = -{cache_kib};"))
            .unwrap();
    }
    storage.classify_repo_retention("r1").unwrap();
    let stats = storage.get_retention_stats("r1").unwrap();

    eprintln!("── fixture ──");
    eprintln!("snapshots={snaps} nodes/snap={nodes} edges/snap={} unres/snap={unres} file={file_mb:.1} MB",
        nodes * 2);
    eprintln!(
        "cache_size override = {} (0 = default 2MB)",
        if cache_kib == 0 {
            "default".to_string()
        } else {
            format!("{cache_kib} KiB")
        }
    );
    eprintln!("prunable snapshots = {}", stats.prunable);

    let t = Instant::now();
    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    let elapsed = t.elapsed();
    eprintln!("── CURRENT prune_prunable_snapshots ──");
    eprintln!(
        "pruned {pruned} snapshot(s) in {:.2}s ({:.3}s/snapshot)",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() / pruned.max(1) as f64
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
