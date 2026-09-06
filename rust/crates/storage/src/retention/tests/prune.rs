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

// SNAPSHOT-RETENTION-1: `reclaimable_bytes` reports the freelist bytes a VACUUM would return to the
// OS (`freelist_count × page_size`) — the honest VACUUM-gate input measured WITHOUT paying the
// VACUUM. After pruning a bloated snapshot the freed pages sit on the freelist (file NOT yet shrunk)
// so reclaimable jumps; after VACUUM the file shrinks and reclaimable drops back toward zero.
#[test]
fn reclaimable_bytes_tracks_freelist_then_drops_after_vacuum() {
    use crate::connection::StorageConnection;
    use crate::types::{CreateSnapshotInput, GraphNode, UpdateSnapshotStatusInput};

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("reclaimable.db");

    let building_uid;
    {
        let mut storage = StorageConnection::open(&db_path).unwrap();
        insert_repo(&storage, "r1");
        // A READY snapshot to keep + a big interrupted one to prune.
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
        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: ready.snapshot_uid.clone(),
                status: "ready".to_string(),
                completed_at: None,
            })
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
        // Drop → last-connection WAL checkpoint folds the bloat into the main DB file.
    }

    let storage = StorageConnection::open(&db_path).unwrap();
    let before_prune = storage.reclaimable_bytes().unwrap();

    // Prune the bloated non-READY snapshot: its rows move to the freelist; the file is NOT yet shrunk.
    let deleted = storage.prune_non_ready_snapshots("r1").unwrap();
    assert_eq!(deleted.len(), 1);
    let after_prune = storage.reclaimable_bytes().unwrap();
    assert!(
        after_prune > before_prune && after_prune > 64 * 1024,
        "pruning frees pages onto the freelist: before={before_prune} after={after_prune}"
    );

    // VACUUM returns those pages to the OS → reclaimable drops back toward zero.
    storage.vacuum().unwrap();
    let after_vacuum = storage.reclaimable_bytes().unwrap();
    assert!(
        after_vacuum < after_prune,
        "vacuum realises the reclaim: after_prune={after_prune} after_vacuum={after_vacuum}"
    );
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

// DAEMON-RESIDUALS-2C §7 (reporting-only budget): with NO deadline the budgeted prune runs to
// completion (identical to the unbudgeted path); with an ALREADY-PASSED deadline it stops at the very
// first chunk boundary, pruning nothing and REPORTING every prunable snapshot as remaining — the store
// is untouched (per-snapshot atomicity: never a half-deleted snapshot) and a later unbudgeted pass
// finishes the job (re-run safe).
#[test]
fn budgeted_prune_none_completes_but_expired_deadline_aborts_and_reports_remaining() {
    use std::time::{Duration, Instant};

    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s4", "r1", None, "2025-01-04T00:00:00Z");
    storage.classify_repo_retention("r1").unwrap();
    assert_eq!(storage.get_retention_stats("r1").unwrap().prunable, 3);

    // Expired deadline → abort at the first chunk boundary: 0 pruned, all 3 reported remaining.
    let past = Instant::now() - Duration::from_secs(1);
    let aborted = storage
        .prune_prunable_snapshots_budgeted("r1", Some(past))
        .unwrap();
    assert_eq!(
        aborted.pruned_count, 0,
        "budget stopped it before any chunk"
    );
    assert_eq!(
        aborted.budget_remaining,
        Some(3),
        "every prunable snapshot is reported as remaining"
    );
    // Nothing was corrupted: all 4 snapshots (3 prunable + current) are still present.
    assert_eq!(storage.get_retention_stats("r1").unwrap().total, 4);

    // A later unbudgeted (None) pass finishes the job — the leftover is prunable again, re-run safe.
    let done = storage
        .prune_prunable_snapshots_budgeted("r1", None)
        .unwrap();
    assert_eq!(done.pruned_count, 3);
    assert_eq!(
        done.budget_remaining, None,
        "ran to completion → no leftover"
    );
    let after = storage.get_retention_stats("r1").unwrap();
    assert_eq!(after.prunable, 0);
    assert_eq!(after.total, 1);
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

    // SNAPSHOT-RETENTION-1: independent snapshots (no parent chain) → s4=current only; s1/s2/s3 all
    // prune (auto-baseline no longer retained).
    let stats_before = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_before.prunable, 3);
    assert_eq!(stats_before.total, 4);

    // Prune
    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned, 3);

    // After prune
    let stats_after = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_after.prunable, 0);
    assert_eq!(stats_after.total, 1);
}

