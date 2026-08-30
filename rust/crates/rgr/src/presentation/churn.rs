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
//! - Deterministic ordering: commit_count DESC, lines_changed DESC, path ASC
//!   (QUANT-MECH-1 §2.1 — mirrors the git layer; commit count is the signal, line
//!   volume the tiebreaker). The JSON array follows this same order.
//! - Human render is BUDGETED (QUANT-MECH-1 §2.2): top [`HUMAN_ROW_BUDGET`] rows +
//!   an explicit `(+N more — --full)` remainder line; `--full` renders every row.
//!   `--json` is always the COMPLETE set (budgets are human-render only).

use serde::Deserialize;

use crate::presentation::{budget_remainder_line, HUMAN_ROW_BUDGET};

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
    /// QUANT-MECH-1 §2.2: BUDGETED — shows the top [`HUMAN_ROW_BUDGET`] rows then an
    /// explicit `(+N more — --full)` remainder line. `full == true` (from `--full`)
    /// renders every row uncapped. The COMPLETE set always rides `churn --json`.
    pub fn render_human(&self, full: bool) -> String {
        let mut out = String::new();

        // Header with time window
        out.push_str(&format!("File Churn ({})\n\n", format_since(&self.since)));

        // Count line — the COMPLETE count, never the displayed subset.
        let file_word = if self.count == 1 { "file" } else { "files" };
        out.push_str(&format!("{} {} changed\n", self.count, file_word));

        if self.count == 0 {
            out.push_str(&format!(
                "\nhint: no files changed in the {} window, or no git history available.\n",
                format_since(&self.since)
            ));
            return out;
        }

        // QUANT-MECH-1 §2.1: commit_count DESC (the signal), then lines_changed DESC
        // (tiebreaker), then path ASC (total order → deterministic). Mirrors the git
        // layer so the human render and the JSON array agree.
        let mut entries = self.results.clone();
        entries.sort_by(|a, b| {
            b.commit_count
                .cmp(&a.commit_count)
                .then_with(|| b.lines_changed.cmp(&a.lines_changed))
                .then_with(|| a.file_path.cmp(&b.file_path))
        });

        // QUANT-MECH-1 §2.2: bound to the budget unless `--full`.
        let cap = if full {
            entries.len()
        } else {
            HUMAN_ROW_BUDGET.min(entries.len())
        };
        let shown = &entries[..cap];

        // Compute column widths over the SHOWN rows (aligns to what is printed).
        let max_path_len = shown.iter().map(|e| e.file_path.len()).max().unwrap_or(0);
        let max_commits = shown.iter().map(|e| e.commit_count).max().unwrap_or(0);
        let max_lines = shown.iter().map(|e| e.lines_changed).max().unwrap_or(0);
        let commits_width = format!("{}", max_commits).len();
        let lines_width = format!("{}", max_lines).len();

        out.push('\n');
        for entry in shown {
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

        // Explicit remainder line (never silent) when the budget hid rows.
        if let Some(line) = budget_remainder_line(entries.len(), shown.len()) {
            out.push_str(&line);
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
        let out = resp.render_human(false);

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
        let out = resp.render_human(false);

        assert!(out.contains("File Churn (last 30 days)"));
        assert!(out.contains("1 file changed"));
        assert!(out.contains("src/main.rs"));
        assert!(out.contains("5 commits"));
        assert!(out.contains("120 lines"));
    }

    #[test]
    fn render_multiple_files_sorted_by_commits_then_lines() {
        // QUANT-MECH-1 §2.1: commit_count is the lead key, lines_changed the tiebreaker.
        // b has the most commits; c and a tie on commits? No — b(10) > c(3) > a(2), so
        // the order is b, c, a. Under the OLD lines-first sort a and c (both 50 lines)
        // would sort alphabetically (a before c); commit-first inverts that pair.
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
        let out = resp.render_human(false);

        assert!(out.contains("3 files changed"));

        let b_pos = out.find("src/b.rs").unwrap();
        let c_pos = out.find("src/c.rs").unwrap();
        let a_pos = out.find("src/a.rs").unwrap();

        assert!(b_pos < c_pos, "b (10 commits) before c (3 commits)");
        assert!(
            c_pos < a_pos,
            "c (3 commits) before a (2 commits) — commit count leads, not line volume"
        );
    }

    #[test]
    fn render_commit_leader_outranks_line_leader() {
        // QUANT-MECH-1 §2.1: the sustained-change file (many commits, few lines) ranks
        // above the one-shot bulk edit (one commit, huge line count).
        let resp = make_response(
            vec![
                ChurnEntry {
                    file_path: "big.rs".to_string(),
                    commit_count: 1,
                    lines_changed: 1000,
                },
                ChurnEntry {
                    file_path: "hot.rs".to_string(),
                    commit_count: 8,
                    lines_changed: 40,
                },
            ],
            "90.days.ago",
        );
        let out = resp.render_human(false);
        let hot = out.find("hot.rs").unwrap();
        let big = out.find("big.rs").unwrap();
        assert!(hot < big, "commit leader must rank first:\n{out}");
    }

    #[test]
    fn render_budgets_default_with_explicit_remainder() {
        // §2.2: default render shows the top HUMAN_ROW_BUDGET rows + an explicit
        // "(+N more — --full)" line; the count line stays the COMPLETE total.
        let n = HUMAN_ROW_BUDGET + 7;
        let entries: Vec<ChurnEntry> = (0..n)
            .map(|i| ChurnEntry {
                file_path: format!("f{i:03}.rs"),
                // Descending commits so f000 ranks first, f{n-1} last.
                commit_count: (n - i) as u64,
                lines_changed: 1,
            })
            .collect();
        let resp = make_response(entries, "90.days.ago");
        let out = resp.render_human(false);

        assert!(out.contains(&format!("{n} files changed")), "{out}");
        // Top row shown, first over-budget row omitted.
        assert!(out.contains("f000.rs"), "{out}");
        assert!(
            !out.contains(&format!("f{HUMAN_ROW_BUDGET:03}.rs")),
            "over-budget row must be omitted:\n{out}"
        );
        assert!(out.contains("(+7 more — --full)"), "{out}");
    }

    #[test]
    fn render_full_uncaps_and_has_no_remainder() {
        let n = HUMAN_ROW_BUDGET + 7;
        let entries: Vec<ChurnEntry> = (0..n)
            .map(|i| ChurnEntry {
                file_path: format!("f{i:03}.rs"),
                commit_count: (n - i) as u64,
                lines_changed: 1,
            })
            .collect();
        let resp = make_response(entries, "90.days.ago");
        let out = resp.render_human(true);

        // Every row present; no remainder line.
        assert!(
            out.contains(&format!("f{:03}.rs", n - 1)),
            "last row must appear under --full:\n{out}"
        );
        assert!(
            !out.contains("more — --full"),
            "no remainder under --full:\n{out}"
        );
    }

    #[test]
    fn render_no_remainder_when_within_budget() {
        let resp = make_response(
            vec![ChurnEntry {
                file_path: "only.rs".to_string(),
                commit_count: 1,
                lines_changed: 1,
            }],
            "90.days.ago",
        );
        let out = resp.render_human(false);
        assert!(!out.contains("more — --full"), "{out}");
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
        let out = resp.render_human(false);

        // Both rows should have aligned columns
        // The short path should be padded to match the long path
        assert!(out.contains("x.rs"));
        assert!(out.contains("very/long/path/to/file.rs"));
    }
}
