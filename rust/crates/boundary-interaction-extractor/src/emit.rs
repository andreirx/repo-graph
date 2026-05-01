//! Boundary interaction emitter — accumulates extracted facts.
//!
//! Analogous to `StateBoundaryEmitter` in `state-extractor`. Provides
//! a stateful emitter that:
//!
//! 1. Receives callsite inputs from extractors
//! 2. Matches against the binding table
//! 3. Produces surface and channel detail facts
//! 4. Deduplicates by surface UID
//! 5. Tracks dual projection requirements for shared memory

use std::collections::HashMap;

use thiserror::Error;

use repo_graph_boundary_interaction::{
    table::{BindingEntry, BindingTable, Language, ScopeHeuristic},
    BoundaryInteractionSurface, BoundaryScope, ChannelDetail, ChannelKind,
    EndpointLocality, ProtocolFamily,
};
use repo_graph_classification::types::SourceLocation;

use crate::evidence::BoundaryInteractionEvidence;

/// Error during boundary interaction emission.
#[derive(Debug, Error)]
pub enum EmitError {
    /// No binding matched the callsite.
    #[error("no binding matched function '{function}' for language {language:?}")]
    NoBindingMatch {
        /// Function name that was not found.
        function: String,
        /// Language being extracted.
        language: Language,
    },

    /// Invalid surface builder state.
    #[error("surface builder error: {0}")]
    BuilderError(String),
}

/// Input from an extractor describing a detected API call.
///
/// Extractors populate this struct when they detect a function call
/// that might be a boundary interaction. The emitter matches it against
/// the binding table and produces output facts.
///
/// ## Guard predicates
///
/// Some bindings require additional context to determine if they apply:
///
/// - `socket()` requires `socket_family` to distinguish AF_UNIX from AF_INET
/// - `mmap()` requires `mmap_flags` to verify MAP_SHARED is present
/// - `mknod()` requires `mknod_mode` to verify S_IFIFO is present
///
/// If the required context is missing or doesn't match, the emitter
/// declines to emit (returns `Ok(None)`), not emits with unknown scope.
#[derive(Debug, Clone)]
pub struct BoundaryCallsite {
    /// Language of the source file.
    pub language: Language,

    /// Function name that was called (e.g., "bind", "shm_open").
    pub function_name: String,

    /// Source location of the call.
    pub location: SourceLocation,

    /// Source file path (repo-relative).
    pub source_file: String,

    /// Stable key of the enclosing symbol (function/method).
    pub enclosing_symbol_key: String,

    /// Extracted argument value (path, address, etc.) if available.
    pub extracted_argument: Option<String>,

    /// Argument index where the value was extracted.
    pub argument_index: Option<usize>,

    /// Raw argument text (for debugging/evidence).
    pub raw_argument_text: Option<String>,

    /// Socket address family if known (AF_UNIX, AF_INET, etc.).
    /// Required for socket/bind/connect/listen/accept to match Slice 1A.
    pub socket_family: Option<SocketFamily>,

    /// mmap flags (MAP_SHARED, MAP_PRIVATE, etc.).
    /// Required for mmap() to emit as shared memory.
    pub mmap_flags: Option<MmapFlags>,

    /// mknod mode bits. Required for mknod() to emit as FIFO.
    /// Must include S_IFIFO (0o010000) to match.
    pub mknod_mode: Option<u32>,
}

/// mmap flags for shared memory detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmapFlags {
    /// MAP_SHARED — changes visible to other processes.
    Shared,
    /// MAP_PRIVATE — copy-on-write, not shared.
    Private,
    /// Unknown/unparseable flags.
    Unknown,
}

/// S_IFIFO mode bit for FIFO detection in mknod.
pub const S_IFIFO: u32 = 0o010000;

/// Socket address family for scope heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketFamily {
    /// Unix domain socket (AF_UNIX).
    Unix,
    /// IPv4 (AF_INET).
    Inet,
    /// IPv6 (AF_INET6).
    Inet6,
    /// CAN bus (AF_CAN).
    Can,
    /// Unknown/other.
    Unknown,
}