// DAEMON-CRASH-RECOVERY-1 (review-1, Change 2): a reconciled crash orphan is `status='failed'` +
// `retention_class='prunable'`. The READY-retention prune is guarded to `status='ready'`, so it must
// NOT delete the orphan — that is the non-READY (`prune_non_ready_snapshots` + `VACUUM`) path's job,
// and deleting it here would skip that VACUUM (the "disk never came back" field bug). `get_retention_stats`
// still COUNTS it as prunable BEFORE any reclaim (the visibility the slice requires).
#[test]
fn prune_prunable_leaves_non_ready_orphan_for_the_vacuum_path() {
    use crate::types::{CreateSnapshotInput, UpdateSnapshotStatusInput};

    let storage = setup_storage();
    insert_repo(&storage, "r1");

    let mk = |kind: &str| {
        storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                kind: kind.to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
            .snapshot_uid
    };

    // A READY current snapshot + a crash orphan reconciled to failed+prunable.
    let ready = mk("full");
    storage
        .update_snapshot_status(&UpdateSnapshotStatusInput {
            snapshot_uid: ready.clone(),
            status: "ready".to_string(),
            completed_at: None,
        })
        .unwrap();
    let orphan = mk("full");
    assert!(storage
        .mark_snapshot_interrupted(&orphan, "daemon restart")
        .unwrap());

    // Classify touches only the READY row (→ `current`); the orphan keeps its reconciliation-set class.
    storage.classify_repo_retention("r1").unwrap();
    // The orphan IS counted as prunable (visibility) BEFORE any reclaim — the reviewer's assertion.
    assert_eq!(
        storage.get_retention_stats("r1").unwrap().prunable,
        1,
        "the non-READY orphan is classified + counted prunable"
    );
    // …yet the READY-retention prune leaves it (guarded to status='ready'), so the VACUUM path keeps it.
    assert_eq!(
        storage.prune_prunable_snapshots("r1").unwrap(),
        0,
        "prune_prunable does not delete the failed orphan"
    );
    assert!(
        storage.get_snapshot(&orphan).unwrap().is_some(),
        "orphan still present for the non-READY VACUUM path"
    );

    // The non-READY path is what actually reclaims it (the daemon VACUUMs after); READY survives.
    assert_eq!(
        storage.prune_non_ready_snapshots("r1").unwrap(),
        vec![orphan],
        "the orphan is reclaimed through the non-READY path"
    );
    assert!(
        storage.get_snapshot(&ready).unwrap().is_some(),
        "the READY snapshot survives"
    );
}

