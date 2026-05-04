//! MB-2A fixture test: Kafka / kafkajs boundary detection.
//!
//! Tests the end-to-end path from TS fixture files through extraction,
//! emission, and storage to verify that kafkajs patterns produce
//! the expected boundary interaction surfaces.
//!
//! Fixture: `test/fixtures/kafka-basic/`
//!
//! ## What this test proves
//!
//! 1. `producer.send({ topic, ... })` emits as `kafka_topic` with direction=provider
//! 2. `consumer.subscribe({ topic })` emits as `kafka_topic` with direction=consumer
//!
//! ## What is NOT detected (by design)
//!
//! - Files without kafkajs import (scope guard)
//! - Calls without extractable topic evidence (topic evidence guard)
//! - `consumer.run(...)` — deferred to future correlation with subscribe()
//! - Generic .send()/.subscribe() on non-Kafka objects
//! - Kafka connection/admin operations
//!
//! See `docs/slices/mb-2a-kafka-topic.md` for design.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, ChannelKind, Direction,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn kafka_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("..")
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("kafka-basic")
}

#[test]
fn index_kafka_fixture_produces_kafka_surfaces() {
    let repo_path = kafka_fixture_path();
    assert!(
        repo_path.join("producer.ts").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "kafka-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: producer.ts, consumer.ts
    assert_eq!(result.files_total, 2, "expected 2 TS fixture files");

    // Query kafka_topic surfaces only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::KafkaTopic),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces:
    //   producer.ts: send = 1 (with topic evidence)
    //   consumer.ts: subscribe = 1 (with topic evidence)
    //   consumer.ts: run = 0 (no topic evidence — intentionally not detected)
    // Total: 2 surfaces
    assert_eq!(
        surfaces.len(),
        2,
        "expected 2 kafka_topic surfaces (send + subscribe); got {} from {:?}",
        surfaces.len(),
        surfaces
            .iter()
            .map(|s| format!(
                "{}:{} {} {:?}",
                s.source_file, s.line_start, s.channel_kind.as_str(), s.direction
            ))
            .collect::<Vec<_>>()
    );

    // Verify all are KafkaTopic
    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::KafkaTopic,
            "expected kafka_topic channel kind"
        );
    }
}

#[test]
fn kafka_provider_consumer_roles_detected() {
    let repo_path = kafka_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "kafka-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::KafkaTopic),
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
    // - 1 provider: send (producer.ts)
    // - 1 consumer: subscribe (consumer.ts)
    // Note: run() is NOT detected — no topic evidence
    assert_eq!(
        providers.len(),
        1,
        "expected 1 provider surface (send); got {}",
        providers.len()
    );
    assert_eq!(
        consumers.len(),
        1,
        "expected 1 consumer surface (subscribe only — run excluded); got {}",
        consumers.len()
    );
}

#[test]
fn kafka_surfaces_have_message_broker_family() {
    let repo_path = kafka_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "kafka-fixture",
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

    // All KafkaTopic surfaces should have message_broker family
    // Expected: 2 surfaces (send + subscribe — run excluded)
    assert_eq!(
        surfaces.len(),
        2,
        "expected 2 message_broker family surfaces; got {}",
        surfaces.len()
    );

    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::KafkaTopic,
            "message_broker family should contain kafka_topic surfaces"
        );
    }
}

#[test]
fn kafka_surfaces_have_publish_subscribe_pattern() {
    let repo_path = kafka_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "kafka-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::KafkaTopic),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // All Kafka surfaces should have publish_subscribe interaction pattern
    for surface in &surfaces {
        assert_eq!(
            surface.interaction_pattern.as_str(),
            "publish_subscribe",
            "kafka surfaces should have publish_subscribe pattern, got: {}",
            surface.interaction_pattern.as_str()
        );
    }
}
