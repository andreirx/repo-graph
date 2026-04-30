//! Module graph facts loading and precomputation.
//!
//! This module provides the single-load orchestration for module graph data.
//! All module commands should use `load_module_graph_facts` to get precomputed
//! facts instead of loading and deriving edges themselves.

use crate::context::ModuleQueryContext;
use repo_graph_classification::module_edges::{
    derive_module_dependency_edges, FileOwnershipFact, ModuleDependencyEdge,
    ModuleEdgeDerivationInput, ModuleEdgeDiagnostics, ModuleRef, ResolvedImportFact,
};
use repo_graph_storage::crud::module_edges_support::{FileOwnership, OwnedFileForRollup};
use repo_graph_storage::types::ModuleCandidate;
use repo_graph_storage::StorageConnection;

/// Error type for module query operations.
#[derive(Debug, thiserror::Error)]
pub enum ModuleQueryError {
    /// Storage error during data loading.
    #[error("storage error: {0}")]
    Storage(#[from] repo_graph_storage::error::StorageError),

    /// Edge derivation error.
    #[error("edge derivation failed: {0}")]
    EdgeDerivation(String),
}

/// Preloaded module graph facts.
///
/// This struct bundles all module graph data that is typically needed
/// by module commands. Loading happens once via `load_module_graph_facts`,
/// then the facts can be passed to multiple operations without re-querying
/// storage or re-deriving edges.
#[derive(Debug, Clone)]
pub struct ModuleGraphFacts {
    /// Module context with unified read model.
    pub context: ModuleQueryContext,

    /// Derived module dependency edges.
    pub edges: Vec<ModuleDependencyEdge>,

    /// Edge derivation diagnostics (import counts, ownership gaps).
    pub diagnostics: ModuleEdgeDiagnostics,

    /// Module references used for edge derivation.
    /// Cached for downstream operations that need them.
    pub module_refs: Vec<ModuleRef>,
}

impl ModuleGraphFacts {
    /// Get all modules from the context.
    pub fn modules(&self) -> &[ModuleCandidate] {
        &self.context.modules
    }

    /// Get file ownership from the context.
    pub fn ownership(&self) -> &[FileOwnership] {
        &self.context.ownership
    }

    /// Get owned files from the context.
    pub fn owned_files(&self) -> &[OwnedFileForRollup] {
        &self.context.owned_files
    }

    /// Check if context came from fallback (Rust MODULE nodes).
    pub fn is_fallback(&self) -> bool {
        self.context.is_fallback
    }

    /// Resolve a module argument using the context.
    pub fn resolve_module(&self, arg: &str) -> Option<&ModuleCandidate> {
        self.context.resolve_module(arg)
    }

    /// Get files for a specific module using the context.
    pub fn files_for_module(&self, module_uid: &str) -> Vec<&OwnedFileForRollup> {
        self.context.files_for_module(module_uid)
    }
}

/// Load module graph facts for a snapshot.
///
/// This is the single-load orchestration point for all module graph data.
/// It performs:
/// 1. Module context loading (with TS/Rust fallback)
/// 2. Resolved import loading
/// 3. Module edge derivation
///
/// The returned facts contain all precomputed data needed by module commands.
///
/// # Errors
///
/// Returns `ModuleQueryError::Storage` if storage queries fail.
/// Returns `ModuleQueryError::EdgeDerivation` if edge derivation fails.
pub fn load_module_graph_facts(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<ModuleGraphFacts, ModuleQueryError> {
    // 1. Load module context (with fallback)
    let context = ModuleQueryContext::load(storage, snapshot_uid)?;

    // 2. Load resolved imports
    let imports = storage.get_resolved_imports_for_snapshot(snapshot_uid)?;

    // 3. Build classification DTOs
    let module_refs: Vec<ModuleRef> = context
        .modules
        .iter()
        .map(|m| ModuleRef {
            module_uid: m.module_candidate_uid.clone(),
            canonical_path: m.canonical_root_path.clone(),
        })
        .collect();

    let import_facts: Vec<ResolvedImportFact> = imports
        .into_iter()
        .map(|i| ResolvedImportFact {
            source_file_uid: i.source_file_uid,
            target_file_uid: i.target_file_uid,
        })
        .collect();

    let ownership_facts: Vec<FileOwnershipFact> = context
        .ownership
        .iter()
        .map(|o| FileOwnershipFact {
            file_uid: o.file_uid.clone(),
            module_uid: o.module_candidate_uid.clone(),
        })
        .collect();

    // 4. Derive module edges
    let derivation_input = ModuleEdgeDerivationInput {
        imports: import_facts,
        ownership: ownership_facts,
        modules: module_refs.clone(),
    };

    let derivation_result = derive_module_dependency_edges(derivation_input)
        .map_err(|e| ModuleQueryError::EdgeDerivation(e.to_string()))?;

    Ok(ModuleGraphFacts {
        context,
        edges: derivation_result.edges,
        diagnostics: derivation_result.diagnostics,
        module_refs,
    })
}
