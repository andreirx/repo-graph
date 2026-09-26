//! TOOLCHAIN-STALENESS-1 (RG-REQ-001-L06, P-TS-01): every full index and every refresh stamps
//! exactly the running toolchain — the stamp the daemon compares against comes from the one
//! extractor set both write paths compose (`repo_graph_repo_index::toolchain::ExtractorSet`).
//!
//! The orchestrator builds the stamp from the ports it receives, so byte-equality with
//! `running_toolchain_json()` on both paths also proves the port order is the composition order.

use std::fs;

use repo_graph_repo_index::compose::{index_into_storage, refresh_into_storage, ComposeOptions};
use repo_graph_repo_index::toolchain::running_toolchain_json;
use repo_graph_storage::StorageConnection;

fn write_fixture(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(dir.join("src/lib.rs"), "pub fn a() -> u32 { 1 }\n").unwrap();
    fs::write(
        dir.join("src/util.ts"),
        "export function u(): number { return 2; }\n",
    )
    .unwrap();
}

fn stamp_of(storage: &StorageConnection, snapshot_uid: &str) -> Option<String> {
    storage
        .get_snapshot(snapshot_uid)
        .unwrap()
        .expect("snapshot row exists")
        .toolchain_json
}

#[test]
fn full_index_stamps_exactly_the_running_toolchain() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result =
        index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    assert_eq!(
        stamp_of(&storage, &result.snapshot_uid).as_deref(),
        Some(running_toolchain_json().as_str())
    );
}

#[test]
fn refresh_stamps_exactly_the_running_toolchain() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let mut storage = StorageConnection::open_in_memory().unwrap();
    let full =
        index_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    // One edit, so the refresh extracts one file and copies the other forward.
    fs::write(dir.path().join("src/lib.rs"), "pub fn a() -> u32 { 3 }\n").unwrap();
    let refreshed =
        refresh_into_storage(dir.path(), &mut storage, "fx", &ComposeOptions::default()).unwrap();
    assert_ne!(refreshed.snapshot_uid, full.snapshot_uid);
    assert_eq!(
        stamp_of(&storage, &refreshed.snapshot_uid).as_deref(),
        Some(running_toolchain_json().as_str())
    );
}
