//! DAEMON-RESIDUALS-2C §7: `rmap repo rebuild <path>` — the wipe-and-reindex verb.
//!
//! # Why this module exists (abstraction ledger)
//!
//! - **What:** the daemon-owned form of the recovery an operator performed BY HAND on 2026-09-04
//!   (`repo remove` in a retry loop while startup readers held the coordinator, then `index`).
//!   One coordinated operation: acquire the repo's writer discipline, retire the repo's store files
//!   behind a crash-detecting sentinel, then reindex from scratch on the SAME connection lifecycle
//!   `index` uses. The multi-file retire is SEQUENTIAL (one rename per store file); the sentinel (detect-and-name, HUMAN RULING
//!   cycle 2) is what guarantees a partial store is never SERVED.
//! - **Concrete current users:** the `repo_rebuild` wire method (`dispatch.rs` routing) and the
//!   `rmap repo rebuild` CLI verb (`rgr::commands::repo`). One caller today; this is not a variation
//!   seam — it is a cohesion split from the 10k-line `dispatch.rs` handler hub (structural guardrail:
//!   a NEW responsibility does not get appended to a file already far over 500 lines).
//! - **Named axis of variation:** none. Direct implementation of one ratified verb (§7). It REUSES
//!   the ratified primitives rather than reinventing them: `foreground_open::acquire_foreground_write`
//!   (bounded-patience DB-write-mutex + coordinator-writer acquisition with a holder-NAMED `Busy`, the
//!   §2.2 vocabulary), `repo_index::compose::index_path_with_progress` (the exact reindex connection
//!   lifecycle `handle_index` drives), and `ServiceDispatcher::finish_write_with_maintenance` (the
//!   async enrich→seed→retention chain the fresh snapshot must queue, identical to `index`).
//! - **Rejected simpler alternative:** call `self.handle_index` after the wipe. Rejected because
//!   `handle_index` re-acquires the DB write mutex, which rebuild must ALREADY hold across the wipe →
//!   the reindex (`parking_lot::Mutex` is non-reentrant → deadlock); and releasing the guard between
//!   wipe and reindex opens a window where a reader would open a MISSING store — a partial-write a
//!   reader must never see (frozen invariant). Rebuild therefore holds the reader-excluding
//!   coordinator guard across the whole wipe+reindex and drives the shared lower-level reindex fn.
//!
//! # Coordination (cited frozen-invariant sites)
//!
//! Rebuild is reader-destructive (it deletes the store readers serve from), so — like `handle_refresh`
//! (`dispatch.rs`) and unlike an initial `handle_index` — it holds BOTH writer layers:
//! 1. the DB write mutex (`state::DatabaseState`, the universal write barrier an in-flight/detached
//!    index coordinates on) and
//! 2. the repo coordinator's writer guard (`daemon_policy::RepoCoordinator`, FIFO-fair, excludes
//!    active readers — the "Writing" discipline `retention_pass` takes around its VACUUM).
//!
//! `acquire_foreground_write` takes both with bounded patience and, on timeout of EITHER, returns the
//! holder-NAMED `Busy` (`foreground_open::busy_message` reads the activity registry: an in-progress
//! index/refresh, a background enrich/retention pass, or the honest unknown). This is the packet's
//! "bounces with a NAMED Busy when a reader or a detached index holds it": the coordinator layer
//! catches an active reader; the DB-write-mutex layer catches a detached index still persisting into
//! the old store (the 2026-09-04 hazard).

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use repo_graph_daemon_policy::RepoCoordinator;
use repo_graph_daemon_transport::{
    DispatchResult, ErrorCode, ErrorDetail, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_repo_index::compose::{index_path_with_progress, ComposeOptions, ProgressEvent};

use crate::dispatch::ServiceDispatcher;
use crate::foreground_open::{acquire_foreground_write, FOREGROUND_WRITE_PATIENCE};
use crate::state::{DaemonState, RepoState};
use crate::util::compute_storage_root_path;

/// Handle the `repo_rebuild` wire method: wipe the repo's store and reindex from scratch.
///
/// Params: `{ "repo": "<alias|path>", "confirm": true, "include_roots": [..] }`. `confirm` is the
/// daemon-side guard behind the CLI's `--yes`/interactive confirmation (defence in depth: a raw wire
/// call cannot wipe a store without the explicit intent bit). The registry entry (repo_uid, db_path,
/// alias) is KEPT; everything in the store — every snapshot incl. human baseline stamps, seed
/// vectors, measurements, inferences — and the in-memory LiveGraph residency are DISCARDED.
pub(crate) fn handle_repo_rebuild(
    state: &Arc<DaemonState>,
    request: &Request,
    emitter: &mut dyn ProgressEmitter,
) -> DispatchResult {
    let started = Instant::now();

    let repo_ref = match request.params.get("repo").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("missing required parameter: repo"),
            )
        }
    };

    // Explicit intent is REQUIRED daemon-side. The CLI collects `--yes`/an interactive confirmation
    // and sends `confirm: true`; without it, refuse (never wipe on an accidental/raw call).
    let confirmed = request
        .params
        .get("confirm")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !confirmed {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::invalid_request(
                "rebuild not confirmed: pass `--yes` (or confirm interactively) — rebuild discards \
                 every snapshot, baseline stamp, seed vector, measurement and inference for this repo",
            ),
        );
    }

    // Resolve the repo. It MUST already be registered — rebuild is recovery for an indexed repo, not
    // a first index (that is `rmap index`). Capture the identity from the registry entry (repo_uid +
    // db_path are KEPT across the rebuild).
    let (canonical_path, db_path, repo_uid) = match state.resolve_alias_or_path(repo_ref) {
        Some(entry) => (
            entry.canonical_path.clone(),
            entry.db_path.clone(),
            entry.repo_uid.clone(),
        ),
        None => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::RepoNotFound,
                    format!(
                        "repo not indexed: {repo_ref}. `rmap repo rebuild` rebuilds an already-indexed \
                         repo; run `rmap index {repo_ref}` for a first index"
                    ),
                ),
            )
        }
    };

    // The repo root must still exist on disk — we are about to reindex it from source.
    if !canonical_path.is_dir() {
        return DispatchResult::error(
            &request.id,
            ErrorDetail::invalid_request(format!(
                "repo path no longer exists or is not a directory: {} — nothing was discarded",
                canonical_path.display()
            )),
        );
    }

    // ── Coordinator acquisition, sentinel-aware (cycle-3 defect fix) ──
    // An INTERRUPTED rebuild may have already retired the base `.db` (crash after the first rename), so
    // we must NOT route through the DB-validating load path (`RepoState::open` → "database file does
    // not exist", and the load path is itself sentinel-gated) before we even reach the sentinel logic — otherwise
    // the advertised remedy could not rebuild the very failure state the sentinel names. Decide the
    // coordinator we hold up front, BEFORE touching/validating the store:
    //   - sentinel PRESENT (recovery): reuse the SHARED coordinator iff the repo is still loaded (a
    //     failed-restore in THIS daemon process can leave it cached with a reader mid-request); else no
    //     reader can be mid-flight on a sentinel'd store — every load/serve open refuses on the
    //     sentinel (the gated open primitives `open_existing_gated` / `open_existing_with_busy_retry`,
    //     plus the `load_repo` pre-canonicalization guard, all consult `refuse_if_rebuild_interrupted`),
    //     and reads load the repo first — so a standalone coordinator whose writer guard acquires with
    //     NO readers to exclude is correct (`loaded_repo_by_uid`'s documented invariant).
    //   - sentinel ABSENT (normal): `load_repo` (load-and-CACHE) so the shared coordinator is
    //     registered BEFORE any concurrent reader can load the repo — readers then contend on this exact
    //     instance (FIFO-fair exclusion). A load failure here means the store is unreadable AND not
    //     sentinel'd — report it rather than wipe blindly.
    // The DB write mutex layer (`db_runtime`, keyed on canonical parent + filename, independent of the
    // `.db` file's presence) still serializes against any concurrent (re-)index/rebuild in BOTH cases.
    let sentinel_up = sentinel_present(&db_path);
    let standalone_coordinator = RepoCoordinator::new();
    let coord_owner: Option<Arc<RepoState>> = if sentinel_up {
        state.loaded_repo_by_uid(&repo_uid, &db_path)
    } else {
        match state.load_repo(&db_path, &repo_uid) {
            Ok(rs) => Some(rs),
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!(
                            "could not open repo to coordinate the rebuild: {e} — nothing was discarded"
                        ),
                    ),
                )
            }
        }
    };
    let coordinator: &RepoCoordinator = match &coord_owner {
        Some(rs) => &rs.coordinator,
        None => &standalone_coordinator,
    };

    // The DB write coordination slot — the SAME slot `handle_index`/`reclaim::forget_repo` key on
    // (`get_or_create_db_runtime_for_new_db`, canonical-parent + filename), so a concurrent
    // (re-)index contends on it whether or not the `.db` file is present.
    let db_runtime = match state.get_or_create_db_runtime_for_new_db(&db_path) {
        Ok(r) => r,
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    // Acquire BOTH writer layers with bounded patience → a holder-NAMED `Busy` on timeout (the
    // packet's coordinator-FIFO + named-Busy contract). Held across the wipe AND the reindex so no
    // reader ever opens a missing/partial store (frozen invariant), and a detached index holding the
    // DB write mutex bounces here. Both guards drop together at end of scope (reader exclusion ends
    // only once the fresh store is committed).
    // Bind the two guards to SEPARATE locals (not a tuple) so their drop order is reverse-declaration
    // — coordinator-refresh first, then the DB write mutex — matching the historical inline
    // `let _db; let _refresh;` order the `foreground_open` seam documents. (A single tuple binding
    // would drop the DB mutex first, the reverse.)
    let (_db_write_guard, _refresh_guard) = match acquire_foreground_write(
        &db_runtime,
        coordinator,
        state.activity(),
        &db_path,
        FOREGROUND_WRITE_PATIENCE,
    ) {
        Ok(g) => g,
        Err(detail) => return DispatchResult::error(&request.id, detail),
    };

    // Now that we hold the write discipline, make the rebuild visible on `rmap doctor` for its
    // duration (RAII-cleared on every exit). `OpKind::Index` — a rebuild reindexes the repo, so
    // "indexing <repo>" is the honest reader-frame verb; stamped AFTER the guard so the Busy naming
    // above described OTHER holders, never this op.
    let _activity = state.activity().begin(
        crate::activity::OpKind::Index,
        canonical_path.to_string_lossy().to_string(),
        Some(repo_uid.clone()),
        db_path.clone(),
    );

    // ── Prepare the reindex inputs BEFORE touching the store ──
    // Everything that can fail (storage-root resolution, git-HEAD basis) is computed while the old
    // store is still complete on disk, so a setup fault returns with NOTHING discarded. The store is
    // retired only in the immediately-following step, right before the reindex that replaces it.
    let storage_root_path = match compute_storage_root_path(&canonical_path, &db_path) {
        Ok(p) => Some(p),
        Err(e) => {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            )
        }
    };

    let c_include_roots: Vec<String> = request
        .params
        .get("include_roots")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // INDEX-BASIS-1: classify the git HEAD this rebuild is built FROM, exactly as `handle_index` does
    // (compose persists the outcome in the same write flow that writes the snapshot row).
    let basis_probe = crate::index_drift::basis_at_index(&canonical_path);
    if let Err(e) = &basis_probe {
        eprintln!(
            "warning: could not read git HEAD to stamp index basis for {}: {e}",
            canonical_path.display()
        );
    }
    let basis_outcome = crate::index_drift::basis_outcome_from_probe(&canonical_path, basis_probe);
    let basis_commit = basis_outcome.basis_commit();

    let options = ComposeOptions {
        c_include_roots,
        storage_root_path,
        basis_commit,
        basis_outcome: Some(basis_outcome),
        ..ComposeOptions::default()
    };

    // ── Mark the store "rebuilding" (detect-and-name) BEFORE touching any file ──
    // HUMAN RULING (cycle 2): the multi-file retire is SEQUENTIAL — the guarantee is "never SERVE a
    // partial store", not a single-step multi-file replace. Instead a
    // sentinel is written + fsync'd BEFORE the first rename, so a crash anywhere in the retire/reindex
    // window leaves a DISCOVERABLE marker; every store-open path then REFUSES with a NAMED reason
    // (`refuse_if_rebuild_interrupted`) until the user re-runs the verb. Written before retire so no
    // crash window is unmarked; removed only after the fresh store commits (success) or the old store
    // is restored (failure).
    if let Err(detail) = write_rebuild_sentinel(&db_path) {
        return DispatchResult::error(&request.id, detail);
    }

    // ── Retire the store aside under the held write discipline ──
    // NOT an in-place delete: each store file is RENAMED to a sibling `.retired` staging path. rename(2)
    // replaces a single name in one filesystem step, but the multi-file retire as a whole is a sequence
    // of four such renames — the
    // sentinel above, not this sequence, is what guarantees a partial retire is never SERVED. The retire
    // still rolls back on a mid-way fault (every rename so far undone → the ORIGINAL store complete) so
    // the common failure leaves nothing discarded; the sentinel covers the uncommon crash. The retired
    // copy is kept until the reindex commits: `discard`ed on success, `restore`d on reindex failure.
    let retired = match retire_store_files(&db_path) {
        Ok(r) => r,
        Err(mut detail) => {
            // The retire rolled back to the ORIGINAL complete store, so clear the sentinel — the store
            // is servable again; keeping it would wrongly report "rebuild interrupted". If the unlink
            // itself fails, the restored store stays GATED, so fold that into the error rather than
            // imply the store is servable when it is not (review-6 item 4).
            if let Err(sentinel_err) = remove_rebuild_sentinel(&db_path) {
                detail.message = format!(
                    "{}; additionally, {sentinel_err} — the restored store will not be served until \
                     that file is removed",
                    detail.message
                );
            }
            return DispatchResult::error(&request.id, detail);
        }
    };

    // Progress callback: tee phase/counters into the activity record so `rmap doctor` renders live
    // progress, and best-effort emit to the client (a dead socket must never abort the reindex —
    // INDEX-DISCONNECT-1). Never returns `Break` on transport failure.
    let mut client_gone = false;
    let mut progress_callback = |event: &ProgressEvent| -> ControlFlow<()> {
        _activity.update(&event.phase, event.current, event.total);
        if client_gone {
            return ControlFlow::Continue(());
        }
        if emitter
            .emit(ProgressDetail {
                phase: event.phase.clone(),
                current: event.current,
                total: event.total,
            })
            .is_err()
        {
            client_gone = true;
            crate::detached::log_detached_continuation("rebuild", &repo_uid);
        }
        ControlFlow::Continue(())
    };

    crate::oplog::log_op_start("rebuild", &repo_uid, None);

    match index_path_with_progress(
        &canonical_path,
        &db_path,
        &repo_uid,
        &options,
        Some(&mut progress_callback),
    ) {
        Ok(result) => {
            // The fresh store is committed at `db_path`; the retired copy is now dead weight. Best
            // effort — a leftover `.retired` staging file is inert junk (never a servable store) and the
            // authoritative fresh store already exists.
            retired.discard();

            // Store is complete and safe to serve again → clear the sentinel. Ordered EXACTLY as the
            // ruling requires: AFTER the reindex commits and `retired.discard()`. Every subsequent
            // open below (classify/retention/serving) must find NO sentinel, or it would refuse.
            // review-6 item 4: if the unlink FAILS the fresh store is complete but WILL NOT be served
            // (every open refuses on the retained sentinel), so this is a NAMED non-success outcome —
            // never a bare "rebuilt" success while the store is un-servable. The sentinel is retained
            // (the unlink failed); the remedy is to remove that file or re-run the verb.
            if let Err(sentinel_err) = remove_rebuild_sentinel(&db_path) {
                crate::oplog::log_op_outcome(
                    "rebuild",
                    &repo_uid,
                    Some(&result.snapshot_uid),
                    "reindexed but sentinel not cleared",
                );
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!(
                            "rebuild reindexed the store successfully but {sentinel_err} — the fresh \
                             store is complete yet will NOT be served while the sentinel {} exists; \
                             remove that file, or re-run `rmap repo rebuild`, to serve it",
                            rebuild_sentinel_path(&db_path).display()
                        ),
                    ),
                );
            }

            crate::oplog::log_op_outcome(
                "rebuild",
                &repo_uid,
                Some(&result.snapshot_uid),
                "completed",
            );

            // Refresh the registry's last-indexed stamp (the entry itself was kept).
            {
                let mut registry = state.registry_mut();
                if let Err(e) = registry.record_index(
                    &canonical_path,
                    crate::util::utc_now_iso8601(),
                    result.snapshot_uid.clone(),
                ) {
                    eprintln!("warning: failed to update registry after rebuild: {e}");
                }
                if let Err(e) = registry.save() {
                    eprintln!("warning: failed to save registry after rebuild: {e}");
                }
            }

            let mut response = serde_json::json!({
                "repo_uid": repo_uid,
                "canonical_path": canonical_path,
                "db_path": db_path,
                "snapshot_uid": result.snapshot_uid,
                "files_total": result.files_total,
                "nodes_total": result.nodes_total,
                "edges_total": result.edges_total,
                "edges_unresolved": result.edges_unresolved,
                // The verb's own reader-frame summary: what it discarded + how long it took.
                "rebuild": {
                    "discarded": "all snapshots, baseline stamps, seed vectors, measurements, \
                                  inferences and the in-memory LiveGraph residency",
                    "registry_entry_kept": true,
                    "duration_secs": started.elapsed().as_secs(),
                },
            });

            // REFRESH-HANG-1: classify only on the foreground path; the actual prune is the async
            // chain queued by `finish_write_with_maintenance` below. Reload the repo for the storage
            // handle: the sentinel was cleared above and the fresh store exists, so the (now ungated)
            // load succeeds even when we took the recovery branch (where `coord_owner` may be None). A
            // reload failure is non-fatal — classification is a reporting nicety, not the write path.
            match state
                .load_repo(&db_path, &repo_uid)
                .and_then(|rs| rs.storage())
            {
                Ok(storage) => {
                    match crate::handlers::inventory::classify_retention_only(&storage, &repo_uid) {
                        Ok(lifecycle) => {
                            response["retention"] = serde_json::json!({
                                "pruned_count": lifecycle.pruned_count,
                                "prunable_count": lifecycle.prunable_count,
                                "current": lifecycle.stats.current,
                                "parent": lifecycle.stats.parent,
                                "total": lifecycle.stats.total,
                            });
                        }
                        Err(e) => eprintln!(
                        "warning: retention classification failed after rebuild for {repo_uid}: {e}"
                    ),
                    }

                    // review-6 item 2: the REAL symbol count for the fresh snapshot — the repo-level
                    // all-SYMBOL COUNT(*) `compute_repo_summary().symbol_count` (the same figure `orient`
                    // shows), NOT `nodes_total` (which is COUNT(*) over EVERY node kind, the "nodes (all
                    // kinds)" superset `index` reports). Additive `symbols_total` wire field. On a summary
                    // error we OMIT it — never inject 0, never relabel `nodes_total` — and the CLI renders
                    // a NAMED gap (honesty rule 2; the D4 omit-on-error precedent in dispatch.rs).
                    match repo_graph_agent::AgentStorageRead::compute_repo_summary(
                        &storage,
                        &result.snapshot_uid,
                    ) {
                        Ok(summary) => {
                            response["symbols_total"] = serde_json::json!(summary.symbol_count);
                        }
                        Err(e) => eprintln!(
                            "warning: repo summary (symbol count) unavailable after rebuild for \
                             {repo_uid}: {e}"
                        ),
                    }
                }
                Err(e) => eprintln!(
                    "warning: retention classification skipped (repo reload/storage open failed) \
                     after rebuild for {repo_uid}: {e}"
                ),
            }

            // Discard the in-memory LiveGraph residency: the old graph belongs to the wiped store.
            // Done AFTER the reindex + classify (which still use the loaded coordinator) but the
            // eviction itself only removes the in-memory entry — the fresh store on disk is
            // authoritative and the next query reloads from it. `_guards`/`_activity` are still held
            // here (dropped at function end); eviction removes the map entry while either our
            // `coord_owner` Arc (shared-coordinator case) or the `standalone_coordinator` local keeps
            // the coordinator alive for the guards.
            state.evict_repo_memory(&repo_uid, &db_path);

            // Queue the SAME async enrich→seed→retention chain an index queues, so the fresh
            // snapshot's prune-on-commit cap holds exactly as proven for `index`.
            ServiceDispatcher::finish_write_with_maintenance(
                state,
                &request.id,
                response,
                &db_path,
                &repo_uid,
                canonical_path.to_string_lossy().to_string(),
            )
        }
        Err(e) => {
            crate::oplog::log_op_outcome("rebuild", &repo_uid, None, &format!("failed: {e}"));
            // Reindex failed: reinstate the retired store so the repo is left as it was before the
            // rebuild (review-0 item 1 — never a partial/absent store on failure).
            match retired.restore(&db_path) {
                Ok(()) => {
                    // Old store is back intact and servable → clear the sentinel. A sentinel-unlink
                    // failure leaves the restored store GATED, so NAME it rather than imply it is
                    // servable (review-6 item 4).
                    let sentinel_note = match remove_rebuild_sentinel(&db_path) {
                        Ok(()) => String::new(),
                        Err(sentinel_err) => format!(
                            "; additionally, {sentinel_err} — the restored store will not be served \
                             until that file is removed"
                        ),
                    };
                    DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InternalError,
                            format!(
                                "rebuild reindex failed: {e} — the previous store was restored intact; \
                                 nothing was discarded{sentinel_note}"
                            ),
                        ),
                    )
                }
                Err(restore_err) => {
                    // Restore ALSO failed → the store may be incomplete. KEEP the sentinel so every
                    // open path refuses to serve it (detect-and-name); the user's remedy is exactly the
                    // verb the message names.
                    DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InternalError,
                            format!(
                                "rebuild reindex failed: {e}; restoring the previous store ALSO failed: \
                                 {restore_err} — the store at {} may be incomplete; it is marked \
                                 'rebuild interrupted' and will not be served — re-run `rmap repo \
                                 rebuild` once the fault is cleared",
                                db_path.display()
                            ),
                        ),
                    )
                }
            }
        }
    }
    // `_activity` + `_guards` drop here (op deregistered, DB write mutex + coordinator writer released).
}