// DAEMON-RESIDUALS-2 §6 (review-1, item 2): `delete_snapshots_cascade` bumps the maintenance
// connection's `PRAGMA cache_size` for the delete loop and MUST restore the caller's prior value
// afterwards — on BOTH a successful prune and a failed one — so the enlarged maintenance cache does
// not linger across the rest of the maintenance operation's work on that same connection. The
// restore is no longer swallowed (`let _ =` → propagated), so these also assert the connection is
// left in the caller's original cache state.
fn cache_size(storage: &super::StorageConnection) -> i64 {
    storage
        .connection()
        .query_row("PRAGMA cache_size", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn prune_restores_prior_cache_size_on_success() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // Independent snapshots → classify keeps only the current; the rest prune (matches
    // `prune_prunable_snapshots_deletes_marked`).
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");
    storage.classify_repo_retention("r1").unwrap();

    // A NON-default prior cache_size (7 MiB). Distinct from the maintenance value the prune sets
    // for a tiny store (the 2 MiB MIN_KIB clamp → -2000), so a missing restore is observable.
    storage
        .connection()
        .execute_batch("PRAGMA cache_size = -7000;")
        .unwrap();
    assert_eq!(cache_size(&storage), -7000);

    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    assert!(pruned >= 1, "fixture must have prunable snapshots");

    assert_eq!(
        cache_size(&storage),
        -7000,
        "cache_size must be restored to the caller's prior value after a successful prune"
    );
}

/// Insert one `unresolved_edges` row for `snapshot_uid`, sourced from `node_uid`. `unresolved_edges`
/// is one of the orphan-cleanup tables the prune deletes EXPLICITLY inside each per-snapshot chunk's
/// transaction (it has no `ON DELETE CASCADE` on `snapshot_uid`), so it is the row that proves a
/// failed chunk's in-transaction deletes are rolled back. Its `source_node_uid` FK requires the node
/// to already exist (migration_007).
fn insert_unresolved_edge(
    storage: &super::StorageConnection,
    edge_uid: &str,
    snapshot_uid: &str,
    node_uid: &str,
) {
    storage
        .connection()
        .execute(
            "INSERT INTO unresolved_edges \
             (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, resolution, \
              extractor, category, classification, classifier_version, basis_code, observed_at) \
             VALUES (?1, ?2, 'r1', ?3, 'target::key', 'CALLS', 'unresolved', 'test', \
                     'external', 'unresolved', 1, 'none', '2025-01-01T00:00:00Z')",
            rusqlite::params![edge_uid, snapshot_uid, node_uid],
        )
        .unwrap();
}

// DAEMON-RESIDUALS-2 §4 (review-2, item 2): the prune is CHUNKED one transaction per snapshot, NOT
// all-or-nothing across snapshots. This proves the ratified per-snapshot atomicity ACROSS a chunk
// boundary: when a LATER snapshot's delete fails after an earlier snapshot has already committed,
// (a) the earlier chunk STAYS deleted (the prune is not rolled back wholesale), and (b) the failing
// chunk's whole transaction — including the orphan-table (`unresolved_edges`) rows it had already
// deleted inside that same transaction — rolls back. Guards against the false all-snapshots-atomic
// contract the stale doc claimed returning unnoticed.
#[test]
fn prune_chunk_boundary_preserves_per_snapshot_atomicity_on_failure() {
    let mut storage = setup_storage();
    insert_repo(&storage, "r1");
    // Three independent prunable snapshots (+ current s4) — same fixture shape as
    // `prune_prunable_snapshots_deletes_marked`: classify → s1/s2/s3 prunable, s4 current.
    for (uid, day) in [("s1", "01"), ("s2", "02"), ("s3", "03"), ("s4", "04")] {
        insert_current_epoch_snapshot(
            &storage,
            uid,
            "r1",
            None,
            &format!("2025-01-{day}T00:00:00Z"),
        );
    }
    // Give each prunable snapshot a dependent orphan row (unresolved_edges); its FK needs a node.
    for uid in ["s1", "s2", "s3"] {
        let node_uid = format!("{uid}-node");
        storage.insert_nodes(&[bloat_node(uid, &node_uid)]).unwrap();
        insert_unresolved_edge(&storage, &format!("{uid}-edge"), uid, &node_uid);
    }
    storage.classify_repo_retention("r1").unwrap();
    let stats_before = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_before.prunable, 3);
    assert_eq!(stats_before.total, 4);

    // Fail deterministically on the SECOND chunk, independent of the (unordered) delete order: a
    // BEFORE DELETE trigger on `snapshots` counts deletions in `_prune_probe` and RAISE(ABORT)s once
    // the second is attempted. `snapshots` is deleted exactly once per chunk (after that chunk's
    // orphan deletes), so chunk 1 commits (n 0→1, no raise) and chunk 2 raises (n 1→2) → its
    // per-snapshot transaction is dropped without commit → ROLLBACK; the loop then returns Err, so
    // chunk 3 is never reached.
    storage
        .connection()
        .execute_batch(
            "CREATE TABLE _prune_probe (n INTEGER NOT NULL);\
             INSERT INTO _prune_probe VALUES (0);\
             CREATE TRIGGER fail_on_second_chunk BEFORE DELETE ON snapshots \
             BEGIN \
               UPDATE _prune_probe SET n = n + 1; \
               SELECT RAISE(ABORT, 'forced failure on a later prune chunk') \
                 WHERE (SELECT n FROM _prune_probe) >= 2; \
             END;",
        )
        .unwrap();

    let result = storage.prune_prunable_snapshots("r1");
    assert!(
        result.is_err(),
        "the forced later-chunk failure must surface, not be swallowed"
    );

    // (a) Exactly ONE snapshot committed-deleted; the failed + unreached prunable snapshots and the
    //     current one remain. The prune was NOT rolled back wholesale.
    let stats_after = storage.get_retention_stats("r1").unwrap();
    assert_eq!(
        stats_after.total, 3,
        "one committed chunk deleted its snapshot; 3 of 4 snapshots remain"
    );
    assert_eq!(
        stats_after.prunable, 2,
        "one prunable snapshot committed-deleted; two prunable snapshots remain"
    );

    // (b) Per-chunk atomicity of the orphan cleanup: exactly ONE unresolved_edge is gone (the
    //     committed chunk's). The FAILING chunk had already deleted its own unresolved_edge inside
    //     its transaction, but the per-snapshot rollback RESTORED it — so 2 of the original 3 remain,
    //     NOT 1. This is the airtight proof that the failed chunk's in-transaction deletes reverted.
    let remaining_orphans: i64 = storage
        .connection()
        .query_row("SELECT COUNT(*) FROM unresolved_edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        remaining_orphans, 2,
        "only the committed chunk's orphan row is gone; the failed chunk's orphan delete rolled back"
    );
}

#[test]
fn prune_restores_prior_cache_size_on_failure() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");
    storage.classify_repo_retention("r1").unwrap();

    storage
        .connection()
        .execute_batch("PRAGMA cache_size = -7000;")
        .unwrap();
    assert_eq!(cache_size(&storage), -7000);

    // Force a mid-prune failure: drop the first orphan-cleanup table the cascade deletes from
    // (`unresolved_edges`), so the very first `DELETE FROM unresolved_edges …` inside the per-snapshot
    // transaction errors AFTER the cache_size has been bumped. This exercises the restore-on-error path.
    storage
        .connection()
        .execute_batch("DROP TABLE unresolved_edges;")
        .unwrap();

    let err = storage.prune_prunable_snapshots("r1");
    assert!(
        err.is_err(),
        "prune must surface the forced delete failure, not swallow it"
    );

    assert_eq!(
        cache_size(&storage),
        -7000,
        "cache_size must be restored to the caller's prior value even when the prune fails"
    );
}
