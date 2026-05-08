//! Impact policy enumeration.
//!
//! Defines what happens when upstream Layer 0 changes.

use serde::{Deserialize, Serialize};

/// What happens when upstream Layer 0 changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactPolicy {
    /// Recompute immediately when relevant Layer 0 items change.
    ///
    /// The artifact is cheap enough to recompute during refresh.
    /// After Layer 0-1 refresh completes, these artifacts are
    /// regenerated from the current snapshot's facts.
    ///
    /// Applies to: Cheap deterministic relationships
    RecomputeOnRelevantLayer0Change,

    /// Mark rows as impacted when their provenance anchors change.
    ///
    /// Do not eagerly recompute. The rows remain queryable but
    /// are marked with `freshness_state = 'impacted'`. Recomputation
    /// happens on demand or in background.
    ///
    /// This is the key policy for expensive computations that
    /// should not block refresh.
    ///
    /// Applies to: Expensive hints, inferences, relationships
    MarkImpactedOnRelevantLayer0Change,

    /// Mark rows as impacted on any Layer 0 change in the snapshot.
    ///
    /// Coarse-grained impact for families that depend on global
    /// snapshot state rather than specific Layer 0 items.
    ///
    /// Applies to: Global summaries, trust aggregates
    MarkImpactedOnAnyLayer0Change,

    /// Not affected by Layer 0 refresh.
    ///
    /// The artifact is independent of source code changes.
    /// Human-authored governance overlays fall into this category.
    ///
    /// Applies to: Governance overlays
    UnaffectedByLayer0Refresh,
}

impl ImpactPolicy {
    /// Returns true if this policy triggers recomputation.
    pub fn triggers_recomputation(&self) -> bool {
        matches!(self, Self::RecomputeOnRelevantLayer0Change)
    }

    /// Returns true if this policy may mark rows as impacted.
    pub fn may_mark_impacted(&self) -> bool {
        matches!(
            self,
            Self::MarkImpactedOnRelevantLayer0Change | Self::MarkImpactedOnAnyLayer0Change
        )
    }

    /// Returns true if impact is precise (per-provenance) vs coarse (any change).
    pub fn is_precise_impact(&self) -> bool {
        matches!(self, Self::MarkImpactedOnRelevantLayer0Change)
    }

    /// Returns true if this policy is unaffected by Layer 0.
    pub fn is_unaffected(&self) -> bool {
        matches!(self, Self::UnaffectedByLayer0Refresh)
    }
}

impl std::fmt::Display for ImpactPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RecomputeOnRelevantLayer0Change => write!(f, "recompute_on_relevant_layer0_change"),
            Self::MarkImpactedOnRelevantLayer0Change => write!(f, "mark_impacted_on_relevant_layer0_change"),
            Self::MarkImpactedOnAnyLayer0Change => write!(f, "mark_impacted_on_any_layer0_change"),
            Self::UnaffectedByLayer0Refresh => write!(f, "unaffected_by_layer0_refresh"),
        }
    }
}
