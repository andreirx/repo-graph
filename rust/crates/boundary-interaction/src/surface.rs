//! Level 1: Boundary Interaction Surface.
//!
//! High-level architectural relationship between communicating parties.
//! This is the primary fact emitted by extractors — one surface per
//! detected boundary interaction site.
//!
//! Contract: `docs/design/boundary-interaction-ipc-device.md` section 4.2.

use serde::{Deserialize, Serialize};

use crate::types::{
    BoundaryScope, ChannelKind, Direction, EndpointLocality, InteractionBasis, InteractionPattern,
    ProtocolFamily, TransportClass,
};

/// Level 1 boundary interaction fact.
///
/// Represents an architectural relationship at a specific source location
/// where code crosses a process or device boundary. The surface captures
/// the "what" and "where" of the interaction; the associated
/// `ChannelDetail` (Level 2) captures mechanism-specific addressing.
///
/// ## Identity
///
/// `surface_uid` is the primary key. It is deterministically derived from:
/// - `repo_uid`
/// - `source_file`
/// - `line_start`
/// - `channel_kind`
/// - `direction`
///
/// This ensures the same source location with the same characteristics
/// produces the same UID across re-indexing.
///
/// ## Relationship to other facts
///
/// - One surface may have multiple `ChannelDetail` records (e.g., a
///   function that creates sockets to multiple endpoints).
/// - Surfaces are local observations, not paired links. Matching
///   provider/consumer surfaces is a separate query-time operation.
/// - Shared memory surfaces require dual projection: emit to
///   `boundary_interaction_surfaces` AND to state-boundary tables
///   per design doc section 4.4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryInteractionSurface {
    // ── Identity ──────────────────────────────────────────────────
    /// Primary key. Deterministically derived.
    pub surface_uid: String,

    /// Snapshot this fact belongs to.
    pub snapshot_uid: String,

    /// Repository this fact belongs to.
    pub repo_uid: String,

    // ── Classification ────────────────────────────────────────────
    /// Scope of the boundary crossing.
    pub boundary_scope: BoundaryScope,

    /// Mechanism type (determines which ChannelDetail fields apply).
    pub channel_kind: ChannelKind,

    /// Role of this symbol in the interaction.
    pub direction: Direction,

    /// Transport mechanism class (orthogonal to scope).
    /// Added by multi-track boundary detection (migration 025).
    /// Optional for backward compatibility with BI-1A surfaces.
    pub transport_class: Option<TransportClass>,

    /// How the boundary fact was discovered.
    /// Added by multi-track boundary detection (migration 025).
    /// Optional for backward compatibility with BI-1A surfaces.
    pub provenance: Option<String>,

    /// Basis for confidence assessment.
    /// Added by multi-track boundary detection (migration 025).
    /// Optional for backward compatibility with BI-1A surfaces.
    pub confidence_basis: Option<String>,

    // ── Protocol ──────────────────────────────────────────────────
    /// Low-level protocol identifier (e.g., "unix", "tcp", "shm").
    pub protocol: String,

    /// Protocol family for filtering/grouping.
    pub protocol_family: ProtocolFamily,

    /// Communication pattern.
    pub interaction_pattern: InteractionPattern,

    // ── Locality ──────────────────────────────────────────────────
    /// Observable endpoint locality from this callsite.
    pub endpoint_locality: EndpointLocality,

    // ── Source provenance ─────────────────────────────────────────
    /// Stable key of the enclosing symbol (function/method).
    pub symbol_stable_key: String,

    /// Source file path (repo-relative).
    pub source_file: String,

    /// Start line of the interaction site.
    pub line_start: u32,

    /// End line of the interaction site.
    pub line_end: u32,

    /// Start column of the interaction site (for UID discrimination).
    pub col_start: u32,

    /// End column of the interaction site.
    pub col_end: u32,

    // ── Extraction provenance ─────────────────────────────────────
    /// Extractor that produced this fact (e.g., "c-ipc:0.1.0").
    pub extractor: String,

    /// How the interaction was detected.
    pub basis: InteractionBasis,

    /// Confidence score (0.0 to 1.0).
    pub confidence: f64,

    /// Structured evidence JSON (mechanism-specific).
    pub evidence_json: String,
}

