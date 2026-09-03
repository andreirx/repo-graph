//! Compose-side POSTPASS that promotes C++ gtest/gmock test files to
//! `is_test = true` by the STRUCTURAL marker the cpp-extractor emitted onto each
//! FILE node's `metadata_json` (IS-TEST-CPP-1). Split from `compose.rs` so the
//! storage postpass + its tests do not grow the >500-line orchestration file
//! (mirrors `rust_test_reclassify`).
//!
//! Unlike the Rust postpass, C++ needs NO cross-file resolution: a gtest marker
//! (a `gtest/`/`gmock/` include or a top-level `TEST`/`TEST_F`/`TEST_P`/
//! `TYPED_TEST` macro) is in-file, so the extractor's emitted `is_gtest_test`
//! flag IS the classification. This module owns only the read/promote/write
//! orchestration and its failure contract; the structural detection lives in the
//! cpp-extractor (`gtest_marker`).

use repo_graph_storage::StorageConnection;

use crate::compose::{merge_extraction_diagnostics, ComposeError};

/// IS-TEST-CPP-1: reclassify C++ gtest/gmock test files to `is_test = true` by the
/// structural marker, overriding the path-based `is_test` the orchestrator stamped
/// during extraction.
///
/// Reads the whole snapshot's FILE nodes (FRESH this index + copied-forward on
/// refresh — the schema copy-forward preserves `metadata_json`, so the set is
/// complete on BOTH paths), restricts to C/C++ candidates ([`is_cpp_family_path`]
/// — the only files that can carry the marker), keeps those carrying
/// `is_gtest_test = true`, and PROMOTES them. Promote-only by design (mirrors the
/// ratified Rust postpass and slice §2.3): it never demotes, so every other
/// language and every non-marker C/C++ file keeps its prior classification. A
/// no-marker file is NOT asserted to be a non-test — the slice detects gtest/gmock
/// ONLY (§2.2), so gtest-absence is not test-absence; demoting on it would make a
/// false Layer-0 claim about a non-gtest test.
///
/// A FILE node whose `metadata_json` is malformed JSON (or carries a
/// wrong-typed marker) is a broken evidence record, NOT a license to guess — it
/// is counted as a parse failure, recorded as an extraction diagnostic (honest
/// degradation), and the file keeps its prior classification.
///
/// Returns the number of files promoted (for the caller's `isolate_postpass`
/// contract). FALLIBLE — a storage or diagnostics-persist failure propagates so
/// the caller records it and, if the isolation mechanism itself fails, demotes the
/// snapshot out of READY rather than serving a silently-incomplete classification.
pub(crate) fn reclassify_cpp_test_files(
    storage: &mut StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
) -> Result<usize, ComposeError> {
    // 1. Read all nodes for the snapshot and keep the FILE nodes. `query_all_nodes`
    //    is the existing snapshot node read (no new storage API); a FILE node's
    //    `qualified_name` is its repo-relative path.
    let file_facts: Vec<(String, Option<String>)> =
        <StorageConnection>::query_all_nodes(storage, snapshot_uid)
            .map_err(ComposeError::Storage)?
            .into_iter()
            .filter(|n| n.kind == "FILE")
            .filter_map(|n| n.qualified_name.map(|qn| (qn, n.metadata_json)))
            .collect();

    // 2. Collect the paths carrying the structural gtest marker; count malformed
    //    metadata blobs separately (honest degradation, never a guessed marker).
    //    Only C/C++ FILE nodes are inspected: the `is_gtest_test` marker is
    //    emitted solely by the cpp-extractor, so a non-C/C++ FILE node can carry
    //    neither a marker nor a C++ parse failure. Skipping them keeps other
    //    languages untouched — a malformed metadata blob written by another
    //    extractor is never miscounted as a C++ degradation (review-0 item 3).
    let mut test_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut metadata_parse_failures: u64 = 0;
    for (rel_path, metadata_json) in file_facts {
        if !is_cpp_family_path(&rel_path) {
            continue;
        }
        match parse_gtest_marker(metadata_json.as_deref()) {
            Ok(true) => {
                test_paths.insert(rel_path);
            }
            Ok(false) => {}
            Err(()) => metadata_parse_failures += 1,
        }
    }

    // No markers and no failures → nothing to do. Other languages and every
    // non-test C++ file are byte-stable (neither `files` nor the diagnostics blob
    // is touched). This is the openxcom control-group path.
    if test_paths.is_empty() && metadata_parse_failures == 0 {
        return Ok(0);
    }

    // 3. Promote is_test for the marked files whose stored value is still false.
    //    `upsert_files` is a single transaction — the promotion is atomic (all
    //    promoted rows land or none), so a failure rolls back to the prior
    //    path-based values (the safe fallback) rather than a partial mix.
    let tracked =
        <StorageConnection>::get_files_by_repo(storage, repo_uid).map_err(ComposeError::Storage)?;
    let promoted: Vec<repo_graph_storage::types::TrackedFile> = tracked
        .into_iter()
        .filter(|tf| test_paths.contains(&tf.path) && !tf.is_test)
        .map(|mut tf| {
            tf.is_test = true;
            tf
        })
        .collect();
    let promoted_count = promoted.len();
    if !promoted.is_empty() {
        <StorageConnection>::upsert_files(storage, &promoted).map_err(ComposeError::Storage)?;
    }

    // 4. Record what this full recompute did + any degradation into the snapshot's
    //    extraction-diagnostics blob (SET semantics — a full recompute overwrites,
    //    it does not accumulate across refreshes). Gated so a C++ repo with no test
    //    markers and no parse failures leaves the blob byte-stable.
    if promoted_count > 0 || metadata_parse_failures > 0 {
        merge_extraction_diagnostics(storage, snapshot_uid, |obj| {
            obj.insert(
                "cpp_test_files_promoted".to_string(),
                serde_json::json!(promoted_count as u64),
            );
            obj.insert(
                "cpp_test_metadata_parse_failures".to_string(),
                serde_json::json!(metadata_parse_failures),
            );
        })?;
    }

    Ok(promoted_count)
}

