//! IS-TEST-RUST-1 reproducing test — a Rust file's `is_test` is its
//! `#[cfg(test)]` inclusion chain, NOT its filename.
//!
//! Before the structural fix, in-crate Rust test modules (`src/**/tests.rs`,
//! `#[cfg(test)] mod tests;`) carried `is_test = 0` because the classifier was
//! path-only and `tests.rs`/`foo_tests.rs` match no test path-pattern. After
//! the fix the rust-extractor emits each file's `mod` inclusion facts and a
//! compose-side resolver walks the crate's `#[cfg(test)]` chain to promote the
//! genuinely-test files — while a PRODUCTION module whose NAME merely contains
//! "tests" is NOT promoted (the name-trap witness).
//!
//! Drives the REAL write path (`compose` → `orchestrator` → structural
//! reclassify postpass), and proves the classification survives a no-change
//! `refresh` (copy-forward preserves FILE-node metadata; the postpass recomputes
//! over the whole file set on refresh too).

use std::fs;

use repo_graph_indexer::storage_port::{FileCatalogPort, TrackedFile};
use repo_graph_repo_index::compose::{index_into_storage, refresh_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

/// Fixture crate exercising every §4 case:
/// - (a) `#[cfg(test)] mod tests;` → src/tests.rs is_test=TRUE
/// - (b) nested `mod helper;` under (a) → src/tests/helper.rs is_test=TRUE (transitive)
/// - (c) production `mod tests_util;` (no cfg) → src/tests_util.rs is_test=FALSE (name-trap)
/// - (d) undeclared stray file → src/stray.rs unchanged (FALSE)
/// - corpus idiom `#[cfg(test)] #[path="foo_tests.rs"] mod t;` → src/foo_tests.rs is_test=TRUE
/// - (e) inline `#[cfg(test)] mod scope { mod child; }` → src/scope/child.rs is_test=TRUE
///   (the inline module contributes a directory segment; review-2 item 1)
/// - plain production module → src/real.rs is_test=FALSE
fn write_fixture(dir: &std::path::Path) {
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/lib.rs"),
        "pub mod real;\n\
         mod tests_util;\n\
         mod foo;\n\
         #[cfg(test)]\n\
         mod tests;\n\
         #[cfg(test)]\n\
         mod scope {\n\
         mod child;\n\
         }\n",
    )
    .unwrap();
    // (e) inline module case: `scope` is an inline #[cfg(test)] block, so its
    // nested `mod child;` lives at src/scope/child.rs and must be promoted.
    fs::create_dir_all(dir.join("src/scope")).unwrap();
    fs::write(dir.join("src/scope/child.rs"), "pub fn c() {}\n").unwrap();
    // (a) test module — declared under #[cfg(test)]; declares (b) a nested include.
    fs::write(dir.join("src/tests.rs"), "mod helper;\n\nfn t() {}\n").unwrap();
    fs::create_dir_all(dir.join("src/tests")).unwrap();
    // (b) transitive: pulled in by the cfg(test) module tests.rs.
    fs::write(dir.join("src/tests/helper.rs"), "pub fn h() {}\n").unwrap();
    // (c) name-trap: a PRODUCTION module named tests_util, included WITHOUT cfg(test).
    fs::write(dir.join("src/tests_util.rs"), "pub fn util() {}\n").unwrap();
    // corpus idiom: cfg(test) + #[path] override.
    fs::write(
        dir.join("src/foo.rs"),
        "#[cfg(test)]\n#[path = \"foo_tests.rs\"]\nmod foo_test_suite;\n\npub fn f() {}\n",
    )
    .unwrap();
    fs::write(dir.join("src/foo_tests.rs"), "fn ft() {}\n").unwrap();
    // plain production module.
    fs::write(dir.join("src/real.rs"), "pub fn r() {}\n").unwrap();
    // (d) undeclared stray — declared by no `mod`.
    fs::write(dir.join("src/stray.rs"), "pub fn s() {}\n").unwrap();
}

fn is_test_of(files: &[TrackedFile], path: &str) -> bool {
    files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("file {path} not tracked; tracked = {files:?}"))
        .is_test
}

#[test]
fn cfg_test_inclusion_chain_drives_is_test_not_filename() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();

    let files = FileCatalogPort::get_files_by_repo(&storage, "fx").unwrap();

    // (a) directly cfg(test)-gated.
    assert!(
        is_test_of(&files, "src/tests.rs"),
        "src/tests.rs is included under #[cfg(test)] → is_test must be TRUE",
    );
    // (b) transitively test (pulled in by the cfg(test) module).
    assert!(
        is_test_of(&files, "src/tests/helper.rs"),
        "src/tests/helper.rs is pulled in by the cfg(test) module → TRUE (transitive)",
    );
    // corpus idiom: cfg(test) + #[path] override.
    assert!(
        is_test_of(&files, "src/foo_tests.rs"),
        "src/foo_tests.rs is a #[cfg(test)] #[path]-mod target → TRUE",
    );
    // (e) inline module: `#[cfg(test)] mod scope { mod child; }` → the inline
    // block contributes a directory segment, so src/scope/child.rs is test.
    assert!(
        is_test_of(&files, "src/scope/child.rs"),
        "src/scope/child.rs is nested under an inline #[cfg(test)] module → TRUE",
    );
    // (c) name-trap: production module with "tests" in its name, no cfg(test).
    assert!(
        !is_test_of(&files, "src/tests_util.rs"),
        "src/tests_util.rs is production (no cfg(test)) → must stay FALSE despite the name",
    );
    // plain production module.
    assert!(
        !is_test_of(&files, "src/real.rs"),
        "src/real.rs is production"
    );
    // (d) undeclared stray keeps its (path-based) classification, unchanged.
    assert!(
        !is_test_of(&files, "src/stray.rs"),
        "src/stray.rs is declared by no mod → keeps existing classification (FALSE)",
    );
    // The crate root is production.
    assert!(
        !is_test_of(&files, "src/lib.rs"),
        "crate root is production"
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
        is_test_of(&files, "src/tests.rs"),
        "refresh copy-forward must preserve the structural test classification",
    );
    assert!(is_test_of(&files, "src/tests/helper.rs"));
    assert!(is_test_of(&files, "src/foo_tests.rs"));
    assert!(
        is_test_of(&files, "src/scope/child.rs"),
        "refresh must preserve the inline-module structural test classification",
    );
    assert!(!is_test_of(&files, "src/tests_util.rs"));
    assert!(!is_test_of(&files, "src/real.rs"));
}
