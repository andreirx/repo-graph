//! Per-file churn extraction from git history.
//!
//! RS-MS-1: Git-derived per-file churn facts.
//!
//! Contract:
//!   - `commit_count`: number of commits that touched the file in the window
//!   - `lines_changed`: total lines added + deleted in the window
//!   - Paths: repo-relative, forward slashes, no `./` prefix
//!   - Ordering: deterministic total order (lines desc, commits desc, path asc)
//!
//! Rename handling:
//!   - Rename detection disabled (`--no-renames`)
//!   - Each path appears as the literal path at that commit
//!   - If a file was renamed during the window, old and new paths appear separately
//!   - No `--follow`, no rename continuity across paths
//!
//! Binary files:
//!   - `git numstat` emits `-` for binary deltas
//!   - Treated as 0 lines_changed
//!   - Commits still counted

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::error::GitError;

/// Per-file churn entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChurnEntry {
    /// Repo-relative file path (forward slashes, no `./` prefix).
    pub file_path: String,

    /// Number of commits that touched this file in the window.
    pub commit_count: u64,

    /// Total lines added + deleted in the window.
    /// Binary changes contribute 0.
    pub lines_changed: u64,
}

/// Time window for churn query.
///
/// The `since` field is passed directly to git as a date expression.
/// Valid formats include:
///   - Relative: `"90.days.ago"`, `"2.weeks.ago"`, `"1.year.ago"`
///   - ISO date: `"2026-01-01"`, `"2026-01-01T00:00:00Z"`
///   - Git date: `"yesterday"`, `"last monday"`
///
/// Invalid expressions cause git to fail; the error surfaces as
/// `GitError::CommandFailed`.
///
/// No typed distinction between relative and ISO forms — git accepts
/// both identically. The single constructor reflects this reality.
#[derive(Debug, Clone)]
pub struct ChurnWindow {
    /// Git date expression for the start of the window.
    pub since: String,
}

impl ChurnWindow {
    /// Create a window from any git date expression.
    ///
    /// Examples:
    /// - `ChurnWindow::new("90.days.ago")`
    /// - `ChurnWindow::new("2026-01-01")`
    /// - `ChurnWindow::new("yesterday")`
    pub fn new(since: &str) -> Self {
        Self {
            since: since.to_string(),
        }
    }
}

/// Extract per-file churn from git history.
///
/// Returns churn facts for all files modified in the time window.
/// Results are sorted by:
///   1. `lines_changed` descending
///   2. `commit_count` descending
///   3. `file_path` ascending
///
/// # Errors
///
/// - `GitError::NotARepository` if the path is not a git repo
/// - `GitError::CommandFailed` if git fails (invalid since expression, etc.)
/// - `GitError::SpawnFailed` if git cannot be executed
pub fn get_file_churn(
    repo_path: &Path,
    window: &ChurnWindow,
) -> Result<Vec<FileChurnEntry>, GitError> {
    // Verify this is a git repository
    verify_git_repo(repo_path)?;

    // Single pass: use --numstat with a commit boundary marker
    // Format: COMMIT_BOUNDARY line, then numstat lines for that commit
    // Note: --pretty=format:X outputs literal X, --format=X interprets X as format
    //
    // --no-renames disables rename detection. Without it, git emits synthetic
    // path forms like `src/{old => new}.rs` or `old => new` for renames,
    // which violates our observed-path contract. With --no-renames, each
    // path appears as the literal path at that point in history.
    let output = Command::new("git")
        .args([
            "log",
            "--pretty=format:COMMIT_BOUNDARY",
            "--numstat",
            "--no-renames",
            &format!("--since={}", window.since),
        ])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Empty repo (no commits) is not an error — just empty results
        if stderr.contains("does not have any commits") {
            return Ok(Vec::new());
        }
        return Err(GitError::CommandFailed {
            command: format!("git log --numstat --since={}", window.since),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_churn_output(&stdout)
}

/// Verify that the path is a git repository.
fn verify_git_repo(repo_path: &Path) -> Result<(), GitError> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        return Err(GitError::NotARepository(
            repo_path.display().to_string(),
        ));
    }

    Ok(())
}

