//! Identity policy enumeration.
//!
//! Defines how row identity works for each artifact family.

use serde::{Deserialize, Serialize};

/// How row identity works for an artifact family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityPolicy {
    /// Row has a stable logical identity separate from row UID.
    ///
    /// Example: `repo:file:symbol` for nodes
    ///
    /// The row UID is snapshot-scoped (different each snapshot).
    /// The logical key is stable across snapshots and enables
    /// cross-snapshot comparison and provenance tracking.
    ///
    /// Applies to: Nodes, Edges, Measurements, Surfaces, Schemas, Elements
    StableLogicalKey,

    /// Identity is snapshot-row-id only; no stable key.
    ///
    /// Used for computed relationships where identity is derived
    /// from current inputs, not from a stable semantic identity.
    ///
    /// Applies to: BoundaryContracts, BoundaryInteractionLinks
    SnapshotRowId,

    /// Identity derived from referenced rows in current snapshot.
    ///
    /// FKs point to current snapshot's rows, not parent snapshot.
    /// The identity is transitively defined by the parent artifacts.
    ///
    /// Applies to: Child artifacts with FK relationships (channel_details, elements)
    DerivedFromCurrentSnapshot,

    /// Identity independent of snapshots.
    ///
    /// Row persists across all snapshots. Not snapshot-scoped.
    ///
    /// Applies to: Repos, RequirementDeclarations, Waivers
    SnapshotIndependent,
}

impl IdentityPolicy {
    /// Returns true if this policy uses stable logical keys.
    pub fn has_stable_key(&self) -> bool {
        matches!(self, Self::StableLogicalKey)
    }

    /// Returns true if this policy is snapshot-scoped.
    pub fn is_snapshot_scoped(&self) -> bool {
        !matches!(self, Self::SnapshotIndependent)
    }

    /// Returns true if identity is derived from parent artifacts.
    pub fn is_derived(&self) -> bool {
        matches!(self, Self::DerivedFromCurrentSnapshot | Self::SnapshotRowId)
    }
}

impl std::fmt::Display for IdentityPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StableLogicalKey => write!(f, "stable_logical_key"),
            Self::SnapshotRowId => write!(f, "snapshot_row_id"),
            Self::DerivedFromCurrentSnapshot => write!(f, "derived_from_current_snapshot"),
            Self::SnapshotIndependent => write!(f, "snapshot_independent"),
        }
    }
}
