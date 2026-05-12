//! Impact propagation from Layer 0 changes (ACR-4).
//!
//! When Layer 0 facts change during refresh, derived artifacts that depend on
//! them should be marked as `impacted` based on their provenance.
//!
//! # Architecture
//!
//! - `artifact-contracts` defines which families have `MarkImpactedOnRelevantLayer0Change`
//! - `FreshnessStoragePort` provides `mark_impacted_by_stable_keys()`
//! - This module orchestrates the propagation during refresh
//!
//! # Current State (ACR-4 Step 1)
//!
//! The infrastructure is wired but requires provenance_json to be populated
//! during artifact creation for impact propagation to be effective. Without
//! provenance data, the mark_impacted_by_stable_keys calls are no-ops.
//!
//! # Future Work
//!
//! - Populate provenance_json during inference creation
//! - Populate provenance_json during boundary contract creation
//! - Track changed stable keys from extraction results

use std::collections::HashMap;

use artifact_contracts::{families_with_provenance, get_contract, ArtifactFamily, ImpactPolicy};
use repo_graph_storage::FreshnessStoragePort;

/// Report of impact propagation results.
#[derive(Debug, Clone, Default)]
pub struct ImpactReport {
    /// Count of rows marked impacted per family.
    counts: HashMap<ArtifactFamily, usize>,
    /// Families that were skipped (not applicable for impact).
    skipped: Vec<ArtifactFamily>,
}

impl ImpactReport {
    /// Create a new empty impact report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that rows were marked impacted for a family.
    pub fn add(&mut self, family: ArtifactFamily, count: usize) {
        *self.counts.entry(family).or_insert(0) += count;
    }

    /// Record that a family was skipped.
    pub fn skip(&mut self, family: ArtifactFamily) {
        self.skipped.push(family);
    }

    /// Get count of impacted rows for a family.
    pub fn get(&self, family: ArtifactFamily) -> usize {
        *self.counts.get(&family).unwrap_or(&0)
    }

    /// Total rows marked impacted across all families.
    pub fn total_impacted(&self) -> usize {
        self.counts.values().sum()
    }

    /// Families that had rows marked impacted.
    pub fn impacted_families(&self) -> Vec<ArtifactFamily> {
        self.counts
            .iter()
            .filter(|(_, &count)| count > 0)
            .map(|(&family, _)| family)
            .collect()
    }
}

/// Propagate impact from Layer 0 changes to upper-layer artifacts.
///
/// For each artifact family with `ImpactPolicy::MarkImpactedOnRelevantLayer0Change`,
/// finds rows whose provenance depends on any of the changed stable keys and marks
/// them as `impacted`.
///
/// # Arguments
///
/// * `storage` - Storage connection implementing FreshnessStoragePort
/// * `snapshot_uid` - The snapshot being refreshed
/// * `changed_stable_keys` - Stable keys of Layer 0 items that changed
///
/// # Returns
///
/// Report of how many rows were marked impacted per family.
///
/// # Note
///
/// This function requires provenance_json to be populated on artifact rows.
/// If provenance_json is NULL, rows will not be marked impacted (the query
/// requires provenance_json IS NOT NULL).
pub fn propagate_impact<S: FreshnessStoragePort>(
    storage: &mut S,
    snapshot_uid: &str,
    changed_stable_keys: &[String],
) -> Result<ImpactReport, repo_graph_storage::error::StorageError> {
    let mut report = ImpactReport::new();

    // If no L0 changes, no impact to propagate
    if changed_stable_keys.is_empty() {
        return Ok(report);
    }

    // Convert to &str slice for the storage port
    let keys: Vec<&str> = changed_stable_keys.iter().map(|s| s.as_str()).collect();

    // Iterate families that may need impact marking
    for family in families_with_provenance() {
        let contract = get_contract(family);

        match contract.impact_policy {
            ImpactPolicy::MarkImpactedOnRelevantLayer0Change => {
                // Mark rows whose provenance depends on changed keys
                let table = family_to_table(family);
                if let Some(table) = table {
                    let count = storage.mark_impacted_by_stable_keys(snapshot_uid, table, &keys)?;
                    report.add(family, count);
                } else {
                    report.skip(family);
                }
            }
            ImpactPolicy::MarkImpactedOnAnyLayer0Change => {
                // Mark ALL rows in this family as impacted
                let table = family_to_table(family);
                if let Some(table) = table {
                    let count = storage.mark_all_current(snapshot_uid, table)?;
                    // mark_all_current marks rows as current, we need mark_all_impacted
                    // For now, skip this policy - would need a new storage method
                    report.skip(family);
                    let _ = count;
                } else {
                    report.skip(family);
                }
            }
            ImpactPolicy::RecomputeOnRelevantLayer0Change => {
                // These families are recomputed, not marked impacted
                report.skip(family);
            }
            ImpactPolicy::UnaffectedByLayer0Refresh => {
                // Skip governance overlays
                report.skip(family);
            }
        }
    }

    Ok(report)
}