/// Suffix appended to a store file's name when it is retired aside during a rebuild
/// (`foo.db` → `foo.db.retired`). A file bearing this suffix is a rebuild staging artifact, never a
/// servable store; a leftover one is inert junk from a crashed rebuild. Distinct from the
/// [`REBUILD_SENTINEL_SUFFIX`] (`.rebuilding`), which is the MEANINGFUL crash marker — a leftover
/// `.rebuilding` file makes every store-open path REFUSE (detect-and-name), whereas a leftover
/// `.retired` file is harmless junk.
const RETIRE_SUFFIX: &str = ".retired";

/// Suffix of the rebuild SENTINEL file (`foo.db` → `foo.db.rebuilding`) — the detect-and-name marker
/// (HUMAN RULING, cycle 2). Written (and fsync'd) BEFORE the first store-file rename and removed only
/// AFTER the reindex commits and the retired copy is discarded. Its PRESENCE means a rebuild did not
/// finish: the store files may be mid-retire or a partial fresh index, so every open path must refuse
/// to serve them. The multi-file retire is SEQUENTIAL, not a single-step multi-file replace; the
/// sentinel makes the failure DETECTABLE and NAMED — a crash costs one re-run of `rmap repo rebuild`.
const REBUILD_SENTINEL_SUFFIX: &str = ".rebuilding";

