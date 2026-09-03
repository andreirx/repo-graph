//! IS-TEST-CPP-1 reproducing test — a C++ file's `is_test` is its gtest/gmock
//! STRUCTURAL marker, NOT its filename.
//!
//! Before the structural fix, C++ files carried a path-only `is_test`: a
//! `recovery_test.cc` in `db/` matched no test path-pattern → `is_test = 0` (the
//! measured leveldb gap). After the fix the cpp-extractor emits an `is_gtest_test`
//! marker (a `gtest/`/`gmock/` include OR a top-level `TEST`/`TEST_F`/`TEST_P`/
//! `TYPED_TEST` macro) onto the FILE node, and a compose-side postpass promotes
//! the genuinely-test files — while a PRODUCTION file whose NAME merely ends in
//! `_test.cc` is NOT promoted (the name-trap witness), and a file mentioning gtest
//! only in a comment/string is NOT promoted (structural, not substring).
//!
//! Drives the REAL write path (`compose` → `orchestrator` → reclassify postpass)
//! and proves the classification survives a no-change `refresh` (copy-forward
//! preserves FILE-node metadata; the postpass recomputes over the whole set).

use std::fs;

use repo_graph_indexer::storage_port::{FileCatalogPort, TrackedFile};
use repo_graph_repo_index::compose::{index_into_storage, refresh_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

/// Fixture exercising every §4 case:
/// - (a) gtest-include file → src/with_include.cc is_test=TRUE
/// - (b) TEST_F-only file (no in-file include) → src/macro_only.cc is_test=TRUE
/// - (c) PRODUCTION file named `*_test.cc` with no marker → src/parser_test.cc FALSE (name-trap)
/// - (d) gtest named only in a comment + a string literal → src/mentions.cc FALSE (structural)
/// - (e) `.h` gtest carrier (C++ header) → src/fixture_util.h is_test=TRUE (review-0 item 2)
/// - (f) unmarked C++ file UNDER `tests/` → tests/production.cc is_test=TRUE, UNCHANGED
///   (§2.3-conflict witness: absence of a gtest marker is WEAK negative evidence
///   and NEVER overrides the existing path-based classification — promote-only
///   preserves the value; operator ruling ISTESTCPP1-STRUCTURAL-SEMANTICS)
/// - plain production file → src/real.cc FALSE
fn write_fixture(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("tests")).unwrap();

    // (a) gtest include (the corpus quote form) → TRUE.
    fs::write(
        dir.join("src/with_include.cc"),
        "#include \"gtest/gtest.h\"\n\nTEST(WithInclude, Works) {\n  EXPECT_EQ(1, 1);\n}\n",
    )
    .unwrap();

    // (b) TEST_F with NO in-file gtest include (the dominant vcmi shape) → TRUE.
    fs::write(
        dir.join("src/macro_only.cc"),
        "TEST_F(MacroOnly, DoesThing) {\n  ASSERT_TRUE(true);\n}\n",
    )
    .unwrap();

    // (c) name-trap: production file named `*_test.cc`, NO marker → FALSE.
    fs::write(
        dir.join("src/parser_test.cc"),
        "int parse(const char* s) {\n  return s == nullptr ? 0 : 1;\n}\n",
    )
    .unwrap();

    // (d) gtest mentioned only in a comment and a string literal → FALSE.
    fs::write(
        dir.join("src/mentions.cc"),
        "// This file does not #include <gtest/gtest.h>; see TEST(Foo, Bar) in the docs.\n\
         const char* kDoc = \"#include <gtest/gtest.h>\";\n\
         int run() { return 0; }\n",
    )
    .unwrap();

    // (e) a `.h` gtest carrier that is a C++ header (namespace/class/`::` markers
    // → content-routed to the cpp-extractor) → TRUE. Path has no test-dir segment,
    // so promotion is by the STRUCTURAL gtest include alone, not the name. Uses the
    // REAL corpus shape of leveldb's `util/testutil.h` — an `#ifndef` include guard
    // wrapping the gtest include, so the include is inside a `preproc_ifdef`, not a
    // direct TU child; the detector must descend into it (review-0 item 2).
    fs::write(
        dir.join("src/fixture_util.h"),
        "#ifndef FX_FIXTURE_UTIL_H_\n#define FX_FIXTURE_UTIL_H_\n\
         #include \"gtest/gtest.h\"\n\
         namespace fx {\n\
         class Helper : public ::testing::Test {\n\
          public:\n\
           int value() const { return 0; }\n\
         };\n\
         }\n\
         #endif  // FX_FIXTURE_UTIL_H_\n",
    )
    .unwrap();

    // (f) §2.3-conflict witness: a C++ file with NO gtest/gmock marker that lives
    // under `tests/`. `routing::is_test_file` path-classifies it is_test=TRUE at
    // extraction; the promote-only postpass sees no marker and — per the ratified
    // evidence-strength semantics (marker absence is WEAK negative evidence) —
    // leaves the existing classification UNCHANGED. This is the reviewer's exact
    // concern from review-1, ruled CORRECT: promote-only preserves the value; it
    // never mints a false is_test=FALSE ("not a test") fact about a directory the
    // repo itself treats as tests. This is a known OVER-count residual, counted in
    // the build report per corpus repo, never a silent demotion.
    fs::write(
        dir.join("tests/production.cc"),
        "int helper(int a, int b) { return a * b; }\n",
    )
    .unwrap();

    // plain production file → FALSE.
    fs::write(
        dir.join("src/real.cc"),
        "int add(int a, int b) { return a + b; }\n",
    )
    .unwrap();
}

