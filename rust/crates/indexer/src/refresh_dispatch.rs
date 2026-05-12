//! Contract-driven refresh dispatch for deterministic relationships.
//!
//! This module provides contract-driven execution for artifact families that
//! require recomputation or re-indexing during refresh. The registry CHOOSES
//! the action, not just validates it.
//!
//! # Architecture
//!
//! - `artifact-contracts` defines the canonical policies
//! - This module dispatches execution based on those policies
//! - Families with `RecomputeFromCurrentSnapshot` are recomputed here
//! - Families with `ReextractChangedInputs` that drift to reindex are handled here
//!
//! # ACR-2 Contract-Driven Dispatch
//!
//! The distinction from validation:
//! - **Validation**: code chooses action, registry audits
//! - **Dispatch**: registry chooses action, code executes
//!
//! This module implements dispatch, not validation.

use artifact_contracts::{get_contract, ArtifactFamily, RefreshPolicy};

use crate::grpc_client_hint::{self, GrpcClientHintResult};
use crate::grpc_impl_hint::{self, GrpcImplHintResult};
use crate::grpc_link::{self, GrpcLinkResult};
use crate::grpc_registration_proof::{self, GrpcRegistrationProofResult};
use crate::storage_port::{
    GrpcClientHintReadPort, GrpcClientHintStorePort, GrpcImplHintReadPort, GrpcImplHintStorePort,
    GrpcLinkReadPort, GrpcLinkStorePort, GrpcRegistrationProofPort, IndexerStoragePort,
};
use crate::types::GeneratedCodeMappingResult;

// ═══════════════════════════════════════════════════════════════════════════
// Recompute Dispatch Result
// ═══════════════════════════════════════════════════════════════════════════

