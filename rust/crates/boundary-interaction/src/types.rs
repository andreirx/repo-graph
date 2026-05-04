//! Core domain types for boundary interaction modeling.
//!
//! These enums define the vocabulary for IPC and inter-device boundary
//! interactions. They are the stable substrate consumed by extractors,
//! storage, and query surfaces.
//!
//! Contract: `docs/design/boundary-interaction-ipc-device.md` section 4.

use serde::{Deserialize, Serialize};

// ── Boundary scope ────────────────────────────────────────────────────

/// Scope of a boundary interaction — where the communication crosses.
///
/// This is a first-class attribute, not inferred from mechanism type.
/// The same mechanism (e.g., TCP socket) can be used for inter-process
/// (loopback) or inter-device (remote host) communication.
///
/// `boundary_scope` and `transport_class` are orthogonal dimensions:
/// - `boundary_scope` answers: "What physical/process boundary does this cross?"
/// - `transport_class` answers: "What kind of mechanism carries this interaction?"
///
/// Contract: design doc section 4.1.
/// Extended by: `docs/design/boundary-detection-multitrack.md`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryScope {
    /// Same OS process, different execution contexts.
    /// Examples: SharedArrayBuffer between main thread and Web Worker,
    /// thread-shared memory, V8 isolates.
    /// Added for multi-track boundary detection (BI-1C).
    IntraProcess,

    /// Cross-process on same host (IPC).
    /// Examples: Unix sockets, pipes, shared memory, loopback TCP.
    InterProcess,

    /// Cross-device/host (network, bus, serial).
    /// Examples: TCP to remote host, CAN bus, serial port.
    InterDevice,

    /// Scope cannot be determined statically.
    /// Examples: TCP to config-sourced address, dynamic endpoint.
    Unknown,
}

impl BoundaryScope {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            BoundaryScope::IntraProcess => "intra_process",
            BoundaryScope::InterProcess => "inter_process",
            BoundaryScope::InterDevice => "inter_device",
            BoundaryScope::Unknown => "unknown",
        }
    }
}

// ── Transport class ───────────────────────────────────────────────────

/// Transport mechanism class — orthogonal to boundary scope.
///
/// Answers: "What kind of mechanism carries this interaction?"
/// This is distinct from `BoundaryScope` which answers where the
/// boundary is crossed.
///
/// Examples:
/// - gRPC call over TCP to remote host: scope=inter_device, class=schema_rpc
/// - gRPC call over Unix socket to local service: scope=inter_process, class=schema_rpc
/// - Raw TCP socket to localhost: scope=inter_process, class=raw_socket
/// - SharedArrayBuffer: scope=intra_process, class=raw_ipc
///
/// Contract: `docs/design/boundary-detection-multitrack.md`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportClass {
    /// Bare TCP/UDP/Unix socket without higher-level framing.
    RawSocket,

    /// Pipes, shared memory, message queues.
    RawIpc,

    /// Contract-backed RPC: protobuf, gRPC, eRPC.
    SchemaRpc,

    /// Message broker: Kafka, RabbitMQ, etc. (future).
    MessageBroker,

    /// Application-specific protocol framing.
    CustomProtocol,
}

impl TransportClass {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            TransportClass::RawSocket => "raw_socket",
            TransportClass::RawIpc => "raw_ipc",
            TransportClass::SchemaRpc => "schema_rpc",
            TransportClass::MessageBroker => "message_broker",
            TransportClass::CustomProtocol => "custom_protocol",
        }
    }

    /// Whether this transport class is contract/schema-backed.
    pub const fn is_schema_backed(self) -> bool {
        matches!(self, TransportClass::SchemaRpc)
    }
}

// ── Endpoint locality ─────────────────────────────────────────────────

/// Observable locality of the endpoint from the callsite.
///
/// Distinct from `BoundaryScope`: scope is about architectural boundary
/// crossing, locality is about what can be determined from the code at
/// this specific callsite.
///
/// Contract: design doc section 4.2 (corrections applied).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointLocality {
    /// Literal loopback address (127.0.0.1, ::1, localhost).
    /// Strong signal for `inter_process` scope.
    Loopback,

    /// Named same-host resource (Unix socket path, shm key, mqueue name).
    /// Strong signal for `inter_process` scope.
    SameHostNamed,

    /// Literal non-loopback IP or hostname.
    /// Strong signal for `inter_device` scope.
    RemoteLiteral,

    /// Locality cannot be determined (variable, config-sourced).
    Unknown,
}

