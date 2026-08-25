//! DAEMON-CRASH-RECOVERY-1 named proofs, driven through the REAL `ServiceDispatcher::dispatch` +
//! storage path (mirrors `tests/daemon_visibility.rs`'s harness).
//!
//! - **F7/F11 (reconciliation)** — `boot_reconciliation_marks_logs_and_reclaims_a_crash_orphan`: a
//!   crash-orphaned `building` snapshot (a re-index the daemon began and never finalized) is, at the
//!   next boot sweep, marked interrupted (terminal `failed`), LOGGED with the "daemon restart"
//!   reason, NAMED by the real `classify_retention` (prune) report, and RECLAIMED — while the READY
//!   snapshot survives.
//! - **F8 (log lifecycle)** — `index_logs_start_and_outcome_then_reconcile_repairs_the_missing_outcome`:
//!   a real index writes `op index started` + `op index completed` to the log sink; a crashed
//!   re-index leaves a START with no outcome, and the next boot's reconciliation line supplies it.
//!
//! F10 (no bare "index the repo first" on a READY-requiring surface) is proven by the deterministic
//! source audit inlined in `docs/slices/daemon-crash-recovery-1.build-1.md` and the existing
//! behavioral F2 proofs in `tests/daemon_visibility.rs`; F12's client render is unit-tested in
//! `rgr::commands::maintenance`.

use std::path::Path;
use std::sync::Arc;

use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_storage::types::CreateSnapshotInput;
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

// ── Harness (minimal subset of tests/daemon_visibility.rs) ───────────────────

struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

fn isolated() -> (Arc<ServiceDispatcher>, Arc<DaemonState>, TempDir) {
    // The background auto-passes would stamp their own activity/write-lock; this binary drives
    // reconciliation directly, so disable them for deterministic assertions (same as daemon_visibility).
    repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test(false);
    repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = Arc::new(DaemonState::with_registry(registry));
    let dispatcher = Arc::new(ServiceDispatcher::new(Arc::clone(&state)));
    (dispatcher, state, state_root)
}

fn write_s1(repo_dir: &Path) {
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
        DispatchResult::Error(e) => {
            panic!(
                "expected success, got error {}: {}",
                e.error.code, e.error.message
            )
        }
    }
}

fn index_s1(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> Value {
    expect_success(run(
        dispatcher,
        "idx",
        "index",
        json!({ "repo_path": repo_dir.to_string_lossy() }),
    ))
}

/// Seed a crash-orphaned `building` snapshot directly into a repo's DB — a re-index the daemon began
/// and never finalized before it died. Returns the orphan's snapshot_uid.
fn seed_building_orphan(db_path: &str, repo_uid: &str) -> String {
    let storage = StorageConnection::open(db_path).expect("open repo db");
    storage
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: repo_uid.to_string(),
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            parent_snapshot_uid: None,
            label: None,
            toolchain_json: None,
        })
        .expect("seed building orphan")
        .snapshot_uid
}

// ── F7/F11 — reconciliation proof ────────────────────────────────────────────

