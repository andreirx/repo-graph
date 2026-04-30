//! Level 2: Channel Detail.
//!
//! Mechanism-specific facts about the communication channel. One or more
//! channel details may be associated with a single `BoundaryInteractionSurface`.
//!
//! Contract: `docs/design/boundary-interaction-ipc-device.md` section 4.2.

use serde::{Deserialize, Serialize};

use crate::types::ChannelKind;

/// Level 2 channel detail fact.
///
/// Contains mechanism-specific addressing and protocol details for a
/// boundary interaction. The addressing fields are mutually exclusive
/// based on `channel_kind`.
///
/// ## Identity
///
/// `channel_uid` is the primary key. It is deterministically derived from:
/// - `surface_uid`
/// - `channel_identity`
///
/// ## Channel identity
///
/// `channel_identity` is a normalized key for matching providers and
/// consumers. Format depends on channel kind:
///
/// - Unix socket: path (e.g., `/var/run/daemon.sock`)
/// - Named pipe: path (e.g., `/tmp/myfifo`)
/// - Anonymous pipe: descriptor context (e.g., `pipe:parent_to_child`)
/// - Shared memory: key/name (e.g., `/my_shm_region`)
/// - Message queue: name (e.g., `/my_queue`)
/// - TCP/UDP: `host:port` (e.g., `*:8080`, `127.0.0.1:3000`)
/// - CAN: message ID (e.g., `can:0x123`)
/// - Serial: device path (e.g., `/dev/ttyUSB0`)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelDetail {
    // ── Identity ──────────────────────────────────────────────────
    /// Primary key. Deterministically derived from surface_uid + channel_identity.
    pub channel_uid: String,

    /// FK to the parent surface.
    pub surface_uid: String,

    // ── Channel identity ──────────────────────────────────────────
    /// Mechanism type (determines which addressing fields apply).
    pub channel_kind: ChannelKind,

    /// Normalized key for matching (format depends on channel_kind).
    pub channel_identity: String,

    // ── Addressing (mechanism-specific, at most one populated) ────
    /// Unix domain socket path (e.g., `/var/run/daemon.sock`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<String>,

    /// TCP endpoint (e.g., `*:8080`, `127.0.0.1:3000`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_endpoint: Option<String>,

    /// UDP endpoint (e.g., `*:5353`, `224.0.0.1:1900`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_endpoint: Option<String>,

    /// CAN message ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_id: Option<u32>,

    /// I2C device address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i2c_address: Option<u8>,

    /// SPI device path (e.g., `/dev/spidev0.0`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spi_device: Option<String>,

    /// Serial device path (e.g., `/dev/ttyUSB0`, `COM3`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_device: Option<String>,

    /// Shared memory key/name (e.g., `/my_shm_region`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_key: Option<String>,

    /// Named pipe / FIFO path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipe_path: Option<String>,

    /// Anonymous pipe descriptor context (for correlation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipe_context: Option<String>,

    /// POSIX message queue name (e.g., `/my_queue`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mqueue_name: Option<String>,

    // ── Protocol details (optional enrichment) ────────────────────
    /// Baud rate for serial/CAN.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baud_rate: Option<u32>,

    /// CAN extended ID flag (29-bit vs 11-bit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_extended: Option<bool>,

    /// Binary framing description (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_format: Option<String>,

    /// IDL/schema reference (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_contract: Option<String>,

    // ── Metadata ──────────────────────────────────────────────────
    /// Additional mechanism-specific metadata as JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

impl ChannelDetail {
    /// Build a deterministic channel UID.
    ///
    /// Format: `ch:<surface_uid>:<channel_identity_hash>`
    ///
    /// Uses a simple hash of the channel identity to keep UIDs readable
    /// while maintaining uniqueness.
    pub fn build_uid(surface_uid: &str, channel_identity: &str) -> String {
        // Simple hash for determinism without cryptographic overhead
        let hash: u64 = channel_identity
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        format!("ch:{}:{:016x}", surface_uid, hash)
    }

    /// Validate that the channel detail is internally consistent.
    pub fn validate(&self) -> Result<(), String> {
        if self.channel_uid.is_empty() {
            return Err("channel_uid is empty".to_string());
        }
        if self.surface_uid.is_empty() {
            return Err("surface_uid is empty".to_string());
        }
        if self.channel_identity.is_empty() {
            return Err("channel_identity is empty".to_string());
        }

        // Verify at least one addressing field is populated for non-anonymous channels
        if !matches!(self.channel_kind, ChannelKind::AnonymousPipe) {
            let has_addressing = self.socket_path.is_some()
                || self.tcp_endpoint.is_some()
                || self.udp_endpoint.is_some()
                || self.can_id.is_some()
                || self.i2c_address.is_some()
                || self.spi_device.is_some()
                || self.serial_device.is_some()
                || self.shm_key.is_some()
                || self.pipe_path.is_some()
                || self.mqueue_name.is_some();

            if !has_addressing {
                return Err(format!(
                    "channel_kind {:?} requires at least one addressing field",
                    self.channel_kind
                ));
            }
        }

        Ok(())
    }
}