impl BoundaryInteractionSurface {
    /// Build a deterministic surface UID from identifying attributes.
    ///
    /// Format: `bi:<repo_uid>:<file>:<line>:<col>:<kind>:<direction>`
    ///
    /// The UID is stable across re-indexing if the source location
    /// and classification remain unchanged.
    ///
    /// ## Column discriminator
    ///
    /// Column is included to distinguish multiple interactions on the same
    /// line (e.g., macro-expanded code, chained calls). Without column,
    /// `socket(); bind();` on one line would collapse to a single surface.
    pub fn build_uid(
        repo_uid: &str,
        source_file: &str,
        line_start: u32,
        col_start: u32,
        channel_kind: ChannelKind,
        direction: Direction,
    ) -> String {
        format!(
            "bi:{}:{}:{}:{}:{}:{}",
            repo_uid,
            source_file,
            line_start,
            col_start,
            channel_kind.as_str(),
            direction.as_str()
        )
    }

    /// Validate that the surface is internally consistent.
    ///
    /// Returns `Err` with a description if validation fails.
    pub fn validate(&self) -> Result<(), String> {
        // Confidence must be in [0.0, 1.0]
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(format!(
                "confidence {} out of range [0.0, 1.0]",
                self.confidence
            ));
        }

        // Line range must be valid
        if self.line_end < self.line_start {
            return Err(format!(
                "line_end {} < line_start {}",
                self.line_end, self.line_start
            ));
        }

        // Required strings must be non-empty
        if self.surface_uid.is_empty() {
            return Err("surface_uid is empty".to_string());
        }
        if self.snapshot_uid.is_empty() {
            return Err("snapshot_uid is empty".to_string());
        }
        if self.repo_uid.is_empty() {
            return Err("repo_uid is empty".to_string());
        }
        if self.symbol_stable_key.is_empty() {
            return Err("symbol_stable_key is empty".to_string());
        }
        if self.source_file.is_empty() {
            return Err("source_file is empty".to_string());
        }
        if self.protocol.is_empty() {
            return Err("protocol is empty".to_string());
        }
        if self.extractor.is_empty() {
            return Err("extractor is empty".to_string());
        }

        Ok(())
    }

    /// Whether this surface requires dual projection to state-boundary.
    pub fn requires_dual_projection(&self) -> bool {
        self.channel_kind.requires_dual_projection()
    }
}

/// Builder for constructing `BoundaryInteractionSurface` instances.
///
/// Provides a fluent API with validation at build time.
#[derive(Debug, Default)]
pub struct SurfaceBuilder {
    snapshot_uid: Option<String>,
    repo_uid: Option<String>,
    boundary_scope: Option<BoundaryScope>,
    channel_kind: Option<ChannelKind>,
    direction: Option<Direction>,
    transport_class: Option<TransportClass>,
    provenance: Option<String>,
    confidence_basis: Option<String>,
    protocol: Option<String>,
    interaction_pattern: Option<InteractionPattern>,
    endpoint_locality: Option<EndpointLocality>,
    symbol_stable_key: Option<String>,
    source_file: Option<String>,
    line_start: Option<u32>,
    line_end: Option<u32>,
    col_start: Option<u32>,
    col_end: Option<u32>,
    extractor: Option<String>,
    basis: Option<InteractionBasis>,
    confidence: Option<f64>,
    evidence_json: Option<String>,
}

