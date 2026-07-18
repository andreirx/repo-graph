//! RETENTION-POLICY-1: Retention lifecycle enforcement.
//!
//! # Architecture (REFRESH-HANG-1 fix)
//!
//! Retention is split into two phases:
//!
//! **Foreground (synchronous index/refresh):**
//! - `classify_retention_only()` — assign retention classes, return stats
//! - Fast (~1ms), never blocks user commands
//! - Reports prunable_count so user knows maintenance is needed
//!
//! **Maintenance (explicit or deferred):**
//! - `enforce_retention_lifecycle()` — classify + prune + stats
//! - Can be slow (deletes rows), runs only on explicit request
//! - Called by `classify_retention` daemon command
//!
//! # Why prune is not on the hot path
//!
//! Prune deletes potentially millions of rows from `unresolved_edges` and
//! other tables. On repos with many stale snapshots, this can take 60+
//! seconds. That must never block interactive index/refresh.
//!
//! # Invariants
//!
//! - Never prunes: current, parent, baseline_auto, baseline_user, baseline_stamp
//!   (EC-M7: a `baseline_stamp` mark's SNAPSHOT ROW is never pruned; its
//!   graph-family rows are narrowed away by the lifecycle once the mark leaves
//!   the serving pair — see `storage::retention::narrow`)
//! - Stale-epoch snapshots are prunable because classification made them so
//! - READY-retention prune is post-snapshot-success only
//! - Prune is idempotent (no-op if no prunable snapshots); so is the narrow step
//!
//! # DAEMON-VISIBILITY-1 (F3) amendment — interrupted-snapshot reclaim (operator Option A, 2026-07-03)
//!
//! The READY-retention invariants above are unchanged. This handler ALSO reclaims ORPHANED non-READY
//! (interrupted / failed) snapshots — the day-2 field bug was a 4 GB `building` snapshot invisible to
//! the READY model that silently held disk. "Prune is post-snapshot-success only" therefore no longer
//! describes the whole handler: it governs the READY path; the non-READY reclaim is gated instead on
//! "no live write op on this DB" (the operator's ratified safety rule — consult the activity registry
//! AND hold the DB write lock, so an in-flight index's `building` snapshot is never touched). See
//! `reclaim_orphaned_non_ready` below and `docs/slices/daemon-visibility-1.md` §2 F3.
//!
//! # References
//!
//! - `docs/slices/retention-policy-1.md`
//! - `docs/slices/cache-semantics-1.md`
//! - `docs/slices/refresh-hang-1.md`

use std::path::Path;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::connection::StorageConnection;
use repo_graph_storage::error::StorageError;
use repo_graph_storage::retention::RetentionStats;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::DaemonState;

/// Result of retention lifecycle enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleResult {
    /// Retention classification completed
    pub classified: bool,
    /// Number of snapshots pruned (0 if prune was not performed)
    pub pruned_count: i64,
    /// Number of snapshots marked prunable but not yet deleted
    pub prunable_count: i64,
    /// EC-M7: `baseline_stamp` marks narrowed to their stamp this run
    /// (graph families deleted; snapshot row + measurements kept).
    /// 0 on the classify-only foreground path.
    pub narrowed_count: i64,
    /// Rows removed by the narrowing above: direct deletions PLUS cascade-linked
    /// child deletions (sums `NarrowedBaseline.rows_deleted`).
    pub narrowed_rows: i64,
    /// Retention stats after lifecycle enforcement
    pub stats: RetentionStats,
}

/// Classify retention only — foreground path for index/refresh.
///
/// This is the fast path for synchronous index/refresh. It:
/// 1. Classifies all snapshots (assigns retention classes)
/// 2. Returns stats including prunable_count
///
/// It does **NOT** prune. Pruning is deferred to explicit maintenance
/// because deleting rows from large tables can take 60+ seconds.
///
/// # When to use
///
/// Call from `handle_index` and `handle_refresh` after successful
/// snapshot commit. The user sees `prunable_count` in the response
/// and knows to run maintenance if needed.
///
/// # Performance
///
/// Classification is fast (~1ms). Safe to call on every index/refresh.
pub fn classify_retention_only(
    storage: &StorageConnection,
    repo_uid: &str,
) -> Result<LifecycleResult, StorageError> {
    // 1. Classify all snapshots
    storage.classify_repo_retention(repo_uid)?;

    // 2. Get current retention stats (includes prunable count)
    let stats = storage.get_retention_stats(repo_uid)?;

    // Calculate prunable count from stats
    let prunable_count = stats
        .total
        .saturating_sub(stats.current)
        .saturating_sub(stats.parent)
        .saturating_sub(stats.baseline_auto)
        .saturating_sub(stats.baseline_user)
        .saturating_sub(stats.baseline_stamp);

    Ok(LifecycleResult {
        classified: true,
        pruned_count: 0,
        prunable_count,
        narrowed_count: 0,
        narrowed_rows: 0,
        stats,
    })
}

