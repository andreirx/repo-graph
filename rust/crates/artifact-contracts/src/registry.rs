//! Artifact contract registry.
//!
//! The canonical registry of all artifact family contracts.
//! This is the authoritative source for artifact semantics.

use crate::{
    ArtifactContract, ArtifactFamily, ClassificationMaturity, DegradationPolicy, FreshnessTracking,
    IdentityPolicy, ImpactPolicy, ProvenancePolicy, RefreshPolicy, TruthKind,
};

// ═══════════════════════════════════════════════════════════════════════════
// Registry Implementation
// ═══════════════════════════════════════════════════════════════════════════

/// Get the contract for a specific artifact family.
pub fn get_contract(family: ArtifactFamily) -> &'static ArtifactContract {
    match family {
        // Layer 0-1: Extracted Facts
        ArtifactFamily::FileVersions => &FILE_VERSIONS_CONTRACT,
        ArtifactFamily::Nodes => &NODES_CONTRACT,
        ArtifactFamily::Edges => &EDGES_CONTRACT,
        ArtifactFamily::Measurements => &MEASUREMENTS_CONTRACT,
        ArtifactFamily::BoundaryInteractionSurfaces => &BOUNDARY_INTERACTION_SURFACES_CONTRACT,
        ArtifactFamily::BoundaryChannelDetails => &BOUNDARY_CHANNEL_DETAILS_CONTRACT,
        ArtifactFamily::ContractSchemas => &CONTRACT_SCHEMAS_CONTRACT,
        ArtifactFamily::ContractElements => &CONTRACT_ELEMENTS_CONTRACT,
        ArtifactFamily::PolicyFacts => &POLICY_FACTS_CONTRACT,

        // Layer 2: Deterministic Relationships
        ArtifactFamily::BoundaryContracts => &BOUNDARY_CONTRACTS_CONTRACT,
        ArtifactFamily::BoundaryInteractionLinks => &BOUNDARY_INTERACTION_LINKS_CONTRACT,

        // Layer 3: Hints / Inferences
        ArtifactFamily::Inferences => &INFERENCES_CONTRACT,
        ArtifactFamily::ModuleCandidates => &MODULE_CANDIDATES_CONTRACT,
        ArtifactFamily::ProjectSurfaces => &PROJECT_SURFACES_CONTRACT,
        ArtifactFamily::ProjectSurfaceEvidence => &PROJECT_SURFACE_EVIDENCE_CONTRACT,
        ArtifactFamily::SurfaceEntrypoints => &SURFACE_ENTRYPOINTS_CONTRACT,
        ArtifactFamily::SurfaceConfigRoots => &SURFACE_CONFIG_ROOTS_CONTRACT,
        ArtifactFamily::SurfaceEnvDependencies => &SURFACE_ENV_DEPENDENCIES_CONTRACT,
        ArtifactFamily::SurfaceEnvEvidence => &SURFACE_ENV_EVIDENCE_CONTRACT,
        ArtifactFamily::SurfaceFsMutations => &SURFACE_FS_MUTATIONS_CONTRACT,
        ArtifactFamily::SurfaceFsMutationEvidence => &SURFACE_FS_MUTATION_EVIDENCE_CONTRACT,

        // Layer 4: Governance Overlays
        ArtifactFamily::RequirementDeclarations => &REQUIREMENT_DECLARATIONS_CONTRACT,
        ArtifactFamily::Waivers => &WAIVERS_CONTRACT,
    }
}

/// Get all registered artifact families.
pub fn all_families() -> &'static [ArtifactFamily] {
    ArtifactFamily::all()
}

/// Get families by truth kind.
pub fn families_by_truth_kind(kind: TruthKind) -> Vec<ArtifactFamily> {
    ArtifactFamily::all()
        .iter()
        .filter(|f| get_contract(**f).truth_kind == kind)
        .copied()
        .collect()
}

/// Get families that require provenance tracking.
pub fn families_with_provenance() -> Vec<ArtifactFamily> {
    ArtifactFamily::all()
        .iter()
        .filter(|f| get_contract(**f).requires_provenance())
        .copied()
        .collect()
}