/// A path whose extension routes to the C or C++ extractor — the only files that
/// can carry the cpp-extractor's `is_gtest_test` marker. Uses the AUTHORITATIVE
/// routing predicate ([`repo_graph_indexer::routing::detect_language`]) rather
/// than a local extension list, so the candidate set can never drift from the
/// real routing table.
///
/// `.h` maps to `"c"` here, yet a C++ header carrying C++ markers is
/// content-routed to the cpp-extractor (`routing::route_file_content_aware`) and
/// so DOES receive the gtest marker. Including the `"c"` arm is therefore
/// required to honour `.h` gtest carriers like leveldb's `util/testutil.h`
/// (review-0 item 2); a genuine pure-C `.c`/`.h` file simply never carries the
/// marker (`parse_gtest_marker` → `Ok(false)`), so admitting it is harmless.
fn is_cpp_family_path(rel_path: &str) -> bool {
    matches!(
        repo_graph_indexer::routing::detect_language(rel_path),
        Some("c") | Some("cpp")
    )
}

/// Parse a FILE node's `metadata_json` for the `is_gtest_test` marker.
///
/// - absent blob / absent key → `Ok(false)` (no marker; the common non-test case)
/// - `is_gtest_test: true|false` → `Ok(b)`
/// - malformed JSON, or the key present but not a bool → `Err(())` (broken
///   evidence — counted as a degradation, NEVER silently treated as a marker or
///   as a non-marker classification).
fn parse_gtest_marker(metadata_json: Option<&str>) -> Result<bool, ()> {
    let Some(raw) = metadata_json else {
        return Ok(false);
    };
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|_| ())?;
    match value.get("is_gtest_test") {
        None => Ok(false),
        Some(serde_json::Value::Bool(b)) => Ok(*b),
        Some(_) => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::{index_into_storage, isolate_postpass, ComposeOptions};
    use std::fs;

    /// A minimal C++ repo with ONE gtest test file (`src/thing_test.cc`, a
    /// `TEST_F` with no in-file include — the dominant corpus shape) and one
    /// production file, so the reclassify postpass has exactly one file to promote.
    fn make_cpp_test_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/thing.cc"),
            "int add(int a, int b) { return a + b; }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/thing_test.cc"),
            "TEST_F(ThingSuite, Adds) {\n  EXPECT_EQ(add(1, 2), 3);\n}\n",
        )
        .unwrap();
        dir
    }

    /// The reclassify postpass records its outcome through
    /// `merge_extraction_diagnostics`, so even when the diagnostics column is still
    /// NULL the record is CREATED rather than silently dropped.
    #[test]
    fn reclassify_creates_diagnostics_blob_when_column_null() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_cpp_test_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap = result.snapshot_uid.clone();

        // Reset is_test back so the re-run has a real promotion to record, and force
        // the diagnostics column to NULL. Then re-run the postpass directly.
        storage
            .execute_raw(
                "UPDATE files SET is_test = 0 WHERE repo_uid = 'r1' AND path = 'src/thing_test.cc'",
            )
            .unwrap();
        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = NULL \
                 WHERE snapshot_uid = '{snap}'"
            ))
            .unwrap();

        let promoted = reclassify_cpp_test_files(&mut storage, "r1", &snap).unwrap();
        assert_eq!(promoted, 1, "src/thing_test.cc must be promoted");

        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap)
            .unwrap()
            .expect("reclassify must create the blob even when the column was NULL");
        let v: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            v.get("cpp_test_files_promoted").and_then(|x| x.as_u64()),
            Some(1),
            "the promotion is recorded honestly rather than dropped"
        );
    }

    /// A MALFORMED diagnostics blob must PROPAGATE, never be silently swallowed —
    /// a dropped degradation record is a READY snapshot claiming completeness it
    /// does not have.
    #[test]
    fn reclassify_propagates_malformed_diagnostics_blob() {
        let fixture = make_cpp_test_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap = result.snapshot_uid.clone();

        storage
            .execute_raw(
                "UPDATE files SET is_test = 0 WHERE repo_uid = 'r1' AND path = 'src/thing_test.cc'",
            )
            .unwrap();
        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = 'not-json' \
                 WHERE snapshot_uid = '{snap}'"
            ))
            .unwrap();

        let outcome = reclassify_cpp_test_files(&mut storage, "r1", &snap);
        assert!(
            outcome.is_err(),
            "a degradation record that cannot be written must propagate, not be silently lost"
        );
    }

    /// A reclassify failure whose degradation cannot be recorded either must DEMOTE
    /// the snapshot out of READY (via the exact `isolate_postpass` wiring), never
    /// serve a snapshot with path-derived is_test.
    #[test]
    fn reclassify_failure_demotes_snapshot_out_of_ready() {
        let fixture = make_cpp_test_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap = result.snapshot_uid.clone();

        storage
            .execute_raw(
                "UPDATE files SET is_test = 0 WHERE repo_uid = 'r1' AND path = 'src/thing_test.cc'",
            )
            .unwrap();
        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = 'not-json' \
                 WHERE snapshot_uid = '{snap}'"
            ))
            .unwrap();

        let cpp_test_outcome = reclassify_cpp_test_files(&mut storage, "r1", &snap);
        let wired = isolate_postpass(
            &mut storage,
            &snap,
            "cpp-test-classify",
            "cpp_test_classify_postpass_error",
            cpp_test_outcome,
            |_s| Ok(()),
        );
        assert!(
            wired.is_err(),
            "the compounded infra failure must propagate"
        );

        let status: String = storage
            .query_scalar(&format!(
                "SELECT status FROM snapshots WHERE snapshot_uid = '{snap}'"
            ))
            .unwrap();
        assert_eq!(
            status, "failed",
            "a reclassify failure whose degradation cannot be recorded is demoted out of READY, \
             never served with path-derived is_test"
        );
    }

    #[test]
    fn parse_gtest_marker_cases() {
        assert_eq!(parse_gtest_marker(None), Ok(false));
        assert_eq!(parse_gtest_marker(Some("{}")), Ok(false));
        assert_eq!(
            parse_gtest_marker(Some(r#"{"is_gtest_test":true}"#)),
            Ok(true)
        );
        assert_eq!(
            parse_gtest_marker(Some(r#"{"is_gtest_test":false}"#)),
            Ok(false)
        );
        // extern-C linkage blob without the marker → no marker.
        assert_eq!(
            parse_gtest_marker(Some(
                r#"{"has_extern_c_declarations":true,"extern_c_symbol_count":2}"#
            )),
            Ok(false)
        );
        // merged blob (both facts) → marker.
        assert_eq!(
            parse_gtest_marker(Some(
                r#"{"has_extern_c_declarations":true,"extern_c_symbol_count":2,"is_gtest_test":true}"#
            )),
            Ok(true)
        );
        // broken evidence → degradation, never a silent classification.
        assert_eq!(parse_gtest_marker(Some("not-json")), Err(()));
        assert_eq!(
            parse_gtest_marker(Some(r#"{"is_gtest_test":"yes"}"#)),
            Err(())
        );
    }

    /// The C/C++ candidate guard admits exactly the source/header extensions that
    /// route to the C or C++ extractor (`.h` included — a C++ header is
    /// content-routed to the cpp-extractor) and rejects every other language.
    #[test]
    fn is_cpp_family_path_scopes_to_c_and_cpp() {
        for p in [
            "src/a.cc",
            "src/a.cpp",
            "src/a.cxx",
            "inc/a.hpp",
            "inc/a.hxx",
            "util/testutil.h",
            "src/a.c",
        ] {
            assert!(is_cpp_family_path(p), "{p} must be a C/C++ candidate");
        }
        for p in [
            "src/a.rs",
            "app/main.py",
            "web/app.ts",
            "Main.java",
            "README.md",
        ] {
            assert!(!is_cpp_family_path(p), "{p} must NOT be a C/C++ candidate");
        }
    }

    /// review-0 item 3: a malformed metadata blob on a NON-C/C++ FILE node must
    /// never be counted as a C++ parse failure. The postpass restricts to C/C++
    /// candidates, so an injected `.py` FILE node with unparseable metadata is
    /// skipped — `cpp_test_metadata_parse_failures` stays 0 even though a real
    /// C++ promotion forces the diagnostics blob to be written.
    #[test]
    fn non_cpp_malformed_metadata_is_not_a_cpp_parse_failure() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_cpp_test_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap = result.snapshot_uid.clone();

        // A real C++ promotion to record (forces the diagnostics write path).
        storage
            .execute_raw(
                "UPDATE files SET is_test = 0 WHERE repo_uid = 'r1' AND path = 'src/thing_test.cc'",
            )
            .unwrap();

        // Inject a NON-C/C++ FILE node whose metadata_json is unparseable. If the
        // postpass parsed every FILE node it would count this as a failure; the
        // C/C++ candidate guard must skip it entirely.
        storage
            .execute_raw(&format!(
                "INSERT INTO nodes \
                 (node_uid, snapshot_uid, repo_uid, stable_key, kind, name, qualified_name, metadata_json) \
                 VALUES ('py-1', '{snap}', 'r1', 'r1:app/main.py:FILE', 'FILE', 'main.py', 'app/main.py', 'not-json')"
            ))
            .unwrap();

        let promoted = reclassify_cpp_test_files(&mut storage, "r1", &snap).unwrap();
        assert_eq!(promoted, 1, "the C++ test file is still promoted");

        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap)
            .unwrap()
            .expect("a promotion was recorded");
        let v: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            v.get("cpp_test_metadata_parse_failures")
                .and_then(|x| x.as_u64()),
            Some(0),
            "a malformed blob on a non-C/C++ FILE node must not be a C++ parse failure",
        );
    }
}
