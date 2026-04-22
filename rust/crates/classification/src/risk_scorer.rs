//! Pure risk scorer.
//!
//! RS-MS-4: Computes risk scores from hotspot and coverage inputs.
//!
//! Contract:
//!   - `risk_score = hotspot_score * (1 - line_coverage)`
//!   - Only files with BOTH hotspot AND coverage are included
//!   - Files with hotspot but no coverage: EXCLUDED (not degraded)
//!   - Files with coverage but no hotspot: EXCLUDED (not hot anyway)
//!   - Sorted: `risk_score desc` → `hotspot_score desc` → `file_path asc`
//!
//! Critical design decision:
//!   - Missing coverage is NOT treated as zero coverage
//!   - This prevents silent degradation of risk = hotspot when coverage is absent
//!   - An AI agent must distinguish "high risk" from "risk unknown"
//!
//! This is a pure function with no I/O. Callers provide the inputs;
//! this module computes the joined, scored, sorted result.

use crate::hotspot_scorer::HotspotEntry;
use std::collections::HashMap;

/// Coverage input for a single file.
#[derive(Debug, Clone)]
pub struct CoverageInput {
    pub file_path: String,
    /// Line coverage ratio (0.0 to 1.0).
    pub line_coverage: f64,
}

/// Risk result for a single file.
#[derive(Debug, Clone, PartialEq)]
pub struct RiskEntry {
    pub file_path: String,
    /// risk_score = hotspot_score * (1 - line_coverage)
    pub risk_score: f64,
    pub hotspot_score: u64,
    pub line_coverage: f64,
    pub lines_changed: u64,
    pub sum_complexity: u64,
}

