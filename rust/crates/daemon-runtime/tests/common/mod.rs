//! Shared INDEX-BASIS-1 e2e harness for the daemon WRITE-path / drift-provenance tests.
//!
//! Extracted so `tests/index_basis.rs` (the WRITE-path stamping proofs) and
//! `tests/index_basis_failures.rs` (the HEAD-failure provenance proofs) can share one
//! real-dispatcher + real-git harness WITHOUT either file exceeding the 500-line
//! structural guardrail, and WITHOUT duplicating ~180 lines of harness across both.
//!
//! ABSTRACTION (test-support module, NOT a production abstraction — never compiled into
//! a shipped artifact):
//!   - what: an isolated `ServiceDispatcher` under a throwaway state root + real-git repo
//!     builders + storage read-back helpers.
//!   - concrete users: `tests/index_basis.rs` + `tests/index_basis_failures.rs`.
//!   - axis: two cohesive integration-test binaries sharing one harness (the split forced
//!     by the 500-line guardrail, operator RULING 3 / review-9 #4).
//!   - rejected simpler alternative: one 500+-line file (breaches the guardrail), or
//!     duplicating the harness in both files (a maintenance hazard).
//!
//! `#![allow(dead_code)]`: `tests/common/mod.rs` is compiled INTO EACH integration-test
//! binary, and each binary uses only the subset of helpers its tests need — the standard
//! Rust `tests/common` pattern. Unused-in-one-binary is expected, not a defect.
#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use repo_graph_daemon_runtime::enrich_pass::set_auto_enrich_for_test;
use repo_graph_daemon_runtime::retention_pass::set_auto_retention_for_test;
use repo_graph_daemon_runtime::{DaemonState, RepoRegistry, ServiceDispatcher};
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, EmitError, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_indexer::SnapshotLifecyclePort;
use repo_graph_repo_index::compose::INDEX_BASIS_DIAG_KEY;
use repo_graph_storage::StorageConnection;
use repo_graph_trust::TrustStorageRead;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};

/// A progress emitter that discards events.
pub struct Quiet;
impl ProgressEmitter for Quiet {
    fn emit(&mut self, _detail: ProgressDetail) -> Result<(), EmitError> {
        Ok(())
    }
}

/// Isolated dispatcher under a throwaway state root — never touches the operator's
/// real registry. Maintenance passes forced OFF so no detached thread races teardown.
pub fn isolated() -> (ServiceDispatcher, TempDir) {
    set_auto_retention_for_test(false);
    set_auto_enrich_for_test(false);
    repo_graph_daemon_runtime::seed::set_auto_seed_for_test(false);
    let state_root = tempdir().expect("state root tempdir");
    let registry = RepoRegistry::with_state_root(state_root.path())
        .expect("isolated registry under temp root");
    let state = std::sync::Arc::new(DaemonState::with_registry(registry));
    let dispatcher = ServiceDispatcher::new(state);
    (dispatcher, state_root)
}

pub fn run_git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A real cross-file TS pair so a full index produces a real snapshot.
pub fn write_source(dir: &Path) {
    std::fs::write(
        dir.join("helper.ts"),
        "export function helperFunction() {\n    return 1;\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.ts"),
        "import { helperFunction } from './helper';\n\nexport function mainEntry() {\n    helperFunction();\n}\n",
    )
    .unwrap();
}

pub fn init_git(dir: &Path) {
    run_git(dir, &["init"]);
    run_git(dir, &["config", "user.email", "test@example.com"]);
    run_git(dir, &["config", "user.name", "Test User"]);
    run_git(dir, &["checkout", "-B", "main"]);
}

