//! Integration tests for [`diagnose_history`] against real temporary git repos.
//!
//! CHURN-SHALLOW-1 (review-1 #2): the pure `classify_history` unit tests pin the
//! branch logic from already-extracted facts; these tests pin the I/O function's
//! REACHABILITY DOMAIN — that `diagnose_history` counts commits from HEAD (what churn
//! walks), not from `--all` (which would let an unrelated ref's history inflate the
//! count and hide a single-commit whole-tree HEAD). Constructing these shapes requires
//! a real repo, so they live here rather than in the src-level pure tests.

use std::fs;
use std::process::Command;

use repo_graph_git::{diagnose_history, ChurnWindow, HistoryShape};

/// Temp git repo helper (mirrors the churn/basis integration harnesses).
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

/// review-1 #2: a HEAD that is a single whole-tree commit (an orphan branch) while an
/// unrelated ref (`main`) holds deeper history. `rev-list --count --all` would report 3
/// and classify this Healthy/ZeroInWindow, mis-framing the whole-tree HEAD snapshot as
/// ordinary churn. HEAD-domain counting (`rev-list --count HEAD`) sees exactly ONE
/// commit → `ShallowOrSingle`, the honest shape.
#[test]
fn diagnose_uses_head_domain_not_all_refs() {
    let repo = TestRepo::new();

    // `main`: two recent commits (in-window), reachable ONLY from `main`.
    repo.write_file("src/a.rs", "fn a() {}\n");
    repo.commit("main #1");
    repo.write_file("src/b.rs", "fn b() {}\n");
    repo.commit("main #2");

    // Orphan branch: HEAD becomes a single parentless whole-tree commit.
    run_git(repo.path(), &["checkout", "--orphan", "snapshot"]);
    repo.write_file("src/snapshot.rs", "fn snap() {}\n");
    repo.commit("orphan snapshot (whole tree, no parent)");

    let window = ChurnWindow::new("90.days.ago");
    match diagnose_history(repo.path(), &window).unwrap() {
        HistoryShape::ShallowOrSingle {
            commits_available,
            is_shallow,
            commits_in_window,
            ..
        } => {
            assert_eq!(
                commits_available, 1,
                "HEAD reaches ONE commit; the 2 commits on `main` must not inflate the count"
            );
            assert!(
                !is_shallow,
                "an orphan single-commit HEAD is not a shallow clone"
            );
            assert_eq!(
                commits_in_window, 1,
                "the orphan commit is recent → in window"
            );
        }
        other => panic!(
            "divergent non-HEAD refs must not inflate the HEAD count \
             (got {other:?}); this is the review-1 #2 regression"
        ),
    }
}

/// A genuine multi-commit HEAD with recent activity still classifies Healthy — the
/// HEAD-domain switch must not over-correct real history into ShallowOrSingle.
#[test]
fn diagnose_multi_commit_head_is_healthy() {
    let repo = TestRepo::new();
    repo.write_file("src/a.rs", "1\n");
    repo.commit("c1");
    repo.write_file("src/a.rs", "2\n");
    repo.commit("c2");
    repo.write_file("src/a.rs", "3\n");
    repo.commit("c3");

    let window = ChurnWindow::new("90.days.ago");
    assert_eq!(
        diagnose_history(repo.path(), &window).unwrap(),
        HistoryShape::Healthy
    );
}

/// An unborn repo (`git init`, zero commits) is NoHistory — the HEAD-domain path
/// establishes unborn positively (via `is_unborn_head`), never guessing from stderr.
#[test]
fn diagnose_unborn_head_is_no_history() {
    let repo = TestRepo::new(); // init only, no commits
    let window = ChurnWindow::new("90.days.ago");
    assert_eq!(
        diagnose_history(repo.path(), &window).unwrap(),
        HistoryShape::NoHistory
    );
}