/// Output from the emitter for a single callsite.
#[derive(Debug, Clone)]
pub struct EmittedFacts {
    /// The boundary interaction surface (Level 1).
    pub surface: BoundaryInteractionSurface,

    /// The channel detail (Level 2).
    pub channel: ChannelDetail,

    /// Whether this fact requires dual projection to state-boundary.
    pub requires_dual_projection: bool,
}

/// Context for emission — snapshot and repo identity.
#[derive(Debug, Clone)]
pub struct EmitterContext {
    /// Current snapshot UID.
    pub snapshot_uid: String,

    /// Repository UID.
    pub repo_uid: String,

    /// Extractor identifier (e.g., "c-ipc:0.1.0").
    pub extractor: String,
}

/// Stateful emitter for boundary interaction facts.
///
/// Accumulates facts, deduplicates by surface UID, and tracks
/// dual projection requirements.
pub struct BoundaryInteractionEmitter {
    /// Emission context.
    context: EmitterContext,

    /// Binding table for API matching.
    binding_table: &'static BindingTable,

    /// Accumulated surfaces by UID (dedup).
    surfaces: HashMap<String, BoundaryInteractionSurface>,

    /// Accumulated channel details by UID.
    channels: HashMap<String, ChannelDetail>,

    /// Surface UIDs that require dual projection.
    dual_projection_surfaces: Vec<String>,
}

impl BoundaryInteractionEmitter {
    /// Create a new emitter with the given context.
    pub fn new(context: EmitterContext) -> Self {
        Self {
            context,
            binding_table: BindingTable::load_embedded(),
            surfaces: HashMap::new(),
            channels: HashMap::new(),
            dual_projection_surfaces: Vec::new(),
        }
    }

    /// Process a callsite and emit facts if it matches the binding table.
    ///
    /// Returns `Ok(Some(facts))` if a binding matched and facts were emitted.
    /// Returns `Ok(None)` if:
    /// - No binding matched the function name
    /// - A binding matched but the callsite context doesn't qualify (e.g.,
    ///   socket() with AF_INET instead of AF_UNIX for Slice 1A)
    ///
    /// Returns `Err` only on actual emission failures.
    pub fn try_emit(&mut self, callsite: &BoundaryCallsite) -> Result<Option<EmittedFacts>, EmitError> {
        // Look up binding by function name
        let binding = match self
            .binding_table
            .find_by_function(callsite.language, &callsite.function_name)
        {
            Some(b) => b,
            None => return Ok(None), // Not a boundary interaction
        };

        // Guard predicate: reject binding if callsite context doesn't match.
        // This prevents emitting Unix socket facts for AF_INET sockets, etc.
        if !self.binding_matches_context(binding, callsite) {
            return Ok(None); // Binding doesn't apply to this callsite
        }

        // Determine scope and locality from heuristics
        let (scope, locality) = self.determine_scope_and_locality(binding, callsite);

        // Build evidence
        let evidence = self.build_evidence(binding, callsite, scope, locality);

        // Build surface
        let surface = self.build_surface(binding, callsite, scope, locality, &evidence)?;

        // Build channel detail
        let channel = self.build_channel(binding, callsite, &surface.surface_uid)?;

        // Track dual projection
        let requires_dual_projection = binding.channel_kind.requires_dual_projection();
        if requires_dual_projection {
            self.dual_projection_surfaces.push(surface.surface_uid.clone());
        }

        // Dedup and store
        self.surfaces
            .entry(surface.surface_uid.clone())
            .or_insert_with(|| surface.clone());
        self.channels
            .entry(channel.channel_uid.clone())
            .or_insert_with(|| channel.clone());

        Ok(Some(EmittedFacts {
            surface,
            channel,
            requires_dual_projection,
        }))
    }

