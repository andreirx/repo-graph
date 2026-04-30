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
/// Contract: design doc section 4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryScope {
    /// Cross-process on same host (IPC).
    /// Examples: Unix sockets, pipes, shared memory, loopback TCP.
    InterProcess,

    /// Cross-device/host (network, bus, serial).
    /// Examples: TCP to remote host, CAN bus, serial port.
    InterDevice,

    /// Scope cannot be determined statically.
    /// Examples: TCP to config-sourced address, dynamic endpoint.
    Unknown,
    // NOTE: `InProcess` is reserved per design doc section 4.1.
    // It is deliberately NOT included in v1 to avoid muddying
    // semantics with call graph / state boundary overlap.
}

impl BoundaryScope {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            BoundaryScope::InterProcess => "inter_process",
            BoundaryScope::InterDevice => "inter_device",
            BoundaryScope::Unknown => "unknown",
        }
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

    // ── Slice 1B: Generic sockets (deferred) ──────────────────────
    /// TCP socket (AF_INET/AF_INET6 + SOCK_STREAM).
    TcpSocket,

    /// UDP socket (AF_INET/AF_INET6 + SOCK_DGRAM).
    UdpSocket,

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

    /// Whether this channel kind requires dual projection (boundary + state).
    /// Currently only shared memory per design doc section 4.4.
    pub const fn requires_dual_projection(self) -> bool {
        matches!(self, ChannelKind::SharedMemory)
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

    /// Shared memory.
    SharedMemory,

    /// Message queue.
    MessageQueue,

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
            ChannelKind::SharedMemory => ProtocolFamily::SharedMemory,
            ChannelKind::MessageQueue => ProtocolFamily::MessageQueue,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_scope_serializes_snake_case() {
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
    fn channel_kind_slice_1a_subset() {
        assert!(ChannelKind::UnixSocket.is_slice_1a());
        assert!(ChannelKind::SharedMemory.is_slice_1a());
        assert!(!ChannelKind::TcpSocket.is_slice_1a());
        assert!(!ChannelKind::SerialPort.is_slice_1a());
    }

    #[test]
    fn shared_memory_requires_dual_projection() {
        assert!(ChannelKind::SharedMemory.requires_dual_projection());
        assert!(!ChannelKind::UnixSocket.requires_dual_projection());
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
    }

    #[test]
    fn interaction_basis_confidence_ordering() {
        // Declaration is highest, inferred is lowest
        assert!(InteractionBasis::Declaration.default_confidence() > InteractionBasis::ApiCall.default_confidence());
        assert!(InteractionBasis::ApiCall.default_confidence() > InteractionBasis::Inferred.default_confidence());
    }
}
