//! Coherence tests for the artifact contract registry.
//!
//! These tests ensure that policy combinations are valid and
//! consistent across the registry.

use artifact_contracts::{
    all_families, families_by_truth_kind, families_with_freshness_tracking,
    families_with_provenance, get_contract, unsupported_families, ArtifactFamily,
    DegradationPolicy, IdentityPolicy, ImpactPolicy, ProvenancePolicy, RefreshPolicy, TruthKind,
};

#[test]
fn extracted_facts_have_direct_provenance() {
    for family in families_by_truth_kind(TruthKind::ExtractedFact) {
        let contract = get_contract(family);
        assert!(
            matches!(
                contract.provenance_policy,
                ProvenancePolicy::DirectFromSourceFile
            ),
            "{:?}: ExtractedFact should have DirectFromSourceFile provenance, got {:?}",
            family,
            contract.provenance_policy
        );
    }
}

#[test]
fn deterministic_relationships_have_derived_provenance() {
    for family in families_by_truth_kind(TruthKind::DeterministicRelationship) {
        let contract = get_contract(family);
        assert!(
            matches!(
                contract.provenance_policy,
                ProvenancePolicy::DerivedFromLayer0Items
            ),
            "{:?}: DeterministicRelationship should have DerivedFromLayer0Items provenance, got {:?}",
            family,
            contract.provenance_policy
        );
    }
}

#[test]
fn derived_provenance_has_dependencies() {
    for family in all_families() {
        let contract = get_contract(*family);
        if matches!(
            contract.provenance_policy,
            ProvenancePolicy::DerivedFromLayer0Items
                | ProvenancePolicy::DerivedFromArtifactFamilies
        ) {
            assert!(
                !contract.layer_dependencies.is_empty(),
                "{:?}: DerivedFrom* provenance requires non-empty layer_dependencies",
                family
            );
        }
    }
}

#[test]
fn snapshot_independent_refresh_has_snapshot_independent_identity() {
    for family in all_families() {
        let contract = get_contract(*family);
        if matches!(contract.refresh_policy, RefreshPolicy::SnapshotIndependent) {
            assert!(
                matches!(contract.identity_policy, IdentityPolicy::SnapshotIndependent),
                "{:?}: SnapshotIndependent refresh should have SnapshotIndependent identity, got {:?}",
                family,
                contract.identity_policy
            );
        }
    }
}

#[test]
fn governance_overlays_have_human_provenance() {
    for family in families_by_truth_kind(TruthKind::GovernanceOverlay) {
        let contract = get_contract(family);
        assert!(
            matches!(contract.provenance_policy, ProvenancePolicy::HumanAuthored),
            "{:?}: GovernanceOverlay should have HumanAuthored provenance, got {:?}",
            family,
            contract.provenance_policy
        );
    }
}

#[test]
fn governance_overlays_are_snapshot_independent() {
    for family in families_by_truth_kind(TruthKind::GovernanceOverlay) {
        let contract = get_contract(family);
        assert!(
            matches!(contract.refresh_policy, RefreshPolicy::SnapshotIndependent),
            "{:?}: GovernanceOverlay should have SnapshotIndependent refresh, got {:?}",
            family,
            contract.refresh_policy
        );
        assert!(
            matches!(
                contract.impact_policy,
                ImpactPolicy::UnaffectedByLayer0Refresh
            ),
            "{:?}: GovernanceOverlay should be UnaffectedByLayer0Refresh, got {:?}",
            family,
            contract.impact_policy
        );
    }
}

#[test]
fn freshness_tracking_families_have_appropriate_provenance() {
    for family in families_with_freshness_tracking() {
        let contract = get_contract(family);
        // Families with per-row freshness should either:
        // 1. Have derived provenance (for impact tracking), or
        // 2. Be deterministic relationships (recomputed anyway)
        let has_derived_provenance = matches!(
            contract.provenance_policy,
            ProvenancePolicy::DerivedFromLayer0Items
                | ProvenancePolicy::DerivedFromArtifactFamilies
        );
        let is_deterministic = matches!(contract.truth_kind, TruthKind::DeterministicRelationship);
        assert!(
            has_derived_provenance || is_deterministic,
            "{:?}: Per-row freshness tracking requires derived provenance or deterministic relationship",
            family
        );
    }
}

#[test]
fn provenance_families_include_all_required() {
    let prov_families = families_with_provenance();

    // All deterministic relationships need provenance
    for family in families_by_truth_kind(TruthKind::DeterministicRelationship) {
        assert!(
            prov_families.contains(&family),
            "{:?}: DeterministicRelationship should be in families_with_provenance",
            family
        );
    }

    // Inferences need provenance
    assert!(
        prov_families.contains(&ArtifactFamily::Inferences),
        "Inferences should be in families_with_provenance"
    );
}