/// The exact NAMED reason every store-open path returns when the rebuild sentinel is present (HUMAN
/// RULING, cycle 2). Kept as one const so the sentinel authority ([`refuse_if_rebuild_interrupted`],
/// called by the gated open primitives `state::open_existing_gated` /
/// `state::open_existing_with_busy_retry`, the `load_repo` pre-canonicalization guard, and the
/// `handle_index` create-path gate), the doctor render (`snapshot_facts`/`storage_probe`), and the
/// unit tests all assert the SAME text.
pub(crate) const REBUILD_INTERRUPTED_REASON: &str =
    "rebuild interrupted — run `rmap repo rebuild <path>` again";

/// Path of the rebuild sentinel for the store at `db_path` (`<db_path>.rebuilding`). The one shared
/// convention used by the writer ([`write_rebuild_sentinel`]), the remover ([`remove_rebuild_sentinel`]),
/// and every reader (the open gates + the doctor probe). A daemon-runtime concept, NOT a storage-crate
/// one: the sentinel names the rebuild verb's coordination state, so it lives beside `rebuild`, not in
/// the lower-level storage mechanism.
pub(crate) fn rebuild_sentinel_path(db_path: &Path) -> PathBuf {
    sidecar(db_path, REBUILD_SENTINEL_SUFFIX)
}

