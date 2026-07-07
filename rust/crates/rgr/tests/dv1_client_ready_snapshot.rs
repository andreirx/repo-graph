//! DAEMON-VISIBILITY-1 (F2) named proofs for the DIRECT-STORAGE client commands.
//!
//! review-5 required change: the direct-storage client paths that resolve a READY snapshot
//! (`rmap metrics`, `rmap modules boundary`) used to print a bare
//! `"no snapshot found for repo '<uid>'"` when only a non-READY (interrupted) snapshot existed — the
//! exact gaslighting F2 forbids, on the actual user-facing commands wired in `main.rs`.
//!
//! NOTE (ENRICH-LIFECYCLE-1 §3.6, REG-1 closure): `rmap enrich` was the third command here but is NO
//! LONGER direct-storage — it is now a registry-resolved daemon client (like `orient`), so its F2
//! behavior is the daemon's, not this client path's. Its former direct-storage F2 proof was removed
//! with the transport change; whether the daemon-routed enrich preserves the same honest partial
//! message is raised for verification in the build report (DECISION_REQUIRED: enrich-f2-via-daemon).
//!
//! These proofs drive the REAL compiled `rmap` binary (`CARGO_BIN_EXE_rmap`) against a fixture repo
//! whose only snapshot is non-READY, and assert stderr NAMES the partial (state + on-disk size) and
//! BOTH next actions (`rmap index` / `rmap maintenance prune`) — never the bare message. A fourth
//! proof covers the honest fallback: a never-indexed repo gets the plain "index it first" with no
//! fabricated partial. The message wording itself is additionally unit-tested in
//! `cli::snapshot_hint::tests`.

use std::path::{Path, PathBuf};
use std::process::Command;

use repo_graph_storage::types::{CreateSnapshotInput, Repo};
use repo_graph_storage::StorageConnection;
use tempfile::TempDir;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

fn add_repo(storage: &StorageConnection, repo_uid: &str) {
    storage
        .add_repo(&Repo {
            repo_uid: repo_uid.to_string(),
            name: "test-repo".to_string(),
            root_path: format!("/tmp/{repo_uid}"),
            default_branch: None,
            created_at: "2026-07-02T10:00:00Z".to_string(),
            metadata_json: None,
        })
        .unwrap();
}

/// A repo whose ONLY snapshot is non-READY: `create_snapshot` defaults to `building`, and we never
/// mark it ready — the day-2 field case (an index that started and never finalized). `get_latest_snapshot`
/// (READY-only) therefore returns `Ok(None)`, taking the command into the F2 path.
fn repo_with_only_non_ready_snapshot() -> (TempDir, PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let storage = StorageConnection::open(&db_path).unwrap();
    add_repo(&storage, "r1");
    let snap = storage
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: "r1".to_string(),
            parent_snapshot_uid: None,
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            label: None,
            toolchain_json: None,
        })
        .unwrap();
    assert_eq!(
        snap.status, "building",
        "fixture precondition: the only snapshot is a non-READY partial"
    );
    drop(storage);
    (dir, db_path, "r1".to_string())
}

/// A repo that exists but was NEVER indexed (no snapshot at all) — the honest-fallback case.
fn repo_never_indexed() -> (TempDir, PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let storage = StorageConnection::open(&db_path).unwrap();
    add_repo(&storage, "r1");
    drop(storage);
    (dir, db_path, "r1".to_string())
}

/// The F2 contract at a command surface: NAMES the partial (state + on-disk size) + BOTH next
/// actions, and is NOT the bare gaslighting message.
#[track_caller]
fn assert_names_partial(stderr: &str) {
    assert!(
        !stderr.contains("no snapshot found"),
        "must NOT be the bare gaslighting message: {stderr}"
    );
    assert!(
        stderr.contains("interrupted"),
        "names the partial's state: {stderr}"
    );
    assert!(
        stderr.contains("on disk"),
        "names the on-disk size held: {stderr}"
    );
    assert!(
        stderr.contains("rmap index"),
        "next action 1 (re-index): {stderr}"
    );
    assert!(
        stderr.contains("rmap maintenance prune"),
        "next action 2 (reclaim): {stderr}"
    );
}

fn run_rmap(args: &[&str]) -> std::process::Output {
    Command::new(binary_path())
        .args(args)
        .output()
        .expect("spawn rmap")
}

fn db(path: &Path) -> &str {
    path.to_str().unwrap()
}

// ── enrich: removed — no longer a direct-storage command (ENRICH-LIFECYCLE-1 §3.6, REG-1). It is now
//    a registry-resolved daemon client, so `rmap enrich <db> <uid>` no longer opens storage to reach
//    the F2 path; driving it here would send a request to the operator's real daemon. See module doc.

// ── metrics ───────────────────────────────────────────────────────────────────

#[test]
fn metrics_direct_storage_on_only_non_ready_snapshot_names_the_partial() {
    let (_dir, db_path, repo_uid) = repo_with_only_non_ready_snapshot();
    let out = run_rmap(&["metrics", db(&db_path), &repo_uid]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "runtime-error exit preserved: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_names_partial(&String::from_utf8_lossy(&out.stderr));
}

// ── modules boundary ────────────────────────────────────────────────────────────

#[test]
fn modules_boundary_direct_storage_on_only_non_ready_snapshot_names_the_partial() {
    let (_dir, db_path, repo_uid) = repo_with_only_non_ready_snapshot();
    // The snapshot check fires BEFORE module resolution, so placeholder source/target suffice.
    let out = run_rmap(&[
        "modules",
        "boundary",
        db(&db_path),
        &repo_uid,
        "packages/app",
        "--forbids",
        "packages/core",
    ]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "runtime-error exit preserved: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_names_partial(&String::from_utf8_lossy(&out.stderr));
}

// ── honest fallback: never-indexed repo is NOT gaslit as a partial ──────────────
// Uses `metrics` (a still-direct-storage command sharing the F2 `snapshot_hint` path); the original
// used `enrich`, which is now daemon-routed (ENRICH-LIFECYCLE-1 §3.6) and would send a request to the
// operator's real daemon. `metrics` exercises the identical never-indexed honest-fallback behavior.

#[test]
fn direct_storage_never_indexed_repo_gets_plain_index_first_not_a_fake_partial() {
    let (_dir, db_path, repo_uid) = repo_never_indexed();
    let out = run_rmap(&["metrics", db(&db_path), &repo_uid]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no snapshot for repo 'r1'"),
        "plain never-indexed message: {stderr}"
    );
    assert!(stderr.contains("rmap index"), "next action: {stderr}");
    // No partial exists, so we must NOT fabricate one.
    assert!(
        !stderr.contains("interrupted"),
        "must not claim a partial that does not exist: {stderr}"
    );
    assert!(
        !stderr.contains("rmap maintenance prune"),
        "nothing to reclaim when never indexed: {stderr}"
    );
}
