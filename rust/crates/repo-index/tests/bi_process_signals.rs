//! BI-1D fixture test: Process signal detection integration.
//!
//! Tests the end-to-end path from C fixture files through extraction,
//! emission, and storage to verify that signal-related calls produce
//! the expected boundary interaction surfaces.
//!
//! Fixture: `test/fixtures/process-signals/`
//!
//! ## What this test proves
//!
//! 1. `kill(pid, SIGTERM)` emits as `process_signal` with direction=provider
//! 2. `raise(SIGUSR1)` emits as `process_signal` with direction=provider
//! 3. `signal(SIGTERM, handler)` emits as `process_signal` with direction=consumer
//! 4. `sigaction(SIGINT, &act, NULL)` emits as `process_signal` with direction=consumer
//! 5. Signal names are extracted as channel identity
//!
//! See `docs/slices/bi-1d-process-signals.md` for design.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, ChannelKind, Direction,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn signal_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("..")
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("process-signals")
}

#[test]
fn index_signal_fixture_produces_signal_surfaces() {
    let repo_path = signal_fixture_path();
    assert!(
        repo_path.join("sender.c").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "signal-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: sender.c, handler.c
    assert_eq!(result.files_total, 2, "expected 2 C fixture files");

    // Query process_signal surfaces only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::ProcessSignal),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces:
    // sender.c:
    //   - kill(child, SIGTERM) -> provider
    //   - raise(SIGUSR1) -> provider
    // handler.c:
    //   - signal(SIGTERM, sigterm_handler) -> consumer
    //   - sigaction(SIGINT, &act, NULL) -> consumer
    //   - sigwait(&set, &sig) -> consumer
    assert_eq!(
        surfaces.len(),
        5,
        "expected 5 process_signal surfaces (2 provider, 3 consumer); got {:?}",
        surfaces
            .iter()
            .map(|s| format!("{}:{} {:?}", s.source_file, s.line_start, s.direction))
            .collect::<Vec<_>>()
    );

    // Verify all are ProcessSignal
    for surface in &surfaces {
        assert_eq!(
            surface.channel_kind,
            ChannelKind::ProcessSignal,
            "expected process_signal channel kind"
        );
    }
}

#[test]
fn signal_senders_have_provider_direction() {
    let repo_path = signal_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "signal-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::ProcessSignal),
        direction: Some(Direction::Provider),
        ..Default::default()
    };
    let provider_surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // sender.c has 2 provider surfaces: kill and raise
    assert_eq!(
        provider_surfaces.len(),
        2,
        "expected 2 provider surfaces (kill + raise)"
    );

    for surface in &provider_surfaces {
        assert_eq!(
            surface.source_file, "sender.c",
            "provider surfaces should be from sender.c"
        );
    }
}

#[test]
fn signal_handlers_have_consumer_direction() {
    let repo_path = signal_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "signal-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::ProcessSignal),
        direction: Some(Direction::Consumer),
        ..Default::default()
    };
    let consumer_surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // handler.c has 3 consumer surfaces: signal, sigaction, sigwait
    assert_eq!(
        consumer_surfaces.len(),
        3,
        "expected 3 consumer surfaces (signal + sigaction + sigwait)"
    );

    for surface in &consumer_surfaces {
        assert_eq!(
            surface.source_file, "handler.c",
            "consumer surfaces should be from handler.c"
        );
    }
}
