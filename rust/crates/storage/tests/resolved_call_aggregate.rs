//! EC-M3B-TRUST-AGG-1 — pre-migration fallback + core-level byte identity.
//!
//! A snapshot built through public CRUD without ever calling
//! `persist_resolved_call_aggregate` IS the pre-migration shape: migration
//! 030 leaves `resolved_call_count`/`resolved_call_provenance` NULL on
//! existing rows, and `create_snapshot` never writes them. This suite
//! proves, against the REAL SQLite adapter and the REAL trust core:
//!
//! 1. **Fallback:** with no persisted aggregate and CALLS rows present,
//!    `resolved_calls` is served from the live COUNT — the exact pre-M-3b
//!    accounting; never a fabricated 0.
//! 2. **Byte identity while parity holds:** the assembled `TrustReport`
//!    serializes to byte-identical JSON whether `resolved_calls` came from
//!    the fallback or from the persisted aggregate. The five consuming
//!    surfaces (trust, check, orient, explain, stats) are projections of
//!    this one core output (`assemble_trust_report`/`get_trust_summary`),
//!    so core-level byte identity is the load-bearing half of the slice's
//!    surface byte-compare.

use repo_graph_storage::types::{CreateSnapshotInput, GraphEdge, GraphNode, Repo};
use repo_graph_storage::StorageConnection;
use repo_graph_trust::{assemble_trust_report, TrustStorageRead};

fn node(node_uid: &str, snap: &str, stable_key: &str, name: &str) -> GraphNode {
    GraphNode {
        node_uid: node_uid.to_string(),
        snapshot_uid: snap.to_string(),
        repo_uid: "r1".to_string(),
        stable_key: stable_key.to_string(),
        kind: "SYMBOL".to_string(),
        subtype: Some("FUNCTION".to_string()),
        name: name.to_string(),
        qualified_name: Some(name.to_string()),
        file_uid: None,
        parent_node_uid: None,
        location: None,
        signature: None,
        visibility: Some("export".to_string()),
        doc_comment: None,
        metadata_json: None,
    }
}

fn calls_edge(edge_uid: &str, snap: &str, source: &str, target: &str) -> GraphEdge {
    GraphEdge {
        edge_uid: edge_uid.to_string(),
        snapshot_uid: snap.to_string(),
        repo_uid: "r1".to_string(),
        source_node_uid: source.to_string(),
        target_node_uid: target.to_string(),
        edge_type: "CALLS".to_string(),
        resolution: "static".to_string(),
        extractor: "test:0.0.1".to_string(),
        location: None,
        metadata_json: None,
    }
}

/// Repo + snapshot + two nodes + two CALLS edges, built through public
/// CRUD only — the aggregate columns stay NULL (pre-migration shape).
fn seed_pre_migration_snapshot() -> (StorageConnection, String) {
    let mut storage = StorageConnection::open_in_memory().unwrap();
    storage
        .add_repo(&Repo {
            repo_uid: "r1".to_string(),
            name: "repo".to_string(),
            root_path: "/tmp/r1".to_string(),
            default_branch: None,
            created_at: "2026-07-17T00:00:00Z".to_string(),
            metadata_json: None,
        })
        .unwrap();
    let snap = storage
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: "r1".to_string(),
            parent_snapshot_uid: None,
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            label: None,
            toolchain_json: None,
        })
        .unwrap()
        .snapshot_uid;

    storage
        .insert_nodes(&[
            node("n-a", &snap, "r1:src/a.ts#fnA:SYMBOL:FUNCTION", "fnA"),
            node("n-b", &snap, "r1:src/a.ts#fnB:SYMBOL:FUNCTION", "fnB"),
        ])
        .unwrap();
    storage
        .insert_edges(&[
            calls_edge("e-1", &snap, "n-a", "n-b"),
            calls_edge("e-2", &snap, "n-b", "n-a"),
        ])
        .unwrap();

    (storage, snap)
}

#[test]
fn pre_migration_snapshot_serves_live_count_never_zero() {
    let (storage, snap) = seed_pre_migration_snapshot();

    // Pre-condition: this snapshot carries NO persisted aggregate…
    assert_eq!(
        TrustStorageRead::get_resolved_call_aggregate(&storage, &snap).unwrap(),
        None,
        "CRUD-built snapshot models the pre-migration shape"
    );

    // …yet the trust core serves the live CALLS count, not a fabricated 0.
    let report = assemble_trust_report(&storage, "r1", &snap, None, None).unwrap();
    assert_eq!(
        report.summary.resolved_calls, 2,
        "fallback must serve the live COUNT while CALLS rows exist"
    );
}

#[test]
fn report_bytes_identical_across_fallback_and_persisted_sources() {
    let (storage, snap) = seed_pre_migration_snapshot();

    // Source 1: fallback (no persisted aggregate).
    let report_fallback = assemble_trust_report(&storage, "r1", &snap, None, None).unwrap();

    // Persist the aggregate through the ONE production writer — the
    // pipeline supplies the stream-side count (2, matching the two CALLS
    // rows so the parity window holds) — then read again — source 2: the
    // persisted aggregate.
    storage.persist_resolved_call_aggregate(&snap, 2).unwrap();
    assert!(
        TrustStorageRead::get_resolved_call_aggregate(&storage, &snap)
            .unwrap()
            .is_some(),
        "aggregate persisted for the second assembly"
    );
    let report_persisted = assemble_trust_report(&storage, "r1", &snap, None, None).unwrap();

    assert_eq!(report_persisted.summary.resolved_calls, 2);

    // Byte-compare: while parity holds, the source swap must be invisible
    // in the core output the five surfaces project from.
    let json_fallback = serde_json::to_string(&report_fallback).unwrap();
    let json_persisted = serde_json::to_string(&report_persisted).unwrap();
    assert_eq!(
        json_fallback, json_persisted,
        "TrustReport must serialize byte-identically across the two sources"
    );
}