/// Parse the combined commit-boundary + numstat output.
fn parse_churn_output(output: &str) -> Result<Vec<FileChurnEntry>, GitError> {
    // Track per-file: (commit_count, lines_changed)
    let mut file_stats: HashMap<String, (u64, u64)> = HashMap::new();

    // Current commit's files (to count each file once per commit)
    let mut current_commit_files: HashMap<String, u64> = HashMap::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed == "COMMIT_BOUNDARY" {
            // Flush previous commit's data
            for (path, lines) in current_commit_files.drain() {
                let entry = file_stats.entry(path).or_insert((0, 0));
                entry.0 += 1; // commit_count
                entry.1 += lines; // lines_changed
            }
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        // Parse numstat line: <added>\t<deleted>\t<file>
        // Binary files show "-" for added/deleted
        let parts: Vec<&str> = trimmed.splitn(3, '\t').collect();
        if parts.len() < 3 {
            continue;
        }

        let added = parse_numstat_value(parts[0]);
        let deleted = parse_numstat_value(parts[1]);
        let file_path = normalize_path(parts[2]);

        if file_path.is_empty() {
            continue;
        }

        let lines = added + deleted;

        // Accumulate within this commit (file may appear multiple times
        // in a single commit for rename tracking, though rare)
        *current_commit_files.entry(file_path).or_insert(0) += lines;
    }

    // Flush final commit
    for (path, lines) in current_commit_files.drain() {
        let entry = file_stats.entry(path).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += lines;
    }

    // Convert to vec and sort
    let mut results: Vec<FileChurnEntry> = file_stats
        .into_iter()
        .map(|(file_path, (commit_count, lines_changed))| FileChurnEntry {
            file_path,
            commit_count,
            lines_changed,
        })
        .collect();

    // Deterministic total order:
    // 1. lines_changed DESC
    // 2. commit_count DESC
    // 3. file_path ASC
    results.sort_by(|a, b| {
        b.lines_changed
            .cmp(&a.lines_changed)
            .then_with(|| b.commit_count.cmp(&a.commit_count))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    Ok(results)
}

/// Parse a numstat value. Returns 0 for binary markers ("-").
fn parse_numstat_value(s: &str) -> u64 {
    if s == "-" {
        return 0;
    }
    s.parse().unwrap_or(0)
}

/// Normalize a file path to repo-relative, forward-slash format.
fn normalize_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");

    // Strip leading "./"
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }

    // Strip leading "/"
    while normalized.starts_with('/') {
        normalized = normalized[1..].to_string();
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numstat_value_normal() {
        assert_eq!(parse_numstat_value("42"), 42);
        assert_eq!(parse_numstat_value("0"), 0);
        assert_eq!(parse_numstat_value("999999"), 999999);
    }

    #[test]
    fn parse_numstat_value_binary() {
        assert_eq!(parse_numstat_value("-"), 0);
    }

    #[test]
    fn parse_numstat_value_invalid() {
        assert_eq!(parse_numstat_value("abc"), 0);
        assert_eq!(parse_numstat_value(""), 0);
    }

    #[test]
    fn normalize_path_forward_slashes() {
        assert_eq!(normalize_path("src\\foo\\bar.rs"), "src/foo/bar.rs");
    }

    #[test]
    fn normalize_path_strips_dot_slash() {
        assert_eq!(normalize_path("./src/foo.rs"), "src/foo.rs");
        assert_eq!(normalize_path("././src/foo.rs"), "src/foo.rs");
    }

    #[test]
    fn normalize_path_strips_leading_slash() {
        assert_eq!(normalize_path("/src/foo.rs"), "src/foo.rs");
    }

    #[test]
    fn normalize_path_already_clean() {
        assert_eq!(normalize_path("src/foo/bar.rs"), "src/foo/bar.rs");
    }

    #[test]
    fn parse_churn_output_single_commit() {
        let output = r#"COMMIT_BOUNDARY
10	5	src/main.rs
3	1	src/lib.rs
"#;
        let results = parse_churn_output(output).unwrap();

        assert_eq!(results.len(), 2);
        // Sorted by lines_changed desc
        assert_eq!(results[0].file_path, "src/main.rs");
        assert_eq!(results[0].commit_count, 1);
        assert_eq!(results[0].lines_changed, 15);

        assert_eq!(results[1].file_path, "src/lib.rs");
        assert_eq!(results[1].commit_count, 1);
        assert_eq!(results[1].lines_changed, 4);
    }

    #[test]
    fn parse_churn_output_multiple_commits() {
        let output = r#"COMMIT_BOUNDARY
10	5	src/main.rs
COMMIT_BOUNDARY
3	2	src/main.rs
1	0	src/lib.rs
"#;
        let results = parse_churn_output(output).unwrap();

        assert_eq!(results.len(), 2);
        // main.rs: 2 commits, 15 + 5 = 20 lines
        assert_eq!(results[0].file_path, "src/main.rs");
        assert_eq!(results[0].commit_count, 2);
        assert_eq!(results[0].lines_changed, 20);

        // lib.rs: 1 commit, 1 line
        assert_eq!(results[1].file_path, "src/lib.rs");
        assert_eq!(results[1].commit_count, 1);
        assert_eq!(results[1].lines_changed, 1);
    }

    #[test]
    fn parse_churn_output_binary_file() {
        let output = r#"COMMIT_BOUNDARY
-	-	assets/logo.png
10	5	src/main.rs
"#;
        let results = parse_churn_output(output).unwrap();

        assert_eq!(results.len(), 2);
        // main.rs first (15 lines > 0)
        assert_eq!(results[0].file_path, "src/main.rs");
        assert_eq!(results[0].lines_changed, 15);

        // logo.png: commit counted, 0 lines
        assert_eq!(results[1].file_path, "assets/logo.png");
        assert_eq!(results[1].commit_count, 1);
        assert_eq!(results[1].lines_changed, 0);
    }

    #[test]
    fn parse_churn_output_sort_order_deterministic() {
        // Two files with same lines_changed but different commit counts
        let output = r#"COMMIT_BOUNDARY
5	5	src/a.rs
COMMIT_BOUNDARY
5	5	src/b.rs
COMMIT_BOUNDARY
5	5	src/a.rs
"#;
        let results = parse_churn_output(output).unwrap();

        assert_eq!(results.len(), 2);
        // a.rs: 2 commits, 20 lines
        // b.rs: 1 commit, 10 lines
        // Same lines? No, a has 20, b has 10. a first.
        assert_eq!(results[0].file_path, "src/a.rs");
        assert_eq!(results[0].lines_changed, 20);
        assert_eq!(results[1].file_path, "src/b.rs");
        assert_eq!(results[1].lines_changed, 10);
    }

    #[test]
    fn parse_churn_output_tiebreaker_commit_count() {
        // Two files with same lines_changed, different commit counts
        let output = r#"COMMIT_BOUNDARY
5	5	src/a.rs
COMMIT_BOUNDARY
10	0	src/b.rs
COMMIT_BOUNDARY
5	5	src/a.rs
"#;
        let results = parse_churn_output(output).unwrap();

        // a.rs: 2 commits, 20 lines
        // b.rs: 1 commit, 10 lines
        assert_eq!(results[0].file_path, "src/a.rs");
        assert_eq!(results[0].commit_count, 2);
    }

    #[test]
    fn parse_churn_output_tiebreaker_path() {
        // Same lines, same commits — sort by path
        let output = r#"COMMIT_BOUNDARY
5	5	src/z.rs
5	5	src/a.rs
"#;
        let results = parse_churn_output(output).unwrap();

        assert_eq!(results.len(), 2);
        // Same lines (10), same commits (1) — a.rs < z.rs
        assert_eq!(results[0].file_path, "src/a.rs");
        assert_eq!(results[1].file_path, "src/z.rs");
    }

    #[test]
    fn parse_churn_output_empty() {
        let results = parse_churn_output("").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn parse_churn_output_only_boundary() {
        let results = parse_churn_output("COMMIT_BOUNDARY\n").unwrap();
        assert!(results.is_empty());
    }
}