/// Compute risk scores from hotspot and coverage inputs.
///
/// Only files present in BOTH inputs are included in the result.
/// Files with hotspot but no coverage are EXCLUDED (not degraded to risk = hotspot).
///
/// Formula: `risk_score = hotspot_score * (1 - line_coverage)`
///
/// Result is sorted by:
///   1. `risk_score` descending
///   2. `hotspot_score` descending
///   3. `file_path` ascending
pub fn compute_risk(
    hotspots: &[HotspotEntry],
    coverage: &[CoverageInput],
) -> Vec<RiskEntry> {
    // Build coverage lookup by file path
    let coverage_map: HashMap<&str, f64> = coverage
        .iter()
        .map(|c| (c.file_path.as_str(), c.line_coverage))
        .collect();

    // Join hotspots with coverage, compute risk scores
    let mut results: Vec<RiskEntry> = hotspots
        .iter()
        .filter_map(|h| {
            coverage_map.get(h.file_path.as_str()).map(|&line_coverage| {
                let coverage_gap = 1.0 - line_coverage;
                let risk_score = (h.hotspot_score as f64) * coverage_gap;
                RiskEntry {
                    file_path: h.file_path.clone(),
                    risk_score,
                    hotspot_score: h.hotspot_score,
                    line_coverage,
                    lines_changed: h.lines_changed,
                    sum_complexity: h.sum_complexity,
                }
            })
        })
        .collect();

    // Sort: risk_score desc, hotspot_score desc, file_path asc
    results.sort_by(|a, b| {
        // f64 comparison: use total_cmp for deterministic ordering
        b.risk_score
            .total_cmp(&a.risk_score)
            .then_with(|| b.hotspot_score.cmp(&a.hotspot_score))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hotspot(file_path: &str, lines_changed: u64, sum_complexity: u64) -> HotspotEntry {
        HotspotEntry {
            file_path: file_path.to_string(),
            commit_count: 1,
            lines_changed,
            sum_complexity,
            hotspot_score: lines_changed * sum_complexity,
        }
    }

    fn make_coverage(file_path: &str, line_coverage: f64) -> CoverageInput {
        CoverageInput {
            file_path: file_path.to_string(),
            line_coverage,
        }
    }

    #[test]
    fn compute_risk_basic() {
        let hotspots = vec![
            make_hotspot("src/a.ts", 100, 10), // hotspot_score = 1000
            make_hotspot("src/b.ts", 50, 20),  // hotspot_score = 1000
        ];

        let coverage = vec![
            make_coverage("src/a.ts", 0.8), // gap = 0.2, risk = 1000 * 0.2 = 200
            make_coverage("src/b.ts", 0.5), // gap = 0.5, risk = 1000 * 0.5 = 500
        ];

        let results = compute_risk(&hotspots, &coverage);

        assert_eq!(results.len(), 2);

        // b.ts has higher risk (500 > 200)
        assert_eq!(results[0].file_path, "src/b.ts");
        assert!((results[0].risk_score - 500.0).abs() < 0.001);
        assert_eq!(results[0].hotspot_score, 1000);
        assert!((results[0].line_coverage - 0.5).abs() < 0.001);

        assert_eq!(results[1].file_path, "src/a.ts");
        assert!((results[1].risk_score - 200.0).abs() < 0.001);
    }

    #[test]
    fn compute_risk_excludes_files_without_coverage() {
        let hotspots = vec![
            make_hotspot("src/a.ts", 100, 10),
            make_hotspot("src/no_coverage.ts", 200, 20), // High hotspot but no coverage
        ];

        let coverage = vec![make_coverage("src/a.ts", 0.5)];

        let results = compute_risk(&hotspots, &coverage);

        // Only a.ts is included - no_coverage.ts is EXCLUDED, not degraded
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/a.ts");
    }

    #[test]
    fn compute_risk_excludes_files_without_hotspot() {
        let hotspots = vec![make_hotspot("src/a.ts", 100, 10)];

        let coverage = vec![
            make_coverage("src/a.ts", 0.5),
            make_coverage("src/no_hotspot.ts", 0.1), // Low coverage but no hotspot
        ];

        let results = compute_risk(&hotspots, &coverage);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/a.ts");
    }

    #[test]
    fn compute_risk_zero_coverage_is_max_risk() {
        let hotspots = vec![make_hotspot("src/a.ts", 100, 10)]; // hotspot = 1000

        let coverage = vec![make_coverage("src/a.ts", 0.0)]; // coverage = 0%

        let results = compute_risk(&hotspots, &coverage);

        assert_eq!(results.len(), 1);
        // risk = 1000 * (1 - 0) = 1000
        assert!((results[0].risk_score - 1000.0).abs() < 0.001);
    }

    #[test]
    fn compute_risk_full_coverage_is_zero_risk() {
        let hotspots = vec![make_hotspot("src/a.ts", 100, 10)]; // hotspot = 1000

        let coverage = vec![make_coverage("src/a.ts", 1.0)]; // coverage = 100%

        let results = compute_risk(&hotspots, &coverage);

        assert_eq!(results.len(), 1);
        // risk = 1000 * (1 - 1) = 0
        assert!((results[0].risk_score - 0.0).abs() < 0.001);
    }

    #[test]
    fn compute_risk_sorted_by_score_desc() {
        let hotspots = vec![
            make_hotspot("low.ts", 10, 10),   // hotspot = 100
            make_hotspot("high.ts", 100, 10), // hotspot = 1000
        ];

        let coverage = vec![
            make_coverage("low.ts", 0.5),  // risk = 100 * 0.5 = 50
            make_coverage("high.ts", 0.5), // risk = 1000 * 0.5 = 500
        ];

        let results = compute_risk(&hotspots, &coverage);

        assert_eq!(results[0].file_path, "high.ts");
        assert_eq!(results[1].file_path, "low.ts");
    }

    #[test]
    fn compute_risk_tiebreaker_hotspot_score() {
        let hotspots = vec![
            make_hotspot("a.ts", 50, 20),  // hotspot = 1000
            make_hotspot("b.ts", 100, 10), // hotspot = 1000
        ];

        let coverage = vec![
            make_coverage("a.ts", 0.5), // risk = 1000 * 0.5 = 500
            make_coverage("b.ts", 0.5), // risk = 1000 * 0.5 = 500
        ];

        let results = compute_risk(&hotspots, &coverage);

        // Same risk, same hotspot_score, so file_path asc
        assert_eq!(results[0].file_path, "a.ts");
        assert_eq!(results[1].file_path, "b.ts");
    }

    #[test]
    fn compute_risk_tiebreaker_file_path() {
        let hotspots = vec![
            make_hotspot("z.ts", 100, 10),
            make_hotspot("a.ts", 100, 10),
        ];

        let coverage = vec![
            make_coverage("z.ts", 0.5),
            make_coverage("a.ts", 0.5),
        ];

        let results = compute_risk(&hotspots, &coverage);

        // Same risk, same hotspot_score, a.ts < z.ts
        assert_eq!(results[0].file_path, "a.ts");
        assert_eq!(results[1].file_path, "z.ts");
    }

    #[test]
    fn compute_risk_empty_inputs() {
        let results = compute_risk(&[], &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn compute_risk_preserves_fields() {
        let hotspots = vec![HotspotEntry {
            file_path: "src/a.ts".to_string(),
            commit_count: 42,
            lines_changed: 100,
            sum_complexity: 10,
            hotspot_score: 1000,
        }];

        let coverage = vec![make_coverage("src/a.ts", 0.75)];

        let results = compute_risk(&hotspots, &coverage);

        assert_eq!(results[0].lines_changed, 100);
        assert_eq!(results[0].sum_complexity, 10);
        assert_eq!(results[0].hotspot_score, 1000);
        assert!((results[0].line_coverage - 0.75).abs() < 0.001);
    }

    #[test]
    fn compute_risk_high_coverage_reduces_risk() {
        let hotspots = vec![
            make_hotspot("well_tested.ts", 100, 10), // hotspot = 1000
            make_hotspot("poorly_tested.ts", 50, 5), // hotspot = 250
        ];

        let coverage = vec![
            make_coverage("well_tested.ts", 0.95),  // risk = 1000 * 0.05 = 50
            make_coverage("poorly_tested.ts", 0.1), // risk = 250 * 0.9 = 225
        ];

        let results = compute_risk(&hotspots, &coverage);

        // Despite higher hotspot, well_tested.ts has lower risk due to coverage
        assert_eq!(results[0].file_path, "poorly_tested.ts");
        assert!((results[0].risk_score - 225.0).abs() < 0.001);
        assert_eq!(results[1].file_path, "well_tested.ts");
        assert!((results[1].risk_score - 50.0).abs() < 0.001);
    }
}
