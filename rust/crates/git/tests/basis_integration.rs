//! Integration tests for index-basis + working-tree drift extraction.
//!
//! INDEX-BASIS-1: exercises [`head_commit`] and [`working_tree_drift`] against
//! real temporary git repositories with controlled state.

use std::fs;
use std::process::Command;

use repo_graph_git::{head_commit, is_git_repo, is_unborn_head, working_tree_drift, GitError};

/// Temp git repo helper (mirrors the churn integration harness).
struct TestRepo {
    dir: tempfile::TempDir,
}

impl TestRepo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("create temp dir");
        run_git(dir.path(), &["init"]);
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        run_git(dir.path(), &["config", "user.name", "Test User"]);
        // Pin the initial branch name so behavior is stable across git versions.
        run_git(dir.path(), &["checkout", "-B", "main"]);
        Self { dir }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    fn write_file(&self, path: &str, content: &str) {
        let full = self.dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&full, content).expect("write file");
    }

    fn commit(&self, message: &str) {
        run_git(self.dir.path(), &["add", "-A"]);
        run_git(self.dir.path(), &["commit", "-m", message, "--allow-empty"]);
    }

    fn head(&self) -> String {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(self.dir.path())
            .output()
            .expect("rev-parse");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
}

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run git");
    if !output.status.success() {
        panic!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// ── head_commit ─────────────────────────────────────────────────

#[test]
fn head_commit_returns_sha() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit("c1");

    let sha = head_commit(repo.path()).expect("head_commit ok");
    assert_eq!(sha, Some(repo.head()));
    assert_eq!(sha.as_deref().map(str::len), Some(40));
}

#[test]
fn head_commit_none_when_not_a_git_repo() {
    let dir = tempfile::tempdir().unwrap();
    // A plain temp dir is not a git repo → Ok(None), never an error.
    assert_eq!(head_commit(dir.path()).unwrap(), None);
    assert!(!is_git_repo(dir.path()).unwrap());
}

#[test]
fn head_commit_errors_on_empty_repo() {
    // A git repo with zero commits has no HEAD: rev-parse fails → Err (a real
    // git failure), distinct from the not-a-repo None above.
    let repo = TestRepo::new();
    match head_commit(repo.path()) {
        Err(GitError::CommandFailed { .. }) => {}
        other => panic!("expected CommandFailed on empty repo, got {other:?}"),
    }
}

// ── working_tree_drift ──────────────────────────────────────────

#[test]
fn drift_clean_when_head_equals_basis() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit("c1");
    let basis = repo.head();

    let drift = working_tree_drift(repo.path(), &basis).expect("drift ok");
    assert_eq!(drift.commits_ahead, 0);
    assert!(drift.changed_files.is_empty(), "{:?}", drift.changed_files);
}

#[test]
fn drift_counts_commits_ahead_and_changed_files() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.write_file("src/b.rs", "fn b() {}\n");
    repo.commit("c1");
    let basis = repo.head();

    // One new commit modifying a.txt (committed change).
    repo.write_file("a.txt", "one\ntwo\n");
    repo.commit("c2");

    // An uncommitted edit to a tracked file.
    repo.write_file("src/b.rs", "fn b() { /* edit */ }\n");
    // An untracked new file.
    repo.write_file("src/c.rs", "fn c() {}\n");

    let drift = working_tree_drift(repo.path(), &basis).expect("drift ok");
    assert_eq!(drift.commits_ahead, 1, "one commit past basis");
    assert_eq!(
        drift.changed_files,
        vec![
            "a.txt".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ],
        "committed + uncommitted + untracked, deduped & sorted"
    );
}

#[test]
fn drift_reports_quoted_paths_verbatim() {
    // A path git would C-quote in its LINE format (non-ASCII bytes under the
    // default core.quotepath, plus a space) must arrive VERBATIM through the `-z`
    // machine format — never the mangled `"r\303\251pertoire ..."` octal form.
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    // Commit a file inside src/ at the basis so the DIRECTORY is tracked — an
    // untracked directory would collapse to `?? src/`; a single untracked FILE in a
    // tracked dir is reported individually (so the special path actually surfaces).
    repo.write_file("src/keep.txt", "keep\n");
    repo.commit("c1");
    let basis = repo.head();

    // Unicode + space in the filename → triggers C-quoting in the non-z format.
    let special = "src/résumé café.txt";
    repo.write_file(special, "x\n"); // untracked file in a tracked dir → shows in -z

    let drift = working_tree_drift(repo.path(), &basis).expect("drift ok");
    assert!(
        drift.changed_files.contains(&special.to_string()),
        "quoted/special path must be verbatim, got {:?}",
        drift.changed_files
    );
    // And never the C-quoted/escaped form.
    assert!(
        !drift
            .changed_files
            .iter()
            .any(|p| p.contains('\\') || p.contains('"')),
        "no C-quoting/escaping leaked through: {:?}",
        drift.changed_files
    );
}

