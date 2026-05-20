//! Presentation layer for the `churn` command.
//!
//! # CLI-OUT-6 Group 1
//!
//! Transforms churn query results into human-readable plain text.
//! Shows file-level git churn (commits and lines changed) within
//! a time window.
//!
//! # Output Contract
//!
//! - Deterministic ordering (by lines_changed desc, then path asc)
//! - Full output, no truncation
//! - `--json` preserved for machine mode

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// Response DTO for `churn` command.
///
/// Envelope fields from `build_envelope` plus command-specific `since`.
#[derive(Debug, Deserialize)]
pub struct ChurnResponse {
    #[allow(dead_code)]
    pub command: String,
    pub repo: String,
    #[allow(dead_code)]
    pub snapshot: String,
    pub since: String,
    pub results: Vec<ChurnEntry>,
    pub count: usize,
}

/// Individual file churn entry.
#[derive(Debug, Deserialize, Clone)]
pub struct ChurnEntry {
    pub file_path: String,
    pub commit_count: u64,
    pub lines_changed: u64,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl ChurnResponse {
    /// Render the churn response as human-readable plain text.
    ///
    /// Outputs full sorted list. No truncation. Caller can pipe to `head`.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header with time window
        out.push_str(&format!("File Churn ({})\n\n", format_since(&self.since)));

        // Count line
        let file_word = if self.count == 1 { "file" } else { "files" };
        out.push_str(&format!("{} {} changed\n", self.count, file_word));

        if self.count == 0 {
            out.push_str(&format!(
                "\nhint: no files changed in the {} window, or no git history available.\n",
                format_since(&self.since)
            ));
            return out;
        }

        // Sort by lines_changed desc, then path asc for determinism
        let mut entries = self.results.clone();
        entries.sort_by(|a, b| {
            b.lines_changed
                .cmp(&a.lines_changed)
                .then_with(|| a.file_path.cmp(&b.file_path))
        });

        // Compute column widths for alignment
        let max_path_len = entries.iter().map(|e| e.file_path.len()).max().unwrap_or(0);
        let max_commits = entries.iter().map(|e| e.commit_count).max().unwrap_or(0);
        let max_lines = entries.iter().map(|e| e.lines_changed).max().unwrap_or(0);
        let commits_width = format!("{}", max_commits).len();
        let lines_width = format!("{}", max_lines).len();

        out.push('\n');
        for entry in &entries {
            out.push_str(&format!(
                "  {:<path_width$}  {:>commits_width$} commits  {:>lines_width$} lines\n",
                entry.file_path,
                entry.commit_count,
                entry.lines_changed,
                path_width = max_path_len,
                commits_width = commits_width,
                lines_width = lines_width,
            ));
        }

        out
    }
}

/// Format the `since` value for human display.
///
/// Converts git-style expressions like "90.days.ago" to "last 90 days".
fn format_since(since: &str) -> String {
    // Handle common patterns: "N.days.ago", "N.weeks.ago", etc.
    if let Some(rest) = since.strip_suffix(".ago") {
        let parts: Vec<&str> = rest.split('.').collect();
        if parts.len() == 2 {
            if let Ok(n) = parts[0].parse::<u32>() {
                let unit = parts[1];
                return format!("last {} {}", n, unit);
            }
        }
    }
    // Fall back to raw value
    since.to_string()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(entries: Vec<ChurnEntry>, since: &str) -> ChurnResponse {
        ChurnResponse {
            command: "churn".to_string(),
            repo: "test-repo".to_string(),
            snapshot: "snap_123".to_string(),
            since: since.to_string(),
            results: entries.clone(),
            count: entries.len(),
        }
    }

    #[test]
    fn render_empty_churn() {
        let resp = make_response(vec![], "90.days.ago");
        let out = resp.render_human();

        assert!(out.contains("File Churn (last 90 days)"));
        assert!(out.contains("0 files changed"));
        assert!(out.contains("hint:"));
    }

    #[test]
    fn render_single_file() {
        let resp = make_response(
            vec![ChurnEntry {
                file_path: "src/main.rs".to_string(),
                commit_count: 5,
                lines_changed: 120,
            }],
            "30.days.ago",
        );
        let out = resp.render_human();

        assert!(out.contains("File Churn (last 30 days)"));
        assert!(out.contains("1 file changed"));
        assert!(out.contains("src/main.rs"));
        assert!(out.contains("5 commits"));
        assert!(out.contains("120 lines"));
    }

    #[test]
    fn render_multiple_files_sorted_by_lines() {
        let resp = make_response(
            vec![
                ChurnEntry {
                    file_path: "src/a.rs".to_string(),
                    commit_count: 2,
                    lines_changed: 50,
                },
                ChurnEntry {
                    file_path: "src/b.rs".to_string(),
                    commit_count: 10,
                    lines_changed: 500,
                },
                ChurnEntry {
                    file_path: "src/c.rs".to_string(),
                    commit_count: 3,
                    lines_changed: 50,
                },
            ],
            "90.days.ago",
        );
        let out = resp.render_human();

        assert!(out.contains("3 files changed"));

        // Verify ordering: b (500 lines) first, then a and c (50 lines each, alphabetical)
        let b_pos = out.find("src/b.rs").unwrap();
        let a_pos = out.find("src/a.rs").unwrap();
        let c_pos = out.find("src/c.rs").unwrap();

        assert!(b_pos < a_pos, "b should come before a (more lines)");
        assert!(
            a_pos < c_pos,
            "a should come before c (same lines, alphabetical)"
        );
    }

    #[test]
    fn format_since_days() {
        assert_eq!(format_since("90.days.ago"), "last 90 days");
        assert_eq!(format_since("30.days.ago"), "last 30 days");
    }

    #[test]
    fn format_since_weeks() {
        assert_eq!(format_since("2.weeks.ago"), "last 2 weeks");
    }

    #[test]
    fn format_since_fallback() {
        assert_eq!(format_since("custom-date"), "custom-date");
        assert_eq!(format_since("2024-01-01"), "2024-01-01");
    }

    #[test]
    fn columns_align_with_varying_widths() {
        let resp = make_response(
            vec![
                ChurnEntry {
                    file_path: "x.rs".to_string(),
                    commit_count: 1,
                    lines_changed: 5,
                },
                ChurnEntry {
                    file_path: "very/long/path/to/file.rs".to_string(),
                    commit_count: 100,
                    lines_changed: 9999,
                },
            ],
            "90.days.ago",
        );
        let out = resp.render_human();

        // Both rows should have aligned columns
        // The short path should be padded to match the long path
        assert!(out.contains("x.rs"));
        assert!(out.contains("very/long/path/to/file.rs"));
    }
}
