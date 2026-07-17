//! CRUD methods for the `snapshots` table.
//!
//! Mirrors `createSnapshot`, `getSnapshot`, `getLatestSnapshot`,
//! `updateSnapshotStatus`, `updateSnapshotCounts` from
//! `src/adapters/storage/sqlite/sqlite-storage.ts:248-319`.
//!
//! All five methods are single-statement operations and are NOT
//! transaction-wrapped.

use rusqlite::Connection;
use uuid::Uuid;

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::types::{CreateSnapshotInput, Snapshot, UpdateSnapshotStatusInput};

/// Initial status for new snapshots, matching the TS
/// `SnapshotStatus.BUILDING` constant from
/// `src/core/model/types.ts:121`.
const SNAPSHOT_STATUS_BUILDING: &str = "building";

/// "Latest" filter status for `get_latest_snapshot`, matching the
/// TS `SnapshotStatus.READY` constant. Per the R2-E parity
/// lock: `get_latest_snapshot` returns the latest READY snapshot
/// only, not the latest by timestamp regardless of status.
const SNAPSHOT_STATUS_READY: &str = "ready";

/// Provenance label stamped on the persisted resolved-call aggregate
/// (EC-1 M-3b): the value is PIPELINE-derived — one coherent accounting,
/// matching the trust denominator — per the ratified interim rule
/// (EC-1 §8, D-EC-1/D-EC-7 supersession clause (c)). EXPLICITLY
/// TEMPORARY: the reconciliation layer (recon-design-1) will introduce
/// its own accounting/label; readers must treat this as a label to
/// match on, never as the only value that can ever appear.
pub const RESOLVED_CALL_PROVENANCE_PIPELINE: &str = "pipeline";

