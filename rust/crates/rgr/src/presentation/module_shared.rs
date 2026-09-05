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

/// Format a module's file count for the compact list view.
///
/// COHERENCE-2 §2.4: `(N test)` is a SUBSET clause product-wide. `total` is ALL files the module
/// owns and `test` is how many OF them are tests (`test <= total`), matching `stats`
/// (`files=3014 (2000 test)` = 2000 of 3014). It is NOT an addend — `total` already includes the
/// test files, so a reader must never sum the two. This corrects `modules list`'s prior addend
/// rendering (it passed the non-test count as the headline, so `907 files (1997 test)` read as
/// 2904), which disagreed with `stats`/`check` about the same module's size.
///
/// Examples:
/// - "100 files"           (total=100, test=0)
/// - "100 files (10 test)" (total=100, of which 10 are tests)
pub fn format_files_compact(total: usize, test: usize) -> String {
    debug_assert!(
        test <= total,
        "COHERENCE-2 §2.4: `(N test)` is a subset — test ({test}) must not exceed total ({total})"
    );
    if test > 0 {
        format!("{} files ({} test)", total, test)
    } else {
        format!("{} files", total)
    }
}

/// Format the unreferenced-symbol count for the `modules list` compact column
/// (OUTPUT-DOC-TRUTH-AUDIT-1).
///
/// The input is the daemon's `dead_symbol_count` — a SYNTACTIC graph-orphan
/// estimate (symbols with no inbound reference in the modeled graph), a
/// LOW-reliability Layer-2 inference, NOT a Layer-0 "safe to delete" fact (the
/// public `rmap dead` surface was withdrawn for exactly this reason). The label
/// is the honest `unref?` (not the bare, overclaiming `dead`): `unref` =
/// unreferenced, `?` = uncertain/syntactic. `modules list` prints a caveat
/// footnote defining it; `modules show` renders the same count as the full word
/// "unreferenced". The function name tracks the `dead_symbol_count` data field
/// (unchanged daemon contract); only the user-facing label is corrected.
///
/// Example: "25 unref?"
pub fn format_dead_compact(dead: usize) -> String {
    format!("{} unref?", dead)
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
    fn format_files_compact_with_test_is_a_subset() {
        // COHERENCE-2 §2.4: the headline (100) is the TOTAL and (10 test) is the subset of it —
        // a reader reads "10 of 100 are tests", never 100+10. Same meaning as `stats`.
        assert_eq!(format_files_compact(100, 10), "100 files (10 test)");
    }

    #[test]
    fn format_dead_compact_value() {
        // OUTPUT-DOC-TRUTH-AUDIT-1: honest `unref?` label, not the overclaiming `dead`.
        assert_eq!(format_dead_compact(25), "25 unref?");
    }
}
