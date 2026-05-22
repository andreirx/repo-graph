//! Coverage matching algorithm.
//!
//! Pure matching logic between parsed coverage reports and indexed files.
//! No storage dependency — returns plain DTOs that callers adapt to their
//! persistence model.
//!
//! # Architecture
//!
//! This module is a boundary translation layer:
//! - Input: coverage report semantics (CoverageParseResult)
//! - Input: indexed file semantics (set of file paths)
//! - Output: matched coverage facts as plain DTOs
//!
//! The matching algorithm is pure and reusable. Both daemon-runtime and
//! CLI can use it, then adapt the results to MeasurementInput or other
//! storage types independently.
//!
//! # Added for LEGACY-CONTRACT-MIGRATION-1B
//!
//! Extracted from rgr/src/coverage.rs to enable sharing between CLI
//! and daemon without duplicating the matching logic.

use repo_graph_coverage::{CoverageParseResult, FileCoverageFact};
use std::collections::{HashMap, HashSet};

/// Result of matching coverage facts to indexed files.
#[derive(Debug, Clone)]
pub struct CoverageMatchResult {
    /// Facts that matched indexed files.
    pub matched: Vec<MatchedCoverageFact>,

    /// Paths from the report that could not be normalized to repo-relative form.
    /// These failed at the parser level (e.g., absolute paths outside repo root).
    pub unnormalized_paths: Vec<String>,

    /// Paths that normalized successfully but do not match any indexed file.
    /// These are valid repo-relative paths that simply aren't in the current snapshot.
    pub unmatched_indexed_paths: Vec<String>,
}

/// A coverage fact that was matched to an indexed file.
///
/// Plain DTO with no storage concerns. Callers adapt this to their
/// persistence model (e.g., MeasurementInput with UIDs and timestamps).
#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchedCoverageFact {
    /// Repo-relative file path.
    pub file_path: String,

    /// Line coverage ratio (0.0 to 1.0).
    pub line_coverage: f64,

    /// Number of covered statements.
    pub covered_statements: u64,

    /// Total number of statements.
    pub total_statements: u64,
}

/// Error during coverage matching.
#[derive(Debug, Clone, PartialEq)]
pub enum CoverageMatchError {
    /// The coverage report contains duplicate paths after normalization.
    /// This would cause silent last-write-wins behavior if not caught.
    DuplicatePaths { paths: Vec<String> },
}

