//! INDEX-BASIS-1 — daemon HEAD-FAILURE provenance proofs, driven through the REAL
//! `ServiceDispatcher::dispatch` `index`/`orient` surfaces against real repositories.
//!
//! Split from `index_basis.rs` (the WRITE-path stamping proofs) so both files stay under
//! the 500-line structural guardrail (review-9 #4); the shared harness lives in
//! `tests/common/mod.rs`. These prove the two NULL-basis FAILURE outcomes are recorded and
//! rendered HONESTLY — never mis-classified as an empty repo or as pre-slice history:
//!
//!   A. a GENERIC (non-unborn) HEAD failure at index time — a REAL git repo WITH a commit
//!      whose `.git/config` declares an unknown required extension, so every git command
//!      fails with `fatal: unknown repository extension found` (NEITHER "not a git
//!      repository" NOR an unborn signature; the positive unborn probe ALSO fails). The
//!      index PROCEEDS, stamps NULL basis, records the generic `failure` via the NORMAL
//!      pre-`Ready` write path (compose→orchestrator, RULING 4), and `orient` renders the
//!      recorded reason after git is repaired (closes review-5/review-8).
//!   B. **review-9 #1** — a committed repo with a BROKEN HEAD (points at a missing branch)
//!      emits the IDENTICAL `ambiguous argument 'HEAD': unknown revision` stderr an unborn
//!      repo emits, but HAS commits. The POSITIVE `git rev-list -n 1 --all` probe finds the
//!      commit → it is classified GENERIC ("git HEAD unreadable at index time"), and MUST
//!      NEVER render "repository has no commits yet". This is the false-positive the old
//!      stderr-text classifier produced.
//!   C. a pre-slice NULL (basis NULL, NO record) → `orient` renders the unchanged
//!      BasisUnknown ("indexed before basis tracking"), never non-git / unknown-with-reason.

mod common;
use common::*;

use serde_json::Value;
use tempfile::tempdir;

#[test]
fn index_of_a_repo_with_generic_head_failure_persists_and_renders_unknown_with_reason() {
    // review-8 item 1: a GENUINE non-unborn `git rev-parse HEAD` failure at index time must
    // (a) let indexing SUCCEED, (b) persist NULL basis, (c) record the generic `failure` via
    // the NORMAL pre-`Ready` write path (compose→orchestrator, RULING 4 — NOT a synthesized
    // record), and (d) render unknown-WITH-REASON at query time. We force a portable,
    // deterministic GENERIC failure: a REAL repo WITH a commit (genuinely non-unborn) whose
    // `.git/config` declares an unknown required extension, so git refuses every command with
    // `fatal: unknown repository extension found` — neither the "not a git repository" nor an
    // unborn signature, and the positive `rev-list --all` probe ALSO fails → generic. The
    // working-tree scan is git-subprocess-independent, so indexing still produces a snapshot.
    let (dispatcher, _root) = isolated();
    let repo = tempdir().unwrap();
    init_git(repo.path());
    write_source(repo.path());
    run_git(repo.path(), &["add", "-A"]);
    run_git(repo.path(), &["commit", "-m", "c1"]); // a REAL commit → genuinely non-unborn

    // Break git at INDEX time. `basis_at_index` → `head_commit` → `is_git_repo` now returns
    // Err(CommandFailed{"unknown repository extension found"}); the daemon classifies it a
    // GENERIC failure and records it on the snapshot's diagnostics via the pre-`Ready` path.
    let pristine = poison_git_config_unknown_extension(repo.path());

    let payload = index(&dispatcher, repo.path());
    // (b) NULL basis — never a fabricated sha.
    assert_eq!(
        persisted_basis(&payload),
        None,
        "a generic HEAD failure records NULL basis, never a fabricated one"
    );

    // (c) WRITE proof via the NORMAL pre-`Ready` path (RULING 4): compose produced the DTO and
    // the orchestrator merged it into extraction diagnostics BEFORE `Ready` — no set_index_basis.
    let db_path = payload["db_path"].as_str().unwrap();
    let snapshot_uid = payload["snapshot_uid"].as_str().unwrap();
    let blob = diagnostics_blob(db_path, snapshot_uid)
        .expect("the index records an extraction-diagnostics blob");
    let diag: Value = serde_json::from_str(&blob).unwrap();
    assert_eq!(
        diag["index_basis"]["outcome"].as_str(),
        Some("failure"),
        "the write path recorded a failure outcome for the generic HEAD failure: {blob}"
    );
    let reason = diag["index_basis"]["reason"]
        .as_str()
        .expect("a failure outcome carries a reason string");
    assert!(
        reason.contains("git HEAD unreadable at index time"),
        "generic reason, classified at write time: {blob}"
    );
    assert!(
        reason.contains("unknown repository extension"),
        "the reason surfaces git's actual stderr verbatim: {blob}"
    );
    assert_ne!(
        reason, "repository has no commits yet",
        "a NON-unborn failure must NOT borrow the empty-repo wording: {blob}"
    );

    // Repair git BEFORE the query. The RECORDED fact is authoritative: `compute_index_drift`'s
    // query-time `is_git_repo` now succeeds, the basis is still NULL, and `basis_none_state`
    // surfaces the RECORDED generic reason — proving the render comes from the persisted record,
    // not a live re-probe.
    restore_git_config(repo.path(), &pristine);

    // (d) END-TO-END render: orient re-reads the record and renders unknown-with-reason.
    let ori = serde_json::to_string(&orient(&dispatcher, repo.path())).unwrap();
    assert!(
        ori.contains("git HEAD unreadable at index time")
            && ori.contains("unknown repository extension"),
        "orient surfaces the recorded generic reason: {ori}"
    );
    assert!(
        !ori.contains("no commits yet"),
        "a generic failure must not borrow the unborn wording: {ori}"
    );
    assert!(
        !ori.contains("basis_unknown") && !ori.contains("not_git"),
        "a recorded failure is unknown-with-reason, never pre-slice or NotGit: {ori}"
    );
}