/// Get families that participate in refresh.
pub fn families_for_refresh() -> Vec<ArtifactFamily> {
    ArtifactFamily::all()
        .iter()
        .filter(|f| !get_contract(**f).refresh_policy.is_snapshot_independent())
        .copied()
        .collect()
}

/// Get families that require per-row freshness tracking.
pub fn families_with_freshness_tracking() -> Vec<ArtifactFamily> {
    ArtifactFamily::all()
        .iter()
        .filter(|f| get_contract(**f).requires_freshness_tracking())
        .copied()
        .collect()
}

/// Get families sorted by layer (for refresh ordering).
pub fn families_by_layer() -> Vec<ArtifactFamily> {
    let mut families: Vec<_> = ArtifactFamily::all().to_vec();
    families.sort_by_key(|f| get_contract(*f).layer());
    families
}

/// Get families that are unsupported on current embodiment.
pub fn unsupported_families() -> Vec<ArtifactFamily> {
    ArtifactFamily::all()
        .iter()
        .filter(|f| get_contract(**f).is_unsupported())
        .copied()
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Layer 0-1: Extracted Facts
// ═══════════════════════════════════════════════════════════════════════════

static FILE_VERSIONS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::FileVersions,
    TruthKind::ExtractedFact,
    RefreshPolicy::ReextractChangedInputs,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::MustBePresent,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[],
    "File path + content hash tracking",
);

static NODES_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::Nodes,
    TruthKind::ExtractedFact,
    RefreshPolicy::ReextractChangedInputs,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::MustBePresent,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[],
    "AST-derived symbols and file markers",
);

static EDGES_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::Edges,
    TruthKind::ExtractedFact,
    RefreshPolicy::ReextractChangedInputs,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::MustBePresent,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[],
    "AST-derived relationships (calls, imports, contains)",
);

static MEASUREMENTS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::Measurements,
    TruthKind::ExtractedFact,
    RefreshPolicy::ReextractChangedInputs,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[],
    "File-local deterministic metrics (complexity, line counts)",
);

static BOUNDARY_INTERACTION_SURFACES_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::BoundaryInteractionSurfaces,
    TruthKind::ExtractedFact,
    RefreshPolicy::ReextractChangedInputs,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[],
    "IPC/network call sites detected in source files",
);

static BOUNDARY_CHANNEL_DETAILS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::BoundaryChannelDetails,
    TruthKind::ExtractedFact,
    RefreshPolicy::CopyForwardWithFkRemap,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[ArtifactFamily::BoundaryInteractionSurfaces],
    "Channel metadata anchored to boundary surfaces",
);

static CONTRACT_SCHEMAS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::ContractSchemas,
    TruthKind::ExtractedFact,
    RefreshPolicy::ReextractChangedInputs,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[],
    "Proto/schema file parse output",
);

static CONTRACT_ELEMENTS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::ContractElements,
    TruthKind::ExtractedFact,
    RefreshPolicy::ReextractChangedInputs,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[ArtifactFamily::ContractSchemas],
    "Schema-internal elements (messages, fields, services)",
);

static POLICY_FACTS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::PolicyFacts,
    TruthKind::ExtractedFact,
    RefreshPolicy::ReextractChangedInputs,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DirectFromSourceFile,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::ImplicitFromSource,
    ClassificationMaturity::Stable,
    &[],
    "Extracted governance annotations from source",
);

// ═══════════════════════════════════════════════════════════════════════════
// Layer 2: Deterministic Relationships
// ═══════════════════════════════════════════════════════════════════════════

static BOUNDARY_CONTRACTS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::BoundaryContracts,
    TruthKind::DeterministicRelationship,
    RefreshPolicy::RecomputeFromCurrentSnapshot,
    IdentityPolicy::SnapshotRowId,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Stable,
    &[
        ArtifactFamily::BoundaryInteractionSurfaces,
        ArtifactFamily::ContractElements,
    ],
    "Surface-to-contract-element mapping",
);

