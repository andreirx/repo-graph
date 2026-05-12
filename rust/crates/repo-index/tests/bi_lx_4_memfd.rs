//! BI-LX-4: memfd_create integration tests.
//!
//! Tests the full pipeline: fixture → C extractor → boundary emitter → storage → query.
//!
//! Note: The fixture also contains mmap/munmap calls which are also shared_memory.
//! Tests filter for memfd-specific surfaces using provenance.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionListItem, BoundaryInteractionReadPort,
    BoundaryScope, ChannelKind, Direction, InteractionPattern,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn memfd_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("..")
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("memfd")
}

/// Filter surfaces to only those with memfd in provenance.
fn filter_memfd_surfaces(
    surfaces: Vec<BoundaryInteractionListItem>,
) -> Vec<BoundaryInteractionListItem> {
    surfaces
        .into_iter()
        .filter(|s| {
            s.provenance
                .as_ref()
                .map(|p| p.contains(":memfd:"))
                .unwrap_or(false)
        })
        .collect()
}

#[test]
fn index_memfd_fixture_produces_shared_memory_surfaces() {
    let repo_path = memfd_fixture_path();
    assert!(
        repo_path.join("memfd_user.c").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "memfd-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: memfd_user.c
    assert_eq!(result.files_total, 1, "expected 1 C fixture file");

    // Query for shared_memory surfaces (memfd uses shared_memory channel kind)
    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::SharedMemory);
    let snapshot = storage
        .get_latest_snapshot("memfd-fixture")
        .unwrap()
        .unwrap();
    let all_surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();

    // Filter to memfd-specific surfaces (excludes mmap/munmap which are also shared_memory)
    let surfaces = filter_memfd_surfaces(all_surfaces);

    // Fixture has 4 memfd_create calls:
    //   create_basic_memfd, create_cloexec_memfd, create_sealable_memfd, create_full_memfd
    assert_eq!(
        surfaces.len(),
        4,
        "expected 4 memfd_create surfaces; got {}",
        surfaces.len()
    );

    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::SharedMemory,
            "all memfd surfaces should be shared_memory"
        );
    }
}

#[test]
fn memfd_surfaces_have_inter_process_scope() {
    let repo_path = memfd_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "memfd-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("memfd-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::SharedMemory);
    let all_surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();
    let surfaces = filter_memfd_surfaces(all_surfaces);

    // All memfd_create surfaces should have inter_process scope
    for surface in &surfaces {
        assert_eq!(
            surface.boundary_scope,
            BoundaryScope::InterProcess,
            "memfd_create surfaces should have inter_process scope"
        );
    }
}

#[test]
fn memfd_surfaces_are_bidirectional() {
    let repo_path = memfd_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "memfd-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("memfd-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::SharedMemory);
    let all_surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();
    let surfaces = filter_memfd_surfaces(all_surfaces);

    // All memfd_create surfaces should be bidirectional
    for surface in &surfaces {
        assert_eq!(
            surface.direction,
            Direction::Bidirectional,
            "memfd_create surfaces should be bidirectional"
        );
    }
}

#[test]
fn memfd_surfaces_have_shared_state_pattern() {
    let repo_path = memfd_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "memfd-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("memfd-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::SharedMemory);
    let all_surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();
    let surfaces = filter_memfd_surfaces(all_surfaces);

    // All memfd_create surfaces should have shared_state interaction pattern
    for surface in &surfaces {
        assert_eq!(
            surface.interaction_pattern,
            InteractionPattern::SharedState,
            "memfd_create surfaces should have shared_state pattern"
        );
    }
}

#[test]
fn memfd_surfaces_have_memfd_provenance() {
    let repo_path = memfd_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "memfd-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("memfd-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::SharedMemory);
    let all_surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();
    let surfaces = filter_memfd_surfaces(all_surfaces);

    // Verify we got the expected number
    assert_eq!(surfaces.len(), 4, "expected 4 memfd_create surfaces");

    // Check provenance contains memfd api_family
    for surface in &surfaces {
        let provenance = surface
            .provenance
            .as_ref()
            .expect("provenance should be set");
        assert!(
            provenance.contains(":memfd:"),
            "memfd surfaces should have memfd api_family in provenance: {}",
            provenance
        );
    }
}