    /// Guard predicate: check if the binding applies to this callsite's context.
    ///
    /// This is the second-stage filter that rejects bindings after inspecting
    /// callsite evidence. It prevents:
    ///
    /// - Emitting `unix_socket` facts for `socket(AF_INET, ...)` calls
    /// - Emitting `shared_memory` facts for `mmap(..., MAP_PRIVATE, ...)` calls
    /// - Emitting `named_pipe` facts for `mknod(..., mode_without_S_IFIFO, ...)` calls
    ///
    /// Returns `true` if the binding should emit, `false` to decline.
    fn binding_matches_context(&self, binding: &BindingEntry, callsite: &BoundaryCallsite) -> bool {
        match binding.channel_kind {
            // Unix socket bindings require AF_UNIX family evidence
            ChannelKind::UnixSocket => {
                match callsite.socket_family {
                    Some(SocketFamily::Unix) => true,
                    Some(_) => false, // AF_INET, AF_INET6, AF_CAN — not Slice 1A
                    None => {
                        // No family evidence. For socket(), reject.
                        // For bind/connect/listen/accept, the family was determined
                        // at socket() time; if we have a sockaddr_un path, accept.
                        if callsite.function_name == "socket" {
                            false // Can't classify without family
                        } else {
                            // For bind/connect, require extracted_argument (the path)
                            // as weak evidence this is a Unix socket operation
                            callsite.extracted_argument.is_some()
                        }
                    }
                }
            }

            // Shared memory bindings: mmap requires MAP_SHARED flag
            ChannelKind::SharedMemory => {
                if callsite.function_name == "mmap" {
                    match callsite.mmap_flags {
                        Some(MmapFlags::Shared) => true,
                        Some(MmapFlags::Private) => false, // Not shared memory
                        Some(MmapFlags::Unknown) => false, // Can't prove MAP_SHARED
                        None => false, // No evidence — decline
                    }
                } else {
                    // shm_open, shm_unlink, munmap — no additional guard needed
                    true
                }
            }

            // Named pipe bindings: mknod requires S_IFIFO mode bit
            ChannelKind::NamedPipe => {
                if callsite.function_name == "mknod" {
                    match callsite.mknod_mode {
                        Some(mode) => (mode & S_IFIFO) != 0,
                        None => false, // No evidence — decline
                    }
                } else {
                    // mkfifo — no additional guard needed (always creates FIFO)
                    true
                }
            }

            // Anonymous pipes, message queues — no guards needed
            ChannelKind::AnonymousPipe | ChannelKind::MessageQueue => true,

            // Future slice channel kinds — not yet supported, decline
            _ => false,
        }
    }

    /// Determine boundary scope and endpoint locality from binding and callsite.
    fn determine_scope_and_locality(
        &self,
        binding: &BindingEntry,
        callsite: &BoundaryCallsite,
    ) -> (BoundaryScope, EndpointLocality) {
        match binding.scope_heuristic {
            ScopeHeuristic::Fixed => {
                let scope = binding.fixed_scope.unwrap_or(BoundaryScope::Unknown);
                let locality = match binding.channel_kind {
                    ChannelKind::UnixSocket
                    | ChannelKind::NamedPipe
                    | ChannelKind::SharedMemory
                    | ChannelKind::MessageQueue => EndpointLocality::SameHostNamed,
                    ChannelKind::AnonymousPipe => EndpointLocality::SameHostNamed,
                    _ => EndpointLocality::Unknown,
                };
                (scope, locality)
            }
            ScopeHeuristic::ArgFamily => {
                // Use socket family from callsite
                match callsite.socket_family {
                    Some(SocketFamily::Unix) => {
                        (BoundaryScope::InterProcess, EndpointLocality::SameHostNamed)
                    }
                    Some(SocketFamily::Inet | SocketFamily::Inet6) => {
                        // TCP/UDP — need address analysis for scope
                        // For now, return unknown; Slice 1B will refine this
                        (BoundaryScope::Unknown, EndpointLocality::Unknown)
                    }
                    Some(SocketFamily::Can) => {
                        (BoundaryScope::InterDevice, EndpointLocality::Unknown)
                    }
                    _ => (BoundaryScope::Unknown, EndpointLocality::Unknown),
                }
            }
            ScopeHeuristic::Sockaddr => {
                // Would analyze sockaddr struct — for Slice 1A, Unix sockets
                // are same_host_named
                if callsite.socket_family == Some(SocketFamily::Unix) {
                    (BoundaryScope::InterProcess, EndpointLocality::SameHostNamed)
                } else {
                    (BoundaryScope::Unknown, EndpointLocality::Unknown)
                }
            }
            ScopeHeuristic::Path => {
                // Path-based mechanisms (pipes, shm) are inter_process
                (BoundaryScope::InterProcess, EndpointLocality::SameHostNamed)
            }
        }
    }

