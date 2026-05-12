//! Socket lineage tracking for fd role detection (BI-1B Phase 2).
//!
//! This module implements function-local fd tracking to refine TCP/UDP socket
//! surfaces with provider/consumer role detection.
//!
//! ## Design Decisions (Locked)
//!
//! - **D1: C-only** — No C++ in this slice (different complexity, separate actor)
//! - **D2: Refine existing surface** — Update direction on origin surface, don't duplicate
//! - **D3: bind alone insufficient** — Require bind+listen for provider
//!
//! ## Mechanism
//!
//! ```text
//! 1. socket(AF_INET, SOCK_STREAM) → emit surface, register: "fd" → tcp + bidirectional
//! 2. bind(fd, &addr) → lookup "fd", evidence.has_bind = true
//! 3. listen(fd, 5) → evidence.has_listen = true → state machine: provider
//!    → update origin surface direction
//! 4. Function ends → clear registry
//! ```
//!
//! ## State Machine
//!
//! ```text
//! initial: bidirectional (no role evidence)
//!
//! on connect(fd):       → consumer
//! on bind(fd):          → still bidirectional (insufficient per D3)
//! on bind(fd)+listen(fd): → provider
//! on listen(fd) [fd known]: → provider
//! on accept(fd):        → provider (reinforcement)
//! ```
//!
//! ## Scope
//!
//! Function-local only. Cleared at function boundary.
//! Does NOT track: cross-function propagation, aliases, globals, parameters.

use std::collections::HashMap;

use repo_graph_boundary_interaction::Direction;

/// Evidence flags for role detection state machine.
#[derive(Debug, Clone, Default)]
pub struct RoleEvidence {
    /// Whether bind() has been called on this fd.
    pub has_bind: bool,
    /// Whether listen() has been called on this fd.
    pub has_listen: bool,
    /// Whether connect() has been called on this fd.
    pub has_connect: bool,
    /// Whether accept() has been called on this fd.
    pub has_accept: bool,
}

impl RoleEvidence {
    /// Resolve direction from accumulated evidence.
    ///
    /// State machine:
    /// - TCP + connect → consumer
    /// - TCP + bind + listen → provider
    /// - TCP + listen alone → provider (if we're tracking, it's on a known socket)
    /// - TCP + accept → provider (reinforcement)
    /// - bind alone → bidirectional (insufficient per D3)
    /// - UDP → always bidirectional (no strong role semantics in first cut)
    /// - no evidence → bidirectional
    ///
    /// Note: UDP sockets stay bidirectional regardless (no strong role heuristics).
    /// This includes UDP connect() which just sets a default destination address.
    pub fn resolve_direction(&self, is_tcp: bool) -> Direction {
        // UDP: always bidirectional, no role semantics in first cut
        // UDP connect() just sets default destination, not a "client" role
        if !is_tcp {
            return Direction::Bidirectional;
        }

        // TCP only from here

        // Consumer: if connect was called, it's a client
        if self.has_connect {
            return Direction::Consumer;
        }

        // Provider: requires listen (with or without bind)
        if self.has_listen {
            return Direction::Provider;
        }

        // accept() also indicates provider (reinforcement)
        if self.has_accept {
            return Direction::Provider;
        }

        // bind alone is insufficient (D3)
        Direction::Bidirectional
    }
}

/// Socket channel kind for lineage tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackedChannelKind {
    /// TCP socket (AF_INET/AF_INET6 + SOCK_STREAM)
    TcpSocket,
    /// UDP socket (AF_INET/AF_INET6 + SOCK_DGRAM)
    UdpSocket,
}

/// Socket lineage entry: tracked socket with evidence.
#[derive(Debug, Clone)]
pub struct SocketLineage {
    /// Variable identifier (e.g., "fd", "server_fd")
    pub identifier: String,

    /// Channel kind (TCP or UDP)
    pub channel_kind: TrackedChannelKind,

    /// Surface UID from socket() emission
    pub origin_surface_uid: String,

    /// Accumulated role evidence
    pub evidence: RoleEvidence,
}

/// Function-scoped fd registry.
///
/// Tracks socket lineages within a function for role detection.
/// Should be cleared at function boundary.
#[derive(Debug, Default)]
pub struct FdRegistry {
    /// Map from identifier to socket lineage
    entries: HashMap<String, SocketLineage>,
}

