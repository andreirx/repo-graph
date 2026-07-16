//! MB-1A fixture test: AMQP / RabbitMQ boundary detection.
//!
//! Tests the end-to-end path from TS fixture files through extraction,
//! emission, and storage to verify that amqplib patterns produce
//! the expected boundary interaction surfaces.
//!
//! Fixture: `rust/crates/repo-index/tests/fixtures/amqp-basic/`
//!
//! ## What this test proves
//!
//! 1. `channel.assertQueue(...)` emits as `amqp_queue` with direction=bidirectional
//! 2. `channel.sendToQueue(...)` emits as `amqp_queue` with direction=provider
//! 3. `channel.consume(...)` emits as `amqp_queue` with direction=consumer
//! 4. `channel.assertExchange(...)` emits as `amqp_queue` with direction=bidirectional
//! 5. `channel.publish(...)` emits as `amqp_queue` with direction=provider
//!
//! ## What is NOT detected (by design)
//!
//! - Connection creation (lower priority)
//! - Channel creation (infrastructure, not boundary)
//! - Publisher confirms (depth, not breadth)
//!
//! See `docs/slices/mb-1a-rabbitmq-amqp.md` for design.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, ChannelKind, Direction,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn amqp_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("tests").join("fixtures").join("amqp-basic")
}

#[test]
fn index_amqp_fixture_produces_amqp_surfaces() {
    let repo_path = amqp_fixture_path();
    assert!(
        repo_path.join("producer.ts").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "amqp-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: producer.ts, consumer.ts, publisher.ts
    assert_eq!(result.files_total, 3, "expected 3 TS fixture files");

    // Query amqp_queue surfaces only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::AmqpQueue),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces:
    //   producer.ts: assertQueue + sendToQueue = 2
    //   consumer.ts: assertQueue + consume = 2
    //   publisher.ts: assertExchange + publish = 2
    // Total: 6 surfaces
    assert!(
        surfaces.len() >= 6,
        "expected at least 6 amqp_queue surfaces; got {} from {:?}",
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

    // Verify all are AmqpQueue
    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::AmqpQueue,
            "expected amqp_queue channel kind"
        );
    }
}

#[test]
fn amqp_provider_consumer_roles_detected() {
    let repo_path = amqp_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "amqp-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::AmqpQueue),
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

    // Should have at least:
    // - 3 providers: sendToQueue (producer.ts), publish (publisher.ts) + sendToQueue
    // - 1 consumer: consume (consumer.ts)
    // - 3 bidirectional: assertQueue (x2), assertExchange
    assert!(
        providers.len() >= 2,
        "expected at least 2 provider surfaces (sendToQueue, publish); got {}",
        providers.len()
    );
    assert!(
        !consumers.is_empty(),
        "expected at least 1 consumer surface (consume); got {}",
        consumers.len()
    );
    assert!(
        bidirectional.len() >= 3,
        "expected at least 3 bidirectional surfaces (assertQueue, assertExchange); got {}",
        bidirectional.len()
    );
}

#[test]
fn amqp_surfaces_have_message_broker_family() {
    let repo_path = amqp_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "amqp-fixture",
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

    // All AmqpQueue surfaces should have message_broker family
    assert!(
        surfaces.len() >= 6,
        "expected at least 6 message_broker family surfaces; got {}",
        surfaces.len()
    );

    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::AmqpQueue,
            "message_broker family should contain amqp_queue surfaces"
        );
    }
}
