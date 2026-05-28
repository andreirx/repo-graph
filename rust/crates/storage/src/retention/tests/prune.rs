//! Pruning operation tests.
//!
//! Tests for prune_prunable_snapshots() which deletes snapshots
//! marked as prunable.

use super::{insert_current_epoch_snapshot, insert_repo, setup_storage};

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
