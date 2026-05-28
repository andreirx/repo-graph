//! User baseline marking tests.
//!
//! Tests for marking snapshots as user baselines and how
//! classification interacts with user-assigned baselines.

use super::{insert_current_epoch_snapshot, insert_repo, setup_storage};
use crate::retention::RetentionClass;

#[test]
fn marking_current_as_baseline_user_promotes_new_current() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z");

    // Initial classification: s3=current, s2=baseline_auto, s1=prunable
    storage.classify_repo_retention("r1").unwrap();
    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.current, 1);
    assert_eq!(stats.baseline_auto, 1);
    assert_eq!(stats.baseline_user, 0);

    // Mark s3 (current) as user baseline
    storage
        .mark_snapshot_retention("s3", RetentionClass::BaselineUser)
        .unwrap();

    // Re-classify to maintain invariants
    storage.classify_repo_retention("r1").unwrap();

    let stats_after = storage.get_retention_stats("r1").unwrap();
    // s3 is now baseline_user
    assert_eq!(stats_after.baseline_user, 1);
    // A new current must be assigned (s2, the next most recent valid snapshot)
    assert_eq!(stats_after.current, 1);
    // s1 is now baseline_auto (s2 is current, s3 is user baseline)
    assert_eq!(stats_after.baseline_auto, 1);
    // No prunable since all 3 are accounted for
    assert_eq!(stats_after.prunable, 0);
}

#[test]
fn marking_parent_as_baseline_user_clears_parent_role() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");

    // Initial: s2=current, s1=parent
    storage.classify_repo_retention("r1").unwrap();
    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.current, 1);
    assert_eq!(stats.parent, 1);

    // Mark s1 (parent) as user baseline
    storage
        .mark_snapshot_retention("s1", RetentionClass::BaselineUser)
        .unwrap();

    // Re-classify
    storage.classify_repo_retention("r1").unwrap();

    let stats_after = storage.get_retention_stats("r1").unwrap();
    // s1 is now baseline_user
    assert_eq!(stats_after.baseline_user, 1);
    // s2 is still current
    assert_eq!(stats_after.current, 1);
    // No parent anymore (s1 was the only candidate and is now user baseline)
    assert_eq!(stats_after.parent, 0);
}
