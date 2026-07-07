//! INDEX-DISCONNECT-1 named proofs — a WRITE op (index / refresh) survives its client.
//!
//! Driven through the REAL `ServiceDispatcher::dispatch` surface against REAL in-flight index/refresh
//! writes on an isolated `DaemonState` (its own temp state root; the operator's registry/daemon are
//! never touched), mirroring `tests/daemon_visibility.rs`.
//!
//! Contract (`docs/slices/index-disconnect-1.md` §3) proven here:
//! - **Item 1 — progress emission is best-effort** (`disconnect_during_index_completes_to_ready` /
//!   `disconnect_during_refresh_completes_to_ready`): an emitter that starts failing mid-op (a dead
//!   client's closed socket) does NOT abort the write — the index/refresh runs to completion, the
//!   snapshot reaches READY, `record_index` is persisted, the handler stops emitting after the first
//!   failure, AND (review-0 change #3) the test DIRECTLY observes the ONE detached-continuation log
//!   line via the `detached` capture seam (not just the emit-call count).
//! - **Item 2 — registration persists up-front** (`failed_index_after_registration_leaves_repo_registered`):
//!   a REAL driven index failure (DB creation denied, before any snapshot) leaves the repo persisted
//!   in the on-disk registry (queryable by path AND uid), with no `last_snapshot_uid` — proving the
//!   entry came from the up-front save, not the success-branch save.
//! - **Item 3 — no `building` limbo, honestly surfaced**
//!   (`failed_index_leaves_terminal_snapshot_and_reader_surface_agree`, review-0 change #2): a snapshot
//!   put into the exact terminal state a post-creation failure leaves (`failed`) is (a) still queryable
//!   in the persisted registry by path AND uid, (b) terminal non-`building`, and (c) NAMED as
//!   interrupted (with its outcome reason) by the `repo info` reader surface — the three views agree.
//!
//! The REAL `Building`→`Failed` transition after an in-pipeline failure — and the "explicit cancel
//! still cancels" guarantee (item 4) — are proven deterministically at the orchestrator level:
//! `repo_graph_indexer::orchestrator::tests::explicit_cancel_during_pipeline_leaves_failed_snapshot`.
//! (After item 1 the daemon callback never feeds the orchestrator a `Break`, so the abort seam is only
//! exercisable where a `Break` can be injected directly; the daemon dispatch path offers no
//! deterministic post-snapshot Failed-snapshot injection — hence item 3's index→flip technique, the
//! same one the accepted `daemon_visibility.rs` F2 proofs use.)

use std::path::Path;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_storage::types::UpdateSnapshotStatusInput;
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

// ── Harness ──────────────────────────────────────────────────────────────────

/// A progress emitter that discards events (for the success setup index).
struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Emits OK `fail_after` times, then fails on every subsequent emit — the daemon-side shape of a
/// client that disconnects mid-op (the socket closes; the next progress write gets `EPIPE`). Counts
/// total emit calls so a test can prove the handler STOPS emitting after the first failure.
struct FailAfter {
    fail_after: usize,
    calls: usize,
}
impl FailAfter {
    fn new(fail_after: usize) -> Self {
        Self {
            fail_after,
            calls: 0,
        }
    }
}
impl ProgressEmitter for FailAfter {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        self.calls += 1;
        if self.calls > self.fail_after {
            Err(EmitError::new(
                "simulated client disconnect (socket closed)",
            ))
        } else {
            Ok(())
        }
    }
}

fn isolated() -> (ServiceDispatcher, TempDir) {
    // SNAPSHOT-RETENTION-1: this suite opens a RAW SQLite connection (bypassing the repo coordinator) to
    // read terminal snapshot status, and creates non-READY snapshots the auto-retention pass would
    // reclaim — changing the very terminal state it asserts. Disable the background actor: this binary
    // tests INDEX-DISCONNECT, not retention. (Production reads go through the coordinator, so the pass's
    // VACUUM excludes them honestly — proven in `retention_pass`'s reader-vs-VACUUM tests; a raw
    // connection has no such guard, which is why the disable is honest here, not a papered-over race.)
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    // ENRICH-LIFECYCLE-1: auto-enrichment is the SECOND background write-lock + activity actor spawned
    // on index/refresh completion (same class as auto-retention above). It would race this binary's raw
    // terminal-status reads and hold the write lock over the very snapshot state it asserts. This binary
    // tests INDEX-DISCONNECT, not enrichment (proven directly in `enrich_lifecycle`), so disable it too.
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(state);
    (dispatcher, state_root)
}

