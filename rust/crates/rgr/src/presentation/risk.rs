//! Presentation layer for the `risk` command.
//!
//! # CLI-OUT-6 Group 2
//!
//! Transforms risk query results into human-readable plain text.
//! Shows file-level risk scores (hotspot × coverage gap) within a time window.
//!
//! Risk is a derived surface that joins hotspot data with coverage data.
//! Only files with BOTH are included. The join metadata shows how many
//! files had each data source and how many were successfully joined.
//!
//! # Output Contract
//!
//! - Deterministic ordering (by risk_score desc, then path asc)
//! - Full output, no truncation
//! - `--json` preserved for machine mode
//! - **No invented verdict labels** (no CRITICAL/HIGH/MEDIUM/LOW)
//! - Evidence-bearing and rank-oriented, not policy-theatrical

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// Response DTO for `risk` command.
///
/// Envelope fields from `build_envelope` plus command-specific fields.
#[derive(Debug, Deserialize)]
pub struct RiskResponse {
    #[allow(dead_code)]
    pub command: String,
    pub repo: String,
    #[allow(dead_code)]
    pub snapshot: String,
    pub since: String,
    pub formula: String,
    pub hotspot_files: usize,
    pub coverage_files: usize,
    pub joined_files: usize,
    pub results: Vec<RiskEntry>,
    pub count: usize,
}