impl EndpointLocality {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            EndpointLocality::Loopback => "loopback",
            EndpointLocality::SameHostNamed => "same_host_named",
            EndpointLocality::RemoteLiteral => "remote_literal",
            EndpointLocality::Unknown => "unknown",
        }
    }
}

// ── Interaction pattern ───────────────────────────────────────────────

/// Communication pattern of the boundary interaction.
///
/// Contract: design doc section 4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionPattern {
    /// Request/response (synchronous or async with reply).
    RequestResponse,

    /// Publish/subscribe (one-to-many, decoupled).
    PublishSubscribe,

    /// Continuous stream (ordered sequence of messages).
    Stream,

    /// Fire and forget (no acknowledgment expected).
    FireAndForget,

    /// Shared state access (memory-mapped, concurrent).
    /// Used for shared memory — requires dual projection per design
    /// doc section 4.4.
    SharedState,

    /// Unknown pattern (used for hint-grade surfaces where pattern cannot
    /// be determined, e.g., GR-1A implementation hints).
    Unknown,
}

impl InteractionPattern {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            InteractionPattern::RequestResponse => "request_response",
            InteractionPattern::PublishSubscribe => "publish_subscribe",
            InteractionPattern::Stream => "stream",
            InteractionPattern::FireAndForget => "fire_and_forget",
            InteractionPattern::SharedState => "shared_state",
            InteractionPattern::Unknown => "unknown",
        }
    }
}

// ── Channel kind ──────────────────────────────────────────────────────

/// Mechanism-specific channel type.
///
/// Protocol-focused naming (not `ipc_` prefixed) per design doc Q1
/// resolution. Scope is a separate dimension via `BoundaryScope`.
///
/// Slice 1A subset: `UnixSocket`, `NamedPipe`, `AnonymousPipe`,
/// `SharedMemory`, `MessageQueue`.
///
/// Contract: design doc section 4.2 (ChannelDetail).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    // ── Slice 1A: Local IPC ───────────────────────────────────────
    /// Unix domain socket (AF_UNIX).
    UnixSocket,

    /// Named pipe / FIFO (mkfifo).
    NamedPipe,

    /// Anonymous pipe (pipe, pipe2).
    AnonymousPipe,

    /// POSIX shared memory (shm_open, mmap MAP_SHARED).
    SharedMemory,

    /// POSIX message queue (mq_open).
    MessageQueue,

    // ── Slice 1B: Generic sockets ───────────────────────────────────
    /// TCP socket (AF_INET/AF_INET6 + SOCK_STREAM).
    TcpSocket,

    /// UDP socket (AF_INET/AF_INET6 + SOCK_DGRAM).
    UdpSocket,

    // ── Slice 1C: SharedArrayBuffer (JS/TS worker boundaries) ─────
    /// SharedArrayBuffer for JS/TS worker communication.
    /// Scope: intra_process (same OS process, different execution contexts).
    SharedArrayBuffer,

    // ── Track B: Schema-backed RPC ────────────────────────────────
    /// gRPC channel (protobuf + HTTP/2).
    /// Transport class: schema_rpc.
    GrpcChannel,

    /// Raw protobuf stream (without gRPC transport).
    /// Transport class: schema_rpc.
    ProtobufStream,

    /// Embedded RPC (eRPC) channel.
    /// Transport class: schema_rpc.
    ErpcChannel,

    // ── Slice 2: Serial/CAN (deferred) ────────────────────────────
    /// Serial port (/dev/tty*, COM*).
    SerialPort,

    /// CAN bus message (AF_CAN).
    CanMessage,

    // ── Slice 3: Library wrappers (deferred) ──────────────────────
    /// MQTT topic.
    MqttTopic,

    /// D-Bus interface.
    DbusInterface,

    /// ZeroMQ socket.
    ZeromqSocket,

    // ── Slice 4: Device protocols (deferred) ──────────────────────
    /// I2C device.
    I2cDevice,

    /// SPI device.
    SpiDevice,

    /// USB endpoint.
    UsbEndpoint,

    /// BLE characteristic.
    BleCharacteristic,

    /// Modbus register.
    ModbusRegister,

    /// Custom / unrecognized protocol.
    CustomProtocol,
}

