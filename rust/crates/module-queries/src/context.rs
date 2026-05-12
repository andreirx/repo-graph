//! Module query context with unified read model.
//!
//! Provides a unified read model for module data from the `module_candidates`
//! and `module_file_ownership` tables.
//!
//! ## Phase 4: MODULE-node fallback deprecated (2026-05-10)
//!
//! Prior to Phase 4, this module supported two backing representations:
//! - Primary: `module_candidates` table + `module_file_ownership` table
//! - Fallback: `nodes` table (kind='MODULE') + `edges` table (type='OWNS')
//!
//! The fallback has been removed. Module topology now comes exclusively from
//! `module_candidates`, which is populated by:
//! - Declared modules (Cargo.toml, package.json, pyproject.toml, settings.gradle)
//! - Inferred modules (top-level directory heuristic for manifest-less repos)
//!
//! If `module_candidates` is empty for a snapshot, the context will be empty.
//! This is intentional: it surfaces repos that need module detection rather
//! than silently synthesizing modules from generic graph structure.

use repo_graph_storage::crud::module_edges_support::{FileOwnership, OwnedFileForRollup};
use repo_graph_storage::error::StorageError;
use repo_graph_storage::types::ModuleCandidate;
use repo_graph_storage::StorageConnection;

/// Bundled module query context with normalized data.
///
/// All module commands should consume this context instead of raw storage
/// queries. The context loads data from `module_candidates` and
/// `module_file_ownership` tables exclusively.
#[derive(Debug, Clone)]
pub struct ModuleQueryContext {
    /// All modules in the snapshot from `module_candidates` table.
    pub modules: Vec<ModuleCandidate>,

    /// File ownership mapping (file_uid → module_candidate_uid).
    pub ownership: Vec<FileOwnership>,

    /// Owned files with path and is_test flag for rollup computation.
    pub owned_files: Vec<OwnedFileForRollup>,

    /// Deprecated: always false after Phase 4.
    /// Kept for backward compatibility with any code checking this field.
    /// The MODULE-node fallback has been removed.
    pub is_fallback: bool,
}

impl ModuleQueryContext {
    /// Load module context for a snapshot.
    ///
    /// Loads data exclusively from `module_candidates` and `module_file_ownership`
    /// tables. If no modules exist, returns an empty context (no fallback).
    ///
    /// ## Phase 4 change
    ///
    /// Prior to Phase 4, this method fell back to MODULE nodes if `module_candidates`
    /// was empty. That fallback has been removed. Empty `module_candidates` now
    /// results in an empty context, surfacing repos that need module detection.
    pub fn load(storage: &StorageConnection, snapshot_uid: &str) -> Result<Self, StorageError> {
        // Load from module_candidates table (the only source after Phase 4)
        let modules = storage.get_module_candidates_for_snapshot(snapshot_uid)?;
        let ownership = storage.get_file_ownership_for_snapshot(snapshot_uid)?;
        let owned_files = storage.get_owned_files_for_rollup(snapshot_uid)?;

        Ok(Self {
            modules,
            ownership,
            owned_files,
            is_fallback: false, // Always false after Phase 4
        })
    }

    /// Resolve a module argument (UID, key, or canonical path) to a module.
    ///
    /// Resolution precedence:
    /// 1. Exact `canonical_root_path` match
    /// 2. Exact `module_key` match
    /// 3. Exact `module_candidate_uid` match
    ///
    /// Returns `None` if no module matches.
    pub fn resolve_module(&self, arg: &str) -> Option<&ModuleCandidate> {
        // Try canonical_root_path first (most common CLI usage)
        if let Some(m) = self.modules.iter().find(|m| m.canonical_root_path == arg) {
            return Some(m);
        }

        // Try module_key
        if let Some(m) = self.modules.iter().find(|m| m.module_key == arg) {
            return Some(m);
        }

        // Try module_candidate_uid
        self.modules.iter().find(|m| m.module_candidate_uid == arg)
    }

