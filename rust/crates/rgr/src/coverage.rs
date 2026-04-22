//! Coverage import orchestration.
//!
//! RS-MS-4-prereq-b: Write-side orchestration for coverage import.
//!
//! Takes parsed coverage facts, matches against indexed files, and
//! produces `MeasurementInput` records for persistence.
//!
//! Responsibilities:
//! - Match normalized file paths to indexed files exactly
//! - Convert matched facts to `MeasurementInput` with file stable keys
//! - Report matched vs unmatched statistics
//! - Detect duplicate facts (same file_path appearing multiple times)
//! - Generate collision-safe measurement UIDs via SHA-256
//!
//! Does NOT:
//! - Parse coverage reports (that's repo-graph-coverage crate)
//! - Write to storage (caller uses replace_measurements_by_kind for atomicity)

use repo_graph_coverage::{CoverageParseResult, FileCoverageFact};
use repo_graph_storage::types::MeasurementInput;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Result of matching coverage facts to indexed files.
#[derive(Debug, Clone)]
pub struct CoverageMatchResult {
    /// Measurements ready for persistence.
    pub measurements: Vec<MeasurementInput>,

    /// Number of facts that matched indexed files.
    pub matched_count: usize,

    /// Paths from the report that could not be normalized to repo-relative form.
    /// These failed at the parser level (e.g., absolute paths outside repo root).
    pub unnormalized_paths: Vec<String>,

