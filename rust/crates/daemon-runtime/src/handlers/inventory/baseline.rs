//! CACHE-SEMANTICS-1: User baseline marking handler.
//!
//! Allows users to explicitly mark a snapshot as a baseline for comparison.
//! User baselines are preserved across automatic retention classification.
//!
//! # EC-M7-BASELINE-STAMP-1 (D-EC-8-D): a mark is a STAMP by default
//!
//! `mark_baseline` now marks a provenance STAMP (`baseline_stamp`): the
//! snapshot row (comparability/toolchain/epoch identity) + the FC4 measurement
//! rows are retained; graph-family rows are NOT promised and are removed by
//! the retention pass once the snapshot leaves the serving pair. The pre-M-7
//! row-retaining behavior remains available behind the explicit `retain_rows`
//! flag (class `baseline_user` — also the class every pre-existing mark keeps,
//! so upgrades lose nothing). The storage cost of the choice is surfaced in
//! the response (exact row counts; dbstat-prorated byte estimates, or an
//! honest "sizes unknown" — never fabricated numbers).
//!
//! Graph-row comparability is keyed on the CLASS, not on whether rows still
//! physically linger: a stamp's graph-row comparisons are `not_comparable`
//! from the moment of marking (an answer must not flip when the background
//! pass fires), with the concrete remediation named. Measurement-level
//! comparison keeps working against a stamp (`measurements` is retained).
//!
//! # Authority Classification (STATE-ROOT-SEPARATION-1)
//!
//! User baselines are A1 (User Authority) data:
//! - Represent explicit user decisions about retention
//! - Cannot be automatically recovered
//! - Blocked in sandbox-local mode
//!
//! See `agent_docs/storage-architecture-v2.md` for tier definitions.

use std::path::Path;

use repo_graph_daemon_transport::{DispatchResult, ErrorCode, ErrorDetail, Request};
use repo_graph_storage::retention::RetentionClass;
use serde_json::json;

use crate::require_global_mode_for_authority_write;
use crate::state::DaemonState;

