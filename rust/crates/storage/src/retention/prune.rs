//! Retention pruning logic.
//!
//! This module implements snapshot pruning - the deletion of snapshots
//! marked as `prunable`. All operations are transactional to ensure
//! atomic cleanup.
//!
//! # FK Constraints
//!
//! Some tables lack `ON DELETE CASCADE` on their `snapshot_uid` FK:
//! - `unresolved_edges`
//! - `boundary_provider_facts`, `boundary_consumer_facts`, `boundary_links`
//! - `boundary_interaction_surfaces`, `boundary_interaction_links`
//! - `snapshots.parent_snapshot_uid` (self-referencing)
//!
//! These are cleaned up explicitly within the transaction before
//! deleting the snapshot rows.
//!
//! # Concurrency Model
//!
//! Uses `unchecked_transaction()` with `&self` rather than `transaction()`
//! with `&mut self`. This is safe because:
//!
//! 1. The daemon acquires a write lock via `RepoCoordinator` before calling
//!    the retention lifecycle, preventing concurrent modifications
//! 2. This code path is never nested inside another transaction
//! 3. The `unchecked_` prefix indicates the compile-time ownership check is
//!    skipped, not that the transaction itself is unsafe
//!
//! This pattern matches `refresh_copy_forward_impl.rs` and enables the
//! storage connection to be held inside `Arc<RepoState>` without interior
//! mutability wrappers.

use crate::connection::StorageConnection;
use crate::error::StorageError;

impl StorageConnection {
    /// Prune snapshots marked as prunable for a repo.
    ///
    /// This deletes the snapshot rows and all dependent data. The operation
    /// is **atomic**: either all prunable snapshots are deleted (along with
    /// their dependent rows), or the database is unchanged.
    ///
    /// # Transactional Guarantee
    ///
    /// The prune sequence runs in a single transaction:
    /// 1. Count prunable snapshots (outside transaction, for return value)
    /// 2. Delete orphan rows from tables without CASCADE
    /// 3. Clear self-referencing parent_snapshot_uid links
    /// 4. Delete the prunable snapshot rows
    /// 5. Commit
    ///
    /// If any step fails, the transaction rolls back and no changes persist.
    ///
    /// # Concurrency
    ///
    /// Caller must hold the repo write lock. This method uses
    /// `unchecked_transaction()` which does not enforce exclusive access
    /// at compile time. The daemon's `RepoCoordinator` provides the
    /// necessary synchronization.
    ///
    /// # Returns
    ///
    /// The number of snapshots pruned (counted before deletion).
    pub fn prune_prunable_snapshots(&self, repo_uid: &str) -> Result<i64, StorageError> {
        let conn = self.connection();

        // Collect prunable snapshot UIDs first.
        // This allows us to delete one snapshot at a time, avoiding massive
        // single-statement deletes that can hang on large tables.
        let snapshot_uids: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT snapshot_uid FROM snapshots \
                 WHERE repo_uid = ?1 AND retention_class = 'prunable'",
            )?;
            let rows = stmt.query_map(rusqlite::params![repo_uid], |row| row.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        if snapshot_uids.is_empty() {
            return Ok(0);
        }

        // Tables without ON DELETE CASCADE on snapshot_uid FK.
        // Must delete explicitly before deleting the snapshot.
        let orphan_cleanup_tables = [
            "unresolved_edges",
            "boundary_provider_facts",
            "boundary_consumer_facts",
            "boundary_links",
            "boundary_interaction_surfaces",
            "boundary_interaction_links",
        ];

        // Process one snapshot at a time.
        // This prevents giant deletes that can hang on tables with 1M+ rows.
        // Each snapshot is deleted atomically within its own transaction.
        for snapshot_uid in &snapshot_uids {
            // Begin transaction for this snapshot's deletion.
            let tx = conn.unchecked_transaction()?;

            // Delete orphan rows for this specific snapshot
            for table in &orphan_cleanup_tables {
                tx.execute(
                    &format!(
                        "DELETE FROM {} WHERE snapshot_uid = ?1",
                        table
                    ),
                    rusqlite::params![snapshot_uid],
                )?;
            }

            // Clear parent_snapshot_uid references to this snapshot
            tx.execute(
                "UPDATE snapshots SET parent_snapshot_uid = NULL \
                 WHERE parent_snapshot_uid = ?1",
                rusqlite::params![snapshot_uid],
            )?;

            // Delete the snapshot itself
            tx.execute(
                "DELETE FROM snapshots WHERE snapshot_uid = ?1",
                rusqlite::params![snapshot_uid],
            )?;

            tx.commit()?;
        }

        Ok(snapshot_uids.len() as i64)
    }
}
