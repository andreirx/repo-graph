//! TEST-EDGE-SCOPE-1A reproducing test — test status through the product's OWN write path
//! (RG-REQ-001-L07; D-TESA-DERIVED-1, D-TESA-10).
//!
//! Drives `compose::index_into_storage` / `refresh_into_storage` into an in-memory store and
//! reads the stored facts back (`FileCatalogPort::get_files_by_repo`, the storage read
//! `query_tracked_file_test_flags`). UNDETERMINED is decided ONLY through the shared function
//! `repo_graph_classification::test_path::undetermined_test_word` over those stored facts —
//! nothing about it is stored.
//!
//! Proves: `testsuite/` directories are test (C and C++, FILE node `TEST_FILE`); every tracked
//! file — extracted, config, contract — takes the conventions; the shared function over the
//! stored flags yields exactly the test-worded, convention-free, non-test files (FILE-bearing or
//! not); a structural marker keeps a file out; the answer survives a no-change refresh; and it
//! follows a marker added later.

use std::collections::BTreeMap;
use std::fs;

use repo_graph_indexer::storage_port::{FileCatalogPort, TrackedFile};
use repo_graph_repo_index::compose::{index_into_storage, refresh_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn write_fixture(dir: &std::path::Path) {
    let w = |rel: &str, body: &str| {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    };
    // A Rust crate: a production module named `tests_util` (name-trap — no marker) and a
    // `#[cfg(test)]` module `test_helpers` (the Rust structural marker makes it test).
    w(
        "Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    w(
        "src/lib.rs",
        "pub mod tests_util;\n#[cfg(test)]\nmod test_helpers;\n",
    );
    w("src/test_helpers.rs", "pub fn h() {}\n");
    w("src/tests_util.rs", "pub fn util() {}\n");
    // `testsuite/` directories (poco's CppUnit layout; a top-level C one).
    w(
        "Lib/testsuite/src/FooTest.cpp",
        "namespace Lib { class FooTest { public: int run() { return 0; } }; }\n",
    );
    w("testsuite/run.c", "int run(void) { return 0; }\n");
    // A C++ header named like CppUnit's framework (`TestCaller.h`): test-worded, no convention.
    w(
        "Lib/include/Lib/TestCaller.h",
        "#ifndef LIB_TESTCALLER_H\n#define LIB_TESTCALLER_H\nnamespace Lib {\nclass TestCaller {\n public:\n  int call() const;\n};\n}\n#endif\n",
    );
    // leveldb's `util/testutil.cc` shape: a test word, no convention, no marker.
    w(
        "util/testutil.cc",
        "namespace util { int RandomString(int len) { return len; } }\n",
    );
    // IS-TEST-CPP-1's name-trap: `_test.cc` with no marker.
    w(
        "src/parser_test.cc",
        "int parse(const char* s) { return s ? 1 : 0; }\n",
    );
    // A gtest-marked file: test by its marker.
    w(
        "src/with_include_test.cc",
        "#include \"gtest/gtest.h\"\nTEST(A, B) {}\n",
    );
    w("pkg/testing.py", "def helper():\n    return 1\n");
    w(
        "web/NuggetTester.ts",
        "export function run(): number { return 1; }\n",
    );
    w("src/real.cc", "int add(int a, int b) { return a + b; }\n");
    // Tracked-only files (no FILE node): config and contract.
    w(
        "integration-tests/pom.xml",
        "<project>\n  <modelVersion>4.0.0</modelVersion>\n</project>\n",
    );
    w("tests/requirements.txt", "pytest\n");
    w(
        "src/test/proto/api.proto",
        "syntax = \"proto3\";\nmessage Ping {}\n",
    );
}

fn is_test_of(files: &[TrackedFile], path: &str) -> bool {
    files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("file {path} not tracked; tracked = {files:?}"))
        .is_test
}