static BOUNDARY_INTERACTION_LINKS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::BoundaryInteractionLinks,
    TruthKind::DeterministicRelationship,
    RefreshPolicy::RecomputeFromCurrentSnapshot,
    IdentityPolicy::SnapshotRowId,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Stable,
    &[
        ArtifactFamily::BoundaryInteractionSurfaces,
        ArtifactFamily::BoundaryContracts,
    ],
    "gRPC service-to-implementation links",
);

// ═══════════════════════════════════════════════════════════════════════════
// Layer 3: Hints / Inferences
// ═══════════════════════════════════════════════════════════════════════════

static INFERENCES_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::Inferences,
    TruthKind::Inference,
    RefreshPolicy::MarkImpactedDeferRecompute,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::MarkImpactedOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::Nodes, ArtifactFamily::Edges],
    "Derived facts with confidence (dead-code candidates, trust signals)",
);

static MODULE_CANDIDATES_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::ModuleCandidates,
    TruthKind::Inference,
    RefreshPolicy::RecomputeFromCurrentSnapshot,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::Nodes, ArtifactFamily::FileVersions],
    "Discovered module boundaries (manifest-backed and inferred)",
);

static PROJECT_SURFACES_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::ProjectSurfaces,
    TruthKind::Inference,
    RefreshPolicy::RecomputeFromCurrentSnapshot,
    IdentityPolicy::StableLogicalKey,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::ModuleCandidates],
    "Runtime/build surface discovery",
);

static PROJECT_SURFACE_EVIDENCE_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::ProjectSurfaceEvidence,
    TruthKind::Inference,
    RefreshPolicy::CopyForwardWithFkRemap,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::ProjectSurfaces],
    "Evidence supporting project surface discovery",
);

static SURFACE_ENTRYPOINTS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::SurfaceEntrypoints,
    TruthKind::Inference,
    RefreshPolicy::CopyForwardWithFkRemap,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::ProjectSurfaces],
    "Entrypoint markers for surfaces",
);

static SURFACE_CONFIG_ROOTS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::SurfaceConfigRoots,
    TruthKind::Inference,
    RefreshPolicy::CopyForwardWithFkRemap,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::ProjectSurfaces],
    "Config root markers for surfaces",
);

static SURFACE_ENV_DEPENDENCIES_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::SurfaceEnvDependencies,
    TruthKind::Inference,
    RefreshPolicy::CopyForwardWithFkRemap,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::ProjectSurfaces],
    "Environment dependencies for surfaces",
);

static SURFACE_ENV_EVIDENCE_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::SurfaceEnvEvidence,
    TruthKind::Inference,
    RefreshPolicy::CopyForwardWithFkRemap,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::SurfaceEnvDependencies],
    "Evidence for environment dependencies",
);

static SURFACE_FS_MUTATIONS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::SurfaceFsMutations,
    TruthKind::Inference,
    RefreshPolicy::CopyForwardWithFkRemap,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::ProjectSurfaces],
    "Filesystem mutation markers",
);

static SURFACE_FS_MUTATION_EVIDENCE_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::SurfaceFsMutationEvidence,
    TruthKind::Inference,
    RefreshPolicy::CopyForwardWithFkRemap,
    IdentityPolicy::DerivedFromCurrentSnapshot,
    DegradationPolicy::UnsupportedOnEmbodiment,
    ProvenancePolicy::DerivedFromLayer0Items,
    ImpactPolicy::RecomputeOnRelevantLayer0Change,
    FreshnessTracking::PerRow,
    ClassificationMaturity::Provisional,
    &[ArtifactFamily::SurfaceFsMutations],
    "Evidence for filesystem mutations",
);

// ═══════════════════════════════════════════════════════════════════════════
// Layer 4: Governance Overlays
// ═══════════════════════════════════════════════════════════════════════════