/// A small cross-file import + call fixture so the index produces a real graph (mirrors
/// `daemon_visibility.rs::write_s1`).
fn write_fixture(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("helper.ts"),
        "export function helperFunction() {\n    console.log('helper');\n}\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("main.ts"),
        "import { helperFunction } from './helper';\n\nexport function mainEntry() {\n    helperFunction();\n}\n",
    )
    .unwrap();
}

fn add_second(repo_dir: &Path) {
    std::fs::write(
        repo_dir.join("second.ts"),
        "import { helperFunction } from './helper';\n\nexport function secondEntry() {\n    helperFunction();\n}\n",
    )
    .unwrap();
}

fn request(id: &str, method: &str, params: Value) -> Request {
    Request {
        id: id.to_string(),
        method: method.to_string(),
        params,
    }
}

fn run(dispatcher: &ServiceDispatcher, id: &str, method: &str, params: Value) -> DispatchResult {
    let mut emitter = Quiet;
    dispatcher.dispatch(&request(id, method, params), &mut emitter)
}

#[track_caller]
fn expect_success(result: DispatchResult) -> Value {
    match result {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => panic!(
            "expected success, got error {}: {}",
            e.error.code, e.error.message
        ),
    }
}

/// The status string of the snapshot with `snapshot_uid` in `db_path` (opened read-only).
fn snapshot_status(db_path: &str, repo_uid: &str, snapshot_uid: &str) -> String {
    let conn = StorageConnection::open(db_path).expect("open db to read snapshot status");
    conn.list_snapshots(repo_uid)
        .expect("list snapshots")
        .into_iter()
        .find(|s| s.snapshot_uid == snapshot_uid)
        .unwrap_or_else(|| panic!("snapshot {snapshot_uid} not found"))
        .status
}

// ── Item 1 — a client disconnect during INDEX never costs the work ───────────

/// F5 regression proof: an emitter that starts failing mid-index (client disconnect) → the index
/// COMPLETES: snapshot READY, `record_index` persisted in the registry, and the handler stops
/// emitting after the first failure (detached exactly once).
#[test]
fn disconnect_during_index_completes_to_ready() {
    let (dispatcher, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);

    // review-0 change #3: capture the detached-continuation LOG LINE directly (below), not just the
    // emit-call count. Enable BEFORE dispatch so this index's line is recorded; parallel-safe because
    // we filter the shared recorder by this repo's unique uid.
    repo_graph_daemon_runtime::detached::enable_detached_capture_for_test();

    // An index emits many progress events (scanning / initializing / extracting / resolving /
    // persisting×N). FailAfter(3) fails on the 4th emit — well before completion — so the pre-fix
    // `Break` would have aborted here. After the fix the index runs to completion detached.
    let mut emitter = FailAfter::new(3);
    let result = dispatcher.dispatch(
        &request(
            "idx",
            "index",
            json!({ "repo_path": repo_dir.to_string_lossy() }),
        ),
        &mut emitter,
    );

    let resp = expect_success(result);
    let db_path = resp["db_path"].as_str().unwrap().to_string();
    let repo_uid = resp["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = resp["snapshot_uid"].as_str().unwrap().to_string();

    // The work completed to READY despite the disconnected client.
    assert_eq!(
        snapshot_status(&db_path, &repo_uid, &snapshot_uid),
        "ready",
        "a disconnected client must NOT abort the index — it completes to READY"
    );

    // `record_index` was persisted (the success-branch save ran): a freshly reloaded registry carries
    // the last snapshot uid, so the repo is queryable after a restart (the F5 "repo not indexed" fix).
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    let entry = reloaded
        .resolve(&repo_dir)
        .expect("repo registered + queryable by path");
    assert_eq!(
        entry.last_snapshot_uid.as_deref(),
        Some(snapshot_uid.as_str()),
        "record_index persisted the completed snapshot"
    );

    // Detached exactly once: after the first emit failure the handler skips all subsequent emits
    // (the same `client_gone` transition gates the single log line), so `calls` stops at
    // `fail_after + 1` even though the index emits many more events.
    assert_eq!(
        emitter.calls,
        emitter.fail_after + 1,
        "the handler stops emitting after the first disconnect (detached once), not once-per-event"
    );

    // review-0 change #3: DIRECTLY observe the detached-continuation LOG LINE (emit count alone is
    // not the required log proof). The capture seam recorded the exact reader-frame line the handler
    // logged; filtering by this index's unique repo_uid isolates it from any parallel test.
    let lines: Vec<String> = repo_graph_daemon_runtime::detached::detached_continuations_for_test()
        .into_iter()
        .filter(|l| l.contains(&repo_uid))
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "exactly one detached-continuation line was logged for this index: {lines:?}"
    );
    assert_eq!(
        lines[0],
        format!("client disconnected; index continues detached (repo {repo_uid})"),
        "the logged line is the ratified reader-frame text"
    );
}

