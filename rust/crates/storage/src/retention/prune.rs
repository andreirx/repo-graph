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

        self.delete_snapshots_cascade(&snapshot_uids)?;
        Ok(snapshot_uids.len() as i64)
    }

    /// DAEMON-VISIBILITY-1 (F3, operator Option A): delete every NON-READY (interrupted / failed /
    /// stale-building) snapshot for a repo and return the deleted UIDs.
    ///
    /// # Why this is separate from [`prune_prunable_snapshots`]
    ///
    /// The retention model only classifies + prunes `status='ready'` snapshots — an interrupted
    /// `building`/`failed` snapshot is invisible to it and silently holds disk (the day-2 field bug:
    /// a 4 GB non-READY snapshot never reclaimed). This deletes exactly those, reusing the SAME
    /// transactional per-snapshot cascade so the two paths cannot drift on the orphan-table set.
    ///
    /// # SAFETY — this is a raw mechanism; the daemon owns the "is it orphaned?" decision
    ///
    /// This method deletes ALL non-READY rows unconditionally. It MUST be called only when NO live
    /// write operation is writing this DB, which the daemon guarantees by (a) consulting its activity
    /// registry and (b) holding the DB-level write lock before calling (see
    /// `handlers::inventory::retention`). A live index's in-flight `building` snapshot is therefore
    /// never reachable here. READY snapshots are never touched (the `status != 'ready'` filter).
    /// Call [`vacuum`](Self::vacuum) afterwards to realise the on-disk reclaim.
    pub fn prune_non_ready_snapshots(&self, repo_uid: &str) -> Result<Vec<String>, StorageError> {
        let conn = self.connection();
        let snapshot_uids: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT snapshot_uid FROM snapshots \
                 WHERE repo_uid = ?1 AND status != 'ready'",
            )?;
            let rows = stmt.query_map(rusqlite::params![repo_uid], |row| row.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        self.delete_snapshots_cascade(&snapshot_uids)?;
        Ok(snapshot_uids)
    }

    /// Reclaim free pages back to the filesystem by rewriting the database file (`VACUUM`).
    ///
    /// SQLite does NOT shrink the file on `DELETE` — freed pages are reused by later writes but the
    /// file stays the same size. `VACUUM` compacts the file so the space a pruned snapshot held is
    /// actually returned to the OS (the operator's field complaint was a 4 GB partial holding disk).
    ///
    /// # Why the journal-mode round-trip (WAL subtlety)
    ///
    /// In WAL mode a plain `VACUUM` writes the compacted image into the **WAL**, so the main DB file
    /// is not shrunk until a later checkpoint — a `std::fs::metadata(db)` reclaim measurement right
    /// after would show ZERO bytes freed. Switching to a rollback journal (`journal_mode=DELETE`) for
    /// the `VACUUM` makes it rewrite + TRUNCATE the main file directly, so the reclaim is realised on
    /// disk immediately; WAL is then restored. (The next `StorageConnection::open` re-asserts WAL
    /// regardless, so leaving it set is belt-and-suspenders.)
    ///
    /// # Concurrency
    ///
    /// `VACUUM` and the `journal_mode` switch both require exclusive access and must not run inside a
    /// transaction. The caller must hold the DB write lock, ensure no live op is on the DB, and open
    /// no other transaction on this connection; the daemon's retention handler satisfies all three.
    pub fn vacuum(&self) -> Result<(), StorageError> {
        let conn = self.connection();
        // Rollback journal → VACUUM truncates the main file directly → restore WAL.
        conn.execute_batch("PRAGMA journal_mode = DELETE;")?;
        conn.execute_batch("VACUUM;")?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        Ok(())
    }

    /// Bytes [`vacuum`](Self::vacuum) would return to the OS if run right now:
    /// `freelist_count × page_size`.
    ///
    /// SNAPSHOT-RETENTION-1: SQLite parks pages freed by a `DELETE` (pruning a snapshot) on the
    /// **freelist** — reused by later writes but NOT returned to the OS until a `VACUUM` rewrites the
    /// file. This is the retention pass's honest "how much would a VACUUM reclaim?" gate input,
    /// measured WITHOUT paying the VACUUM, so the pass can skip the expensive full-file rewrite when
    /// the reclaimable amount is below threshold (the freed pages are simply reused by the next
    /// index). Read-only; safe to call under the DB write lock. `PRAGMA freelist_count` reflects the
    /// current logical DB state (WAL included) on this connection.
    pub fn reclaimable_bytes(&self) -> Result<u64, StorageError> {
        let conn = self.connection();
        let freelist_pages: i64 = conn.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        Ok((freelist_pages.max(0) as u64).saturating_mul(page_size.max(0) as u64))
    }

    /// Delete a set of snapshots and all their dependent rows, one snapshot per transaction.
    ///
    /// Shared by [`prune_prunable_snapshots`] (READY retention prune) and
    /// [`prune_non_ready_snapshots`] (F3 interrupted-snapshot reclaim) so the correctness-critical
    /// orphan-cleanup table set (tables lacking `ON DELETE CASCADE` on `snapshot_uid`) lives in ONE
    /// place. Per-snapshot transactions avoid giant single-statement deletes that can hang on tables
    /// with 1M+ rows; each snapshot is removed atomically. No-op on an empty input.
    fn delete_snapshots_cascade(&self, snapshot_uids: &[String]) -> Result<(), StorageError> {
        if snapshot_uids.is_empty() {
            return Ok(());
        }
        let conn = self.connection();

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

        for snapshot_uid in snapshot_uids {
            // Begin transaction for this snapshot's deletion.
            let tx = conn.unchecked_transaction()?;

            // Delete orphan rows for this specific snapshot
            for table in &orphan_cleanup_tables {
                tx.execute(
                    &format!("DELETE FROM {} WHERE snapshot_uid = ?1", table),
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

        Ok(())
    }
}
