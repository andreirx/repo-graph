//! Truth class enumeration.
//!
//! Every artifact family belongs to exactly one truth class. The class
//! determines the certainty level and semantic properties of the data.

use serde::{Deserialize, Serialize};

/// Truth classification for an artifact family.
///
/// This determines what kind of data the artifact represents and what
/// certainty guarantees it provides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruthKind {
    /// Layer 0-1: Directly extracted from source files.
    ///
    /// Properties:
    /// - Derived from source file content through deterministic extraction
    /// - One source file owns the fact (or a small, enumerable set)
    /// - If the source file is unchanged, the extracted fact is unchanged
    /// - No dependency on other artifact families
    ///
    /// Examples: nodes, edges, measurements, boundary_interaction_surfaces
    ExtractedFact,

    /// Layer 2: Computed deterministically from extracted facts.
    ///
    /// Properties:
    /// - Derived by joining or correlating Layer 0-1 facts
    /// - Deterministic given current snapshot's Layer 0-1 state
    /// - Cannot be copied forward without FK remapping or recomputation
    /// - Must be recomputed or remapped when upstream facts change
    ///
    /// Examples: boundary_contracts, boundary_interaction_links
    DeterministicRelationship,

    /// Layer 2-3: Summaries and presentation-oriented aggregations.
    ///
    /// Properties:
    /// - Computed from lower layers for query convenience
    /// - May be persisted or computed on read
    /// - Regenerate from current lower layers; do not copy forward
    /// - Absence is not source absence; it is query/model absence
    ///
    /// Examples: trust summaries, module summaries, boundary summaries
    Projection,

    /// Layer 3: Evidence-backed but partial, heuristic, degradable.
    ///
    /// Properties:
    /// - Based on patterns, heuristics, or incomplete evidence
    /// - Carry explicit confidence levels
    /// - May be wrong; must be distinguishable from extracted facts
    /// - Degrade explicitly when prerequisites are missing
    /// - Never present as Layer 0 truth
    ///
    /// Examples: gRPC impl hints, dead-code candidates, framework detection
    Inference,

    /// Layer 4: Human-authored policy and declarations.
    ///
    /// Properties:
    /// - Not derived from source code
    /// - Snapshot-independent or overlay-scoped
    /// - Never erased by source refresh
    /// - May reference Layer 0 anchors but are not computed from them
    ///
    /// Examples: requirement_declarations, waivers, policy_facts
    GovernanceOverlay,
}

impl TruthKind {
    /// Returns the layer number for this truth kind.
    ///
    /// Useful for ordering and dependency checking.
    pub fn layer(&self) -> u8 {
        match self {
            Self::ExtractedFact => 0,
            Self::DeterministicRelationship => 2,
            Self::Projection => 2,
            Self::Inference => 3,
            Self::GovernanceOverlay => 4,
        }
    }

    /// Returns true if this truth kind can be copy-forwarded during refresh.
    ///
    /// Only extracted facts from unchanged files can be copy-forwarded.
    /// Deterministic relationships must be recomputed.
    pub fn allows_copy_forward(&self) -> bool {
        matches!(self, Self::ExtractedFact)
    }

    /// Returns true if this truth kind requires provenance tracking.
    pub fn requires_provenance(&self) -> bool {
        matches!(
            self,
            Self::DeterministicRelationship | Self::Projection | Self::Inference
        )
    }
}

impl std::fmt::Display for TruthKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExtractedFact => write!(f, "extracted_fact"),
            Self::DeterministicRelationship => write!(f, "deterministic_relationship"),
            Self::Projection => write!(f, "projection"),
            Self::Inference => write!(f, "inference"),
            Self::GovernanceOverlay => write!(f, "governance_overlay"),
        }
    }
}