/// Handle `mark_baseline` request.
///
/// Marks a specific snapshot as a user baseline. User baselines are preserved
/// across automatic retention classification and are never auto-pruned.
///
/// # Authority Classification
///
/// This is an A1 (User Authority) write. Blocked in sandbox-local mode.
///
/// Params:
///   - `path` (required): Repo path
///   - `snapshot_uid` (optional): Specific snapshot to mark. If omitted, marks
///     the current (most recent) snapshot.
///   - `retain_rows` (optional, default false): the D-EC-8-D row-retention
///     opt-in. `false` → the mark is a provenance STAMP (`baseline_stamp`);
///     `true` → full graph-family rows stay pinned (`baseline_user`, the
///     pre-M-7 behavior).
///
/// Response:
///   - `marked`: true if marking succeeded
///   - `snapshot_uid`: the snapshot that was marked
///   - `repo_path`: canonical path of repo
///   - `retention_class`: `baseline_stamp` | `baseline_user`
///   - `retains` / `graph_row_cost`: what this mark keeps, with exact row
///     counts and honest (estimated or unknown) byte figures
///   - `graph_row_comparisons` (+ `remediation` when `not_comparable`)
///
/// # Semantics on an already-marked snapshot (decide-and-record)
///
/// - default mark on an existing `baseline_user` mark → the row-retaining
///   class is KEPT (a repeat default call must never silently downgrade a
///   pre-existing row promise — clause 7's no-silent-data-loss rule); the
///   response says so. Use `unmark_baseline` first to change intent.
/// - `retain_rows=true` on a stamp whose rows are still present → upgrade to
///   `baseline_user` (the rows exist; the promise is simply widened).
/// - `retain_rows=true` on a snapshot whose RECORDED graph rows are no longer
///   present (narrowed) → ERROR with the concrete remediation (a
///   "row-retaining" mark over removed rows would be a lie).
/// - `retain_rows=true` on a KNOWN-EMPTY snapshot (the index recorded 0
///   files/nodes/edges — see [`known_empty_at_index`]) → allowed
///   (`baseline_user`): the pre-M-7 capability of marking a valid empty-graph
///   snapshot is preserved; the row promise is vacuously satisfiable and
///   comparisons honestly compare against a recorded-zero graph.
pub fn handle_mark_baseline(state: &DaemonState, request: &Request) -> DispatchResult {
    // STATE-ROOT-SEPARATION-1: A1 authority write guard
    if let Err(e) = require_global_mode_for_authority_write(state, request, "mark_baseline") {
        return e;
    }

    let path: &str = match request.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'path' parameter"),
            )
        }
    };

    let snapshot_uid_param = request.params.get("snapshot_uid").and_then(|v| v.as_str());

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

    // Acquire write lock for marking
    let _write_guard = repo_state.coordinator.acquire_write();

    // D-S = S-A (DAEMON-CONCURRENCY-IMPL-1): open one fresh per-operation connection for this
    // handler's SQLite reads. The coordinator guard above keeps it snapshot-consistent for the request.
    let storage = match state.open_repo_storage_for_request(&repo_state) {
        Ok(s) => s,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Resolve the snapshot — the FULL row, not just the uid: the row-retention
    // guard below distinguishes a narrowed snapshot from a known-empty one via
    // the recorded index-time totals on this row (review-1 #2).
    let snapshot = match snapshot_uid_param {
        Some(uid) => {
            // Verify snapshot exists and belongs to this repo
            match storage.get_snapshot(uid) {
                Ok(Some(snap)) => {
                    if snap.repo_uid != *repo_uid {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::invalid_request(format!(
                                "snapshot '{}' belongs to repo '{}', not '{}'",
                                uid, snap.repo_uid, repo_uid
                            )),
                        );
                    }
                    snap
                }
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::invalid_request(format!("snapshot '{}' not found", uid)),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            }
        }
        None => {
            // Use latest snapshot
            match storage.get_latest_snapshot(repo_uid) {
                Ok(Some(snap)) => snap,
                Ok(None) => {
                    // DAEMON-VISIBILITY-1 (F2): baseline is READY-requiring — NAME any existing partial
                    // (state, when, on-disk size) + both next actions, never the bare day-2 string.
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::SnapshotNotFound,
                            crate::snapshot_facts::no_ready_snapshot_message(
                                &storage, db_path, repo_uid,
                            ),
                        ),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            }
        }
    };
    let snapshot_uid = snapshot.snapshot_uid.clone();

    // EC-M7 (D-EC-8-D): stamp by default; row retention is the explicit opt-in.
    // STRICT type guard (review-1 #5): a present, non-boolean value (e.g. the
    // string "true") must be REJECTED, not silently read as `false` — the
    // silent path would turn an intended row-retention promise into eventual
    // row deletion. Absent or JSON `null` = not requested (the default).
    let retain_rows = match request.params.get("retain_rows") {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(other) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(format!(
                    "'retain_rows' must be a boolean (true/false), got {}. Pass \
                     retain_rows=true to pin this snapshot's graph rows, or omit it for a \
                     provenance stamp.",
                    other
                )),
            )
        }
    };

    let existing_class = match storage.get_snapshot_retention_class(&snapshot_uid) {
        Ok(c) => c,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            )
        }
    };
    let rows_present = match storage.snapshot_graph_rows_present(&snapshot_uid) {
        Ok(p) => p,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            )
        }
    };

    // Review-1 #2: distinguish deterministically between a NARROWED snapshot
    // (recorded rows now gone — a row promise over it would be a lie) and an
    // intact KNOWN-EMPTY snapshot (the index recorded a zero graph — the
    // pre-M-7 capability of marking it row-retaining stays available; the
    // promise is vacuously satisfiable and comparisons honestly compare
    // against a recorded-zero graph).
    let recorded_empty = known_empty_at_index(&snapshot);

    // Guard: a row-retaining mark must not promise rows that were recorded
    // and are now gone.
    if retain_rows && !rows_present && !recorded_empty {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::invalid_request(format!(
                "snapshot '{}' had graph rows at index time ({} files / {} nodes / {} \
                 edges recorded) but they are no longer present — they were narrowed to \
                 the provenance stamp. To get a row-retaining baseline: run `rmap \
                 index`/`rmap refresh`, then mark the new snapshot with retain_rows=true.",
                snapshot_uid, snapshot.files_total, snapshot.nodes_total, snapshot.edges_total
            )),
        );
    }

    // Guard: a repeat DEFAULT mark on an existing row-retaining mark keeps the
    // row promise (no silent downgrade — clause 7). Explicit changes go
    // through unmark_baseline.
    let kept_existing_user_mark =
        !retain_rows && existing_class == Some(RetentionClass::BaselineUser);
    let target_class = if retain_rows || kept_existing_user_mark {
        RetentionClass::BaselineUser
    } else {
        RetentionClass::BaselineStamp
    };

    // Measure the cost BEFORE the A1 write (review-1 #1): every fallible
    // storage read precedes `mark_snapshot_retention`, so a read failure can
    // only fail a request whose mark was NOT committed — the response can
    // never report failure after a successful authority write. The numbers
    // are identical measured here or after the mark: the mark and the
    // reclassification below only flip `snapshots.retention_class`; no family
    // rows change inside this handler.
    let cost = match storage.snapshot_family_cost(&snapshot_uid) {
        Ok(c) => c,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            )
        }
    };

    if let Err(e) = storage.mark_snapshot_retention(&snapshot_uid, target_class) {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
        );
    }

    // Re-run classification to maintain coherent current/parent/baseline_auto
    // (e.g., if we just marked the current snapshot, a new current must be assigned)
    if let Err(e) = storage.classify_repo_retention(repo_uid) {
        // Non-fatal warning — the mark succeeded, classification is best-effort
        eprintln!(
            "warning: retention reclassification failed after mark_baseline: {}",
            e
        );
    }

    let response = json!({
        "marked": true,
        "snapshot_uid": snapshot_uid,
        "repo_path": entry.canonical_path,
        "retention_class": target_class.as_str(),
        "retains": {
            "provenance_stamp": true,
            // FC4 measurement/assessment families ONLY (review-1 #3): the
            // retained Tier-A declarations are authority, not measurements,
            // and are reported under their own key.
            "measurements": {
                "families": cost.measurement_families,
                "rows_total": cost.measurement_rows_total,
                "estimated_bytes": cost.measurement_estimated_bytes,
            },
            "declarations": {
                "rows_total": cost.declaration_rows,
                "estimated_bytes": cost.declaration_estimated_bytes,
            },
            "graph_rows": target_class == RetentionClass::BaselineUser,
        },
        "graph_row_cost": {
            "families": cost.graph_families,
            "rows_total": cost.graph_rows_total,
            "estimated_bytes": cost.graph_estimated_bytes,
            "estimate_basis": cost.estimate_basis,
            // Disambiguates rows_total = 0: recorded empty at index time vs
            // narrowed away (review-1 #2 — an agent must not read a
            // known-empty graph as data loss, or vice versa).
            "recorded_empty_at_index": recorded_empty,
        },
        "graph_row_comparisons": comparability_token(target_class),
        "note": mark_note(target_class, kept_existing_user_mark, recorded_empty, &cost),
        "remediation": stamp_remediation(target_class, rows_present, recorded_empty),
    });

    DispatchResult::success(&request.id, response)
}

