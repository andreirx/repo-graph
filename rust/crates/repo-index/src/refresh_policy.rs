//! Refresh policy integration for contract-driven dispatch.
//!
//! This module defines the **refresh-executable subset** for the Rust
//! embodiment and provides contract-driven dispatch for refresh operations.
//!
//! # Architecture
//!
//! - `artifact-contracts` defines the canonical ontology (all families, all policies)
//! - This module defines which families are **executable in this embodiment**
//! - The executable subset is implementation scope, not product semantics
//!
//! # Refresh-Executable Subset
//!
//! Not all artifact families participate in Rust refresh today. Some are:
//! - Unsupported on Rust embodiment
//! - Not yet refresh-participating
//! - Read-side only
//!
//! This module explicitly enumerates what is executable to prevent fake completeness.
//!
//! # Family Behavior Mapping
//!
//! Each executable family has two branches:
//! - **Unchanged files**: copy-forward from parent snapshot
//! - **Changed files**: re-extract or recompute
//!
//! The contract's `RefreshPolicy` determines which branch applies.

use artifact_contracts::{get_contract, ArtifactFamily, RefreshPolicy};

// ═══════════════════════════════════════════════════════════════════════════
// Refresh-Executable Subset
// ═══════════════════════════════════════════════════════════════════════════

/// Families that participate in copy-forward during refresh.
///
/// These families have the `ReextractChangedInputs` or similar policy where:
/// - Unchanged files: copy forward from parent snapshot
/// - Changed files: re-extracted by orchestrator
///
/// This is the **Rust embodiment's current implementation scope**.
/// Not all families in `artifact-contracts` are listed here.
pub const COPY_FORWARD_FAMILIES: &[ArtifactFamily] = &[
    ArtifactFamily::Measurements,
    // TODO(ACR-3/4): Inferences contract is MarkImpactedDeferRecompute, not
    // ReextractChangedInputs. Temporary copy-forward used until ACR-3/4
    // provides per-row freshness/provenance scaffolding for honest impact
    // marking. See: docs/TECH-DEBT.md "ACR-2 Architecture-Carried Deferrals"
    ArtifactFamily::Inferences,
    ArtifactFamily::BoundaryInteractionSurfaces,
    // Note: ContractSchemas has copy-forward code but is currently re-indexed
    // on refresh. When copy-forward becomes the active path, add it here.
];

/// Families that are recomputed from current snapshot on every refresh.
///
/// These families have `RecomputeFromCurrentSnapshot` policy:
/// - Always recomputed using current snapshot's Layer 0-1 data
/// - No copy-forward; parent snapshot data is ignored
///
/// Currently handled by the GR chain in orchestrator.
pub const RECOMPUTE_FAMILIES: &[ArtifactFamily] = &[
    ArtifactFamily::BoundaryContracts,
    ArtifactFamily::BoundaryInteractionLinks,
];

/// Families that are re-indexed (re-extracted) on every refresh.
///
/// These have `ReextractChangedInputs` policy but the current implementation
/// re-indexes all files rather than copy-forward for unchanged.
///
/// This represents implementation drift from the contract policy.
/// Future work: align to copy-forward for unchanged files.
pub const REINDEX_FAMILIES: &[ArtifactFamily] = &[
    ArtifactFamily::ContractSchemas,
    ArtifactFamily::ContractElements,
];

// ═══════════════════════════════════════════════════════════════════════════
// Refresh Dispatch Result
// ═══════════════════════════════════════════════════════════════════════════

/// Result of refreshing a single artifact family.
#[derive(Debug, Clone)]
pub struct FamilyRefreshResult {
    /// The artifact family that was refreshed.
    pub family: ArtifactFamily,
    /// The policy that was applied.
    pub policy: RefreshPolicy,
    /// The action taken.
    pub action: RefreshAction,
    /// Count of rows affected (if applicable).
    pub rows_affected: Option<usize>,
}

/// Action taken during refresh for a family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshAction {
    /// Rows copied forward from parent snapshot.
    CopiedForward,
    /// Family recomputed from current snapshot.
    Recomputed,
    /// Family re-indexed (all files re-extracted).
    Reindexed,
    /// Family skipped (snapshot-independent or not applicable).
    Skipped,
    /// Family not yet implemented in this embodiment.
    NotImplemented,
}

