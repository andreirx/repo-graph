//! BI-1B fixture test: TCP/UDP socket detection integration.
//!
//! Tests the end-to-end path from C fixture files through extraction,
//! emission, and storage to verify that TCP and UDP socket() calls
//! produce the expected boundary interaction surfaces.
//!
//! Fixture: `test/fixtures/tcp-udp-sockets/`
//!
//! ## What this test proves
//!
//! 1. `socket(AF_INET, SOCK_STREAM)` emits as `tcp_socket`
//! 2. `socket(AF_INET, SOCK_DGRAM)` emits as `udp_socket`
//! 3. `socket(AF_INET6, SOCK_DGRAM)` emits as `udp_socket`
//! 4. All surfaces have direction=bidirectional (presence hints only)
//!
//! ## What this test does NOT prove
//!
//! - Role detection (provider vs consumer) - requires fd tracking
//! - Endpoint extraction - requires sockaddr analysis
//! - Scope classification - requires address analysis
//!
//! See `docs/slices/bi-1b-tcp-udp-sockets.md` for partial status.

use std::path::PathBuf;

use repo_graph_boundary_interaction::{
    BoundaryInteractionFilter, BoundaryInteractionReadPort, ChannelKind, Direction,
};
use repo_graph_repo_index::compose::{index_into_storage, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn tcp_udp_fixture_path() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("..")
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("tcp-udp-sockets")
}

#[test]
fn index_tcp_udp_fixture_produces_socket_surfaces() {
    let repo_path = tcp_udp_fixture_path();
    assert!(
        repo_path.join("server.c").exists(),
        "fixture not found at {:?}",
        repo_path
    );

    let mut storage = StorageConnection::open_in_memory().unwrap();
    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "tcp-udp-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Fixture has: server.c, client.c, udp_broadcast.c
    assert_eq!(result.files_total, 3, "expected 3 C fixture files");

    // Query all boundary interaction surfaces
    let filter = BoundaryInteractionFilter::default();
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    // Expected surfaces:
    // - server.c: socket(AF_INET, SOCK_STREAM) → tcp_socket
    // - client.c: socket(AF_INET, SOCK_STREAM) → tcp_socket
    // - udp_broadcast.c: socket(AF_INET, SOCK_DGRAM) → udp_socket (send_broadcast)
    // - udp_broadcast.c: socket(AF_INET6, SOCK_DGRAM) → udp_socket (receive_responses)
    //
    // Note: bind/listen/accept/connect/send/recv/sendto/recvfrom are NOT emitted
    // because they require socket_type context which is not tracked across calls.
    // Only socket() calls produce surfaces with current implementation.
    assert_eq!(
        surfaces.len(),
        4,
        "expected 4 socket() surfaces (2 TCP, 2 UDP); got {:?}",
        surfaces
            .iter()
            .map(|s| format!(
                "{}:{} → {:?}",
                s.source_file, s.line_start, s.channel_kind
            ))
            .collect::<Vec<_>>()
    );

    // Count by channel kind
    let tcp_count = surfaces
        .iter()
        .filter(|s| s.channel_kind == ChannelKind::TcpSocket)
        .count();
    let udp_count = surfaces
        .iter()
        .filter(|s| s.channel_kind == ChannelKind::UdpSocket)
        .count();

    assert_eq!(tcp_count, 2, "expected 2 tcp_socket surfaces");
    assert_eq!(udp_count, 2, "expected 2 udp_socket surfaces");

    // All surfaces should have bidirectional direction (presence hints only)
    for surface in &surfaces {
        assert_eq!(
            surface.direction,
            Direction::Bidirectional,
            "expected bidirectional direction for {}:{} (presence hints only); got {:?}",
            surface.source_file,
            surface.line_start,
            surface.direction
        );
    }
}

#[test]
fn tcp_sockets_from_server_and_client() {
    let repo_path = tcp_udp_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "tcp-udp-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Filter to TCP sockets only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::TcpSocket),
        ..Default::default()
    };
    let tcp_surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    assert_eq!(tcp_surfaces.len(), 2, "expected 2 tcp_socket surfaces");

    // Verify files: one from server.c, one from client.c
    let files: Vec<&str> = tcp_surfaces
        .iter()
        .map(|s| s.source_file.as_str())
        .collect();
    assert!(
        files.contains(&"server.c"),
        "expected tcp_socket from server.c; got files: {:?}",
        files
    );
    assert!(
        files.contains(&"client.c"),
        "expected tcp_socket from client.c; got files: {:?}",
        files
    );
}

#[test]
fn udp_sockets_from_broadcast() {
    let repo_path = tcp_udp_fixture_path();
    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        &repo_path,
        &mut storage,
        "tcp-udp-fixture",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Filter to UDP sockets only
    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::UdpSocket),
        ..Default::default()
    };
    let udp_surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    assert_eq!(udp_surfaces.len(), 2, "expected 2 udp_socket surfaces");

    // Both should be from udp_broadcast.c
    for surface in &udp_surfaces {
        assert_eq!(
            surface.source_file, "udp_broadcast.c",
            "expected udp_socket from udp_broadcast.c; got {}",
            surface.source_file
        );
    }

    // One is AF_INET (send_broadcast), one is AF_INET6 (receive_responses)
    // Both should have InteractionPattern::Datagram
    for surface in &udp_surfaces {
        assert_eq!(
            surface.interaction_pattern,
            repo_graph_boundary_interaction::InteractionPattern::Datagram,
            "expected datagram interaction pattern for UDP socket at {}:{}",
            surface.source_file,
            surface.line_start
        );
    }
}
