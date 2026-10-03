//! TS-ALIAS-RESOLUTION-1 (RG-REQ-006-L04 first clause; RG-REQ-002-L11; D-TSA-RECORD-CONFLICT-1,
//! D-TSA-BOUNDED-SCOPE-1): the pure lookup-and-match behind the resolver's tsconfig `paths` stage.
//!
//! A file whose stored alias signal carries `soleInspectedCoveringProjectMapping` — written by repo-index
//! only when exactly one inspected tsconfig project covers the file and has `paths` — gets that one
//! mapping as a [`TsconfigAliasConfig`]; a specifier is matched against it by
//! `repo_graph_import_resolver::tsconfig_alias_hits` (TypeScript's selection), the ONE matcher
//! (D-TSA-RESOLVER-EDGE-1). A file without a mapping has no entry, so the stage never acts for it.
//!
//! - what: the per-file mapping lookup the orchestrator builds once per index, and the match of a
//!   specifier against the source file's mapping.
//! - concrete current users: built by `orchestrator` (beside `npm_workspace_packages`); read by the
//!   resolver's TS-only `paths` stage (`resolver::resolve_tsconfig_paths_import`).
//! - force: `resolver.rs` is far over the 500-line guardrail; the TS-WORKSPACE-RESOLUTION-1
//!   precedent (`workspace_import.rs`) holds its stage's pure part the same way.
//! - rejected simpler: inlining into `resolver.rs` (the guardrail); a trait (one producer, one
//!   consumer — a map and two functions suffice). Imports nothing from `resolver.rs`.

use std::collections::{BTreeMap, HashMap};

use repo_graph_classification::types::StoredTsconfigAliases;
use repo_graph_import_resolver::{
    tsconfig_alias_hits, FileInventory, TsconfigAliasConfig, TsconfigAliasHits,
};

/// The `basis` a STATIC IMPORTS edge bound through `paths` carries (the target is its one
/// candidate). Stored contract: storage's `RECORDED_BASES` names the same string.
pub const TSCONFIG_PATHS_BASIS: &str = "tsconfig_paths";

/// The `basis` an unresolved IMPORTS row carries when the selected pattern's deciding substitution
/// reaches two or more indexed files (every one recorded, none picked).
pub const AMBIGUOUS_TSCONFIG_PATHS_BASIS: &str = "ambiguous_tsconfig_paths";

/// The `basis` an unresolved IMPORTS row carries when two or more wildcard patterns tie for the
/// longest prefix and their deciding substitutions reach one or more indexed files (every one
/// recorded, none picked — the stored `paths` carry no declaration order).
pub const TIED_TSCONFIG_PATHS_PATTERNS_BASIS: &str = "tied_tsconfig_paths_patterns";

/// Each file's one `paths` mapping, keyed by file uid (`<repo_uid>:<path>`), and the FILE
/// inventory the mappings resolve against.
#[derive(Debug, Clone, Default)]
pub struct TsconfigPathsIndex {
    /// File uid → the mapping of the one inspected tsconfig project covering that file.
    pub config_by_file: HashMap<String, TsconfigAliasConfig>,
    /// Every FILE stable key of the snapshot.
    pub inventory: FileInventory,
}

impl TsconfigPathsIndex {
    /// Build from the snapshot's signal rows (`(file_uid, tsconfig_aliases_json)`) and its FILE
    /// stable keys. A row without `soleInspectedCoveringProjectMapping` (every HEAD-written row, every file
    /// zero or several inspected projects cover), or whose JSON does not decode as
    /// [`StoredTsconfigAliases`], adds no entry.
    pub fn build<'a>(
        signal_rows: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
        file_stable_keys: impl IntoIterator<Item = String>,
    ) -> Self {
        let mut config_by_file = HashMap::new();
        for (file_uid, raw) in signal_rows {
            let Some(raw) = raw else { continue };
            let Ok(stored) = serde_json::from_str::<StoredTsconfigAliases>(raw) else {
                continue;
            };
            let Some(mapping) = stored.sole_inspected_covering_project_mapping else {
                continue;
            };
            let paths: BTreeMap<String, Vec<String>> = mapping
                .entries
                .into_iter()
                .map(|e| (e.pattern, e.substitutions))
                .collect();
            config_by_file.insert(
                file_uid.to_string(),
                TsconfigAliasConfig {
                    base_url: mapping.base_url,
                    paths,
                    partition_prefix: mapping.anchor_dir,
                },
            );
        }
        if config_by_file.is_empty() {
            return Self::default();
        }
        Self {
            config_by_file,
            inventory: FileInventory::from_file_keys(file_stable_keys),
        }
    }

    /// True when no file carries a mapping (the stage is a no-op).
    pub fn is_empty(&self) -> bool {
        self.config_by_file.is_empty()
    }

    /// TypeScript's `paths` selection of `specifier` under the mapping of the file `file_uid`;
    /// `None` when the file carries no mapping.
    pub fn hits_for(&self, file_uid: &str, specifier: &str) -> Option<TsconfigAliasHits> {
        let config = self.config_by_file.get(file_uid)?;
        Some(tsconfig_alias_hits(specifier, config, &self.inventory))
    }
}