    /// Paths that normalized successfully but do not match any indexed file.
    /// These are valid repo-relative paths that simply aren't in the current snapshot.
    pub unmatched_indexed_paths: Vec<String>,
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

/// Match coverage facts to indexed files and produce measurements.
///
/// # Arguments
/// * `parse_result` - Parsed coverage report with normalized paths
/// * `indexed_files` - Set of repo-relative file paths from the current snapshot
/// * `repo_uid` - Repository identifier for stable key construction
/// * `snapshot_uid` - Snapshot identifier for measurement records
/// * `now` - ISO 8601 timestamp for `created_at`
///
/// # Returns
/// * `Ok(CoverageMatchResult)` - Matched measurements and statistics
/// * `Err(CoverageMatchError)` - If duplicate paths detected in the report
///
/// # Matching rules
/// - Exact match only on normalized repo-relative paths
/// - No suffix matching, basename matching, or fuzzy heuristics
/// - Unmatched paths are reported, not silently dropped
pub fn match_coverage_to_indexed_files(
    parse_result: &CoverageParseResult,
    indexed_files: &HashSet<String>,
    repo_uid: &str,
    snapshot_uid: &str,
    now: &str,
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

    let mut measurements = Vec::new();
    let mut unmatched_indexed_paths = Vec::new();

    for fact in &parse_result.facts {
        if indexed_files.contains(&fact.file_path) {
            let measurement = fact_to_measurement(fact, repo_uid, snapshot_uid, now);
            measurements.push(measurement);
        } else {
            unmatched_indexed_paths.push(fact.file_path.clone());
        }
    }

    // Sort for deterministic output
    unmatched_indexed_paths.sort();

    Ok(CoverageMatchResult {
        matched_count: measurements.len(),
        measurements,
        unnormalized_paths: parse_result.unnormalized_paths.clone(),
        unmatched_indexed_paths,
    })
}

/// Convert a single coverage fact to a measurement input.
///
/// Measurement UID is derived from SHA-256 of `(snapshot_uid, target_stable_key, kind)`.
/// This ensures collision-safety for any valid file path (no character-collapsing).
fn fact_to_measurement(
    fact: &FileCoverageFact,
    repo_uid: &str,
    snapshot_uid: &str,
    now: &str,
) -> MeasurementInput {
    // Target identity: file stable key format {repo_uid}:{file_path}:FILE
    let target_stable_key = format!("{}:{}:FILE", repo_uid, fact.file_path);
    let kind = "line_coverage";

    // Measurement UID: SHA-256 of identity tuple, truncated to 32 hex chars.
    // Collision-safe for any file path (no character substitution).
    let identity = format!("{}:{}:{}", snapshot_uid, target_stable_key, kind);
    let hash = Sha256::digest(identity.as_bytes());
    let measurement_uid = format!("msr:{:x}", hash).chars().take(36).collect::<String>();

    // Value JSON with ratio and underlying counts
    let value_json = format!(
        r#"{{"value":{},"covered":{},"total":{}}}"#,
        fact.line_coverage, fact.covered_statements, fact.total_statements
    );

    MeasurementInput {
        measurement_uid,
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        target_stable_key,
        kind: kind.to_string(),
        value_json,
        source: "coverage-istanbul:0.1.0".to_string(),
        created_at: now.to_string(),
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
    fn matched_file_produces_measurement() {
        let parse_result = make_parse_result(vec![("src/main.ts", 0.8, 8, 10)], vec![]);
        let indexed = make_indexed_files(&["src/main.ts"]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01")
                .unwrap();

        assert_eq!(result.matched_count, 1);
        assert_eq!(result.measurements.len(), 1);
        assert!(result.unmatched_indexed_paths.is_empty());
        assert!(result.unnormalized_paths.is_empty());
    }

    #[test]
    fn unmatched_indexed_path_reported() {
        let parse_result = make_parse_result(vec![("src/missing.ts", 0.5, 5, 10)], vec![]);
        let indexed = make_indexed_files(&["src/other.ts"]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01")
                .unwrap();

        assert_eq!(result.matched_count, 0);
        assert!(result.measurements.is_empty());
        assert_eq!(result.unmatched_indexed_paths, vec!["src/missing.ts"]);
    }

    #[test]
    fn unnormalized_paths_preserved() {
        let parse_result = make_parse_result(vec![], vec!["/outside/repo/file.ts"]);
        let indexed = make_indexed_files(&[]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01")
                .unwrap();

        assert_eq!(result.unnormalized_paths, vec!["/outside/repo/file.ts"]);
    }

    #[test]
    fn duplicate_paths_error() {
        // Manually construct a parse result with duplicates
        // (The real parser shouldn't produce this, but we guard against it)
        let parse_result = CoverageParseResult {
            facts: vec![
                FileCoverageFact {
                    file_path: "src/main.ts".to_string(),
                    line_coverage: 0.8,
                    covered_statements: 8,
                    total_statements: 10,
                },
                FileCoverageFact {
                    file_path: "src/main.ts".to_string(), // Duplicate
                    line_coverage: 0.9,
                    covered_statements: 9,
                    total_statements: 10,
                },
            ],
            unnormalized_paths: vec![],
        };
        let indexed = make_indexed_files(&["src/main.ts"]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01");

        assert!(matches!(
            result,
            Err(CoverageMatchError::DuplicatePaths { .. })
        ));
    }

    #[test]
    fn measurement_has_correct_stable_key() {
        let parse_result = make_parse_result(vec![("src/lib/utils.ts", 0.75, 3, 4)], vec![]);
        let indexed = make_indexed_files(&["src/lib/utils.ts"]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "myrepo", "snap1", "2026-01-01")
                .unwrap();

        let m = &result.measurements[0];
        assert_eq!(m.target_stable_key, "myrepo:src/lib/utils.ts:FILE");
        assert_eq!(m.kind, "line_coverage");
        assert_eq!(m.repo_uid, "myrepo");
        assert_eq!(m.snapshot_uid, "snap1");
    }

    #[test]
    fn measurement_value_json_has_counts() {
        let parse_result = make_parse_result(vec![("src/main.ts", 0.6667, 2, 3)], vec![]);
        let indexed = make_indexed_files(&["src/main.ts"]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01")
                .unwrap();

        let m = &result.measurements[0];
        // Check value_json contains all three fields
        assert!(m.value_json.contains(r#""value":0.6667"#));
        assert!(m.value_json.contains(r#""covered":2"#));
        assert!(m.value_json.contains(r#""total":3"#));
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

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01")
                .unwrap();

        assert_eq!(result.matched_count, 2);
        assert_eq!(result.measurements.len(), 2);
        assert_eq!(result.unmatched_indexed_paths, vec!["src/missing.ts"]);
        assert_eq!(result.unnormalized_paths, vec!["/outside/file.ts"]);
    }

    #[test]
    fn only_line_coverage_kind() {
        let parse_result = make_parse_result(vec![("src/main.ts", 0.8, 8, 10)], vec![]);
        let indexed = make_indexed_files(&["src/main.ts"]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01")
                .unwrap();

        // All measurements should be line_coverage only
        for m in &result.measurements {
            assert_eq!(m.kind, "line_coverage");
        }
    }

    #[test]
    fn measurement_uid_is_sha256_based() {
        let parse_result = make_parse_result(vec![("src/main.ts", 0.8, 8, 10)], vec![]);
        let indexed = make_indexed_files(&["src/main.ts"]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01")
                .unwrap();

        let m = &result.measurements[0];
        // SHA-256 based UIDs start with "msr:" prefix
        assert!(m.measurement_uid.starts_with("msr:"), "uid={}", m.measurement_uid);
        // Should be 36 chars: "msr:" (4) + 32 hex chars
        assert_eq!(m.measurement_uid.len(), 36, "uid={}", m.measurement_uid);
    }

    #[test]
    fn measurement_uid_collision_free_for_similar_paths() {
        // These paths would collide under the old `/` and `.` replacement scheme:
        // "src/a.b.ts" and "src/a/b.ts" both became "src_a_b_ts"
        let parse_result = make_parse_result(
            vec![
                ("src/a.b.ts", 0.8, 8, 10),
                ("src/a/b.ts", 0.6, 6, 10),
            ],
            vec![],
        );
        let indexed = make_indexed_files(&["src/a.b.ts", "src/a/b.ts"]);

        let result =
            match_coverage_to_indexed_files(&parse_result, &indexed, "repo", "snap1", "2026-01-01")
                .unwrap();

        assert_eq!(result.measurements.len(), 2);

        let uid1 = &result.measurements[0].measurement_uid;
        let uid2 = &result.measurements[1].measurement_uid;

        // UIDs must be distinct even though paths would have collided before
        assert_ne!(uid1, uid2, "UIDs must be distinct: {} vs {}", uid1, uid2);
    }
}