/// The shared function over the snapshot's stored tracked flags: path → (word, has FILE node).
fn undetermined(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> BTreeMap<String, (String, bool)> {
    storage
        .query_tracked_file_test_flags(snapshot_uid, None)
        .unwrap()
        .into_iter()
        .filter_map(|f| {
            repo_graph_classification::test_path::undetermined_test_word(&f.path, f.is_test)
                .map(|w| (f.path.clone(), (w.to_string(), f.has_file_node)))
        })
        .collect()
}

fn expected_undetermined() -> BTreeMap<String, (String, bool)> {
    [
        ("Lib/include/Lib/TestCaller.h", "Test", true),
        ("integration-tests/pom.xml", "tests", false),
        ("pkg/testing.py", "testing", true),
        ("src/parser_test.cc", "test", true),
        ("src/tests_util.rs", "tests", true),
        ("util/testutil.cc", "testutil", true),
        ("web/NuggetTester.ts", "Tester", true),
    ]
    .into_iter()
    .map(|(p, w, n)| (p.to_string(), (w.to_string(), n)))
    .collect()
}

fn file_subtype(storage: &StorageConnection, snapshot_uid: &str, path: &str) -> Option<String> {
    // FILE node stable keys are `<repo>:<path>:FILE` (format v2).
    let key = format!("fx:{path}:FILE");
    storage
        .query_all_nodes(snapshot_uid)
        .unwrap()
        .into_iter()
        .find(|n| n.kind == "FILE" && n.stable_key == key)
        .unwrap_or_else(|| panic!("no FILE node for {path}"))
        .subtype
}

#[test]
fn testsuite_directory_files_are_test_files() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let r = index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    let files = FileCatalogPort::get_files_by_repo(&storage, "fx").unwrap();
    assert!(is_test_of(&files, "Lib/testsuite/src/FooTest.cpp"));
    assert!(is_test_of(&files, "testsuite/run.c"));
    assert_eq!(
        file_subtype(&storage, &r.snapshot_uid, "Lib/testsuite/src/FooTest.cpp").as_deref(),
        Some("TEST_FILE"),
        "the C++ FILE node takes the routing convention"
    );
    assert_eq!(
        file_subtype(&storage, &r.snapshot_uid, "testsuite/run.c").as_deref(),
        Some("TEST_FILE")
    );
    assert!(!is_test_of(&files, "src/real.cc"));
}

#[test]
fn test_worded_files_without_a_convention_or_marker_are_undetermined() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let r = index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    let files = FileCatalogPort::get_files_by_repo(&storage, "fx").unwrap();
    let got = undetermined(&storage, &r.snapshot_uid);
    assert_eq!(got, expected_undetermined());
    for p in got.keys() {
        assert!(
            !is_test_of(&files, p),
            "{p}: an undetermined file stays production"
        );
    }
}

#[test]
fn a_structural_marker_keeps_a_test_worded_file_out_of_undetermined() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let r = index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    let files = FileCatalogPort::get_files_by_repo(&storage, "fx").unwrap();
    let got = undetermined(&storage, &r.snapshot_uid);
    for p in ["src/with_include_test.cc", "src/test_helpers.rs"] {
        assert!(is_test_of(&files, p), "{p} is test by its marker");
        assert!(!got.contains_key(p), "{p} is not undetermined");
        assert_eq!(
            repo_graph_classification::test_path::undetermined_test_word(p, true),
            None
        );
    }
}

#[test]
fn tracked_only_files_take_is_test_from_the_conventions() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let r = index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    let files = FileCatalogPort::get_files_by_repo(&storage, "fx").unwrap();
    assert!(
        is_test_of(&files, "tests/requirements.txt"),
        "config under tests/"
    );
    assert!(
        is_test_of(&files, "src/test/proto/api.proto"),
        "contract under /test/"
    );
    assert!(!is_test_of(&files, "integration-tests/pom.xml"));
    let flags = storage
        .query_tracked_file_test_flags(&r.snapshot_uid, None)
        .unwrap();
    for p in [
        "tests/requirements.txt",
        "src/test/proto/api.proto",
        "integration-tests/pom.xml",
    ] {
        let f = flags.iter().find(|f| f.path == p).expect(p);
        assert!(!f.has_file_node, "{p} is tracked only");
    }
}

#[test]
fn undetermined_status_survives_a_no_change_refresh() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let r1 =
        index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    let flags1 = storage
        .query_tracked_file_test_flags(&r1.snapshot_uid, None)
        .unwrap();
    let r2 =
        refresh_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    assert_ne!(
        r1.snapshot_uid, r2.snapshot_uid,
        "the refresh made a new snapshot"
    );
    let flags2 = storage
        .query_tracked_file_test_flags(&r2.snapshot_uid, None)
        .unwrap();
    assert_eq!(
        flags1, flags2,
        "the same tracked flags after a no-change refresh"
    );
    assert_eq!(
        undetermined(&storage, &r2.snapshot_uid),
        expected_undetermined()
    );
}

#[test]
fn a_marker_added_later_makes_the_file_test_not_undetermined() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let r1 =
        index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    assert!(undetermined(&storage, &r1.snapshot_uid).contains_key("src/parser_test.cc"));
    fs::write(
        dir.path().join("src/parser_test.cc"),
        "#include \"gtest/gtest.h\"\nint parse(const char* s) { return s ? 1 : 0; }\nTEST(Parse, Works) {}\n",
    )
    .unwrap();
    let r2 =
        refresh_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    let files = FileCatalogPort::get_files_by_repo(&storage, "fx").unwrap();
    assert!(
        is_test_of(&files, "src/parser_test.cc"),
        "the refresh promoted it"
    );
    let mut expected = expected_undetermined();
    expected.remove("src/parser_test.cc");
    assert_eq!(
        undetermined(&storage, &r2.snapshot_uid),
        expected,
        "the status follows the stored flag it is computed from"
    );
}