impl StorageConnection {
    /// Create a new snapshot row in `BUILDING` status.
    ///
    /// Mirrors TS `createSnapshot` (sqlite-storage.ts:248).
    /// Behavior:
    ///
    ///   1. Generate a unique `snapshot_uid` of the form
    ///      `<repo_uid>/<ISO-timestamp>/<UUID-v4-prefix>`. Same
    ///      format as TS:
    ///      ```text
    ///      `${input.repoUid}/${new Date().toISOString()}/${uuidv4().slice(0, 8)}`
    ///      ```
    ///   2. Generate a `created_at` ISO timestamp.
    ///   3. INSERT the new row with status = `BUILDING` and the
    ///      counter columns left at their schema defaults (0).
    ///   4. Read the row back via `get_snapshot(uid)` and return
    ///      the DTO.
    ///
    /// The read-back is a defensive check inherited from TS: if
    /// the insert succeeded but the read returns None, something
    /// is wrong with the database state. Returns
    /// `StorageError::Sqlite` (with rusqlite::Error::QueryReturnedNoRows
    /// wrapped) in that case.
    ///
    /// **Not transaction-wrapped.** The TS adapter does not wrap
    /// either; the read-back depends on autocommit isolation. If
    /// the schema does not implicitly serialize the INSERT and
    /// the subsequent SELECT, the read-back could miss the row
    /// — but in practice both run on the same connection in
    /// SQLite's default journal mode, which serializes single-
    /// connection operations. Mirrors TS exactly.
    pub fn create_snapshot(&self, input: &CreateSnapshotInput) -> Result<Snapshot, StorageError> {
        let now = current_iso_timestamp(self.connection())?;
        let uid = format!(
            "{}/{}/{}",
            input.repo_uid,
            now,
            &Uuid::new_v4().to_string()[..8]
        );

        self.connection().execute(
			"INSERT INTO snapshots \
			 (snapshot_uid, repo_uid, parent_snapshot_uid, kind, basis_ref, basis_commit, status, created_at, label, toolchain_json) \
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
			rusqlite::params![
				uid,
				input.repo_uid,
				input.parent_snapshot_uid,
				input.kind,
				input.basis_ref,
				input.basis_commit,
				SNAPSHOT_STATUS_BUILDING,
				now,
				input.label,
				input.toolchain_json,
			],
		)?;

        // Read back. The TS code throws a generic Error if the
        // read-back fails; the Rust port surfaces it as a
        // rusqlite::Error::QueryReturnedNoRows wrapped in
        // StorageError::Sqlite.
        match self.get_snapshot(&uid)? {
            Some(s) => Ok(s),
            None => Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows)),
        }
    }

    /// Look up a snapshot by uid. Returns `Ok(None)` if not
    /// found, `Ok(Some(Snapshot))` on hit. Mirrors TS
    /// `getSnapshot` (sqlite-storage.ts:276).
    pub fn get_snapshot(&self, snapshot_uid: &str) -> Result<Option<Snapshot>, StorageError> {
        let result = self.connection().query_row(
            "SELECT * FROM snapshots WHERE snapshot_uid = ?",
            rusqlite::params![snapshot_uid],
            Snapshot::from_row,
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Sqlite(e)),
        }
    }

    /// Look up the latest READY snapshot for a repo, ordered by
    /// `created_at DESC`. Mirrors TS `getLatestSnapshot`
    /// (sqlite-storage.ts:283).
    ///
    /// **Parity-critical:** the WHERE clause includes
    /// `status = 'ready'`. Snapshots in `BUILDING`, `STALE`, or
    /// `FAILED` status are excluded. A repo with only a BUILDING
    /// snapshot returns `Ok(None)` from this method even though
    /// the snapshot exists.
    pub fn get_latest_snapshot(&self, repo_uid: &str) -> Result<Option<Snapshot>, StorageError> {
        let result = self.connection().query_row(
            "SELECT * FROM snapshots \
			 WHERE repo_uid = ? AND status = ? \
			 ORDER BY created_at DESC LIMIT 1",
            rusqlite::params![repo_uid, SNAPSHOT_STATUS_READY],
            Snapshot::from_row,
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Sqlite(e)),
        }
    }

    /// DAEMON-VISIBILITY-1 (F): list ALL snapshots for a repo (any status), newest first.
    ///
    /// Unlike [`get_latest_snapshot`] (READY-only) this returns `building` / `stale` / `failed`
    /// rows too — the whole point of F, which surfaces interrupted/partial snapshots that every
    /// existing query filters out. Read-only; the `Snapshot` DTO already carries `status`,
    /// `created_at`, `completed_at`, and the `*_total` counts, so no schema change is needed.
    pub fn list_snapshots(&self, repo_uid: &str) -> Result<Vec<Snapshot>, StorageError> {
        let conn = self.connection();
        let mut stmt =
            conn.prepare("SELECT * FROM snapshots WHERE repo_uid = ? ORDER BY created_at DESC")?;
        let rows = stmt.query_map(rusqlite::params![repo_uid], Snapshot::from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::Sqlite)
    }

    /// DAEMON-VISIBILITY-1 (F2): the latest snapshot for a repo REGARDLESS of status.
    ///
    /// Companion to [`get_latest_snapshot`] (READY-only). When orient/explain find no READY
    /// snapshot, this surfaces the newest non-READY row so the error can NAME the partial (its
    /// state, when it was created) instead of a bare "index the repo first". Read-only.
    pub fn get_latest_snapshot_any_state(
        &self,
        repo_uid: &str,
    ) -> Result<Option<Snapshot>, StorageError> {
        let result = self.connection().query_row(
            "SELECT * FROM snapshots \
			 WHERE repo_uid = ? \
			 ORDER BY created_at DESC LIMIT 1",
            rusqlite::params![repo_uid],
            Snapshot::from_row,
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Sqlite(e)),
        }
    }

    /// Update a snapshot's status and completed_at timestamp.
    /// Mirrors TS `updateSnapshotStatus` (sqlite-storage.ts:296).
    ///
    /// `completed_at` defaults to the current ISO timestamp when
    /// `input.completed_at` is `None`. Matches TS
    /// `input.completedAt ?? new Date().toISOString()`.
    pub fn update_snapshot_status(
        &self,
        input: &UpdateSnapshotStatusInput,
    ) -> Result<(), StorageError> {
        let completed_at = match &input.completed_at {
            Some(ts) => ts.clone(),
            None => current_iso_timestamp(self.connection())?,
        };
        self.connection().execute(
            "UPDATE snapshots SET status = ?, completed_at = ? WHERE snapshot_uid = ?",
            rusqlite::params![input.status, completed_at, input.snapshot_uid],
        )?;
        Ok(())
    }

    /// Recompute and update the three counter columns
    /// (`files_total`, `nodes_total`, `edges_total`) for a
    /// snapshot from actual `COUNT(*)` queries against
    /// `file_versions`, `nodes`, `edges`.
    ///
    /// Mirrors TS `updateSnapshotCounts` (sqlite-storage.ts:309).
    /// The TS code uses a single UPDATE with three correlated
    /// subqueries; the Rust port mirrors that exact SQL shape.
    ///
    /// Single statement (one UPDATE with three SELECT subqueries),
    /// not transaction-wrapped.
    pub fn update_snapshot_counts(&self, snapshot_uid: &str) -> Result<(), StorageError> {
        self.connection().execute(
            "UPDATE snapshots SET \
			   files_total = (SELECT COUNT(*) FROM file_versions WHERE snapshot_uid = ?), \
			   nodes_total = (SELECT COUNT(*) FROM nodes WHERE snapshot_uid = ?), \
			   edges_total = (SELECT COUNT(*) FROM edges WHERE snapshot_uid = ?) \
			 WHERE snapshot_uid = ?",
            rusqlite::params![snapshot_uid, snapshot_uid, snapshot_uid, snapshot_uid],
        )?;
        Ok(())
    }

    /// Persist the snapshot-level resolved-call aggregate (EC-1 M-3b, g1).
    ///
    /// Stores the SUPPLIED count — the index/refresh pipeline counts
    /// resolved CALLS results in the resolver's OUTPUT stream, before any
    /// storage materialization — and stamps the ratified interim-rule
    /// provenance label ([`RESOLVED_CALL_PROVENANCE_PIPELINE`]).
    ///
    /// Deliberately NOT derived from the `edges` table: after a
    /// per-language CALLS-row drop (EC-1 M-6), `edges` holds a FILTERED
    /// subset of the resolution stream, and any `COUNT(*)` over it would
    /// bake that undercount into the persisted value — the exact failure
    /// M-3b exists to prevent. While no filtering exists (pre-M-6),
    /// supplied-count-vs-live-COUNT parity is asserted by the validation
    /// suite; post-M-6 the two legitimately diverge and the persisted
    /// value is the honest one.
    ///
    /// Write census for the aggregate columns (both writers live in THIS
    /// file so the census stays auditable):
    /// - this function — `run_pipeline` Phase-5 finalization (fresh index
    ///   AND delta refresh share it; the delta path re-resolves ALL
    ///   extraction edges, copied-forward + fresh, so the supplied value
    ///   is full-stream and language-complete),
    /// - [`adjust_resolved_call_aggregate`] — the enrichment promotion
    ///   transaction, which adjusts the aggregate by promotion's net
    ///   CALLS-row delta INSIDE the same transaction as the row mutations
    ///   (coherent on every success and failure exit).
    ///
    /// Deliberately NOT folded into [`Self::update_snapshot_counts`]: the
    /// enrichment tail must adjust THIS aggregate without changing the
    /// (currently promotion-stale) `files/nodes/edges_total` semantics.
    pub fn persist_resolved_call_aggregate(
        &self,
        snapshot_uid: &str,
        resolved_call_count: u64,
    ) -> Result<(), StorageError> {
        // The count comes from `len()` sums over in-memory resolver output;
        // a value above i64::MAX cannot arise on a real machine. Assert
        // rather than silently wrap — a wrapped negative would be
        // fabricated data (the read side rejects negatives as invalid).
        let count = i64::try_from(resolved_call_count)
            .expect("resolved-call count exceeds i64::MAX — impossible for a len()-derived count");
        self.connection().execute(
            "UPDATE snapshots SET \
			   resolved_call_count = ?, \
			   resolved_call_provenance = ? \
			 WHERE snapshot_uid = ?",
            rusqlite::params![count, RESOLVED_CALL_PROVENANCE_PIPELINE, snapshot_uid],
        )?;
        Ok(())
    }

    /// DAEMON-CRASH-RECOVERY-1 (F7/F11): mark a crash-orphaned `building` snapshot as interrupted.
    ///
    /// A `building` snapshot with no live writer never finalized — the daemon died mid-index, or the
    /// machine slept. This flips it to the existing terminal `failed` state AND classifies it
    /// `retention_class = 'prunable'`, so the orphan stops reading as "in progress" and becomes a
    /// first-class prunable NON-READY snapshot: `snapshot_facts` renders it "interrupted",
    /// `get_retention_stats` counts it in `prunable` (doctor / `maintenance prune` name it), and BOTH
    /// the F3 `prune_non_ready_snapshots` reclaim (which also VACUUMs) and the auto-retention pass
    /// reclaim it. Reuses INDEX-DISCONNECT-1's terminal `failed` state AND the existing `prunable`
    /// class — no new status vocabulary, no schema migration.
    ///
    /// # Why it sets `retention_class = 'prunable'` — and why that is still VACUUM-safe (review-1)
    ///
    /// The slice's acceptance criterion is "retention classifies them prunable, prune reclaims" and the
    /// reconciliation proof asserts the prunable STAT before any reclaim. Setting the class HERE is the
    /// only way a non-READY snapshot is ever classified, because `classify_repo_retention` only ever
    /// touches `status='ready'` rows — so without this write the orphan would stay `retention_class IS
    /// NULL` (invisible to every retention class, the field bug: `total 3, all classes 0`).
    ///
    /// This does NOT re-introduce the "disk never came back" bug, because
    /// [`StorageConnection::prune_prunable_snapshots`] is guarded to `status='ready'`: the READY-
    /// retention prune (which does NOT VACUUM in the `maintenance prune` handler) therefore never
    /// deletes this `failed` orphan out from under the non-READY reclaim. The orphan is reclaimed
    /// EXCLUSIVELY through the non-READY path (`prune_non_ready_snapshots` + `vacuum`) in BOTH the
    /// `maintenance prune` handler and the auto-retention pass, so the disk is always returned to the
    /// OS. Classification is for VISIBILITY; the non-READY path is for RECLAIM; the `status='ready'`
    /// guard is what keeps the two from colliding.
    ///
    /// # Why `completed_at` is left NULL
    ///
    /// A crash has no honest completion time; stamping "now" would render as a completion that never
    /// happened. `snapshot_outcome` handles a `failed` row with a NULL `completed_at` ("interrupted
    /// … "). The `reason` (e.g. "daemon restart") is NOT a fabricated `completed_at`; it is recorded
    /// durably in the extraction-diagnostics blob (see below), so the reader-frame render can say
    /// "interrupted — daemon restart, reconciled <time>" without inventing a completion timestamp.
    ///
    /// # Why the reason is recorded (operator resolution: Option B, no migration)
    ///
    /// The `snapshots` table has no reason column and this slice forbids a schema migration, so the
    /// interruption reason is merged into the EXISTING `extraction_diagnostics_json` blob (an
    /// additive `interrupted` key — see [`merge_interruption_annotation`]). That makes the reason a
    /// durable current-state fact that survives daemon-log rotation and renders on doctor / repo-info
    /// / orient; the F8 daemon-log line remains a parallel forensic trail. The status flip and the
    /// reason write are ONE atomic, guarded UPDATE (below), so the reason is GUARANTEED whenever the
    /// flip happens — NOT a best-effort second write whose failure could leave a `failed` orphan with
    /// no recorded reason (review-1 change #4: the prior split write swallowed its error with `let _`).
    ///
    /// # Safety — the `status = 'building'` guard
    ///
    /// The WHERE clause requires `status = 'building'`, so if the index finalized (to `ready` or
    /// `failed`) between the caller's enumeration and this UPDATE, the statement is a NO-OP: a
    /// completed snapshot is never clobbered (neither its status NOR its diagnostics blob — the WHERE
    /// excludes the row, so the computed merge is simply discarded). The daemon additionally calls this
    /// only under the two-gate rule (no live op on the DB + the DB write lock held). Returns `true` iff
    /// a row was flipped — and a `true` return therefore GUARANTEES the durable reason landed with it
    /// (so the daemon logs only snapshots it actually reconciled + annotated).
    pub fn mark_snapshot_interrupted(
        &self,
        snapshot_uid: &str,
        reason: &str,
    ) -> Result<bool, StorageError> {
        // An honest reconciliation timestamp (when this daemon repaired the orphan) — NOT a completion
        // time. Same ISO format as `created_at`/`completed_at`.
        let reconciled_at = current_iso_timestamp(self.connection())?;
        // Read the existing blob so the reason MERGES rather than clobbers: PERSIST-RECURSION-1 writes
        // extraction diagnostics at finalize (right before the READY flip), and a crash in that narrow
        // window can leave a populated blob on a `building` orphan; erasing it would violate "do not
        // erase computed facts". A NULL column (the common case — the crash preceded finalize) and a
        // QueryReturnedNoRows both degrade to "no prior diagnostics". A genuine read error propagates
        // (non-silent) rather than risking a lossy overwrite.
        let existing: Option<String> = match self.connection().query_row(
            "SELECT extraction_diagnostics_json FROM snapshots WHERE snapshot_uid = ?",
            rusqlite::params![snapshot_uid],
            |row| row.get::<_, Option<String>>(0),
        ) {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(StorageError::Sqlite(e)),
        };
        let merged = merge_interruption_annotation(existing.as_deref(), reason, &reconciled_at);
        // Flip status, classify `prunable`, AND record the reason in ONE guarded, atomic UPDATE.
        // Because all three are the SAME write, a returned `Ok(true)` GUARANTEES the durable reason AND
        // the prunable classification landed together — no best-effort second write (the prior split
        // write's `let _` swallow was the review-1 defect). The `status = 'building'` guard makes the
        // whole statement a no-op (0 rows) if the snapshot finalized concurrently, so a completed
        // snapshot is never flipped, reclassified, NOR annotated.
        let affected = self.connection().execute(
            "UPDATE snapshots SET status = 'failed', retention_class = 'prunable', \
             extraction_diagnostics_json = ?1 \
             WHERE snapshot_uid = ?2 AND status = 'building'",
            rusqlite::params![merged, snapshot_uid],
        )?;
        Ok(affected > 0)
    }
}