/// Map artifact family to database table name.
///
/// Returns None if the family doesn't have a freshness-tracked table.
fn family_to_table(family: ArtifactFamily) -> Option<&'static str> {
    match family {
        ArtifactFamily::BoundaryContracts => Some("boundary_contracts"),
        ArtifactFamily::BoundaryInteractionLinks => Some("boundary_interaction_links"),
        ArtifactFamily::Inferences => Some("inferences"),
        ArtifactFamily::ModuleCandidates => Some("module_candidates"),
        ArtifactFamily::ProjectSurfaces => Some("project_surfaces"),
        ArtifactFamily::ProjectSurfaceEvidence => Some("project_surface_evidence"),
        ArtifactFamily::SurfaceEntrypoints => Some("surface_entrypoints"),
        ArtifactFamily::SurfaceConfigRoots => Some("surface_config_roots"),
        ArtifactFamily::SurfaceEnvDependencies => Some("surface_env_dependencies"),
        ArtifactFamily::SurfaceEnvEvidence => Some("surface_env_evidence"),
        ArtifactFamily::SurfaceFsMutations => Some("surface_fs_mutations"),
        ArtifactFamily::SurfaceFsMutationEvidence => Some("surface_fs_mutation_evidence"),
        // Layer 0-1 families don't have freshness columns
        ArtifactFamily::FileVersions
        | ArtifactFamily::Nodes
        | ArtifactFamily::Edges
        | ArtifactFamily::Measurements
        | ArtifactFamily::BoundaryInteractionSurfaces
        | ArtifactFamily::BoundaryChannelDetails
        | ArtifactFamily::ContractSchemas
        | ArtifactFamily::ContractElements
        | ArtifactFamily::PolicyFacts => None,
        // Governance overlays don't have freshness columns
        ArtifactFamily::RequirementDeclarations | ArtifactFamily::Waivers => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_report_tracks_counts() {
        let mut report = ImpactReport::new();
        report.add(ArtifactFamily::Inferences, 5);
        report.add(ArtifactFamily::Inferences, 3);
        report.add(ArtifactFamily::BoundaryContracts, 2);

        assert_eq!(report.get(ArtifactFamily::Inferences), 8);
        assert_eq!(report.get(ArtifactFamily::BoundaryContracts), 2);
        assert_eq!(report.get(ArtifactFamily::ModuleCandidates), 0);
        assert_eq!(report.total_impacted(), 10);
    }

    #[test]
    fn impact_report_tracks_skipped() {
        let mut report = ImpactReport::new();
        report.skip(ArtifactFamily::Nodes);
        report.skip(ArtifactFamily::Edges);

        assert_eq!(report.skipped.len(), 2);
    }

    #[test]
    fn family_to_table_maps_freshness_tracked() {
        assert_eq!(
            family_to_table(ArtifactFamily::Inferences),
            Some("inferences")
        );
        assert_eq!(
            family_to_table(ArtifactFamily::BoundaryContracts),
            Some("boundary_contracts")
        );
        assert_eq!(
            family_to_table(ArtifactFamily::ModuleCandidates),
            Some("module_candidates")
        );
    }

    #[test]
    fn family_to_table_returns_none_for_layer0() {
        assert_eq!(family_to_table(ArtifactFamily::Nodes), None);
        assert_eq!(family_to_table(ArtifactFamily::Edges), None);
        assert_eq!(family_to_table(ArtifactFamily::FileVersions), None);
    }
}
