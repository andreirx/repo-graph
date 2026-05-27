//! CACHE-SEMANTICS-1: Snapshot retention management.
//!
//! This module provides types and storage methods for managing snapshot
//! retention according to the cache/authority semantic model.
//!
//! # Retention Classes
//!
//! Each snapshot is assigned a retention class that determines its lifecycle:
//!
//! - `current`: Active snapshot for this repo (always retained)
//! - `parent`: Parent of current snapshot (retained for incremental refresh)
//! - `baseline_auto`: Automatically selected comparison baseline
//! - `baseline_user`: Explicitly marked by user as baseline
//! - `prunable`: Eligible for pruning
//!
//! # Derived Cache Epoch
//!
//! Snapshots carry a `derived_cache_epoch` that indicates the validity of
//! their Tier B (derived cache) data. When the extractor version changes,
//! snapshots with mismatched epochs are considered stale and can be marked
//! prunable.
//!
//! # Whole-Snapshot Invalidation
//!
//! The semantic contract is whole-snapshot invalidation: if the epoch
//! mismatches, the entire snapshot's derived cache is stale. There are
//! no half-valid snapshot states.
//!
//! # References
//!
//! - `docs/slices/cache-semantics-1.md`
//! - `agent_docs/storage-architecture-v2.md`

use serde::{Deserialize, Serialize};

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// Current derived cache epoch.
///
/// Bump major on breaking extractor changes requiring full re-extraction.
/// Bump minor on compatible changes (new optional data).
pub const CURRENT_CACHE_EPOCH: &str = "1.0";

/// Retention class for a snapshot.
///
/// Determines whether a snapshot is protected from pruning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionClass {
    /// Active snapshot for this repo (always retained)
    Current,
    /// Parent of current snapshot (retained for incremental refresh)
    Parent,
    /// Automatically selected comparison baseline
    BaselineAuto,
    /// Explicitly marked by user as baseline
    BaselineUser,
    /// Eligible for pruning
    Prunable,
}

impl RetentionClass {
    /// Convert to string for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            RetentionClass::Current => "current",
            RetentionClass::Parent => "parent",
            RetentionClass::BaselineAuto => "baseline_auto",
            RetentionClass::BaselineUser => "baseline_user",
            RetentionClass::Prunable => "prunable",
        }
    }

    /// Parse from storage string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "current" => Some(RetentionClass::Current),
            "parent" => Some(RetentionClass::Parent),
            "baseline_auto" => Some(RetentionClass::BaselineAuto),
            "baseline_user" => Some(RetentionClass::BaselineUser),
            "prunable" => Some(RetentionClass::Prunable),
            _ => None,
        }
    }

    /// Returns true if this snapshot is protected from pruning.
    pub fn is_protected(&self) -> bool {
        !matches!(self, RetentionClass::Prunable)
    }
}

/// Retention statistics for a repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionStats {
    /// Total snapshot count
    pub total: i64,
    /// Current snapshots
    pub current: i64,
    /// Parent snapshots
    pub parent: i64,
    /// Auto-baseline snapshots
    pub baseline_auto: i64,
    /// User-baseline snapshots
    pub baseline_user: i64,
    /// Prunable snapshots
    pub prunable: i64,
    /// Snapshots with NULL retention_class (pre-classification)
    pub unclassified: i64,
    /// Snapshots with stale epoch
    pub stale_epoch: i64,
}

impl StorageConnection {
    /// Mark a snapshot's retention class.
    ///
    /// Also sets the derived_cache_epoch to the current epoch if not already set.
    pub fn mark_snapshot_retention(
        &self,
        snapshot_uid: &str,
        retention_class: RetentionClass,
    ) -> Result<(), StorageError> {
        let conn = self.connection();

        conn.execute(
            "UPDATE snapshots SET retention_class = ?1, \
             derived_cache_epoch = COALESCE(derived_cache_epoch, ?2) \
             WHERE snapshot_uid = ?3",
            rusqlite::params![retention_class.as_str(), CURRENT_CACHE_EPOCH, snapshot_uid],
        )?;

        Ok(())
    }