/// Write the rebuild sentinel and make it DURABLE before returning: the file's bytes are fsync'd and
/// the containing directory entry is fsync'd, so a crash immediately after this call still leaves a
/// discoverable sentinel (the whole point — the marker must survive the crash it protects against).
/// Called BEFORE the first store-file rename in [`retire_store_files`].
fn write_rebuild_sentinel(db_path: &Path) -> Result<(), ErrorDetail> {
    let sentinel = rebuild_sentinel_path(db_path);
    let write_and_sync = || -> std::io::Result<()> {
        // Truncate-create: a leftover sentinel from a prior crashed rebuild is simply overwritten (we
        // are the remedy for it). Content is a human note; presence is what matters.
        let f = std::fs::File::create(&sentinel)?;
        use std::io::Write as _;
        let mut f = f;
        f.write_all(
            b"rmap: a `rmap repo rebuild` is in progress or was interrupted for this store.\n\
              While this file exists the store is NOT served. Re-run `rmap repo rebuild <path>`.\n",
        )?;
        f.sync_all()?;
        // fsync the directory so the new entry itself is durable (not just the file's data).
        if let Some(parent) = sentinel.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    };
    write_and_sync().map_err(|e| {
        ErrorDetail::new(
            ErrorCode::InternalError,
            format!(
                "could not write the rebuild sentinel {} — nothing was discarded: {e}",
                sentinel.display()
            ),
        )
    })
}