/// Enforce full retention lifecycle: classify → prune → narrow → return summary.
///
/// This is the **maintenance** path that includes pruning.
/// Use for explicit maintenance commands, NOT for interactive index/refresh.
///
/// # Sequence
///
/// 1. Classify all snapshots for the repo (assigns retention classes)
/// 2. Prune all snapshots marked `prunable`
/// 3. Narrow eligible `baseline_stamp` marks to their stamp (EC-M7:
///    graph families deleted, snapshot row + measurements kept; the serving
///    pair — latest READY + its delta-base parent — is structurally excluded)
/// 4. Return stats
///
/// # Warning
///
/// Pruning can be slow (60+ seconds) on repos with many stale snapshots.
/// Do NOT call from synchronous index/refresh hot path.
///
/// # Transaction Boundaries
///
/// Classification, prune, and narrow are each atomic per snapshot, but
/// the combined lifecycle is **sequenced, not single-transaction atomic**.
///
/// # Idempotence
///
/// Safe to call multiple times. Second call with no new prunable snapshots
/// returns pruned_count = 0 (and narrowed_count = 0 — an already-narrowed
/// stamp deletes nothing).
pub fn enforce_retention_lifecycle(
    storage: &StorageConnection,
    repo_uid: &str,
) -> Result<LifecycleResult, StorageError> {
    // 1. Classify all snapshots
    storage.classify_repo_retention(repo_uid)?;

    // 2. Prune prunable snapshots
    let pruned_count = storage.prune_prunable_snapshots(repo_uid)?;

    // 3. EC-M7: narrow eligible stamp marks (same write discipline as the
    //    prune above — the caller already holds the repo write guard).
    let narrowed = storage.narrow_stamp_baselines(repo_uid)?;
    let narrowed_rows: i64 = narrowed.iter().map(|n| n.rows_deleted).sum();

    // 4. Get current retention stats
    let stats = storage.get_retention_stats(repo_uid)?;

    // Log if anything was pruned
    if pruned_count > 0 {
        eprintln!(
            "retention: pruned {} snapshot(s) for repo {}",
            pruned_count, repo_uid
        );
    }

    Ok(LifecycleResult {
        classified: true,
        pruned_count,
        prunable_count: 0, // All prunable were just pruned
        narrowed_count: narrowed.len() as i64,
        narrowed_rows,
        stats,
    })
}

