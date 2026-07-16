//! MB-3A fixture test: NATS boundary detection.
//!
//! Tests the end-to-end path from TS fixture files through extraction,
//! emission, and storage to verify that nats npm package patterns produce
//! the expected boundary interaction surfaces.
//!
//! Fixture: `rust/crates/repo-index/tests/fixtures/nats-basic/`
//!
//! ## What this test proves
//!
//! 1. `nc.publish(subject, ...)` emits as `nats_subject` with direction=provider
//! 2. `nc.subscribe(subject, ...)` emits as `nats_subject` with direction=consumer
//!
//! ## What is NOT detected (by design)
//!
//! - Files without nats import (scope guard)
//! - Calls without extractable subject evidence (subject evidence guard)
//! - `nc.request(...)` — deferred to MB-3B (mixed semantics)
//! - Generic .publish()/.subscribe() on non-NATS objects
//! - JetStream, queue groups, wildcards
//!
//! See `docs/ROADMAP.md` MB-3A entry for scope definition.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, ChannelKind, Direction,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn nats_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("tests").join("fixtures").join("nats-basic")
}

#[test]
fn index_nats_fixture_produces_nats_surfaces() {
    let repo_path = nats_fixture_path();
    assert!(
        repo_path.join("publisher.ts").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "nats-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: publisher.ts, subscriber.ts
    assert_eq!(result.files_total, 2, "expected 2 TS fixture files");

    // Query nats_subject surfaces only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::NatsSubject),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces:
    //   publisher.ts: publish = 1 (with subject evidence)
    //   subscriber.ts: subscribe = 1 (with subject evidence)
    // Total: 2 surfaces
    assert_eq!(
        surfaces.len(),
        2,
        "expected 2 nats_subject surfaces (publish + subscribe); got {} from {:?}",
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

    // Verify all are NatsSubject
    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::NatsSubject,
            "expected nats_subject channel kind"
        );
    }
}

#[test]
fn nats_provider_consumer_roles_detected() {
    let repo_path = nats_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "nats-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::NatsSubject),
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

    // Should have exactly:
    // - 1 provider: publish (publisher.ts)
    // - 1 consumer: subscribe (subscriber.ts)
    assert_eq!(
        providers.len(),
        1,
        "expected 1 provider surface (publish); got {}",
        providers.len()
    );
    assert_eq!(
        consumers.len(),
        1,
        "expected 1 consumer surface (subscribe); got {}",
        consumers.len()
    );
}

#[test]
fn nats_surfaces_have_message_broker_family() {
    let repo_path = nats_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "nats-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Query by message_broker family
    let filter = BoundaryInteractionFilter {
        protocol_family: Some(repo_graph_boundary_interaction::ProtocolFamily::MessageBroker),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All NatsSubject surfaces should have message_broker family
    // Expected: 2 surfaces (publish + subscribe)
    assert_eq!(
        surfaces.len(),
        2,
        "expected 2 message_broker family surfaces; got {}",
        surfaces.len()
    );

    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::NatsSubject,
            "message_broker family should contain nats_subject surfaces"
        );
    }
}

#[test]
fn nats_surfaces_have_publish_subscribe_pattern() {
    let repo_path = nats_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "nats-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::NatsSubject),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All NATS surfaces should have publish_subscribe interaction pattern
    for surface in &surfaces {
        assert_eq!(
            surface.interaction_pattern.as_str(),
            "publish_subscribe",
            "nats surfaces should have publish_subscribe pattern, got: {}",
            surface.interaction_pattern.as_str()
        );
    }
}
