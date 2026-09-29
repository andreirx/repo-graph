//! The ONE definition of RG-REQ-001-L07's test-path conventions and of the
//! UNDETERMINED test status (TEST-EDGE-SCOPE-1A; D-TESA-DERIVED-1).
//!
//! - [`matches_test_path_convention`] — the routing path conventions that make a
//!   file a test file at index time (`__tests__`, `.test.`, `.spec.`, `/test/`,
//!   `/tests/`, the `test/`, `tests/`, `__tests__/` prefixes, and `testsuite/` as a
//!   directory component). `indexer::routing::is_test_file` delegates here, so the
//!   stored `files.is_test` writers and the UNDETERMINED status can never disagree
//!   about a convention.
//! - [`path_test_word`] — the first test word of a path (`test`, `tests`,
//!   `testing`, `tester`, `testutil`, case-insensitive, as a whole token).
//! - [`undetermined_test_word`] — the shared function every partitioned surface
//!   and `explain` call: a tracked file whose path carries a test word, matches no
//!   convention, and whose stored `is_test` is false (no marker promoted it) has
//!   test status UNDETERMINED; the returned word is the evidence.
//!
//! Pure: nothing is read and nothing is stored. The status is computed when asked
//! from two facts the store already holds for every tracked file — its path and
//! its `files.is_test` — so it survives a no-change refresh and covers files that
//! have no FILE node.

/// The five test words of RG-REQ-001-L07, lowercase.
const TEST_WORDS: &[&str] = &["test", "tests", "testing", "tester", "testutil"];

/// Whether `path` (repo-relative) matches one of RG-REQ-001-L07's routing
/// conventions. Case-sensitive, like the routing rule it replaces.
///
/// `testsuite/` is a DIRECTORY convention: `Foundation/testsuite/src/X.cpp` and
/// `testsuite/run.c` match; `src/testsuite.c`, `mytestsuite/x.c` and
/// `testsuites/x.c` do not.
pub fn matches_test_path_convention(path: &str) -> bool {
    path.contains("__tests__")
        || path.contains(".test.")
        || path.contains(".spec.")
        || path.contains("/test/")
        || path.contains("/tests/")
        || path.contains("/testsuite/")
        || path.starts_with("test/")
        || path.starts_with("tests/")
        || path.starts_with("__tests__/")
        || path.starts_with("testsuite/")
}

/// The FIRST token of `path`, in path order and as written, whose ASCII-lowercase
/// form is a test word (`test`, `tests`, `testing`, `tester`, `testutil`).
///
/// Tokens: the path is split at `/`, `.`, `_` and `-`; each piece is split at
/// CamelCase boundaries — a lowercase letter or digit followed by an uppercase
/// letter, and an uppercase letter followed by an uppercase letter that is itself
/// followed by a lowercase letter (`XMLTest` → `XML`, `Test`). Digits are not
/// separators (`test2` is one token), and letters inside a longer word are not a
/// token (`contest`, `latest`, `Testament`). `testsuite` as a token is not a test
/// word (its directory form is a convention).
pub fn path_test_word(path: &str) -> Option<&str> {
    let mut piece_start = 0usize;
    let bytes = path.as_bytes();
    for i in 0..=bytes.len() {
        let at_end = i == bytes.len();
        if at_end || matches!(bytes[i], b'/' | b'.' | b'_' | b'-') {
            if let Some(word) = camel_test_word(&path[piece_start..i]) {
                return Some(word);
            }
            piece_start = i + 1;
        }
    }
    None
}

/// The first CamelCase token of one separator-free piece that is a test word.
fn camel_test_word(piece: &str) -> Option<&str> {
    let b = piece.as_bytes();
    let mut start = 0usize;
    for i in 1..=b.len() {
        let boundary = i == b.len() || {
            let prev = b[i - 1];
            let cur = b[i];
            let lower_or_digit_then_upper =
                (prev.is_ascii_lowercase() || prev.is_ascii_digit()) && cur.is_ascii_uppercase();
            let acronym_end = prev.is_ascii_uppercase()
                && cur.is_ascii_uppercase()
                && b.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            lower_or_digit_then_upper || acronym_end
        };
        if boundary {
            // Boundaries sit only before an ASCII uppercase byte or at the end, so
            // `start..i` is always on a char boundary.
            let token = &piece[start..i];
            if is_test_word(token) {
                return Some(token);
            }
            start = i;
        }
    }
    None
}

fn is_test_word(token: &str) -> bool {
    TEST_WORDS.iter().any(|w| token.eq_ignore_ascii_case(w))
}

/// THE shared UNDETERMINED function (D-TESA-DERIVED-1).
///
/// `None` when `is_test` (a convention or a structural marker made the file test);
/// `None` when the path matches a convention (even where an older store's flag is
/// still false); otherwise the path's first test word, if any. `Some(word)` means
/// the file's test status can't be determined — it stays in the production
/// partition and is marked for opening.
///
/// Callers pass a KNOWN flag only; an unknown flag is never read as false.
pub fn undetermined_test_word(path: &str, is_test: bool) -> Option<&str> {
    if is_test || matches_test_path_convention(path) {
        return None;
    }
    path_test_word(path)
}

#[cfg(test)]
mod tests {
    use super::{matches_test_path_convention, path_test_word, undetermined_test_word};

    // ── separators ───────────────────────────────────────────

    #[test]
    fn slash_separates_a_test_word() {
        assert_eq!(path_test_word("src/Test/helper.c"), Some("Test"));
    }