/// Graph-row comparability for a baseline mark, keyed on the CLASS (never on
/// physical row presence — a comparability answer must not flip when the
/// background narrow pass runs). VISION rule 3: report NOT_COMPARABLE rather
/// than fake numbers. Shared by the mark-time response and the per-mark
/// retention report (`handlers::inventory::retention`).
pub(crate) fn comparability_token(class: RetentionClass) -> &'static str {
    match class {
        RetentionClass::BaselineUser => "comparable",
        _ => "not_comparable",
    }
}

/// Whether this snapshot's graph was KNOWN EMPTY at index time: the recorded
/// `files_total` / `nodes_total` / `edges_total` on its `snapshots` row are
/// all zero.
///
/// # Why this is a deterministic Layer-0 basis (review-1 #2)
///
/// The three totals are written by `update_snapshot_counts` at Phase-5
/// finalization — physical `COUNT(*)` over `file_versions`/`nodes`/`edges`,
/// on BOTH fresh index and delta refresh, before the READY transition — and
/// the narrow pass never touches the `snapshots` row, so they survive
/// narrowing as an index-time record. All-zero therefore means the index
/// recorded a zero graph skeleton (no file versions → no extraction source
/// material for any derived family either); any nonzero total with the rows
/// physically absent means the rows were REMOVED since finalization
/// (narrowed). No new persisted shape is involved. (Enrichment promotion
/// adjusts CALLS rows but cannot create graph rows on a zero-node snapshot —
/// unresolved edges FK into `nodes` — so staleness of the totals cannot flip
/// the all-zero predicate.)
pub(crate) fn known_empty_at_index(snap: &repo_graph_storage::types::Snapshot) -> bool {
    snap.files_total == 0 && snap.nodes_total == 0 && snap.edges_total == 0
}

