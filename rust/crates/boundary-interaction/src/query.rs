//! Read-side support for boundary interaction queries.
//!
//! Defines the query DTOs and port trait for reading boundary interaction
//! facts from storage. The port trait is implemented by the storage adapter;
//! the DTOs are consumed by CLI and application services.
//!
//! Contract: `docs/design/boundary-interaction-ipc-device.md`
//!
//! ## Design
//!
//! - Filter struct for query parameters (all optional, AND semantics)
//! - List item DTO for listings (lighter than full surface)
//! - Detail DTO for show (full surface + channels)
//! - Summary DTO for aggregate stats
//!
//! ## Determinism
//!
//! All query results must be sorted deterministically:
//! - Primary: source_file (alphabetic)
//! - Secondary: line_start (ascending)
//! - Tertiary: col_start (ascending)
//!
//! This matches the project-wide "same input → same output" rule.

use serde::{Deserialize, Serialize};

use crate::types::{
    BoundaryScope, ChannelKind, Direction, EndpointLocality, InteractionBasis, InteractionPattern,
    ProtocolFamily, TransportClass,
};

// ── Filter DTO ───────────────────────────────────────────────────────

/// Filter parameters for boundary interaction queries.
///
/// All fields are optional. When multiple fields are set, they combine
/// with AND semantics. Missing fields match all values.
#[derive(Debug, Clone, Default)]
pub struct BoundaryInteractionFilter {
    /// Filter by channel kind (e.g., "unix_socket", "shared_memory").
    pub channel_kind: Option<ChannelKind>,

    /// Filter by boundary scope (e.g., "inter_process", "inter_device").
    pub boundary_scope: Option<BoundaryScope>,

    /// Filter by direction (e.g., "provider", "consumer").
    pub direction: Option<Direction>,

    /// Filter by protocol family (e.g., "socket", "pipe", "shared_memory").
    pub protocol_family: Option<ProtocolFamily>,

    /// Filter by source file path (exact match or prefix).
    pub file: Option<String>,

    /// Filter by file path prefix (matches files starting with this prefix).
    pub file_prefix: Option<String>,

    /// Filter by enclosing symbol stable key (exact match).
    pub symbol: Option<String>,

    /// Minimum confidence threshold (inclusive).
    pub min_confidence: Option<f64>,
}

impl BoundaryInteractionFilter {
    /// Create an empty filter that matches all surfaces.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set channel kind filter.
    pub fn with_channel_kind(mut self, kind: ChannelKind) -> Self {
        self.channel_kind = Some(kind);
        self
    }

    /// Set boundary scope filter.
    pub fn with_boundary_scope(mut self, scope: BoundaryScope) -> Self {
        self.boundary_scope = Some(scope);
        self
    }

    /// Set direction filter.
    pub fn with_direction(mut self, dir: Direction) -> Self {
        self.direction = Some(dir);
        self
    }

    /// Set protocol family filter.
    pub fn with_protocol_family(mut self, family: ProtocolFamily) -> Self {
        self.protocol_family = Some(family);
        self
    }

    /// Set file filter (exact match).
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Set file prefix filter.
    pub fn with_file_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.file_prefix = Some(prefix.into());
        self
    }

    /// Set symbol filter.
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Set minimum confidence filter.
    pub fn with_min_confidence(mut self, min: f64) -> Self {
        self.min_confidence = Some(min);
        self
    }
}

// ── List item DTO ────────────────────────────────────────────────────

/// Lightweight DTO for boundary interaction listings.
///
/// Contains the essential fields for display without the full evidence
/// and channel details. Use `BoundaryInteractionDetail` for drill-down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryInteractionListItem {
    /// Primary key.
    pub surface_uid: String,

    /// Source file path (repo-relative).
    pub source_file: String,

    /// Start line of the interaction site.
    pub line_start: u32,

    /// End line of the interaction site.
    pub line_end: u32,

    /// Mechanism type.
    pub channel_kind: ChannelKind,

    /// Scope of the boundary crossing.
    pub boundary_scope: BoundaryScope,

    /// Role in the interaction.
    pub direction: Direction,

    /// Transport class (Track B extension).
    /// `None` for surfaces extracted before migration 025.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport_class: Option<TransportClass>,

    /// Evidence provenance (e.g., "grpc-stub:UserService.GetUser").
    /// `None` for surfaces extracted before migration 025.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,

    /// Human-readable confidence basis (e.g., "schema-type-match").
    /// `None` for surfaces extracted before migration 025.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_basis: Option<String>,

    /// Protocol family.
    pub protocol_family: ProtocolFamily,

    /// Low-level protocol identifier.
    pub protocol: String,

    /// Communication pattern.
    pub interaction_pattern: InteractionPattern,

    /// Enclosing symbol stable key.
    pub symbol_stable_key: String,

    /// Confidence score.
    pub confidence: f64,

    /// How the interaction was detected.
    pub basis: InteractionBasis,

    /// Number of associated channel details.
    pub channel_count: u32,

    // ── Contract association (Track B orientation) ────────────────
    /// Primary contract name (for orientation).
    /// Shows the first associated contract's full_name (e.g., "api.v1.Greeter").
    /// None if no contract association exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_name: Option<String>,

    /// Primary contract kind (e.g., "grpc_service").
    /// None if no contract association exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_kind: Option<String>,
}