/// Merge a DAEMON-CRASH-RECOVERY-1 interruption annotation into a snapshot's existing
/// `extraction_diagnostics_json` blob WITHOUT clobbering any Layer-0 extraction diagnostics already
/// there, returning the merged JSON string to persist.
///
/// PERSIST-RECURSION-1 writes that blob at finalize (right before the READY flip); a crash in the
/// narrow window after that write but before the flip would leave a populated blob on a `building`
/// orphan, so [`StorageConnection::mark_snapshot_interrupted`] reads-merges-writes rather than
/// overwriting (`update_snapshot_extraction_diagnostics` is a raw column overwrite). The blob is
/// free-form JSON and the typed `ExtractionDiagnostics` reader ignores unknown keys (no
/// `deny_unknown_fields`), so the added `interrupted` key is purely additive for every existing
/// reader — the same property `snapshot_facts::extraction_degradations` already relies on.
///
/// Shape: `{"interrupted": {"reason": <reason>, "reconciled_at": <iso>}}` merged over any existing
/// object; `snapshot_facts::interruption_reason` parses it back for the reader-frame render.
///
/// PURE (no I/O) — the caller does the single atomic SELECT-merge-UPDATE — so the merge is
/// unit-testable in isolation. A non-object or unparseable existing blob starts fresh (never an error:
/// this is forensic metadata and a corrupt prior blob is not worth failing reconciliation over).
fn merge_interruption_annotation(
    existing: Option<&str>,
    reason: &str,
    reconciled_at: &str,
) -> String {
    let mut obj = existing
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| match v {
            serde_json::Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default();
    obj.insert(
        "interrupted".to_string(),
        serde_json::json!({ "reason": reason, "reconciled_at": reconciled_at }),
    );
    serde_json::Value::Object(obj).to_string()
}

/// Generate an ISO 8601 / RFC 3339 timestamp for the current
/// instant in the **exact format** that JavaScript's
/// `new Date().toISOString()` produces:
///
/// ```text
/// YYYY-MM-DDTHH:MM:SS.sssZ
/// ```
///
/// Example: `2025-01-01T12:34:56.789Z`. 24 characters, ASCII
/// only, UTC ('Z' suffix), millisecond precision.
///
/// Implemented via SQLite's `strftime` function with the
/// `%Y-%m-%dT%H:%M:%fZ` format string. SQLite's `%f` substitution
/// produces `SS.SSS` (seconds with three fractional digits), and
/// SQLite's `'now'` modifier always returns UTC, so the literal
/// `Z` suffix is correct without any timezone conversion.
///
/// **Why this matters (R2-E parity correction):**
///
/// The TypeScript adapter persists snapshot timestamps via
/// `new Date().toISOString()` in BOTH the `created_at` /
/// `completed_at` columns AND the `snapshot_uid` itself
/// (`<repo_uid>/<iso-timestamp>/<uuid-prefix>` per
/// `docs/architecture/schema.txt:23`). An earlier version of this
/// helper used SQLite's `datetime('now')` which produces
/// `YYYY-MM-DD HH:MM:SS` (no T separator, no fractional seconds,
/// no Z). That was a real contract violation, not a cosmetic
/// difference:
///
///   1. `snapshot_uid` is part of the schema's portable identity
///      strategy. The format is locked in `schema.txt`.
///   2. `'T'` (ASCII 0x54) sorts AFTER `' '` (ASCII 0x20). A
///      database with mixed TS-written and Rust-written rows
///      would order incorrectly under `ORDER BY created_at DESC`,
///      and `get_latest_snapshot` could return the wrong row.
///   3. Any tooling that parses these timestamps with strict ISO
///      8601 expectations would reject the SQLite-format strings.
///
/// The corrected helper produces byte-equivalent output to TS for
/// every timestamp it generates. The pinning test
/// `current_iso_timestamp_matches_js_to_iso_string_format`
/// catches regressions.
///
/// **Why not add a `chrono` or `time` dep:** the SQLite
/// `strftime` approach is zero-dep, runs on the connection we
/// already have, and produces the exact byte format we need. A
/// time crate would add a dep, a parsing surface, and timezone-
/// handling complexity for no functional gain at this layer.
///
/// **Asymmetry with `record_migration`:** the migration runner's
/// `record_migration` helper (in `migrations/mod.rs`) uses
/// SQLite's `datetime('now')` to set `schema_migrations.applied_at`.
/// That is intentional and matches the TS adapter exactly: every
/// TS migration file inserts its row with `datetime('now')`, not
/// with `toISOString()`. The TS database therefore has TWO
/// timestamp formats coexisting: SQLite-format for
/// `schema_migrations.applied_at` and ISO 8601 for everything
/// snapshot-related. The Rust port mirrors this asymmetry exactly.
fn current_iso_timestamp(conn: &Connection) -> Result<String, StorageError> {
    let ts = conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
        row.get::<_, String>(0)
    })?;
    Ok(ts)
}