/// Remove the rebuild sentinel once the rebuild has reached a state where the store is safe to serve
/// again — either the fresh store committed (success) or the old store was restored intact (failure).
///
/// `Ok(())` iff the sentinel is now gone (removed, or already absent — a missing sentinel is the goal).
/// A REAL unlink fault returns a NAMED reason (review-6 item 4): while the sentinel survives, every
/// store-open path correctly REFUSES the (otherwise intact) store, so the caller must NOT report a
/// bare success — it turns the failure into a NAMED non-success outcome with the sentinel retained.
fn remove_rebuild_sentinel(db_path: &Path) -> Result<(), String> {
    let sentinel = rebuild_sentinel_path(db_path);
    // Test-only fault injection (review-7 required change 1): force the NAMED unlink fault on THIS
    // thread so the success-path failure branch — fresh store committed but un-servable because the
    // sentinel could not be cleared (review-6 item 4) — can be driven end-to-end through the
    // dispatcher (`tests/repo_rebuild.rs`). This state is unreachable via the filesystem alone:
    // `remove_rebuild_sentinel` and the `write_rebuild_sentinel` that precedes it act on the SAME path,
    // so any obstruction that blocks the unlink blocks the create first (the rebuild would abort at
    // sentinel-write, not sentinel-unlink). A THREAD-LOCAL (not a process-global): the flag is observed
    // on the dispatching thread that runs `remove_rebuild_sentinel` inline, and never leaks into a peer
    // rebuild test running concurrently in the same binary (a process-global flag would). Always
    // compiled — the same always-on `#[doc(hidden)] pub fn set_auto_*_for_test` seam convention this
    // crate uses (`seed::set_auto_seed_for_test`, `enrich_pass::set_auto_enrich_for_test`); the check is
    // one thread-local bool load, and the flag is `false` in every production process.
    if FAIL_SENTINEL_REMOVAL_FOR_TEST.with(|c| c.get()) {
        return Err(format!(
            "could not remove the rebuild sentinel {}: injected test fault",
            sentinel.display()
        ));
    }
    match std::fs::remove_file(&sentinel) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "could not remove the rebuild sentinel {}: {e}",
            sentinel.display()
        )),
    }
}