impl ChannelKind {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            ChannelKind::UnixSocket => "unix_socket",
            ChannelKind::NamedPipe => "named_pipe",
            ChannelKind::AnonymousPipe => "anonymous_pipe",
            ChannelKind::SharedMemory => "shared_memory",
            ChannelKind::MessageQueue => "message_queue",
            ChannelKind::TcpSocket => "tcp_socket",
            ChannelKind::UdpSocket => "udp_socket",
            ChannelKind::SharedArrayBuffer => "shared_array_buffer",
            ChannelKind::GrpcChannel => "grpc_channel",
            ChannelKind::ProtobufStream => "protobuf_stream",
            ChannelKind::ErpcChannel => "erpc_channel",
            ChannelKind::SerialPort => "serial_port",
            ChannelKind::CanMessage => "can_message",
            ChannelKind::MqttTopic => "mqtt_topic",
            ChannelKind::DbusInterface => "dbus_interface",
            ChannelKind::ZeromqSocket => "zeromq_socket",
            ChannelKind::I2cDevice => "i2c_device",
            ChannelKind::SpiDevice => "spi_device",
            ChannelKind::UsbEndpoint => "usb_endpoint",
            ChannelKind::BleCharacteristic => "ble_characteristic",
            ChannelKind::ModbusRegister => "modbus_register",
            ChannelKind::CustomProtocol => "custom_protocol",
        }
    }

    /// Whether this channel kind is in scope for Slice 1A (local IPC).
    pub const fn is_slice_1a(self) -> bool {
        matches!(
            self,
            ChannelKind::UnixSocket
                | ChannelKind::NamedPipe
                | ChannelKind::AnonymousPipe
                | ChannelKind::SharedMemory
                | ChannelKind::MessageQueue
        )
    }

    /// Whether this channel kind is in scope for Slice 1B (TCP/UDP sockets).
    pub const fn is_slice_1b(self) -> bool {
        matches!(self, ChannelKind::TcpSocket | ChannelKind::UdpSocket)
    }

    /// Whether this channel kind is in scope for Slice 1C (SharedArrayBuffer).
    pub const fn is_slice_1c(self) -> bool {
        matches!(self, ChannelKind::SharedArrayBuffer)
    }

    /// Whether this channel kind is schema-backed RPC (Track B).
    pub const fn is_schema_backed(self) -> bool {
        matches!(
            self,
            ChannelKind::GrpcChannel | ChannelKind::ProtobufStream | ChannelKind::ErpcChannel
        )
    }

    /// Whether this channel kind requires dual projection (boundary + state).
    /// SharedMemory and SharedArrayBuffer both represent shared state.
    pub const fn requires_dual_projection(self) -> bool {
        matches!(
            self,
            ChannelKind::SharedMemory | ChannelKind::SharedArrayBuffer
        )
    }

    /// Default transport class for this channel kind.
    pub const fn default_transport_class(self) -> TransportClass {
        match self {
            // Raw sockets
            ChannelKind::UnixSocket
            | ChannelKind::TcpSocket
            | ChannelKind::UdpSocket => TransportClass::RawSocket,

            // Raw IPC
            ChannelKind::NamedPipe
            | ChannelKind::AnonymousPipe
            | ChannelKind::SharedMemory
            | ChannelKind::MessageQueue
            | ChannelKind::SharedArrayBuffer => TransportClass::RawIpc,

            // Schema-backed RPC
            ChannelKind::GrpcChannel
            | ChannelKind::ProtobufStream
            | ChannelKind::ErpcChannel => TransportClass::SchemaRpc,

            // Message brokers
            ChannelKind::MqttTopic
            | ChannelKind::DbusInterface
            | ChannelKind::ZeromqSocket => TransportClass::MessageBroker,

            // Custom/other
            ChannelKind::SerialPort
            | ChannelKind::CanMessage
            | ChannelKind::I2cDevice
            | ChannelKind::SpiDevice
            | ChannelKind::UsbEndpoint
            | ChannelKind::BleCharacteristic
            | ChannelKind::ModbusRegister
            | ChannelKind::CustomProtocol => TransportClass::CustomProtocol,
        }
    }
}

// ── Protocol family ───────────────────────────────────────────────────

/// High-level protocol family grouping.
///
/// Used for filtering and categorization, not for precise identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolFamily {
    /// Socket-based (Unix, TCP, UDP).
    Socket,

    /// Pipe-based (named, anonymous).
    Pipe,

    /// Shared memory (POSIX shm, mmap, SharedArrayBuffer).
    SharedMemory,

    /// Message queue.
    MessageQueue,

    /// Schema-backed RPC (gRPC, protobuf, eRPC).
    Rpc,

    /// Serial communication.
    Serial,

    /// Bus protocols (CAN, I2C, SPI).
    Bus,

    /// Message broker / pub-sub.
    MessageBroker,

    /// USB.
    Usb,

    /// Bluetooth.
    Bluetooth,

    /// Custom / unrecognized.
    Custom,
}

