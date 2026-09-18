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

use repo_graph_classification::types::UnresolvedEdgeCategory;
use repo_graph_repo_index::compose::{index_path, ComposeOptions};
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
    let imports = storage
        .get_resolved_imports_for_snapshot(snap)
        .expect("resolved imports query");
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
    let imports = storage
        .get_resolved_imports_for_snapshot(snap)
        .expect("resolved imports query");
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
