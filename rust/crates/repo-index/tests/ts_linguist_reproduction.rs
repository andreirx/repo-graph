//! TS-LINGUIST-1 reproducing test — a file's language is its content, not its
//! extension.
//!
//! A Qt Linguist translation catalog (`.ts` extension, but XML content) must NOT
//! be classified as TypeScript. Before the content-sniff fix both the Qt `.ts`
//! and a genuine TypeScript `.ts` classified as `"typescript"`; after the fix the
//! XML one carries `None` (the schema's existing "not a code language" value that
//! config files already use) while the genuine one is unchanged.
//!
//! This drives the REAL write path (`compose` → `orchestrator`), not just the
//! pure sniff function, and additionally proves the classification survives a
//! no-change `refresh` (the copy-forward path must preserve, not re-derive).

use std::fs;

use repo_graph_indexer::storage_port::{FileCatalogPort, FileSignalPort};
use repo_graph_repo_index::compose::{index_into_storage, refresh_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

/// The exact VCMI Qt Linguist header (verified 2026-09-01 against
/// `../legacy-codebases/vcmi/mapeditor/translation/czech.ts`).
const QT_LINGUIST_TS: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<!DOCTYPE TS>\n\
<TS version=\"2.1\" language=\"cs_CZ\">\n\
<context>\n\
    <name>AbilitiesWidget</name>\n\
    <message>\n\
        <source>Abilities</source>\n\
        <translation>Schopnosti</translation>\n\
    </message>\n\
</context>\n\
</TS>\n";

const REAL_TYPESCRIPT: &str =
    "export function greet(name: string): string {\n  return `hi ${name}`;\n}\n";

fn write_fixture(dir: &std::path::Path) {
    fs::write(dir.join("package.json"), "{\"name\":\"vcmi-like\"}\n").unwrap();
    fs::create_dir_all(dir.join("translation")).unwrap();
    fs::write(dir.join("translation/czech.ts"), QT_LINGUIST_TS).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/index.ts"), REAL_TYPESCRIPT).unwrap();
}

fn language_of<'a>(
    files: &'a [repo_graph_indexer::storage_port::TrackedFile],
    path: &str,
) -> &'a Option<String> {
    &files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("file {path} not tracked; tracked = {files:?}"))
        .language
}

#[test]
fn qt_linguist_ts_classifies_as_non_code_real_ts_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(dir.path(), &mut storage, "vcmi", &ComposeOptions::default()).unwrap();

    let files = FileCatalogPort::get_files_by_repo(&storage, "vcmi").unwrap();

    // The Qt Linguist catalog is XML — NOT TypeScript. It carries `None`
    // (the existing non-code representation), never the false "typescript".
    assert_eq!(
        language_of(&files, "translation/czech.ts"),
        &None,
        "Qt Linguist .ts must not masquerade as TypeScript",
    );
    // A genuine TypeScript `.ts` is byte-identically classified.
    assert_eq!(
        language_of(&files, "src/index.ts"),
        &Some("typescript".to_string()),
        "genuine TypeScript classification is unchanged",
    );
}

#[test]
fn read_failed_ts_does_not_default_to_typescript() {
    // TS-LINGUIST-1 §2.4: a `.ts` the indexer cannot READ has no content to
    // sniff, so it must NOT be silently classified `"typescript"` (extension is
    // not evidence — it could be a Qt Linguist catalog). It must carry `None`
    // ("unknown"). Drives the REAL read-failure path: a `.ts` containing invalid
    // UTF-8 makes `read_to_string` fail → `ScannedFile::ReadFailed` →
    // `persist_read_failures`.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{\"name\":\"x\"}\n").unwrap();
    // Invalid UTF-8 (0xFF is never a valid UTF-8 lead byte) → read_to_string errs.
    fs::write(dir.path().join("unreadable.ts"), [0x00u8, 0xFF, 0xFE, 0x00]).unwrap();
    // A genuine, readable TypeScript file as a control.
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/ok.ts"), REAL_TYPESCRIPT).unwrap();

    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(dir.path(), &mut storage, "ru", &ComposeOptions::default()).unwrap();

    let files = FileCatalogPort::get_files_by_repo(&storage, "ru").unwrap();
    assert_eq!(
        language_of(&files, "unreadable.ts"),
        &None,
        "an unreadable .ts must not default to TypeScript (§2.4)",
    );
    assert_eq!(
        language_of(&files, "src/ok.ts"),
        &Some("typescript".to_string()),
        "a readable genuine TypeScript file is unaffected",
    );
}

