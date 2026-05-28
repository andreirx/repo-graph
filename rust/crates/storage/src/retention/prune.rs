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

        // Count before transaction - this is the return value.
        // Safe to read outside transaction; worst case we return a count
        // that's slightly stale if concurrent writes happen, but the actual
        // deletion is atomic.
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM snapshots WHERE repo_uid = ?1 AND retention_class = 'prunable'",
            rusqlite::params![repo_uid],
            |row| row.get(0),
        )?;

        if count == 0 {
            return Ok(0);
        }

        // Begin transaction for all mutations.
        // Uses unchecked_transaction() because &self, not &mut self.
        // Safe: daemon holds write lock, no nested transactions.
        let tx = conn.unchecked_transaction()?;

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

        for table in &orphan_cleanup_tables {
            tx.execute(
                &format!(
                    "DELETE FROM {} WHERE snapshot_uid IN ( \
                     SELECT snapshot_uid FROM snapshots \
                     WHERE repo_uid = ?1 AND retention_class = 'prunable' \
                     )",
                    table
                ),
                rusqlite::params![repo_uid],
            )?;
        }

        // Clear parent_snapshot_uid references to prunable snapshots.
        // This breaks the self-referencing FK link before deletion.
        tx.execute(
            "UPDATE snapshots SET parent_snapshot_uid = NULL \
             WHERE repo_uid = ?1 \
             AND parent_snapshot_uid IN ( \
                 SELECT snapshot_uid FROM snapshots \
                 WHERE repo_uid = ?1 AND retention_class = 'prunable' \
             )",
            rusqlite::params![repo_uid],
        )?;

        // Delete the prunable snapshots
        tx.execute(
            "DELETE FROM snapshots WHERE repo_uid = ?1 AND retention_class = 'prunable'",
            rusqlite::params![repo_uid],
        )?;

        // Commit - all changes become visible atomically
        tx.commit()?;

        Ok(count)
    }
}
