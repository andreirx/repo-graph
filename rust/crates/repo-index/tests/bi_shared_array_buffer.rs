//! BI-1C fixture test: SharedArrayBuffer/Atomics boundary detection.
//!
//! Tests the end-to-end path from TS fixture files through extraction,
//! emission, and storage to verify that SAB/Atomics patterns produce
//! the expected boundary interaction surfaces.
//!
//! Fixture: `rust/crates/repo-index/tests/fixtures/shared-array-buffer/`
//!
//! ## What this test proves
//!
//! 1. `new SharedArrayBuffer(...)` emits as `shared_array_buffer` with direction=provider
//! 2. `Atomics.wait(...)` emits as `shared_array_buffer` with direction=consumer
//! 3. `Atomics.notify(...)` emits as `shared_array_buffer` with direction=provider
//! 4. `Atomics.store(...)` emits as `shared_array_buffer` with direction=bidirectional
//! 5. `Atomics.load(...)` emits as `shared_array_buffer` with direction=bidirectional
//!
//! ## What is NOT detected (Option A decision)
//!
//! - `new Worker(...)` — generic worker creation, no SAB correlation
//! - `worker.postMessage(...)` — generic message passing, no SAB in arguments proven
//!
//! See `docs/slices/bi-1c-shared-array-buffer.md` for design.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, BoundaryScope, ChannelKind, Direction,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn sab_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("fixtures")
        .join("shared-array-buffer")
}

#[test]
fn index_sab_fixture_produces_sab_surfaces() {
    let repo_path = sab_fixture_path();
    assert!(
        repo_path.join("main.ts").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sab-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: main.ts, worker.ts
    assert_eq!(result.files_total, 2, "expected 2 TS fixture files");

    // Query shared_array_buffer surfaces only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::SharedArrayBuffer),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces from main.ts (after Option A — no Worker/postMessage):
    //   - new SharedArrayBuffer(...) -> provider
    //   - Atomics.store(...) x2 -> bidirectional
    //   - Atomics.notify(...) -> provider
    // Expected surfaces from worker.ts:
    //   - Atomics.wait(...) -> consumer
    //   - Atomics.load(...) -> bidirectional
    //   - Atomics.store(...) -> bidirectional
    // Total: 7 surfaces (4 from main.ts, 3 from worker.ts)
    assert!(
        surfaces.len() >= 6,
        "expected at least 6 shared_array_buffer surfaces; got {} from {:?}",
        surfaces.len(),
        surfaces
            .iter()
            .map(|s| format!(
                "{}:{} {} {:?}",
                s.source_file,
                s.line_start,
                s.channel_kind.as_str(),
                s.direction
            ))
            .collect::<Vec<_>>()
    );

    // Verify all are SharedArrayBuffer
    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::SharedArrayBuffer,
            "expected shared_array_buffer channel kind"
        );
    }
}

#[test]
fn sab_surfaces_have_intra_process_scope() {
    let repo_path = sab_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sab-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::SharedArrayBuffer),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All SharedArrayBuffer surfaces should have intra_process scope
    // (same OS process, different V8 isolates)
    for surface in &surfaces {
        assert_eq!(
            surface.boundary_scope,
            BoundaryScope::IntraProcess,
            "SharedArrayBuffer surfaces should have intra_process scope"
        );
    }
}

#[test]
fn sab_provider_consumer_roles_detected() {
    let repo_path = sab_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sab-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::SharedArrayBuffer),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Count by direction
    let providers: Vec<_> = surfaces
        .iter()
        .filter(|s| s.direction == Direction::Provider)
        .collect();
    let consumers: Vec<_> = surfaces
        .iter()
        .filter(|s| s.direction == Direction::Consumer)
        .collect();
    let bidirectional: Vec<_> = surfaces
        .iter()
        .filter(|s| s.direction == Direction::Bidirectional)
        .collect();

    // After Option A (no Worker/postMessage), should have at least:
    // - 2 providers: SharedArrayBuffer, Atomics.notify
    // - 1 consumer: Atomics.wait
    // - 3 bidirectional: Atomics.store (x3), Atomics.load
    assert!(
        providers.len() >= 2,
        "expected at least 2 provider surfaces (SAB, notify); got {}",
        providers.len()
    );
    assert!(
        !consumers.is_empty(),
        "expected at least 1 consumer surface (wait); got {}",
        consumers.len()
    );
    assert!(
        bidirectional.len() >= 3,
        "expected at least 3 bidirectional surfaces (store x3, load); got {}",
        bidirectional.len()
    );
}
