//! Istanbul/c8 coverage report parser.
//!
//! RS-MS-4-prereq-a: Parse Istanbul JSON format into FileCoverageFact.
//!
//! Istanbul/c8 coverage-final.json structure:
//! ```json
//! {
//!   "/absolute/path/to/file.ts": {
//!     "path": "/absolute/path/to/file.ts",
//!     "s": { "0": 2, "1": 0, "2": 5 },   // statement hit counts
//!     "f": { "0": 1, "1": 0 },           // function hit counts (not used for line_coverage)
//!     "b": { "0": [1, 0] },              // branch hit counts (not used for line_coverage)
//!     ...
//!   }
//! }
//! ```
//!
//! Line coverage is computed from the `s` (statement) map:
//! - total_statements = len(s)
//! - covered_statements = count of s values > 0
//! - line_coverage = covered / total (or 1.0 if total is 0)

use crate::normalize::normalize_to_repo_relative;
use crate::types::{CoverageParseError, CoverageParseResult, FileCoverageFact};
use serde_json::Value;

/// Parse an Istanbul/c8 coverage report from JSON content.
///
/// # Arguments
/// * `json_content` - The raw JSON string of the coverage report
/// * `repo_root` - The absolute path to the repository root (for path normalization)
///
/// # Returns
/// * `Ok(CoverageParseResult)` - Parsed facts and any unnormalized paths
/// * `Err(CoverageParseError)` - If parsing failed entirely
///
/// # Path normalization
/// Report paths are normalized to repo-relative form using `repo_root`.
/// Paths that cannot be normalized (absolute but not under repo root) are
/// collected in `unnormalized_paths` rather than causing a parse failure.
pub fn parse_istanbul_report(
    json_content: &str,
    repo_root: &str,
) -> Result<CoverageParseResult, CoverageParseError> {
    // Parse as generic JSON first
    let parsed: Value =
        serde_json::from_str(json_content).map_err(|e| CoverageParseError::InvalidJson {
            message: e.to_string(),
        })?;

    // Istanbul format is an object keyed by file paths
    let obj = parsed
        .as_object()
        .ok_or_else(|| CoverageParseError::UnrecognizedFormat {
            message: "expected top-level JSON object".to_string(),
        })?;

    if obj.is_empty() {
        return Ok(CoverageParseResult {
            facts: vec![],
            unnormalized_paths: vec![],
        });
    }

    // Validate Istanbul format: first entry should have an "s" field
    let first_entry = obj.values().next().unwrap();
    if !first_entry.is_object() || first_entry.get("s").is_none() {
        return Err(CoverageParseError::UnrecognizedFormat {
            message: "entries must be objects with 's' (statement) field".to_string(),
        });
    }

    let mut facts = Vec::new();
    let mut unnormalized_paths = Vec::new();

    for (report_path, entry) in obj {
        // Try to normalize the path
        let normalized = match normalize_to_repo_relative(report_path, repo_root) {
            Some(p) => p,
            None => {
                unnormalized_paths.push(report_path.clone());
                continue;
            }
        };

        // Extract statement coverage from "s" field
        // Skip files with no statement data — they have no coverage signal
        let stats = match extract_line_coverage(entry) {
            Some(s) => s,
            None => continue, // No statement data = skip this file
        };

        facts.push(FileCoverageFact {
            file_path: normalized,
            line_coverage: stats.line_coverage,
            covered_statements: stats.covered_statements,
            total_statements: stats.total_statements,
        });
    }

    // Sort facts by file_path for deterministic output
    facts.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    unnormalized_paths.sort();

    Ok(CoverageParseResult {
        facts,
        unnormalized_paths,
    })
}

/// Coverage statistics extracted from an Istanbul file entry.
struct CoverageStats {
    /// Coverage ratio (0.0 to 1.0), rounded to 4 decimal places.
    line_coverage: f64,
    /// Number of statements with hit count > 0.
    covered_statements: u64,
    /// Total number of statement slots.
    total_statements: u64,
}

/// Extract line coverage stats from an Istanbul file entry.
///
/// Uses the "s" (statement) map to compute coverage:
/// - total = number of statement slots
/// - covered = number of slots with hit count > 0
/// - ratio = covered / total (rounded to 4 decimal places)
///
/// Returns `None` if there are no statements. A file with no statement data
/// is not the same as a fully covered file — it means coverage is unavailable
/// for that file. The caller should skip emitting a fact for such files.
fn extract_line_coverage(entry: &Value) -> Option<CoverageStats> {
    let s_map = entry.get("s").and_then(|v| v.as_object())?;

    if s_map.is_empty() {
        return None; // No statement data = coverage unavailable
    }

    let total = s_map.len() as u64;
    let covered = s_map
        .values()
        .filter(|v| v.as_i64().map(|n| n > 0).unwrap_or(false))
        .count() as u64;

    // Round to 4 decimal places for stable output
    let ratio = (covered as f64) / (total as f64);
    let rounded = (ratio * 10000.0).round() / 10000.0;

    Some(CoverageStats {
        line_coverage: rounded,
        covered_statements: covered,
        total_statements: total,
    })
}

