//! CPP-INCLUDE-ROOTS-1 — end-to-end `#include` resolution through per-module
//! `include/` roots, indexed from source through the FULL stack
//! (RG-REQ-006-L03 / RG-REQ-001-L02 / RG-REQ-002-L01). This is check CIR-C05.
//!
//! WHY THIS LIVES HERE:
//!
//! CIR-C05 requires a test that WRITES a C++ include fixture, INDEXES it, and
//! asserts the RESULTING store's IMPORTS edges and unresolved-edge categories.
//! The `repo-graph-indexer` crate cannot host it — its `include_resolver.rs`
//! unit tests exercise the pure resolution policy, but they index nothing.
//! `repo-graph-repo-index` wires cpp-extractor + indexer + storage and already
//! indexes C++ fixtures end-to-end (see `cpp_sb_1_integration.rs` and
//! `call_binding_receiver.rs`). Together they bracket the seam; nothing is
//! mocked.
//!
//! The RC-10 defect (audit round six): before CPP-INCLUDE-ROOTS-1 the include
//! resolver searched only the three repo-root-anchored literals
//! `include`/`inc`/`src/include`, so poco's `#include "Poco/Exception.h"` from
//! `Net/src/*.cpp` — the header living at `Foundation/include/Poco/Exception.h`
//! — never resolved (13,703 unresolved on poco). The fix DERIVES the roots from
//! the indexed file list: every directory named `include`/`inc` at any depth.
//!
//! Two scenarios, mirroring poco:
//!   (1) a header under a per-module `include/` root resolves.
//!   (2) the SAME header under two per-module `include/` roots is AMBIGUOUS —
//!       counted as `imports_ambiguous_match`, never silently bound to one.

use std::fs;
use std::path::PathBuf;

use repo_graph_classification::import_partition::ImportClass;
use repo_graph_classification::types::{
    UnresolvedEdgeCategory, MODULES_LIST_UNRESOLVED_IMPORT_CATEGORIES,
};
use repo_graph_repo_index::compose::{
    index_into_storage, index_path, refresh_into_storage, ComposeOptions,
};
use repo_graph_storage::StorageConnection;
use repo_graph_trust::storage_port::{CountByClassificationInput, TrustStorageRead};

/// A minimal C++ translation unit that includes a per-module header and uses the
/// type it declares, so the `#include` produces an IMPORTS edge.
const NET_SOURCE: &str = "#include \"Poco/Exception.h\"\n\
void use_it() { Poco::Exception e; (void)e; }\n";

/// The header poco keeps under `Foundation/include/Poco/`.
const EXCEPTION_HEADER: &str = "#pragma once\n\
namespace Poco { class Exception { public: int code() const { return 0; } }; }\n";

fn count_category(
    storage: &StorageConnection,
    snap: &str,
    category: UnresolvedEdgeCategory,
) -> u64 {
    TrustStorageRead::count_unresolved_edges_by_classification(
        storage,
        &CountByClassificationInput {
            snapshot_uid: snap.to_string(),
            filter_categories: vec![category],
        },
    )
    .unwrap()
    .iter()
    .map(|row| row.count)
    .sum()
}