    /// Get files owned by a specific module.
    ///
    /// Returns owned files filtered to the given module_candidate_uid,
    /// sorted by file_path.
    pub fn files_for_module(&self, module_uid: &str) -> Vec<&OwnedFileForRollup> {
        let mut files: Vec<_> = self
            .owned_files
            .iter()
            .filter(|f| f.module_candidate_uid == module_uid)
            .collect();
        files.sort_by(|a, b| a.file_path.cmp(&b.file_path));
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_module(uid: &str, path: &str, key: &str) -> ModuleCandidate {
        ModuleCandidate {
            module_candidate_uid: uid.to_string(),
            snapshot_uid: "snap-1".to_string(),
            repo_uid: "repo-1".to_string(),
            module_key: key.to_string(),
            module_kind: "directory".to_string(),
            canonical_root_path: path.to_string(),
            confidence: 1.0,
            display_name: None,
            metadata_json: None,
        }
    }

    #[test]
    fn resolve_module_by_canonical_path() {
        let ctx = ModuleQueryContext {
            modules: vec![
                make_module("m1", "src/core", "npm:core"),
                make_module("m2", "src/cli", "npm:cli"),
            ],
            ownership: vec![],
            owned_files: vec![],
            is_fallback: false,
        };

        let result = ctx.resolve_module("src/core");
        assert!(result.is_some());
        assert_eq!(result.unwrap().module_candidate_uid, "m1");
    }

    #[test]
    fn resolve_module_by_key() {
        let ctx = ModuleQueryContext {
            modules: vec![make_module("m1", "src/core", "npm:core")],
            ownership: vec![],
            owned_files: vec![],
            is_fallback: false,
        };

        let result = ctx.resolve_module("npm:core");
        assert!(result.is_some());
        assert_eq!(result.unwrap().module_candidate_uid, "m1");
    }

    #[test]
    fn resolve_module_by_uid() {
        let ctx = ModuleQueryContext {
            modules: vec![make_module("m1", "src/core", "npm:core")],
            ownership: vec![],
            owned_files: vec![],
            is_fallback: false,
        };

        let result = ctx.resolve_module("m1");
        assert!(result.is_some());
        assert_eq!(result.unwrap().canonical_root_path, "src/core");
    }

    #[test]
    fn resolve_module_not_found() {
        let ctx = ModuleQueryContext {
            modules: vec![make_module("m1", "src/core", "npm:core")],
            ownership: vec![],
            owned_files: vec![],
            is_fallback: false,
        };

        let result = ctx.resolve_module("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn files_for_module_filters_correctly() {
        let ctx = ModuleQueryContext {
            modules: vec![],
            ownership: vec![],
            owned_files: vec![
                OwnedFileForRollup {
                    file_uid: "f1".to_string(),
                    file_path: "src/core/a.ts".to_string(),
                    module_candidate_uid: "m1".to_string(),
                    is_test: false,
                },
                OwnedFileForRollup {
                    file_uid: "f2".to_string(),
                    file_path: "src/cli/b.ts".to_string(),
                    module_candidate_uid: "m2".to_string(),
                    is_test: false,
                },
                OwnedFileForRollup {
                    file_uid: "f3".to_string(),
                    file_path: "src/core/c.ts".to_string(),
                    module_candidate_uid: "m1".to_string(),
                    is_test: true,
                },
            ],
            is_fallback: false,
        };

        let files = ctx.files_for_module("m1");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file_path, "src/core/a.ts");
        assert_eq!(files[1].file_path, "src/core/c.ts");
    }

    // Storage-backed tests for ModuleQueryContext::load() are in
    // rust/crates/rgr/tests/modules_list_command.rs as integration tests.
    // They test:
    // - Empty module_candidates returns empty context (no fallback)
    // - Populated module_candidates works correctly
    // - Legacy MODULE nodes are NOT used as fallback after Phase 4
}