/// Parse an Istanbul report from a file path.
///
/// Convenience wrapper around `parse_istanbul_report` that handles file I/O.
pub fn parse_istanbul_file(
    report_path: &str,
    repo_root: &str,
) -> Result<CoverageParseResult, CoverageParseError> {
    let content =
        std::fs::read_to_string(report_path).map_err(|e| CoverageParseError::IoError {
            message: format!("{}: {}", report_path, e),
        })?;

    parse_istanbul_report(&content, repo_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_istanbul_json(files: &[(&str, &[i64])]) -> String {
        let mut entries = HashMap::new();
        for (path, hits) in files {
            let s_map: HashMap<String, i64> = hits
                .iter()
                .enumerate()
                .map(|(i, &h)| (i.to_string(), h))
                .collect();
            let entry = serde_json::json!({
                "path": path,
                "s": s_map,
                "f": {},
                "b": {}
            });
            entries.insert(path.to_string(), entry);
        }
        serde_json::to_string(&entries).unwrap()
    }

    #[test]
    fn parse_single_file_full_coverage() {
        let json = make_istanbul_json(&[("/repo/src/main.ts", &[1, 2, 3])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].file_path, "src/main.ts");
        assert!((result.facts[0].line_coverage - 1.0).abs() < 0.001);
        assert!(result.unnormalized_paths.is_empty());
    }

    #[test]
    fn parse_single_file_partial_coverage() {
        // 2 covered, 1 uncovered = 66.67%
        let json = make_istanbul_json(&[("/repo/src/main.ts", &[1, 0, 2])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        assert_eq!(result.facts.len(), 1);
        assert!((result.facts[0].line_coverage - 0.6667).abs() < 0.01);
    }

    #[test]
    fn parse_single_file_zero_coverage() {
        let json = make_istanbul_json(&[("/repo/src/main.ts", &[0, 0, 0])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        assert_eq!(result.facts.len(), 1);
        assert!((result.facts[0].line_coverage - 0.0).abs() < 0.001);
    }

    #[test]
    fn parse_multiple_files() {
        let json = make_istanbul_json(&[("/repo/src/a.ts", &[1, 1]), ("/repo/src/b.ts", &[1, 0])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        assert_eq!(result.facts.len(), 2);
        // Sorted by path
        assert_eq!(result.facts[0].file_path, "src/a.ts");
        assert_eq!(result.facts[1].file_path, "src/b.ts");
    }

    #[test]
    fn unnormalized_paths_collected() {
        let json = make_istanbul_json(&[("/repo/src/a.ts", &[1]), ("/other/path/b.ts", &[1])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].file_path, "src/a.ts");
        assert_eq!(result.unnormalized_paths, vec!["/other/path/b.ts"]);
    }

    #[test]
    fn empty_report() {
        let json = "{}";
        let result = parse_istanbul_report(json, "/repo").unwrap();

        assert!(result.facts.is_empty());
        assert!(result.unnormalized_paths.is_empty());
    }

    #[test]
    fn invalid_json() {
        let result = parse_istanbul_report("not json", "/repo");
        assert!(matches!(
            result,
            Err(CoverageParseError::InvalidJson { .. })
        ));
    }

    #[test]
    fn not_an_object() {
        let result = parse_istanbul_report("[1, 2, 3]", "/repo");
        assert!(matches!(
            result,
            Err(CoverageParseError::UnrecognizedFormat { .. })
        ));
    }

    #[test]
    fn missing_s_field() {
        let json = r#"{ "/repo/src/a.ts": { "path": "/repo/src/a.ts" } }"#;
        let result = parse_istanbul_report(json, "/repo");
        assert!(matches!(
            result,
            Err(CoverageParseError::UnrecognizedFormat { .. })
        ));
    }

    #[test]
    fn empty_s_field_skips_file() {
        let json = r#"{ "/repo/src/a.ts": { "path": "/repo/src/a.ts", "s": {} } }"#;
        let result = parse_istanbul_report(json, "/repo").unwrap();

        // Files with no statement data are skipped, not treated as 100% covered
        assert_eq!(result.facts.len(), 0);
    }

    #[test]
    fn path_with_spaces() {
        let json = make_istanbul_json(&[(
            "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph/src/main.ts",
            &[1, 1, 0],
        )]);
        let result = parse_istanbul_report(
            &json,
            "/Users/apple/Documents/APLICATII BIJUTERIE/repo-graph",
        )
        .unwrap();

        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].file_path, "src/main.ts");
        assert!((result.facts[0].line_coverage - 0.6667).abs() < 0.01);
    }

    #[test]
    fn coverage_includes_statement_counts() {
        // 2 covered, 1 uncovered out of 3 total
        let json = make_istanbul_json(&[("/repo/src/main.ts", &[1, 0, 2])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        assert_eq!(result.facts.len(), 1);
        let fact = &result.facts[0];
        assert_eq!(fact.covered_statements, 2);
        assert_eq!(fact.total_statements, 3);
        // Verify ratio matches counts
        assert_eq!(fact.line_coverage, 0.6667); // Rounded to 4 decimals
    }

    #[test]
    fn coverage_ratio_rounded_to_4_decimals() {
        // 1/3 = 0.33333... should round to 0.3333
        let json = make_istanbul_json(&[("/repo/src/main.ts", &[1, 0, 0])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        assert_eq!(result.facts[0].line_coverage, 0.3333);
        assert_eq!(result.facts[0].covered_statements, 1);
        assert_eq!(result.facts[0].total_statements, 3);
    }

    #[test]
    fn full_coverage_exact_values() {
        let json = make_istanbul_json(&[("/repo/src/main.ts", &[1, 2, 3])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        let fact = &result.facts[0];
        assert_eq!(fact.line_coverage, 1.0);
        assert_eq!(fact.covered_statements, 3);
        assert_eq!(fact.total_statements, 3);
    }

    #[test]
    fn zero_coverage_exact_values() {
        let json = make_istanbul_json(&[("/repo/src/main.ts", &[0, 0, 0, 0])]);
        let result = parse_istanbul_report(&json, "/repo").unwrap();

        let fact = &result.facts[0];
        assert_eq!(fact.line_coverage, 0.0);
        assert_eq!(fact.covered_statements, 0);
        assert_eq!(fact.total_statements, 4);
    }
}