#[test]
fn indexed_cpp_include_resolves_through_nested_include_root() {
    // Foundation/include/Poco/Exception.h  ← the header, under a per-module root
    // Net/src/a.cpp  #include "Poco/Exception.h"
    let dir = tempfile::tempdir().unwrap();
    let repo: PathBuf = dir.path().to_path_buf();
    fs::create_dir_all(repo.join("Foundation/include/Poco")).unwrap();
    fs::write(
        repo.join("Foundation/include/Poco/Exception.h"),
        EXCEPTION_HEADER,
    )
    .unwrap();
    fs::create_dir_all(repo.join("Net/src")).unwrap();
    fs::write(repo.join("Net/src/a.cpp"), NET_SOURCE).unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");
    let result = index_path(&repo, &db_path, "poco-fix", &ComposeOptions::default())
        .expect("indexing the C++ include fixture must succeed");
    let snap = &result.snapshot_uid;
    let storage = StorageConnection::open(&db_path).unwrap();

    // ── (1) exactly one resolved IMPORTS edge Net/src/a.cpp → the header ──
    // TEST-EDGE-SCOPE-1B: the partitioned file-level read, certain rows.
    let imports: Vec<_> = storage
        .file_imports_with_partition(snap)
        .expect("resolved imports query")
        .into_iter()
        .filter(|i| i.partition.class == ImportClass::Certain)
        .collect();
    let from_a: Vec<&str> = imports
        .iter()
        .filter(|i| i.source_file_uid == "poco-fix:Net/src/a.cpp")
        .map(|i| i.target_file_uid.as_str())
        .collect();
    assert_eq!(
        from_a,
        vec!["poco-fix:Foundation/include/Poco/Exception.h"],
        "the #include \"Poco/Exception.h\" must resolve through the derived \
         Foundation/include root; resolved targets from Net/src/a.cpp = {from_a:?}"
    );

    // ── (2) nothing lands in imports_file_not_found (the RC-10 defect basis) ──
    assert_eq!(
        count_category(&storage, snap, UnresolvedEdgeCategory::ImportsFileNotFound),
        0,
        "the include resolved, so no imports_file_not_found row remains"
    );
    // ── (3) it is not ambiguous either (a single derived root holds it) ──
    assert_eq!(
        count_category(
            &storage,
            snap,
            UnresolvedEdgeCategory::ImportsAmbiguousMatch
        ),
        0,
        "a single derived root ⇒ resolved, not ambiguous"
    );
}

#[test]
fn indexed_cpp_include_ambiguous_across_two_include_roots_is_counted_not_bound() {
    // Foundation/include/Poco/Exception.h  ┐ same header under TWO per-module
    // Util/include/Poco/Exception.h        ┘ include roots
    // Net/src/b.cpp  #include "Poco/Exception.h"  → ambiguous, counted, unbound
    let dir = tempfile::tempdir().unwrap();
    let repo: PathBuf = dir.path().to_path_buf();
    fs::create_dir_all(repo.join("Foundation/include/Poco")).unwrap();
    fs::write(
        repo.join("Foundation/include/Poco/Exception.h"),
        EXCEPTION_HEADER,
    )
    .unwrap();
    fs::create_dir_all(repo.join("Util/include/Poco")).unwrap();
    fs::write(repo.join("Util/include/Poco/Exception.h"), EXCEPTION_HEADER).unwrap();
    fs::create_dir_all(repo.join("Net/src")).unwrap();
    fs::write(repo.join("Net/src/b.cpp"), NET_SOURCE).unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");
    let result = index_path(&repo, &db_path, "poco-ambig", &ComposeOptions::default())
        .expect("indexing the ambiguous C++ include fixture must succeed");
    let snap = &result.snapshot_uid;
    let storage = StorageConnection::open(&db_path).unwrap();

    // ── (1) NO resolved IMPORTS edge from Net/src/b.cpp — never a silent pick ──
    // TEST-EDGE-SCOPE-1B: the partitioned file-level read, certain rows.
    let imports: Vec<_> = storage
        .file_imports_with_partition(snap)
        .expect("resolved imports query")
        .into_iter()
        .filter(|i| i.partition.class == ImportClass::Certain)
        .collect();
    let from_b: Vec<&str> = imports
        .iter()
        .filter(|i| i.source_file_uid == "poco-ambig:Net/src/b.cpp")
        .map(|i| i.target_file_uid.as_str())
        .collect();
    assert!(
        from_b.is_empty(),
        "an include present under two derived roots must NOT bind to either; \
         resolved targets from Net/src/b.cpp = {from_b:?}"
    );

    // ── (2) exactly one counted imports_ambiguous_match row ──
    assert_eq!(
        count_category(
            &storage,
            snap,
            UnresolvedEdgeCategory::ImportsAmbiguousMatch
        ),
        1,
        "the ambiguous include is preserved as one counted imports_ambiguous_match row"
    );
    // ── (3) it is NOT recorded as file-not-found (it WAS found, just ambiguously) ──
    assert_eq!(
        count_category(&storage, snap, UnresolvedEdgeCategory::ImportsFileNotFound),
        0,
        "an ambiguous include is imports_ambiguous_match, never imports_file_not_found"
    );
}