#[test]
fn unsupported_families_have_correct_degradation() {
    let unsupported = unsupported_families();

    for family in &unsupported {
        let contract = get_contract(*family);
        assert!(
            matches!(
                contract.degradation_policy,
                DegradationPolicy::UnsupportedOnEmbodiment
            ),
            "{:?}: unsupported_families should have UnsupportedOnEmbodiment degradation",
            family
        );
    }

    // Known unsupported families on Rust path
    assert!(unsupported.contains(&ArtifactFamily::ModuleCandidates));
    assert!(unsupported.contains(&ArtifactFamily::ProjectSurfaces));
}

#[test]
fn layer_ordering_is_consistent() {
    // Layer 0-1 families should have layer <= 1
    for family in families_by_truth_kind(TruthKind::ExtractedFact) {
        let contract = get_contract(family);
        assert!(
            contract.layer() <= 1,
            "{:?}: ExtractedFact should have layer <= 1, got {}",
            family,
            contract.layer()
        );
    }

    // Layer 2 families
    for family in families_by_truth_kind(TruthKind::DeterministicRelationship) {
        let contract = get_contract(family);
        assert_eq!(
            contract.layer(),
            2,
            "{:?}: DeterministicRelationship should have layer 2",
            family
        );
    }

    // Layer 3 families
    for family in families_by_truth_kind(TruthKind::Inference) {
        let contract = get_contract(family);
        assert_eq!(
            contract.layer(),
            3,
            "{:?}: Inference should have layer 3",
            family
        );
    }

    // Layer 4 families
    for family in families_by_truth_kind(TruthKind::GovernanceOverlay) {
        let contract = get_contract(family);
        assert_eq!(
            contract.layer(),
            4,
            "{:?}: GovernanceOverlay should have layer 4",
            family
        );
    }
}

#[test]
fn recompute_policy_matches_truth_kind() {
    for family in all_families() {
        let contract = get_contract(*family);

        match contract.truth_kind {
            TruthKind::ExtractedFact => {
                // Extracted facts should reextract or copy-forward
                assert!(
                    matches!(
                        contract.refresh_policy,
                        RefreshPolicy::ReextractChangedInputs
                            | RefreshPolicy::CopyForwardWithFkRemap
                    ),
                    "{:?}: ExtractedFact should have Reextract or CopyForwardWithFkRemap refresh",
                    family
                );
            }
            TruthKind::DeterministicRelationship => {
                // Deterministic relationships should recompute
                assert!(
                    matches!(
                        contract.refresh_policy,
                        RefreshPolicy::RecomputeFromCurrentSnapshot
                    ),
                    "{:?}: DeterministicRelationship should have RecomputeFromCurrentSnapshot refresh",
                    family
                );
            }
            TruthKind::GovernanceOverlay => {
                // Governance is snapshot-independent
                assert!(
                    matches!(contract.refresh_policy, RefreshPolicy::SnapshotIndependent),
                    "{:?}: GovernanceOverlay should have SnapshotIndependent refresh",
                    family
                );
            }
            _ => {
                // Projections and Inferences have more flexibility
            }
        }
    }
}

#[test]
fn table_names_are_valid() {
    for family in all_families() {
        let table = family.table_name();
        assert!(
            !table.is_empty(),
            "{:?}: table_name should not be empty",
            family
        );
        assert!(
            !table.contains(' '),
            "{:?}: table_name should not contain spaces",
            family
        );
    }
}

#[test]
fn boundary_contracts_dependencies_are_correct() {
    let contract = get_contract(ArtifactFamily::BoundaryContracts);
    assert!(
        contract
            .layer_dependencies
            .contains(&ArtifactFamily::BoundaryInteractionSurfaces),
        "BoundaryContracts should depend on BoundaryInteractionSurfaces"
    );
    assert!(
        contract
            .layer_dependencies
            .contains(&ArtifactFamily::ContractElements),
        "BoundaryContracts should depend on ContractElements"
    );
}

#[test]
fn boundary_interaction_links_dependencies_are_correct() {
    let contract = get_contract(ArtifactFamily::BoundaryInteractionLinks);
    assert!(
        contract
            .layer_dependencies
            .contains(&ArtifactFamily::BoundaryInteractionSurfaces),
        "BoundaryInteractionLinks should depend on BoundaryInteractionSurfaces"
    );
    assert!(
        contract
            .layer_dependencies
            .contains(&ArtifactFamily::BoundaryContracts),
        "BoundaryInteractionLinks should depend on BoundaryContracts"
    );
}