    /// Build evidence struct from binding and callsite.
    fn build_evidence(
        &self,
        binding: &BindingEntry,
        callsite: &BoundaryCallsite,
        scope: BoundaryScope,
        locality: EndpointLocality,
    ) -> BoundaryInteractionEvidence {
        let mut evidence = BoundaryInteractionEvidence::new(
            binding.binding_key(),
            binding.api_family.clone(),
            callsite.function_name.clone(),
        )
        .with_basis(binding.basis)
        .with_channel_kind(binding.channel_kind)
        .with_direction(binding.direction())
        .with_boundary_scope(scope)
        .with_endpoint_locality(locality)
        .with_interaction_pattern(binding.interaction_pattern);

        if let Some(ref addr) = callsite.extracted_argument {
            evidence = evidence.with_extracted_address(addr.clone(), callsite.argument_index);
        }

        if let Some(ref raw) = callsite.raw_argument_text {
            evidence = evidence.with_raw_argument(raw.clone());
        }

        if let Some(ref notes) = binding.notes {
            evidence = evidence.with_binding_notes(notes.clone());
        }

        evidence
    }

    /// Build a BoundaryInteractionSurface from binding and callsite.
    fn build_surface(
        &self,
        binding: &BindingEntry,
        callsite: &BoundaryCallsite,
        scope: BoundaryScope,
        locality: EndpointLocality,
        evidence: &BoundaryInteractionEvidence,
    ) -> Result<BoundaryInteractionSurface, EmitError> {
        let direction = binding.direction();
        let line_start = callsite.location.line_start.max(0) as u32;
        let line_end = callsite.location.line_end.max(0) as u32;
        let col_start = callsite.location.col_start.max(0) as u32;
        let col_end = callsite.location.col_end.max(0) as u32;

        let surface_uid = BoundaryInteractionSurface::build_uid(
            &self.context.repo_uid,
            &callsite.source_file,
            line_start,
            col_start,
            binding.channel_kind,
            direction,
        );

        let protocol = protocol_for_channel_kind(binding.channel_kind);
        let protocol_family = ProtocolFamily::from(binding.channel_kind);

        // Track A surfaces derive transport_class from channel_kind
        let transport_class = binding.channel_kind.default_transport_class();

        // Provenance identifies the evidence source for Track A:
        // format is "api:<family>:<function>" to distinguish from Track B schema-backed
        let provenance = format!("api:{}:{}", binding.api_family, callsite.function_name);

        // Confidence basis explains the default confidence derivation
        let confidence_basis = format!("{}-match", binding.basis.as_str());

        Ok(BoundaryInteractionSurface {
            surface_uid,
            snapshot_uid: self.context.snapshot_uid.clone(),
            repo_uid: self.context.repo_uid.clone(),
            boundary_scope: scope,
            channel_kind: binding.channel_kind,
            direction,
            transport_class: Some(transport_class),
            provenance: Some(provenance),
            confidence_basis: Some(confidence_basis),
            protocol: protocol.to_string(),
            protocol_family,
            interaction_pattern: binding.interaction_pattern,
            endpoint_locality: locality,
            symbol_stable_key: callsite.enclosing_symbol_key.clone(),
            source_file: callsite.source_file.clone(),
            line_start,
            line_end,
            col_start,
            col_end,
            extractor: self.context.extractor.clone(),
            basis: binding.basis,
            confidence: binding.basis.default_confidence(),
            evidence_json: evidence.to_json(),
        })
    }