/// Reader-frame remediation for a stamp mark's `not_comparable` graph rows.
/// `None` for a row-retaining mark (nothing to remediate). Shared by the
/// mark-time response and the per-mark retention report. `recorded_empty`
/// (see [`known_empty_at_index`]) keeps the absent-rows text honest: a
/// known-empty snapshot's rows were never removed — there were none.
pub(crate) fn stamp_remediation(
    class: RetentionClass,
    rows_present: bool,
    recorded_empty: bool,
) -> Option<String> {
    if class == RetentionClass::BaselineUser {
        return None;
    }
    Some(if rows_present {
        "graph-row baseline comparisons against this stamp are not comparable; to keep \
         row-level comparison, re-mark this snapshot with retain_rows=true while its rows \
         are still present"
            .to_string()
    } else if recorded_empty {
        "graph-row baseline comparisons against this stamp are not comparable; this \
         snapshot's graph was recorded empty at index time (0 files/nodes/edges), so there \
         are no graph rows to compare. To get a row-level baseline, index the repo once it \
         has source files and mark that snapshot with retain_rows=true"
            .to_string()
    } else {
        "graph-row baseline comparisons against this stamp are not comparable and its rows \
         are already gone; run `rmap index`/`rmap refresh`, then mark the new snapshot with \
         retain_rows=true"
            .to_string()
    })
}

/// One reader-frame sentence stating what this mark keeps and what it costs.
/// Declarations (retained authority) are named separately from measurements
/// and only when present — a declaration is not a measurement (review-1 #3).
/// `recorded_empty` keeps the text honest on a known-empty snapshot: there
/// are no graph rows to pin or remove, and saying "0 rows will be removed"
/// would imply a removal that can never happen.
fn mark_note(
    class: RetentionClass,
    kept_existing_user_mark: bool,
    recorded_empty: bool,
    cost: &repo_graph_storage::retention::SnapshotFamilyCost,
) -> String {
    let bytes = |b: Option<u64>| match b {
        Some(b) => format!("~{}", format_approx_bytes(b)),
        None => "size unknown".to_string(),
    };
    let declarations_suffix = if cost.declaration_rows > 0 {
        format!(
            " and {} declaration rows (retained authority)",
            cost.declaration_rows
        )
    } else {
        String::new()
    };
    match class {
        RetentionClass::BaselineUser if kept_existing_user_mark => format!(
            "this snapshot already carries a row-retaining baseline mark; it was kept (a \
             default mark never silently drops an existing row promise). It pins {} graph \
             rows ({}). Use unmark_baseline first if you want a stamp instead.",
            cost.graph_rows_total,
            bytes(cost.graph_estimated_bytes)
        ),
        RetentionClass::BaselineUser if recorded_empty => format!(
            "row-retaining baseline on a recorded-empty snapshot: the index recorded 0 \
             files/nodes/edges, so there are no graph rows to pin; the provenance stamp \
             and {} measurement rows ({}){} are kept",
            cost.measurement_rows_total,
            bytes(cost.measurement_estimated_bytes),
            declarations_suffix
        ),
        RetentionClass::BaselineUser => format!(
            "row-retaining baseline: {} graph rows ({}) stay pinned until unmarked, in \
             addition to the provenance stamp, {} measurement rows ({}){}",
            cost.graph_rows_total,
            bytes(cost.graph_estimated_bytes),
            cost.measurement_rows_total,
            bytes(cost.measurement_estimated_bytes),
            declarations_suffix
        ),
        _ if recorded_empty => format!(
            "provenance stamp: comparability metadata, {} measurement rows ({}){} are kept; \
             this snapshot's graph was recorded empty at index time (0 files/nodes/edges) — \
             there are no graph rows to retain or remove",
            cost.measurement_rows_total,
            bytes(cost.measurement_estimated_bytes),
            declarations_suffix
        ),
        _ => format!(
            "provenance stamp: comparability metadata, {} measurement rows ({}){} are kept; \
             the {} graph rows ({}) are not retained and will be removed by the retention \
             pass once this snapshot leaves the serving pair",
            cost.measurement_rows_total,
            bytes(cost.measurement_estimated_bytes),
            declarations_suffix,
            cost.graph_rows_total,
            bytes(cost.graph_estimated_bytes)
        ),
    }
}

