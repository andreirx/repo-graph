//! Refresh policy enumeration.
//!
//! Defines how an artifact family behaves during incremental refresh.

use serde::{Deserialize, Serialize};

/// How an artifact family behaves during incremental refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshPolicy {
    /// Re-extract from changed source files; copy forward unchanged.
    ///
    /// Applies to: Extracted Facts (Layer 0-1)
    ///
    /// Behavior:
    /// - Changed/config-widened source files: re-extract
    /// - Unchanged source files: copy forward with new snapshot-scoped row IDs
    /// - Deleted source files: do not copy forward
    ReextractChangedInputs,

    /// Recompute entirely from current snapshot's lower-layer facts.
    ///
    /// Applies to: Deterministic Relationships, Projections
    ///
    /// Behavior:
    /// - Never copy forward rows from parent snapshot
    /// - Run after Layer 0-1 refresh is complete
    /// - Uses only current snapshot's artifact UIDs
    RecomputeFromCurrentSnapshot,

    /// Copy forward only if parent artifact was copied forward.
    ///
    /// Applies to: Child artifacts with FK to parent (e.g., channel_details → surface)
    ///
    /// Behavior:
    /// - Copy forward only if parent artifact was copied forward
    /// - Remap FK to new parent row ID
    /// - Requires old→new ID mapping from parent copy-forward
    CopyForwardWithFkRemap,

    /// Mark rows as impacted; defer recomputation.
    ///
    /// Applies to: Expensive hints/inferences
    ///
    /// Behavior:
    /// - Do not eagerly recompute
    /// - Mark rows as `impacted` when upstream Layer 0 changes
    /// - Recompute on demand or in background
    MarkImpactedDeferRecompute,

    /// Always regenerate; never copy forward.
    ///
    /// Applies to: Projections computed on read
    ///
    /// Behavior:
    /// - Always regenerate
    /// - No persistence or transient persistence only
    NeverCopyForward,

    /// Not affected by source refresh.
    ///
    /// Applies to: Governance Overlays
    ///
    /// Behavior:
    /// - Not affected by source refresh
    /// - Persist independently of snapshots
    /// - May reference snapshot-scoped artifacts by stable key
    SnapshotIndependent,
}

impl RefreshPolicy {
    /// Returns true if this policy allows copying rows from parent snapshot.
    pub fn allows_copy_forward(&self) -> bool {
        matches!(
            self,
            Self::ReextractChangedInputs | Self::CopyForwardWithFkRemap
        )
    }

    /// Returns true if this policy requires recomputation after Layer 0-1.
    pub fn requires_recomputation(&self) -> bool {
        matches!(
            self,
            Self::RecomputeFromCurrentSnapshot | Self::NeverCopyForward
        )
    }

    /// Returns true if this policy may mark rows as impacted.
    pub fn may_mark_impacted(&self) -> bool {
        matches!(self, Self::MarkImpactedDeferRecompute)
    }

    /// Returns true if this policy is independent of snapshots.
    pub fn is_snapshot_independent(&self) -> bool {
        matches!(self, Self::SnapshotIndependent)
    }
}

impl std::fmt::Display for RefreshPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReextractChangedInputs => write!(f, "reextract_changed_inputs"),
            Self::RecomputeFromCurrentSnapshot => write!(f, "recompute_from_current_snapshot"),
            Self::CopyForwardWithFkRemap => write!(f, "copy_forward_with_fk_remap"),
            Self::MarkImpactedDeferRecompute => write!(f, "mark_impacted_defer_recompute"),
            Self::NeverCopyForward => write!(f, "never_copy_forward"),
            Self::SnapshotIndependent => write!(f, "snapshot_independent"),
        }
    }
}