impl FdRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a socket lineage from a socket() call emission.
    ///
    /// Called when socket() emits a surface. Records the fd identifier
    /// for subsequent bind/listen/connect/accept tracking.
    pub fn register_socket(
        &mut self,
        identifier: &str,
        channel_kind: TrackedChannelKind,
        origin_surface_uid: &str,
    ) {
        self.entries.insert(
            identifier.to_string(),
            SocketLineage {
                identifier: identifier.to_string(),
                channel_kind,
                origin_surface_uid: origin_surface_uid.to_string(),
                evidence: RoleEvidence::default(),
            },
        );
    }

    /// Record bind evidence on an fd.
    ///
    /// If the fd is tracked, updates its evidence. Otherwise ignored.
    pub fn record_bind(&mut self, identifier: &str) {
        if let Some(lineage) = self.entries.get_mut(identifier) {
            lineage.evidence.has_bind = true;
        }
    }

    /// Record listen evidence on an fd.
    ///
    /// If the fd is tracked, updates its evidence. Otherwise ignored.
    pub fn record_listen(&mut self, identifier: &str) {
        if let Some(lineage) = self.entries.get_mut(identifier) {
            lineage.evidence.has_listen = true;
        }
    }

    /// Record connect evidence on an fd.
    ///
    /// If the fd is tracked, updates its evidence. Otherwise ignored.
    pub fn record_connect(&mut self, identifier: &str) {
        if let Some(lineage) = self.entries.get_mut(identifier) {
            lineage.evidence.has_connect = true;
        }
    }

    /// Record accept evidence on an fd.
    ///
    /// If the fd is tracked, updates its evidence. Otherwise ignored.
    pub fn record_accept(&mut self, identifier: &str) {
        if let Some(lineage) = self.entries.get_mut(identifier) {
            lineage.evidence.has_accept = true;
        }
    }

    /// Check if an identifier is tracked.
    pub fn is_tracked(&self, identifier: &str) -> bool {
        self.entries.contains_key(identifier)
    }

    /// Get the channel kind for a tracked identifier.
    pub fn channel_kind(&self, identifier: &str) -> Option<TrackedChannelKind> {
        self.entries.get(identifier).map(|l| l.channel_kind)
    }

    /// Drain all lineages for direction refinement at function end.
    ///
    /// Returns (surface_uid, resolved_direction) pairs for surfaces
    /// whose direction should be updated.
    ///
    /// Only returns entries where direction differs from bidirectional
    /// (the default emitted direction for socket() calls).
    pub fn drain_for_refinement(&mut self) -> Vec<(String, Direction)> {
        let entries: Vec<_> = self.entries.drain().collect();
        let mut refinements = Vec::new();

        for (_, lineage) in entries {
            let is_tcp = lineage.channel_kind == TrackedChannelKind::TcpSocket;
            let direction = lineage.evidence.resolve_direction(is_tcp);

            // Only emit refinement if direction changed from bidirectional
            if direction != Direction::Bidirectional {
                refinements.push((lineage.origin_surface_uid, direction));
            }
        }

        refinements
    }

    /// Clear the registry (call at function boundary).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_evidence_is_bidirectional() {
        let evidence = RoleEvidence::default();
        assert_eq!(evidence.resolve_direction(true), Direction::Bidirectional);
        assert_eq!(evidence.resolve_direction(false), Direction::Bidirectional);
    }

    #[test]
    fn connect_makes_consumer_tcp_only() {
        // TCP connect() → Consumer
        // UDP connect() → Bidirectional (tested separately)
        let mut evidence = RoleEvidence::default();
        evidence.has_connect = true;
        assert_eq!(evidence.resolve_direction(true), Direction::Consumer);
    }

    #[test]
    fn bind_alone_stays_bidirectional_per_d3() {
        let mut evidence = RoleEvidence::default();
        evidence.has_bind = true;
        // D3: bind alone is insufficient for provider
        assert_eq!(evidence.resolve_direction(true), Direction::Bidirectional);
        assert_eq!(evidence.resolve_direction(false), Direction::Bidirectional);
    }

    #[test]
    fn bind_plus_listen_makes_provider_tcp() {
        let mut evidence = RoleEvidence::default();
        evidence.has_bind = true;
        evidence.has_listen = true;
        assert_eq!(evidence.resolve_direction(true), Direction::Provider);
    }

    #[test]
    fn listen_alone_makes_provider_tcp() {
        // If we're tracking and see listen, it's on a known socket
        let mut evidence = RoleEvidence::default();
        evidence.has_listen = true;
        assert_eq!(evidence.resolve_direction(true), Direction::Provider);
    }

    #[test]
    fn udp_stays_bidirectional_with_bind_listen() {
        // UDP doesn't have listen semantics, so even if we somehow
        // accumulated these flags, it should stay bidirectional
        let mut evidence = RoleEvidence::default();
        evidence.has_bind = true;
        evidence.has_listen = true; // Shouldn't happen for UDP, but test the logic
        assert_eq!(evidence.resolve_direction(false), Direction::Bidirectional);
    }

    #[test]
    fn udp_connect_stays_bidirectional() {
        // UDP connect() just sets default destination, not a "client" role
        // This is a critical test for D3 compliance
        let mut evidence = RoleEvidence::default();
        evidence.has_connect = true;
        assert_eq!(
            evidence.resolve_direction(false),
            Direction::Bidirectional,
            "UDP connect() must stay bidirectional per slice contract"
        );
    }

    #[test]
    fn accept_makes_provider_tcp() {
        let mut evidence = RoleEvidence::default();
        evidence.has_accept = true;
        assert_eq!(evidence.resolve_direction(true), Direction::Provider);
    }

    #[test]
    fn registry_tracks_socket() {
        let mut registry = FdRegistry::new();
        registry.register_socket("fd", TrackedChannelKind::TcpSocket, "uid:test");
        assert!(registry.is_tracked("fd"));
        assert!(!registry.is_tracked("other"));
    }

    #[test]
    fn registry_accumulates_evidence() {
        let mut registry = FdRegistry::new();
        registry.register_socket("server_fd", TrackedChannelKind::TcpSocket, "uid:server");

        registry.record_bind("server_fd");
        registry.record_listen("server_fd");

        let refinements = registry.drain_for_refinement();
        assert_eq!(refinements.len(), 1);
        assert_eq!(refinements[0].0, "uid:server");
        assert_eq!(refinements[0].1, Direction::Provider);
    }

    #[test]
    fn registry_ignores_untracked_fd() {
        let mut registry = FdRegistry::new();
        // No socket registered for "unknown_fd"
        registry.record_bind("unknown_fd");
        registry.record_listen("unknown_fd");

        let refinements = registry.drain_for_refinement();
        assert!(refinements.is_empty());
    }

    #[test]
    fn drain_only_returns_changed_directions() {
        let mut registry = FdRegistry::new();

        // TCP socket with no evidence → stays bidirectional, not returned
        registry.register_socket("fd1", TrackedChannelKind::TcpSocket, "uid:1");

        // TCP socket with connect → consumer, returned
        registry.register_socket("fd2", TrackedChannelKind::TcpSocket, "uid:2");
        registry.record_connect("fd2");

        // TCP socket with bind+listen → provider, returned
        registry.register_socket("fd3", TrackedChannelKind::TcpSocket, "uid:3");
        registry.record_bind("fd3");
        registry.record_listen("fd3");

        let refinements = registry.drain_for_refinement();
        assert_eq!(refinements.len(), 2);

        // Check that fd1 (bidirectional) is not in refinements
        let uids: Vec<_> = refinements.iter().map(|(uid, _)| uid.as_str()).collect();
        assert!(!uids.contains(&"uid:1"));
        assert!(uids.contains(&"uid:2"));
        assert!(uids.contains(&"uid:3"));
    }

    #[test]
    fn full_tcp_server_flow() {
        let mut registry = FdRegistry::new();

        // Simulate: int server_fd = socket(AF_INET, SOCK_STREAM, 0);
        registry.register_socket("server_fd", TrackedChannelKind::TcpSocket, "bi:test:server.c:10:5:tcp_socket:bidirectional");

        // Simulate: bind(server_fd, &addr, sizeof(addr));
        registry.record_bind("server_fd");

        // At this point, direction is still bidirectional (D3)

        // Simulate: listen(server_fd, 5);
        registry.record_listen("server_fd");

        // Now direction should be provider
        let refinements = registry.drain_for_refinement();
        assert_eq!(refinements.len(), 1);
        assert_eq!(refinements[0].1, Direction::Provider);
    }

    #[test]
    fn full_tcp_client_flow() {
        let mut registry = FdRegistry::new();

        // Simulate: int client_fd = socket(AF_INET, SOCK_STREAM, 0);
        registry.register_socket("client_fd", TrackedChannelKind::TcpSocket, "bi:test:client.c:10:5:tcp_socket:bidirectional");

        // Simulate: connect(client_fd, &addr, sizeof(addr));
        registry.record_connect("client_fd");

        let refinements = registry.drain_for_refinement();
        assert_eq!(refinements.len(), 1);
        assert_eq!(refinements[0].1, Direction::Consumer);
    }
}
