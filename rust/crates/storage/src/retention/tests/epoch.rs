//! Stale epoch handling tests.
//!
//! Tests for epoch-based invalidation and the whole-snapshot
//! invalidation semantic contract.

use super::{insert_current_epoch_snapshot, insert_repo, insert_snapshot, setup_storage};
use crate::retention::RetentionClass;

#[test]
fn mark_stale_epochs_prunable_marks_old_epochs() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // s1 has old epoch
    insert_snapshot(
        &storage,
        "s1",
        "r1",
        None,
        "2025-01-01T00:00:00Z",
        Some("0.9"),
    );
    // s2 has current epoch
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");

    let marked = storage.mark_stale_epochs_prunable("r1").unwrap();
    assert_eq!(marked, 1);

    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.stale_epoch, 1);
}

#[test]
fn mark_stale_epochs_preserves_user_baseline() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // s1 has old epoch but is user baseline
    insert_snapshot(
        &storage,
        "s1",
        "r1",
        None,
        "2025-01-01T00:00:00Z",
        Some("0.9"),
    );
    storage
        .mark_snapshot_retention("s1", RetentionClass::BaselineUser)
        .unwrap();

    let marked = storage.mark_stale_epochs_prunable("r1").unwrap();
    assert_eq!(marked, 0); // User baseline not touched

    // Verify retention class unchanged
    let class: Option<String> = storage
        .connection()
        .query_row(
            "SELECT retention_class FROM snapshots WHERE snapshot_uid = 's1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(class, Some("baseline_user".to_string()));
}

#[test]
fn stale_epoch_snapshots_cannot_become_protected() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // s1 has stale epoch (would be parent based on parent_snapshot_uid)
    insert_snapshot(
        &storage,
        "s1",
        "r1",
        None,
        "2025-01-01T00:00:00Z",
        Some("0.9"),
    );
    // s2 has current epoch, parent points to s1
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");

    storage.classify_repo_retention("r1").unwrap();

    let stats = storage.get_retention_stats("r1").unwrap();
    // s2 is current (valid epoch)
    assert_eq!(stats.current, 1);
    // s1 is NOT parent despite parent_snapshot_uid link, because it has stale epoch
    assert_eq!(stats.parent, 0);
    // s1 is prunable due to stale epoch
    assert_eq!(stats.prunable, 1);
    // stale_epoch count confirms s1 has mismatched epoch
    assert_eq!(stats.stale_epoch, 1);
}

#[test]
fn stale_epoch_parent_skipped_for_valid_epoch_grandparent() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // s1 has current epoch
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    // s2 has stale epoch
    insert_snapshot(
        &storage,
        "s2",
        "r1",
        Some("s1"),
        "2025-01-02T00:00:00Z",
        Some("0.9"),
    );
    // s3 has current epoch, parent points to stale s2
    insert_current_epoch_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z");

    storage.classify_repo_retention("r1").unwrap();

    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.current, 1); // s3
                                  // s2 cannot be parent (stale epoch), so no parent assigned
    assert_eq!(stats.parent, 0);
    // SNAPSHOT-RETENTION-1: s1 is valid-epoch but neither current nor delta-base parent → prunable
    // (auto-baseline no longer retained); s2 is prunable for its stale epoch.
    assert_eq!(stats.baseline_auto, 0);
    assert_eq!(stats.prunable, 2); // s1 (not kept) + s2 (stale)
    assert_eq!(stats.stale_epoch, 1);
}

#[test]
fn null_epoch_treated_as_valid_legacy() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    // s1 has NULL epoch (legacy snapshot)
    insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", None);
    // s2 has current epoch
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");

    storage.classify_repo_retention("r1").unwrap();

    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.current, 1); // s2
                                  // s1 can be parent (NULL epoch treated as valid)
    assert_eq!(stats.parent, 1); // s1
    assert_eq!(stats.prunable, 0);
    assert_eq!(stats.stale_epoch, 0); // NULL is not counted as stale
}
