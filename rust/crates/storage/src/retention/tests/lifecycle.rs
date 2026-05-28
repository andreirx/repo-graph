//! Lifecycle sequence tests.
//!
//! Tests for the classify -> prune sequence that forms the
//! retention lifecycle. The daemon's enforce_retention_lifecycle()
//! helper uses this sequence.

use super::{insert_current_epoch_snapshot, insert_repo, insert_snapshot, setup_storage};
use crate::retention::RetentionClass;

#[test]
fn classify_then_prune_sequence_works() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // 5 independent snapshots
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s4", "r1", None, "2025-01-04T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s5", "r1", None, "2025-01-05T00:00:00Z");

    // Step 1: Classify
    storage.classify_repo_retention("r1").unwrap();

    // Verify: s5=current, s4=baseline_auto, s1/s2/s3=prunable
    let stats_pre = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_pre.current, 1);
    assert_eq!(stats_pre.baseline_auto, 1);
    assert_eq!(stats_pre.prunable, 3);

    // Step 2: Prune
    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned, 3);

    // Verify: only current + baseline_auto remain
    let stats_post = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_post.total, 2);
    assert_eq!(stats_post.current, 1);
    assert_eq!(stats_post.baseline_auto, 1);
    assert_eq!(stats_post.prunable, 0);
}

#[test]
fn classify_then_prune_is_idempotent() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");

    // First run
    storage.classify_repo_retention("r1").unwrap();
    let pruned1 = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned1, 1); // s1 pruned

    // Second run: no change
    storage.classify_repo_retention("r1").unwrap();
    let pruned2 = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned2, 0);

    // Third run: still no change
    storage.classify_repo_retention("r1").unwrap();
    let pruned3 = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned3, 0);

    // Total should be 2 (current + baseline_auto)
    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.total, 2);
}

#[test]
fn classify_then_prune_reclaims_stale_epochs() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // s1, s2 have stale epoch
    insert_snapshot(
        &storage,
        "s1",
        "r1",
        None,
        "2025-01-01T00:00:00Z",
        Some("0.9"),
    );
    insert_snapshot(
        &storage,
        "s2",
        "r1",
        None,
        "2025-01-02T00:00:00Z",
        Some("0.9"),
    );
    // s3 has current epoch
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");

    // Classify: stale epochs become prunable
    storage.classify_repo_retention("r1").unwrap();
    let stats_pre = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_pre.prunable, 2);
    assert_eq!(stats_pre.stale_epoch, 2);

    // Prune: stale epochs deleted
    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned, 2);

    // Only current remains
    let stats_post = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_post.total, 1);
    assert_eq!(stats_post.current, 1);
}

#[test]
fn classify_then_prune_preserves_all_protected_classes() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // Create independent snapshots (no parent chain)
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s4", "r1", None, "2025-01-04T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s5", "r1", None, "2025-01-05T00:00:00Z");

    // Mark s1 as user baseline
    storage
        .mark_snapshot_retention("s1", RetentionClass::BaselineUser)
        .unwrap();

    // Classify
    storage.classify_repo_retention("r1").unwrap();
    let stats_pre = storage.get_retention_stats("r1").unwrap();
    // s5=current, s4=baseline_auto, s1=baseline_user, s2/s3=prunable
    assert_eq!(stats_pre.current, 1); // s5
    assert_eq!(stats_pre.baseline_auto, 1); // s4
    assert_eq!(stats_pre.baseline_user, 1); // s1
    assert_eq!(stats_pre.prunable, 2); // s2, s3

    // Prune
    let pruned = storage.prune_prunable_snapshots("r1").unwrap();
    assert_eq!(pruned, 2); // s2, s3 pruned

    // Verify all protected remain
    let stats_post = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats_post.total, 3);
    assert_eq!(stats_post.current, 1); // s5
    assert_eq!(stats_post.baseline_auto, 1); // s4
    assert_eq!(stats_post.baseline_user, 1); // s1
    assert_eq!(stats_post.prunable, 0);
}