impl ProtocolFamily {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProtocolFamily::Socket => "socket",
            ProtocolFamily::Pipe => "pipe",
            ProtocolFamily::SharedMemory => "shared_memory",
            ProtocolFamily::MessageQueue => "message_queue",
            ProtocolFamily::Rpc => "rpc",
            ProtocolFamily::Serial => "serial",
            ProtocolFamily::Bus => "bus",
            ProtocolFamily::MessageBroker => "message_broker",
            ProtocolFamily::Usb => "usb",
            ProtocolFamily::Bluetooth => "bluetooth",
            ProtocolFamily::Custom => "custom",
        }
    }
}

impl From<ChannelKind> for ProtocolFamily {
    fn from(kind: ChannelKind) -> Self {
        match kind {
            ChannelKind::UnixSocket | ChannelKind::TcpSocket | ChannelKind::UdpSocket => {
                ProtocolFamily::Socket
            }
            ChannelKind::NamedPipe | ChannelKind::AnonymousPipe => ProtocolFamily::Pipe,
            ChannelKind::SharedMemory | ChannelKind::SharedArrayBuffer => {
                ProtocolFamily::SharedMemory
            }
            ChannelKind::MessageQueue => ProtocolFamily::MessageQueue,
            ChannelKind::GrpcChannel | ChannelKind::ProtobufStream | ChannelKind::ErpcChannel => {
                ProtocolFamily::Rpc
            }
            ChannelKind::SerialPort => ProtocolFamily::Serial,
            ChannelKind::CanMessage | ChannelKind::I2cDevice | ChannelKind::SpiDevice => {
                ProtocolFamily::Bus
            }
            ChannelKind::MqttTopic | ChannelKind::DbusInterface | ChannelKind::ZeromqSocket => {
                ProtocolFamily::MessageBroker
            }
            ChannelKind::UsbEndpoint => ProtocolFamily::Usb,
            ChannelKind::BleCharacteristic => ProtocolFamily::Bluetooth,
            ChannelKind::ModbusRegister | ChannelKind::CustomProtocol => ProtocolFamily::Custom,
        }
    }
}

// ── Direction ─────────────────────────────────────────────────────────

/// Role of the symbol in the boundary interaction.
///
/// Not all mechanisms have clear provider/consumer roles. Shared memory
/// and serial ports are typically bidirectional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Provides the service / listens for connections.
    /// Examples: bind+listen on socket, mkfifo creator, shm_open O_CREAT.
    Provider,

    /// Consumes the service / initiates connection.
    /// Examples: connect on socket, open FIFO for read.
    Consumer,

    /// Both directions (peer-to-peer, shared state).
    /// Examples: shared memory read+write, serial port.
    Bidirectional,
}

impl Direction {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Direction::Provider => "provider",
            Direction::Consumer => "consumer",
            Direction::Bidirectional => "bidirectional",
        }
    }
}

// ── Interaction basis ─────────────────────────────────────────────────

/// How the boundary interaction was detected.
///
/// Extraction provenance for confidence assessment and debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionBasis {
    /// Direct API call (socket, bind, connect, shm_open, etc.).
    ApiCall,

    /// Library wrapper over raw API (ZeroMQ, nanomsg, Paho MQTT).
    WrapperCall,

    /// Attribute/decorator declaring the boundary.
    Annotation,

    /// Naming pattern (e.g., *_handler, *_callback).
    Convention,

    /// User-declared via `rmap declare`.
    Declaration,

    /// Heuristic-derived (low confidence).
    Inferred,

    /// Class extends generated ImplBase (gRPC server implementation pattern).
    /// Used by GR-1A: confidence is moderate (0.85) because the inheritance
    /// pattern is strong evidence but we lack runtime proof.
    ExtendsImplBase,

    /// Code creates a gRPC client stub (newBlockingStub, newFutureStub, newStub).
    /// Used by GR-2A: confidence is moderate (0.85) because stub creation
    /// is strong evidence of client intent but we lack runtime proof.
    StubCreation,
}