impl std::fmt::Display for CoverageMatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverageMatchError::DuplicatePaths { paths } => {
                write!(
                    f,
                    "coverage report contains {} duplicate paths: {}",
                    paths.len(),
                    paths.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for CoverageMatchError {}

/// Match coverage facts to indexed files.
///
/// Pure matching algorithm. No storage dependency.
/// Caller adapts results to their persistence model.
///
/// # Arguments
///
/// * `parse_result` - Parsed coverage report with normalized paths
/// * `indexed_paths` - Set of repo-relative file paths from the current snapshot
///
/// # Returns
///
/// * `Ok(CoverageMatchResult)` - Matched facts and statistics
/// * `Err(CoverageMatchError)` - If duplicate paths detected in the report
///
/// # Matching rules
///
/// - Exact match only on normalized repo-relative paths
/// - No suffix matching, basename matching, or fuzzy heuristics
/// - Unmatched paths are reported, not silently dropped
pub fn match_coverage_to_indexed(
    parse_result: &CoverageParseResult,
    indexed_paths: &HashSet<String>,
) -> Result<CoverageMatchResult, CoverageMatchError> {
    // Check for duplicate paths in the coverage report
    let mut seen_paths: HashMap<&str, usize> = HashMap::new();
    for fact in &parse_result.facts {
        *seen_paths.entry(&fact.file_path).or_insert(0) += 1;
    }
    let duplicates: Vec<String> = seen_paths
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(path, _)| path.to_string())
        .collect();

    if !duplicates.is_empty() {
        return Err(CoverageMatchError::DuplicatePaths { paths: duplicates });
    }

    let mut matched = Vec::new();
    let mut unmatched_indexed_paths = Vec::new();

    for fact in &parse_result.facts {
        if indexed_paths.contains(&fact.file_path) {
            matched.push(fact_to_matched(fact));
        } else {
            unmatched_indexed_paths.push(fact.file_path.clone());
        }
    }

    // Sort for deterministic output
    unmatched_indexed_paths.sort();

    Ok(CoverageMatchResult {
        matched,
        unnormalized_paths: parse_result.unnormalized_paths.clone(),
        unmatched_indexed_paths,
    })
}

/// Convert a coverage fact to a matched fact DTO.
fn fact_to_matched(fact: &FileCoverageFact) -> MatchedCoverageFact {
    MatchedCoverageFact {
        file_path: fact.file_path.clone(),
        line_coverage: fact.line_coverage,
        covered_statements: fact.covered_statements,
        total_statements: fact.total_statements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_parse_result(
        facts: Vec<(&str, f64, u64, u64)>,
        unnormalized: Vec<&str>,
    ) -> CoverageParseResult {
        CoverageParseResult {
            facts: facts
                .into_iter()
                .map(|(path, cov, covered, total)| FileCoverageFact {
                    file_path: path.to_string(),
                    line_coverage: cov,
                    covered_statements: covered,
                    total_statements: total,
                })
                .collect(),
            unnormalized_paths: unnormalized.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    fn make_indexed_files(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn matched_file_produces_fact() {
        let parse_result = make_parse_result(vec![("src/main.ts", 0.8, 8, 10)], vec![]);
        let indexed = make_indexed_files(&["src/main.ts"]);

        let result = match_coverage_to_indexed(&parse_result, &indexed).unwrap();

        assert_eq!(result.matched.len(), 1);
        assert_eq!(result.matched[0].file_path, "src/main.ts");
        assert_eq!(result.matched[0].line_coverage, 0.8);
        assert!(result.unmatched_indexed_paths.is_empty());
        assert!(result.unnormalized_paths.is_empty());
    }

    #[test]
    fn unmatched_indexed_path_reported() {
        let parse_result = make_parse_result(vec![("src/missing.ts", 0.5, 5, 10)], vec![]);
        let indexed = make_indexed_files(&["src/other.ts"]);

        let result = match_coverage_to_indexed(&parse_result, &indexed).unwrap();

        assert!(result.matched.is_empty());
        assert_eq!(result.unmatched_indexed_paths, vec!["src/missing.ts"]);
    }

    #[test]
    fn unnormalized_paths_preserved() {
        let parse_result = make_parse_result(vec![], vec!["/outside/repo/file.ts"]);
        let indexed = make_indexed_files(&[]);

        let result = match_coverage_to_indexed(&parse_result, &indexed).unwrap();

        assert_eq!(result.unnormalized_paths, vec!["/outside/repo/file.ts"]);
    }

    #[test]
    fn duplicate_paths_error() {
        let parse_result = CoverageParseResult {
            facts: vec![
                FileCoverageFact {
                    file_path: "src/main.ts".to_string(),
                    line_coverage: 0.8,
                    covered_statements: 8,
                    total_statements: 10,
                },
                FileCoverageFact {
                    file_path: "src/main.ts".to_string(),
                    line_coverage: 0.9,
                    covered_statements: 9,
                    total_statements: 10,
                },
            ],
            unnormalized_paths: vec![],
        };
        let indexed = make_indexed_files(&["src/main.ts"]);

        let result = match_coverage_to_indexed(&parse_result, &indexed);

        assert!(matches!(
            result,
            Err(CoverageMatchError::DuplicatePaths { .. })
        ));
    }

    #[test]
    fn mixed_matched_and_unmatched() {
        let parse_result = make_parse_result(
            vec![
                ("src/a.ts", 0.8, 8, 10),
                ("src/b.ts", 0.6, 6, 10),
                ("src/missing.ts", 0.5, 5, 10),
            ],
            vec!["/outside/file.ts"],
        );
        let indexed = make_indexed_files(&["src/a.ts", "src/b.ts", "src/other.ts"]);

        let result = match_coverage_to_indexed(&parse_result, &indexed).unwrap();

        assert_eq!(result.matched.len(), 2);
        assert_eq!(result.unmatched_indexed_paths, vec!["src/missing.ts"]);
        assert_eq!(result.unnormalized_paths, vec!["/outside/file.ts"]);
    }
}