/// Individual risk entry.
#[derive(Debug, Deserialize, Clone)]
pub struct RiskEntry {
    pub file_path: String,
    pub risk_score: f64,
    pub hotspot_score: u64,
    pub line_coverage: f64,
    #[allow(dead_code)]
    pub lines_changed: u64,
    #[allow(dead_code)]
    pub sum_complexity: u64,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl RiskResponse {
    /// Render the risk response as human-readable plain text.
    ///
    /// Outputs full sorted list. No truncation. Caller can pipe to `head`.
    /// No verdict labels — numbers speak for themselves.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header with time window
        out.push_str(&format!(
            "Risk Analysis ({})\n\n",
            format_since(&self.since)
        ));

        // Formula
        out.push_str(&format!("Formula: {}\n\n", self.formula));

        // Join coverage metadata
        out.push_str("Join coverage:\n");
        out.push_str(&format!(
            "  {} files with hotspot data\n",
            self.hotspot_files
        ));
        out.push_str(&format!(
            "  {} files with coverage data\n",
            self.coverage_files
        ));

        let shown_word = if self.joined_files == 1 {
            "file"
        } else {
            "files"
        };
        out.push_str(&format!(
            "  {} {} with both (shown below)\n",
            self.joined_files, shown_word
        ));

        if self.count == 0 {
            out.push_str("\nhint: no files have both hotspot and coverage data.\n");
            if self.hotspot_files > 0 && self.coverage_files == 0 {
                out.push_str(
                    "      Import coverage data with 'rmap coverage <db> <repo> <report>'.\n",
                );
            } else if self.hotspot_files == 0 {
                out.push_str("      No hotspot data available. Check churn and complexity.\n");
            }
            return out;
        }

        // Sort by risk_score desc, then path asc for determinism
        let mut entries = self.results.clone();
        entries.sort_by(|a, b| {
            b.risk_score
                .partial_cmp(&a.risk_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.file_path.cmp(&b.file_path))
        });

        // Compute column widths for alignment
        let max_risk = entries
            .iter()
            .map(|e| format!("{:.1}", e.risk_score).len())
            .max()
            .unwrap_or(4);
        let max_hotspot = entries.iter().map(|e| e.hotspot_score).max().unwrap_or(0);
        let hotspot_width = format!("{}", max_hotspot).len().max(7); // "Hotspot"

        let risk_width = max_risk.max(4); // "Risk"

        // Table header
        out.push_str(&format!(
            "\n  {:>risk_width$}  {:>hotspot_width$}  Coverage  File\n",
            "Risk",
            "Hotspot",
            risk_width = risk_width,
            hotspot_width = hotspot_width,
        ));

        // Table rows
        for entry in &entries {
            let coverage_pct = format!("{:.1}%", entry.line_coverage * 100.0);
            out.push_str(&format!(
                "  {:>risk_width$.1}  {:>hotspot_width$}  {:>8}  {}\n",
                entry.risk_score,
                entry.hotspot_score,
                coverage_pct,
                entry.file_path,
                risk_width = risk_width,
                hotspot_width = hotspot_width,
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

    fn make_response(
        entries: Vec<RiskEntry>,
        hotspot_files: usize,
        coverage_files: usize,
    ) -> RiskResponse {
        RiskResponse {
            command: "risk".to_string(),
            repo: "test-repo".to_string(),
            snapshot: "snap_123".to_string(),
            since: "90.days.ago".to_string(),
            formula: "hotspot_score * (1 - line_coverage)".to_string(),
            hotspot_files,
            coverage_files,
            joined_files: entries.len(),
            results: entries.clone(),
            count: entries.len(),
        }
    }

    fn make_entry(path: &str, risk: f64, hotspot: u64, coverage: f64) -> RiskEntry {
        RiskEntry {
            file_path: path.to_string(),
            risk_score: risk,
            hotspot_score: hotspot,
            line_coverage: coverage,
            lines_changed: 100,
            sum_complexity: 50,
        }
    }

    #[test]
    fn render_empty_risk_no_coverage() {
        let resp = make_response(vec![], 50, 0);
        let out = resp.render_human();

        assert!(out.contains("Risk Analysis (last 90 days)"));
        assert!(out.contains("Formula: hotspot_score * (1 - line_coverage)"));
        assert!(out.contains("50 files with hotspot data"));
        assert!(out.contains("0 files with coverage data"));
        assert!(out.contains("0 files with both"));
        assert!(out.contains("hint:"));
        assert!(out.contains("Import coverage data"));
    }

    #[test]
    fn render_empty_risk_no_hotspots() {
        let resp = make_response(vec![], 0, 30);
        let out = resp.render_human();

        assert!(out.contains("0 files with hotspot data"));
        assert!(out.contains("30 files with coverage data"));
        assert!(out.contains("No hotspot data"));
    }

    #[test]
    fn render_single_risk() {
        let resp = make_response(
            vec![make_entry("src/main.rs", 15000.0, 50000, 0.7)],
            100,
            50,
        );
        let out = resp.render_human();

        assert!(out.contains("100 files with hotspot data"));
        assert!(out.contains("50 files with coverage data"));
        assert!(out.contains("1 file with both"));
        assert!(out.contains("src/main.rs"));
        assert!(out.contains("15000.0"));
        assert!(out.contains("50000"));
        assert!(out.contains("70.0%"));
    }

    #[test]
    fn render_multiple_risks_sorted_by_score() {
        let resp = make_response(
            vec![
                make_entry("src/low.rs", 1000.0, 10000, 0.9),
                make_entry("src/high.rs", 50000.0, 100000, 0.5),
                make_entry("src/mid.rs", 20000.0, 50000, 0.6),
            ],
            150,
            80,
        );
        let out = resp.render_human();

        assert!(out.contains("3 files with both"));

        // Verify ordering: high (50000) first, then mid (20000), then low (1000)
        let high_pos = out.find("src/high.rs").unwrap();
        let mid_pos = out.find("src/mid.rs").unwrap();
        let low_pos = out.find("src/low.rs").unwrap();

        assert!(high_pos < mid_pos, "high should come before mid");
        assert!(mid_pos < low_pos, "mid should come before low");
    }

    #[test]
    fn table_header_present() {
        let resp = make_response(
            vec![make_entry("src/main.rs", 15000.0, 50000, 0.7)],
            100,
            50,
        );
        let out = resp.render_human();

        assert!(out.contains("Risk"));
        assert!(out.contains("Hotspot"));
        assert!(out.contains("Coverage"));
        assert!(out.contains("File"));
    }

    #[test]
    fn coverage_displayed_as_percentage() {
        let resp = make_response(
            vec![make_entry("src/main.rs", 15000.0, 50000, 0.85)],
            100,
            50,
        );
        let out = resp.render_human();

        assert!(out.contains("85.0%"));
    }

    #[test]
    fn no_verdict_labels_in_output() {
        let resp = make_response(
            vec![
                make_entry("src/top_risk.rs", 999999.0, 1000000, 0.0),
                make_entry("src/bottom_risk.rs", 1.0, 100, 0.99),
            ],
            100,
            50,
        );
        let out = resp.render_human();

        // Explicitly verify no verdict language (excluding file paths)
        // Split by lines and check non-path content
        let verdict_words = [
            "critical",
            "high risk",
            "medium risk",
            "low risk",
            "danger",
            "severe",
        ];
        for word in verdict_words {
            assert!(
                !out.to_lowercase().contains(word),
                "output should not contain verdict word '{}', got:\n{}",
                word,
                out
            );
        }
    }
}