    /// Build a ChannelDetail from binding and callsite.
    fn build_channel(
        &self,
        binding: &BindingEntry,
        callsite: &BoundaryCallsite,
        surface_uid: &str,
    ) -> Result<ChannelDetail, EmitError> {
        let channel_identity = callsite
            .extracted_argument
            .clone()
            .unwrap_or_else(|| format!("{}:{}", callsite.source_file, callsite.location.line_start));

        let channel_uid = ChannelDetail::build_uid(surface_uid, &channel_identity);

        let mut channel = ChannelDetail {
            channel_uid,
            surface_uid: surface_uid.to_string(),
            channel_kind: binding.channel_kind,
            channel_identity,
            socket_path: None,
            tcp_endpoint: None,
            udp_endpoint: None,
            can_id: None,
            i2c_address: None,
            spi_device: None,
            serial_device: None,
            shm_key: None,
            pipe_path: None,
            pipe_context: None,
            mqueue_name: None,
            baud_rate: None,
            can_extended: None,
            frame_format: None,
            payload_contract: None,
            metadata_json: None,
        };

        // Populate mechanism-specific field
        if let Some(ref addr) = callsite.extracted_argument {
            match binding.channel_kind {
                ChannelKind::UnixSocket => channel.socket_path = Some(addr.clone()),
                ChannelKind::NamedPipe => channel.pipe_path = Some(addr.clone()),
                ChannelKind::AnonymousPipe => channel.pipe_context = Some(addr.clone()),
                ChannelKind::SharedMemory => channel.shm_key = Some(addr.clone()),
                ChannelKind::MessageQueue => channel.mqueue_name = Some(addr.clone()),
                ChannelKind::TcpSocket => channel.tcp_endpoint = Some(addr.clone()),
                ChannelKind::UdpSocket => channel.udp_endpoint = Some(addr.clone()),
                ChannelKind::SerialPort => channel.serial_device = Some(addr.clone()),
                _ => {}
            }
        }

        Ok(channel)
    }

    /// Get all accumulated surfaces.
    pub fn surfaces(&self) -> impl Iterator<Item = &BoundaryInteractionSurface> {
        self.surfaces.values()
    }

    /// Get all accumulated channel details.
    pub fn channels(&self) -> impl Iterator<Item = &ChannelDetail> {
        self.channels.values()
    }

    /// Get surface UIDs that require dual projection to state-boundary.
    pub fn dual_projection_surfaces(&self) -> &[String] {
        &self.dual_projection_surfaces
    }

    /// Total number of accumulated surfaces.
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    /// Total number of accumulated channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Clear all accumulated facts.
    pub fn clear(&mut self) {
        self.surfaces.clear();
        self.channels.clear();
        self.dual_projection_surfaces.clear();
    }
}

