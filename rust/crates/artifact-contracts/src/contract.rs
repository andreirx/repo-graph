//! Artifact contract definition.
//!
//! The complete contract for an artifact family, combining all
//! policies and classifications.

use crate::{
    ArtifactFamily, ClassificationMaturity, DegradationPolicy, FreshnessTracking, IdentityPolicy,
    ImpactPolicy, ProvenancePolicy, RefreshPolicy, TruthKind,
};

/// Complete contract for one artifact family.
///
/// This struct defines all the policies and classifications that
/// govern how an artifact family behaves during refresh, query,
/// and degradation scenarios.
#[derive(Debug, Clone)]
pub struct ArtifactContract {
    /// The artifact family this contract applies to.
    pub family: ArtifactFamily,

    /// Truth classification (Layer 0-1 fact, relationship, inference, etc.).
    pub truth_kind: TruthKind,

    /// How this family behaves during incremental refresh.
    pub refresh_policy: RefreshPolicy,

    /// How row identity works for this family.
    pub identity_policy: IdentityPolicy,

    /// How missing/incomplete data should be reported.
    pub degradation_policy: DegradationPolicy,

    /// How provenance is tracked for this family.
    pub provenance_policy: ProvenancePolicy,

    /// What happens when upstream Layer 0 changes.
    pub impact_policy: ImpactPolicy,

    /// How freshness is tracked for this family.
    pub freshness_tracking: FreshnessTracking,

    /// Maturity level of this classification.
    pub classification_maturity: ClassificationMaturity,

    /// Artifact families that this family depends on.
    ///
    /// For deterministic relationships, this lists the Layer 0-1
    /// families that must be current before this family can be
    /// computed.
    pub layer_dependencies: &'static [ArtifactFamily],

    /// Human-readable description of this family.
    pub description: &'static str,
}

impl ArtifactContract {
    /// Create a new artifact contract.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        family: ArtifactFamily,
        truth_kind: TruthKind,
        refresh_policy: RefreshPolicy,
        identity_policy: IdentityPolicy,
        degradation_policy: DegradationPolicy,
        provenance_policy: ProvenancePolicy,
        impact_policy: ImpactPolicy,
        freshness_tracking: FreshnessTracking,
        classification_maturity: ClassificationMaturity,
        layer_dependencies: &'static [ArtifactFamily],
        description: &'static str,
    ) -> Self {
        Self {
            family,
            truth_kind,
            refresh_policy,
            identity_policy,
            degradation_policy,
            provenance_policy,
            impact_policy,
            freshness_tracking,
            classification_maturity,
            layer_dependencies,
            description,
        }
    }

    /// Returns true if this family is a Layer 0-1 extracted fact.
    pub fn is_extracted_fact(&self) -> bool {
        matches!(self.truth_kind, TruthKind::ExtractedFact)
    }

    /// Returns true if this family requires provenance tracking.
    pub fn requires_provenance(&self) -> bool {
        self.provenance_policy.requires_row_provenance()
    }

    /// Returns true if this family requires freshness tracking.
    pub fn requires_freshness_tracking(&self) -> bool {
        self.freshness_tracking.requires_column()
    }

    /// Returns true if this family can be copied forward during refresh.
    pub fn allows_copy_forward(&self) -> bool {
        self.refresh_policy.allows_copy_forward()
    }

    /// Returns true if this family is unsupported on current embodiment.
    pub fn is_unsupported(&self) -> bool {
        self.degradation_policy.is_unsupported()
    }

    /// Returns the layer number for ordering.
    pub fn layer(&self) -> u8 {
        self.truth_kind.layer()
    }
}
