//! Retention pruning logic.
//!
//! This module implements snapshot pruning - the deletion of snapshots
//! marked as `prunable`. Deletion is CHUNKED one transaction per snapshot
//! (`delete_snapshots_cascade`): each snapshot's cleanup is atomic, but the
//! prune as a whole is not a single transaction — see the per-snapshot
//! (not global) contract on [`StorageConnection::prune_prunable_snapshots`].
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
    /// Prune the READY snapshots marked as prunable for a repo.
    ///
    /// This deletes each prunable READY snapshot's row and all its dependent
    /// data. See the chunked, per-snapshot transactional contract below — this
    /// is deliberately NOT all-or-nothing across snapshots.
    ///
    /// # Why the `status = 'ready'` guard (DAEMON-CRASH-RECOVERY-1, review-1)
    ///
    /// The READY-retention model only ever CLASSIFIES `status='ready'` rows
    /// (`classify_repo_retention`), so historically every `retention_class =
    /// 'prunable'` row was already READY and this guard was implicit. It is now
    /// EXPLICIT because reconciliation classifies crash-orphaned NON-READY
    /// snapshots `prunable` too (so `get_retention_stats` counts them — the
    /// slice's "retention classifies them prunable"). Those orphans must be
    /// reclaimed through [`prune_non_ready_snapshots`], which is followed by a
    /// `VACUUM` that returns their disk to the OS. If this READY-retention
    /// prune (which does NOT VACUUM in the `maintenance prune` handler) deleted
    /// them first, the non-READY reclaim would find nothing and SKIP the VACUUM
    /// — re-introducing the "disk never came back" field bug. The guard keeps
    /// the two paths from colliding: classification for visibility, the
    /// non-READY path for reclaim. Behaviour-preserving for every pre-existing
    /// caller (all prior prunable rows are READY).
    ///
    /// # Transactional contract (chunked, NOT all-or-nothing)
    ///
    /// Deletion is deliberately NOT one transaction over all prunable
    /// snapshots. It is CHUNKED one transaction per snapshot
    /// (`delete_snapshots_cascade`); within each chunk the sequence is:
    /// 1. Delete this snapshot's orphan rows from tables without `ON DELETE
    ///    CASCADE` on `snapshot_uid`
    /// 2. Clear self-referencing `parent_snapshot_uid` links to this snapshot
    /// 3. Delete this snapshot's row (FK cascade removes its `nodes`/`edges`)
    /// 4. Commit — then the SQLite write lock is released before the next
    ///    snapshot, so a waiting foreground writer can interleave
    ///    (DAEMON-RESIDUALS-2 §6, "write slot released between chunks")
    ///
    /// The guarantee is therefore PER SNAPSHOT, not global:
    /// - each snapshot is removed atomically — never its row without its
    ///   dependent rows, nor its dependent rows without its row;
    /// - if a later snapshot's delete fails, the snapshots ALREADY committed
    ///   in earlier chunks STAY deleted (the prune is not rolled back
    ///   wholesale); the failing snapshot's own transaction rolls back in
    ///   full — including the orphan deletes it had already run in that same
    ///   chunk — and the error is returned; snapshots not yet reached are
    ///   left intact.
    ///
    /// Re-running the prune after a partial failure is safe: it re-selects
    /// whatever prunable snapshots still remain.
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
                 WHERE repo_uid = ?1 AND retention_class = 'prunable' AND status = 'ready'",
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

        // ── Maintenance page cache, sized to the store, scoped to this prune ──
        //
        // DAEMON-RESIDUALS-2 §6 (ratified): size the page cache to the store for the
        // duration of the prune, so the FK-cascade child-table pages (edges,
        // unresolved_edges, nodes) stay resident instead of being re-faulted from disk.
        // Measured MARGINAL on its own (374 s → 265 s on a 68 MB / 4-snapshot fixture) —
        // the migration-035 single-column FK-child indexes are the primary fix
        // (374 s → 0.84 s); the cache sizing is ratified belt-and-suspenders alongside.
        //
        // Scoped and restored: `cache_size` is a connection-level setting. This is the
        // caller-supplied per-operation maintenance connection — both production callers
        // hand us a FRESH connection opened for the maintenance op
        // (`retention_pass::run_retention_pass` via `open_existing_with_busy_retry`;
        // `handlers::inventory::retention` via `open_repo_storage_for_request`), NOT a
        // long-lived serving connection. It is still not exclusive to this delete: within
        // one retention pass the same connection also runs classify, the non-READY reclaim,
        // narrow, the reclaimable-bytes probe and (guarded) VACUUM. So we bump the cache
        // only for the delete loop and restore the prior value afterwards, so the enlarged
        // maintenance cache does not linger across the rest of that operation's work.
        let prev_cache: i64 = conn.query_row("PRAGMA cache_size", [], |row| row.get(0))?;
        let target_kib = maintenance_cache_kib(conn)?;
        conn.execute_batch(&format!("PRAGMA cache_size = -{target_kib};"))?;

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

        // Run the per-snapshot cascade with the enlarged cache, capturing the outcome so
        // the cache_size is restored on both success and failure. Per-snapshot
        // transactions are the "chunked deletes": each snapshot commits independently, so
        // the SQLite write lock is released between chunks and a waiting foreground writer
        // can interleave (DAEMON-RESIDUALS-2 §6 "write slot released between chunks").
        let result = (|| -> Result<(), StorageError> {
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
        })();

        // Restore the prior cache_size regardless of the prune outcome. `prev_cache` is
        // SQLite's own reported value (negative = KiB, positive = pages), valid to feed
        // straight back. STANDING HONESTY RULE 2: we do NOT swallow a restore failure. The
        // prune outcome is primary — if the delete loop failed, that error is returned and
        // is the meaningful one; only if the prune succeeded does a restore failure surface,
        // so a connection left with the wrong cache_size is never reported as a clean prune.
        let restore = conn
            .execute_batch(&format!("PRAGMA cache_size = {prev_cache};"))
            .map_err(StorageError::from);

        result.and(restore)
    }
}

/// Page-cache size in KiB for a maintenance prune: the store's own size, clamped to
/// `[default 2 MiB, 512 MiB]`.
///
/// DAEMON-RESIDUALS-2 §6: the maintenance connection sizes its cache to the store so the
/// FK-cascade child-table pages stay resident. The upper clamp bounds memory on a
/// multi-GB store (the operator's was 4.8 GB — an unclamped cache would try to hold the
/// whole file); the lower clamp never shrinks below SQLite's 2 MiB default.
fn maintenance_cache_kib(conn: &rusqlite::Connection) -> Result<i64, StorageError> {
    let page_count: i64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let store_kib = (page_count.max(0) as i128 * page_size.max(0) as i128) / 1024;
    const MIN_KIB: i128 = 2_000; // SQLite's default cache (2 MiB), never smaller
    const MAX_KIB: i128 = 512 * 1024; // 512 MiB ceiling, bounds memory on a multi-GB store
    Ok(store_kib.clamp(MIN_KIB, MAX_KIB) as i64)
}