impl std::fmt::Display for RefreshAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CopiedForward => write!(f, "copied_forward"),
            Self::Recomputed => write!(f, "recomputed"),
            Self::Reindexed => write!(f, "reindexed"),
            Self::Skipped => write!(f, "skipped"),
            Self::NotImplemented => write!(f, "not_implemented"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Refresh Diagnostics
// ═══════════════════════════════════════════════════════════════════════════

/// Structured refresh diagnostics for a complete refresh operation.
#[derive(Debug, Clone, Default)]
pub struct RefreshDiagnostics {
    /// Results per family.
    pub family_results: Vec<FamilyRefreshResult>,
    /// Total rows copied forward across all families.
    pub total_copied_forward: usize,
    /// Total rows recomputed across all families.
    pub total_recomputed: usize,
    /// Families that were skipped.
    pub skipped_families: Vec<ArtifactFamily>,
}

impl RefreshDiagnostics {
    /// Create new empty diagnostics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a family refresh result.
    pub fn record(&mut self, result: FamilyRefreshResult) {
        match result.action {
            RefreshAction::CopiedForward => {
                if let Some(n) = result.rows_affected {
                    self.total_copied_forward += n;
                }
            }
            RefreshAction::Recomputed => {
                if let Some(n) = result.rows_affected {
                    self.total_recomputed += n;
                }
            }
            RefreshAction::Skipped | RefreshAction::NotImplemented => {
                self.skipped_families.push(result.family);
            }
            _ => {}
        }
        self.family_results.push(result);
    }

    /// Generate a summary line for logging.
    pub fn summary(&self) -> String {
        format!(
            "refresh: {} families processed, {} rows copied forward, {} rows recomputed, {} skipped",
            self.family_results.len(),
            self.total_copied_forward,
            self.total_recomputed,
            self.skipped_families.len()
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Policy Validation
// ═══════════════════════════════════════════════════════════════════════════

/// Validate that a family's declared contract matches its implementation category.
///
/// Returns an error message if there's a mismatch, None if valid.
pub fn validate_family_implementation(family: ArtifactFamily) -> Option<String> {
    let contract = get_contract(family);

    // Check copy-forward families
    if COPY_FORWARD_FAMILIES.contains(&family) {
        match contract.refresh_policy {
            RefreshPolicy::ReextractChangedInputs | RefreshPolicy::MarkImpactedDeferRecompute => {
                // Valid: these policies support copy-forward for unchanged
                None
            }
            _ => Some(format!(
                "{:?}: in COPY_FORWARD_FAMILIES but has {:?} policy",
                family, contract.refresh_policy
            )),
        }
    }
    // Check recompute families
    else if RECOMPUTE_FAMILIES.contains(&family) {
        match contract.refresh_policy {
            RefreshPolicy::RecomputeFromCurrentSnapshot => None,
            _ => Some(format!(
                "{:?}: in RECOMPUTE_FAMILIES but has {:?} policy",
                family, contract.refresh_policy
            )),
        }
    }
    // Check reindex families (implementation drift)
    else if REINDEX_FAMILIES.contains(&family) {
        // These are known drift cases - the contract says ReextractChangedInputs
        // but implementation currently re-indexes everything.
        // This is documented, not an error.
        None
    } else {
        // Family not in any executable subset - that's fine, just not implemented
        None
    }
}

/// Validate all executable families against their contracts.
///
/// Returns a list of mismatches. Empty list means all valid.
pub fn validate_all_implementations() -> Vec<String> {
    let mut errors = Vec::new();

    for family in COPY_FORWARD_FAMILIES {
        if let Some(err) = validate_family_implementation(*family) {
            errors.push(err);
        }
    }

    for family in RECOMPUTE_FAMILIES {
        if let Some(err) = validate_family_implementation(*family) {
            errors.push(err);
        }
    }

    for family in REINDEX_FAMILIES {
        if let Some(err) = validate_family_implementation(*family) {
            errors.push(err);
        }
    }

    errors
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_forward_families_have_valid_policies() {
        for family in COPY_FORWARD_FAMILIES {
            let contract = get_contract(*family);
            assert!(
                matches!(
                    contract.refresh_policy,
                    RefreshPolicy::ReextractChangedInputs
                        | RefreshPolicy::MarkImpactedDeferRecompute
                ),
                "{:?} should have copy-forward compatible policy, got {:?}",
                family,
                contract.refresh_policy
            );
        }
    }

    #[test]
    fn recompute_families_have_recompute_policy() {
        for family in RECOMPUTE_FAMILIES {
            let contract = get_contract(*family);
            assert!(
                matches!(
                    contract.refresh_policy,
                    RefreshPolicy::RecomputeFromCurrentSnapshot
                ),
                "{:?} should have RecomputeFromCurrentSnapshot policy, got {:?}",
                family,
                contract.refresh_policy
            );
        }
    }

    #[test]
    fn reindex_families_document_drift() {
        // These families have ReextractChangedInputs policy but are currently
        // re-indexed fully. This test documents the known drift.
        for family in REINDEX_FAMILIES {
            let contract = get_contract(*family);
            assert!(
                matches!(
                    contract.refresh_policy,
                    RefreshPolicy::ReextractChangedInputs
                ),
                "{:?} should have ReextractChangedInputs policy (drift documented), got {:?}",
                family,
                contract.refresh_policy
            );
        }
    }

    #[test]
    fn all_implementations_valid() {
        let errors = validate_all_implementations();
        assert!(
            errors.is_empty(),
            "Implementation validation errors: {:?}",
            errors
        );
    }

    #[test]
    fn diagnostics_summary_format() {
        let mut diag = RefreshDiagnostics::new();
        diag.record(FamilyRefreshResult {
            family: ArtifactFamily::Measurements,
            policy: RefreshPolicy::ReextractChangedInputs,
            action: RefreshAction::CopiedForward,
            rows_affected: Some(42),
        });
        diag.record(FamilyRefreshResult {
            family: ArtifactFamily::BoundaryContracts,
            policy: RefreshPolicy::RecomputeFromCurrentSnapshot,
            action: RefreshAction::Recomputed,
            rows_affected: Some(10),
        });

        let summary = diag.summary();
        assert!(summary.contains("2 families"));
        assert!(summary.contains("42 rows copied forward"));
        assert!(summary.contains("10 rows recomputed"));
    }
}