// ── CPP-INCLUDE-BASENAME-1 (RG-REQ-006-L11, RG-REQ-002-L11, RG-REQ-002-L02) ──────────────
//
// An include no earlier stage resolves binds `static` on a unique path suffix of two or more
// segments, `inferred` (with its one candidate and the reason) on a unique basename, and stays
// unresolved as `imports_ambiguous_match` with every candidate otherwise. The stores below are
// read through a second, raw SQLite connection: storage diagnostics of what the pipeline wrote.

/// An IMPORTS edge as stored: (source file path, target stable key, resolution, carrier).
type StoredImport = (String, String, String, serde_json::Value);
/// An unresolved IMPORTS row as stored: (source file path, target key, category, carrier).
type StoredUnresolvedImport = (String, String, String, serde_json::Value);

fn write(repo: &std::path::Path, rel: &str, content: &str) {
    let path = repo.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn carrier_of(raw: Option<String>) -> serde_json::Value {
    serde_json::from_str(raw.as_deref().expect("a stored carrier")).expect("a JSON carrier")
}

fn stored_imports(db: &std::path::Path, snap: &str) -> Vec<StoredImport> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sf.path, tn.stable_key, e.resolution, e.metadata_json
               FROM edges e
               JOIN nodes sn ON sn.node_uid = e.source_node_uid
               JOIN nodes tn ON tn.node_uid = e.target_node_uid
               JOIN files sf ON sf.file_uid = sn.file_uid
              WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS' AND sn.kind = 'FILE'
              ORDER BY sf.path, tn.stable_key",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap();
    rows.map(|r| {
        let (src, tgt, res, md) = r.unwrap();
        (src, tgt, res, carrier_of(md))
    })
    .collect()
}

fn stored_unresolved_imports(db: &std::path::Path, snap: &str) -> Vec<StoredUnresolvedImport> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sf.path, u.target_key, u.category, u.metadata_json
               FROM unresolved_edges u
               JOIN nodes sn ON sn.node_uid = u.source_node_uid
               JOIN files sf ON sf.file_uid = sn.file_uid
              WHERE u.snapshot_uid = ? AND u.type = 'IMPORTS'
              ORDER BY sf.path, u.target_key",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap();
    rows.map(|r| {
        let (src, tk, cat, md) = r.unwrap();
        (src, tk, cat, carrier_of(md))
    })
    .collect()
}