// ── Detail DTO ───────────────────────────────────────────────────────

/// Full detail DTO for boundary interaction drill-down.
///
/// Includes the complete surface plus all associated channel details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryInteractionDetail {
    // ── Surface identity ──────────────────────────────────────────
    /// Primary key.
    pub surface_uid: String,

    /// Snapshot this fact belongs to.
    pub snapshot_uid: String,

    /// Repository this fact belongs to.
    pub repo_uid: String,

    // ── Classification ────────────────────────────────────────────
    /// Scope of the boundary crossing.
    pub boundary_scope: BoundaryScope,

    /// Mechanism type.
    pub channel_kind: ChannelKind,

    /// Role in the interaction.
    pub direction: Direction,

    // ── Transport classification (Track B extension) ─────────────
    /// Transport class: raw socket, raw IPC, schema-RPC, etc.
    /// `None` for surfaces extracted before migration 025.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport_class: Option<TransportClass>,

    /// Evidence provenance (e.g., "grpc-stub:UserService.GetUser").
    /// `None` for surfaces extracted before migration 025.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,

    /// Human-readable confidence basis (e.g., "schema-type-match").
    /// `None` for surfaces extracted before migration 025.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_basis: Option<String>,

    // ── Protocol ──────────────────────────────────────────────────
    /// Low-level protocol identifier.
    pub protocol: String,

    /// Protocol family.
    pub protocol_family: ProtocolFamily,

    /// Communication pattern.
    pub interaction_pattern: InteractionPattern,

    // ── Locality ──────────────────────────────────────────────────
    /// Observable endpoint locality.
    pub endpoint_locality: EndpointLocality,

    // ── Source provenance ─────────────────────────────────────────
    /// Enclosing symbol stable key.
    pub symbol_stable_key: String,

    /// Source file path (repo-relative).
    pub source_file: String,

    /// Start line.
    pub line_start: u32,

    /// End line.
    pub line_end: u32,

    /// Start column.
    pub col_start: u32,

    /// End column.
    pub col_end: u32,

    // ── Extraction provenance ─────────────────────────────────────
    /// Extractor identifier.
    pub extractor: String,

    /// Detection basis.
    pub basis: InteractionBasis,

    /// Confidence score.
    pub confidence: f64,

    /// Structured evidence JSON.
    pub evidence_json: String,

    // ── Channel details ───────────────────────────────────────────
    /// Associated channel details (Level 2 facts).
    pub channels: Vec<BoundaryInteractionChannelView>,

    // ── Contract associations (Track B) ───────────────────────────
    /// Associated contract elements (schema-backed RPC).
    /// Empty for raw IPC surfaces without contract associations.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub contracts: Vec<BoundaryContractView>,
}

/// Channel detail view for inclusion in BoundaryInteractionDetail.
///
/// Flattened representation of ChannelDetail for JSON output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryInteractionChannelView {
    /// Primary key.
    pub channel_uid: String,

    /// Mechanism type.
    pub channel_kind: ChannelKind,

    /// Normalized matching key.
    pub channel_identity: String,

    // ── Addressing (mechanism-specific) ───────────────────────────
    /// Unix socket path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<String>,

    /// TCP endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp_endpoint: Option<String>,

    /// UDP endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub udp_endpoint: Option<String>,

    /// CAN message ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_id: Option<u32>,

    /// I2C address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i2c_address: Option<u8>,

    /// SPI device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spi_device: Option<String>,

    /// Serial device.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_device: Option<String>,

    /// Shared memory key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_key: Option<String>,

    /// Named pipe path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipe_path: Option<String>,

    /// Anonymous pipe context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipe_context: Option<String>,

    /// Message queue name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mqueue_name: Option<String>,

    // ── Protocol details ──────────────────────────────────────────
    /// Baud rate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baud_rate: Option<u32>,

    /// CAN extended ID flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_extended: Option<bool>,

    /// Frame format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_format: Option<String>,

    /// Payload contract reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_contract: Option<String>,

    /// Additional metadata JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