pub fn head_sha(dir: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("rev-parse");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn index(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> Value {
    let request = Request {
        id: "idx".to_string(),
        method: "index".to_string(),
        params: json!({ "repo_path": repo_dir.to_string_lossy() }),
    };
    let mut emitter = Quiet;
    match dispatcher.dispatch(&request, &mut emitter) {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => panic!("index failed {}: {}", e.error.code, e.error.message),
    }
}

/// Drive the REAL `refresh` dispatch arm for an already-registered repo. Returns the
/// refresh payload — which carries `snapshot_uid` (the freshly composed snapshot) but,
/// unlike `index`, NOT `db_path`/`repo_uid`; the caller reuses the index payload's DB
/// coordinates (same repo, same DB) to read the new snapshot's basis back.
pub fn refresh(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> Value {
    // `refresh` resolves the repo via the `repo` param (alias OR path) through
    // `resolve_and_load_repo`, unlike `index` which takes `repo_path`.
    let request = Request {
        id: "rfr".to_string(),
        method: "refresh".to_string(),
        params: json!({ "repo": repo_dir.to_string_lossy() }),
    };
    let mut emitter = Quiet;
    match dispatcher.dispatch(&request, &mut emitter) {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => panic!("refresh failed {}: {}", e.error.code, e.error.message),
    }
}

/// Drive the REAL `orient` dispatch arm for an already-registered repo and return the
/// full response JSON (which carries the query-time `index_drift` render injected on
/// `value`). Used to prove the basis/drift line END-TO-END (write → query → render).
pub fn orient(dispatcher: &ServiceDispatcher, repo_dir: &Path) -> Value {
    let request = Request {
        id: "ori".to_string(),
        method: "orient".to_string(),
        params: json!({ "repo": repo_dir.to_string_lossy() }),
    };
    let mut emitter = Quiet;
    match dispatcher.dispatch(&request, &mut emitter) {
        DispatchResult::Success(s) => s.result,
        DispatchResult::Error(e) => panic!("orient failed {}: {}", e.error.code, e.error.message),
    }
}

/// The raw `extraction_diagnostics_json` blob persisted for a snapshot, read straight back
/// through the trust read port (the same channel the RED-floor family uses). Proves the
/// WRITE path recorded (or did NOT record) the additive `index_basis` key.
pub fn diagnostics_blob(db_path: &str, snapshot_uid: &str) -> Option<String> {
    let conn = StorageConnection::open(db_path).unwrap();
    TrustStorageRead::get_snapshot_extraction_diagnostics(&conn, snapshot_uid).unwrap()
}

/// Write a diagnostics blob back through the SAME storage port compose writes through
/// (dev-only channel).
pub fn write_diagnostics_blob(db_path: &str, snapshot_uid: &str, blob: &str) {
    let mut conn = StorageConnection::open(db_path).unwrap();
    SnapshotLifecyclePort::update_snapshot_extraction_diagnostics(&mut conn, snapshot_uid, blob)
        .unwrap();
}

/// Read-modify-write: STRIP the `index_basis` key from the snapshot's existing blob
/// (preserving the rest), simulating a PRE-slice NULL snapshot (a NULL basis with no
/// recorded outcome).
pub fn strip_index_basis(db_path: &str, snapshot_uid: &str) {
    let existing = diagnostics_blob(db_path, snapshot_uid).expect("a diagnostics blob exists");
    let mut v: Value = serde_json::from_str(&existing).unwrap();
    v.as_object_mut().unwrap().remove(INDEX_BASIS_DIAG_KEY);
    write_diagnostics_blob(db_path, snapshot_uid, &serde_json::to_string(&v).unwrap());
}

/// The persisted `basis_commit` for a specific snapshot in a repo's DB.
pub fn basis_of(db_path: &str, repo_uid: &str, snapshot_uid: &str) -> Option<String> {
    let conn = StorageConnection::open(db_path).unwrap();
    let snap = conn
        .list_snapshots(repo_uid)
        .unwrap()
        .into_iter()
        .find(|s| s.snapshot_uid == snapshot_uid)
        .expect("the snapshot exists");
    snap.basis_commit
}

/// The persisted `basis_commit` for the snapshot an `index` payload produced.
pub fn persisted_basis(payload: &Value) -> Option<String> {
    basis_of(
        payload["db_path"].as_str().unwrap(),
        payload["repo_uid"].as_str().unwrap(),
        payload["snapshot_uid"].as_str().unwrap(),
    )
}

/// Append a poison stanza to a repo's `.git/config` declaring an UNKNOWN REQUIRED
/// extension (`repositoryformatversion = 1` + `extensions.doesnotexist`). Git then
/// refuses EVERY command on the repo with `fatal: unknown repository extension found`,
/// a deterministic, portable GENERIC failure that is NEITHER "not a git repository"
/// (so `is_git_repo` → `Err`, not `NonGit`) NOR an unborn signature (so the positive
/// unborn probe ALSO fails → generic `Failure`, not "no commits yet"). Returns the
/// PRISTINE config bytes so the caller can restore (repair git) before the query.
pub fn poison_git_config_unknown_extension(repo: &Path) -> String {
    let config_path = repo.join(".git").join("config");
    let pristine = std::fs::read_to_string(&config_path).expect("read .git/config");
    std::fs::write(
        &config_path,
        format!("{pristine}\n[core]\n\trepositoryformatversion = 1\n[extensions]\n\tdoesnotexist = true\n"),
    )
    .expect("write poisoned .git/config");
    pristine
}

/// Restore a repo's `.git/config` to its pristine bytes (repairing git).
pub fn restore_git_config(repo: &Path, pristine: &str) {
    std::fs::write(repo.join(".git").join("config"), pristine).expect("restore .git/config");
}

/// Point a repo's `HEAD` at a branch that does NOT exist (leaving the real branch and its
/// commit intact). `git rev-parse HEAD` then fails with the SAME `fatal: ambiguous
/// argument 'HEAD': unknown revision …` stderr an UNBORN repo emits — but the repo HAS
/// commits, so it is a committed-repo-with-broken-HEAD, NOT an empty one. This is the
/// review-9 #1 false-positive: text-matching that stderr would wrongly render "no commits
/// yet"; the POSITIVE `git rev-list -n 1 --all` probe still finds the commit and correctly
/// classifies it generic.
pub fn break_head_to_missing_branch(repo: &Path) {
    std::fs::write(
        repo.join(".git").join("HEAD"),
        "ref: refs/heads/does-not-exist\n",
    )
    .expect("rewrite .git/HEAD to a missing branch");
}