#[test]
fn boot_reconciliation_marks_logs_and_reclaims_a_crash_orphan() {
    repo_graph_daemon_runtime::oplog::enable_oplog_capture_for_test();
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    let repo_dir = repo_root.path().join("recon_repo");
    write_s1(&repo_dir);

    // A healthy index → a READY snapshot + a registered repo.
    let indexed = index_s1(&dispatcher, &repo_dir);
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();
    let canonical = indexed["canonical_path"].clone();

    // Simulate the crash: a re-index that began (building) and never finalized.
    let orphan = seed_building_orphan(&db_path, &repo_uid);

    // The boot sweep (run_daemon spawns this on a thread; drive it directly). The activity registry is
    // empty (index done), so the orphan is detected without any surviving-daemon evidence.
    repo_graph_daemon_runtime::reconcile::reconcile_all_repos(&state);

    // MARKED: the orphan is now the terminal `failed` state (reader-frame "interrupted"); the READY
    // snapshot is untouched.
    let storage = StorageConnection::open(&db_path).unwrap();
    assert_eq!(
        storage.get_snapshot(&orphan).unwrap().unwrap().status,
        "failed",
        "orphan marked interrupted (terminal failed)"
    );
    assert!(
        storage.get_latest_snapshot(&repo_uid).unwrap().is_some(),
        "the READY snapshot survives reconciliation"
    );

    // CLASSIFIED prunable (review-1, the blocking acceptance criterion): the retention STAT counts the
    // orphan BEFORE any reclaim, so "retention classifies them prunable" is a durable current-state
    // fact doctor / prune read — not a side effect of the reclaim. Only the orphan is prunable (the
    // surviving READY snapshot is `current`), so the count is exactly 1.
    assert_eq!(
        storage.get_retention_stats(&repo_uid).unwrap().prunable,
        1,
        "the reconciled orphan is classified + counted prunable before reclaim"
    );

    // LOGGED: a reconciliation line names the snapshot + the "daemon restart" reason.
    let logged = repo_graph_daemon_runtime::oplog::oplog_lines_for_test();
    assert!(
        logged
            .iter()
            .any(|l| l.contains("interrupted (daemon restart)") && l.contains(&orphan)),
        "reconciliation line logged with reason: {logged:?}"
    );

    // RENDERED (operator resolution: Option B) — the DURABLE reason survives on `rmap repo info`
    // through the real dispatch path, so the truth is legible even after the daemon log rotates: the
    // reconciled orphan's per-snapshot outcome names "daemon restart, reconciled <time>".
    let info = expect_success(run(
        &dispatcher,
        "info",
        "repo_info",
        json!({ "repo": canonical }),
    ));
    let orphan_outcome = info["storage"]["snapshots"]
        .as_array()
        .and_then(|snaps| {
            snaps
                .iter()
                .find(|s| s["snapshot_uid"] == json!(orphan))
                .and_then(|s| s["outcome"].as_str())
        })
        .unwrap_or("");
    assert!(
        orphan_outcome.contains("interrupted — daemon restart")
            && orphan_outcome.contains("reconciled "),
        "repo info renders the durable reconciliation reason (Option B): outcome={orphan_outcome:?}, full={info}"
    );

    // NAMED + RECLAIMED through the real `maintenance prune` (classify_retention) handler: the orphan
    // is listed (pre-reclaim) then deleted; the READY snapshot survives.
    let prune = expect_success(run(
        &dispatcher,
        "prune",
        "classify_retention",
        json!({ "path": canonical }),
    ));
    assert_eq!(
        prune["interrupted_snapshots"].as_array().map(|a| a.len()),
        Some(1),
        "the orphan is NAMED in the prune report (stats never imply an empty store): {prune}"
    );
    assert!(
        prune["non_ready_reclaim"]["reclaimed"]
            .as_bool()
            .unwrap_or(false),
        "prune RECLAIMS the orphan: {prune}"
    );
    let after = StorageConnection::open(&db_path).unwrap();
    assert!(
        after.get_snapshot(&orphan).unwrap().is_none(),
        "the orphan row is gone after reclaim"
    );
    assert!(
        after.get_latest_snapshot(&repo_uid).unwrap().is_some(),
        "the READY snapshot is still served after prune"
    );
}

// ── F8 — log-lifecycle proof ─────────────────────────────────────────────────

#[test]
fn index_logs_start_and_outcome_then_reconcile_repairs_the_missing_outcome() {
    repo_graph_daemon_runtime::oplog::enable_oplog_capture_for_test();
    let (dispatcher, state, _root) = isolated();
    let repo_root = tempdir().unwrap();
    // A distinct dir → a distinct repo_uid → the shared (non-draining) capture buffer is filtered to
    // THIS test, so it is parallel-safe against the reconciliation test above.
    let repo_dir = repo_root.path().join("oplog_repo");
    write_s1(&repo_dir);

    let indexed = index_s1(&dispatcher, &repo_dir);
    let db_path = indexed["db_path"].as_str().unwrap().to_string();
    let repo_uid = indexed["repo_uid"].as_str().unwrap().to_string();

    // A real index writes a START and a terminal OUTCOME line.
    let after_index: Vec<String> = repo_graph_daemon_runtime::oplog::oplog_lines_for_test()
        .into_iter()
        .filter(|l| l.contains(&repo_uid))
        .collect();
    assert!(
        after_index.iter().any(|l| l.contains("op index started")),
        "index logs a START line: {after_index:?}"
    );
    assert!(
        after_index.iter().any(|l| l.contains("op index completed")),
        "index logs an OUTCOME line: {after_index:?}"
    );

    // A crashed re-index leaves a `building` orphan whose START has NO matching outcome (the daemon
    // died). The next boot's reconciliation line SUPPLIES the missing outcome — the log tells the
    // whole story without doctor ever being reachable.
    let orphan = seed_building_orphan(&db_path, &repo_uid);
    repo_graph_daemon_runtime::reconcile::reconcile_all_repos(&state);

    let after_reconcile: Vec<String> = repo_graph_daemon_runtime::oplog::oplog_lines_for_test()
        .into_iter()
        .filter(|l| l.contains(&repo_uid))
        .collect();
    assert!(
        after_reconcile
            .iter()
            .any(|l| l.contains("interrupted (daemon restart)") && l.contains(&orphan)),
        "the crashed op's missing outcome is repaired by the reconciliation line: {after_reconcile:?}"
    );
}
