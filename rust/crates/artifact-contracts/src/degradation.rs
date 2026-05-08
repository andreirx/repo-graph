//! Degradation policy enumeration.
//!
//! Defines how missing/incomplete data should be reported to consumers.

use serde::{Deserialize, Serialize};

/// How missing/incomplete data should be reported to consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradationPolicy {
    /// Absence is an error if source files exist.
    ///
    /// Query surfaces should fail or warn loudly if this data
    /// is missing when source files are present.
    ///
    /// Applies to: Core extracted facts (Nodes, Edges, FileVersions)
    MustBePresent,

    /// May be absent. Query surfaces report explicit "unknown" state.
    ///
    /// Consumers must not interpret absence as "known zero."
    /// The distinction between "no data" and "zero results" must
    /// be explicit in query output.
    ///
    /// Applies to: Boundaries, Contracts, Measurements, Inferences
    MayBeOmittedWithExplicitUnknown,

    /// Absence triggers warning in diagnostics.
    ///
    /// System is functional but degraded. Users should be notified
    /// but queries still return results.
    ///
    /// Applies to: Optional enrichment families
    MustTriggerWarning,

    /// Absence or staleness triggers recommendation to rebuild.
    ///
    /// The data is important enough that missing/stale state
    /// should prompt user action.
    ///
    /// Applies to: Stale or corrupted artifacts
    MustTriggerRebuildRecommendation,

    /// Known unsupported on current indexer embodiment.
    ///
    /// Not a bug; a capability gap. The Rust indexer may not
    /// populate certain tables that the TS prototype did.
    ///
    /// Query surfaces should report "unsupported" rather than
    /// "no data" or "zero results."
    ///
    /// Applies to: ModuleCandidates on Rust-only path, ProjectSurfaces on Rust-only path
    UnsupportedOnEmbodiment,
}

impl DegradationPolicy {
    /// Returns true if absence is an error condition.
    pub fn absence_is_error(&self) -> bool {
        matches!(self, Self::MustBePresent)
    }

    /// Returns true if this policy requires explicit unknown handling.
    pub fn requires_explicit_unknown(&self) -> bool {
        matches!(
            self,
            Self::MayBeOmittedWithExplicitUnknown | Self::UnsupportedOnEmbodiment
        )
    }

    /// Returns true if this family is known to be unsupported.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::UnsupportedOnEmbodiment)
    }

    /// Returns a human-readable reason for degradation.
    pub fn degradation_reason(&self) -> &'static str {
        match self {
            Self::MustBePresent => "required data is missing",
            Self::MayBeOmittedWithExplicitUnknown => "data not available",
            Self::MustTriggerWarning => "optional data not available",
            Self::MustTriggerRebuildRecommendation => "data is stale or corrupted",
            Self::UnsupportedOnEmbodiment => "not supported on this indexer embodiment",
        }
    }
}

impl std::fmt::Display for DegradationPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MustBePresent => write!(f, "must_be_present"),
            Self::MayBeOmittedWithExplicitUnknown => write!(f, "may_be_omitted_with_explicit_unknown"),
            Self::MustTriggerWarning => write!(f, "must_trigger_warning"),
            Self::MustTriggerRebuildRecommendation => write!(f, "must_trigger_rebuild_recommendation"),
            Self::UnsupportedOnEmbodiment => write!(f, "unsupported_on_embodiment"),
        }
    }
}
