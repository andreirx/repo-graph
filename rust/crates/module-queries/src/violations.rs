//! Violation evaluation from preloaded module graph facts.
//!
//! This module provides violation evaluation that operates on preloaded
//! `ModuleGraphFacts` instead of re-querying storage and re-deriving edges.

use std::collections::HashMap;

use crate::facts::{ModuleGraphFacts, ModuleQueryError};
use repo_graph_classification::boundary_evaluator::{
    evaluate_module_boundaries, ModuleBoundaryEvaluation,
};
use repo_graph_classification::boundary_parser::{
    parse_discovered_module_boundaries, RawBoundaryDeclaration,
};
use repo_graph_classification::module_edges::ModuleEdgeDiagnostics;
use repo_graph_storage::StorageConnection;

/// Result of discovered-module violations evaluation.
///
/// Bundles the boundary evaluation with edge derivation diagnostics so callers
/// can report graph degradation (e.g., missing module ownership) alongside
/// violation results.
#[derive(Debug, Clone)]
pub struct DiscoveredModuleViolationsResult {
    /// Boundary evaluation result (violations + stale declarations).
    pub evaluation: ModuleBoundaryEvaluation,

    /// Edge derivation diagnostics from the module graph facts.
    pub diagnostics: ModuleEdgeDiagnostics,
}

/// Evaluate discovered-module violations from preloaded facts.
///
/// This function uses preloaded `ModuleGraphFacts` instead of re-querying
/// storage or re-deriving edges. The only storage query is for boundary
/// declarations, which are small and not part of the graph facts.
///
/// # Arguments
///
/// * `storage` - Storage connection for loading declarations
/// * `repo_uid` - Repository UID for declaration lookup
/// * `facts` - Preloaded module graph facts
///
/// # Errors
///
/// Returns `ModuleQueryError::Storage` if declaration loading fails.
/// Returns `ModuleQueryError::EdgeDerivation` if boundary parsing fails.
pub fn evaluate_violations_from_facts(
    storage: &StorageConnection,
    repo_uid: &str,
    facts: &ModuleGraphFacts,
) -> Result<DiscoveredModuleViolationsResult, ModuleQueryError> {
    // 1. Load boundary declarations (the only storage query needed)
    let declarations = storage
        .get_active_boundary_declarations_for_repo(repo_uid)?;

    // 2. Parse discovered-module boundaries
    let raw_boundaries: Vec<RawBoundaryDeclaration> = declarations
        .iter()
        .map(|d| RawBoundaryDeclaration {
            declaration_uid: d.declaration_uid.clone(),
            value_json: d.value_json.clone(),
        })
        .collect();

    let parsed_boundaries = parse_discovered_module_boundaries(&raw_boundaries)
        .map_err(|e| ModuleQueryError::EdgeDerivation(e.to_string()))?;

    // 3. Build module index for stale detection
    let module_index: HashMap<String, String> = facts
        .context
        .modules
        .iter()
        .map(|m| {
            (
                m.canonical_root_path.clone(),
                m.module_candidate_uid.clone(),
            )
        })
        .collect();

    // 4. Evaluate boundaries against preloaded edges
    let evaluation = evaluate_module_boundaries(
        &parsed_boundaries,
        &facts.edges,
        &module_index,
    );

    Ok(DiscoveredModuleViolationsResult {
        evaluation,
        diagnostics: facts.diagnostics.clone(),
    })
}

/// Evaluate discovered-module violations from preloaded facts and declarations.
///
/// This is the fully preloaded variant — even declarations are passed in.
/// Use this when the caller has already loaded declarations for other purposes.
///
/// # Arguments
///
/// * `declarations` - Pre-loaded boundary declarations
/// * `facts` - Preloaded module graph facts
///
/// # Errors
///
/// Returns `ModuleQueryError::EdgeDerivation` if boundary parsing fails.
///
/// # Note
///
/// Currently unused but exported as intentional API surface for callers that
/// batch-load declarations (e.g., combined policy evaluation scenarios).
#[allow(dead_code)]
pub fn evaluate_violations_from_preloaded(
    declarations: &[repo_graph_storage::crud::declarations::DeclarationRow],
    facts: &ModuleGraphFacts,
) -> Result<DiscoveredModuleViolationsResult, ModuleQueryError> {
    // 1. Parse discovered-module boundaries
    let raw_boundaries: Vec<RawBoundaryDeclaration> = declarations
        .iter()
        .map(|d| RawBoundaryDeclaration {
            declaration_uid: d.declaration_uid.clone(),
            value_json: d.value_json.clone(),
        })
        .collect();

    let parsed_boundaries = parse_discovered_module_boundaries(&raw_boundaries)
        .map_err(|e| ModuleQueryError::EdgeDerivation(e.to_string()))?;

    // 2. Build module index for stale detection
    let module_index: HashMap<String, String> = facts
        .context
        .modules
        .iter()
        .map(|m| {
            (
                m.canonical_root_path.clone(),
                m.module_candidate_uid.clone(),
            )
        })
        .collect();

    // 3. Evaluate boundaries against preloaded edges
    let evaluation = evaluate_module_boundaries(
        &parsed_boundaries,
        &facts.edges,
        &module_index,
    );

    Ok(DiscoveredModuleViolationsResult {
        evaluation,
        diagnostics: facts.diagnostics.clone(),
    })
}