#[test]
fn drift_lists_each_nested_untracked_file_not_the_directory_placeholder() {
    // A WHOLLY untracked directory: git's default `--untracked-files=normal`
    // collapses it to a single `?? new_pkg/` placeholder. `working_tree_drift`
    // must run `--untracked-files=all` so each nested file is reported
    // individually — otherwise one directory would be miscounted as one changed
    // "file", the nested files undercounted, and none intersectable with the
    // indexed (per-file) set.
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit("c1");
    let basis = repo.head();

    // An entirely new, untracked directory tree (never `git add`-ed).
    repo.write_file("new_pkg/mod.rs", "fn a() {}\n");
    repo.write_file("new_pkg/util/helpers.rs", "fn b() {}\n");
    repo.write_file("new_pkg/util/deep/inner.rs", "fn c() {}\n");

    let drift = working_tree_drift(repo.path(), &basis).expect("drift ok");
    assert_eq!(
        drift.changed_files,
        vec![
            "new_pkg/mod.rs".to_string(),
            "new_pkg/util/deep/inner.rs".to_string(),
            "new_pkg/util/helpers.rs".to_string(),
        ],
        "each nested untracked file listed individually, sorted"
    );
    // And NEVER the directory placeholder (which the default mode would emit).
    assert!(
        !drift
            .changed_files
            .iter()
            .any(|p| p.ends_with('/') || p == "new_pkg"),
        "no `?? dir/` placeholder leaked through: {:?}",
        drift.changed_files
    );
}

#[test]
fn drift_errors_when_basis_unknown() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit("c1");

    // A basis sha that does not exist in this repo → CommandFailed, never "clean".
    let bogus = "0000000000000000000000000000000000000000";
    match working_tree_drift(repo.path(), bogus) {
        Err(GitError::CommandFailed { .. }) => {}
        other => panic!("expected CommandFailed for unknown basis, got {other:?}"),
    }
}

#[test]
fn drift_errors_when_not_a_git_repo() {
    let dir = tempfile::tempdir().unwrap();
    match working_tree_drift(dir.path(), "HEAD") {
        Err(GitError::NotARepository(_)) => {}
        other => panic!("expected NotARepository, got {other:?}"),
    }
}

// ── is_unborn_head (review-9 #1: POSITIVE unborn establishment) ──────────

#[test]
fn is_unborn_head_true_on_a_commitless_repo() {
    // `git init` with no commit → the commit graph is empty → positively unborn.
    let repo = TestRepo::new(); // init + config only, never commits
    assert!(
        is_unborn_head(repo.path()).expect("probe runs on an unborn repo"),
        "a repo with zero commits is unborn"
    );
}

#[test]
fn is_unborn_head_false_when_the_repo_has_a_commit() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit("c1");
    assert!(
        !is_unborn_head(repo.path()).expect("probe runs on a committed repo"),
        "a repo with a commit is not unborn"
    );
}

#[test]
fn is_unborn_head_false_for_a_committed_repo_with_a_broken_head() {
    // THE review-9 #1 CASE: a repo WITH a commit whose HEAD points at a missing branch.
    // `rev-parse HEAD` fails with `ambiguous argument 'HEAD': unknown revision` — the same
    // stderr an unborn repo emits — but the commit still exists and is reachable from the
    // real branch, so `is_unborn_head` must return `false` (the probe finds the commit),
    // never impersonating an empty repo.
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit("c1");
    fs::write(
        repo.path().join(".git").join("HEAD"),
        "ref: refs/heads/does-not-exist\n",
    )
    .expect("break HEAD");

    // Precondition: `rev-parse HEAD` fails (this is what would tempt the stderr classifier).
    assert!(
        head_commit(repo.path()).is_err(),
        "broken HEAD makes rev-parse HEAD fail"
    );
    // But the positive probe still finds the commit → NOT unborn.
    assert!(
        !is_unborn_head(repo.path()).expect("probe runs despite the broken HEAD"),
        "a committed repo with a broken HEAD is NOT unborn — it has a commit"
    );
}

#[test]
fn is_unborn_head_errs_on_a_non_git_dir() {
    // Not a git repo → `rev-list` fails → Err (the caller must not claim unborn on Err).
    let dir = tempfile::tempdir().unwrap();
    assert!(is_unborn_head(dir.path()).is_err());
}