/// The persisted MODULE → MODULE import pairs, as (source module key, target module key).
fn module_import_pairs(db: &std::path::Path, snap: &str) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT sn.stable_key, tn.stable_key
               FROM edges e
               JOIN nodes sn ON sn.node_uid = e.source_node_uid
               JOIN nodes tn ON tn.node_uid = e.target_node_uid
              WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS'
                AND sn.kind = 'MODULE' AND tn.kind = 'MODULE'
              ORDER BY 1, 2",
        )
        .unwrap();
    let rows = stmt
        .query_map([snap], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn nginx_style_single_segment_include_resolves_inferred_by_unique_basename() {
    // nginx keeps its headers beside their sources, with no `include/` directory.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(repo, "src/core/ngx_core.h", "typedef int ngx_int_t;\n");
    write(
        repo,
        "src/event/ngx_event.c",
        "#include <ngx_core.h>\n#include <ngx_time.h>\nint ngx_event(void) { return 0; }\n",
    );
    write(repo, "src/os/unix/ngx_time.h", "typedef long ngx_msec_t;\n");
    write(
        repo,
        "src/os/win32/ngx_time.h",
        "typedef long ngx_msec_t;\n",
    );

    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "ngx", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;

    let imports: Vec<_> = stored_imports(&db, snap)
        .into_iter()
        .filter(|i| i.0 == "src/event/ngx_event.c")
        .collect();
    assert_eq!(
        imports,
        vec![(
            "src/event/ngx_event.c".to_string(),
            "ngx:src/core/ngx_core.h:FILE".to_string(),
            "inferred".to_string(),
            serde_json::json!({
                "isTypeOnly": false,
                "basis": "unique_basename",
                "candidates": ["ngx:src/core/ngx_core.h:FILE"],
            }),
        )],
        "ONE inferred edge to the one file of that basename, its candidate and reason recorded \
         and the extractor's keys kept"
    );

    let unresolved: Vec<_> = stored_unresolved_imports(&db, snap)
        .into_iter()
        .filter(|u| u.0 == "src/event/ngx_event.c")
        .collect();
    assert_eq!(
        unresolved,
        vec![(
            "src/event/ngx_event.c".to_string(),
            "ngx_time.h".to_string(),
            "imports_ambiguous_match".to_string(),
            serde_json::json!({
                "isTypeOnly": false,
                "basis": "ambiguous_basename",
                "candidates": [
                    "ngx:src/os/unix/ngx_time.h:FILE",
                    "ngx:src/os/win32/ngx_time.h:FILE",
                ],
            }),
        )],
        "ONE unresolved ambiguous row with both candidates, never a pick"
    );

    assert!(
        !module_import_pairs(&db, snap).contains(&(
            "ngx:src/event:MODULE".to_string(),
            "ngx:src/core:MODULE".to_string()
        )),
        "an inferred include never feeds the persisted module graph"
    );
}

#[test]
fn unique_multi_segment_suffix_include_resolves_static() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "app/main.cpp",
        "#include \"util/foo.h\"\nint main() { return foo(); }\n",
    );
    write(
        repo,
        "lib/src/util/foo.h",
        "inline int foo() { return 0; }\n",
    );

    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "sfx", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;

    let imports: Vec<_> = stored_imports(&db, snap)
        .into_iter()
        .filter(|i| i.0 == "app/main.cpp")
        .collect();
    assert_eq!(
        imports,
        vec![(
            "app/main.cpp".to_string(),
            "sfx:lib/src/util/foo.h:FILE".to_string(),
            "static".to_string(),
            serde_json::json!({
                "rawPath": "./util/foo.h",
                "isTypeOnly": false,
                "basis": "unique_suffix",
                "candidates": ["sfx:lib/src/util/foo.h:FILE"],
            }),
        )],
        "ONE static edge: a unique two-segment suffix is a certain fact; the extractor's keys \
         (`rawPath`, `isTypeOnly`) are kept"
    );
    assert!(
        module_import_pairs(&db, snap).contains(&(
            "sfx:app:MODULE".to_string(),
            "sfx:lib/src/util:MODULE".to_string()
        )),
        "a certain include feeds the persisted module graph: {:?}",
        module_import_pairs(&db, snap)
    );
}

