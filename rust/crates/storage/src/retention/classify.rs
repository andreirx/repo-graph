//! Retention classification logic.
//!
//! This module implements the classification algorithm that assigns
//! retention classes to snapshots based on their relationships and
//! epoch validity.
//!
//! # Whole-Snapshot Invalidation
//!
//! Snapshots with stale `derived_cache_epoch` are excluded from
//! protected roles (current, parent, baseline_auto). They can only
//! be marked as `prunable` or preserved as `baseline_user`.
//!
//! A snapshot is considered "valid epoch" if:
//! - `derived_cache_epoch == CURRENT_CACHE_EPOCH`, OR
//! - `derived_cache_epoch IS NULL` (legacy/unclassified, treated as valid)
//!
//! # Transactional Guarantee
//!
//! Classification is atomic: either all snapshots are reclassified or
//! none are. This prevents partial classification states on failure.

use crate::connection::StorageConnection;
use crate::error::StorageError;

use super::types::{RetentionClass, RetentionStats, CURRENT_CACHE_EPOCH};

/// Snapshot row data: (uid, parent_uid, retention_class, derived_cache_epoch)
type SnapshotRow = (String, Option<String>, Option<String>, Option<String>);

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
    /// - `current`: the most recent ready snapshot with valid epoch
    /// - `parent`: the parent of current (if valid epoch)
    /// - `baseline_auto`: the most recent valid-epoch snapshot not current/parent
    /// - `prunable`: all other ready snapshots
    ///
    /// Does not modify snapshots already marked as `baseline_user`.
    ///
    /// **Whole-snapshot invalidation**: Snapshots with stale `derived_cache_epoch`
    /// are excluded from classification candidates. They cannot become `current`,
    /// `parent`, or `baseline_auto` - only `prunable`. This enforces the semantic
    /// rule that epoch mismatch invalidates the entire snapshot's derived cache.
    ///
    /// # Transactional Guarantee
    ///
    /// All classification updates run in a single transaction. If any update
    /// fails, the entire classification is rolled back and no snapshots are
    /// modified. This prevents partial classification states.
    pub fn classify_repo_retention(&self, repo_uid: &str) -> Result<(), StorageError> {
        let conn = self.connection();

        // Get all ready snapshots with epoch info, ordered by creation time (newest first)
        let mut stmt = conn.prepare(
            "SELECT snapshot_uid, parent_snapshot_uid, retention_class, derived_cache_epoch \
             FROM snapshots \
             WHERE repo_uid = ?1 AND status = 'ready' \
             ORDER BY created_at DESC",
        )?;

        let snapshots: Vec<SnapshotRow> = stmt
            .query_map(rusqlite::params![repo_uid], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Need to drop stmt before starting transaction
        drop(stmt);

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

        // Find current: most recent valid-epoch snapshot (not user baseline)
        for (uid, parent, retention, epoch) in &snapshots {
            if is_valid_epoch(epoch) {
                let is_user_baseline = retention
                    .as_ref()
                    .map(|r| r == "baseline_user")
                    .unwrap_or(false);
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
                let is_user_baseline = retention
                    .as_ref()
                    .map(|r| r == "baseline_user")
                    .unwrap_or(false);

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

        // Compute assignments before transaction
        let mut assignments: Vec<(&str, RetentionClass)> = Vec::new();

        for (uid, _, existing_retention, epoch) in &snapshots {
            // Preserve user baselines
            if existing_retention
                .as_ref()
                .map(|r| r == "baseline_user")
                .unwrap_or(false)
            {
                continue;
            }

            // Stale-epoch snapshots are always prunable
            let class = if !is_valid_epoch(epoch) {
                RetentionClass::Prunable
            } else if Some(uid.as_str()) == current_uid {
                RetentionClass::Current
            } else if Some(uid.as_str()) == parent_uid {
                RetentionClass::Parent
            } else if Some(uid.as_str()) == baseline_auto_uid {
                RetentionClass::BaselineAuto
            } else {
                RetentionClass::Prunable
            };

            assignments.push((uid.as_str(), class));
        }

        // Apply all assignments in a single transaction
        let tx = conn.unchecked_transaction()?;

        for (uid, class) in &assignments {
            tx.execute(
                "UPDATE snapshots SET retention_class = ?1, \
                 derived_cache_epoch = COALESCE(derived_cache_epoch, ?2) \
                 WHERE snapshot_uid = ?3",
                rusqlite::params![class.as_str(), CURRENT_CACHE_EPOCH, uid],
            )?;
        }

        tx.commit()?;

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
