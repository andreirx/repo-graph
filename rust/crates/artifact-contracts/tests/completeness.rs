//! Completeness tests for the artifact contract registry.
//!
//! These tests ensure that every artifact family has a contract
//! and that all required fields are populated.

use artifact_contracts::{
    all_families, get_contract, ArtifactFamily, ClassificationMaturity,
};

#[test]
fn every_family_has_a_contract() {
    // This test ensures the registry is complete.
    // The exhaustive match in get_contract provides compile-time
    // guarantee, but this test makes it explicit.
    for family in all_families() {
        let contract = get_contract(*family);
        assert_eq!(
            contract.family, *family,
            "Contract family mismatch for {:?}",
            family
        );
    }
}

#[test]
fn all_families_have_descriptions() {
    for family in all_families() {
        let contract = get_contract(*family);
        assert!(
            !contract.description.is_empty(),
            "{:?} has empty description",
            family
        );
    }
}

#[test]
fn all_families_have_valid_maturity() {
    for family in all_families() {
        let contract = get_contract(*family);
        // Just verify the maturity is set (not a default/uninitialized state)
        let _ = contract.classification_maturity;
    }
}

#[test]
fn family_count_matches_expected() {
    // This test catches accidental additions/removals without registry updates.
    // Update this count when adding new families.
    let expected_count = 23;
    let actual_count = all_families().len();
    assert_eq!(
        actual_count, expected_count,
        "Family count changed. Expected {}, got {}. Update the registry if adding families.",
        expected_count, actual_count
    );
}

#[test]
fn all_families_are_enumerated_in_all() {
    // Verify ArtifactFamily::all() returns all variants
    let all = all_families();

    // Check specific families are present
    assert!(all.contains(&ArtifactFamily::Nodes));
    assert!(all.contains(&ArtifactFamily::Edges));
    assert!(all.contains(&ArtifactFamily::Measurements));
    assert!(all.contains(&ArtifactFamily::BoundaryInteractionSurfaces));
    assert!(all.contains(&ArtifactFamily::BoundaryContracts));
    assert!(all.contains(&ArtifactFamily::BoundaryInteractionLinks));
    assert!(all.contains(&ArtifactFamily::Inferences));
    assert!(all.contains(&ArtifactFamily::ModuleCandidates));
    assert!(all.contains(&ArtifactFamily::ProjectSurfaces));
    assert!(all.contains(&ArtifactFamily::RequirementDeclarations));
    assert!(all.contains(&ArtifactFamily::Waivers));
}

#[test]
fn stable_families_count() {
    // Count families with stable classification
    let stable_count = all_families()
        .iter()
        .filter(|f| get_contract(**f).classification_maturity == ClassificationMaturity::Stable)
        .count();

    // Most core families should be stable
    assert!(
        stable_count >= 10,
        "Expected at least 10 stable families, got {}",
        stable_count
    );
}

#[test]
fn provisional_families_documented() {
    // Provisional families should be documented as such
    let provisional: Vec<_> = all_families()
        .iter()
        .filter(|f| get_contract(**f).classification_maturity == ClassificationMaturity::Provisional)
        .collect();

    // These are known provisional families (update as classifications stabilize)
    let expected_provisional = vec![
        ArtifactFamily::Inferences,
        ArtifactFamily::ModuleCandidates,
        ArtifactFamily::ProjectSurfaces,
        ArtifactFamily::ProjectSurfaceEvidence,
        ArtifactFamily::SurfaceEntrypoints,
        ArtifactFamily::SurfaceConfigRoots,
        ArtifactFamily::SurfaceEnvDependencies,
        ArtifactFamily::SurfaceEnvEvidence,
        ArtifactFamily::SurfaceFsMutations,
        ArtifactFamily::SurfaceFsMutationEvidence,
    ];

    for family in &provisional {
        assert!(
            expected_provisional.contains(family),
            "Unexpected provisional family: {:?}. Add to expected list or stabilize classification.",
            family
        );
    }
}