/// Builder for constructing `ChannelDetail` instances.
#[derive(Debug, Default)]
pub struct ChannelDetailBuilder {
    surface_uid: Option<String>,
    channel_kind: Option<ChannelKind>,
    channel_identity: Option<String>,
    socket_path: Option<String>,
    tcp_endpoint: Option<String>,
    udp_endpoint: Option<String>,
    can_id: Option<u32>,
    i2c_address: Option<u8>,
    spi_device: Option<String>,
    serial_device: Option<String>,
    shm_key: Option<String>,
    pipe_path: Option<String>,
    pipe_context: Option<String>,
    mqueue_name: Option<String>,
    baud_rate: Option<u32>,
    can_extended: Option<bool>,
    frame_format: Option<String>,
    payload_contract: Option<String>,
    metadata_json: Option<String>,
}

impl ChannelDetailBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the parent surface UID.
    pub fn surface_uid(mut self, uid: impl Into<String>) -> Self {
        self.surface_uid = Some(uid.into());
        self
    }

    /// Set the channel kind.
    pub fn channel_kind(mut self, kind: ChannelKind) -> Self {
        self.channel_kind = Some(kind);
        self
    }

    /// Set the channel identity (normalized matching key).
    pub fn channel_identity(mut self, identity: impl Into<String>) -> Self {
        self.channel_identity = Some(identity.into());
        self
    }

    // ── Addressing setters ────────────────────────────────────────

    /// Set Unix socket path.
    pub fn socket_path(mut self, path: impl Into<String>) -> Self {
        self.socket_path = Some(path.into());
        self
    }

    /// Set TCP endpoint.
    pub fn tcp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.tcp_endpoint = Some(endpoint.into());
        self
    }

    /// Set UDP endpoint.
    pub fn udp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.udp_endpoint = Some(endpoint.into());
        self
    }

    /// Set CAN message ID.
    pub fn can_id(mut self, id: u32) -> Self {
        self.can_id = Some(id);
        self
    }

    /// Set I2C address.
    pub fn i2c_address(mut self, addr: u8) -> Self {
        self.i2c_address = Some(addr);
        self
    }

    /// Set SPI device path.
    pub fn spi_device(mut self, device: impl Into<String>) -> Self {
        self.spi_device = Some(device.into());
        self
    }

    /// Set serial device path.
    pub fn serial_device(mut self, device: impl Into<String>) -> Self {
        self.serial_device = Some(device.into());
        self
    }

    /// Set shared memory key.
    pub fn shm_key(mut self, key: impl Into<String>) -> Self {
        self.shm_key = Some(key.into());
        self
    }

    /// Set named pipe path.
    pub fn pipe_path(mut self, path: impl Into<String>) -> Self {
        self.pipe_path = Some(path.into());
        self
    }

    /// Set anonymous pipe context.
    pub fn pipe_context(mut self, ctx: impl Into<String>) -> Self {
        self.pipe_context = Some(ctx.into());
        self
    }

    /// Set message queue name.
    pub fn mqueue_name(mut self, name: impl Into<String>) -> Self {
        self.mqueue_name = Some(name.into());
        self
    }

    // ── Protocol detail setters ───────────────────────────────────

    /// Set baud rate.
    pub fn baud_rate(mut self, rate: u32) -> Self {
        self.baud_rate = Some(rate);
        self
    }

    /// Set CAN extended ID flag.
    pub fn can_extended(mut self, extended: bool) -> Self {
        self.can_extended = Some(extended);
        self
    }

    /// Set frame format description.
    pub fn frame_format(mut self, format: impl Into<String>) -> Self {
        self.frame_format = Some(format.into());
        self
    }

    /// Set payload contract reference.
    pub fn payload_contract(mut self, contract: impl Into<String>) -> Self {
        self.payload_contract = Some(contract.into());
        self
    }

    /// Set metadata JSON.
    pub fn metadata_json(mut self, json: impl Into<String>) -> Self {
        self.metadata_json = Some(json.into());
        self
    }

    /// Build the channel detail, validating all required fields.
    pub fn build(self) -> Result<ChannelDetail, String> {
        let surface_uid = self.surface_uid.ok_or("surface_uid is required")?;
        let channel_identity = self.channel_identity.ok_or("channel_identity is required")?;

        let channel_uid = ChannelDetail::build_uid(&surface_uid, &channel_identity);

        let detail = ChannelDetail {
            channel_uid,
            surface_uid,
            channel_kind: self.channel_kind.ok_or("channel_kind is required")?,
            channel_identity,
            socket_path: self.socket_path,
            tcp_endpoint: self.tcp_endpoint,
            udp_endpoint: self.udp_endpoint,
            can_id: self.can_id,
            i2c_address: self.i2c_address,
            spi_device: self.spi_device,
            serial_device: self.serial_device,
            shm_key: self.shm_key,
            pipe_path: self.pipe_path,
            pipe_context: self.pipe_context,
            mqueue_name: self.mqueue_name,
            baud_rate: self.baud_rate,
            can_extended: self.can_extended,
            frame_format: self.frame_format,
            payload_contract: self.payload_contract,
            metadata_json: self.metadata_json,
        };

        detail.validate()?;
        Ok(detail)
    }
}

