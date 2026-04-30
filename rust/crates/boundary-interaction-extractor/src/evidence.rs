//! Evidence struct for boundary interaction extraction provenance.
//!
//! Analogous to `StateBoundaryEvidence` in `state-extractor`. Captures
//! the extraction provenance for debugging and confidence assessment.

use serde::{Deserialize, Serialize};

use repo_graph_boundary_interaction::{
    BoundaryScope, ChannelKind, Direction, EndpointLocality, InteractionBasis, InteractionPattern,
};

/// Schema version for the evidence JSON format.
///
/// Bump this when the `BoundaryInteractionEvidence` struct changes in a
/// backwards-incompatible way. Consumers can check the version before
/// attempting to parse.
pub const BOUNDARY_INTERACTION_EVIDENCE_VERSION: u32 = 1;

/// Structured evidence for a boundary interaction fact.
///
/// Serialized as JSON in the `evidence_json` field of
/// `boundary_interaction_surfaces`. The evidence captures the extraction
/// provenance so that downstream tools can assess confidence and debug
/// false positives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryInteractionEvidence {
    /// Evidence schema version.
    pub version: u32,

    /// Binding key that matched (e.g., "c:posix_socket:bind:provider").
    pub binding_key: String,

    /// API family from the binding table (e.g., "posix_socket").
    pub api_family: String,

    /// Function name that was detected.
    pub function_name: String,

    /// Detection basis.
    pub basis: InteractionBasis,

    /// Channel kind determined from binding.
    pub channel_kind: ChannelKind,

    /// Direction determined from binding role.
    pub direction: Direction,

    /// Boundary scope (inter_process, inter_device, unknown).
    pub boundary_scope: BoundaryScope,

    /// Endpoint locality observation.
    pub endpoint_locality: EndpointLocality,

    /// Interaction pattern.
    pub interaction_pattern: InteractionPattern,

    /// Extracted address/path/key (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_address: Option<String>,

    /// Argument index where address was found (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address_arg_index: Option<usize>,

    /// Raw argument text (for debugging).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_argument: Option<String>,

    /// Whether dual projection to state-boundary is required.
    pub requires_dual_projection: bool,

    /// Additional notes from the binding table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_notes: Option<String>,
}

impl BoundaryInteractionEvidence {
    /// Create a new evidence struct with current schema version.
    pub fn new(binding_key: String, api_family: String, function_name: String) -> Self {
        Self {
            version: BOUNDARY_INTERACTION_EVIDENCE_VERSION,
            binding_key,
            api_family,
            function_name,
            basis: InteractionBasis::ApiCall,
            channel_kind: ChannelKind::UnixSocket,
            direction: Direction::Bidirectional,
            boundary_scope: BoundaryScope::Unknown,
            endpoint_locality: EndpointLocality::Unknown,
            interaction_pattern: InteractionPattern::Stream,
            extracted_address: None,
            address_arg_index: None,
            raw_argument: None,
            requires_dual_projection: false,
            binding_notes: None,
        }
    }

    /// Set the detection basis.
    pub fn with_basis(mut self, basis: InteractionBasis) -> Self {
        self.basis = basis;
        self
    }

    /// Set the channel kind.
    pub fn with_channel_kind(mut self, kind: ChannelKind) -> Self {
        self.channel_kind = kind;
        self.requires_dual_projection = kind.requires_dual_projection();
        self
    }

    /// Set the direction.
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Set the boundary scope.
    pub fn with_boundary_scope(mut self, scope: BoundaryScope) -> Self {
        self.boundary_scope = scope;
        self
    }

    /// Set the endpoint locality.
    pub fn with_endpoint_locality(mut self, locality: EndpointLocality) -> Self {
        self.endpoint_locality = locality;
        self
    }

    /// Set the interaction pattern.
    pub fn with_interaction_pattern(mut self, pattern: InteractionPattern) -> Self {
        self.interaction_pattern = pattern;
        self
    }

    /// Set the extracted address.
    pub fn with_extracted_address(mut self, address: String, arg_index: Option<usize>) -> Self {
        self.extracted_address = Some(address);
        self.address_arg_index = arg_index;
        self
    }

    /// Set the raw argument text.
    pub fn with_raw_argument(mut self, raw: String) -> Self {
        self.raw_argument = Some(raw);
        self
    }

    /// Set binding notes.
    pub fn with_binding_notes(mut self, notes: String) -> Self {
        self.binding_notes = Some(notes);
        self
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("BoundaryInteractionEvidence serializes to JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_serializes_to_json() {
        let evidence = BoundaryInteractionEvidence::new(
            "c:posix_socket:bind:provider".to_string(),
            "posix_socket".to_string(),
            "bind".to_string(),
        )
        .with_channel_kind(ChannelKind::UnixSocket)
        .with_direction(Direction::Provider)
        .with_boundary_scope(BoundaryScope::InterProcess)
        .with_endpoint_locality(EndpointLocality::SameHostNamed)
        .with_extracted_address("/var/run/daemon.sock".to_string(), Some(1));

        let json = evidence.to_json();
        assert!(json.contains("\"bindingKey\":\"c:posix_socket:bind:provider\""));
        assert!(json.contains("\"extractedAddress\":\"/var/run/daemon.sock\""));
        assert!(json.contains("\"version\":1"));
    }

    #[test]
    fn shared_memory_sets_dual_projection_flag() {
        let evidence = BoundaryInteractionEvidence::new(
            "c:posix_shm:shm_open:bidirectional".to_string(),
            "posix_shm".to_string(),
            "shm_open".to_string(),
        )
        .with_channel_kind(ChannelKind::SharedMemory);

        assert!(evidence.requires_dual_projection);
    }

    #[test]
    fn unix_socket_does_not_require_dual_projection() {
        let evidence = BoundaryInteractionEvidence::new(
            "c:posix_socket:bind:provider".to_string(),
            "posix_socket".to_string(),
            "bind".to_string(),
        )
        .with_channel_kind(ChannelKind::UnixSocket);

        assert!(!evidence.requires_dual_projection);
    }
}