#[test]
fn qt_catalog_in_npm_repo_acquires_no_ts_specific_signals() {
    // TS-LINGUIST-1 (review-2 item 2): compose's readable-file classification at
    // the dependency-signal gate must be the SAME content-aware fact as the index
    // write. Before the fix, `compose` re-derived language from the EXTENSION
    // there, so a Qt Linguist `.ts` in a repo WITH a package.json still entered
    // the JS/TS arm and acquired package.json dependency signals — a TS-specific
    // signal an XML catalog must never have. VCMI (no npm) did not exercise this;
    // this fixture does.
    let dir = tempfile::tempdir().unwrap();
    // A package.json declaring a dependency: genuine `.ts` files here DO get a
    // `express` dependency signal (the control proves the repo produces signals).
    fs::write(
        dir.path().join("package.json"),
        "{\"name\":\"vcmi-like\",\"dependencies\":{\"express\":\"^4.0.0\"}}\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("translation")).unwrap();
    fs::write(dir.path().join("translation/czech.ts"), QT_LINGUIST_TS).unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/index.ts"), REAL_TYPESCRIPT).unwrap();

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result =
        index_into_storage(dir.path(), &mut storage, "vcmi", &ComposeOptions::default()).unwrap();

    // Control: the genuine TypeScript file DID receive the package.json signal.
    let real_signals = FileSignalPort::query_file_signals_batch(
        &storage,
        &result.snapshot_uid,
        &["vcmi:src/index.ts".into()],
    )
    .unwrap();
    let real_deps = real_signals
        .first()
        .and_then(|s| s.package_dependencies_json.as_deref())
        .expect("genuine src/index.ts must receive package.json dependency signals");
    assert!(
        real_deps.contains("express"),
        "control: genuine .ts must see express, got: {real_deps}",
    );

    // The Qt Linguist catalog must have NO TypeScript dependency/tsconfig signal.
    // (A row may still exist for other reasons, but the two TS-specific signal
    // columns must be absent — the catalog never entered the JS/TS arm.)
    let qt_signals = FileSignalPort::query_file_signals_batch(
        &storage,
        &result.snapshot_uid,
        &["vcmi:translation/czech.ts".into()],
    )
    .unwrap();
    if let Some(sig) = qt_signals.first() {
        assert!(
            sig.package_dependencies_json.is_none(),
            "Qt catalog must not acquire package.json deps, got: {:?}",
            sig.package_dependencies_json,
        );
        assert!(
            sig.tsconfig_aliases_json.is_none(),
            "Qt catalog must not acquire tsconfig aliases, got: {:?}",
            sig.tsconfig_aliases_json,
        );
    }
}

#[test]
fn classification_survives_no_change_refresh() {
    // The copy-forward path must PRESERVE the sniffed classification of an
    // unchanged file, not re-derive it from the extension (which would regress
    // the Qt catalog back to "typescript").
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(dir.path(), &mut storage, "vcmi", &ComposeOptions::default()).unwrap();

    // No file changes → both files are "unchanged" → copy-forward path.
    refresh_into_storage(dir.path(), &mut storage, "vcmi", &ComposeOptions::default()).unwrap();

    let files = FileCatalogPort::get_files_by_repo(&storage, "vcmi").unwrap();
    assert_eq!(
        language_of(&files, "translation/czech.ts"),
        &None,
        "refresh copy-forward must not regress the Qt catalog to TypeScript",
    );
    assert_eq!(
        language_of(&files, "src/index.ts"),
        &Some("typescript".to_string()),
    );
}