#[test]
fn index_of_a_committed_repo_with_broken_head_is_generic_never_no_commits() {
    // review-9 #1 REGRESSION: the reviewer-flagged false-positive. A repo WITH a commit whose
    // HEAD points at a MISSING branch makes `git rev-parse HEAD` fail with the SAME
    // `fatal: ambiguous argument 'HEAD': unknown revision …` stderr an UNBORN repo emits.
    // The OLD stderr-text classifier would render "repository has no commits yet" — FALSE, the
    // repo has a commit. The POSITIVE `git rev-list -n 1 --all` probe still finds that commit,
    // so the daemon MUST record & render the GENERIC "git HEAD unreadable at index time",
    // NEVER the empty-repo wording. Proven end-to-end: index → recorded record → orient.
    let (dispatcher, _root) = isolated();
    let repo = tempdir().unwrap();
    init_git(repo.path());
    write_source(repo.path());
    run_git(repo.path(), &["add", "-A"]);
    run_git(repo.path(), &["commit", "-m", "c1"]); // a REAL commit exists

    // Break HEAD at INDEX time: point it at a branch that does not exist. `is_git_repo`
    // (rev-parse --git-dir) still succeeds; `rev-parse HEAD` fails with `unknown revision`.
    break_head_to_missing_branch(repo.path());

    let payload = index(&dispatcher, repo.path());
    assert_eq!(
        persisted_basis(&payload),
        None,
        "a broken HEAD records NULL basis, never a fabricated one"
    );

    // WRITE proof: the record is a GENERIC failure — the positive probe found the commit, so
    // unborn is NOT claimed.
    let db_path = payload["db_path"].as_str().unwrap();
    let snapshot_uid = payload["snapshot_uid"].as_str().unwrap();
    let blob = diagnostics_blob(db_path, snapshot_uid)
        .expect("the index records an extraction-diagnostics blob");
    let diag: Value = serde_json::from_str(&blob).unwrap();
    assert_eq!(
        diag["index_basis"]["outcome"].as_str(),
        Some("failure"),
        "a broken HEAD is a failure outcome: {blob}"
    );
    let reason = diag["index_basis"]["reason"]
        .as_str()
        .expect("a failure outcome carries a reason");
    assert!(
        reason.contains("git HEAD unreadable at index time") && reason.contains("unknown revision"),
        "a committed repo with a broken HEAD is GENERIC (surfaces the real stderr): {blob}"
    );
    assert_ne!(
        reason, "repository has no commits yet",
        "review-9 #1: a repo WITH commits must NEVER be classified as empty: {blob}"
    );

    // END-TO-END render: repair HEAD, then orient renders the recorded GENERIC reason, never
    // the empty-repo wording.
    run_git(repo.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    let ori = serde_json::to_string(&orient(&dispatcher, repo.path())).unwrap();
    assert!(
        ori.contains("git HEAD unreadable at index time"),
        "orient surfaces the recorded generic reason: {ori}"
    );
    assert!(
        !ori.contains("no commits yet"),
        "the broken-HEAD-with-commits repo must NEVER render 'no commits yet': {ori}"
    );
}

#[test]
fn a_pre_slice_null_basis_with_no_record_renders_pre_slice_message_end_to_end() {
    // review-5 / RULING 3 required e2e: a snapshot indexed BEFORE this slice — NULL
    // basis_commit AND no `index_basis` record — renders the UNCHANGED pre-slice state
    // (BasisUnknown), never non-git, never unknown-with-reason. RULING 3 makes this
    // deterministic: every post-slice write records an outcome, so "NULL + no record" is
    // unambiguously pre-slice. We reproduce that persisted state on a REAL git repo with a
    // NULL basis (unborn — `is_git_repo` == true) by stripping the `index_basis` key from
    // its diagnostics blob, then prove the query renders BasisUnknown.
    let (dispatcher, _root) = isolated();
    let repo = tempdir().unwrap();
    init_git(repo.path());
    write_source(repo.path());

    let payload = index(&dispatcher, repo.path());
    assert_eq!(persisted_basis(&payload), None);
    let db_path = payload["db_path"].as_str().unwrap();
    let snapshot_uid = payload["snapshot_uid"].as_str().unwrap();

    // Strip the recorded outcome → simulate a pre-slice NULL snapshot (no `index_basis`),
    // preserving the rest of the diagnostics blob the trust reader requires.
    strip_index_basis(db_path, snapshot_uid);

    let ori = serde_json::to_string(&orient(&dispatcher, repo.path())).unwrap();
    assert!(
        ori.contains("\"index_drift\":{\"state\":\"basis_unknown\"}"),
        "a pre-slice NULL basis (no record) renders BasisUnknown: {ori}"
    );
}