impl InteractionBasis {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            InteractionBasis::ApiCall => "api_call",
            InteractionBasis::WrapperCall => "wrapper_call",
            InteractionBasis::Annotation => "annotation",
            InteractionBasis::Convention => "convention",
            InteractionBasis::Declaration => "declaration",
            InteractionBasis::Inferred => "inferred",
            InteractionBasis::ExtendsImplBase => "extends_impl_base",
            InteractionBasis::StubCreation => "stub_creation",
        }
    }

    /// Default confidence score for this basis type.
    pub const fn default_confidence(self) -> f64 {
        match self {
            InteractionBasis::ApiCall => 0.95,
            InteractionBasis::WrapperCall => 0.90,
            InteractionBasis::Annotation => 0.99,
            InteractionBasis::Convention => 0.70,
            InteractionBasis::Declaration => 1.0,
            InteractionBasis::Inferred => 0.50,
            InteractionBasis::ExtendsImplBase => 0.85,
            InteractionBasis::StubCreation => 0.85,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_scope_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&BoundaryScope::IntraProcess).unwrap(),
            "\"intra_process\""
        );
        assert_eq!(
            serde_json::to_string(&BoundaryScope::InterProcess).unwrap(),
            "\"inter_process\""
        );
        assert_eq!(
            serde_json::to_string(&BoundaryScope::InterDevice).unwrap(),
            "\"inter_device\""
        );
    }

    #[test]
    fn transport_class_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&TransportClass::RawSocket).unwrap(),
            "\"raw_socket\""
        );
        assert_eq!(
            serde_json::to_string(&TransportClass::SchemaRpc).unwrap(),
            "\"schema_rpc\""
        );
    }

    #[test]
    fn channel_kind_slice_1a_subset() {
        assert!(ChannelKind::UnixSocket.is_slice_1a());
        assert!(ChannelKind::SharedMemory.is_slice_1a());
        assert!(!ChannelKind::TcpSocket.is_slice_1a());
        assert!(!ChannelKind::SerialPort.is_slice_1a());
    }

    #[test]
    fn channel_kind_slice_1b_subset() {
        assert!(ChannelKind::TcpSocket.is_slice_1b());
        assert!(ChannelKind::UdpSocket.is_slice_1b());
        assert!(!ChannelKind::UnixSocket.is_slice_1b());
    }

    #[test]
    fn channel_kind_slice_1c_subset() {
        assert!(ChannelKind::SharedArrayBuffer.is_slice_1c());
        assert!(!ChannelKind::SharedMemory.is_slice_1c());
    }

    #[test]
    fn channel_kind_schema_backed() {
        assert!(ChannelKind::GrpcChannel.is_schema_backed());
        assert!(ChannelKind::ProtobufStream.is_schema_backed());
        assert!(ChannelKind::ErpcChannel.is_schema_backed());
        assert!(!ChannelKind::TcpSocket.is_schema_backed());
        assert!(!ChannelKind::UnixSocket.is_schema_backed());
    }

    #[test]
    fn shared_memory_requires_dual_projection() {
        assert!(ChannelKind::SharedMemory.requires_dual_projection());
        assert!(ChannelKind::SharedArrayBuffer.requires_dual_projection());
        assert!(!ChannelKind::UnixSocket.requires_dual_projection());
        assert!(!ChannelKind::GrpcChannel.requires_dual_projection());
    }

    #[test]
    fn channel_kind_default_transport_class() {
        assert_eq!(
            ChannelKind::TcpSocket.default_transport_class(),
            TransportClass::RawSocket
        );
        assert_eq!(
            ChannelKind::UnixSocket.default_transport_class(),
            TransportClass::RawSocket
        );
        assert_eq!(
            ChannelKind::SharedMemory.default_transport_class(),
            TransportClass::RawIpc
        );
        assert_eq!(
            ChannelKind::SharedArrayBuffer.default_transport_class(),
            TransportClass::RawIpc
        );
        assert_eq!(
            ChannelKind::GrpcChannel.default_transport_class(),
            TransportClass::SchemaRpc
        );
        assert_eq!(
            ChannelKind::MqttTopic.default_transport_class(),
            TransportClass::MessageBroker
        );
    }

    #[test]
    fn protocol_family_from_channel_kind() {
        assert_eq!(
            ProtocolFamily::from(ChannelKind::UnixSocket),
            ProtocolFamily::Socket
        );
        assert_eq!(
            ProtocolFamily::from(ChannelKind::NamedPipe),
            ProtocolFamily::Pipe
        );
        assert_eq!(
            ProtocolFamily::from(ChannelKind::SharedMemory),
            ProtocolFamily::SharedMemory
        );
        assert_eq!(
            ProtocolFamily::from(ChannelKind::SharedArrayBuffer),
            ProtocolFamily::SharedMemory
        );
        assert_eq!(
            ProtocolFamily::from(ChannelKind::GrpcChannel),
            ProtocolFamily::Rpc
        );
    }

    #[test]
    fn interaction_basis_confidence_ordering() {
        // Declaration is highest, inferred is lowest
        assert!(InteractionBasis::Declaration.default_confidence() > InteractionBasis::ApiCall.default_confidence());
        assert!(InteractionBasis::ApiCall.default_confidence() > InteractionBasis::Inferred.default_confidence());
    }
}