/// Adjust the persisted resolved-call aggregate by a net delta
/// (EC-1 M-3b — the enrichment-promotion writer; see the write census on
/// [`StorageConnection::persist_resolved_call_aggregate`]).
///
/// Takes a raw [`Connection`] so the caller can pass a
/// `rusqlite::Transaction` (derefs to `Connection`): the promotion adapter
/// MUST run this in the SAME transaction as its edge-row mutations, so the
/// aggregate and the rows commit or roll back together — the aggregate is
/// never stale relative to a partial mutation.
///
/// `resolved_call_count + delta` is NULL-propagating by SQL semantics:
/// a snapshot with NO persisted aggregate (pre-migration; NULL) stays
/// NULL — explicitly unavailable, so the trust core's labeled live-COUNT
/// fallback applies. It is NEVER seeded here: seeding would require
/// deriving a base from the `edges` table, which is exactly the
/// filtered-subset accounting M-3b removes. Provenance is deliberately
/// untouched: it is already stamped when a count exists and stays NULL
/// when none does.
pub(crate) fn adjust_resolved_call_aggregate(
    conn: &Connection,
    snapshot_uid: &str,
    delta: i64,
) -> Result<(), StorageError> {
    conn.execute(
        "UPDATE snapshots SET \
		   resolved_call_count = resolved_call_count + ? \
		 WHERE snapshot_uid = ?",
        rusqlite::params![delta, snapshot_uid],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::test_helpers::{
        fresh_storage, make_edge, make_file, make_file_version, make_node, make_repo,
    };

    fn create_test_snapshot(storage: &StorageConnection) -> Snapshot {
        storage
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: "r1".to_string(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: Some("abc123".to_string()),
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
    }

    #[test]
    fn create_snapshot_returns_dto_with_building_status() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();

        let snap = create_test_snapshot(&storage);

        assert_eq!(snap.repo_uid, "r1");
        assert_eq!(snap.status, SNAPSHOT_STATUS_BUILDING);
        assert_eq!(snap.kind, "full");
    }

    #[test]
    fn create_snapshot_generates_unique_uid_with_repo_prefix() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();

        let snap1 = create_test_snapshot(&storage);
        // Sleep is not needed; UUID prefix randomization makes
        // collision astronomically unlikely.
        let snap2 = create_test_snapshot(&storage);

        assert_ne!(snap1.snapshot_uid, snap2.snapshot_uid);
        // UID format: <repo_uid>/<timestamp>/<uuid_prefix>
        assert!(snap1.snapshot_uid.starts_with("r1/"));
        assert!(snap2.snapshot_uid.starts_with("r1/"));
    }

    #[test]
    fn get_snapshot_returns_none_for_nonexistent() {
        let storage = fresh_storage();
        let result = storage.get_snapshot("nope").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_snapshot_status_changes_status() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);

        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: snap.snapshot_uid.clone(),
                status: SNAPSHOT_STATUS_READY.to_string(),
                completed_at: None,
            })
            .unwrap();

        let updated = storage.get_snapshot(&snap.snapshot_uid).unwrap().unwrap();
        assert_eq!(updated.status, SNAPSHOT_STATUS_READY);
        assert!(updated.completed_at.is_some());
    }

    #[test]
    fn get_latest_snapshot_excludes_building_snapshots() {
        // PARITY-CRITICAL: getLatestSnapshot must filter by status=READY,
        // not just by ORDER BY created_at. A repo with only a BUILDING
        // snapshot returns None.
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let _snap = create_test_snapshot(&storage);

        let latest = storage.get_latest_snapshot("r1").unwrap();
        assert!(
            latest.is_none(),
            "BUILDING snapshot must NOT be returned by get_latest_snapshot"
        );
    }

    // DAEMON-VISIBILITY-1 (F): `list_snapshots` returns EVERY state (the point of F — surfacing
    // interrupted/partial snapshots that the READY-only queries hide).
    #[test]
    fn list_snapshots_returns_all_states() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        // One building (interrupted) + one ready.
        let _building = create_test_snapshot(&storage);
        let ready = create_test_snapshot(&storage);
        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: ready.snapshot_uid.clone(),
                status: SNAPSHOT_STATUS_READY.to_string(),
                completed_at: None,
            })
            .unwrap();

        let all = storage.list_snapshots("r1").unwrap();
        assert_eq!(
            all.len(),
            2,
            "list_snapshots must return non-READY rows too"
        );
        let statuses: Vec<&str> = all.iter().map(|s| s.status.as_str()).collect();
        assert!(statuses.contains(&SNAPSHOT_STATUS_BUILDING));
        assert!(statuses.contains(&SNAPSHOT_STATUS_READY));
    }

    // DAEMON-VISIBILITY-1 (F2): the latest snapshot regardless of state names the partial that
    // `get_latest_snapshot` (READY-only) returns None for.
    #[test]
    fn get_latest_snapshot_any_state_returns_the_interrupted_partial() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let building = create_test_snapshot(&storage);

        // READY-only view sees nothing…
        assert!(storage.get_latest_snapshot("r1").unwrap().is_none());
        // …but the any-state view surfaces the interrupted (building) snapshot.
        let any = storage.get_latest_snapshot_any_state("r1").unwrap();
        let any = any.expect("any-state latest must surface the building snapshot");
        assert_eq!(any.snapshot_uid, building.snapshot_uid);
        assert_eq!(any.status, SNAPSHOT_STATUS_BUILDING);
    }

    #[test]
    fn get_latest_snapshot_returns_ready_snapshot_after_status_update() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);

        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: snap.snapshot_uid.clone(),
                status: SNAPSHOT_STATUS_READY.to_string(),
                completed_at: None,
            })
            .unwrap();

        let latest = storage.get_latest_snapshot("r1").unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().snapshot_uid, snap.snapshot_uid);
    }

    // ── Timestamp format parity (regression pin) ──────────────

    #[test]
    fn current_iso_timestamp_matches_js_to_iso_string_format() {
        // Pins the R2-E parity correction.
        //
        // JavaScript `new Date().toISOString()` produces strings
        // of the exact form `YYYY-MM-DDTHH:MM:SS.sssZ`:
        //
        //   - 24 ASCII characters
        //   - '-' at positions 4 and 7
        //   - 'T' at position 10
        //   - ':' at positions 13 and 16
        //   - '.' at position 19
        //   - 'Z' at position 23
        //
        // An earlier version of `current_iso_timestamp` used
        // SQLite's `datetime('now')` which produces a different
        // format (`YYYY-MM-DD HH:MM:SS`, 19 chars, space
        // separator). That was a real contract violation because
        // snapshot_uid embeds this timestamp and ORDER BY
        // created_at would sort incorrectly across mixed
        // TS-written and Rust-written rows.
        //
        // This test pins the corrected format. If a future
        // maintainer reverts to `datetime('now')` or any other
        // format that does not match toISOString(), this test
        // fails immediately.
        let storage = fresh_storage();
        let ts = current_iso_timestamp(storage.connection())
            .expect("current_iso_timestamp must succeed");

        assert_eq!(
            ts.len(),
            24,
            "toISOString() format is exactly 24 chars, got {} chars: {:?}",
            ts.len(),
            ts
        );
        let bytes = ts.as_bytes();
        assert_eq!(bytes[4], b'-', "expected '-' at position 4 in {:?}", ts);
        assert_eq!(bytes[7], b'-', "expected '-' at position 7 in {:?}", ts);
        assert_eq!(
			bytes[10], b'T',
			"expected 'T' (NOT space) at position 10 in {:?}; if this fails the format reverted to SQLite datetime('now')",
			ts
		);
        assert_eq!(bytes[13], b':', "expected ':' at position 13 in {:?}", ts);
        assert_eq!(bytes[16], b':', "expected ':' at position 16 in {:?}", ts);
        assert_eq!(
            bytes[19], b'.',
            "expected '.' (fractional seconds separator) at position 19 in {:?}",
            ts
        );
        assert_eq!(
            bytes[23], b'Z',
            "expected 'Z' (Zulu/UTC suffix) at position 23 in {:?}",
            ts
        );
        // Year, month, day digit positions.
        for pos in [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22] {
            assert!(
                (bytes[pos] as char).is_ascii_digit(),
                "expected ASCII digit at position {} in {:?}",
                pos,
                ts
            );
        }
    }

    #[test]
    fn create_snapshot_uid_includes_iso_timestamp_with_t_separator() {
        // Cross-check: the snapshot UID format
        // `<repo_uid>/<iso-timestamp>/<uuid-prefix>` must contain
        // the ISO 8601 'T' separator in the timestamp segment.
        // Pins the schema.txt:23 contract that snapshot_uid uses
        // an ISO timestamp.
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);

        // Format: r1/<iso-timestamp>/<uuid-prefix>
        // Splitting on '/' should yield exactly 3 parts.
        let parts: Vec<&str> = snap.snapshot_uid.split('/').collect();
        assert_eq!(parts.len(), 3, "snapshot_uid format is r1/<ts>/<uuid>");
        assert_eq!(parts[0], "r1");

        // The middle segment must contain a 'T' separator
        // (proves it's the ISO 8601 format, not SQLite format
        // which uses a space — and a space would not appear in
        // the segment because we split on '/').
        assert!(
            parts[1].contains('T'),
            "snapshot_uid timestamp segment must contain 'T' separator, got: {:?}",
            parts[1]
        );
        assert!(
            parts[1].ends_with('Z'),
            "snapshot_uid timestamp segment must end with 'Z' (UTC), got: {:?}",
            parts[1]
        );

        // The created_at column must also contain the same format.
        assert!(
            snap.created_at.contains('T'),
            "created_at must be ISO 8601 with T separator, got: {:?}",
            snap.created_at
        );
        assert!(
            snap.created_at.ends_with('Z'),
            "created_at must end with Z (UTC), got: {:?}",
            snap.created_at
        );
    }

    /// Read a snapshot's raw `extraction_diagnostics_json` blob (test-only, via the crate-internal
    /// connection accessor). `None` when the column is NULL.
    fn diagnostics_blob(storage: &StorageConnection, uid: &str) -> Option<String> {
        storage
            .connection()
            .query_row(
                "SELECT extraction_diagnostics_json FROM snapshots WHERE snapshot_uid = ?",
                rusqlite::params![uid],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap()
    }

    // DAEMON-CRASH-RECOVERY-1 (F7/F11 + review-1): a crash-orphaned `building` snapshot is flipped to
    // the terminal `failed` state AND classified `prunable` (`get_retention_stats` counts it — the
    // slice's "retention classifies them prunable", asserted BEFORE reclaim). It is reclaimed through
    // the non-READY (VACUUM) path; the READY-retention prune is guarded off it so the disk still comes
    // back.
    #[test]
    fn mark_snapshot_interrupted_flips_building_to_failed_prunable_and_reclaims() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let building = create_test_snapshot(&storage); // status = building

        let flipped = storage
            .mark_snapshot_interrupted(&building.snapshot_uid, "daemon restart")
            .unwrap();
        assert!(flipped, "a building snapshot is flipped");

        let after = storage
            .get_snapshot(&building.snapshot_uid)
            .unwrap()
            .unwrap();
        assert_eq!(
            after.status, "failed",
            "status is the terminal failed state"
        );
        assert!(
            after.completed_at.is_none(),
            "a crash has no honest completion time — completed_at stays NULL"
        );

        // review-1 (the blocking mismatch): the orphan is CLASSIFIED prunable and the stat counts it
        // BEFORE any reclaim — so doctor / `maintenance prune` name it, never an "empty store".
        let stats = storage.get_retention_stats("r1").unwrap();
        assert_eq!(
            stats.prunable, 1,
            "the reconciled orphan is counted as prunable: {stats:?}"
        );
        assert_eq!(
            stats.unclassified, 0,
            "it is no longer `retention_class IS NULL` (the field bug was total 3 / all classes 0)"
        );
        assert_eq!(stats.total, 1);

        // VACUUM-safety (Change 2): the READY-retention prune is guarded to `status='ready'`, so it
        // does NOT delete this `failed` orphan out from under the non-READY reclaim (which VACUUMs).
        assert_eq!(
            storage.prune_prunable_snapshots("r1").unwrap(),
            0,
            "prune_prunable leaves the non-READY orphan for the VACUUM path"
        );

        // DAEMON-CRASH-RECOVERY-1 (operator resolution: Option B): the reason is DURABLE in the
        // extraction-diagnostics blob (survives log rotation), with an honest `reconciled_at` that is
        // NOT a fabricated completion time.
        let blob =
            diagnostics_blob(&storage, &building.snapshot_uid).expect("interrupted blob written");
        let v: serde_json::Value = serde_json::from_str(&blob).unwrap();
        assert_eq!(
            v["interrupted"]["reason"], "daemon restart",
            "durable reason: {blob}"
        );
        assert!(
            v["interrupted"]["reconciled_at"]
                .as_str()
                .is_some_and(|s| s.ends_with('Z')),
            "durable reconciled-at timestamp (ISO Z), not a completion time: {blob}"
        );

        // The orphan is NON-READY, so the F3 reclaim (the VACUUM path) deletes it; a fresh READY
        // snapshot would survive.
        let reclaimed = storage.prune_non_ready_snapshots("r1").unwrap();
        assert_eq!(
            reclaimed,
            vec![building.snapshot_uid],
            "the interrupted orphan is reclaimed through the non-READY (VACUUM) path"
        );
    }

    // DAEMON-CRASH-RECOVERY-1: the `status='building'` guard — a snapshot that finalized (to READY)
    // between the daemon's enumeration and the flip is NEVER clobbered.
    #[test]
    fn mark_snapshot_interrupted_never_clobbers_a_finalized_snapshot() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);
        storage
            .update_snapshot_status(&UpdateSnapshotStatusInput {
                snapshot_uid: snap.snapshot_uid.clone(),
                status: SNAPSHOT_STATUS_READY.to_string(),
                completed_at: None,
            })
            .unwrap();

        let flipped = storage
            .mark_snapshot_interrupted(&snap.snapshot_uid, "daemon restart")
            .unwrap();
        assert!(!flipped, "a finalized snapshot is never flipped");
        let after = storage.get_snapshot(&snap.snapshot_uid).unwrap().unwrap();
        assert_eq!(after.status, SNAPSHOT_STATUS_READY, "ready is preserved");
        // The guard also protects the blob: a READY snapshot is never annotated "interrupted".
        assert!(
            diagnostics_blob(&storage, &snap.snapshot_uid).is_none(),
            "no interrupted reason is written onto a finalized snapshot"
        );
        // …and it protects the CLASS: a finalized (READY) snapshot is never reclassified `prunable`
        // by a racing reconcile (the single guarded UPDATE writes status + class + reason together).
        assert_eq!(
            storage.get_retention_stats("r1").unwrap().prunable,
            0,
            "a READY snapshot is not reclassified prunable"
        );
    }

    // DAEMON-CRASH-RECOVERY-1: recording the interruption reason MERGES into the existing
    // extraction-diagnostics blob — it never clobbers Layer-0 diagnostics that PERSIST-RECURSION-1
    // wrote in the narrow crash-after-finalize-write window.
    #[test]
    fn mark_snapshot_interrupted_preserves_existing_extraction_diagnostics() {
        use repo_graph_indexer::storage_port::SnapshotLifecyclePort;

        let mut storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let building = create_test_snapshot(&storage); // status = building
                                                       // Simulate the narrow window: a finalize-time diagnostics blob already landed on the orphan.
        let prior = r#"{"diagnostics_version":1,"edges_total":100,"unresolved_total":2,"boundary_facts_files_skipped_deep_nesting":3}"#;
        SnapshotLifecyclePort::update_snapshot_extraction_diagnostics(
            &mut storage,
            &building.snapshot_uid,
            prior,
        )
        .unwrap();

        assert!(storage
            .mark_snapshot_interrupted(&building.snapshot_uid, "daemon restart")
            .unwrap());

        let blob = diagnostics_blob(&storage, &building.snapshot_uid).expect("blob present");
        let v: serde_json::Value = serde_json::from_str(&blob).unwrap();
        // The interruption reason is present…
        assert_eq!(v["interrupted"]["reason"], "daemon restart", "{blob}");
        // …AND the pre-existing extraction diagnostics survive (not clobbered).
        assert_eq!(
            v["diagnostics_version"], 1,
            "existing key preserved: {blob}"
        );
        assert_eq!(v["edges_total"], 100, "existing key preserved: {blob}");
        assert_eq!(
            v["boundary_facts_files_skipped_deep_nesting"], 3,
            "the PERSIST-RECURSION-1 degradation key survives the merge: {blob}"
        );
    }

    // DAEMON-CRASH-RECOVERY-1 (review-1 change #4): the PURE merge helper now composes the exact string
    // the single atomic UPDATE writes — merge over an existing object, start fresh on NULL/corrupt,
    // never lose a prior key. Unit-testable without a DB precisely because the reason write is no longer
    // a separate best-effort call.
    #[test]
    fn merge_interruption_annotation_merges_not_clobbers() {
        // NULL / absent prior blob → a fresh object carrying only the interruption.
        let fresh = merge_interruption_annotation(None, "daemon restart", "2026-07-02T11:00:00Z");
        let v: serde_json::Value = serde_json::from_str(&fresh).unwrap();
        assert_eq!(v["interrupted"]["reason"], "daemon restart");
        assert_eq!(v["interrupted"]["reconciled_at"], "2026-07-02T11:00:00Z");

        // Existing Layer-0 diagnostics are preserved beside the new `interrupted` key.
        let prior = r#"{"diagnostics_version":1,"edges_total":100}"#;
        let merged = merge_interruption_annotation(Some(prior), "daemon restart", "t");
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["diagnostics_version"], 1, "prior key preserved: {merged}");
        assert_eq!(v["edges_total"], 100, "prior key preserved: {merged}");
        assert_eq!(v["interrupted"]["reason"], "daemon restart");

        // A non-object / corrupt prior blob starts fresh rather than failing (forensic metadata is not
        // worth losing the flip over) — the annotation is still recorded.
        for junk in ["not json at all", "[1,2,3]", "42"] {
            let v: serde_json::Value = serde_json::from_str(&merge_interruption_annotation(
                Some(junk),
                "daemon restart",
                "t",
            ))
            .unwrap();
            assert_eq!(
                v["interrupted"]["reason"], "daemon restart",
                "corrupt prior blob ({junk}) still yields the interruption annotation"
            );
        }
    }

    #[test]
    fn update_snapshot_counts_recomputes_from_actual_data() {
        let mut storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);

        // Insert a file, file_version, two nodes, and one edge
        // so we have non-zero counts to verify against.
        let file = make_file("r1", "src/a.ts");
        storage.upsert_files(std::slice::from_ref(&file)).unwrap();
        storage
            .upsert_file_versions(&[make_file_version(&snap.snapshot_uid, &file.file_uid)])
            .unwrap();

        let node_a = make_node(
            "node-a",
            &snap.snapshot_uid,
            "r1",
            "r1:src/a.ts#fnA:SYMBOL:FUNCTION",
            &file.file_uid,
            "fnA",
        );
        let node_b = make_node(
            "node-b",
            &snap.snapshot_uid,
            "r1",
            "r1:src/a.ts#fnB:SYMBOL:FUNCTION",
            &file.file_uid,
            "fnB",
        );
        storage
            .insert_nodes(&[node_a.clone(), node_b.clone()])
            .unwrap();

        let edge = make_edge(
            "edge-1",
            &snap.snapshot_uid,
            "r1",
            &node_a.node_uid,
            &node_b.node_uid,
        );
        storage.insert_edges(&[edge]).unwrap();

        // Pre-recompute: counts should still be 0 (default).
        let pre = storage.get_snapshot(&snap.snapshot_uid).unwrap().unwrap();
        assert_eq!(pre.files_total, 0);
        assert_eq!(pre.nodes_total, 0);
        assert_eq!(pre.edges_total, 0);

        // Recompute.
        storage.update_snapshot_counts(&snap.snapshot_uid).unwrap();

        // Post-recompute: counts match actual data.
        let post = storage.get_snapshot(&snap.snapshot_uid).unwrap().unwrap();
        assert_eq!(post.files_total, 1);
        assert_eq!(post.nodes_total, 2);
        assert_eq!(post.edges_total, 1);
    }

    /// Read the persisted g1 aggregate columns raw (test-side observer —
    /// production reads go through `TrustStorageRead::get_resolved_call_aggregate`).
    fn read_aggregate_columns(
        storage: &StorageConnection,
        snapshot_uid: &str,
    ) -> (Option<i64>, Option<String>) {
        storage
            .connection()
            .query_row(
                "SELECT resolved_call_count, resolved_call_provenance \
                 FROM snapshots WHERE snapshot_uid = ?",
                rusqlite::params![snapshot_uid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    #[test]
    fn persist_resolved_call_aggregate_stores_supplied_count_and_stamps_provenance() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);

        // Pre-persist: NULL (not persisted), never a fabricated zero.
        assert_eq!(
            read_aggregate_columns(&storage, &snap.snapshot_uid),
            (None, None)
        );

        // The pipeline supplies the count it observed in the resolver's
        // OUTPUT stream. The writer stores it verbatim — deliberately no
        // `edges` involvement: this snapshot has ZERO edges rows, and the
        // persisted value must still be the supplied 7 (the full-stream
        // number survives non-materialization; EC-1 M-6).
        storage
            .persist_resolved_call_aggregate(&snap.snapshot_uid, 7)
            .unwrap();

        let (count, provenance) = read_aggregate_columns(&storage, &snap.snapshot_uid);
        assert_eq!(
            count,
            Some(7),
            "supplied count stored verbatim — never recomputed from edges rows"
        );
        assert_eq!(
            provenance.as_deref(),
            Some(RESOLVED_CALL_PROVENANCE_PIPELINE),
            "the ratified interim-rule provenance label is stamped"
        );
    }

    #[test]
    fn persist_resolved_call_aggregate_zero_is_measured_zero_not_null() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);

        storage
            .persist_resolved_call_aggregate(&snap.snapshot_uid, 0)
            .unwrap();

        // 0 = measured-and-absent (distinct from NULL = not persisted).
        let (count, provenance) = read_aggregate_columns(&storage, &snap.snapshot_uid);
        assert_eq!(count, Some(0));
        assert_eq!(
            provenance.as_deref(),
            Some(RESOLVED_CALL_PROVENANCE_PIPELINE)
        );
    }

    #[test]
    fn adjust_resolved_call_aggregate_applies_net_delta_and_keeps_label() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);

        storage
            .persist_resolved_call_aggregate(&snap.snapshot_uid, 5)
            .unwrap();

        // Promotion nets +2 CALLS rows…
        crate::crud::snapshots::adjust_resolved_call_aggregate(
            storage.connection(),
            &snap.snapshot_uid,
            2,
        )
        .unwrap();
        assert_eq!(
            read_aggregate_columns(&storage, &snap.snapshot_uid),
            (Some(7), Some(RESOLVED_CALL_PROVENANCE_PIPELINE.to_string()))
        );

        // …and a later pass can net negative (re-promotion shrank the set).
        crate::crud::snapshots::adjust_resolved_call_aggregate(
            storage.connection(),
            &snap.snapshot_uid,
            -3,
        )
        .unwrap();
        assert_eq!(
            read_aggregate_columns(&storage, &snap.snapshot_uid),
            (Some(4), Some(RESOLVED_CALL_PROVENANCE_PIPELINE.to_string()))
        );
    }

    #[test]
    fn adjust_resolved_call_aggregate_null_stays_null_never_seeded() {
        let storage = fresh_storage();
        storage.add_repo(&make_repo("r1")).unwrap();
        let snap = create_test_snapshot(&storage);

        // Pre-migration shape: no persisted aggregate. Promotion on such a
        // snapshot must NOT mint one (a seed would need an edges-derived
        // base — the filtered accounting M-3b removes). NULL + delta = NULL
        // by SQL semantics: explicitly unavailable → the labeled live-COUNT
        // fallback applies.
        crate::crud::snapshots::adjust_resolved_call_aggregate(
            storage.connection(),
            &snap.snapshot_uid,
            3,
        )
        .unwrap();
        assert_eq!(
            read_aggregate_columns(&storage, &snap.snapshot_uid),
            (None, None),
            "NULL aggregate stays NULL under adjustment — never seeded, never labeled"
        );
    }
}
