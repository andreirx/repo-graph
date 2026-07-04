//! Pruning operation tests.
//!
//! Tests for prune_prunable_snapshots() which deletes snapshots
//! marked as prunable.

use super::{insert_current_epoch_snapshot, insert_repo, setup_storage};

/// Insert a snapshot with an explicit status (DAEMON-VISIBILITY-1 F3 needs non-READY rows).
fn insert_snapshot_with_status(
    storage: &super::StorageConnection,
    snapshot_uid: &str,
    repo_uid: &str,
    status: &str,
    created_at: &str,
) {
    storage
        .connection()
        .execute(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) \
             VALUES (?1, ?2, 'full', ?3, ?4)",
            rusqlite::params![snapshot_uid, repo_uid, status, created_at],
        )
        .unwrap();
}

// DAEMON-VISIBILITY-1 (F3, operator Option A): `prune_non_ready_snapshots` deletes every NON-READY
// (interrupted/failed) snapshot and returns their UIDs, while leaving READY snapshots untouched — the
// storage half of the interrupted-snapshot reclaim. `vacuum()` then runs clean.
#[test]
fn prune_non_ready_deletes_only_non_ready_then_vacuum_ok() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_snapshot_with_status(&storage, "ready1", "r1", "ready", "2025-01-01T00:00:00Z");
    insert_snapshot_with_status(
        &storage,
        "building1",
        "r1",
        "building",
        "2025-01-02T00:00:00Z",
    );
    insert_snapshot_with_status(&storage, "failed1", "r1", "failed", "2025-01-03T00:00:00Z");

    let deleted = storage.prune_non_ready_snapshots("r1").unwrap();
    assert_eq!(
        deleted.len(),
        2,
        "both non-READY snapshots deleted: {deleted:?}"
    );
    assert!(deleted.contains(&"building1".to_string()));
    assert!(deleted.contains(&"failed1".to_string()));

    // The READY snapshot is untouched; the non-READY rows are gone.
    let remaining = storage.list_snapshots("r1").unwrap();
    assert_eq!(remaining.len(), 1, "only the READY snapshot survives");
    assert_eq!(remaining[0].snapshot_uid, "ready1");
    assert_eq!(remaining[0].status, "ready");

    // VACUUM (the reclaim step) runs without error. On-disk byte reclaim is proven at the daemon
    // integration level; here the connection is in-memory, so we assert only that it succeeds.
    storage.vacuum().unwrap();

    // Idempotent: a second prune finds nothing non-READY to delete.
    assert!(storage.prune_non_ready_snapshots("r1").unwrap().is_empty());
}

// DAEMON-VISIBILITY-1 (F3, operator Option A): the on-disk reclaim proof — deleting a bloated
// non-READY snapshot + `vacuum()` actually SHRINKS the DB file (rows gone AND disk returned to the OS),
// while the READY snapshot's data survives. File-backed (in-memory has no meaningful file size).
#[test]
fn prune_non_ready_then_vacuum_reclaims_disk_and_keeps_ready() {
    use crate::connection::StorageConnection;
    use crate::types::{CreateSnapshotInput, GraphNode};

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("reclaim.db");

    // Repo + a READY snapshot holding a couple of real nodes + an interrupted (building) snapshot
    // holding ~8k bloat nodes. FK cascade (foreign_keys=ON at open) removes a snapshot's nodes when
    // the snapshot row is deleted, so the bulk goes with the building snapshot.
    let ready_uid;
    let building_uid;
    {
        let mut storage = StorageConnection::open(&db_path).unwrap();
        insert_repo(&storage, "r1");
        let ready = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap();
        ready_uid = ready.snapshot_uid.clone();
        storage
            .update_snapshot_status(&crate::types::UpdateSnapshotStatusInput {
                snapshot_uid: ready_uid.clone(),
                status: "ready".to_string(),
                completed_at: None,
            })
            .unwrap();
        storage
            .insert_nodes(&[
                bloat_node(&ready_uid, "keep-0"),
                bloat_node(&ready_uid, "keep-1"),
            ])
            .unwrap();

        let building = storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap();
        building_uid = building.snapshot_uid.clone();
        let bloat: Vec<GraphNode> = (0..8_000)
            .map(|i| bloat_node(&building_uid, &format!("bloat-{i}")))
            .collect();
        storage.insert_nodes(&bloat).unwrap();
        // Drop → last-connection WAL checkpoint moves the bloat into the main DB file.
    }

    let size_before = std::fs::metadata(&db_path).unwrap().len();

    let (deleted, size_after) = {
        let storage = StorageConnection::open(&db_path).unwrap();
        let deleted = storage.prune_non_ready_snapshots("r1").unwrap();
        storage.vacuum().unwrap();
        (deleted, std::fs::metadata(&db_path).unwrap().len())
    };

    assert_eq!(
        deleted,
        vec![building_uid],
        "only the interrupted snapshot was deleted"
    );
    assert!(
        size_after < size_before,
        "disk was reclaimed: before={size_before} after={size_after}"
    );

    // The READY snapshot survives with its nodes (FK cascade only removed the deleted snapshot's rows).
    let storage = StorageConnection::open(&db_path).unwrap();
    let snaps = storage.list_snapshots("r1").unwrap();
    assert_eq!(snaps.len(), 1, "only READY remains: {snaps:?}");
    assert_eq!(snaps[0].snapshot_uid, ready_uid);
    assert_eq!(snaps[0].status, "ready");
}

/// A padded SYMBOL node — the text fields grow the row so a few thousand of them make a measurable
/// file-size difference for the reclaim assertion.
#[cfg(test)]
fn bloat_node(snapshot_uid: &str, node_uid: &str) -> crate::types::GraphNode {
    crate::types::GraphNode {
        node_uid: node_uid.to_string(),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: "r1".to_string(),
        stable_key: format!("r1:{node_uid}:SYMBOL"),
        kind: "SYMBOL".to_string(),
        subtype: Some("FUNCTION".to_string()),
        name: node_uid.to_string(),
        qualified_name: Some(format!("bloated::module::path::to::{node_uid}")),
        file_uid: None,
        parent_node_uid: None,
        location: None,
        signature: Some("fn bloat(a: usize, b: usize, c: usize) -> usize".to_string()),
        visibility: Some("export".to_string()),
        doc_comment: Some(
            "a padded doc comment to grow the row size for a measurable reclaim".to_string(),
        ),
        metadata_json: None,
    }
}

#[test]
fn prune_prunable_snapshots_deletes_marked() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // Create independent snapshots (no parent relationships)
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s4", "r1", None, "2025-01-04T00:00:00Z");

    storage.classify_repo_retention("r1").unwrap();

    // Before prune: s1, s2 are prunable (s4=current, s3=baseline_auto, no parent)
    let stats_before = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_before.prunable, 2);
    assert_eq!(stats_before.total, 4);

    // Prune
    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned, 2);

    // After prune
    let stats_after = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_after.prunable, 0);
    assert_eq!(stats_after.total, 2);
}