thread_local! {
    /// Test-only sentinel-removal fault flag (see the guard in [`remove_rebuild_sentinel`]). Thread-local
    /// so concurrent tests in the same binary never see each other's fault; set via
    /// [`set_fail_sentinel_removal_for_test`]. Default `false` — a no-op in every production process.
    static FAIL_SENTINEL_REMOVAL_FOR_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test seam: force [`remove_rebuild_sentinel`] to return its NAMED unlink fault on the CURRENT thread,
/// so the success-path "reindexed but sentinel not cleared" branch can be driven end-to-end through the
/// dispatcher (review-7). Mirrors the always-compiled `#[doc(hidden)] pub fn set_auto_*_for_test`
/// convention (`seed::set_auto_seed_for_test`). Set `true` before dispatching the rebuild whose
/// post-reindex sentinel-unlink must fail; reset to `false` afterwards. No production caller.
#[doc(hidden)]
pub fn set_fail_sentinel_removal_for_test(fail: bool) {
    FAIL_SENTINEL_REMOVAL_FOR_TEST.with(|c| c.set(fail));
}

/// True iff the rebuild sentinel is DEFINITELY present for `db_path` — used by the doctor probe to
/// render the interrupted state deterministically. Only `Ok(true)` counts as present; a `NotFound`
/// (`Ok(false)`) or an ambiguous stat error (`Err`) is NOT reported as interrupted here — the open
/// gate ([`refuse_if_rebuild_interrupted`]) is the authority that refuses on the ambiguous case.
pub(crate) fn sentinel_present(db_path: &Path) -> bool {
    matches!(rebuild_sentinel_path(db_path).try_exists(), Ok(true))
}

/// The ONE sentinel AUTHORITY (HUMAN RULING, cycle 2; single-seam consolidation, cycle 5): refuse to
/// open a store whose rebuild sentinel is present, with the exact [`REBUILD_INTERRUPTED_REASON`]. To
/// stop the per-caller bypasses three cycles found (`load_repo` → `reconcile` → `enrich_pass`), this
/// is now consulted from a MINIMAL, fixed set of sites — every store OPEN funnels through one of two
/// gated primitives that call it:
///   - `state::open_existing_gated` — the general gated open (RepoState::open, reconcile, enrich, and
///     rgr's client read all route here);
///   - `state::open_existing_with_busy_retry` — the serving primitive (its own RAW-error loop, §2.3).
///
/// Two paths the open primitives cannot cover consult it directly: `DaemonState::load_repo` (a
/// PRE-CANONICALIZATION guard — `RepoKey::new` canonicalizes a possibly-retired `.db` before any
/// open), and `handle_index` (the `compose::open` CREATE path, not an `open_existing`). NOT called by
/// `handle_repo_rebuild` itself (it is the remedy; it reindexes via the create path and never opens a
/// sentinelled store through these primitives). Conservative on an ambiguous stat error: if we cannot
/// determine the sentinel's presence we REFUSE rather than risk serving a mid-rebuild store.
pub(crate) fn refuse_if_rebuild_interrupted(db_path: &Path) -> Result<(), String> {
    match rebuild_sentinel_path(db_path).try_exists() {
        Ok(false) => Ok(()),
        Ok(true) => Err(REBUILD_INTERRUPTED_REASON.to_string()),
        Err(e) => Err(format!(
            "could not check the rebuild sentinel {} (refusing to open the store): {e}",
            rebuild_sentinel_path(db_path).display()
        )),
    }
}

/// The repo's store files renamed aside by [`retire_store_files`], recorded as `(original, staged)`.
///
/// Existence of a `RetiredStore` means the old store lives at the `staged` paths and `db_path` is free
/// for a fresh index. Exactly one terminal move is taken: [`discard`](Self::discard) once the fresh
/// store commits, or [`restore`](Self::restore) if the reindex fails — so on the NON-crash paths the
/// on-disk outcome is the old complete store or the new complete store. A crash BETWEEN the per-file
/// renames can still leave a mixed set; that residue is never SERVED because the rebuild sentinel is
/// present (detect-and-name) — this type handles the recoverable faults, the sentinel handles the crash.
///
/// Abstraction ledger: a small owned record with three cohesive operations (rollback / discard /
/// restore) over one set of `(original, staged)` renames. Concrete current user: `handle_repo_rebuild`
/// (one caller). Axis of variation: none — it exists to localize the retire/rollback moves the
/// review requires, keeping the fault-recovery moves out of the handler's happy path. Rejected simpler
/// alternative: the prior in-place sequential delete, which is exactly the partial-drop defect
/// (review-0 item 1); and a bare `Vec<(PathBuf, PathBuf)>` with free functions, rejected because the
/// three moves share the same invariant (each staged path is the reverse of its original) and read
/// clearer as named methods on the owned set.
#[derive(Debug)]
struct RetiredStore {
    /// `(original, staged)` for each store file that existed and was renamed aside, in retire order.
    files: Vec<(PathBuf, PathBuf)>,
}

impl RetiredStore {
    /// Drop the retired copy after the fresh store has committed. Best-effort: a leftover staging file
    /// is inert junk (never a servable store), so a failure here is logged, not fatal.
    fn discard(self) {
        for (_original, staged) in self.files {
            if let Err(e) = std::fs::remove_file(&staged) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!(
                        "warning: could not remove retired store file {} after rebuild: {e} \
                         (harmless leftover)",
                        staged.display()
                    );
                }
            }
        }
    }

    /// Reinstate the retired store after a FAILED reindex: clear whatever partial fresh store the
    /// failed index wrote to the live locations, then rename each retired file back. Leaves the repo
    /// exactly as it was before the rebuild. Returns the first fault that prevented a clean restore
    /// (the store may then be incomplete and need manual recovery — surfaced by the caller).
    fn restore(self, db_path: &Path) -> Result<(), String> {
        // Clear any partial fresh store at the live locations first, so a fresh sidecar can never
        // survive to pair with a restored `.db`.
        remove_store_files_inplace(db_path).map_err(|(p, e)| {
            format!(
                "could not clear the partial fresh store file {}: {e}",
                p.display()
            )
        })?;
        for (original, staged) in self.files {
            std::fs::rename(&staged, &original).map_err(|e| {
                format!(
                    "could not move retired store file {} back to {}: {e}",
                    staged.display(),
                    original.display()
                )
            })?;
        }
        Ok(())
    }

    /// Undo the renames done so far (used when a retire is aborted mid-way): move each retired file
    /// back to its original. Best-effort — each target was just vacated by this same routine, so the
    /// reverse rename is as reliable as the forward one; a residual failure is logged.
    fn rollback(&mut self) {
        while let Some((original, staged)) = self.files.pop() {
            if let Err(e) = std::fs::rename(&staged, &original) {
                eprintln!(
                    "warning: could not roll back retired store file {} to {} during a failed \
                     rebuild retire: {e}",
                    staged.display(),
                    original.display()
                );
            }
        }
    }
}

