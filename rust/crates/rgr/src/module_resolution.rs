//! Module resolution helper for CLI commands.
//!
//! This module provides a unified read model for module data that abstracts
//! over two different backing representations:
//!
//! - **TS path**: `module_candidates` table + `module_file_ownership` table
//! - **Rust path**: `nodes` table (kind='MODULE') + `edges` table (type='OWNS')
//!
//! The fallback logic is application-level normalization policy, not raw
//! storage truth. Keeping it here (not in storage CRUD) preserves honest
//! method semantics in the storage layer.
//!
//! ## Technical Debt
//!
//! Rust directory modules are synthesized compatibility projections, not full
//! module-candidate parity. The dual persistence model remains until a unified
//! module read model exists at the indexer level.

use repo_graph_storage::crud::module_edges_support::{FileOwnership, OwnedFileForRollup};
use repo_graph_storage::error::StorageError;
use repo_graph_storage::types::ModuleCandidate;
use repo_graph_storage::StorageConnection;

/// Bundled module query context with normalized data.
///
/// All module commands should consume this context instead of raw storage
/// queries. The context handles fallback logic internally and presents a
/// unified view regardless of backing representation.
#[derive(Debug)]
#[allow(dead_code)] // Some fields/methods reserved for future use
pub struct ModuleQueryContext {
    /// All modules in the snapshot (normalized from either backing model).
    pub modules: Vec<ModuleCandidate>,

    /// File ownership mapping (file_uid → module_candidate_uid).
    pub ownership: Vec<FileOwnership>,

    /// Owned files with path and is_test flag for rollup computation.
    pub owned_files: Vec<OwnedFileForRollup>,

    /// True if data came from fallback (MODULE nodes + OWNS edges).
    /// Commands can use this to adjust messaging or suppress features
    /// that don't work well with synthesized modules.
    pub is_fallback: bool,
}

impl ModuleQueryContext {
    /// Load module context for a snapshot, applying fallback if needed.
    ///
    /// Strategy:
    /// 1. Try TS tables (`module_candidates`, `module_file_ownership`)
    /// 2. If empty, fall back to Rust tables (`nodes` kind=MODULE, `edges` type=OWNS)
    /// 3. Normalize both paths into the same DTOs
    ///
    /// Returns `Ok(context)` with `is_fallback=true` if fallback was used.
    pub fn load(
        storage: &StorageConnection,
        snapshot_uid: &str,
    ) -> Result<Self, StorageError> {
        // Try TS path first: module_candidates table
        let modules = storage.get_module_candidates_for_snapshot(snapshot_uid)?;

        if !modules.is_empty() {
            // TS path: use module_candidates and module_file_ownership
            let ownership = storage.get_file_ownership_for_snapshot(snapshot_uid)?;
            let owned_files = storage.get_owned_files_for_rollup(snapshot_uid)?;

            return Ok(Self {
                modules,
                ownership,
                owned_files,
                is_fallback: false,
            });
        }

        // Fallback: Rust path using MODULE nodes and OWNS edges
        let modules = storage.get_module_nodes_as_candidates(snapshot_uid)?;
        let ownership = storage.get_file_ownership_from_owns_edges(snapshot_uid)?;
        let owned_files = storage.get_owned_files_from_owns_edges(snapshot_uid)?;

        Ok(Self {
            modules,
            ownership,
            owned_files,
            is_fallback: true,
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
        self.modules
            .iter()
            .find(|m| m.module_candidate_uid == arg)
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

    /// Get file ownership entries for a specific module.
    ///
    /// Returns ownership entries filtered to the given module_candidate_uid.
    pub fn ownership_for_module(&self, module_uid: &str) -> Vec<&FileOwnership> {
        self.ownership
            .iter()
            .filter(|o| o.module_candidate_uid == module_uid)
            .collect()
    }

    /// Count files owned by each module.
    ///
    /// Returns a map of module_candidate_uid → (source_file_count, test_file_count).
    pub fn file_counts_by_module(&self) -> std::collections::HashMap<String, (usize, usize)> {
        let mut counts: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();

        for file in &self.owned_files {
            let entry = counts.entry(file.module_candidate_uid.clone()).or_default();
            if file.is_test {
                entry.1 += 1;
            } else {
                entry.0 += 1;
            }
        }

        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests are in the command test files.
    // Unit tests here focus on resolution logic.

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

    #[test]
    fn file_counts_by_module_separates_test_and_source() {
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
                    file_path: "src/core/b.ts".to_string(),
                    module_candidate_uid: "m1".to_string(),
                    is_test: false,
                },
                OwnedFileForRollup {
                    file_uid: "f3".to_string(),
                    file_path: "src/core/a.test.ts".to_string(),
                    module_candidate_uid: "m1".to_string(),
                    is_test: true,
                },
            ],
            is_fallback: false,
        };

        let counts = ctx.file_counts_by_module();
        assert_eq!(counts.get("m1"), Some(&(2, 1))); // 2 source, 1 test
    }
}
