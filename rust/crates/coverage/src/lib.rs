//! Coverage report parsing and normalization.
//!
//! RS-MS-4-prereq-a: Pure support crate for importing coverage reports.
//!
//! # Scope
//!
//! This crate handles:
//! - Parsing Istanbul/c8 JSON coverage reports
//! - Normalizing report paths to repo-relative form
//! - Producing `FileCoverageFact` DTOs for downstream persistence
//!
//! This crate does NOT handle:
//! - Storage writes (that's the compose/CLI layer)
//! - Matching against indexed files (that's the orchestration layer)
//! - Risk scoring or gate logic
//!
//! # Format support
//!
//! Currently supports Istanbul/c8 JSON format only (`coverage-final.json`).
//! Other formats (lcov, cobertura, jacoco) are not supported in v1.
//!
//! # Path normalization contract
//!
//! 1. Normalize separators to `/`
//! 2. If report path is absolute and under repo root, strip the prefix
//! 3. Trim leading `/` after stripping
//! 4. Already-relative paths pass through with normalized separators
//! 5. Absolute paths not under repo root are reported as unnormalized
//! 6. No suffix matching, basename matching, or fuzzy heuristics
//!
//! # Example
//!
//! ```ignore
//! use repo_graph_coverage::{parse_istanbul_report, FileCoverageFact};
//!
//! let json = std::fs::read_to_string("coverage/coverage-final.json")?;
//! let result = parse_istanbul_report(&json, "/path/to/repo")?;
//!
//! for fact in result.facts {
//!     println!("{}: {:.1}%", fact.file_path, fact.line_coverage * 100.0);
//! }
//!
//! if !result.unnormalized_paths.is_empty() {
//!     eprintln!("Warning: {} paths could not be normalized", result.unnormalized_paths.len());
//! }
//! ```

mod istanbul;
mod normalize;
mod types;

pub use istanbul::{parse_istanbul_file, parse_istanbul_report};
pub use normalize::normalize_to_repo_relative;
pub use types::{CoverageParseError, CoverageParseResult, FileCoverageFact};
