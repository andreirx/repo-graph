//! Classification algorithm tests.
//!
//! Tests for classify_repo_retention() which assigns retention classes
//! based on snapshot relationships.

use super::{insert_current_epoch_snapshot, insert_repo, setup_storage};
use crate::retention::RetentionClass;

#[test]
fn classify_repo_retention_single_snapshot() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");

    storage.classify_repo_retention("r1").unwrap();

    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.total, 1);
    assert_eq!(stats.current, 1);
    assert_eq!(stats.parent, 0);
    assert_eq!(stats.baseline_auto, 0);
    assert_eq!(stats.prunable, 0);
}

#[test]
fn classify_repo_retention_with_parent() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");

    storage.classify_repo_retention("r1").unwrap();

    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.total, 2);
    assert_eq!(stats.current, 1);
    assert_eq!(stats.parent, 1);
    assert_eq!(stats.baseline_auto, 0);
    assert_eq!(stats.prunable, 0);
}

#[test]
fn classify_repo_retention_with_baseline_auto() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z");

    storage.classify_repo_retention("r1").unwrap();

    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.total, 3);
    assert_eq!(stats.current, 1); // s3
    assert_eq!(stats.parent, 1); // s2
    assert_eq!(stats.baseline_auto, 1); // s1
    assert_eq!(stats.prunable, 0);
}

#[test]
fn classify_repo_retention_marks_excess_as_prunable() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s4", "r1", Some("s3"), "2025-01-04T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s5", "r1", Some("s4"), "2025-01-05T00:00:00Z");

    storage.classify_repo_retention("r1").unwrap();

    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.total, 5);
    assert_eq!(stats.current, 1); // s5
    assert_eq!(stats.parent, 1); // s4
    assert_eq!(stats.baseline_auto, 1); // s3
    assert_eq!(stats.prunable, 2); // s1, s2
}

#[test]
fn classify_repo_retention_preserves_user_baseline() {
    let storage = setup_storage();
    insert_repo(&storage, "r1");
    insert_current_epoch_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z");
    insert_current_epoch_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z");

    // Mark s1 as user baseline before classification
    storage
        .mark_snapshot_retention("s1", RetentionClass::BaselineUser)
        .unwrap();

    storage.classify_repo_retention("r1").unwrap();

    let stats = storage.get_retention_stats("r1").unwrap();
    assert_eq!(stats.baseline_user, 1); // s1 preserved
    assert_eq!(stats.current, 1); // s3
    assert_eq!(stats.parent, 1); // s2
                                 // s1 is user baseline, so no auto baseline
    assert_eq!(stats.baseline_auto, 0);
}
