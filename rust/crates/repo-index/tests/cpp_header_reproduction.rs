//! FIND-KIND-MISLABEL-1 reproducing test — a C++ class in a `.h` is a CLASS,
//! not a FUNCTION.
//!
//! On vcmi, `class HeroClassID …` living in a `.h` header was rendered
//! `SYMBOL:FUNCTION`. Diagnosis (build-0, H1): `.h` mapped to C by extension, so
//! a C++ header routed to the C extractor, `tree-sitter-c` misparsed the `class`
//! into a function-shaped node, and `extract_function` stamped
//! `subtype = FUNCTION`. The fix content-sniffs the `.h`: C++ structural markers
//! promote it to `cpp`, and the extractor is selected from that same content
//! fact.
//!
//! This drives the REAL write path (`compose::index_into_storage` →
//! `orchestrator` → extractor selection → persistence), NOT the pure sniff
//! function — the review-1 requirement for an end-to-end regression that a unit
//! test on `classify_file_language` alone cannot provide. It asserts BOTH the
//! persisted `files.language` AND the persisted node kind/subtype, and includes a
//! genuine C header control that must stay C with its function correctly
//! `FUNCTION`.

use std::fs;

use repo_graph_indexer::storage_port::FileCatalogPort;
use repo_graph_repo_index::compose::{index_into_storage, refresh_into_storage, ComposeOptions};
use repo_graph_storage::types::GraphNode;
use repo_graph_storage::StorageConnection;

/// vcmi `lib/constants/EntityIdentifiers.h` shape: a C++ `class` in a `.h`.
/// Pre-fix routed to the C extractor → `SYMBOL:FUNCTION`; post-fix a `cpp`
/// header → `SYMBOL:CLASS`.
const CPP_CLASS_HEADER: &str = "#pragma once\n\
class HeroClassID : public EntityIdentifier<HeroClassID> {\n\
public:\n\
    HeroClassID() = default;\n\
};\n";

/// OpenXcom `Scalers/common.h` shape: a genuine C header — a real function
/// definition, no C++ markers anywhere in code. Must stay C, and its function
/// must stay correctly `SYMBOL:FUNCTION` (FUNCTION is RIGHT here).
const C_FUNCTION_HEADER: &str = "#ifndef ADD_H\n\
#define ADD_H\n\
int add(int a, int b) { return a + b; }\n\
#endif\n";

fn write_fixture(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("lib/constants")).unwrap();
    fs::write(
        dir.join("lib/constants/EntityIdentifiers.h"),
        CPP_CLASS_HEADER,
    )
    .unwrap();
    fs::create_dir_all(dir.join("include")).unwrap();
    fs::write(dir.join("include/add.h"), C_FUNCTION_HEADER).unwrap();
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

/// The single node whose `name` matches, in the given source file, panicking
/// with the full node list if absent (so a regression prints what WAS emitted).
fn node_named<'a>(nodes: &'a [GraphNode], file_frag: &str, name: &str) -> &'a GraphNode {
    nodes
        .iter()
        .find(|n| n.name == name && n.stable_key.contains(file_frag))
        .unwrap_or_else(|| {
            let symbols: Vec<_> = nodes
                .iter()
                .filter(|n| n.kind == "SYMBOL")
                .map(|n| format!("{} {:?} [{}]", n.name, n.subtype, n.stable_key))
                .collect();
            panic!("no node named {name} in {file_frag}; SYMBOL nodes = {symbols:#?}")
        })
}

#[test]
fn cpp_class_in_h_labels_as_class_c_header_stays_function() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result =
        index_into_storage(dir.path(), &mut storage, "vcmi", &ComposeOptions::default()).unwrap();

    // ── persisted language (the content-sniff fact) ──
    let files = FileCatalogPort::get_files_by_repo(&storage, "vcmi").unwrap();
    assert_eq!(
        language_of(&files, "lib/constants/EntityIdentifiers.h"),
        &Some("cpp".to_string()),
        "a C++ class header must persist language cpp, not c",
    );
    assert_eq!(
        language_of(&files, "include/add.h"),
        &Some("c".to_string()),
        "a genuine C header must stay c",
    );

    // ── persisted node kind/subtype (what `find` renders) ──
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let hero = node_named(&nodes, "EntityIdentifiers.h", "HeroClassID");
    assert_eq!(hero.kind, "SYMBOL", "HeroClassID must be a SYMBOL");
    assert_eq!(
        hero.subtype.as_deref(),
        Some("CLASS"),
        "the reproducing defect: HeroClassID must render SYMBOL:CLASS, not \
         SYMBOL:FUNCTION (got {:?})",
        hero.subtype,
    );

    // Nothing named HeroClassID may carry the FUNCTION subtype (the mislabel).
    assert!(
        !nodes
            .iter()
            .any(|n| n.name == "HeroClassID" && n.subtype.as_deref() == Some("FUNCTION")),
        "HeroClassID must never be a FUNCTION after the fix",
    );

    // Control: the genuine C function stays FUNCTION (FUNCTION is correct here).
    let add = node_named(&nodes, "add.h", "add");
    assert_eq!(add.kind, "SYMBOL");
    assert_eq!(
        add.subtype.as_deref(),
        Some("FUNCTION"),
        "a genuine C function in a C header must stay SYMBOL:FUNCTION (got {:?})",
        add.subtype,
    );
}

#[test]
fn cpp_class_header_classification_survives_no_change_refresh() {
    // Persistence completeness: the copy-forward path must PRESERVE the promoted
    // classification of an unchanged header, not re-derive it from the extension
    // (which would regress the C++ header back to c → FUNCTION).
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(dir.path(), &mut storage, "vcmi", &ComposeOptions::default()).unwrap();

    // No file changes → unchanged → copy-forward path.
    let result =
        refresh_into_storage(dir.path(), &mut storage, "vcmi", &ComposeOptions::default()).unwrap();

    let files = FileCatalogPort::get_files_by_repo(&storage, "vcmi").unwrap();
    assert_eq!(
        language_of(&files, "lib/constants/EntityIdentifiers.h"),
        &Some("cpp".to_string()),
        "refresh copy-forward must not regress the C++ header to c",
    );

    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();
    let hero = node_named(&nodes, "EntityIdentifiers.h", "HeroClassID");
    assert_eq!(
        hero.subtype.as_deref(),
        Some("CLASS"),
        "refresh must preserve the CLASS label",
    );
}