/// Map channel kind to protocol string.
fn protocol_for_channel_kind(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::UnixSocket => "unix",
        ChannelKind::TcpSocket => "tcp",
        ChannelKind::UdpSocket => "udp",
        ChannelKind::NamedPipe => "fifo",
        ChannelKind::AnonymousPipe => "pipe",
        ChannelKind::SharedMemory => "shm",
        ChannelKind::MessageQueue => "mqueue",
        ChannelKind::SharedArrayBuffer => "sab",
        ChannelKind::GrpcChannel => "grpc",
        ChannelKind::ProtobufStream => "protobuf",
        ChannelKind::ErpcChannel => "erpc",
        ChannelKind::SerialPort => "serial",
        ChannelKind::CanMessage => "can",
        ChannelKind::MqttTopic => "mqtt",
        ChannelKind::DbusInterface => "dbus",
        ChannelKind::ZeromqSocket => "zmq",
        ChannelKind::I2cDevice => "i2c",
        ChannelKind::SpiDevice => "spi",
        ChannelKind::UsbEndpoint => "usb",
        ChannelKind::BleCharacteristic => "ble",
        ChannelKind::ModbusRegister => "modbus",
        ChannelKind::CustomProtocol => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_boundary_interaction::{Direction, InteractionPattern};

    fn test_context() -> EmitterContext {
        EmitterContext {
            snapshot_uid: "snap-test".to_string(),
            repo_uid: "test-repo".to_string(),
            extractor: "c-ipc:0.1.0".to_string(),
        }
    }

    fn unix_socket_bind_callsite() -> BoundaryCallsite {
        BoundaryCallsite {
            language: Language::C,
            function_name: "bind".to_string(),
            location: SourceLocation {
                line_start: 100,
                line_end: 100,
                col_start: 5,
                col_end: 50,
            },
            source_file: "src/server.c".to_string(),
            enclosing_symbol_key: "test-repo:src/server.c#start_server:SYMBOL:function".to_string(),
            extracted_argument: Some("/var/run/daemon.sock".to_string()),
            argument_index: Some(1),
            raw_argument_text: Some("&addr".to_string()),
            socket_family: Some(SocketFamily::Unix),
            mmap_flags: None,
            mknod_mode: None,
        }
    }

    #[test]
    fn emitter_matches_unix_socket_bind() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = unix_socket_bind_callsite();

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(result.is_some());

        let facts = result.unwrap();
        assert_eq!(facts.surface.channel_kind, ChannelKind::UnixSocket);
        assert_eq!(facts.surface.direction, Direction::Provider);
        assert_eq!(facts.surface.boundary_scope, BoundaryScope::InterProcess);
        assert!(!facts.requires_dual_projection);
    }

    #[test]
    fn emitter_returns_none_for_unknown_function() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "printf".to_string(), // Not a boundary function
            location: SourceLocation {
                line_start: 50,
                line_end: 50,
                col_start: 1,
                col_end: 20,
            },
            source_file: "src/main.c".to_string(),
            enclosing_symbol_key: "test-repo:src/main.c#main:SYMBOL:function".to_string(),
            extracted_argument: None,
            argument_index: None,
            raw_argument_text: None,
            socket_family: None,
            mmap_flags: None,
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn emitter_deduplicates_by_surface_uid() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = unix_socket_bind_callsite();

        // Emit the same callsite twice
        emitter.try_emit(&callsite).unwrap();
        emitter.try_emit(&callsite).unwrap();

        // Should only have one surface
        assert_eq!(emitter.surface_count(), 1);
    }

    #[test]
    fn shared_memory_requires_dual_projection() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "shm_open".to_string(),
            location: SourceLocation {
                line_start: 200,
                line_end: 200,
                col_start: 5,
                col_end: 40,
            },
            source_file: "src/shm.c".to_string(),
            enclosing_symbol_key: "test-repo:src/shm.c#init_shm:SYMBOL:function".to_string(),
            extracted_argument: Some("/my_shared_mem".to_string()),
            argument_index: Some(0),
            raw_argument_text: Some("\"/my_shared_mem\"".to_string()),
            socket_family: None,
            mmap_flags: None, // shm_open doesn't need mmap_flags
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(result.is_some());

        let facts = result.unwrap();
        assert!(facts.requires_dual_projection);
        assert_eq!(facts.surface.channel_kind, ChannelKind::SharedMemory);
        assert_eq!(
            facts.surface.interaction_pattern,
            InteractionPattern::SharedState
        );
    }

    #[test]
    fn emitter_tracks_dual_projection_surfaces() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());

        // Emit a Unix socket (no dual projection)
        let socket_callsite = unix_socket_bind_callsite();
        emitter.try_emit(&socket_callsite).unwrap();

        // Emit shared memory (requires dual projection)
        let shm_callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "shm_open".to_string(),
            location: SourceLocation {
                line_start: 200,
                line_end: 200,
                col_start: 5,
                col_end: 40,
            },
            source_file: "src/shm.c".to_string(),
            enclosing_symbol_key: "test-repo:src/shm.c#init:SYMBOL:function".to_string(),
            extracted_argument: Some("/test_shm".to_string()),
            argument_index: Some(0),
            raw_argument_text: None,
            socket_family: None,
            mmap_flags: None, // shm_open doesn't need mmap_flags
            mknod_mode: None,
        };
        emitter.try_emit(&shm_callsite).unwrap();

        // Should have one dual projection surface
        assert_eq!(emitter.dual_projection_surfaces().len(), 1);
        assert_eq!(emitter.surface_count(), 2);
    }

    // ── Guard predicate tests ─────────────────────────────────────────
    // These verify the false-positive rejection logic.

    #[test]
    fn socket_with_af_inet_is_rejected() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "socket".to_string(),
            location: SourceLocation {
                line_start: 50,
                line_end: 50,
                col_start: 5,
                col_end: 40,
            },
            source_file: "src/net.c".to_string(),
            enclosing_symbol_key: "test-repo:src/net.c#connect_server:SYMBOL:function".to_string(),
            extracted_argument: None,
            argument_index: None,
            raw_argument_text: None,
            socket_family: Some(SocketFamily::Inet), // AF_INET — not Slice 1A
            mmap_flags: None,
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_none(),
            "socket(AF_INET) should not emit — not Slice 1A scope"
        );
    }

    #[test]
    fn socket_with_af_unix_emits() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "socket".to_string(),
            location: SourceLocation {
                line_start: 50,
                line_end: 50,
                col_start: 5,
                col_end: 40,
            },
            source_file: "src/ipc.c".to_string(),
            enclosing_symbol_key: "test-repo:src/ipc.c#start_server:SYMBOL:function".to_string(),
            extracted_argument: None,
            argument_index: None,
            raw_argument_text: None,
            socket_family: Some(SocketFamily::Unix), // AF_UNIX — Slice 1A
            mmap_flags: None,
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_some(),
            "socket(AF_UNIX) should emit as unix_socket"
        );
        assert_eq!(result.unwrap().surface.channel_kind, ChannelKind::UnixSocket);
    }

    #[test]
    fn socket_without_family_is_rejected() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "socket".to_string(),
            location: SourceLocation {
                line_start: 50,
                line_end: 50,
                col_start: 5,
                col_end: 40,
            },
            source_file: "src/net.c".to_string(),
            enclosing_symbol_key: "test-repo:src/net.c#init:SYMBOL:function".to_string(),
            extracted_argument: None,
            argument_index: None,
            raw_argument_text: None,
            socket_family: None, // Unknown family
            mmap_flags: None,
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_none(),
            "socket() without family evidence should not emit"
        );
    }

    #[test]
    fn mmap_with_map_private_is_rejected() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "mmap".to_string(),
            location: SourceLocation {
                line_start: 100,
                line_end: 100,
                col_start: 5,
                col_end: 60,
            },
            source_file: "src/alloc.c".to_string(),
            enclosing_symbol_key: "test-repo:src/alloc.c#alloc_buffer:SYMBOL:function".to_string(),
            extracted_argument: None,
            argument_index: None,
            raw_argument_text: None,
            socket_family: None,
            mmap_flags: Some(MmapFlags::Private), // MAP_PRIVATE — not shared memory
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_none(),
            "mmap(MAP_PRIVATE) should not emit — not shared memory"
        );
    }

    #[test]
    fn mmap_with_map_shared_emits() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "mmap".to_string(),
            location: SourceLocation {
                line_start: 100,
                line_end: 100,
                col_start: 5,
                col_end: 60,
            },
            source_file: "src/shm.c".to_string(),
            enclosing_symbol_key: "test-repo:src/shm.c#map_shared:SYMBOL:function".to_string(),
            extracted_argument: None,
            argument_index: None,
            raw_argument_text: None,
            socket_family: None,
            mmap_flags: Some(MmapFlags::Shared), // MAP_SHARED — is shared memory
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_some(),
            "mmap(MAP_SHARED) should emit as shared_memory"
        );
        let facts = result.unwrap();
        assert_eq!(facts.surface.channel_kind, ChannelKind::SharedMemory);
        assert!(facts.requires_dual_projection);
    }

    #[test]
    fn mmap_without_flags_is_rejected() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "mmap".to_string(),
            location: SourceLocation {
                line_start: 100,
                line_end: 100,
                col_start: 5,
                col_end: 60,
            },
            source_file: "src/mem.c".to_string(),
            enclosing_symbol_key: "test-repo:src/mem.c#map_file:SYMBOL:function".to_string(),
            extracted_argument: None,
            argument_index: None,
            raw_argument_text: None,
            socket_family: None,
            mmap_flags: None, // No flag evidence
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_none(),
            "mmap() without flag evidence should not emit"
        );
    }

    #[test]
    fn mknod_without_s_ififo_is_rejected() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "mknod".to_string(),
            location: SourceLocation {
                line_start: 75,
                line_end: 75,
                col_start: 5,
                col_end: 50,
            },
            source_file: "src/dev.c".to_string(),
            enclosing_symbol_key: "test-repo:src/dev.c#create_device:SYMBOL:function".to_string(),
            extracted_argument: Some("/dev/mydev".to_string()),
            argument_index: Some(0),
            raw_argument_text: None,
            socket_family: None,
            mmap_flags: None,
            mknod_mode: Some(0o020666), // S_IFCHR (character device), not S_IFIFO
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_none(),
            "mknod() without S_IFIFO should not emit as named_pipe"
        );
    }

    #[test]
    fn mknod_with_s_ififo_emits() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "mknod".to_string(),
            location: SourceLocation {
                line_start: 75,
                line_end: 75,
                col_start: 5,
                col_end: 50,
            },
            source_file: "src/pipe.c".to_string(),
            enclosing_symbol_key: "test-repo:src/pipe.c#create_fifo:SYMBOL:function".to_string(),
            extracted_argument: Some("/tmp/myfifo".to_string()),
            argument_index: Some(0),
            raw_argument_text: None,
            socket_family: None,
            mmap_flags: None,
            mknod_mode: Some(S_IFIFO | 0o666), // S_IFIFO — is a FIFO
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_some(),
            "mknod(S_IFIFO) should emit as named_pipe"
        );
        assert_eq!(result.unwrap().surface.channel_kind, ChannelKind::NamedPipe);
    }

    #[test]
    fn mkfifo_emits_without_mode_guard() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "mkfifo".to_string(),
            location: SourceLocation {
                line_start: 80,
                line_end: 80,
                col_start: 5,
                col_end: 40,
            },
            source_file: "src/pipe.c".to_string(),
            enclosing_symbol_key: "test-repo:src/pipe.c#create_pipe:SYMBOL:function".to_string(),
            extracted_argument: Some("/tmp/fifo".to_string()),
            argument_index: Some(0),
            raw_argument_text: None,
            socket_family: None,
            mmap_flags: None,
            mknod_mode: None, // mkfifo doesn't need mode guard
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_some(),
            "mkfifo() always creates FIFO — no mode guard needed"
        );
        assert_eq!(result.unwrap().surface.channel_kind, ChannelKind::NamedPipe);
    }

    #[test]
    fn bind_with_unix_path_but_no_family_emits() {
        // bind/connect can emit if extracted_argument is present,
        // even without explicit socket_family (weak evidence)
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "bind".to_string(),
            location: SourceLocation {
                line_start: 100,
                line_end: 100,
                col_start: 5,
                col_end: 50,
            },
            source_file: "src/server.c".to_string(),
            enclosing_symbol_key: "test-repo:src/server.c#start:SYMBOL:function".to_string(),
            extracted_argument: Some("/var/run/app.sock".to_string()), // Path suggests Unix socket
            argument_index: Some(1),
            raw_argument_text: None,
            socket_family: None, // No explicit family, but we have a path
            mmap_flags: None,
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_some(),
            "bind() with Unix socket path should emit even without explicit family"
        );
    }

    #[test]
    fn bind_without_family_or_path_is_rejected() {
        let mut emitter = BoundaryInteractionEmitter::new(test_context());
        let callsite = BoundaryCallsite {
            language: Language::C,
            function_name: "bind".to_string(),
            location: SourceLocation {
                line_start: 100,
                line_end: 100,
                col_start: 5,
                col_end: 50,
            },
            source_file: "src/server.c".to_string(),
            enclosing_symbol_key: "test-repo:src/server.c#start:SYMBOL:function".to_string(),
            extracted_argument: None, // No path extracted
            argument_index: None,
            raw_argument_text: None,
            socket_family: None, // No family evidence
            mmap_flags: None,
            mknod_mode: None,
        };

        let result = emitter.try_emit(&callsite).unwrap();
        assert!(
            result.is_none(),
            "bind() without family or path evidence should not emit"
        );
    }
}
