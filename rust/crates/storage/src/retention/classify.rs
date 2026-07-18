//! Retention classification logic.
//!
//! This module implements the classification algorithm that assigns
//! retention classes to snapshots based on their relationships and
//! epoch validity.
//!
//! # Whole-Snapshot Invalidation
//!
//! Snapshots with stale `derived_cache_epoch` are excluded from
//! protected roles (current, parent). They can only be marked as
//! `prunable` or preserved as a baseline (`baseline_user` or `baseline_stamp`).
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
    /// SNAPSHOT-RETENTION-1 (ratified 2026-07-04, "git has history — I want
    /// DISCOVERY"): the keep-set is **current-state only**. This assigns:
    /// - `current`: the most recent ready snapshot with valid epoch;
    /// - `parent`: the parent of current (the delta-refresh base) when it has a
    ///   valid epoch — kept as *mechanics* (it makes the next refresh cheap), not
    ///   as history. If current has no parent, or the parent's epoch is stale, no
    ///   snapshot takes the `parent` role and the old parent falls to `prunable`;
    /// - `prunable`: **everything else**, including what earlier policy protected as
    ///   an auto-selected comparison baseline. The operator explicitly does not want
    ///   retained comparison history; cross-snapshot "what changed" recomputes from a
    ///   git baseline on demand (VISION: "git owns history").
    ///
    /// Steady state is ≤ 2 ready snapshots per repo (current + delta base).
    ///
    /// Does not modify snapshots already marked as `baseline_user` or
    /// `baseline_stamp` — an explicit human "keep this" that survives every
    /// reclassification. (EC-M7-BASELINE-STAMP-1: both are user baseline marks;
    /// they differ only in what the mark RETAINS — full graph rows vs the
    /// provenance stamp + measurements. The keep-set COUNT semantics are
    /// identical and unchanged.)
    ///
    /// The `baseline_auto` class is no longer *assigned* (the ratified keep-set
    /// dropped it); the enum variant is retained only so legacy rows still parse and
    /// are reclassified to `prunable` on the next pass. No schema migration.
    ///
    /// **Whole-snapshot invalidation**: Snapshots with stale `derived_cache_epoch`
    /// are excluded from the protected roles (`current`, `parent`) — they can only
    /// be `prunable` (or preserved as `baseline_user`/`baseline_stamp`). Epoch mismatch invalidates
    /// the entire snapshot's derived cache.
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

        // CACHE-SEMANTICS-1 + SNAPSHOT-RETENTION-1: only valid-epoch snapshots can be current/parent.
        // Stale-epoch snapshots are always prunable (unless baseline_user).

        let mut current_uid: Option<&str> = None;
        let mut parent_uid: Option<&str> = None;

        // A user baseline MARK (row-retaining or stamp) is excluded from the
        // current/parent serving roles and preserved as-is across reclassification.
        let is_user_mark = |retention: &Option<String>| -> bool {
            retention
                .as_ref()
                .map(|r| r == "baseline_user" || r == "baseline_stamp")
                .unwrap_or(false)
        };

        // Find current: most recent valid-epoch snapshot (not a user baseline mark)
        for (uid, parent, retention, epoch) in &snapshots {
            if is_valid_epoch(epoch) && !is_user_mark(retention) {
                current_uid = Some(uid.as_str());
                if let Some(p) = parent {
                    parent_uid = Some(p.as_str());
                }
                break;
            }
        }

        // Validate parent has valid epoch; if not, clear it — delta refresh is then inapplicable,
        // so the old parent falls through to `prunable` (ratified: parent kept ONLY as delta base).
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
            // Preserve user baseline marks (row-retaining AND stamp)
            if is_user_mark(existing_retention) {
                continue;
            }

            // Stale-epoch snapshots are always prunable; otherwise current + delta-base parent are
            // the ONLY protected roles (ratified keep-set) — everything else prunes.
            let class = if !is_valid_epoch(epoch) {
                RetentionClass::Prunable
            } else if Some(uid.as_str()) == current_uid {
                RetentionClass::Current
            } else if Some(uid.as_str()) == parent_uid {
                RetentionClass::Parent
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

    /// Read one snapshot's retention class.
    ///
    /// `None` when the snapshot has no class yet (pre-classification) or does
    /// not exist. Values this codebase never writes parse to `None` too — the
    /// callers (the mark handler's upgrade/downgrade guards and the per-mark
    /// retention report) treat both identically as "not a baseline mark".
    pub fn get_snapshot_retention_class(
        &self,
        snapshot_uid: &str,
    ) -> Result<Option<RetentionClass>, StorageError> {
        let class: Option<Option<String>> = match self.connection().query_row(
            "SELECT retention_class FROM snapshots WHERE snapshot_uid = ?1",
            rusqlite::params![snapshot_uid],
            |row| row.get(0),
        ) {
            Ok(c) => Some(c),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(StorageError::Sqlite(e)),
        };
        Ok(class.flatten().and_then(|c| RetentionClass::parse(&c)))
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
        let baseline_stamp = count_class("baseline_stamp")?;
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
            baseline_stamp,
            prunable,
            unclassified,
            stale_epoch,
        })
    }

    /// Mark all snapshots with stale epochs as prunable.
    ///
    /// A snapshot has a stale epoch if its `derived_cache_epoch` does not match
    /// the current cache epoch. This does not affect snapshots marked as
    /// `baseline_user` or `baseline_stamp` — both are explicit user marks; a
    /// stamp's retained content (snapshot row + measurements) is epoch-independent
    /// provenance, so an epoch bump must not silently delete a human's mark.
    ///
    /// Returns the number of snapshots marked.
    pub fn mark_stale_epochs_prunable(&self, repo_uid: &str) -> Result<i64, StorageError> {
        let conn = self.connection();

        let affected = conn.execute(
            "UPDATE snapshots SET retention_class = 'prunable' \
             WHERE repo_uid = ?1 \
             AND derived_cache_epoch IS NOT NULL \
             AND derived_cache_epoch != ?2 \
             AND (retention_class IS NULL \
                  OR retention_class NOT IN ('baseline_user', 'baseline_stamp'))",
            rusqlite::params![repo_uid, CURRENT_CACHE_EPOCH],
        )?;

        Ok(affected as i64)
    }
}