/// Handle `classify_retention` request.
///
/// Runs full retention lifecycle: classify → prune → report.
/// Uses the shared `enforce_retention_lifecycle` helper.
///
/// Params:
///   - `path` (required): Repo path to classify
///
/// Response:
///   - `classified`: true if classification ran
///   - `pruned_count`: number of snapshots pruned
///   - `retention`: current retention stats
///   - `repo_path`: canonical path of repo
pub fn handle_classify_retention(state: &DaemonState, request: &Request) -> DispatchResult {
    let path: &str = match request.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'path' parameter"),
            )
        }
    };

    // Resolve path to repo entry
    let entry = match state.resolve_alias_or_path(path) {
        Some(e) => e,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::RepoNotFound,
                    format!("repo not indexed: {} (run: rmap index {})", path, path),
                ),
            )
        }
    };

    let db_path = Path::new(&entry.db_path);
    let repo_uid = &entry.repo_uid;

    // Load repo state
    let repo_state = match state.load_repo(db_path, repo_uid) {
        Ok(rs) => rs,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // Acquire write lock for lifecycle enforcement
    let _write_guard = repo_state.coordinator.acquire_write();

    // D-S = S-A (DAEMON-CONCURRENCY-IMPL-1): open one fresh per-operation connection for this
    // handler's SQLite reads. The coordinator guard above keeps it snapshot-consistent for the request.
    let storage = match repo_state.storage() {
        Ok(s) => s,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // Enforce retention lifecycle
    let result = match enforce_retention_lifecycle(&storage, repo_uid) {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
            );
        }
    };

    // DAEMON-VISIBILITY-1 (F3): non-READY (interrupted) snapshots are reclaimed OUTSIDE the READY
    // retention model. `classify_repo_retention` only classifies `status='ready'`, and
    // `prune_prunable_snapshots` is guarded to `status='ready'` (DAEMON-CRASH-RECOVERY-1) — so even a
    // crash orphan that reconciliation classified `prunable` (for stats visibility) is NOT deleted by
    // the READY prune above; it silently held disk in the day-2 field bug. Enumerate the non-READY
    // rows here (pre-reclaim, so the report can NAME state + when even after they are deleted), then
    // RECLAIM the orphaned ones (with the VACUUM that returns their disk to the OS).
    let interrupted: Vec<serde_json::Value> = storage
        .list_snapshots(repo_uid)
        .unwrap_or_default()
        .iter()
        .filter(|s| s.status != "ready")
        .map(|s| {
            json!({
                "snapshot_uid": s.snapshot_uid,
                "status": s.status,
                "state": crate::snapshot_facts::snapshot_state_label(&s.status, false),
                "created_at": s.created_at,
                "files_total": s.files_total,
            })
        })
        .collect();

    // F3 (operator Option A): actually delete + reclaim the orphaned non-READY snapshots. Gated so a
    // live index's `building` snapshot is NEVER touched (see `reclaim_orphaned_non_ready`). This runs
    // AFTER the enumeration above and re-queries under the DB lock, so the reported list is stable.
    let reclaim =
        reclaim_orphaned_non_ready(state, &storage, db_path, repo_uid, !interrupted.is_empty());

    // Measured AFTER the reclaim: the honest "storage this repo holds now" (post-VACUUM if we ran one).
    let db_size_bytes = std::fs::metadata(&entry.db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // EC-M7: per-mark cost report — retention reporting shows what each
    // baseline mark retains and costs, either way (D-EC-8-D clause 2).
    let baseline_marks = baseline_marks_report(&storage, repo_uid);

    let response = json!({
        "classified": result.classified,
        "pruned_count": result.pruned_count,
        // EC-M7: stamp marks narrowed this run (graph families deleted; stamp
        // + measurements kept).
        "narrowed_count": result.narrowed_count,
        "narrowed_rows": result.narrowed_rows,
        "retention": {
            "current": result.stats.current,
            "parent": result.stats.parent,
            "baseline_auto": result.stats.baseline_auto,
            "baseline_user": result.stats.baseline_user,
            "baseline_stamp": result.stats.baseline_stamp,
            "prunable": result.stats.prunable,
            "total": result.stats.total,
        },
        "baseline_marks": baseline_marks,
        // F3: interrupted snapshots that were present (named for the report), + the reclaim outcome.
        "interrupted_snapshots": interrupted,
        "non_ready_reclaim": reclaim,
        "db_size_bytes": db_size_bytes,
        "repo_path": entry.canonical_path
    });

    DispatchResult::success(&request.id, response)
}

