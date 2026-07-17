//! EC-M3B-TRUST-AGG-1 — persisted resolved-call aggregate parity (g1).
//!
//! The slice's self-validating parity window: while CALLS rows exist, the
//! persisted snapshot-level resolved-call count MUST equal the live
//! `COUNT(*)` over CALLS rows — asserted through the REAL disk-to-SQLite
//! pipeline on BOTH a fresh index and a delta refresh (copy-forward path
//! exercised, not just fresh).
//!
//! Both sides of every parity assertion go through the production read
//! port (`TrustStorageRead`): the persisted side via
//! `get_resolved_call_aggregate`, the live side via `count_edges_by_type`
//! — the exact accounting the trust core used before M-3b.

use std::fs;
use std::path::Path;

use repo_graph_repo_index::compose::{index_into_storage, refresh_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;
use repo_graph_trust::TrustStorageRead;

/// Two-file TS repo whose CALLS edges come from the UNCHANGED file, so a
/// delta refresh exercises copy-forward + full re-resolution for the
/// CALLS-carrying extraction edges.
fn make_calls_repo(dir: &Path) {
    fs::write(dir.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    // Intra-file function→function calls: the most reliably extracted
    // CALLS shape (no cross-file resolution required).
    fs::write(
        dir.join("src/util.ts"),
        "export function helper() { return 1; }\n\
         export function caller() { return helper(); }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/other.ts"),
        "export function other() { return 2; }\n",
    )
    .unwrap();
}

/// Assert persisted-aggregate ↔ live-COUNT parity for one snapshot and
/// return the agreed count.
fn assert_parity(storage: &StorageConnection, snapshot_uid: &str, context: &str) -> u64 {
    let live =
        TrustStorageRead::count_edges_by_type(storage, snapshot_uid, "CALLS").expect("live COUNT");
    let aggregate = TrustStorageRead::get_resolved_call_aggregate(storage, snapshot_uid)
        .expect("aggregate read")
        .unwrap_or_else(|| panic!("{context}: pipeline snapshot must carry a persisted aggregate"));
    assert_eq!(
        aggregate.count, live,
        "{context}: persisted resolved-call aggregate must equal the live CALLS COUNT \
         (parity window violated)"
    );
    assert_eq!(
        aggregate.provenance, "pipeline",
        "{context}: the ratified interim-rule provenance label must be stamped"
    );
    live
}

#[test]
fn fresh_index_persists_parity_equal_aggregate() {
    let dir = tempfile::tempdir().unwrap();
    make_calls_repo(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result =
        index_into_storage(dir.path(), &mut storage, "r1", &ComposeOptions::default()).unwrap();

    let count = assert_parity(&storage, &result.snapshot_uid, "fresh index");
    // Guard against a vacuous 0 == 0 pass: the fixture must actually
    // produce resolved CALLS rows.
    assert!(
        count > 0,
        "fixture must produce at least one resolved CALLS edge (got 0 — \
         the parity assertion above would be vacuous)"
    );
}

#[test]
fn delta_refresh_recomputes_aggregate_with_copy_forward_exercised() {
    let dir = tempfile::tempdir().unwrap();
    make_calls_repo(dir.path());

    let mut storage = StorageConnection::open_in_memory().unwrap();

    // Phase 1: full index.
    let r1 =
        index_into_storage(dir.path(), &mut storage, "r1", &ComposeOptions::default()).unwrap();
    let fresh_count = assert_parity(&storage, &r1.snapshot_uid, "fresh index (pre-refresh)");
    assert!(fresh_count > 0, "fixture must produce CALLS edges");

    // Phase 2: change ONLY other.ts — util.ts (the CALLS carrier) stays
    // unchanged, so its extraction edges ride the delta copy-forward and
    // are re-resolved into the child snapshot.
    fs::write(
        dir.path().join("src/other.ts"),
        "export function other() { return 3; }\n",
    )
    .unwrap();

    // Phase 3: delta refresh.
    let r2 =
        refresh_into_storage(dir.path(), &mut storage, "r1", &ComposeOptions::default()).unwrap();

    // Prove this exercised the DELTA path, not the no-parent full-index
    // fallback (else the copy-forward claim below is untested).
    let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
    assert_eq!(snap2.kind, "refresh", "delta path must be exercised");
    assert_eq!(
        snap2.parent_snapshot_uid,
        Some(r1.snapshot_uid.clone()),
        "refresh snapshot must link to the parent"
    );

    // The child snapshot's aggregate: recomputed from the full re-resolved
    // stream (copied-forward + fresh), parity-equal, provenance-labeled.
    let refresh_count = assert_parity(&storage, &r2.snapshot_uid, "delta refresh");
    assert_eq!(
        refresh_count, fresh_count,
        "unchanged CALLS carrier ⇒ same resolved-call count across refresh"
    );

    // The PARENT snapshot's aggregate is untouched by the refresh (its
    // rows remain readable by pinned uid — the W-B rule; the aggregate
    // stays parity-equal against them).
    assert_parity(&storage, &r1.snapshot_uid, "parent after refresh");
}