/// Coarse byte formatter for the reader-frame cost note.
fn format_approx_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Handle `unmark_baseline` request.
///
/// Removes the user baseline marking from a snapshot. The snapshot will be
/// reclassified during the next retention classification.
///
/// # Authority Classification
///
/// This is an A1 (User Authority) write. Blocked in sandbox-local mode.
///
/// Params:
///   - `path` (required): Repo path
///   - `snapshot_uid` (required): Snapshot to unmark
///
/// Response:
///   - `unmarked`: true if unmarking succeeded
///   - `snapshot_uid`: the snapshot that was unmarked
pub fn handle_unmark_baseline(state: &DaemonState, request: &Request) -> DispatchResult {
    // STATE-ROOT-SEPARATION-1: A1 authority write guard
    if let Err(e) = require_global_mode_for_authority_write(state, request, "unmark_baseline") {
        return e;
    }

    let path: &str = match request.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'path' parameter"),
            )
        }
    };

    let snapshot_uid: &str = match request.params.get("snapshot_uid").and_then(|v| v.as_str()) {
        Some(u) => u,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing or invalid 'snapshot_uid' parameter"),
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

    // Acquire write lock
    let _write_guard = repo_state.coordinator.acquire_write();

    // D-S = S-A (DAEMON-CONCURRENCY-IMPL-1): open one fresh per-operation connection for this
    // handler's SQLite reads. The coordinator guard above keeps it snapshot-consistent for the request.
    let storage = match state.open_repo_storage_for_request(&repo_state) {
        Ok(s) => s,
        Err(e) => return DispatchResult::error(&request.id, e),
    };

    // Verify snapshot exists and is marked as user baseline
    match storage.get_snapshot(snapshot_uid) {
        Ok(Some(snap)) => {
            if snap.repo_uid != *repo_uid {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "snapshot '{}' belongs to repo '{}', not '{}'",
                        snapshot_uid, snap.repo_uid, repo_uid
                    )),
                );
            }
        }
        Ok(None) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(format!("snapshot '{}' not found", snapshot_uid)),
            );
        }
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }
    }

    // Mark as prunable (will be reclassified on next refresh)
    if let Err(e) = storage.mark_snapshot_retention(snapshot_uid, RetentionClass::Prunable) {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::new(ErrorCode::InternalError, format!("{}", e)),
        );
    }

    // Re-run classification to assign proper class
    if let Err(e) = storage.classify_repo_retention(repo_uid) {
        // Non-fatal warning
        eprintln!(
            "warning: retention reclassification failed after unmark: {}",
            e
        );
    }

    let response = json!({
        "unmarked": true,
        "snapshot_uid": snapshot_uid,
        "repo_path": entry.canonical_path
    });

    DispatchResult::success(&request.id, response)
}