    /// Classify retention for all snapshots of a repo based on relationships.
    ///
    /// This assigns:
    /// - `current`: the most recent ready snapshot
    /// - `parent`: the parent of the current snapshot
    /// - `baseline_auto`: the most recent ready snapshot before current (if not parent)
    /// - `prunable`: all other ready snapshots
    ///
    /// Does not modify snapshots already marked as `baseline_user`.
    ///
    /// **Whole-snapshot invalidation**: Snapshots with stale `derived_cache_epoch`
    /// are excluded from classification candidates. They cannot become `current`,
    /// `parent`, or `baseline_auto` — only `prunable`. This enforces the semantic
    /// rule that epoch mismatch invalidates the entire snapshot's derived cache.
    ///
    /// A snapshot is considered "valid epoch" if:
    /// - `derived_cache_epoch == CURRENT_CACHE_EPOCH`, OR
    /// - `derived_cache_epoch IS NULL` (legacy/unclassified, treated as potentially valid)
    pub fn classify_repo_retention(&self, repo_uid: &str) -> Result<(), StorageError> {
        let conn = self.connection();

        // Get all ready snapshots with epoch info, ordered by creation time (newest first)
        let mut stmt = conn.prepare(
            "SELECT snapshot_uid, parent_snapshot_uid, retention_class, derived_cache_epoch \
             FROM snapshots \
             WHERE repo_uid = ?1 AND status = 'ready' \
             ORDER BY created_at DESC",
        )?;

        let snapshots: Vec<(String, Option<String>, Option<String>, Option<String>)> = stmt
            .query_map(rusqlite::params![repo_uid], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if snapshots.is_empty() {
            return Ok(());
        }

        // Helper: check if epoch is valid (current or NULL)
        let is_valid_epoch = |epoch: &Option<String>| -> bool {
            match epoch {
                None => true, // NULL treated as legacy/valid
                Some(e) => e == CURRENT_CACHE_EPOCH,
            }
        };

        // CACHE-SEMANTICS-1: Only valid-epoch snapshots can be current/parent/baseline_auto.
        // Stale-epoch snapshots are always prunable (unless baseline_user).

        let mut current_uid: Option<&str> = None;
        let mut parent_uid: Option<&str> = None;
        let mut baseline_auto_uid: Option<&str> = None;

        // Find current: most recent valid-epoch snapshot
        for (uid, parent, retention, epoch) in &snapshots {
            if is_valid_epoch(epoch) {
                let is_user_baseline = retention.as_ref().map(|r| r == "baseline_user").unwrap_or(false);
                if !is_user_baseline {
                    current_uid = Some(uid.as_str());
                    if let Some(p) = parent {
                        parent_uid = Some(p.as_str());
                    }
                    break;
                }
            }
        }

        // Find baseline_auto: first valid-epoch snapshot that is not current, parent, or user_baseline
        if current_uid.is_some() {
            for (uid, _, retention, epoch) in &snapshots {
                if !is_valid_epoch(epoch) {
                    continue; // Skip stale epochs
                }
                let is_current = current_uid.map(|c| c == uid.as_str()).unwrap_or(false);
                let is_parent = parent_uid.map(|p| p == uid.as_str()).unwrap_or(false);
                let is_user_baseline = retention.as_ref().map(|r| r == "baseline_user").unwrap_or(false);

                if !is_current && !is_parent && !is_user_baseline {
                    baseline_auto_uid = Some(uid.as_str());
                    break;
                }
            }
        }

        // Validate parent has valid epoch; if not, clear it
        if let Some(p_uid) = parent_uid {
            let parent_valid = snapshots
                .iter()
                .find(|(uid, _, _, _)| uid == p_uid)
                .map(|(_, _, _, epoch)| is_valid_epoch(epoch))
                .unwrap_or(false);
            if !parent_valid {
                parent_uid = None;
            }
        }

        // Now assign retention classes
        for (uid, _, existing_retention, epoch) in &snapshots {
            // Preserve user baselines
            if existing_retention.as_ref().map(|r| r == "baseline_user").unwrap_or(false) {
                continue;
            }

            // Stale-epoch snapshots are always prunable
            if !is_valid_epoch(epoch) {
                self.mark_snapshot_retention(uid, RetentionClass::Prunable)?;
                continue;
            }

            let class = if Some(uid.as_str()) == current_uid {
                RetentionClass::Current
            } else if Some(uid.as_str()) == parent_uid {
                RetentionClass::Parent
            } else if Some(uid.as_str()) == baseline_auto_uid {
                RetentionClass::BaselineAuto
            } else {
                RetentionClass::Prunable
            };

            self.mark_snapshot_retention(uid, class)?;
        }

        Ok(())
    }

    /// Get retention statistics for a repo.
    pub fn get_retention_stats(&self, repo_uid: &str) -> Result<RetentionStats, StorageError> {
        let conn = self.connection();

        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ?1",
            rusqlite::params![repo_uid],
            |row| row.get(0),
        )?;

        let count_class = |class: &str| -> Result<i64, StorageError> {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ?1 AND retention_class = ?2",
                rusqlite::params![repo_uid, class],
                |row| row.get(0),
            )?)
        };

        let current = count_class("current")?;
        let parent = count_class("parent")?;
        let baseline_auto = count_class("baseline_auto")?;
        let baseline_user = count_class("baseline_user")?;
        let prunable = count_class("prunable")?;

        let unclassified: i64 = conn.query_row(
            "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ?1 AND retention_class IS NULL",
            rusqlite::params![repo_uid],
            |row| row.get(0),
        )?;

        let stale_epoch: i64 = conn.query_row(
            "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ?1 \
             AND derived_cache_epoch IS NOT NULL AND derived_cache_epoch != ?2",
            rusqlite::params![repo_uid, CURRENT_CACHE_EPOCH],
            |row| row.get(0),
        )?;

        Ok(RetentionStats {
            total,
            current,
            parent,
            baseline_auto,
            baseline_user,
            prunable,
            unclassified,
            stale_epoch,
        })
    }

    /// Prune snapshots marked as prunable for a repo.
    ///
    /// This deletes the snapshot rows. Tier B data in other tables is
    /// cascade-deleted via foreign key constraints.
    ///
    /// Returns the number of snapshots pruned.
    pub fn prune_prunable_snapshots(&self, repo_uid: &str) -> Result<i64, StorageError> {
        let conn = self.connection();

        // Count before delete
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ?1 AND retention_class = 'prunable'",
            rusqlite::params![repo_uid],
            |row| row.get(0),
        )?;

        if count > 0 {
            conn.execute(
                "DELETE FROM snapshots WHERE repo_uid = ?1 AND retention_class = 'prunable'",
                rusqlite::params![repo_uid],
            )?;
        }

        Ok(count)
    }

    /// Mark all snapshots with stale epochs as prunable.
    ///
    /// A snapshot has a stale epoch if its `derived_cache_epoch` does not match
    /// the current cache epoch. This does not affect snapshots marked as
    /// `baseline_user`.
    ///
    /// Returns the number of snapshots marked.
    pub fn mark_stale_epochs_prunable(&self, repo_uid: &str) -> Result<i64, StorageError> {
        let conn = self.connection();

        let affected = conn.execute(
            "UPDATE snapshots SET retention_class = 'prunable' \
             WHERE repo_uid = ?1 \
             AND derived_cache_epoch IS NOT NULL \
             AND derived_cache_epoch != ?2 \
             AND (retention_class IS NULL OR retention_class != 'baseline_user')",
            rusqlite::params![repo_uid, CURRENT_CACHE_EPOCH],
        )?;

        Ok(affected as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::StorageConnection;

    fn setup_storage() -> StorageConnection {
        StorageConnection::open_in_memory().unwrap()
    }

    fn insert_repo(storage: &StorageConnection, repo_uid: &str) {
        storage
            .connection()
            .execute(
                "INSERT INTO repos (repo_uid, name, root_path, created_at) \
                 VALUES (?1, 'test', '/test', '2025-01-01T00:00:00Z')",
                rusqlite::params![repo_uid],
            )
            .unwrap();
    }

    fn insert_snapshot(
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

    #[test]
    fn retention_class_roundtrip() {
        for class in [
            RetentionClass::Current,
            RetentionClass::Parent,
            RetentionClass::BaselineAuto,
            RetentionClass::BaselineUser,
            RetentionClass::Prunable,
        ] {
            let s = class.as_str();
            let parsed = RetentionClass::from_str(s).unwrap();
            assert_eq!(class, parsed);
        }
    }

    #[test]
    fn protection_status() {
        assert!(RetentionClass::Current.is_protected());
        assert!(RetentionClass::Parent.is_protected());
        assert!(RetentionClass::BaselineAuto.is_protected());
        assert!(RetentionClass::BaselineUser.is_protected());
        assert!(!RetentionClass::Prunable.is_protected());
    }

    #[test]
    fn classify_repo_retention_single_snapshot() {
        let storage = setup_storage();
        insert_repo(&storage, "r1");
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

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
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

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
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

        storage.classify_repo_retention("r1").unwrap();

        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.current, 1);  // s3
        assert_eq!(stats.parent, 1);   // s2
        assert_eq!(stats.baseline_auto, 1);  // s1
        assert_eq!(stats.prunable, 0);
    }

    #[test]
    fn classify_repo_retention_marks_excess_as_prunable() {
        let storage = setup_storage();
        insert_repo(&storage, "r1");
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s4", "r1", Some("s3"), "2025-01-04T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s5", "r1", Some("s4"), "2025-01-05T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

        storage.classify_repo_retention("r1").unwrap();

        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.total, 5);
        assert_eq!(stats.current, 1);  // s5
        assert_eq!(stats.parent, 1);   // s4
        assert_eq!(stats.baseline_auto, 1);  // s3
        assert_eq!(stats.prunable, 2);  // s1, s2
    }

    #[test]
    fn classify_repo_retention_preserves_user_baseline() {
        let storage = setup_storage();
        insert_repo(&storage, "r1");
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

        // Mark s1 as user baseline before classification
        storage.mark_snapshot_retention("s1", RetentionClass::BaselineUser).unwrap();

        storage.classify_repo_retention("r1").unwrap();

        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.baseline_user, 1);  // s1 preserved
        assert_eq!(stats.current, 1);  // s3
        assert_eq!(stats.parent, 1);   // s2
        // s1 is user baseline, so no auto baseline
        assert_eq!(stats.baseline_auto, 0);
    }

    #[test]
    fn prune_prunable_snapshots_deletes_marked() {
        let storage = setup_storage();
        insert_repo(&storage, "r1");
        // Create independent snapshots (no parent relationships) to avoid FK issues
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s4", "r1", None, "2025-01-04T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

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

    #[test]
    fn mark_stale_epochs_prunable_marks_old_epochs() {
        let storage = setup_storage();
        insert_repo(&storage, "r1");
        // s1 has old epoch
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some("0.9"));
        // s2 has current epoch
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

        let marked = storage.mark_stale_epochs_prunable("r1").unwrap();
        assert_eq!(marked, 1);

        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.stale_epoch, 1);  // s1 has stale epoch (even though now marked prunable)
    }

    #[test]
    fn mark_stale_epochs_preserves_user_baseline() {
        let storage = setup_storage();
        insert_repo(&storage, "r1");
        // s1 has old epoch but is user baseline
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some("0.9"));
        storage.mark_snapshot_retention("s1", RetentionClass::BaselineUser).unwrap();

        let marked = storage.mark_stale_epochs_prunable("r1").unwrap();
        assert_eq!(marked, 0);  // User baseline not touched

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
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some("0.9"));
        // s2 has current epoch, parent points to s1
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

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
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        // s2 has stale epoch
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some("0.9"));
        // s3 has current epoch, parent points to stale s2
        insert_snapshot(&storage, "s3", "r1", Some("s2"), "2025-01-03T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

        storage.classify_repo_retention("r1").unwrap();

        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.current, 1);  // s3
        // s2 cannot be parent (stale epoch), so no parent assigned
        assert_eq!(stats.parent, 0);
        // s1 could be baseline_auto (valid epoch, not current/parent)
        assert_eq!(stats.baseline_auto, 1);  // s1
        // s2 is prunable (stale epoch)
        assert_eq!(stats.prunable, 1);
        assert_eq!(stats.stale_epoch, 1);
    }

    #[test]
    fn null_epoch_treated_as_valid_legacy() {
        let storage = setup_storage();
        insert_repo(&storage, "r1");
        // s1 has NULL epoch (legacy snapshot)
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", None);
        // s2 has current epoch
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

        storage.classify_repo_retention("r1").unwrap();

        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.current, 1);  // s2
        // s1 can be parent (NULL epoch treated as valid)
        assert_eq!(stats.parent, 1);  // s1
        assert_eq!(stats.prunable, 0);
        assert_eq!(stats.stale_epoch, 0);  // NULL is not counted as stale
    }

    #[test]
    fn marking_current_as_baseline_user_promotes_new_current() {
        let storage = setup_storage();
        insert_repo(&storage, "r1");
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s2", "r1", None, "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s3", "r1", None, "2025-01-03T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

        // Initial classification: s3=current, s2=baseline_auto, s1=prunable
        storage.classify_repo_retention("r1").unwrap();
        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.current, 1);
        assert_eq!(stats.baseline_auto, 1);
        assert_eq!(stats.baseline_user, 0);

        // Mark s3 (current) as user baseline
        storage.mark_snapshot_retention("s3", RetentionClass::BaselineUser).unwrap();

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
        insert_snapshot(&storage, "s1", "r1", None, "2025-01-01T00:00:00Z", Some(CURRENT_CACHE_EPOCH));
        insert_snapshot(&storage, "s2", "r1", Some("s1"), "2025-01-02T00:00:00Z", Some(CURRENT_CACHE_EPOCH));

        // Initial: s2=current, s1=parent
        storage.classify_repo_retention("r1").unwrap();
        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(stats.current, 1);
        assert_eq!(stats.parent, 1);

        // Mark s1 (parent) as user baseline
        storage.mark_snapshot_retention("s1", RetentionClass::BaselineUser).unwrap();

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
}
