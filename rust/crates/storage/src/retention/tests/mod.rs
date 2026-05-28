//! Tests for retention management.
//!
//! Split by concern:
//! - `types`: RetentionClass roundtrip and protection status
//! - `classify`: Classification algorithm tests
//! - `epoch`: Stale epoch handling tests
//! - `baseline`: User baseline marking tests
//! - `prune`: Pruning operation tests
//! - `lifecycle`: Full classify-then-prune sequence tests

mod baseline;
mod classify;
mod epoch;
mod lifecycle;
mod prune;
mod types;

use crate::connection::StorageConnection;
use crate::retention::CURRENT_CACHE_EPOCH;

/// Create an in-memory storage connection for testing.
pub fn setup_storage() -> StorageConnection {
    StorageConnection::open_in_memory().unwrap()
}

/// Insert a test repo.
pub fn insert_repo(storage: &StorageConnection, repo_uid: &str) {
    storage
        .connection()
        .execute(
            "INSERT INTO repos (repo_uid, name, root_path, created_at) \
             VALUES (?1, 'test', '/test', '2025-01-01T00:00:00Z')",
            rusqlite::params![repo_uid],
        )
        .unwrap();
}

/// Insert a test snapshot.
pub fn insert_snapshot(
    storage: &StorageConnection,
    snapshot_uid: &str,
    repo_uid: &str,
    parent_uid: Option<&str>,
    created_at: &str,
    epoch: Option<&str>,
) {
    storage
        .connection()
        .execute(
            "INSERT INTO snapshots \
             (snapshot_uid, repo_uid, kind, status, created_at, parent_snapshot_uid, derived_cache_epoch) \
             VALUES (?1, ?2, 'full', 'ready', ?3, ?4, ?5)",
            rusqlite::params![snapshot_uid, repo_uid, created_at, parent_uid, epoch],
        )
        .unwrap();
}

/// Insert a snapshot with current epoch (convenience).
pub fn insert_current_epoch_snapshot(
    storage: &StorageConnection,
    snapshot_uid: &str,
    repo_uid: &str,
    parent_uid: Option<&str>,
    created_at: &str,
) {
    insert_snapshot(
        storage,
        snapshot_uid,
        repo_uid,
        parent_uid,
        created_at,
        Some(CURRENT_CACHE_EPOCH),
    );
}
