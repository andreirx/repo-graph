//! Coverage report DTOs.
//!
//! RS-MS-4-prereq-a: Pure data types for coverage import.

use serde::{Deserialize, Serialize};

/// A single file's line coverage fact.
///
/// This is the normalized output of parsing a coverage report.
/// The `file_path` is repo-relative after normalization.
///
/// Only files with actual statement coverage data produce a `FileCoverageFact`.
/// Files with no statement data (empty `s` map or missing `s` field in Istanbul)
/// are skipped entirely — they do not appear in the result set. This prevents
/// false 100% coverage signals for files that have no coverage information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileCoverageFact {
    /// Repo-relative file path (normalized, `/` separators).
    pub file_path: String,

    /// Line coverage ratio (0.0 to 1.0).
    ///
    /// Computed as `covered_statements / total_statements` from the Istanbul `s` map.
    /// Rounded to 4 decimal places for stable output.
    pub line_coverage: f64,

    /// Number of statements with hit count > 0.
    pub covered_statements: u64,

    /// Total number of statement slots in the file.
    ///
    /// Always >= 1 for files that produce a `FileCoverageFact`.
    /// Files with 0 statements are skipped by the parser.
    pub total_statements: u64,
}

/// Result of parsing a coverage report.
///
/// Contains both successfully parsed facts and information about
/// entries that could not be normalized or matched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageParseResult {
    /// Successfully parsed and normalized file coverage facts.
    pub facts: Vec<FileCoverageFact>,

    /// Paths from the report that could not be normalized to repo-relative form.
    ///
    /// These are paths that:
    /// - Are absolute but not under the repo root
    /// - Have other normalization failures
    ///
    /// Reported explicitly so the caller can diagnose coverage report issues.
    pub unnormalized_paths: Vec<String>,
}

/// Error during coverage report parsing.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CoverageParseError {
    /// JSON parsing failed.
    #[error("invalid JSON: {message}")]
    InvalidJson { message: String },

    /// Report structure is not recognized as Istanbul/c8 format.
    #[error("unrecognized coverage format: {message}")]
    UnrecognizedFormat { message: String },

    /// I/O error reading the report file.
    #[error("failed to read report: {message}")]
    IoError { message: String },
}
