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

    /// One or more files are claimed by more than one module (duplicate ownership).
    ///
    /// The module-graph invariant defect the pure guards in
    /// `classification::module_edges` / `module_rollup` protect against. Generation
    /// (`repo-index` compose) resolves the known npm-vs-inferred collision (npm
    /// wins), so this should not occur; if it ever recurs, the module command
    /// surface reports it as a labeled degradation naming the affected files —
    /// never a bare InternalError (MODULE-OWNERSHIP-DUPLICATE-1). It is NOT resolved
    /// here: the ownership DTOs carry no ecosystem, so there is no non-arbitrary
    /// winner to pick — honest degradation, not a coin flip.
    ///
    /// `affected_files` holds the offending file_uids, sorted and de-duplicated.
    #[error("duplicate module ownership on {} file(s)", .affected_files.len())]
    DuplicateOwnership { affected_files: Vec<String> },
}

/// Detect files claimed by more than one module (duplicate ownership).
///
/// Returns the affected file_uids, sorted and de-duplicated; empty when every
/// file has at most one owner. This is the enumerate-ALL companion to the pure
/// guard in `classification::module_edges` (which reports only the first
/// offender): the module command surface needs the complete affected set to
/// degrade honestly. Kept as a standalone pure function so the degradation path
/// is unit-testable without storage.
///
/// Crate-internal (`pub(crate)`): the only caller is `load_module_graph_facts`
/// below (plus this crate's tests). No external consumer — the reader-facing
/// degradation is produced downstream in `daemon-runtime::module_degradation`.
pub(crate) fn detect_duplicate_file_ownership(ownership: &[FileOwnershipFact]) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut owners: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for fact in ownership {
        owners
            .entry(fact.file_uid.as_str())
            .or_default()
            .insert(fact.module_uid.as_str());
    }
    owners
        .into_iter()
        .filter(|(_, modules)| modules.len() > 1)
        .map(|(file_uid, _)| file_uid.to_string())
        .collect()
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

    /// Check if context came from fallback.
    ///
    /// After Phase 4 (2026-05-10), this always returns `false`.
    /// The MODULE-node fallback has been removed; `module_candidates`
    /// is now the sole source of module topology.
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
/// 1. Module context loading from `module_candidates` (no fallback after Phase 4)
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
    // 1. Load module context (no fallback after Phase 4)
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

    // MODULE-OWNERSHIP-DUPLICATE-1: pre-detect duplicate ownership so the surface
    // can degrade honestly (naming EVERY affected file) instead of the pure guard
    // aborting with only the first offender as an opaque InternalError. Generation
    // resolves the npm-vs-inferred collision (npm wins); this is the safety net for
    // any residual collision shape. The pure guard in derive_module_dependency_edges
    // stays as the invariant witness for callers that bypass this path (e.g. gate).
    let duplicate_files = detect_duplicate_file_ownership(&ownership_facts);
    if !duplicate_files.is_empty() {
        return Err(ModuleQueryError::DuplicateOwnership {
            affected_files: duplicate_files,
        });
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn own(file: &str, module: &str) -> FileOwnershipFact {
        FileOwnershipFact {
            file_uid: file.to_string(),
            module_uid: module.to_string(),
        }
    }

    #[test]
    fn no_duplicate_ownership_returns_empty() {
        let ownership = vec![
            own("repo:a.ts", "npm-mod-1"),
            own("repo:b.ts", "npm-mod-1"),
            own("repo:c.py", "inferred-mod-1"),
        ];
        assert!(detect_duplicate_file_ownership(&ownership).is_empty());
    }

    #[test]
    fn same_file_same_module_is_not_a_duplicate() {
        // Idempotent rows (same file, same module) are not a conflict.
        let ownership = vec![own("repo:a.ts", "npm-mod-1"), own("repo:a.ts", "npm-mod-1")];
        assert!(detect_duplicate_file_ownership(&ownership).is_empty());
    }

    #[test]
    fn duplicate_ownership_enumerates_all_affected_files_sorted() {
        // The vscode shape: a .mts claimed by both an npm module and an inferred
        // module — plus a second collision — must both surface, sorted.
        let ownership = vec![
            own("repo:extensions/esbuild-common.mts", "inferred-mod-1"),
            own("repo:extensions/esbuild-common.mts", "npm-mod-1"),
            own("repo:a.ts", "npm-mod-1"),
            own("repo:shared/util.ts", "npm-mod-2"),
            own("repo:shared/util.ts", "inferred-mod-2"),
        ];
        assert_eq!(
            detect_duplicate_file_ownership(&ownership),
            vec![
                "repo:extensions/esbuild-common.mts".to_string(),
                "repo:shared/util.ts".to_string(),
            ]
        );
    }
}
