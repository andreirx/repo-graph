//! Shared formatting helpers for module presentation.
//!
//! # CLI-OUT-4 Module Identity Contract
//!
//! These helpers establish the module identity formatting contract used by
//! all module-facing commands:
//! - `modules list`
//! - `modules show`
//! - `modules files` (Group 2)
//! - `modules deps` (Group 3)
//!
//! ## Change Axis
//!
//! This file changes when the module identity display format changes.
//! It does NOT change when individual command output structure changes.

/// Format module kind with confidence.
///
/// Examples:
/// - "inferred (0.7)"
/// - "manifest"
/// - "declared (1.0)" -> "declared"
pub fn format_kind_confidence(kind: &str, confidence: f64) -> String {
    if (confidence - 1.0).abs() < 0.001 {
        // Confidence 1.0 - don't show decimal
        kind.to_string()
    } else {
        format!("{} ({:.1})", kind, confidence)
    }
}

/// Format a count with label, handling singular/plural.
///
/// Examples:
/// - "1 file"
/// - "646 files"
/// - "0 violations"
pub fn format_count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {}", singular)
    } else {
        format!("{} {}", count, plural)
    }
}

/// Format file count for list view (compact).
///
/// Examples:
/// - "100 files"
/// - "100 files (10 test)"
pub fn format_files_compact(owned: usize, test: usize) -> String {
    if test > 0 {
        format!("{} files ({} test)", owned, test)
    } else {
        format!("{} files", owned)
    }
}

/// Format dead symbol count for list view (compact).
///
/// Example: "25 dead"
pub fn format_dead_compact(dead: usize) -> String {
    format!("{} dead", dead)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_kind_confidence_shows_decimal() {
        assert_eq!(format_kind_confidence("inferred", 0.7), "inferred (0.7)");
    }

    #[test]
    fn format_kind_confidence_hides_1_0() {
        assert_eq!(format_kind_confidence("manifest", 1.0), "manifest");
    }

    #[test]
    fn format_count_singular() {
        assert_eq!(format_count(1, "file", "files"), "1 file");
    }

    #[test]
    fn format_count_plural() {
        assert_eq!(format_count(5, "file", "files"), "5 files");
    }

    #[test]
    fn format_count_zero() {
        assert_eq!(format_count(0, "violation", "violations"), "0 violations");
    }

    #[test]
    fn format_files_compact_without_test() {
        assert_eq!(format_files_compact(100, 0), "100 files");
    }

    #[test]
    fn format_files_compact_with_test() {
        assert_eq!(format_files_compact(100, 10), "100 files (10 test)");
    }

    #[test]
    fn format_dead_compact_value() {
        assert_eq!(format_dead_compact(25), "25 dead");
    }
}
