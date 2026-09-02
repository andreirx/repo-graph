//! Compose-side POSTPASS that promotes in-crate Rust test-module files to
//! `is_test = true` by structural `#[cfg(test)]` inclusion evidence
//! (IS-TEST-RUST-1). Split out of `compose.rs` (review-2 item 3) so the storage
//! postpass and its tests do not grow the >500-line orchestration file.
//!
//! This is the STORAGE-touching half of the mechanism: it reads the snapshot's
//! FILE-node inclusion facts (emitted by the rust-extractor onto `metadata_json`),
//! runs the pure cross-file resolver ([`crate::rust_test_classifier`]), and
//! writes the promotions + a degradation diagnostic. The pure classification
//! logic lives in `rust_test_classifier`; this module owns only the read/compute/
//! write orchestration and its failure contract.

use repo_graph_storage::StorageConnection;

use crate::compose::{merge_extraction_diagnostics, ComposeError};

/// IS-TEST-RUST-1: reclassify Rust in-crate test-module files by structural
/// `#[cfg(test)]` inclusion evidence, overriding the path-based `is_test` that
/// the orchestrator stamped during extraction.
///
/// This is the RESOLUTION half of the ratified mechanism. The rust-extractor
/// emitted each file's `mod <name>;` inclusion facts onto its FILE-node
/// `metadata_json`; here we read the whole snapshot's nodes (FRESH files this
/// index + copied-forward FILE nodes on refresh — the schema copy-forward
/// preserves `metadata_json`, so the set is complete on BOTH paths), keep the
/// FILE nodes, walk the cross-file chain (`rust_test_classifier`), and PROMOTE
/// the cfg(test)-reachable files to `is_test = true`. Promote-only by design
/// (see the classifier's module doc): it never demotes, so integration-test /
/// bench / example classification and every other language are untouched.
///
/// Full recomputation over the whole Rust file set (no incremental shortcut —
/// the chain is cross-file), as ratified. Unresolved inclusions and metadata
/// parse failures are recorded as an extraction diagnostic via
/// [`merge_extraction_diagnostics`] (the established honest-degradation channel:
/// NULL blob → create it; malformed/non-object blob → PROPAGATE, never silently
/// dropped — review-1 item 3), never a guessed classification.
///
/// Returns the number of files promoted (for the caller's `isolate_postpass`
/// contract; the value is not otherwise consumed). FALLIBLE — a storage or
/// diagnostics-persist failure propagates so the caller's `isolate_postpass`
/// records it and, if the isolation mechanism itself fails, demotes the snapshot
/// out of READY rather than serving a silently-incomplete classification
/// (review-1 item 4).
pub(crate) fn reclassify_rust_test_files(
    storage: &mut StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
) -> Result<usize, ComposeError> {
    // 1. Read all nodes for the snapshot and keep the FILE nodes. `query_all_nodes`
    //    is the existing snapshot node read (no new storage API — review-1 item 1);
    //    a FILE node's `qualified_name` is its repo-relative path.
    let file_facts: Vec<(String, Option<String>)> =
        <StorageConnection>::query_all_nodes(storage, snapshot_uid)
            .map_err(ComposeError::Storage)?
            .into_iter()
            .filter(|n| n.kind == "FILE")
            .filter_map(|n| n.qualified_name.map(|qn| (qn, n.metadata_json)))
            .collect();

    // 2. Build per-Rust-file inclusion facts. Non-`.rs` files carry no Rust mod
    //    facts and are skipped.
    let mut facts = Vec::new();
    let mut metadata_parse_failures: u64 = 0;
    for (rel_path, metadata_json) in file_facts {
        if !rel_path.ends_with(".rs") {
            continue;
        }
        let (mod_decls, failed) =
            crate::rust_test_classifier::parse_file_mod_decls(metadata_json.as_deref());
        if failed {
            metadata_parse_failures += 1;
        }
        facts.push(crate::rust_test_classifier::RustFileFacts {
            rel_path,
            mod_decls,
        });
    }

    // No Rust in this repo → nothing to do (other languages byte-stable: neither
    // the `files` table nor the diagnostics blob is touched).
    if facts.is_empty() {
        return Ok(0);
    }

    // 3. Structural classification (pure).
    let outcome = crate::rust_test_classifier::classify(&facts);

    // 4. Promote is_test for the cfg(test)-reachable files whose stored value is
    //    still false. `upsert_files` is a single transaction — the promotion is
    //    atomic (all promoted rows land or none), so a failure here rolls back to
    //    the prior path-based values (the safe fallback) rather than a partial mix.
    let tracked =
        <StorageConnection>::get_files_by_repo(storage, repo_uid).map_err(ComposeError::Storage)?;
    let promoted: Vec<repo_graph_storage::types::TrackedFile> = tracked
        .into_iter()
        .filter(|tf| outcome.test_files.contains(&tf.path) && !tf.is_test)
        .map(|mut tf| {
            tf.is_test = true;
            tf
        })
        .collect();
    let promoted_count = promoted.len();
    if !promoted.is_empty() {
        <StorageConnection>::upsert_files(storage, &promoted).map_err(ComposeError::Storage)?;
    }

    // 5. Record what this full recompute did + any degradation into the snapshot's
    //    extraction-diagnostics blob (SET semantics — a full recompute overwrites,
    //    it does not accumulate across refreshes). Gated so a Rust repo with no
    //    test modules and no unresolved inclusions leaves the blob byte-stable.
    let unresolved_count = outcome.unresolved.len() as u64;
    if promoted_count > 0 || unresolved_count > 0 || metadata_parse_failures > 0 {
        merge_extraction_diagnostics(storage, snapshot_uid, |obj| {
            obj.insert(
                "rust_test_files_promoted".to_string(),
                serde_json::json!(promoted_count as u64),
            );
            obj.insert(
                "rust_mod_inclusions_unresolved".to_string(),
                serde_json::json!(unresolved_count),
            );
            obj.insert(
                "rust_mod_metadata_parse_failures".to_string(),
                serde_json::json!(metadata_parse_failures),
            );
        })?;
    }

    Ok(promoted_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::{index_into_storage, isolate_postpass, ComposeOptions};
    use std::fs;

    /// A minimal Rust crate with ONE `#[cfg(test)] mod tests;` inclusion, so the
    /// IS-TEST-RUST-1 reclassify postpass has exactly one file to promote and one
    /// diagnostic to record. Used by the diagnostics-honesty tests below.
    fn make_rust_test_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub fn f() {}\n#[cfg(test)]\nmod tests;\n",
        )
        .unwrap();
        fs::write(root.join("src/tests.rs"), "fn t() {}\n").unwrap();
        dir
    }

    /// IS-TEST-RUST-1 review-1 item 3: the reclassify postpass records its outcome
    /// through `merge_extraction_diagnostics`, so even when the diagnostics column
    /// is still NULL the record is CREATED (an empty object is started and written)
    /// rather than silently dropped — the `if let Some(json_str)` swallow the
    /// iteration-1 code had is gone.
    #[test]
    fn reclassify_creates_diagnostics_blob_when_column_null() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_rust_test_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap = result.snapshot_uid.clone();

        // Reset is_test back to the path-based value so the re-run has a real
        // promotion to record, and force the diagnostics column to NULL. Then
        // re-run the postpass directly.
        storage
            .execute_raw(
                "UPDATE files SET is_test = 0 WHERE repo_uid = 'r1' AND path = 'src/tests.rs'",
            )
            .unwrap();
        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = NULL \
                 WHERE snapshot_uid = '{snap}'"
            ))
            .unwrap();

        let promoted = reclassify_rust_test_files(&mut storage, "r1", &snap).unwrap();
        assert_eq!(promoted, 1, "src/tests.rs must be promoted");

        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap)
            .unwrap()
            .expect("reclassify must create the blob even when the column was NULL");
        let v: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            v.get("rust_test_files_promoted").and_then(|x| x.as_u64()),
            Some(1),
            "the promotion is recorded honestly rather than dropped"
        );
    }

    /// IS-TEST-RUST-1 review-1 item 3: a MALFORMED diagnostics blob must PROPAGATE
    /// (via `merge_extraction_diagnostics`), never be silently swallowed — a
    /// dropped degradation record is a READY snapshot claiming a completeness it
    /// does not have. The caller's `isolate_postpass` turns this propagated error
    /// into a recorded failure or a demotion.
    #[test]
    fn reclassify_propagates_malformed_diagnostics_blob() {
        let fixture = make_rust_test_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap = result.snapshot_uid.clone();

        // A real promotion to record + a malformed blob → the merge is attempted
        // and must FAIL (propagate), not be swallowed.
        storage
            .execute_raw(
                "UPDATE files SET is_test = 0 WHERE repo_uid = 'r1' AND path = 'src/tests.rs'",
            )
            .unwrap();
        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = 'not-json' \
                 WHERE snapshot_uid = '{snap}'"
            ))
            .unwrap();

        let outcome = reclassify_rust_test_files(&mut storage, "r1", &snap);
        assert!(
            outcome.is_err(),
            "a degradation record that cannot be written must propagate, not be silently lost"
        );
    }

    /// IS-TEST-RUST-1 review-1 item 4: the reclassify postpass runs AFTER the
    /// snapshot is READY, so a failure must never leave a servable snapshot with
    /// path-derived is_test. This drives the EXACT call-site wiring (reclassify →
    /// `isolate_postpass`) with a broken degradation channel: the postpass fails,
    /// its error cannot be recorded either, so `isolate_postpass` DEMOTES the
    /// snapshot to `failed` (excluded from `get_latest_snapshot`) and propagates.
    #[test]
    fn reclassify_failure_demotes_snapshot_out_of_ready() {
        let fixture = make_rust_test_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap = result.snapshot_uid.clone();

        // A real promotion to record, then corrupt the diagnostics blob so BOTH
        // the success-path record AND the isolate_postpass error-record fail — the
        // compounded-failure path that must demote out of READY.
        storage
            .execute_raw(
                "UPDATE files SET is_test = 0 WHERE repo_uid = 'r1' AND path = 'src/tests.rs'",
            )
            .unwrap();
        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = 'not-json' \
                 WHERE snapshot_uid = '{snap}'"
            ))
            .unwrap();

        let rust_test_outcome = reclassify_rust_test_files(&mut storage, "r1", &snap);
        let wired = isolate_postpass(
            &mut storage,
            &snap,
            "rust-test-classify",
            "rust_test_classify_postpass_error",
            rust_test_outcome,
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
}