// ── Contract association DTO ─────────────────────────────────────────

/// Contract association view for boundary interaction detail.
///
/// Surfaces the contract element associated with a boundary surface,
/// typically for schema-backed RPC like gRPC. A single surface may have
/// multiple contract associations.
///
/// ## GR-1A context
///
/// For gRPC server implementation hints (GR-1A), this links the boundary
/// surface (the Java class extending *ImplBase) to the proto service
/// element (from contract_elements via CS-2A mapping).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryContractView {
    /// Association primary key.
    pub association_uid: String,

    /// Contract element UID (FK to contract_elements).
    /// None if the association exists but element was deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_element_uid: Option<String>,

    /// Contract kind (e.g., "grpc_service", "protobuf_message").
    pub contract_kind: String,

    /// Contract element fully qualified name (e.g., "api.v1.Greeter").
    /// None if element was deleted or not joined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_name: Option<String>,

    /// How the association was established.
    pub association_basis: String,

    /// Association confidence score.
    pub confidence: f64,

    /// Evidence JSON (optional, for detailed provenance).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_json: Option<String>,
}

// ── Summary DTO ──────────────────────────────────────────────────────

/// Aggregate summary of boundary interactions for a snapshot.
///
/// Provides rollup counts by various dimensions for orientation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryInteractionSummary {
    /// Total surface count.
    pub total_surfaces: usize,

    /// Total channel detail count.
    pub total_channels: usize,

    /// Count by channel kind.
    pub by_channel_kind: Vec<KindCount>,

    /// Count by boundary scope.
    pub by_boundary_scope: Vec<ScopeCount>,

    /// Count by direction.
    pub by_direction: Vec<DirectionCount>,

    /// Count by protocol family.
    pub by_protocol_family: Vec<FamilyCount>,

    /// Count by detection basis.
    pub by_basis: Vec<BasisCount>,

    /// Files with boundary interactions (sorted, deterministic).
    pub files_with_boundaries: Vec<String>,
}

/// Count by channel kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KindCount {
    /// The channel kind being counted.
    pub channel_kind: ChannelKind,
    /// Number of surfaces with this channel kind.
    pub count: usize,
}

/// Count by boundary scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeCount {
    /// The boundary scope being counted.
    pub boundary_scope: BoundaryScope,
    /// Number of surfaces with this scope.
    pub count: usize,
}

/// Count by direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectionCount {
    /// The direction being counted.
    pub direction: Direction,
    /// Number of surfaces with this direction.
    pub count: usize,
}

/// Count by protocol family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyCount {
    /// The protocol family being counted.
    pub protocol_family: ProtocolFamily,
    /// Number of surfaces in this family.
    pub count: usize,
}

/// Count by detection basis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BasisCount {
    /// The detection basis being counted.
    pub basis: InteractionBasis,
    /// Number of surfaces with this basis.
    pub count: usize,
}

// ── Read port trait ──────────────────────────────────────────────────

/// Read port for boundary interaction facts.
///
/// Implemented by the storage adapter. Methods return typed DTOs
/// for consumption by CLI and application services.
///
/// ## Error handling
///
/// All methods return `Result<T, BoundaryInteractionReadError>`.
/// Concrete error type is defined here (zero workspace deps constraint)
/// and mapped from storage errors by the adapter.
pub trait BoundaryInteractionReadPort {
    /// List boundary interaction surfaces with optional filtering.
    ///
    /// Returns lightweight list items sorted by (source_file, line_start, col_start).
    fn list_boundary_interactions(
        &self,
        snapshot_uid: &str,
        filter: &BoundaryInteractionFilter,
    ) -> Result<Vec<BoundaryInteractionListItem>, BoundaryInteractionReadError>;

    /// Get full detail for a single boundary interaction surface.
    ///
    /// Returns the surface with all associated channel details.
    /// Returns `Ok(None)` if the surface does not exist.
    fn get_boundary_interaction_detail(
        &self,
        surface_uid: &str,
    ) -> Result<Option<BoundaryInteractionDetail>, BoundaryInteractionReadError>;