// ── Item 1 — a client disconnect during REFRESH never costs the work ─────────

/// Refresh shares the write-op emitter pattern, so it gets the same best-effort treatment: a failing
/// emitter mid-refresh does not abort it — the refresh completes to a READY snapshot.
#[test]
fn disconnect_during_refresh_completes_to_ready() {
    let (dispatcher, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);

    // Index once (quiet) so the repo is registered + loaded; refresh resolves it by path.
    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    add_second(&repo_dir); // give the refresh real work.

    // review-0 change #3: capture the refresh's detached-continuation line directly (below).
    repo_graph_daemon_runtime::detached::enable_detached_capture_for_test();

    // Refresh with an emitter that fails almost immediately (client disconnect).
    let mut emitter = FailAfter::new(1);
    let result = dispatcher.dispatch(
        &request(
            "rf",
            "refresh",
            json!({ "repo": repo_dir.to_string_lossy() }),
        ),
        &mut emitter,
    );

    let resp = expect_success(result);
    let snapshot_uid = resp["snapshot_uid"].as_str().unwrap().to_string();
    assert_eq!(
        snapshot_status(&db_path, &repo_uid, &snapshot_uid),
        "ready",
        "a disconnected client must NOT abort a refresh — it completes to READY"
    );
    assert_eq!(
        emitter.calls,
        emitter.fail_after + 1,
        "the refresh handler also stops emitting after the first disconnect (detached once)"
    );

    // review-0 change #3: directly observe the refresh detached-continuation line (op label
    // "refresh"), filtered by this repo's unique uid.
    let lines: Vec<String> = repo_graph_daemon_runtime::detached::detached_continuations_for_test()
        .into_iter()
        .filter(|l| l.contains(&repo_uid))
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "exactly one detached-continuation line was logged for this refresh: {lines:?}"
    );
    assert_eq!(
        lines[0],
        format!("client disconnected; refresh continues detached (repo {repo_uid})"),
        "the logged line names the refresh op in the ratified reader-frame text"
    );
}

// ── Item 2 — registration persists up-front, surviving an index failure ──────

/// An index that fails AFTER registration leaves the repo persisted in the registry — queryable by
/// path AND by its stable uid — with no `last_snapshot_uid` (record_index runs only on success). The
/// failure is injected by making the daemon's `databases/` dir read-only so DB creation fails DURING
/// indexing, after `handle_index` has registered + saved the repo (registry.json lives in the state
/// root, still writable). Unix-only: the injection uses POSIX directory permissions.
#[cfg(unix)]
#[test]
fn failed_index_after_registration_leaves_repo_registered() {
    use std::os::unix::fs::PermissionsExt;

    let (dispatcher, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);

    let db_dir = root.path().join("databases");
    let set_mode = |dir: &Path, mode: u32| {
        let mut perms = std::fs::metadata(dir).unwrap().permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(dir, perms).unwrap();
    };
    set_mode(&db_dir, 0o555); // read-only: DB file creation will fail, after registration.

    let result = run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    );

    // Restore write perms first so the TempDir cleans up regardless of the assertions below.
    set_mode(&db_dir, 0o755);

    assert!(
        matches!(result, DispatchResult::Error(_)),
        "the index must fail when its DB cannot be created"
    );

    // The registration was persisted UP-FRONT: a freshly RELOADED registry still resolves the repo by
    // path, carries its stable uid (queryable by uid), and has NO last_snapshot_uid — so the entry
    // came from the up-front save, not the (never-reached) success-branch save.
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    let entry = reloaded
        .resolve(&repo_dir)
        .expect("repo persisted + queryable by path after a failed index");
    assert!(
        entry.repo_uid.starts_with("repo_"),
        "the entry carries its stable uid (queryable by uid): {}",
        entry.repo_uid
    );
    assert!(
        reloaded.list().iter().any(|e| e.repo_uid == entry.repo_uid),
        "the stable uid is present in the persisted registry (queryable by uid)"
    );
    assert!(
        entry.last_snapshot_uid.is_none(),
        "no successful index was recorded — the entry came from the up-front save"
    );
}

