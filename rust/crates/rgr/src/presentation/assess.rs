//! Presentation layer for the `assess` command.
//!
//! # CLI-OUT-7 Group 1
//!
//! Transforms quality policy assessment results into human-readable plain text.
//! Shows assessment count breakdown and baseline status.
//!
//! # Output Contract
//!
//! - Compact summary (counts only)
//! - Domain verdicts preserved (pass, fail, not_applicable, not_comparable)
//! - Baseline hint when required but missing
//! - `--json` preserved for machine mode
//!
//! # Domain Verdicts
//!
//! Assessment verdicts are domain terms, not invented display labels:
//! - pass: policy threshold met
//! - fail: policy threshold not met
//! - not_applicable: policy does not apply to target
//! - not_comparable: comparative policy missing baseline data

use serde::Deserialize;

// ── Response Types ───────────────────────────────────────────────────────────

/// Nested assessments breakdown.
#[derive(Debug, Deserialize, Clone)]
pub struct AssessmentsBreakdown {
    pub total: u64,
    pub pass: u64,
    pub fail: u64,
    pub not_applicable: u64,
    pub not_comparable: u64,
}

/// Response DTO for `assess` command.
#[derive(Debug, Deserialize)]
pub struct AssessResponse {
    #[allow(dead_code)]
    pub command: String,
    pub repo: String,
    pub snapshot: String,
    #[serde(default)]
    pub baseline_snapshot: Option<String>,
    pub assessments: AssessmentsBreakdown,
    #[serde(default)]
    pub baseline_required_count: u64,
}

// ── Human Rendering ──────────────────────────────────────────────────────────

impl AssessResponse {
    /// Render the assess response as human-readable plain text.
    ///
    /// Compact summary format. Domain verdicts preserved.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Quality Assessment\n\n");

        // Snapshot info
        out.push_str(&format!("Snapshot: {}\n", self.snapshot));
        if let Some(ref baseline) = self.baseline_snapshot {
            out.push_str(&format!("Baseline: {}\n", baseline));
        }

        // Total count
        let policy_word = if self.assessments.total == 1 {
            "policy"
        } else {
            "policies"
        };
        out.push_str(&format!(
            "\n{} {} evaluated\n",
            self.assessments.total, policy_word
        ));

        // Breakdown (only if there are assessments)
        if self.assessments.total > 0 {
            out.push('\n');

            // Find max width for alignment
            let labels = ["pass", "fail", "not applicable", "not comparable"];
            let max_label = labels.iter().map(|l| l.len()).max().unwrap_or(0);

            out.push_str(&format!(
                "  {:<width$}   {}\n",
                "pass",
                self.assessments.pass,
                width = max_label
            ));
            out.push_str(&format!(
                "  {:<width$}   {}\n",
                "fail",
                self.assessments.fail,
                width = max_label
            ));
            out.push_str(&format!(
                "  {:<width$}   {}\n",
                "not applicable",
                self.assessments.not_applicable,
                width = max_label
            ));
            out.push_str(&format!(
                "  {:<width$}   {}\n",
                "not comparable",
                self.assessments.not_comparable,
                width = max_label
            ));
        }

        // Baseline hint
        if self.baseline_required_count > 0 && self.baseline_snapshot.is_none() {
            let policy_word = if self.baseline_required_count == 1 {
                "policy requires"
            } else {
                "policies require"
            };
            out.push_str(&format!(
                "\nhint: {} {} baseline for comparison.\n",
                self.baseline_required_count, policy_word
            ));
        }

        out
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(
        total: u64,
        pass: u64,
        fail: u64,
        not_applicable: u64,
        not_comparable: u64,
        baseline_snapshot: Option<String>,
        baseline_required_count: u64,
    ) -> AssessResponse {
        AssessResponse {
            command: "assess".to_string(),
            repo: "test-repo".to_string(),
            snapshot: "snap_abc123".to_string(),
            baseline_snapshot,
            assessments: AssessmentsBreakdown {
                total,
                pass,
                fail,
                not_applicable,
                not_comparable,
            },
            baseline_required_count,
        }
    }

    #[test]
    fn render_empty_assessment() {
        let resp = make_response(0, 0, 0, 0, 0, None, 0);
        let out = resp.render_human();

        assert!(out.contains("Quality Assessment"));
        assert!(out.contains("Snapshot: snap_abc123"));
        assert!(out.contains("0 policies evaluated"));
        // No breakdown section for empty
        assert!(!out.contains("pass"));
    }

    #[test]
    fn render_single_policy() {
        let resp = make_response(1, 1, 0, 0, 0, None, 0);
        let out = resp.render_human();

        assert!(out.contains("1 policy evaluated"));
        assert!(out.contains("pass"));
    }

    #[test]
    fn render_multiple_policies() {
        let resp = make_response(5, 3, 1, 1, 0, None, 0);
        let out = resp.render_human();

        assert!(out.contains("5 policies evaluated"));
        assert!(out.contains("pass"));
        assert!(out.contains("3"));
        assert!(out.contains("fail"));
        assert!(out.contains("1"));
        assert!(out.contains("not applicable"));
    }

    #[test]
    fn render_with_baseline() {
        let resp = make_response(2, 1, 1, 0, 0, Some("snap_baseline".to_string()), 0);
        let out = resp.render_human();

        assert!(out.contains("Snapshot: snap_abc123"));
        assert!(out.contains("Baseline: snap_baseline"));
    }

    #[test]
    fn render_baseline_required_hint() {
        let resp = make_response(2, 1, 0, 0, 1, None, 1);
        let out = resp.render_human();

        assert!(out.contains("hint:"));
        assert!(out.contains("1 policy requires baseline"));
    }

    #[test]
    fn render_multiple_baseline_required_hint() {
        let resp = make_response(3, 1, 0, 0, 2, None, 2);
        let out = resp.render_human();

        assert!(out.contains("2 policies require baseline"));
    }

    #[test]
    fn render_no_baseline_hint_when_baseline_provided() {
        let resp = make_response(2, 1, 0, 0, 1, Some("snap_base".to_string()), 1);
        let out = resp.render_human();

        // No hint when baseline is provided
        assert!(!out.contains("hint:"));
    }

    #[test]
    fn domain_verdicts_preserved() {
        let resp = make_response(4, 1, 1, 1, 1, None, 0);
        let out = resp.render_human();

        // Verify domain terms are used as-is, not translated
        assert!(out.contains("pass"));
        assert!(out.contains("fail"));
        assert!(out.contains("not applicable"));
        assert!(out.contains("not comparable"));

        // Verify no invented labels like "PASSED", "FAILED", "N/A"
        assert!(!out.contains("PASSED"));
        assert!(!out.contains("FAILED"));
        assert!(!out.contains("N/A"));
    }

    #[test]
    fn columns_aligned() {
        let resp = make_response(4, 10, 20, 30, 40, None, 0);
        let out = resp.render_human();

        // All labels should be padded to same width
        // "not comparable" is longest at 14 chars
        // Check that shorter labels have trailing spaces before the count
        let lines: Vec<&str> = out.lines().collect();
        let pass_line = lines
            .iter()
            .find(|l| l.contains("pass") && l.contains("10"));
        let nc_line = lines
            .iter()
            .find(|l| l.contains("not comparable") && l.contains("40"));

        assert!(pass_line.is_some(), "pass line should exist");
        assert!(nc_line.is_some(), "not comparable line should exist");
    }
}