/// Candidate store file paths for a repo whose base DB is `db_path`: the `-wal`/`-shm` sidecars, the
/// base `.db`, and the `.vec` seed sidecar. Sidecars precede the base `.db` so an in-place delete
/// never leaves a `.db` paired with a stale WAL. Only those that EXIST are acted on by callers.
fn store_file_candidates(db_path: &Path) -> Vec<PathBuf> {
    let mut c = vec![
        sidecar(db_path, "-wal"),
        sidecar(db_path, "-shm"),
        db_path.to_path_buf(),
    ];
    // The `.vec` seed sidecar lives beside the DB (a sibling under the state root).
    if let Some(vec_path) = crate::seed::sidecar_path(db_path) {
        c.push(vec_path);
    }
    c
}

/// Retire the repo's store files aside so a fault leaves the ORIGINAL store complete (review-0 item 1).
///
/// Each EXISTING store file is renamed to a sibling `<name>.retired` staging path. rename(2) replaces
/// one name in a single filesystem step. If ANY rename fails, the renames already done are rolled back (each retired file
/// moved back to its original) and the fault is returned — the caller has NOT yet reindexed, so the
/// store is left exactly as it was. Safe under the held write discipline: no other writer touches the
/// store, so the `exists()`→`rename` sequence has no racing mutator.
fn retire_store_files(db_path: &Path) -> Result<RetiredStore, ErrorDetail> {
    let mut retired = RetiredStore { files: Vec::new() };
    for original in store_file_candidates(db_path) {
        if !original.exists() {
            continue;
        }
        let staged = sidecar(&original, RETIRE_SUFFIX);
        // A staging file here is a leftover from a prior CRASHED rebuild (never a live store file);
        // clear it so the rename cannot silently clobber or fail ambiguously. If even that fails,
        // abort the whole retire (rollback) rather than proceed.
        if staged.exists() {
            if let Err(e) = std::fs::remove_file(&staged) {
                retired.rollback();
                return Err(ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!(
                        "could not clear a stale rebuild staging file {} before retiring the store: \
                         {e} — the previous store was left intact; nothing was discarded",
                        staged.display()
                    ),
                ));
            }
        }
        match std::fs::rename(&original, &staged) {
            Ok(()) => retired.files.push((original, staged)),
            Err(e) => {
                retired.rollback();
                return Err(ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!(
                        "could not retire store file {} during rebuild: {e} — the previous store was \
                         left intact; nothing was discarded",
                        original.display()
                    ),
                ));
            }
        }
    }
    Ok(retired)
}

/// Delete the repo's store files in place (`db_path` + sidecars). `NotFound` is fine (nothing there);
/// the first OTHER I/O fault returns `(path, error)`. Used by [`RetiredStore::restore`] to clear a
/// partial fresh store before reinstating the retired one.
fn remove_store_files_inplace(db_path: &Path) -> Result<(), (PathBuf, std::io::Error)> {
    for path in store_file_candidates(db_path) {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err((path, e)),
        }
    }
    Ok(())
}