// ── Item 3 (no `building` limbo) — a post-snapshot failure is terminal + honestly surfaced ───

/// review-0 change #2: a named FAILURE proof where a snapshot EXISTS (the failure is AFTER snapshot
/// creation), proving the three DAEMON-VISIBILITY-1 views AGREE:
///   (a) the repo is still queryable in the PERSISTED registry — by path AND by uid,
///   (b) the snapshot is in a TERMINAL non-`building` state (`failed`), never a stuck `building` limbo,
///   (c) the reader/status surface (`repo info`) NAMES it as interrupted, with the outcome reason.
///
/// How the terminal state is produced (honesty note): the daemon dispatch path has NO deterministic
/// post-snapshot failure injection — a write-blocked DB fails AT `create_snapshot`, before any
/// snapshot row exists (that is `failed_index_after_registration_leaves_repo_registered`, which proves
/// the registration-survival half on a REAL driven failure). The REAL `Building`→`Failed` transition
/// after an in-pipeline failure is proven deterministically at the orchestrator
/// (`repo_graph_indexer::orchestrator::tests::explicit_cancel_during_pipeline_leaves_failed_snapshot`).
/// So here we index for real (registering up-front + creating a REAL snapshot on a REAL DB), then put
/// that real snapshot into the exact terminal state `index_repo`'s error arm writes (`Failed`, no
/// `completed_at`) — the same index→flip technique the accepted `daemon_visibility.rs` F2 proofs use —
/// and assert the registry + reader surface behave correctly over that real on-disk state.
#[test]
fn failed_index_leaves_terminal_snapshot_and_reader_surface_agree() {
    let (dispatcher, root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("repo");
    write_fixture(&repo_dir);

    let indexed = expect_success(run(
        &dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ));
    let canonical = indexed["canonical_path"].as_str().unwrap().to_string();
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let snapshot_uid = indexed["snapshot_uid"].as_str().unwrap().to_string();

    // Put the REAL extracted snapshot into the terminal state a post-creation failure leaves
    // (`index_repo`'s `Err(pipeline_err)` arm: `SnapshotStatus::Failed`, `completed_at: None`).
    {
        let conn = StorageConnection::open(&db_path).expect("open db to fail the snapshot");
        conn.update_snapshot_status(&UpdateSnapshotStatusInput {
            snapshot_uid: snapshot_uid.clone(),
            status: "failed".to_string(),
            completed_at: None,
        })
        .expect("flip the snapshot to the terminal Failed state");
    }

    // (a) Still queryable in the PERSISTED (reloaded-from-disk) registry — by path AND by uid.
    let reloaded = RepoRegistry::with_state_root(root.path()).unwrap();
    let entry = reloaded
        .resolve(&repo_dir)
        .expect("repo still queryable by path after a failed index");
    assert_eq!(
        entry.repo_uid, repo_uid,
        "same stable uid, queryable by path"
    );
    assert!(
        reloaded.list().iter().any(|e| e.repo_uid == repo_uid),
        "repo queryable by uid in the persisted registry"
    );

    // (b) Terminal non-`building` snapshot on disk (never a stuck `building` limbo).
    assert_eq!(
        snapshot_status(&db_path, &repo_uid, &snapshot_uid),
        "failed",
        "the snapshot is terminal (failed), not left in `building`"
    );

    // (c) The reader/status surface (`repo info`) exposes the interruption reason — the same
    // classification DAEMON-VISIBILITY-1's `snapshot_facts` renders for `doctor` (the two views agree).
    let info = expect_success(run(
        &dispatcher,
        "ri",
        "repo_info",
        json!({ "repo": canonical }),
    ));
    let interrupted = info["storage"]["interrupted_snapshots"]
        .as_array()
        .expect("repo info surfaces interrupted_snapshots");
    assert_eq!(
        interrupted.len(),
        1,
        "the failed snapshot is surfaced as interrupted: {info}"
    );
    assert_eq!(
        interrupted[0]["state"], "interrupted",
        "reader-frame state names the interruption"
    );
    assert!(
        interrupted[0]["outcome"]
            .as_str()
            .unwrap_or_default()
            .contains("interrupted"),
        "reader-frame outcome names the interruption reason: {}",
        interrupted[0]["outcome"]
    );
}
