//! BI-EM-1: Inter-core messaging (mailbox + RPMsg) integration tests.
//!
//! Tests the full pipeline: fixture → C extractor → boundary emitter → storage → query.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, BoundaryScope, ChannelKind, Direction,
    InteractionPattern,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn inter_core_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("tests").join("fixtures").join("inter_core")
}

#[test]
fn index_inter_core_fixture_produces_inter_core_channel_surfaces() {
    let repo_path = inter_core_fixture_path();
    assert!(
        repo_path.join("mailbox_user.c").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "inter-core-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: mailbox_user.c, rpmsg_user.c
    assert_eq!(result.files_total, 2, "expected 2 C fixture files");

    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::InterCoreChannel);
    let snapshot = storage
        .get_latest_snapshot("inter-core-fixture")
        .unwrap()
        .unwrap();
    let surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();

    // Mailbox: 6 functions (mbox_request_channel, mbox_request_channel_byname,
    //          mbox_free_channel, mbox_send_message, mbox_client_txdone, mbox_client_peek_data)
    // RPMsg: 8 functions (rpmsg_create_ept, rpmsg_destroy_ept, rpmsg_send, rpmsg_sendto,
    //        rpmsg_send_offchannel, rpmsg_trysend, rpmsg_trysendto, rpmsg_register_device)
    // Note: rpmsg_recv does not exist in Linux kernel API - receive is callback-based.
    // Total: 14 surfaces
    assert!(
        surfaces.len() >= 14,
        "expected at least 14 inter_core_channel surfaces; got {}",
        surfaces.len()
    );

    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::InterCoreChannel,
            "all surfaces should be inter_core_channel"
        );
    }
}

#[test]
fn mailbox_surfaces_detected_in_fixture() {
    let repo_path = inter_core_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "inter-core-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("inter-core-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new()
        .with_channel_kind(ChannelKind::InterCoreChannel)
        .with_file("mailbox_user.c".to_string());
    let surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();

    // 6 mailbox functions in the fixture
    assert_eq!(
        surfaces.len(),
        6,
        "expected 6 mailbox surfaces; got {}",
        surfaces.len()
    );

    // Check provenance contains mailbox api_family
    for surface in &surfaces {
        let provenance = surface
            .provenance
            .as_ref()
            .expect("provenance should be set");
        assert!(
            provenance.contains(":mailbox:"),
            "mailbox surfaces should have mailbox api_family in provenance: {}",
            provenance
        );
    }
}

#[test]
fn rpmsg_surfaces_detected_in_fixture() {
    let repo_path = inter_core_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "inter-core-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("inter-core-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new()
        .with_channel_kind(ChannelKind::InterCoreChannel)
        .with_file("rpmsg_user.c".to_string());
    let surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();

    // 8 rpmsg functions in the fixture (rpmsg_recv does not exist - receive is callback-based)
    assert_eq!(
        surfaces.len(),
        8,
        "expected 8 rpmsg surfaces; got {}",
        surfaces.len()
    );

    // Check provenance contains rpmsg api_family
    for surface in &surfaces {
        let provenance = surface
            .provenance
            .as_ref()
            .expect("provenance should be set");
        assert!(
            provenance.contains(":rpmsg:"),
            "rpmsg surfaces should have rpmsg api_family in provenance: {}",
            provenance
        );
    }
}

#[test]
fn inter_core_surfaces_have_unknown_scope() {
    let repo_path = inter_core_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "inter-core-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("inter-core-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::InterCoreChannel);
    let surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();

    // All inter_core_channel surfaces should have unknown scope
    // (per BI-EM-1 design: same-SoC inter-core doesn't fit inter_process or inter_device)
    for surface in &surfaces {
        assert_eq!(
            surface.boundary_scope,
            BoundaryScope::Unknown,
            "inter_core_channel surfaces should have unknown scope (per BI-EM-1 design)"
        );
    }
}

#[test]
fn inter_core_surfaces_have_correct_directions() {
    let repo_path = inter_core_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "inter-core-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("inter-core-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::InterCoreChannel);
    let surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();

    // Count directions
    let provider_count = surfaces
        .iter()
        .filter(|s| s.direction == Direction::Provider)
        .count();
    let consumer_count = surfaces
        .iter()
        .filter(|s| s.direction == Direction::Consumer)
        .count();
    let bidirectional_count = surfaces
        .iter()
        .filter(|s| s.direction == Direction::Bidirectional)
        .count();

    // Mailbox: mbox_send_message, mbox_client_txdone = provider (2)
    //          mbox_client_peek_data = consumer (1)
    //          mbox_request_channel, mbox_request_channel_byname, mbox_free_channel = bidirectional (3)
    // RPMsg: rpmsg_send, rpmsg_sendto, rpmsg_send_offchannel, rpmsg_trysend, rpmsg_trysendto = provider (5)
    //        (no consumer - rpmsg_recv does not exist, receive is callback-based)
    //        rpmsg_create_ept, rpmsg_destroy_ept, rpmsg_register_device = bidirectional (3)
    // Total: provider=7, consumer=1, bidirectional=6
    assert_eq!(provider_count, 7, "expected 7 provider surfaces");
    assert_eq!(
        consumer_count, 1,
        "expected 1 consumer surface (mbox_client_peek_data only)"
    );
    assert_eq!(bidirectional_count, 6, "expected 6 bidirectional surfaces");
}

#[test]
fn inter_core_surfaces_have_message_passing_or_fire_and_forget_pattern() {
    let repo_path = inter_core_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();
    index_into_storage(
        &repo_path,
        &mut storage,
        "inter-core-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let snapshot = storage
        .get_latest_snapshot("inter-core-fixture")
        .unwrap()
        .unwrap();
    let filter = BoundaryInteractionFilter::new().with_channel_kind(ChannelKind::InterCoreChannel);
    let surfaces = storage
        .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        .unwrap();

    // All inter_core_channel surfaces should have message_passing or fire_and_forget pattern
    for surface in &surfaces {
        assert!(
            surface.interaction_pattern == InteractionPattern::MessagePassing
                || surface.interaction_pattern == InteractionPattern::FireAndForget,
            "inter_core_channel surfaces should have message_passing or fire_and_forget pattern; got {:?}",
            surface.interaction_pattern
        );
    }
}
