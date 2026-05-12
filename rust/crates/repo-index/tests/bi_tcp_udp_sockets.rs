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
//! 4. BI-1B Phase 2: Role detection from fd tracking:
//!    - TCP server (bind+listen) → direction=Provider
//!    - TCP client (connect) → direction=Consumer
//!    - UDP → direction=Bidirectional (no strong role semantics)
//!
//! ## What this test does NOT prove
//!
//! - Endpoint extraction - requires sockaddr analysis
//! - Scope classification - requires address analysis
//!
//! See `docs/slices/bi-1b-tcp-udp-sockets.md` for Phase 2 design.

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

    // BI-1B Phase 2: Verify role detection from fd tracking
    for surface in &surfaces {
        let expected_direction = match (surface.source_file.as_str(), surface.channel_kind) {
            // TCP server: bind+listen → Provider
            ("server.c", ChannelKind::TcpSocket) => Direction::Provider,
            // TCP client: connect → Consumer
            ("client.c", ChannelKind::TcpSocket) => Direction::Consumer,
            // UDP: no strong role semantics → Bidirectional
            (_, ChannelKind::UdpSocket) => Direction::Bidirectional,
            // Fallback
            _ => Direction::Bidirectional,
        };

        assert_eq!(
            surface.direction, expected_direction,
            "BI-1B Phase 2: expected {:?} direction for {}:{} ({:?}); got {:?}",
            expected_direction,
            surface.source_file,
            surface.line_start,
            surface.channel_kind,
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

// ── BI-1B Phase 2: Refresh integration tests ────────────────────────────────

use std::fs;
use tempfile::TempDir;

fn create_temp_repo_with_files(files: &[(&str, &str)]) -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    for (name, content) in files {
        let path = temp_dir.path().join(name);
        fs::write(&path, content).unwrap();
    }
    temp_dir
}

#[test]
fn bi1b_refresh_preserves_unchanged_file_role_detection() {
    // Test that role detection survives refresh when files are unchanged.
    let temp_dir = create_temp_repo_with_files(&[
        ("server.c", r#"
            #include <sys/socket.h>
            void server() {
                int fd = socket(AF_INET, SOCK_STREAM, 0);
                bind(fd, (struct sockaddr*)&addr, sizeof(addr));
                listen(fd, 5);
            }
        "#),
    ]);

    let mut storage = StorageConnection::open_in_memory().unwrap();

    // Initial index
    let result1 = index_into_storage(
        temp_dir.path(),
        &mut storage,
        "bi1b-refresh-test",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::TcpSocket),
        ..Default::default()
    };
    let surfaces1 = storage
        .list_boundary_interactions(&result1.snapshot_uid, &filter)
        .unwrap();

    assert_eq!(surfaces1.len(), 1, "expected 1 tcp_socket surface after initial index");
    assert_eq!(
        surfaces1[0].direction,
        Direction::Provider,
        "expected Provider direction after initial index"
    );

    // Refresh without changes
    let result2 = index_into_storage(
        temp_dir.path(),
        &mut storage,
        "bi1b-refresh-test",
        &ComposeOptions::default(),
    )
    .unwrap();

    let surfaces2 = storage
        .list_boundary_interactions(&result2.snapshot_uid, &filter)
        .unwrap();

    assert_eq!(surfaces2.len(), 1, "expected 1 tcp_socket surface after refresh");
    assert_eq!(
        surfaces2[0].direction,
        Direction::Provider,
        "Provider direction must survive refresh"
    );
}

#[test]
fn bi1b_refresh_mixed_changed_unchanged_files() {
    // Test refresh with one changed file and one unchanged file.
    let temp_dir = create_temp_repo_with_files(&[
        ("server.c", r#"
            #include <sys/socket.h>
            void server() {
                int fd = socket(AF_INET, SOCK_STREAM, 0);
                bind(fd, (struct sockaddr*)&addr, sizeof(addr));
                listen(fd, 5);
            }
        "#),
        ("client.c", r#"
            #include <sys/socket.h>
            void client() {
                int fd = socket(AF_INET, SOCK_STREAM, 0);
                connect(fd, (struct sockaddr*)&addr, sizeof(addr));
            }
        "#),
    ]);

    let mut storage = StorageConnection::open_in_memory().unwrap();

    // Initial index
    let result1 = index_into_storage(
        temp_dir.path(),
        &mut storage,
        "bi1b-mixed-test",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::TcpSocket),
        ..Default::default()
    };
    let surfaces1 = storage
        .list_boundary_interactions(&result1.snapshot_uid, &filter)
        .unwrap();

    assert_eq!(surfaces1.len(), 2, "expected 2 tcp_socket surfaces after initial index");

    // Modify client.c only (add a comment)
    let client_path = temp_dir.path().join("client.c");
    let client_content = fs::read_to_string(&client_path).unwrap();
    fs::write(&client_path, format!("// modified\n{}", client_content)).unwrap();

    // Refresh
    let result2 = index_into_storage(
        temp_dir.path(),
        &mut storage,
        "bi1b-mixed-test",
        &ComposeOptions::default(),
    )
    .unwrap();

    let surfaces2 = storage
        .list_boundary_interactions(&result2.snapshot_uid, &filter)
        .unwrap();

    assert_eq!(surfaces2.len(), 2, "expected 2 tcp_socket surfaces after refresh");

    // Verify both directions survived
    let server_surface = surfaces2.iter().find(|s| s.source_file == "server.c").unwrap();
    let client_surface = surfaces2.iter().find(|s| s.source_file == "client.c").unwrap();

    assert_eq!(
        server_surface.direction,
        Direction::Provider,
        "server.c Provider direction must survive refresh (unchanged file)"
    );
    assert_eq!(
        client_surface.direction,
        Direction::Consumer,
        "client.c Consumer direction must be re-detected after file change"
    );
}

#[test]
fn bi1b_refresh_no_duplicate_surfaces() {
    // Test that refresh doesn't create duplicate surfaces.
    let temp_dir = create_temp_repo_with_files(&[
        ("server.c", r#"
            #include <sys/socket.h>
            void server() {
                int fd = socket(AF_INET, SOCK_STREAM, 0);
                bind(fd, (struct sockaddr*)&addr, sizeof(addr));
                listen(fd, 5);
            }
        "#),
    ]);

    let mut storage = StorageConnection::open_in_memory().unwrap();

    // Initial index
    let result1 = index_into_storage(
        temp_dir.path(),
        &mut storage,
        "bi1b-dedup-test",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter::default();
    let surfaces1 = storage
        .list_boundary_interactions(&result1.snapshot_uid, &filter)
        .unwrap();

    let count1 = surfaces1.len();

    // Refresh multiple times
    for _ in 0..3 {
        let result = index_into_storage(
            temp_dir.path(),
            &mut storage,
            "bi1b-dedup-test",
            &ComposeOptions::default(),
        )
        .unwrap();

        let surfaces = storage
            .list_boundary_interactions(&result.snapshot_uid, &filter)
            .unwrap();

        assert_eq!(
            surfaces.len(),
            count1,
            "surface count must not increase on refresh"
        );
    }
}

#[test]
fn bi1b_udp_connect_stays_bidirectional() {
    // Critical test: UDP connect() must NOT become Consumer.
    let temp_dir = create_temp_repo_with_files(&[
        ("udp_client.c", r#"
            #include <sys/socket.h>
            void send_data() {
                int fd = socket(AF_INET, SOCK_DGRAM, 0);
                connect(fd, (struct sockaddr*)&addr, sizeof(addr));
                send(fd, data, len, 0);
            }
        "#),
    ]);

    let mut storage = StorageConnection::open_in_memory().unwrap();

    let result = index_into_storage(
        temp_dir.path(),
        &mut storage,
        "bi1b-udp-connect",
        &ComposeOptions::default(),
    )
    .unwrap();

    let filter = BoundaryInteractionFilter {
        channel_kind: Some(ChannelKind::UdpSocket),
        ..Default::default()
    };
    let surfaces = storage
        .list_boundary_interactions(&result.snapshot_uid, &filter)
        .unwrap();

    assert_eq!(surfaces.len(), 1, "expected 1 udp_socket surface");
    assert_eq!(
        surfaces[0].direction,
        Direction::Bidirectional,
        "UDP connect() must stay Bidirectional per slice contract (D3)"
    );
}
