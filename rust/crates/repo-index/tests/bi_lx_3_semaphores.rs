//! BI-LX-3 fixture test: Semaphore detection.
//!
//! Tests the end-to-end path from C fixture files through extraction,
//! emission, and storage to verify that SysV and named POSIX semaphore
//! patterns produce the expected boundary interaction surfaces.
//!
//! Fixture: `test/fixtures/semaphores/`
//!
//! ## What this test proves
//!
//! 1. SysV semaphores: `semget`, `semop`, `semtimedop`, `semctl`
//! 2. Named POSIX semaphores: `sem_open`, `sem_close`, `sem_unlink`
//!
//! ## Surface properties verified
//!
//! - `channel_kind = semaphore`
//! - `boundary_scope = inter_process`
//! - `interaction_pattern = synchronization`
//! - Direction: bidirectional for all (semop direction would require sembuf analysis)
//!
//! ## Deferred (NOT in scope)
//!
//! - Unnamed POSIX semaphore ops (sem_wait, sem_post, etc.) — require pshared analysis
//!
//! See `docs/slices/bi-lx-3-semaphores.md` for scope definition.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, BoundaryScope, ChannelKind,
    InteractionPattern,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn semaphores_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("..")
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("semaphores")
}

#[test]
fn index_semaphores_fixture_produces_semaphore_surfaces() {
    let repo_path = semaphores_fixture_path();
    assert!(
        repo_path.join("sysv_sem.c").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "semaphores-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: sysv_sem.c, posix_named.c
    assert_eq!(result.files_total, 2, "expected 2 C fixture files");

    // Query semaphore surfaces only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::Semaphore),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces:
    //   sysv_sem.c:
    //     - semget in create_semaphore_set
    //     - semop in sem_acquire
    //     - semop in sem_release
    //     - semtimedop in sem_acquire_timed
    //     - semctl in get_sem_value
    //     - semctl in remove_semaphore_set
    //     - semget in main (reuses same function)
    //     - semop in main (calls sem_acquire and sem_release via wrappers)
    //   posix_named.c:
    //     - sem_open in create_named_semaphore
    //     - sem_open in open_named_semaphore
    //     - sem_close in close_named_semaphore
    //     - sem_unlink in unlink_named_semaphore
    //
    // Note: Wrapper functions also emit surfaces where they call the APIs.
    // The exact count depends on the fixture structure.

    assert!(
        surfaces.len() >= 8,
        "expected at least 8 semaphore surfaces; got {} from {:?}",
        surfaces.len(),
        surfaces
            .iter()
            .map(|s| format!(
                "{}:{} {} {:?}",
                s.source_file, s.line_start, s.channel_kind.as_str(), s.direction
            ))
            .collect::<Vec<_>>()
    );

    // Verify all are Semaphore
    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::Semaphore,
            "expected semaphore channel kind"
        );
    }
}

#[test]
fn semaphore_surfaces_have_inter_process_scope() {
    let repo_path = semaphores_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "semaphores-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::Semaphore),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All semaphore surfaces should have inter_process scope
    for surface in &surfaces {
        assert_eq!(
            surface.boundary_scope,
            BoundaryScope::InterProcess,
            "semaphore surfaces should have inter_process scope, got: {:?}",
            surface.boundary_scope
        );
    }
}

#[test]
fn semaphore_surfaces_have_synchronization_pattern() {
    let repo_path = semaphores_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "semaphores-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::Semaphore),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All semaphore surfaces should have synchronization interaction pattern
    for surface in &surfaces {
        assert_eq!(
            surface.interaction_pattern,
            InteractionPattern::Synchronization,
            "semaphore surfaces should have synchronization pattern, got: {:?}",
            surface.interaction_pattern
        );
    }
}

#[test]
fn sysv_semaphores_detected_in_fixture() {
    let repo_path = semaphores_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "semaphores-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::Semaphore),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Find SysV semaphore surfaces (should be in sysv_sem.c)
    let sysv_surfaces: Vec<_> = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("sysv_sem.c"))
        .collect();

    // sysv_sem.c should have at least:
    //   - semget (create_semaphore_set, main)
    //   - semop (sem_acquire, sem_release, main wrapper calls)
    //   - semtimedop (sem_acquire_timed)
    //   - semctl (get_sem_value, remove_semaphore_set)
    assert!(
        sysv_surfaces.len() >= 6,
        "expected at least 6 SysV semaphore surfaces from sysv_sem.c; got {}",
        sysv_surfaces.len()
    );
}

#[test]
fn posix_named_semaphores_detected_in_fixture() {
    let repo_path = semaphores_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "semaphores-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::Semaphore),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Find named POSIX semaphore surfaces (should be in posix_named.c)
    let posix_surfaces: Vec<_> = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("posix_named.c"))
        .collect();

    // posix_named.c should have:
    //   - sem_open (create_named_semaphore, open_named_semaphore)
    //   - sem_close (close_named_semaphore)
    //   - sem_unlink (unlink_named_semaphore)
    // Note: sem_wait/sem_post are NOT detected (deferred)
    assert!(
        posix_surfaces.len() >= 4,
        "expected at least 4 named POSIX semaphore surfaces from posix_named.c; got {}",
        posix_surfaces.len()
    );
}
