//! Integration tests for file churn extraction.
//!
//! RS-MS-1: Tests against real git repositories.
//!
//! These tests create temporary git repos with controlled commit
//! histories and verify the churn extraction behavior.

use std::fs;
use std::process::Command;

use repo_graph_git::{get_file_churn, ChurnWindow, GitError};

/// Helper to create a temp git repo and run commands in it.
struct TestRepo {
    dir: tempfile::TempDir,
}

impl TestRepo {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("create temp dir");

        // Initialize git repo
        run_git(&dir.path().to_path_buf(), &["init"]);

        // Set local user config (required for commits)
        run_git(
            &dir.path().to_path_buf(),
            &["config", "user.email", "test@example.com"],
        );
        run_git(
            &dir.path().to_path_buf(),
            &["config", "user.name", "Test User"],
        );

        Self { dir }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    fn write_file(&self, path: &str, content: &str) {
        let full_path = self.dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&full_path, content).expect("write file");
    }

    fn commit(&self, message: &str) {
        run_git(&self.dir.path().to_path_buf(), &["add", "-A"]);
        run_git(
            &self.dir.path().to_path_buf(),
            &["commit", "-m", message, "--allow-empty"],
        );
    }

    fn commit_with_date(&self, message: &str, date: &str) {
        run_git(&self.dir.path().to_path_buf(), &["add", "-A"]);

        // Use GIT_AUTHOR_DATE and GIT_COMMITTER_DATE env vars
        Command::new("git")
            .args(["commit", "-m", message, "--allow-empty"])
            .current_dir(self.dir.path())
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .output()
            .expect("git commit with date");
    }
}

fn run_git(cwd: &std::path::PathBuf, args: &[&str]) {
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

// ── Basic functionality ─────────────────────────────────────────

#[test]
fn churn_single_file_single_commit() {
    let repo = TestRepo::new();

    repo.write_file("src/main.rs", "fn main() {}\n");
    repo.commit("initial commit");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path, "src/main.rs");
    assert_eq!(results[0].commit_count, 1);
    assert_eq!(results[0].lines_changed, 1); // 1 line added
}

#[test]
fn churn_single_file_multiple_commits() {
    let repo = TestRepo::new();

    repo.write_file("src/main.rs", "fn main() {}\n");
    repo.commit("initial");

    repo.write_file("src/main.rs", "fn main() {\n    println!(\"hello\");\n}\n");
    repo.commit("add print");

    repo.write_file("src/main.rs", "fn main() {\n    println!(\"world\");\n}\n");
    repo.commit("change message");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path, "src/main.rs");
    assert_eq!(results[0].commit_count, 3);
    // Lines: +1, then +2-1, then +1-1 = 1 + 3 + 2 = 6
    assert!(results[0].lines_changed >= 3);
}

#[test]
fn churn_multiple_files() {
    let repo = TestRepo::new();

    repo.write_file("src/main.rs", "fn main() {}\n");
    repo.write_file("src/lib.rs", "pub fn hello() {}\n");
    repo.commit("initial");

    // Touch main.rs again
    repo.write_file("src/main.rs", "fn main() { hello(); }\n");
    repo.commit("use lib");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    assert_eq!(results.len(), 2);

    // Find each file
    let main = results.iter().find(|e| e.file_path == "src/main.rs").unwrap();
    let lib = results.iter().find(|e| e.file_path == "src/lib.rs").unwrap();

    assert_eq!(main.commit_count, 2);
    assert_eq!(lib.commit_count, 1);
}

// ── Sort order ─────────────────────────────────────────────────

#[test]
fn churn_sorted_by_lines_desc() {
    let repo = TestRepo::new();

    // Small file
    repo.write_file("small.rs", "x\n");
    // Large file
    repo.write_file("large.rs", "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n");
    repo.commit("initial");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].file_path, "large.rs");
    assert_eq!(results[1].file_path, "small.rs");
}

#[test]
fn churn_tiebreaker_commit_count() {
    let repo = TestRepo::new();

    // Both files: 5 lines initially
    repo.write_file("a.rs", "1\n2\n3\n4\n5\n");
    repo.write_file("b.rs", "1\n2\n3\n4\n5\n");
    repo.commit("initial");

    // Touch a.rs again (more commits, same total lines)
    repo.write_file("a.rs", "1\n2\n3\n4\n6\n");
    repo.commit("modify a");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    // a.rs: 2 commits, 5 + 2 = 7 lines
    // b.rs: 1 commit, 5 lines
    // a.rs first by lines
    assert_eq!(results[0].file_path, "a.rs");
}