    #[test]
    fn dot_separates_a_test_word() {
        assert_eq!(
            path_test_word("django/contrib/admin/tests.py"),
            Some("tests")
        );
    }

    #[test]
    fn underscore_separates_a_test_word() {
        assert_eq!(path_test_word("db/c_test.c"), Some("test"));
    }

    #[test]
    fn hyphen_separates_a_test_word() {
        assert_eq!(
            path_test_word("committer-tools/find-unfinished-test.py"),
            Some("test")
        );
    }

    #[test]
    fn camel_case_boundary_separates_a_test_word() {
        assert_eq!(
            path_test_word("CppUnit/include/CppUnit/TestCaller.h"),
            Some("Test")
        );
    }

    #[test]
    fn acronym_camel_case_boundary_separates_a_test_word() {
        assert_eq!(path_test_word("src/XMLTest.cpp"), Some("Test"));
    }

    // ── word forms ───────────────────────────────────────────

    #[test]
    fn word_test_is_a_test_word() {
        assert_eq!(path_test_word("src/core/test.py"), Some("test"));
    }

    #[test]
    fn word_tests_is_a_test_word() {
        assert_eq!(
            path_test_word("django/contrib/admin/tests.py"),
            Some("tests")
        );
    }

    #[test]
    fn word_testing_is_a_test_word() {
        assert_eq!(
            path_test_word("django/contrib/staticfiles/testing.py"),
            Some("testing")
        );
    }

    #[test]
    fn word_tester_is_a_test_word() {
        assert_eq!(
            path_test_word("packages/engine/src/nuggets/NuggetTester.ts"),
            Some("Tester")
        );
    }

    #[test]
    fn word_testutil_is_a_test_word() {
        assert_eq!(path_test_word("util/testutil.cc"), Some("testutil"));
    }

    #[test]
    fn test_word_match_is_case_insensitive() {
        assert_eq!(path_test_word("src/TESTING.c"), Some("TESTING"));
        assert_eq!(path_test_word("lib/Tests/x.c"), Some("Tests"));
    }

    // ── negatives ────────────────────────────────────────────

    #[test]
    fn letters_inside_a_longer_word_are_not_a_test_word() {
        for p in [
            "src/contest.c",
            "src/latest.h",
            "src/attestation.rs",
            "src/protest.py",
            "src/Testament.h",
            "src/testable.c",
            "src/detest.go",
        ] {
            assert_eq!(path_test_word(p), None, "{p}");
        }
    }

    #[test]
    fn digits_do_not_separate_a_test_word() {
        assert_eq!(path_test_word("src/test2.c"), None);
    }

    #[test]
    fn testsuite_token_is_not_a_test_word() {
        assert_eq!(path_test_word("src/testsuite.c"), None);
    }

    #[test]
    fn first_test_word_in_path_order_is_reported() {
        let p = "Data/DataTest/include/Poco/Data/Test/DataTest.h";
        let w = path_test_word(p).expect("a test word");
        assert_eq!(w, "Test");
        // The FIRST occurrence — inside the `DataTest` directory component.
        let offset = w.as_ptr() as usize - p.as_ptr() as usize;
        assert_eq!(offset, "Data/Data".len());
    }

    // ── conventions ──────────────────────────────────────────

    #[test]
    fn every_routing_convention_matches() {
        for p in [
            "src/__tests__/foo.ts",
            "src/app.test.ts",
            "src/app.spec.ts",
            "src/core/test/helper.ts",
            "src/core/tests/helper.ts",
            "test/unit/foo.ts",
            "tests/integration/bar.ts",
            "__tests__/foo.ts",
        ] {
            assert!(matches_test_path_convention(p), "{p}");
        }
        assert!(!matches_test_path_convention("src/core/service.ts"));
        assert!(!matches_test_path_convention("src/testing-utils.ts"));
    }

    #[test]
    fn testsuite_directory_is_a_convention() {
        assert!(matches_test_path_convention(
            "Foundation/testsuite/src/ArrayTest.cpp"
        ));
        assert!(matches_test_path_convention("testsuite/run.c"));
    }

    #[test]
    fn testsuite_without_directory_boundary_is_not_a_convention() {
        for p in ["src/testsuite.c", "mytestsuite/x.c", "testsuites/x.c"] {
            assert!(!matches_test_path_convention(p), "{p}");
        }
    }

    // ── the shared function ──────────────────────────────────

    #[test]
    fn undetermined_needs_a_test_word_no_convention_and_is_test_zero() {
        assert_eq!(
            undetermined_test_word("CppUnit/include/CppUnit/Test.h", false),
            Some("Test")
        );
        assert_eq!(undetermined_test_word("src/core/service.ts", false), None);
    }

    #[test]
    fn a_test_file_is_never_undetermined() {
        assert_eq!(undetermined_test_word("util/testutil.h", true), None);
    }

    #[test]
    fn a_convention_path_is_never_undetermined_even_when_its_flag_is_zero() {
        assert_eq!(
            undetermined_test_word("Foundation/testsuite/src/ArrayTest.cpp", false),
            None
        );
    }

    #[test]
    fn a_fileless_tracked_path_is_judged_by_path_and_flag_alone() {
        assert_eq!(
            undetermined_test_word(
                "hadoop-client-modules/hadoop-client-integration-tests/pom.xml",
                false
            ),
            Some("tests")
        );
    }
}
