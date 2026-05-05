//! BI-LX-1 fixture test: SysV shared memory detection.
//!
//! Tests the end-to-end path from C fixture files through extraction,
//! emission, and storage to verify that SysV shared memory patterns produce
//! the expected boundary interaction surfaces.
//!
//! Fixture: `test/fixtures/sysv-shared-memory/`
//!
//! ## What this test proves
//!
//! 1. `shmget(key, size, flags)` emits as `shared_memory` with api_family=sysv_shm
//! 2. `shmat(shmid, addr, flags)` emits as `shared_memory` with api_family=sysv_shm
//! 3. `shmdt(addr)` emits as `shared_memory` with api_family=sysv_shm
//! 4. `shmctl(shmid, cmd, buf)` emits as `shared_memory` with api_family=sysv_shm
//!
//! ## Surface properties verified
//!
//! - `channel_kind = shared_memory`
//! - `boundary_scope = inter_process`
//! - `interaction_pattern = shared_state`
//! - `direction = bidirectional`
//!
//! See `docs/slices/bi-lx-1-sysv-shared-memory.md` for scope definition.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, ChannelKind, Direction,
    InteractionPattern, BoundaryScope,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn sysv_shm_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("..")
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("sysv-shared-memory")
}

#[test]
fn index_sysv_shm_fixture_produces_shared_memory_surfaces() {
    let repo_path = sysv_shm_fixture_path();
    assert!(
        repo_path.join("creator.c").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-shm-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: creator.c, worker.c, cleanup.c
    assert_eq!(result.files_total, 3, "expected 3 C fixture files");

    // Query shared_memory surfaces only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::SharedMemory),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces:
    //   creator.c: shmget + shmat + shmdt = 3
    //   worker.c: shmget + shmat + shmdt = 3
    //   cleanup.c: shmget + shmctl = 2
    // Total: 8 surfaces
    assert_eq!(
        surfaces.len(),
        8,
        "expected 8 shared_memory surfaces; got {} from {:?}",
        surfaces.len(),
        surfaces
            .iter()
            .map(|s| format!(
                "{}:{} {} {:?}",
                s.source_file, s.line_start, s.channel_kind.as_str(), s.direction
            ))
            .collect::<Vec<_>>()
    );

    // Verify all are SharedMemory
    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::SharedMemory,
            "expected shared_memory channel kind"
        );
    }
}

#[test]
fn sysv_shm_surfaces_have_inter_process_scope() {
    let repo_path = sysv_shm_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-shm-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::SharedMemory),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All SysV shm surfaces should have inter_process scope
    for surface in &surfaces {
        assert_eq!(
            surface.boundary_scope,
            BoundaryScope::InterProcess,
            "sysv_shm surfaces should have inter_process scope, got: {:?}",
            surface.boundary_scope
        );
    }
}

#[test]
fn sysv_shm_surfaces_have_shared_state_pattern() {
    let repo_path = sysv_shm_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-shm-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::SharedMemory),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All SysV shm surfaces should have shared_state interaction pattern
    for surface in &surfaces {
        assert_eq!(
            surface.interaction_pattern,
            InteractionPattern::SharedState,
            "sysv_shm surfaces should have shared_state pattern, got: {:?}",
            surface.interaction_pattern
        );
    }
}

#[test]
fn sysv_shm_surfaces_are_bidirectional() {
    let repo_path = sysv_shm_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-shm-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::SharedMemory),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All SysV shm surfaces should be bidirectional (per slice design)
    for surface in &surfaces {
        assert_eq!(
            surface.direction,
            Direction::Bidirectional,
            "sysv_shm surfaces should be bidirectional, got: {:?}",
            surface.direction
        );
    }
}

#[test]
fn sysv_shm_surfaces_per_file() {
    let repo_path = sysv_shm_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-shm-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::SharedMemory),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Count surfaces per file
    let creator_count = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("creator.c"))
        .count();
    let worker_count = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("worker.c"))
        .count();
    let cleanup_count = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("cleanup.c"))
        .count();

    // Expected counts based on fixture:
    // creator.c: shmget + shmat + shmdt = 3
    // worker.c: shmget + shmat + shmdt = 3
    // cleanup.c: shmget + shmctl = 2
    assert_eq!(creator_count, 3, "expected 3 surfaces from creator.c");
    assert_eq!(worker_count, 3, "expected 3 surfaces from worker.c");
    assert_eq!(cleanup_count, 2, "expected 2 surfaces from cleanup.c");
}