    /// Get aggregate summary of boundary interactions for a snapshot.
    fn get_boundary_interaction_summary(
        &self,
        snapshot_uid: &str,
    ) -> Result<BoundaryInteractionSummary, BoundaryInteractionReadError>;
}

// ── Error type ───────────────────────────────────────────────────────

/// Error type for boundary interaction read operations.
///
/// Defined in the policy crate (zero workspace deps) so the storage
/// adapter can return it directly without type conversion.
#[derive(Debug, Clone)]
pub enum BoundaryInteractionReadError {
    /// Storage layer error.
    Storage(String),

    /// Invalid filter parameter.
    InvalidFilter(String),

    /// Snapshot not found.
    SnapshotNotFound(String),
}

impl std::fmt::Display for BoundaryInteractionReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoundaryInteractionReadError::Storage(msg) => write!(f, "storage error: {}", msg),
            BoundaryInteractionReadError::InvalidFilter(msg) => {
                write!(f, "invalid filter: {}", msg)
            }
            BoundaryInteractionReadError::SnapshotNotFound(uid) => {
                write!(f, "snapshot not found: {}", uid)
            }
        }
    }
}

impl std::error::Error for BoundaryInteractionReadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_default_matches_all() {
        let filter = BoundaryInteractionFilter::new();
        assert!(filter.channel_kind.is_none());
        assert!(filter.boundary_scope.is_none());
        assert!(filter.direction.is_none());
        assert!(filter.file.is_none());
    }

    #[test]
    fn filter_builder_chain() {
        let filter = BoundaryInteractionFilter::new()
            .with_channel_kind(ChannelKind::UnixSocket)
            .with_boundary_scope(BoundaryScope::InterProcess)
            .with_direction(Direction::Provider)
            .with_file("src/server.c");

        assert_eq!(filter.channel_kind, Some(ChannelKind::UnixSocket));
        assert_eq!(filter.boundary_scope, Some(BoundaryScope::InterProcess));
        assert_eq!(filter.direction, Some(Direction::Provider));
        assert_eq!(filter.file.as_deref(), Some("src/server.c"));
    }

    #[test]
    fn list_item_serializes_snake_case() {
        let item = BoundaryInteractionListItem {
            surface_uid: "test".to_string(),
            source_file: "src/main.c".to_string(),
            line_start: 100,
            line_end: 105,
            channel_kind: ChannelKind::UnixSocket,
            boundary_scope: BoundaryScope::InterProcess,
            direction: Direction::Provider,
            transport_class: Some(TransportClass::RawIpc),
            provenance: Some("api:posix:bind".to_string()),
            confidence_basis: Some("api_call-match".to_string()),
            protocol_family: ProtocolFamily::Socket,
            protocol: "unix".to_string(),
            interaction_pattern: InteractionPattern::Stream,
            symbol_stable_key: "repo:src/main.c#fn:SYMBOL:function".to_string(),
            confidence: 0.95,
            basis: InteractionBasis::ApiCall,
            channel_count: 1,
            contract_name: None,
            contract_kind: None,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"channelKind\":\"unix_socket\""));
        assert!(json.contains("\"boundaryScope\":\"inter_process\""));
    }

    #[test]
    fn summary_counts_serialize() {
        let summary = BoundaryInteractionSummary {
            total_surfaces: 10,
            total_channels: 15,
            by_channel_kind: vec![KindCount {
                channel_kind: ChannelKind::UnixSocket,
                count: 5,
            }],
            by_boundary_scope: vec![ScopeCount {
                boundary_scope: BoundaryScope::InterProcess,
                count: 8,
            }],
            by_direction: vec![DirectionCount {
                direction: Direction::Provider,
                count: 4,
            }],
            by_protocol_family: vec![FamilyCount {
                protocol_family: ProtocolFamily::Socket,
                count: 7,
            }],
            by_basis: vec![BasisCount {
                basis: InteractionBasis::ApiCall,
                count: 10,
            }],
            files_with_boundaries: vec!["src/a.c".to_string(), "src/b.c".to_string()],
        };

        let json = serde_json::to_string_pretty(&summary).unwrap();
        assert!(json.contains("\"totalSurfaces\": 10"));
        assert!(json.contains("\"filesWithBoundaries\""));
    }

    #[test]
    fn error_display() {
        let err = BoundaryInteractionReadError::Storage("connection failed".to_string());
        assert_eq!(err.to_string(), "storage error: connection failed");

        let err = BoundaryInteractionReadError::SnapshotNotFound("snap-123".to_string());
        assert_eq!(err.to_string(), "snapshot not found: snap-123");
    }
}