/// EC-M7: one entry per baseline mark — class, what it retains, its measured
/// cost (exact rows; estimated-or-unknown bytes), and the graph-row
/// comparability contract with remediation.
///
/// # Failure honesty (review-1 #4)
///
/// Every read failure is an EXPLICIT entry, never a silent conversion:
/// - `list_snapshots` failure → one entry naming the failed report (an empty
///   list must mean "no marks", not "could not look");
/// - a per-mark class / graph-row-presence / cost read failure → an entry for
///   that mark naming the error (a `present=false` guess could misdirect the
///   reader to re-index when rows may still exist).
fn baseline_marks_report(storage: &StorageConnection, repo_uid: &str) -> Vec<serde_json::Value> {
    use repo_graph_storage::retention::RetentionClass;

    let snapshots = match storage.list_snapshots(repo_uid) {
        Ok(s) => s,
        Err(e) => {
            return vec![json!({
                "error": format!(
                    "baseline-mark report unavailable — could not list snapshots: {e}"
                ),
            })]
        }
    };
    let mut marks = Vec::new();
    for snap in snapshots {
        let class = match storage.get_snapshot_retention_class(&snap.snapshot_uid) {
            Ok(Some(c @ (RetentionClass::BaselineUser | RetentionClass::BaselineStamp))) => c,
            Ok(_) => continue,
            Err(e) => {
                marks.push(json!({
                    "snapshot_uid": snap.snapshot_uid,
                    "error": format!("could not read retention class: {e}"),
                }));
                continue;
            }
        };
        let rows_present = match storage.snapshot_graph_rows_present(&snap.snapshot_uid) {
            Ok(p) => p,
            Err(e) => {
                marks.push(json!({
                    "snapshot_uid": snap.snapshot_uid,
                    "retention_class": class.as_str(),
                    "error": format!("could not check graph-row presence: {e}"),
                }));
                continue;
            }
        };
        let cost = match storage.snapshot_family_cost(&snap.snapshot_uid) {
            Ok(c) => c,
            Err(e) => {
                marks.push(json!({
                    "snapshot_uid": snap.snapshot_uid,
                    "retention_class": class.as_str(),
                    "error": format!("could not measure cost: {e}"),
                }));
                continue;
            }
        };
        // Review-1 #2: disambiguate absent rows — recorded empty at index time
        // vs narrowed away. Deterministic from the recorded index-time totals
        // on the snapshots row (see `baseline::known_empty_at_index`).
        let recorded_empty = super::baseline::known_empty_at_index(&snap);
        marks.push(json!({
            "snapshot_uid": snap.snapshot_uid,
            "retention_class": class.as_str(),
            "created_at": snap.created_at,
            "label": snap.label,
            // Reader-frame retention split: what the mark pins vs the stamp.
            "graph_rows": {
                "retained": class == RetentionClass::BaselineUser,
                "present": rows_present,
                "recorded_empty_at_index": recorded_empty,
                "rows_total": cost.graph_rows_total,
                "estimated_bytes": cost.graph_estimated_bytes,
            },
            // FC4 measurement/assessment families only; retained declarations
            // are authority, reported under their own key (review-1 #3).
            "measurements": {
                "rows_total": cost.measurement_rows_total,
                "estimated_bytes": cost.measurement_estimated_bytes,
            },
            "declarations": {
                "rows_total": cost.declaration_rows,
                "estimated_bytes": cost.declaration_estimated_bytes,
            },
            "estimate_basis": cost.estimate_basis,
            "graph_row_comparisons": super::baseline::comparability_token(class),
            "remediation": super::baseline::stamp_remediation(class, rows_present, recorded_empty),
        }));
    }
    marks
}