#[test]
fn refresh_turns_an_inferred_unique_basename_include_ambiguous_when_a_second_header_is_added() {
    // D-CIB-REFRESH-1 = A: a refresh re-resolves every include of the snapshot, copied-forward
    // ones included; only the extraction is copied forward.
    use repo_graph_indexer::storage_port::FileCatalogPort;

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    write(
        repo,
        "src/app/main.c",
        "#include <foo.h>\nint main(void) { return 0; }\n",
    );
    write(repo, "src/lib/foo.h", "typedef int foo_t;\n");

    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let mut storage = StorageConnection::open(&db).unwrap();
    let r1 = index_into_storage(repo, &mut storage, "rf", &ComposeOptions::default()).unwrap();
    let from_main = |snap: &str| -> Vec<StoredImport> {
        stored_imports(&db, snap)
            .into_iter()
            .filter(|i| i.0 == "src/app/main.c")
            .collect()
    };
    let before = from_main(&r1.snapshot_uid);
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].1, "rf:src/lib/foo.h:FILE");
    assert_eq!(before[0].2, "inferred");
    assert_eq!(before[0].3["basis"], "unique_basename");

    // A second `foo.h` appears; main.c is unchanged.
    write(repo, "src/other/foo.h", "typedef int foo_t;\n");
    let r2 = refresh_into_storage(repo, &mut storage, "rf", &ComposeOptions::default()).unwrap();

    let h1 = FileCatalogPort::query_file_version_hashes(&storage, &r1.snapshot_uid).unwrap();
    let h2 = FileCatalogPort::query_file_version_hashes(&storage, &r2.snapshot_uid).unwrap();
    assert_eq!(
        h1.get("rf:src/app/main.c"),
        h2.get("rf:src/app/main.c"),
        "main.c is unchanged across the two snapshots (copied forward)"
    );
    assert!(h1.contains_key("rf:src/app/main.c"));

    assert!(
        from_main(&r2.snapshot_uid).is_empty(),
        "the refresh re-resolved the copied-forward include: no edge remains"
    );
    let unresolved: Vec<_> = stored_unresolved_imports(&db, &r2.snapshot_uid)
        .into_iter()
        .filter(|u| u.0 == "src/app/main.c")
        .collect();
    assert_eq!(
        unresolved,
        vec![(
            "src/app/main.c".to_string(),
            "foo.h".to_string(),
            "imports_ambiguous_match".to_string(),
            serde_json::json!({
                "isTypeOnly": false,
                "basis": "ambiguous_basename",
                "candidates": ["rf:src/lib/foo.h:FILE", "rf:src/other/foo.h:FILE"],
            }),
        )]
    );
}

#[test]
fn trust_unresolved_import_figure_equals_the_modules_list_headline_on_one_snapshot() {
    // D-CIB-COUNT-1 = A: one unresolved import in each of three IMPORTS categories.
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    // imports_file_not_found: no indexed file of that name.
    write(
        repo,
        "src/a.c",
        "#include \"missing.h\"\nint a(void) { return 0; }\n",
    );
    // imports_ambiguous_match: two indexed files of that basename.
    write(
        repo,
        "src/b.c",
        "#include <dup.h>\nint b(void) { return 0; }\n",
    );
    write(repo, "x/dup.h", "typedef int dup_t;\n");
    write(repo, "y/dup.h", "typedef int dup_t;\n");
    // imports_wildcard: a Java package import.
    write(
        repo,
        "src/main/java/com/acme/app/App.java",
        "package com.acme.app;\n\nimport com.acme.*;\n\npublic class App {}\n",
    );

    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("test.db");
    let result = index_path(repo, &db, "seam", &ComposeOptions::default()).unwrap();
    let snap = &result.snapshot_uid;
    let storage = StorageConnection::open(&db).unwrap();

    for (category, want) in [
        (UnresolvedEdgeCategory::ImportsFileNotFound, 1),
        (UnresolvedEdgeCategory::ImportsAmbiguousMatch, 1),
        (UnresolvedEdgeCategory::ImportsWildcard, 1),
    ] {
        assert_eq!(
            count_category(&storage, snap, category),
            want,
            "{category:?}"
        );
    }
    let headline: u64 = TrustStorageRead::count_unresolved_edges_by_classification(
        &storage,
        &CountByClassificationInput {
            snapshot_uid: snap.to_string(),
            filter_categories: MODULES_LIST_UNRESOLVED_IMPORT_CATEGORIES.to_vec(),
        },
    )
    .unwrap()
    .iter()
    .map(|row| row.count)
    .sum();
    assert_eq!(headline, 3, "the modules-list headline's read");

    let report = repo_graph_trust::assemble_trust_report(&storage, "seam", snap, None, None)
        .expect("trust report");
    assert_eq!(
        report.summary.reliability.import_graph.reasons,
        vec![format!("unresolved_imports={headline}")],
        "trust's unresolved-import figure is the headline's number on the same snapshot"
    );
}
