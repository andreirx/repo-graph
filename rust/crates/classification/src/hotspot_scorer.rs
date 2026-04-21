//! Pure hotspot scorer.
//!
//! RS-MS-3a: Computes hotspot scores from churn and complexity inputs.
//!
//! Contract:
//!   - `hotspot_score = lines_changed * sum_complexity`
//!   - Raw multiplication, no normalization
//!   - Only files with BOTH churn AND complexity are included
//!   - Files with churn but no complexity: excluded
//!   - Sorted: `hotspot_score desc` → `lines_changed desc` → `file_path asc`
//!
//! This is a pure function with no I/O. Callers provide the inputs;
//! this module computes the joined, scored, sorted result.

use std::collections::HashMap;

/// Churn input for a single file.
#[derive(Debug, Clone)]
pub struct ChurnInput {
    pub file_path: String,
    pub commit_count: u64,
    pub lines_changed: u64,
}

/// Complexity input for a single file (pre-summed).
#[derive(Debug, Clone)]
pub struct ComplexityInput {
    pub file_path: String,
    pub sum_complexity: u64,
}

/// Hotspot result for a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotspotEntry {
    pub file_path: String,
    pub commit_count: u64,
    pub lines_changed: u64,
    pub sum_complexity: u64,
    pub hotspot_score: u64,
}

