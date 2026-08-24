//! INDEX-BASIS-1 — daemon WRITE-path stamping proofs, driven through the REAL
//! `ServiceDispatcher::dispatch` `index`/`refresh` surfaces against real repositories.
//!
//! These close review-1 finding 3: the prior revision added the production
//! `basis_commit` assignments in `dispatch.rs` but no assertion that a real daemon
//! `index` actually PERSISTS the stamp. Here we index/refresh three repo shapes and read
//! `snapshots.basis_commit` (and the additive `index_basis` diagnostic) straight back:
//!
//!   1. a git repo WITH a commit → `basis_commit` == `git rev-parse HEAD` (the stamp).
//!   2. a NON-git directory       → `basis_commit` is NULL (the reserved "no basis") AND a
//!      persisted `index_basis` = `non_git` record (RULING 3: a THIS-slice non-git NULL is
//!      distinct from a pre-slice NULL, which has NO record); `orient` renders "not a git repo".
//!   3. a git repo with ZERO commits (unborn HEAD) → `basis_commit` is NULL AND the
//!      snapshot's `extraction_diagnostics_json` carries the additive `index_basis` =
//!      `failure` record whose reason is the write-time-classified "repository has no
//!      commits yet" (RULING 3), rendered END-TO-END — never mis-attributed to pre-slice
//!      history (review-4). The unborn state is established by the POSITIVE `git rev-list
//!      -n 1 --all` probe (review-9 #1), not by matching git's stderr.
//!
//! `refresh_restamps_basis_to_the_refresh_start_head` closes review-2 finding 3: it drives
//! the REAL `refresh` dispatch arm after advancing HEAD past the index basis and asserts
//! the freshly composed snapshot records the refresh-start HEAD (refresh RE-ANCHORS).
//!
//! The HEAD-failure provenance proofs (generic non-unborn failure, the broken-HEAD-with-
//! commits regression, pre-slice NULL) live in the sibling `index_basis_failures.rs` — split
//! to keep both files under the 500-line structural guardrail (review-9 #4).

mod common;
use common::*;

use serde_json::Value;
use tempfile::tempdir;

#[test]
fn index_of_a_git_repo_persists_head_as_basis() {
    let (dispatcher, _root) = isolated();
    let repo = tempdir().unwrap();
    init_git(repo.path());
    write_source(repo.path());
    run_git(repo.path(), &["add", "-A"]);
    run_git(repo.path(), &["commit", "-m", "c1"]);
    let expected = head_sha(repo.path());

    let payload = index(&dispatcher, repo.path());
    assert_eq!(
        persisted_basis(&payload),
        Some(expected),
        "index stamps snapshots.basis_commit = git rev-parse HEAD"
    );
}

#[test]
fn index_of_a_non_git_dir_persists_null_basis_and_non_git_diagnostic() {
    let (dispatcher, _root) = isolated();
    let repo = tempdir().unwrap();
    write_source(repo.path()); // no `git init` → not a repo

    let payload = index(&dispatcher, repo.path());
    assert_eq!(
        persisted_basis(&payload),
        None,
        "a non-git index records NULL basis (the reserved 'no basis' state)"
    );

    // RULING 3: a THIS-slice non-git index records a `non_git` outcome in the snapshot's
    // extraction diagnostics, so the query path can tell it apart from a PRE-slice NULL
    // (no record → "indexed before basis tracking"). Written by compose, no schema change.
    let db_path = payload["db_path"].as_str().unwrap();
    let snapshot_uid = payload["snapshot_uid"].as_str().unwrap();
    let blob = diagnostics_blob(db_path, snapshot_uid)
        .expect("a non-git index records an extraction-diagnostics blob");
    let diag: Value = serde_json::from_str(&blob).unwrap();
    assert_eq!(
        diag["index_basis"]["outcome"].as_str(),
        Some("non_git"),
        "the write path recorded the non_git outcome: {blob}"
    );

    // And the query surface classifies it NotGit (the daemon JSON carries the serialized
    // `IndexDrift` state tag; the human "not a git repo" prose is rendered CLI-side), never
    // the unborn/pre-slice states.
    let ori = serde_json::to_string(&orient(&dispatcher, repo.path())).unwrap();
    assert!(
        ori.contains("\"index_drift\":{\"state\":\"not_git\"}"),
        "orient classifies NotGit: {ori}"
    );
    assert!(
        !ori.contains("no commits yet") && !ori.contains("basis_unknown"),
        "non-git must not borrow the unborn/pre-slice states: {ori}"
    );
}