// ── Convenience constructors for Slice 1A mechanisms ──────────────────

impl ChannelDetailBuilder {
    /// Create a Unix socket channel detail.
    pub fn unix_socket(surface_uid: impl Into<String>, path: impl Into<String>) -> Self {
        let path = path.into();
        Self::new()
            .surface_uid(surface_uid)
            .channel_kind(ChannelKind::UnixSocket)
            .channel_identity(&path)
            .socket_path(path)
    }

    /// Create a named pipe (FIFO) channel detail.
    pub fn named_pipe(surface_uid: impl Into<String>, path: impl Into<String>) -> Self {
        let path = path.into();
        Self::new()
            .surface_uid(surface_uid)
            .channel_kind(ChannelKind::NamedPipe)
            .channel_identity(&path)
            .pipe_path(path)
    }

    /// Create an anonymous pipe channel detail.
    pub fn anonymous_pipe(surface_uid: impl Into<String>, context: impl Into<String>) -> Self {
        let ctx = context.into();
        Self::new()
            .surface_uid(surface_uid)
            .channel_kind(ChannelKind::AnonymousPipe)
            .channel_identity(format!("pipe:{}", &ctx))
            .pipe_context(ctx)
    }

    /// Create a shared memory channel detail.
    pub fn shared_memory(surface_uid: impl Into<String>, key: impl Into<String>) -> Self {
        let key = key.into();
        Self::new()
            .surface_uid(surface_uid)
            .channel_kind(ChannelKind::SharedMemory)
            .channel_identity(&key)
            .shm_key(key)
    }

    /// Create a message queue channel detail.
    pub fn message_queue(surface_uid: impl Into<String>, name: impl Into<String>) -> Self {
        let name = name.into();
        Self::new()
            .surface_uid(surface_uid)
            .channel_kind(ChannelKind::MessageQueue)
            .channel_identity(&name)
            .mqueue_name(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_socket_builder() {
        let detail = ChannelDetailBuilder::unix_socket("surface-123", "/var/run/daemon.sock")
            .build()
            .unwrap();

        assert_eq!(detail.channel_kind, ChannelKind::UnixSocket);
        assert_eq!(detail.socket_path.as_deref(), Some("/var/run/daemon.sock"));
        assert_eq!(detail.channel_identity, "/var/run/daemon.sock");
        assert!(detail.validate().is_ok());
    }

    #[test]
    fn shared_memory_builder() {
        let detail = ChannelDetailBuilder::shared_memory("surface-456", "/my_shm")
            .build()
            .unwrap();

        assert_eq!(detail.channel_kind, ChannelKind::SharedMemory);
        assert_eq!(detail.shm_key.as_deref(), Some("/my_shm"));
        assert!(detail.validate().is_ok());
    }

    #[test]
    fn anonymous_pipe_no_addressing_required() {
        let detail = ChannelDetailBuilder::anonymous_pipe("surface-789", "parent_to_child")
            .build()
            .unwrap();

        assert_eq!(detail.channel_kind, ChannelKind::AnonymousPipe);
        assert!(detail.validate().is_ok());
    }

    #[test]
    fn uid_is_deterministic() {
        let uid1 = ChannelDetail::build_uid("surface-123", "/var/run/daemon.sock");
        let uid2 = ChannelDetail::build_uid("surface-123", "/var/run/daemon.sock");
        assert_eq!(uid1, uid2);
        assert!(uid1.starts_with("ch:"));
    }

    #[test]
    fn validation_requires_addressing_for_non_anonymous() {
        let result = ChannelDetailBuilder::new()
            .surface_uid("surface")
            .channel_kind(ChannelKind::UnixSocket)
            .channel_identity("/path")
            // Missing socket_path
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("addressing field"));
    }
}
