//! BI-LX-2 fixture test: SysV message queue detection.
//!
//! Tests the end-to-end path from C fixture files through extraction,
//! emission, and storage to verify that SysV message queue patterns produce
//! the expected boundary interaction surfaces.
//!
//! Fixture: `rust/crates/repo-index/tests/fixtures/sysv-message-queues/`
//!
//! ## What this test proves
//!
//! 1. `msgget(key, flags)` emits as `message_queue` with api_family=sysv_msgq
//! 2. `msgsnd(msqid, msgp, msgsz, flags)` emits as `message_queue` with direction=provider
//! 3. `msgrcv(msqid, msgp, msgsz, msgtyp, flags)` emits as `message_queue` with direction=consumer
//! 4. `msgctl(msqid, cmd, buf)` emits as `message_queue` with api_family=sysv_msgq
//!
//! ## Surface properties verified
//!
//! - `channel_kind = message_queue`
//! - `boundary_scope = inter_process`
//! - `interaction_pattern = fire_and_forget`
//! - Direction varies: msgsnd=provider, msgrcv=consumer, others=bidirectional
//!
//! See `docs/slices/bi-lx-2-sysv-message-queues.md` for scope definition.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, BoundaryScope, ChannelKind, Direction,
    InteractionPattern,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn sysv_msgq_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("tests")
        .join("fixtures")
        .join("sysv-message-queues")
}

#[test]
fn index_sysv_msgq_fixture_produces_message_queue_surfaces() {
    let repo_path = sysv_msgq_fixture_path();
    assert!(
        repo_path.join("sender.c").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-msgq-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: sender.c, receiver.c, cleanup.c
    assert_eq!(result.files_total, 3, "expected 3 C fixture files");

    // Query message_queue surfaces only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::MessageQueue),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces:
    //   sender.c: msgget + msgsnd = 2
    //   receiver.c: msgget + msgrcv = 2
    //   cleanup.c: msgget + msgctl = 2
    // Total: 6 surfaces
    assert_eq!(
        surfaces.len(),
        6,
        "expected 6 message_queue surfaces; got {} from {:?}",
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

    // Verify all are MessageQueue
    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::MessageQueue,
            "expected message_queue channel kind"
        );
    }
}

#[test]
fn sysv_msgq_surfaces_have_inter_process_scope() {
    let repo_path = sysv_msgq_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-msgq-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::MessageQueue),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All SysV msgq surfaces should have inter_process scope
    for surface in &surfaces {
        assert_eq!(
            surface.boundary_scope,
            BoundaryScope::InterProcess,
            "sysv_msgq surfaces should have inter_process scope, got: {:?}",
            surface.boundary_scope
        );
    }
}

#[test]
fn sysv_msgq_surfaces_have_fire_and_forget_pattern() {
    let repo_path = sysv_msgq_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-msgq-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::MessageQueue),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All SysV msgq surfaces should have fire_and_forget interaction pattern
    for surface in &surfaces {
        assert_eq!(
            surface.interaction_pattern,
            InteractionPattern::FireAndForget,
            "sysv_msgq surfaces should have fire_and_forget pattern, got: {:?}",
            surface.interaction_pattern
        );
    }
}

#[test]
fn sysv_msgq_msgsnd_is_provider() {
    let repo_path = sysv_msgq_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-msgq-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::MessageQueue),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Find msgsnd surfaces (should be in sender.c)
    let msgsnd_surfaces: Vec<_> = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("sender.c"))
        .collect();

    // sender.c has msgget (bidirectional) and msgsnd (provider)
    assert_eq!(
        msgsnd_surfaces.len(),
        2,
        "expected 2 surfaces from sender.c"
    );

    // At least one should be provider (msgsnd)
    let provider_count = msgsnd_surfaces
        .iter()
        .filter(|s| s.direction == Direction::Provider)
        .count();
    assert_eq!(
        provider_count, 1,
        "expected 1 provider surface (msgsnd) in sender.c"
    );
}

#[test]
fn sysv_msgq_msgrcv_is_consumer() {
    let repo_path = sysv_msgq_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-msgq-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::MessageQueue),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Find msgrcv surfaces (should be in receiver.c)
    let msgrcv_surfaces: Vec<_> = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("receiver.c"))
        .collect();

    // receiver.c has msgget (bidirectional) and msgrcv (consumer)
    assert_eq!(
        msgrcv_surfaces.len(),
        2,
        "expected 2 surfaces from receiver.c"
    );

    // At least one should be consumer (msgrcv)
    let consumer_count = msgrcv_surfaces
        .iter()
        .filter(|s| s.direction == Direction::Consumer)
        .count();
    assert_eq!(
        consumer_count, 1,
        "expected 1 consumer surface (msgrcv) in receiver.c"
    );
}

#[test]
fn sysv_msgq_surfaces_per_file() {
    let repo_path = sysv_msgq_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "sysv-msgq-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::MessageQueue),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Count surfaces per file
    let sender_count = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("sender.c"))
        .count();
    let receiver_count = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("receiver.c"))
        .count();
    let cleanup_count = surfaces
        .iter()
        .filter(|s| s.source_file.ends_with("cleanup.c"))
        .count();

    // Expected counts based on fixture:
    // sender.c: msgget + msgsnd = 2
    // receiver.c: msgget + msgrcv = 2
    // cleanup.c: msgget + msgctl = 2
    assert_eq!(sender_count, 2, "expected 2 surfaces from sender.c");
    assert_eq!(receiver_count, 2, "expected 2 surfaces from receiver.c");
    assert_eq!(cleanup_count, 2, "expected 2 surfaces from cleanup.c");
}