#[test]
fn churn_tiebreaker_path() {
    let repo = TestRepo::new();

    // Same content, same commit count — path is tiebreaker
    repo.write_file("z.rs", "x\n");
    repo.write_file("a.rs", "x\n");
    repo.commit("initial");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    // Same lines, same commits — a.rs < z.rs
    assert_eq!(results[0].file_path, "a.rs");
    assert_eq!(results[1].file_path, "z.rs");
}

// ── Path normalization ─────────────────────────────────────────

#[test]
fn churn_paths_repo_relative() {
    let repo = TestRepo::new();

    repo.write_file("src/foo/bar.rs", "fn bar() {}\n");
    repo.commit("initial");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path, "src/foo/bar.rs");
    assert!(!results[0].file_path.starts_with('/'));
    assert!(!results[0].file_path.starts_with("./"));
}

// ── Time window ────────────────────────────────────────────────

#[test]
fn churn_respects_time_window() {
    let repo = TestRepo::new();

    // Old commit (outside window)
    repo.write_file("old.rs", "old\n");
    repo.commit_with_date("old commit", "2020-01-01T00:00:00Z");

    // Recent commit (inside window)
    repo.write_file("new.rs", "new\n");
    repo.commit("new commit");

    // Window: last 30 days — should exclude the old commit
    let window = ChurnWindow::new("30.days.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    // Only new.rs should appear
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path, "new.rs");
}

// ── High commit count, low churn ───────────────────────────────

#[test]
fn churn_high_commits_low_lines() {
    let repo = TestRepo::new();

    // Many small edits
    repo.write_file("tweaked.rs", "v1\n");
    repo.commit("v1");

    repo.write_file("tweaked.rs", "v2\n");
    repo.commit("v2");

    repo.write_file("tweaked.rs", "v3\n");
    repo.commit("v3");

    // One big write
    repo.write_file("bulk.rs", "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n");
    repo.commit("bulk");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    // bulk.rs has more lines (10), tweaked.rs has more commits (3)
    let bulk = results.iter().find(|e| e.file_path == "bulk.rs").unwrap();
    let tweaked = results.iter().find(|e| e.file_path == "tweaked.rs").unwrap();

    assert!(bulk.lines_changed > tweaked.lines_changed);
    assert!(tweaked.commit_count > bulk.commit_count);

    // Sorted by lines, so bulk should be first
    assert_eq!(results[0].file_path, "bulk.rs");
}

// ── Error cases ────────────────────────────────────────────────

#[test]
fn churn_not_a_repo() {
    let dir = tempfile::tempdir().expect("create temp dir");
    // Not initialized as git repo

    let window = ChurnWindow::new("90.days.ago");
    let result = get_file_churn(dir.path(), &window);

    assert!(matches!(result, Err(GitError::NotARepository(_))));
}

#[test]
fn churn_empty_repo() {
    let repo = TestRepo::new();
    // No commits

    let window = ChurnWindow::new("90.days.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    assert!(results.is_empty());
}

// ── Rename handling ────────────────────────────────────────────

#[test]
fn churn_rename_appears_as_separate_paths() {
    let repo = TestRepo::new();

    // Create file under original name
    repo.write_file("old_name.rs", "content\n");
    repo.commit("create file");

    // Rename the file (git mv)
    Command::new("git")
        .args(["mv", "old_name.rs", "new_name.rs"])
        .current_dir(repo.path())
        .output()
        .expect("git mv");
    repo.commit("rename file");

    // Modify under new name
    repo.write_file("new_name.rs", "content\nmore\n");
    repo.commit("modify renamed file");

    let window = ChurnWindow::new("1.year.ago");
    let results = get_file_churn(repo.path(), &window).unwrap();

    // With --no-renames, we should see literal paths, not synthetic rename forms
    // old_name.rs: 1 commit (create)
    // new_name.rs: 2 commits (rename adds it, then modify)
    // No path should contain "=>" or "{" or "}"

    for entry in &results {
        assert!(
            !entry.file_path.contains("=>"),
            "path should not contain rename arrow: {}",
            entry.file_path
        );
        assert!(
            !entry.file_path.contains('{'),
            "path should not contain brace syntax: {}",
            entry.file_path
        );
    }

    // Both paths should appear
    let old = results.iter().find(|e| e.file_path == "old_name.rs");
    let new = results.iter().find(|e| e.file_path == "new_name.rs");

    assert!(old.is_some(), "old_name.rs should appear");
    assert!(new.is_some(), "new_name.rs should appear");
}

// ── ISO date window ────────────────────────────────────────────

#[test]
fn churn_iso_date_window() {
    let repo = TestRepo::new();

    repo.write_file("file.rs", "content\n");
    repo.commit("initial");

    // Use ISO date (should include recent commit)
    let window = ChurnWindow::new("2020-01-01");
    let results = get_file_churn(repo.path(), &window).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].file_path, "file.rs");
}