impl SurfaceBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set snapshot UID.
    pub fn snapshot_uid(mut self, uid: impl Into<String>) -> Self {
        self.snapshot_uid = Some(uid.into());
        self
    }

    /// Set repository UID.
    pub fn repo_uid(mut self, uid: impl Into<String>) -> Self {
        self.repo_uid = Some(uid.into());
        self
    }

    /// Set boundary scope.
    pub fn boundary_scope(mut self, scope: BoundaryScope) -> Self {
        self.boundary_scope = Some(scope);
        self
    }

    /// Set channel kind.
    pub fn channel_kind(mut self, kind: ChannelKind) -> Self {
        self.channel_kind = Some(kind);
        self
    }

    /// Set direction.
    pub fn direction(mut self, dir: Direction) -> Self {
        self.direction = Some(dir);
        self
    }

    /// Set transport class (optional, for multi-track boundary detection).
    pub fn transport_class(mut self, class: TransportClass) -> Self {
        self.transport_class = Some(class);
        self
    }

    /// Set provenance (optional, for multi-track boundary detection).
    pub fn provenance(mut self, prov: impl Into<String>) -> Self {
        self.provenance = Some(prov.into());
        self
    }

    /// Set confidence basis (optional, for multi-track boundary detection).
    pub fn confidence_basis(mut self, basis: impl Into<String>) -> Self {
        self.confidence_basis = Some(basis.into());
        self
    }

    /// Set protocol identifier.
    pub fn protocol(mut self, proto: impl Into<String>) -> Self {
        self.protocol = Some(proto.into());
        self
    }

    /// Set interaction pattern.
    pub fn interaction_pattern(mut self, pattern: InteractionPattern) -> Self {
        self.interaction_pattern = Some(pattern);
        self
    }

    /// Set endpoint locality.
    pub fn endpoint_locality(mut self, locality: EndpointLocality) -> Self {
        self.endpoint_locality = Some(locality);
        self
    }

    /// Set enclosing symbol stable key.
    pub fn symbol_stable_key(mut self, key: impl Into<String>) -> Self {
        self.symbol_stable_key = Some(key.into());
        self
    }

    /// Set source file path.
    pub fn source_file(mut self, path: impl Into<String>) -> Self {
        self.source_file = Some(path.into());
        self
    }

    /// Set line range.
    pub fn lines(mut self, start: u32, end: u32) -> Self {
        self.line_start = Some(start);
        self.line_end = Some(end);
        self
    }

    /// Set column range.
    pub fn cols(mut self, start: u32, end: u32) -> Self {
        self.col_start = Some(start);
        self.col_end = Some(end);
        self
    }

    /// Set full location (lines and columns).
    pub fn location(mut self, line_start: u32, line_end: u32, col_start: u32, col_end: u32) -> Self {
        self.line_start = Some(line_start);
        self.line_end = Some(line_end);
        self.col_start = Some(col_start);
        self.col_end = Some(col_end);
        self
    }

    /// Set extractor identifier.
    pub fn extractor(mut self, name: impl Into<String>) -> Self {
        self.extractor = Some(name.into());
        self
    }

    /// Set detection basis.
    pub fn basis(mut self, basis: InteractionBasis) -> Self {
        self.basis = Some(basis);
        self
    }

    /// Set confidence score.
    pub fn confidence(mut self, conf: f64) -> Self {
        self.confidence = Some(conf);
        self
    }

    /// Set evidence JSON.
    pub fn evidence_json(mut self, json: impl Into<String>) -> Self {
        self.evidence_json = Some(json.into());
        self
    }

    /// Build the surface, validating all required fields.
    pub fn build(self) -> Result<BoundaryInteractionSurface, String> {
        let repo_uid = self.repo_uid.ok_or("repo_uid is required")?;
        let source_file = self.source_file.ok_or("source_file is required")?;
        let line_start = self.line_start.ok_or("line_start is required")?;
        let col_start = self.col_start.ok_or("col_start is required")?;
        let channel_kind = self.channel_kind.ok_or("channel_kind is required")?;
        let direction = self.direction.ok_or("direction is required")?;
        let basis = self.basis.ok_or("basis is required")?;

        let surface_uid =
            BoundaryInteractionSurface::build_uid(&repo_uid, &source_file, line_start, col_start, channel_kind, direction);

        let protocol_family = ProtocolFamily::from(channel_kind);
        let confidence = self.confidence.unwrap_or_else(|| basis.default_confidence());

        let surface = BoundaryInteractionSurface {
            surface_uid,
            snapshot_uid: self.snapshot_uid.ok_or("snapshot_uid is required")?,
            repo_uid,
            boundary_scope: self.boundary_scope.ok_or("boundary_scope is required")?,
            channel_kind,
            direction,
            transport_class: self.transport_class,
            provenance: self.provenance,
            confidence_basis: self.confidence_basis,
            protocol: self.protocol.ok_or("protocol is required")?,
            protocol_family,
            interaction_pattern: self
                .interaction_pattern
                .ok_or("interaction_pattern is required")?,
            endpoint_locality: self
                .endpoint_locality
                .ok_or("endpoint_locality is required")?,
            symbol_stable_key: self.symbol_stable_key.ok_or("symbol_stable_key is required")?,
            source_file,
            line_start,
            line_end: self.line_end.ok_or("line_end is required")?,
            col_start,
            col_end: self.col_end.ok_or("col_end is required")?,
            extractor: self.extractor.ok_or("extractor is required")?,
            basis,
            confidence,
            evidence_json: self.evidence_json.unwrap_or_else(|| "{}".to_string()),
        };

        surface.validate()?;
        Ok(surface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_surface() -> BoundaryInteractionSurface {
        SurfaceBuilder::new()
            .snapshot_uid("snap-123")
            .repo_uid("test-repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::UnixSocket)
            .direction(Direction::Provider)
            .protocol("unix")
            .interaction_pattern(InteractionPattern::Stream)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key("test-repo:src/main.c#start_server:SYMBOL:function")
            .source_file("src/main.c")
            .location(100, 105, 5, 50)
            .extractor("c-ipc:0.1.0")
            .basis(InteractionBasis::ApiCall)
            .build()
            .unwrap()
    }

    #[test]
    fn builder_produces_valid_surface() {
        let surface = test_surface();
        assert!(surface.validate().is_ok());
        assert_eq!(surface.boundary_scope, BoundaryScope::InterProcess);
        assert_eq!(surface.channel_kind, ChannelKind::UnixSocket);
    }

    #[test]
    fn uid_is_deterministic() {
        let uid1 = BoundaryInteractionSurface::build_uid(
            "repo",
            "src/main.c",
            100,
            5,
            ChannelKind::UnixSocket,
            Direction::Provider,
        );
        let uid2 = BoundaryInteractionSurface::build_uid(
            "repo",
            "src/main.c",
            100,
            5,
            ChannelKind::UnixSocket,
            Direction::Provider,
        );
        assert_eq!(uid1, uid2);
        assert!(uid1.starts_with("bi:"));
    }

    #[test]
    fn uid_distinguishes_by_column() {
        let uid1 = BoundaryInteractionSurface::build_uid(
            "repo",
            "src/main.c",
            100,
            5,  // col 5
            ChannelKind::UnixSocket,
            Direction::Provider,
        );
        let uid2 = BoundaryInteractionSurface::build_uid(
            "repo",
            "src/main.c",
            100,
            20, // col 20
            ChannelKind::UnixSocket,
            Direction::Provider,
        );
        assert_ne!(uid1, uid2, "different columns must produce different UIDs");
    }

    #[test]
    fn shared_memory_requires_dual_projection() {
        let surface = SurfaceBuilder::new()
            .snapshot_uid("snap")
            .repo_uid("repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::SharedMemory)
            .direction(Direction::Bidirectional)
            .protocol("shm")
            .interaction_pattern(InteractionPattern::SharedState)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key("repo:src/shm.c#init_shm:SYMBOL:function")
            .source_file("src/shm.c")
            .location(50, 55, 5, 40)
            .extractor("c-ipc:0.1.0")
            .basis(InteractionBasis::ApiCall)
            .build()
            .unwrap();

        assert!(surface.requires_dual_projection());
    }

    #[test]
    fn builder_validates_confidence_range() {
        let result = SurfaceBuilder::new()
            .snapshot_uid("snap")
            .repo_uid("repo")
            .boundary_scope(BoundaryScope::InterProcess)
            .channel_kind(ChannelKind::UnixSocket)
            .direction(Direction::Provider)
            .protocol("unix")
            .interaction_pattern(InteractionPattern::Stream)
            .endpoint_locality(EndpointLocality::SameHostNamed)
            .symbol_stable_key("repo:src/main.c#fn:SYMBOL:function")
            .source_file("src/main.c")
            .location(1, 1, 1, 10)
            .extractor("test")
            .basis(InteractionBasis::ApiCall)
            .confidence(1.5) // Invalid
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("confidence"));
    }
}