fn is_test_of(files: &[TrackedFile], path: &str) -> bool {
    files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("file {path} not tracked; tracked = {files:?}"))
        .is_test
}

#[test]
fn gtest_marker_drives_is_test_not_filename() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();

    let files = FileCatalogPort::get_files_by_repo(&storage, "fx").unwrap();

    // (a) gtest include → TRUE.
    assert!(
        is_test_of(&files, "src/with_include.cc"),
        "src/with_include.cc includes gtest → is_test must be TRUE",
    );
    // (b) TEST_F-only (no include) → TRUE.
    assert!(
        is_test_of(&files, "src/macro_only.cc"),
        "src/macro_only.cc has a top-level TEST_F → TRUE even with no in-file include",
    );
    // (c) name-trap: production `*_test.cc` with no marker → FALSE.
    assert!(
        !is_test_of(&files, "src/parser_test.cc"),
        "src/parser_test.cc is production (no marker) → must stay FALSE despite the `_test` name",
    );
    // (d) marker only in comment/string → FALSE (structural, not substring).
    assert!(
        !is_test_of(&files, "src/mentions.cc"),
        "src/mentions.cc mentions gtest only in a comment/string → FALSE",
    );
    // (e) `.h` gtest carrier (content-routed C++ header) → TRUE, by the marker
    // not the name (review-0 item 2).
    assert!(
        is_test_of(&files, "src/fixture_util.h"),
        "src/fixture_util.h is a C++ header with a gtest include → is_test must be TRUE",
    );
    // (f) §2.3-conflict witness: unmarked C++ file under `tests/` keeps its
    // path-based is_test=TRUE — absence of a gtest marker NEVER demotes an existing
    // classification (promote-only; operator ruling ISTESTCPP1-STRUCTURAL-SEMANTICS).
    assert!(
        is_test_of(&files, "tests/production.cc"),
        "tests/production.cc has NO gtest marker but is path-classified test → \
         promote-only must PRESERVE is_test=TRUE, never mint a false not-a-test fact",
    );
    // plain production file.
    assert!(
        !is_test_of(&files, "src/real.cc"),
        "src/real.cc is production"
    );
}

#[test]
fn structural_is_test_survives_no_change_refresh() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    // No file changes → all files copy-forward; the reclassify postpass recomputes
    // over the whole set (fresh + copied-forward FILE-node metadata).
    refresh_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();

    let files = FileCatalogPort::get_files_by_repo(&storage, "fx").unwrap();
    assert!(
        is_test_of(&files, "src/with_include.cc"),
        "refresh copy-forward must preserve the structural test classification",
    );
    assert!(is_test_of(&files, "src/macro_only.cc"));
    assert!(!is_test_of(&files, "src/parser_test.cc"));
    assert!(!is_test_of(&files, "src/mentions.cc"));
    assert!(
        is_test_of(&files, "src/fixture_util.h"),
        "refresh copy-forward must preserve the `.h` gtest-carrier classification",
    );
    assert!(
        is_test_of(&files, "tests/production.cc"),
        "refresh copy-forward must preserve the path-based classification of an \
         unmarked file under tests/ (§2.3: absence of a marker never demotes)",
    );
    assert!(!is_test_of(&files, "src/real.cc"));
}