/// `db_path` with a suffix appended to its file name (`foo.db` + `-wal` → `foo.db-wal`), matching
/// SQLite's WAL/SHM sidecar naming (the same construction `reclaim::sidecar` uses).
fn sidecar(db_path: &Path, suffix: &str) -> PathBuf {
    let mut s = db_path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // review-0 item 1: a retire fault rolls back → the ORIGINAL store is left complete, never partial.
    #[test]
    fn retire_rolls_back_on_fault_leaving_the_original_store_complete() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("repo.db");
        let wal = dir.path().join("repo.db-wal");
        fs::write(&db, "DB-OLD").unwrap();
        fs::write(&wal, "WAL-OLD").unwrap();

        // Force the base `.db` retire to fail: occupy its staging path with a DIRECTORY, which
        // `remove_file` cannot clear and onto which a file cannot be renamed. `-wal` is retired first
        // (candidate order) and MUST be rolled back when the `.db` step aborts.
        let blocker = sidecar(&db, RETIRE_SUFFIX); // repo.db.retired
        fs::create_dir(&blocker).unwrap();

        let err =
            retire_store_files(&db).expect_err("retire must fail when a staging path is blocked");
        assert!(
            err.message.contains("left intact"),
            "error names the intact store: {}",
            err.message
        );

        // The ORIGINAL store is complete and unchanged — no partial state.
        assert_eq!(
            fs::read_to_string(&db).unwrap(),
            "DB-OLD",
            "base db untouched"
        );
        assert_eq!(
            fs::read_to_string(&wal).unwrap(),
            "WAL-OLD",
            "wal rolled back to its original"
        );
        assert!(
            !sidecar(&wal, RETIRE_SUFFIX).exists(),
            "no retired wal left at its staging path after rollback"
        );
    }

    // review-0 item 1: on a FAILED reindex, restore clears the partial fresh store and reinstates the
    // retired one → the repo is left exactly as before (old complete store, not partial/absent).
    #[test]
    fn restore_clears_partial_fresh_store_and_reinstates_the_retired_one() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("repo.db");
        let wal = dir.path().join("repo.db-wal");
        fs::write(&db, "DB-OLD").unwrap();
        fs::write(&wal, "WAL-OLD").unwrap();

        let retired = retire_store_files(&db).expect("retire ok");
        assert!(!db.exists() && !wal.exists(), "originals retired aside");
        assert!(sidecar(&db, RETIRE_SUFFIX).exists(), "db staged");

        // Simulate a partial fresh store written by a reindex that then FAILED.
        fs::write(&db, "DB-NEW-PARTIAL").unwrap();
        fs::write(dir.path().join("repo.db-shm"), "SHM-NEW-PARTIAL").unwrap();

        retired.restore(&db).expect("restore ok");

        // The OLD store is back byte-for-byte; the partial fresh remnants are gone; no staging leftover.
        assert_eq!(
            fs::read_to_string(&db).unwrap(),
            "DB-OLD",
            "old db reinstated"
        );
        assert_eq!(
            fs::read_to_string(&wal).unwrap(),
            "WAL-OLD",
            "old wal reinstated"
        );
        assert!(
            !dir.path().join("repo.db-shm").exists(),
            "partial fresh shm cleared"
        );
        assert!(
            !sidecar(&db, RETIRE_SUFFIX).exists(),
            "no retired leftover after restore"
        );
    }

    // HUMAN RULING (cycle 2): the sentinel is written durably and, while present, the open GATE
    // refuses with the EXACT named reason; removing it lets opens proceed again.
    #[test]
    fn sentinel_write_makes_open_gate_refuse_with_the_exact_reason_and_remove_clears_it() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("repo.db");
        fs::write(&db, "DB").unwrap();

        // No sentinel yet → the gate allows the open.
        assert!(refuse_if_rebuild_interrupted(&db).is_ok());
        assert!(!sentinel_present(&db), "no sentinel before a rebuild");

        write_rebuild_sentinel(&db).expect("sentinel writes");
        assert!(sentinel_present(&db), "sentinel present after write");
        assert!(
            rebuild_sentinel_path(&db).exists(),
            "sentinel is a real file beside the store"
        );
        // The gate refuses with the EXACT named reason (what every open path returns).
        assert_eq!(
            refuse_if_rebuild_interrupted(&db).unwrap_err(),
            REBUILD_INTERRUPTED_REASON,
            "open refused with the exact ruling-named reason"
        );

        remove_rebuild_sentinel(&db).expect("sentinel removes cleanly");
        assert!(!sentinel_present(&db), "sentinel cleared");
        assert!(
            refuse_if_rebuild_interrupted(&db).is_ok(),
            "the gate allows opens again once the sentinel is gone"
        );

        // Idempotent: removing an ALREADY-absent sentinel is Ok (a missing sentinel is the goal), not
        // a fault to surface.
        remove_rebuild_sentinel(&db).expect("removing an absent sentinel is Ok");
    }

    // review-6 item 4: a sentinel-unlink FAILURE is a NAMED reason (the caller turns it into a
    // non-success outcome). Force the failure by occupying the sentinel PATH with a directory, which
    // `remove_file` cannot unlink — the store is otherwise intact, so the NAMED reason is what keeps the
    // caller from reporting a bare success while every open still refuses the retained sentinel.
    #[test]
    fn remove_rebuild_sentinel_names_the_fault_when_unlink_fails() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("repo.db");
        fs::write(&db, "DB").unwrap();

        // A directory at the sentinel path: `remove_file` fails (it is not a regular file), so the
        // helper must return a NAMED Err, never a swallowed Ok.
        fs::create_dir(rebuild_sentinel_path(&db)).unwrap();

        let err = remove_rebuild_sentinel(&db)
            .expect_err("remove_file on a directory must fail, not be swallowed as Ok");
        assert!(
            err.contains("could not remove the rebuild sentinel"),
            "names the fault: {err}"
        );
        assert!(
            err.contains("repo.db.rebuilding"),
            "names the sentinel path: {err}"
        );
    }

    // The sentinel (`.rebuilding`) and the retire staging suffix (`.retired`) MUST NOT collide — the
    // base `.db` retires to `<db>.retired`, leaving `<db>.rebuilding` free as the exclusive sentinel.
    #[test]
    fn sentinel_and_retire_staging_paths_do_not_collide() {
        let db = Path::new("/x/repo.db");
        assert_eq!(
            rebuild_sentinel_path(db),
            Path::new("/x/repo.db.rebuilding")
        );
        assert_eq!(sidecar(db, RETIRE_SUFFIX), Path::new("/x/repo.db.retired"));
        assert_ne!(rebuild_sentinel_path(db), sidecar(db, RETIRE_SUFFIX));
    }

    // Happy terminal move: after a successful reindex the retired copy is removed, fresh store intact.
    #[test]
    fn discard_removes_the_retired_copy_and_leaves_the_fresh_store() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("repo.db");
        fs::write(&db, "DB-OLD").unwrap();
        let retired = retire_store_files(&db).expect("retire ok");
        // Fresh store now lives at db_path.
        fs::write(&db, "DB-NEW").unwrap();
        retired.discard();
        assert!(
            !sidecar(&db, RETIRE_SUFFIX).exists(),
            "retired copy discarded"
        );
        assert_eq!(
            fs::read_to_string(&db).unwrap(),
            "DB-NEW",
            "fresh store untouched by discard"
        );
    }
}