/// Compute hotspots from churn and complexity inputs.
///
/// Only files present in BOTH inputs are included in the result.
/// Files with churn but no complexity are excluded.
///
/// Result is sorted by:
///   1. `hotspot_score` descending
///   2. `lines_changed` descending
///   3. `file_path` ascending
pub fn compute_hotspots(
    churn: &[ChurnInput],
    complexity: &[ComplexityInput],
) -> Vec<HotspotEntry> {
    // Build complexity lookup by file path
    let complexity_map: HashMap<&str, u64> = complexity
        .iter()
        .map(|c| (c.file_path.as_str(), c.sum_complexity))
        .collect();

    // Join churn with complexity, compute scores
    let mut results: Vec<HotspotEntry> = churn
        .iter()
        .filter_map(|c| {
            complexity_map.get(c.file_path.as_str()).map(|&sum_complexity| {
                let hotspot_score = c.lines_changed * sum_complexity;
                HotspotEntry {
                    file_path: c.file_path.clone(),
                    commit_count: c.commit_count,
                    lines_changed: c.lines_changed,
                    sum_complexity,
                    hotspot_score,
                }
            })
        })
        .collect();

    // Sort: hotspot_score desc, lines_changed desc, file_path asc
    results.sort_by(|a, b| {
        b.hotspot_score
            .cmp(&a.hotspot_score)
            .then_with(|| b.lines_changed.cmp(&a.lines_changed))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_hotspots_basic() {
        let churn = vec![
            ChurnInput {
                file_path: "src/a.ts".to_string(),
                commit_count: 5,
                lines_changed: 100,
            },
            ChurnInput {
                file_path: "src/b.ts".to_string(),
                commit_count: 3,
                lines_changed: 50,
            },
        ];

        let complexity = vec![
            ComplexityInput {
                file_path: "src/a.ts".to_string(),
                sum_complexity: 10,
            },
            ComplexityInput {
                file_path: "src/b.ts".to_string(),
                sum_complexity: 20,
            },
        ];

        let results = compute_hotspots(&churn, &complexity);

        assert_eq!(results.len(), 2);

        // a.ts: 100 * 10 = 1000
        // b.ts: 50 * 20 = 1000
        // Same score, so lines_changed desc, then file_path asc
        // a.ts has more lines_changed (100 > 50), so a.ts first
        assert_eq!(results[0].file_path, "src/a.ts");
        assert_eq!(results[0].hotspot_score, 1000);
        assert_eq!(results[1].file_path, "src/b.ts");
        assert_eq!(results[1].hotspot_score, 1000);
    }

    #[test]
    fn compute_hotspots_excludes_files_without_complexity() {
        let churn = vec![
            ChurnInput {
                file_path: "src/a.ts".to_string(),
                commit_count: 5,
                lines_changed: 100,
            },
            ChurnInput {
                file_path: "src/no_complexity.ts".to_string(),
                commit_count: 10,
                lines_changed: 200,
            },
        ];

        let complexity = vec![ComplexityInput {
            file_path: "src/a.ts".to_string(),
            sum_complexity: 10,
        }];

        let results = compute_hotspots(&churn, &complexity);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/a.ts");
    }

    #[test]
    fn compute_hotspots_excludes_files_without_churn() {
        let churn = vec![ChurnInput {
            file_path: "src/a.ts".to_string(),
            commit_count: 5,
            lines_changed: 100,
        }];

        let complexity = vec![
            ComplexityInput {
                file_path: "src/a.ts".to_string(),
                sum_complexity: 10,
            },
            ComplexityInput {
                file_path: "src/no_churn.ts".to_string(),
                sum_complexity: 50,
            },
        ];

        let results = compute_hotspots(&churn, &complexity);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/a.ts");
    }

    #[test]
    fn compute_hotspots_sorted_by_score_desc() {
        let churn = vec![
            ChurnInput {
                file_path: "low.ts".to_string(),
                commit_count: 1,
                lines_changed: 10,
            },
            ChurnInput {
                file_path: "high.ts".to_string(),
                commit_count: 1,
                lines_changed: 100,
            },
        ];

        let complexity = vec![
            ComplexityInput {
                file_path: "low.ts".to_string(),
                sum_complexity: 5,
            },
            ComplexityInput {
                file_path: "high.ts".to_string(),
                sum_complexity: 5,
            },
        ];

        let results = compute_hotspots(&churn, &complexity);

        // high.ts: 100 * 5 = 500
        // low.ts: 10 * 5 = 50
        assert_eq!(results[0].file_path, "high.ts");
        assert_eq!(results[0].hotspot_score, 500);
        assert_eq!(results[1].file_path, "low.ts");
        assert_eq!(results[1].hotspot_score, 50);
    }

    #[test]
    fn compute_hotspots_tiebreaker_lines_changed() {
        let churn = vec![
            ChurnInput {
                file_path: "a.ts".to_string(),
                commit_count: 1,
                lines_changed: 50,
            },
            ChurnInput {
                file_path: "b.ts".to_string(),
                commit_count: 1,
                lines_changed: 100,
            },
        ];

        let complexity = vec![
            ComplexityInput {
                file_path: "a.ts".to_string(),
                sum_complexity: 20,
            },
            ComplexityInput {
                file_path: "b.ts".to_string(),
                sum_complexity: 10,
            },
        ];

        let results = compute_hotspots(&churn, &complexity);

        // a.ts: 50 * 20 = 1000
        // b.ts: 100 * 10 = 1000
        // Same score, b.ts has more lines_changed
        assert_eq!(results[0].file_path, "b.ts");
        assert_eq!(results[1].file_path, "a.ts");
    }

    #[test]
    fn compute_hotspots_tiebreaker_file_path() {
        let churn = vec![
            ChurnInput {
                file_path: "z.ts".to_string(),
                commit_count: 1,
                lines_changed: 100,
            },
            ChurnInput {
                file_path: "a.ts".to_string(),
                commit_count: 1,
                lines_changed: 100,
            },
        ];

        let complexity = vec![
            ComplexityInput {
                file_path: "z.ts".to_string(),
                sum_complexity: 10,
            },
            ComplexityInput {
                file_path: "a.ts".to_string(),
                sum_complexity: 10,
            },
        ];

        let results = compute_hotspots(&churn, &complexity);

        // Same score, same lines_changed, a.ts < z.ts
        assert_eq!(results[0].file_path, "a.ts");
        assert_eq!(results[1].file_path, "z.ts");
    }

    #[test]
    fn compute_hotspots_empty_inputs() {
        let results = compute_hotspots(&[], &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn compute_hotspots_zero_complexity() {
        let churn = vec![ChurnInput {
            file_path: "a.ts".to_string(),
            commit_count: 5,
            lines_changed: 100,
        }];

        let complexity = vec![ComplexityInput {
            file_path: "a.ts".to_string(),
            sum_complexity: 0,
        }];

        let results = compute_hotspots(&churn, &complexity);

        // File included but score is 0
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].hotspot_score, 0);
    }

    #[test]
    fn compute_hotspots_preserves_commit_count() {
        let churn = vec![ChurnInput {
            file_path: "a.ts".to_string(),
            commit_count: 42,
            lines_changed: 100,
        }];

        let complexity = vec![ComplexityInput {
            file_path: "a.ts".to_string(),
            sum_complexity: 10,
        }];

        let results = compute_hotspots(&churn, &complexity);

        assert_eq!(results[0].commit_count, 42);
    }
}