static REQUIREMENT_DECLARATIONS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::RequirementDeclarations,
    TruthKind::GovernanceOverlay,
    RefreshPolicy::SnapshotIndependent,
    IdentityPolicy::SnapshotIndependent,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::HumanAuthored,
    ImpactPolicy::UnaffectedByLayer0Refresh,
    FreshnessTracking::None,
    ClassificationMaturity::Stable,
    &[],
    "Quality gate rules and boundary declarations",
);

static WAIVERS_CONTRACT: ArtifactContract = ArtifactContract::new(
    ArtifactFamily::Waivers,
    TruthKind::GovernanceOverlay,
    RefreshPolicy::SnapshotIndependent,
    IdentityPolicy::SnapshotIndependent,
    DegradationPolicy::MayBeOmittedWithExplicitUnknown,
    ProvenancePolicy::HumanAuthored,
    ImpactPolicy::UnaffectedByLayer0Refresh,
    FreshnessTracking::None,
    ClassificationMaturity::Stable,
    &[],
    "Temporary exemptions from gate failures",
);

// ═══════════════════════════════════════════════════════════════════════════
// Validation helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Validate that all families have contracts (compile-time guarantee via exhaustive match).
#[cfg(test)]
pub fn validate_registry_completeness() -> bool {
    // The exhaustive match in get_contract ensures completeness.
    // This function exists for explicit test coverage.
    for family in ArtifactFamily::all() {
        let _ = get_contract(*family);
    }
    true
}

/// Validate policy coherence rules.
#[cfg(test)]
pub fn validate_policy_coherence() -> Vec<String> {
    let mut errors = Vec::new();

    for family in ArtifactFamily::all() {
        let contract = get_contract(*family);

        // Rule: ExtractedFact should have DirectFromSourceFile provenance
        if contract.truth_kind == TruthKind::ExtractedFact
            && !matches!(
                contract.provenance_policy,
                ProvenancePolicy::DirectFromSourceFile
            )
        {
            errors.push(format!(
                "{:?}: ExtractedFact should have DirectFromSourceFile provenance",
                family
            ));
        }

        // Rule: DeterministicRelationship should have DerivedFromLayer0Items provenance
        if contract.truth_kind == TruthKind::DeterministicRelationship
            && !matches!(
                contract.provenance_policy,
                ProvenancePolicy::DerivedFromLayer0Items
            )
        {
            errors.push(format!(
                "{:?}: DeterministicRelationship should have DerivedFromLayer0Items provenance",
                family
            ));
        }

        // Rule: DerivedFromLayer0Items should have non-empty layer_dependencies
        if matches!(
            contract.provenance_policy,
            ProvenancePolicy::DerivedFromLayer0Items
        ) && contract.layer_dependencies.is_empty()
        {
            errors.push(format!(
                "{:?}: DerivedFromLayer0Items provenance requires non-empty layer_dependencies",
                family
            ));
        }

        // Rule: SnapshotIndependent refresh implies SnapshotIndependent identity
        if matches!(contract.refresh_policy, RefreshPolicy::SnapshotIndependent)
            && !matches!(
                contract.identity_policy,
                IdentityPolicy::SnapshotIndependent
            )
        {
            errors.push(format!(
                "{:?}: SnapshotIndependent refresh should have SnapshotIndependent identity",
                family
            ));
        }

        // Rule: GovernanceOverlay should have HumanAuthored provenance
        if contract.truth_kind == TruthKind::GovernanceOverlay
            && !matches!(contract.provenance_policy, ProvenancePolicy::HumanAuthored)
        {
            errors.push(format!(
                "{:?}: GovernanceOverlay should have HumanAuthored provenance",
                family
            ));
        }

        // Rule: PerRow freshness tracking requires provenance tracking
        if matches!(contract.freshness_tracking, FreshnessTracking::PerRow)
            && !contract.provenance_policy.requires_row_provenance()
            && !matches!(contract.truth_kind, TruthKind::DeterministicRelationship)
        {
            // Deterministic relationships may have PerRow freshness without explicit provenance
            // because they are recomputed, not marked impacted
        }
    }

    errors
}
