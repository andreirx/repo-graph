//! Presentation layer for the `hotspots` command.
//!
//! # CLI-OUT-6 Group 1
//!
//! Transforms hotspot query results into human-readable plain text.
//! Shows file-level hotspot scores (churn x complexity) within a time window.
//!
//! Hotspots extend churn with:
//! - Formula metadata
//! - Optional filtering section (test/vendored exclusions)
//! - Complexity and hotspot score columns
//!
//! # Output Contract
//!
//! - Deterministic ordering (by hotspot_score desc, then path asc)
//! - Human render is BUDGETED (QUANT-MECH-1 §2.2): top [`HUMAN_ROW_BUDGET`] rows +
//!   an explicit `(+N more — --full)` remainder line; `--full` renders every row.
//!   `--json` is always the COMPLETE set (budgets are human-render only).

use repo_graph_classification::measurement_coverage::MeasurementCoverageBlock;
use serde::Deserialize;

use crate::presentation::{budget_remainder_line, HUMAN_ROW_BUDGET};

// ── Response Types ───────────────────────────────────────────────────────────

/// Response DTO for `hotspots` command.
///
/// Envelope fields from `build_envelope` plus command-specific fields.
#[derive(Debug, Deserialize)]
pub struct HotspotsResponse {
    #[allow(dead_code)]
    pub command: String,
    pub repo: String,
    #[allow(dead_code)]
    pub snapshot: String,
    pub since: String,
    pub formula: String,
    pub filtering: Option<HotspotsFiltering>,
    pub results: Vec<HotspotEntry>,
    pub count: usize,
    /// METRIC-LANG-COVERAGE-1 (part A): per-language complexity measurement coverage.
    /// The hotspot score is churn × complexity, so an unmeasured language scores 0 and
    /// drops out of the ranking; the block's honesty line (`caveat_line`) states that
    /// omission (or that coverage could not be read). ALWAYS present on the wire (the
    /// daemon attaches an `available`/`unavailable` block unconditionally); the `Option`
    /// is only `#[serde(default)]` robustness for an older/absent field.
    #[serde(default)]
    pub measurement_coverage: Option<MeasurementCoverageBlock>,
}

/// Filtering metadata when exclusion flags are active.
#[derive(Debug, Deserialize, Clone)]
pub struct HotspotsFiltering {
    pub exclude_tests: bool,
    pub exclude_vendored: bool,
    pub excluded_count: usize,
    pub excluded_tests_count: usize,
    pub excluded_vendored_count: usize,
}