/// Result of dispatching recompute for deterministic relationship families.
#[derive(Debug, Default)]
pub struct RecomputeDispatchResult {
    /// GR-1A: gRPC implementation hints (BoundaryContracts - provider surfaces)
    pub grpc_impl_hints: Option<GrpcImplHintResult>,
    /// GR-1B: gRPC registration proof (BoundaryContracts confidence boost)
    pub grpc_registration_proof: Option<GrpcRegistrationProofResult>,
    /// GR-2A: gRPC client hints (BoundaryContracts - consumer surfaces)
    pub grpc_client_hints: Option<GrpcClientHintResult>,
    /// GR-3A: gRPC links (BoundaryInteractionLinks)
    pub grpc_links: Option<GrpcLinkResult>,
    /// Whether dispatch was skipped due to policy mismatch
    pub skipped: bool,
    /// Reason for skip if skipped
    pub skip_reason: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Contract-Driven Dispatch
// ═══════════════════════════════════════════════════════════════════════════

/// Dispatch recomputation for BoundaryContracts and BoundaryInteractionLinks.
///
/// This function is the contract-driven entry point for recomputing deterministic
/// relationships. It:
/// 1. Checks the contract for each family
/// 2. Only executes if the policy is `RecomputeFromCurrentSnapshot`
/// 3. Returns results or skip reason
///
/// The registry CHOOSES whether to run, not just validates after the fact.
pub fn dispatch_recompute_relationships<S>(
    storage: &mut S,
    snapshot_uid: &str,
    repo_uid: &str,
    generated_code_mappings: Option<&GeneratedCodeMappingResult>,
) -> RecomputeDispatchResult
where
    S: IndexerStoragePort
        + GrpcImplHintReadPort
        + GrpcImplHintStorePort
        + GrpcRegistrationProofPort
        + GrpcClientHintReadPort
        + GrpcClientHintStorePort
        + GrpcLinkReadPort
        + GrpcLinkStorePort,
{
    let mut result = RecomputeDispatchResult::default();

    // ── Contract-driven policy check ──
    // The registry decides whether we should recompute these families.
    let boundary_contracts_contract = get_contract(ArtifactFamily::BoundaryContracts);
    let boundary_links_contract = get_contract(ArtifactFamily::BoundaryInteractionLinks);

    // Verify both families have RecomputeFromCurrentSnapshot policy
    let bc_should_recompute = matches!(
        boundary_contracts_contract.refresh_policy,
        RefreshPolicy::RecomputeFromCurrentSnapshot
    );
    let bil_should_recompute = matches!(
        boundary_links_contract.refresh_policy,
        RefreshPolicy::RecomputeFromCurrentSnapshot
    );

    if !bc_should_recompute {
        result.skipped = true;
        result.skip_reason = Some(format!(
            "BoundaryContracts has {:?} policy, not RecomputeFromCurrentSnapshot",
            boundary_contracts_contract.refresh_policy
        ));
        return result;
    }

    if !bil_should_recompute {
        result.skipped = true;
        result.skip_reason = Some(format!(
            "BoundaryInteractionLinks has {:?} policy, not RecomputeFromCurrentSnapshot",
            boundary_links_contract.refresh_policy
        ));
        return result;
    }

    // ── Execute recomputation (registry authorized) ──
    // Only reach here if contracts say RecomputeFromCurrentSnapshot

    // Check prerequisites - need generated code mappings to proceed
    match generated_code_mappings {
        Some(m) if m.mappings_persisted > 0 && !m.has_error() => {
            // Proceed with recomputation
        }
        _ => {
            // No mappings to work with - skip but not an error
            return result;
        }
    }

    // GR-1A: Recompute BoundaryContracts (provider surfaces)
    let hint_result = grpc_impl_hint::run_grpc_impl_hint_detection(storage, snapshot_uid, repo_uid);
    result.grpc_impl_hints = Some(hint_result);

    // GR-1B: Boost confidence for surfaces with registration proof
    if let Some(ref hints) = result.grpc_impl_hints {
        if hints.hints_emitted > 0 {
            let proof_result =
                grpc_registration_proof::run_grpc_registration_proof(storage, snapshot_uid);
            result.grpc_registration_proof = Some(proof_result);
        }
    }

    // GR-2A: Recompute BoundaryContracts (consumer surfaces)
    let client_hint_result =
        grpc_client_hint::run_grpc_client_hint_detection(storage, snapshot_uid, repo_uid);
    result.grpc_client_hints = Some(client_hint_result);

    // GR-3A: Recompute BoundaryInteractionLinks
    let has_providers = result
        .grpc_impl_hints
        .as_ref()
        .map(|h| h.hints_emitted > 0)
        .unwrap_or(false);
    let has_consumers = result
        .grpc_client_hints
        .as_ref()
        .map(|h| h.hints_emitted > 0)
        .unwrap_or(false);

    if has_providers && has_consumers {
        let link_result = grpc_link::run_grpc_link_detection(storage, snapshot_uid);
        result.grpc_links = Some(link_result);
    }

    result
}

/// Check if a family should be recomputed based on its contract.
///
/// Returns true if the contract says `RecomputeFromCurrentSnapshot`.
pub fn should_recompute(family: ArtifactFamily) -> bool {
    matches!(
        get_contract(family).refresh_policy,
        RefreshPolicy::RecomputeFromCurrentSnapshot
    )
}

/// Check if a family should be re-extracted based on its contract.
///
/// Returns true if the contract says `ReextractChangedInputs`.
pub fn should_reextract(family: ArtifactFamily) -> bool {
    matches!(
        get_contract(family).refresh_policy,
        RefreshPolicy::ReextractChangedInputs
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_contracts_should_recompute() {
        assert!(
            should_recompute(ArtifactFamily::BoundaryContracts),
            "BoundaryContracts should have RecomputeFromCurrentSnapshot policy"
        );
    }

    #[test]
    fn boundary_interaction_links_should_recompute() {
        assert!(
            should_recompute(ArtifactFamily::BoundaryInteractionLinks),
            "BoundaryInteractionLinks should have RecomputeFromCurrentSnapshot policy"
        );
    }

    #[test]
    fn contract_schemas_should_reextract() {
        assert!(
            should_reextract(ArtifactFamily::ContractSchemas),
            "ContractSchemas should have ReextractChangedInputs policy"
        );
    }

    #[test]
    fn contract_elements_should_reextract() {
        assert!(
            should_reextract(ArtifactFamily::ContractElements),
            "ContractElements should have ReextractChangedInputs policy"
        );
    }

    #[test]
    fn measurements_should_not_recompute() {
        assert!(
            !should_recompute(ArtifactFamily::Measurements),
            "Measurements should not have RecomputeFromCurrentSnapshot policy"
        );
    }
}