/// DAEMON-VISIBILITY-1 (F3, operator Option A): delete + reclaim the ORPHANED non-READY snapshots for
/// this repo, returning a reader-frame outcome object for the prune response.
///
/// # Safety (two gates, both required before any deletion)
///
/// The operator's ratified rule: delete a non-READY snapshot only when NO live operation is attached.
/// An initial index coordinates on the DB-level write lock — NOT the `RepoCoordinator` this handler
/// already holds — so the repo write lock alone is blind to it. We therefore require BOTH:
///
/// 1. **Activity registry clear** — no index/refresh/enrich has stamped an in-flight op on this DB
///    (`state.activity().active_for_db`). Every write handler stamps this at entry.
/// 2. **DB write lock free** — `try_acquire_write()` (NON-blocking) on the same `DatabaseState` lock an
///    initial index takes. `try_lock` cannot deadlock against the repo write lock we already hold, and
///    holding it for the deletion + VACUUM excludes any index that would start mid-reclaim.
///
/// If either gate is closed, we SKIP deletion and report "not reclaimed — an operation is in progress"
/// (honest: the interrupted snapshot is still listed; the operator re-runs prune when idle). A live
/// index's in-flight `building` snapshot is thus never reachable by the delete.
///
/// On success it deletes every non-READY snapshot (reusing storage's transactional cascade), runs
/// `VACUUM` to realise the on-disk reclaim (SQLite does not shrink on DELETE), and reports the byte
/// delta. READY snapshots are never touched (the storage query filters `status != 'ready'`).
fn reclaim_orphaned_non_ready(
    state: &DaemonState,
    storage: &StorageConnection,
    db_path: &Path,
    repo_uid: &str,
    has_interrupted: bool,
) -> serde_json::Value {
    let skipped = |reason: &str| json!({ "reclaimed": false, "skipped_reason": reason, "deleted_count": 0, "reclaimed_bytes": 0 });
    if !has_interrupted {
        return json!({ "reclaimed": false, "deleted_count": 0, "reclaimed_bytes": 0 });
    }
    // Gate 1 — operator's named rule: never delete while a live op writes this DB.
    if state.activity().active_for_db(db_path).is_some() {
        return skipped("an operation is in progress on this repo");
    }
    // Gate 2 — take the DB write lock non-blockingly (excludes an initial index coordinating on it).
    let db_runtime = match state.get_or_create_db_runtime(db_path) {
        Ok(r) => r,
        Err(e) => return skipped(&format!("could not resolve db runtime: {e}")),
    };
    let _db_guard = match db_runtime.try_acquire_write() {
        Some(g) => g,
        None => return skipped("an operation is in progress on this repo"),
    };

    let size_before = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
    let deleted = match storage.prune_non_ready_snapshots(repo_uid) {
        Ok(uids) => uids,
        Err(e) => return skipped(&format!("delete failed: {e}")),
    };
    if deleted.is_empty() {
        // Raced away between enumeration and the lock (e.g. a just-finalized index): nothing to do.
        return json!({ "reclaimed": true, "deleted_count": 0, "reclaimed_bytes": 0 });
    }
    // Realise the reclaim on disk. If VACUUM fails the rows are still gone; report honestly.
    if let Err(e) = storage.vacuum() {
        return json!({
            "reclaimed": true,
            "deleted_count": deleted.len(),
            "reclaimed_bytes": 0,
            "vacuum_error": e.to_string(),
        });
    }
    let size_after = std::fs::metadata(db_path)
        .map(|m| m.len())
        .unwrap_or(size_before);
    json!({
        "reclaimed": true,
        "deleted_count": deleted.len(),
        "reclaimed_bytes": size_before.saturating_sub(size_after),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_storage::retention::RetentionClass;

    /// Test fixture writes go through `execute_raw` — the storage crate's
    /// sanctioned cross-crate integration-test seam.
    fn storage_with_repo() -> StorageConnection {
        let storage = StorageConnection::open_in_memory().unwrap();
        storage
            .execute_raw(
                "INSERT INTO repos (repo_uid, name, root_path, created_at) \
                 VALUES ('r1', 'test', '/test', '2025-01-01T00:00:00Z')",
            )
            .unwrap();
        storage
    }

    // Review-1 #4: a failed snapshot listing must yield an EXPLICIT
    // degradation entry — an empty `baseline_marks` array must always mean
    // "no marks exist", never "the report could not be read".
    #[test]
    fn report_degrades_explicitly_when_snapshot_listing_fails() {
        let storage = storage_with_repo();
        // Force the read failure a corrupt store would produce.
        storage.execute_raw("DROP TABLE snapshots").unwrap();

        let marks = baseline_marks_report(&storage, "r1");
        assert_eq!(marks.len(), 1, "one explicit degradation entry: {marks:?}");
        let err = marks[0]["error"].as_str().expect("entry names the error");
        assert!(
            err.contains("could not list snapshots"),
            "the degradation names its cause: {err}"
        );
    }

    // Review-1 #4: a graph-row-presence read failure must be a per-mark error
    // entry — never a silent `present=false`, which would misdirect the reader
    // to re-index while rows may still exist.
    #[test]
    fn report_names_presence_read_failure_per_mark_instead_of_guessing_false() {
        let storage = storage_with_repo();
        storage
            .execute_raw(
                "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) \
                 VALUES ('s1', 'r1', 'full', 'ready', '2025-01-01T00:00:00Z')",
            )
            .unwrap();
        storage
            .mark_snapshot_retention("s1", RetentionClass::BaselineStamp)
            .unwrap();
        // Break exactly the presence read: one narrow table missing.
        storage
            .execute_raw("DROP TABLE project_surface_evidence")
            .unwrap();

        let marks = baseline_marks_report(&storage, "r1");
        assert_eq!(marks.len(), 1, "the mark still appears: {marks:?}");
        let m = &marks[0];
        assert_eq!(m["snapshot_uid"], "s1");
        assert_eq!(m["retention_class"], "baseline_stamp");
        let err = m["error"].as_str().expect("entry names the error");
        assert!(
            err.contains("graph-row presence"),
            "the failure names what could not be read: {err}"
        );
        assert!(
            m.get("graph_rows").is_none(),
            "no fabricated presence/cost object accompanies the error: {m}"
        );
    }

    // Review-2 #2: the report distinguishes the two absent-rows states — a
    // KNOWN-EMPTY stamp (index recorded 0 files/nodes/edges; nothing was ever
    // removed) from a NARROWED stamp (recorded rows now gone) — via the
    // recorded index-time totals, deterministically, with matching remediation
    // text (no false "rows are already gone" claim for a graph that never had
    // rows).
    #[test]
    fn report_distinguishes_recorded_empty_from_narrowed_marks() {
        let storage = storage_with_repo();
        // s-empty: a finalized empty index — READY, totals recorded 0/0/0.
        storage
            .execute_raw(
                "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, \
                 files_total, nodes_total, edges_total, created_at) \
                 VALUES ('s-empty', 'r1', 'full', 'ready', 0, 0, 0, '2025-01-01T00:00:00Z')",
            )
            .unwrap();
        // s-narrowed: the index recorded rows (nonzero totals) but its family
        // rows are physically gone — the post-narrow state.
        storage
            .execute_raw(
                "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, \
                 files_total, nodes_total, edges_total, created_at) \
                 VALUES ('s-narrowed', 'r1', 'full', 'ready', 2, 5, 3, '2025-01-02T00:00:00Z')",
            )
            .unwrap();
        storage
            .mark_snapshot_retention("s-empty", RetentionClass::BaselineStamp)
            .unwrap();
        storage
            .mark_snapshot_retention("s-narrowed", RetentionClass::BaselineStamp)
            .unwrap();

        let marks = baseline_marks_report(&storage, "r1");
        assert_eq!(marks.len(), 2, "{marks:?}");
        let by_uid = |uid: &str| {
            marks
                .iter()
                .find(|m| m["snapshot_uid"] == uid)
                .unwrap_or_else(|| panic!("mark {uid} in report: {marks:?}"))
        };

        let empty = by_uid("s-empty");
        assert_eq!(empty["graph_rows"]["recorded_empty_at_index"], true);
        assert_eq!(empty["graph_rows"]["present"], false);
        let empty_remediation = empty["remediation"].as_str().unwrap();
        assert!(
            empty_remediation.contains("recorded empty at index time"),
            "known-empty remediation states the graph never had rows: {empty_remediation}"
        );
        assert!(
            !empty_remediation.contains("already gone"),
            "no false removal claim for a graph that never had rows: {empty_remediation}"
        );

        let narrowed = by_uid("s-narrowed");
        assert_eq!(narrowed["graph_rows"]["recorded_empty_at_index"], false);
        assert_eq!(narrowed["graph_rows"]["present"], false);
        let narrowed_remediation = narrowed["remediation"].as_str().unwrap();
        assert!(
            narrowed_remediation.contains("already gone"),
            "narrowed remediation states the rows were removed: {narrowed_remediation}"
        );
    }

    // The healthy path keeps the measurement/declaration split (review-1 #3):
    // declarations are never folded into the measurements figure.
    #[test]
    fn report_separates_measurements_from_declarations() {
        let storage = storage_with_repo();
        storage
            .execute_raw(
                "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) \
                 VALUES ('s1', 'r1', 'full', 'ready', '2025-01-01T00:00:00Z')",
            )
            .unwrap();
        storage
            .mark_snapshot_retention("s1", RetentionClass::BaselineStamp)
            .unwrap();
        storage
            .execute_raw(
                "INSERT INTO measurements (measurement_uid, snapshot_uid, repo_uid, \
                 target_stable_key, kind, value_json, source, created_at) \
                 VALUES ('m1', 's1', 'r1', 'k1', 'cyclomatic_complexity', '{\"value\": 3}', \
                         'test', '2025-01-01T00:00:00Z')",
            )
            .unwrap();
        storage
            .execute_raw(
                "INSERT INTO declarations (declaration_uid, repo_uid, snapshot_uid, \
                 target_stable_key, kind, value_json, created_at, is_active) \
                 VALUES ('d1', 'r1', 's1', 'r1:REPO', 'quality_policy', '{}', \
                         '2025-01-01T00:00:00Z', 1)",
            )
            .unwrap();

        let marks = baseline_marks_report(&storage, "r1");
        assert_eq!(marks.len(), 1);
        let m = &marks[0];
        assert_eq!(
            m["measurements"]["rows_total"], 1,
            "measurements count FC4 rows only: {m}"
        );
        assert_eq!(
            m["declarations"]["rows_total"], 1,
            "declarations reported separately as retained authority: {m}"
        );
    }
}