/// Individual hotspot entry.
#[derive(Debug, Deserialize, Clone)]
pub struct HotspotEntry {
    pub file_path: String,
    pub commit_count: u64,
    pub lines_changed: u64,
    pub sum_complexity: u64,
    pub hotspot_score: u64,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl HotspotsResponse {
    /// Render the hotspots response as human-readable plain text.
    ///
    /// QUANT-MECH-1 §2.2: BUDGETED — shows the top [`HUMAN_ROW_BUDGET`] rows then an
    /// explicit `(+N more — --full)` remainder line. `full == true` (from `--full`)
    /// renders every row uncapped. The COMPLETE set always rides `hotspots --json`.
    pub fn render_human(&self, full: bool) -> String {
        let mut out = String::new();

        // Header with time window
        out.push_str(&format!("Hotspots ({})\n\n", format_since(&self.since)));

        // Formula
        out.push_str(&format!("Formula: {}\n\n", self.formula));

        // Count line
        let file_word = if self.count == 1 { "file" } else { "files" };
        out.push_str(&format!("{} {} scored\n", self.count, file_word));

        // METRIC-LANG-COVERAGE-1 (part A): state which languages are unmeasured — they
        // contribute complexity 0 and drop out of the churn×complexity ranking, so the
        // score is not repo-wide (or state that coverage could not be read). Reader-frame
        // wording from `classification`; it disappears by itself once every significant
        // language is measured. Shown even at 0 files scored (an unmeasured language can be
        // *why* the list is empty).
        if let Some(line) = self
            .measurement_coverage
            .as_ref()
            .and_then(|b| b.caveat_line())
        {
            out.push_str(&format!("\n{}\n", line));
        }

        // Filtering section (only if active)
        if let Some(ref f) = self.filtering {
            out.push_str("\nFiltering:\n");
            if f.exclude_tests {
                let word = if f.excluded_tests_count == 1 {
                    "file"
                } else {
                    "files"
                };
                out.push_str(&format!(
                    "  excluded {} test {}\n",
                    f.excluded_tests_count, word
                ));
            }
            if f.exclude_vendored {
                let word = if f.excluded_vendored_count == 1 {
                    "file"
                } else {
                    "files"
                };
                out.push_str(&format!(
                    "  excluded {} vendored {}\n",
                    f.excluded_vendored_count, word
                ));
            }
        }

        if self.count == 0 {
            out.push_str(&format!(
                "\nhint: no hotspots found in the {} window.\n",
                format_since(&self.since)
            ));
            out.push_str("      This may mean no files have both churn and complexity data.\n");
            return out;
        }

        // Sort by hotspot_score desc, then path asc for determinism
        let mut entries = self.results.clone();
        entries.sort_by(|a, b| {
            b.hotspot_score
                .cmp(&a.hotspot_score)
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
        let max_score = shown.iter().map(|e| e.hotspot_score).max().unwrap_or(0);
        let max_churn = shown.iter().map(|e| e.lines_changed).max().unwrap_or(0);
        let max_complexity = shown.iter().map(|e| e.sum_complexity).max().unwrap_or(0);

        let score_width = format!("{}", max_score).len().max(5); // "Score"
        let churn_width = format!("{}", max_churn).len().max(5); // "Churn"
        let complexity_width = format!("{}", max_complexity).len().max(10); // "Complexity"

        // Table header
        out.push_str(&format!(
            "\n  {:>score_width$}  {:>churn_width$}  {:>complexity_width$}  File\n",
            "Score",
            "Churn",
            "Complexity",
            score_width = score_width,
            churn_width = churn_width,
            complexity_width = complexity_width,
        ));

        // Table rows
        for entry in shown {
            out.push_str(&format!(
                "  {:>score_width$}  {:>churn_width$}  {:>complexity_width$}  {}\n",
                entry.hotspot_score,
                entry.lines_changed,
                entry.sum_complexity,
                entry.file_path,
                score_width = score_width,
                churn_width = churn_width,
                complexity_width = complexity_width,
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

    fn make_response(
        entries: Vec<HotspotEntry>,
        since: &str,
        filtering: Option<HotspotsFiltering>,
    ) -> HotspotsResponse {
        HotspotsResponse {
            command: "hotspots".to_string(),
            repo: "test-repo".to_string(),
            snapshot: "snap_123".to_string(),
            since: since.to_string(),
            formula: "lines_changed * sum_complexity".to_string(),
            filtering,
            results: entries.clone(),
            count: entries.len(),
            measurement_coverage: None,
        }
    }

    fn make_entry(path: &str, churn: u64, complexity: u64, score: u64) -> HotspotEntry {
        HotspotEntry {
            file_path: path.to_string(),
            commit_count: 5, // Not displayed in table
            lines_changed: churn,
            sum_complexity: complexity,
            hotspot_score: score,
        }
    }

    #[test]
    fn render_empty_hotspots() {
        let resp = make_response(vec![], "90.days.ago", None);
        let out = resp.render_human(false);

        assert!(out.contains("Hotspots (last 90 days)"));
        assert!(out.contains("Formula: lines_changed * sum_complexity"));
        assert!(out.contains("0 files scored"));
        assert!(out.contains("hint:"));
    }

    #[test]
    fn render_single_hotspot() {
        let resp = make_response(
            vec![make_entry("src/main.rs", 100, 50, 5000)],
            "30.days.ago",
            None,
        );
        let out = resp.render_human(false);

        assert!(out.contains("Hotspots (last 30 days)"));
        assert!(out.contains("1 file scored"));
        assert!(out.contains("src/main.rs"));
        assert!(out.contains("5000")); // score
        assert!(out.contains("100")); // churn
        assert!(out.contains("50")); // complexity
    }

    #[test]
    fn render_multiple_hotspots_sorted_by_score() {
        let resp = make_response(
            vec![
                make_entry("src/low.rs", 10, 10, 100),
                make_entry("src/high.rs", 500, 100, 50000),
                make_entry("src/mid.rs", 200, 50, 10000),
            ],
            "90.days.ago",
            None,
        );
        let out = resp.render_human(false);

        assert!(out.contains("3 files scored"));

        // Verify ordering: high (50000) first, then mid (10000), then low (100)
        let high_pos = out.find("src/high.rs").unwrap();
        let mid_pos = out.find("src/mid.rs").unwrap();
        let low_pos = out.find("src/low.rs").unwrap();

        assert!(high_pos < mid_pos, "high should come before mid");
        assert!(mid_pos < low_pos, "mid should come before low");
    }

    #[test]
    fn render_with_filtering_tests_only() {
        let filtering = HotspotsFiltering {
            exclude_tests: true,
            exclude_vendored: false,
            excluded_count: 5,
            excluded_tests_count: 5,
            excluded_vendored_count: 0,
        };
        let resp = make_response(
            vec![make_entry("src/main.rs", 100, 50, 5000)],
            "90.days.ago",
            Some(filtering),
        );
        let out = resp.render_human(false);

        assert!(out.contains("Filtering:"));
        assert!(out.contains("excluded 5 test files"));
        assert!(!out.contains("vendored"));
    }

    #[test]
    fn render_with_filtering_both() {
        let filtering = HotspotsFiltering {
            exclude_tests: true,
            exclude_vendored: true,
            excluded_count: 8,
            excluded_tests_count: 5,
            excluded_vendored_count: 3,
        };
        let resp = make_response(
            vec![make_entry("src/main.rs", 100, 50, 5000)],
            "90.days.ago",
            Some(filtering),
        );
        let out = resp.render_human(false);

        assert!(out.contains("Filtering:"));
        assert!(out.contains("excluded 5 test files"));
        assert!(out.contains("excluded 3 vendored files"));
    }

    #[test]
    fn render_with_filtering_single_file_grammar() {
        let filtering = HotspotsFiltering {
            exclude_tests: true,
            exclude_vendored: true,
            excluded_count: 2,
            excluded_tests_count: 1,
            excluded_vendored_count: 1,
        };
        let resp = make_response(vec![], "90.days.ago", Some(filtering));
        let out = resp.render_human(false);

        assert!(out.contains("excluded 1 test file"));
        assert!(out.contains("excluded 1 vendored file"));
    }

    #[test]
    fn table_header_present() {
        let resp = make_response(
            vec![make_entry("src/main.rs", 100, 50, 5000)],
            "90.days.ago",
            None,
        );
        let out = resp.render_human(false);

        assert!(out.contains("Score"));
        assert!(out.contains("Churn"));
        assert!(out.contains("Complexity"));
        assert!(out.contains("File"));
    }

    #[test]
    fn columns_align_with_varying_widths() {
        let resp = make_response(
            vec![
                make_entry("x.rs", 1, 1, 1),
                make_entry("long/path/file.rs", 99999, 99999, 9999999999),
            ],
            "90.days.ago",
            None,
        );
        let out = resp.render_human(false);

        // Both rows present
        assert!(out.contains("x.rs"));
        assert!(out.contains("long/path/file.rs"));
        // Large numbers present
        assert!(out.contains("9999999999"));
    }

    // ── QUANT-MECH-1 §2.2: budget + explicit remainder + --full ──

    #[test]
    fn render_budgets_default_with_explicit_remainder() {
        let n = HUMAN_ROW_BUDGET + 5;
        // Descending scores so h000 ranks first, h{n-1} last.
        let entries: Vec<HotspotEntry> = (0..n)
            .map(|i| make_entry(&format!("h{i:03}.rs"), 10, 10, (n - i) as u64 * 100))
            .collect();
        let resp = make_response(entries, "90.days.ago", None);
        let out = resp.render_human(false);

        assert!(out.contains(&format!("{n} files scored")), "{out}");
        assert!(out.contains("h000.rs"), "top row shown:\n{out}");
        assert!(
            !out.contains(&format!("h{HUMAN_ROW_BUDGET:03}.rs")),
            "over-budget row omitted:\n{out}"
        );
        assert!(out.contains("(+5 more — --full)"), "{out}");
    }

    #[test]
    fn render_full_uncaps_and_has_no_remainder() {
        let n = HUMAN_ROW_BUDGET + 5;
        let entries: Vec<HotspotEntry> = (0..n)
            .map(|i| make_entry(&format!("h{i:03}.rs"), 10, 10, (n - i) as u64 * 100))
            .collect();
        let resp = make_response(entries, "90.days.ago", None);
        let out = resp.render_human(true);

        assert!(
            out.contains(&format!("h{:03}.rs", n - 1)),
            "last row present under --full:\n{out}"
        );
        assert!(
            !out.contains("more — --full"),
            "no remainder under --full:\n{out}"
        );
    }

    // ── METRIC-LANG-COVERAGE-1 (part A): coverage caveat rendering ──

    use repo_graph_classification::measurement_coverage::{
        LanguageFunctionCount, MeasurementCoverageBlock,
    };

    fn count(lang: &str, functions: u64, measured: u64) -> LanguageFunctionCount {
        LanguageFunctionCount {
            language: lang.to_string(),
            function_count: functions,
            measured_count: measured,
        }
    }

    #[test]
    fn render_measurement_coverage_caveat_when_language_unmeasured() {
        let mut resp = make_response(
            vec![make_entry("src/a.ts", 100, 50, 5000)],
            "90.days.ago",
            None,
        );
        // Rust unmeasured (72%), TS measured — the repo-graph shape.
        resp.measurement_coverage = Some(MeasurementCoverageBlock::from_counts(vec![
            count("rust", 72, 0),
            count("typescript", 28, 28),
        ]));
        let out = resp.render_human(false);
        assert!(out.contains("Rust (72% of functions)"), "{out}");
        assert!(out.contains("not yet measured"), "{out}");
        assert!(out.contains("rankings omit it"), "{out}");
    }

    #[test]
    fn render_no_caveat_when_coverage_complete() {
        let mut resp = make_response(
            vec![make_entry("src/a.rs", 100, 50, 5000)],
            "90.days.ago",
            None,
        );
        // Everything measured → caveat is None → nothing rendered.
        resp.measurement_coverage = Some(MeasurementCoverageBlock::from_counts(vec![
            count("rust", 72, 72),
            count("typescript", 28, 28),
        ]));
        let out = resp.render_human(false);
        assert!(!out.contains("not yet measured"), "{out}");
    }

    #[test]
    fn render_unavailable_coverage_states_it_explicitly() {
        // review-6 item 2 (human surface): when coverage could not be read, the ranking
        // must SAY SO — never render as if coverage were complete.
        let mut resp = make_response(
            vec![make_entry("src/a.rs", 100, 50, 5000)],
            "90.days.ago",
            None,
        );
        resp.measurement_coverage = Some(MeasurementCoverageBlock::unavailable());
        let out = resp.render_human(false);
        assert!(
            out.contains("could not be read"),
            "unavailable coverage must be stated on the hotspots surface: {out}"
        );
    }
}