#[test]
fn refresh_restamps_basis_to_the_refresh_start_head() {
    // review-2 finding 3: the slice requires unit proof that BOTH index and refresh stamp
    // the basis. Index at HEAD1, advance HEAD to HEAD2, then drive the REAL `refresh`
    // dispatch arm and assert the freshly composed snapshot records HEAD2 (the
    // refresh-start HEAD) — refresh RE-ANCHORS, it does not keep the stale index basis.
    let (dispatcher, _root) = isolated();
    let repo = tempdir().unwrap();
    init_git(repo.path());
    write_source(repo.path());
    run_git(repo.path(), &["add", "-A"]);
    run_git(repo.path(), &["commit", "-m", "c1"]);
    let head1 = head_sha(repo.path());

    let idx = index(&dispatcher, repo.path());
    assert_eq!(
        persisted_basis(&idx),
        Some(head1.clone()),
        "index anchors to HEAD1"
    );

    // Advance HEAD past the index basis.
    std::fs::write(repo.path().join("extra.ts"), "export const extra = 2;\n").unwrap();
    run_git(repo.path(), &["add", "-A"]);
    run_git(repo.path(), &["commit", "-m", "c2"]);
    let head2 = head_sha(repo.path());
    assert_ne!(head1, head2, "HEAD advanced to a new commit");

    let refreshed = refresh(&dispatcher, repo.path());
    let refreshed_snapshot = refreshed["snapshot_uid"].as_str().unwrap();
    assert_ne!(
        refreshed_snapshot,
        idx["snapshot_uid"].as_str().unwrap(),
        "refresh composed a new snapshot"
    );
    let basis = basis_of(
        idx["db_path"].as_str().unwrap(),
        idx["repo_uid"].as_str().unwrap(),
        refreshed_snapshot,
    );
    assert_eq!(
        basis,
        Some(head2),
        "refresh stamps snapshots.basis_commit = git rev-parse HEAD at refresh start (re-anchors)"
    );
}

#[test]
fn index_of_an_unborn_git_repo_records_diagnostic_and_renders_no_commits() {
    // A git repo with ZERO commits (unborn HEAD): `git rev-parse HEAD` fails, so the stamp
    // is the failed-HEAD case. RULING 2: the index PROCEEDS (never hostage to a git edge
    // case), persists NULL basis (schema frozen — never a fake sha), AND records WHY in the
    // snapshot's extraction_diagnostics_json (additive `index_basis` = unborn). The query
    // path then renders "repository has no commits yet" from that recorded fact — proven
    // END-TO-END here — never the mis-attributed "indexed before basis tracking" (review-4).
    // review-9 #1: the unborn state is established by the POSITIVE `git rev-list -n 1 --all`
    // probe (empty commit graph), NOT by matching git's stderr.
    let (dispatcher, _root) = isolated();
    let repo = tempdir().unwrap();
    init_git(repo.path()); // init, but never commit
    write_source(repo.path()); // untracked working-tree files → still indexable

    let payload = index(&dispatcher, repo.path());
    assert_eq!(
        persisted_basis(&payload),
        None,
        "an unreadable HEAD records NULL basis, never a fabricated one"
    );

    // WRITE proof: RULING 3 — the `index_basis` outcome is a THREE-variant record; an
    // unborn HEAD is a `failure` whose reason is the already-rendered "no commits yet"
    // (classified from the positive probe at write time). Persisted by compose, not swallowed.
    let db_path = payload["db_path"].as_str().unwrap();
    let snapshot_uid = payload["snapshot_uid"].as_str().unwrap();
    let blob = diagnostics_blob(db_path, snapshot_uid)
        .expect("the unborn index records an extraction-diagnostics blob");
    let diag: Value = serde_json::from_str(&blob).unwrap();
    assert_eq!(
        diag["index_basis"]["outcome"].as_str(),
        Some("failure"),
        "the write path recorded a failure outcome for the unreadable HEAD: {blob}"
    );
    assert_eq!(
        diag["index_basis"]["reason"].as_str(),
        Some("repository has no commits yet"),
        "the unborn reason is classified at write time: {blob}"
    );

    // END-TO-END render proof: orient re-reads the record and renders the true state.
    let ori = serde_json::to_string(&orient(&dispatcher, repo.path())).unwrap();
    assert!(
        ori.contains("repository has no commits yet"),
        "orient renders the recorded unborn state: {ori}"
    );
    assert!(
        !ori.contains("indexed before basis tracking"),
        "unborn-then-queried must NOT be mis-attributed to pre-slice history: {ori}"
    );
}
