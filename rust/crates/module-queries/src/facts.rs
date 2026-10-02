//! Module graph facts loading and precomputation.
//!
//! This module provides the single-load orchestration for module graph data.
//! All module commands should use `load_module_graph_facts` to get precomputed
//! facts instead of loading and deriving edges themselves.

use crate::context::ModuleQueryContext;
use std::collections::{BTreeMap, BTreeSet};

use repo_graph_classification::import_partition::{ImportRemainder, ImportView, PartitionCounts};
use repo_graph_classification::module_edges::{
    derive_module_dependency_edges_in_view, FileOwnershipFact, ModuleDependencyEdge,
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

    /// TEST-EDGE-SCOPE-1B (D-TESB-17 row U3): an aggregate needed partition evidence the
    /// facts do not hold (an edge without its importer set, an importer without its flag
    /// row). Absent evidence is a named error, never a shorter list or a zero.
    #[error("import partition evidence missing: {0}")]
    PartitionEvidenceMissing(String),
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

    /// Derived module dependency edges of `view` (relations with at least one admitted import;
    /// counts over the admitted imports).
    pub edges: Vec<ModuleDependencyEdge>,

    /// Edge derivation diagnostics (import counts, ownership gaps) over the admitted imports.
    pub diagnostics: ModuleEdgeDiagnostics,

    /// Module references used for edge derivation.
    /// Cached for downstream operations that need them.
    pub module_refs: Vec<ModuleRef>,

    /// TEST-EDGE-SCOPE-1B (RG-REQ-004-L12): the import view the edges answer.
    pub view: ImportView,

    /// The cross-module imports the view excludes and the relations only through them.
    pub remainder: ImportRemainder,

    /// Each edge's four partition counts, keyed by (source, target) module uid.
    pub edge_partitions: BTreeMap<(String, String), PartitionCounts>,

    /// Every cross-module relation (admitted by the view or not) with its four partition counts —
    /// the basis of a filtered remainder (`modules deps <module>`).
    pub relations: Vec<ModuleRelationPartitions>,

    /// Every relation with INFERRED imports, counted over its inferred cells only — the edges
    /// governance evaluates to count what it does not judge (D-TESB-11).
    pub inferred_edges: Vec<ModuleDependencyEdge>,

    /// The production-partition importer files of each edge's admitted imports, keyed by
    /// (source, target) module uid — the universe of the importer UNDETERMINED count (D-TESB-09).
    pub edge_importers: BTreeMap<(String, String), BTreeSet<String>>,

    /// Importer file uid → (path, stored test flag); `None` flag = no tracked-file row.
    pub importer_files: BTreeMap<String, (String, Option<bool>)>,
}

/// TEST-EDGE-SCOPE-1B: one cross-module relation and its partition counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRelationPartitions {
    pub source_canonical_path: String,
    pub target_canonical_path: String,
    pub partitions: PartitionCounts,
}

impl ModuleGraphFacts {
    /// TEST-EDGE-SCOPE-1B: the view's remainder over the relations `keep` selects (e.g. those
    /// involving one module).
    pub fn remainder_where<F>(&self, keep: F) -> ImportRemainder
    where
        F: Fn(&ModuleRelationPartitions) -> bool,
    {
        ImportRemainder::from_relations(
            self.view,
            self.relations
                .iter()
                .filter(|r| keep(r))
                .map(|r| &r.partitions),
        )
    }

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

    /// TEST-EDGE-SCOPE-1B: the four partition counts of one edge of these facts, or `None`
    /// when the pair is not an edge of these facts — absent evidence, never a measured zero
    /// (D-TESB-17 row U3).
    pub fn partitions_of(&self, edge: &ModuleDependencyEdge) -> Option<PartitionCounts> {
        self.edge_partitions
            .get(&(
                edge.source_module_uid.clone(),
                edge.target_module_uid.clone(),
            ))
            .copied()
    }

    /// TEST-EDGE-SCOPE-1B (D-TESB-09): the `(path, flag)` rows of the production importer files of
    /// `edges` (a subset of `self.edges`, e.g. the rows `modules deps` answers), each file once.
    ///
    /// # Errors
    ///
    /// `ModuleQueryError::PartitionEvidenceMissing` when an edge has no importer set in these
    /// facts, or an importer has no flag row — the universe is never silently shortened
    /// (D-TESB-17 row U3). An edge whose admitted imports all come from test (or
    /// unknown-status-free) files has an EMPTY set, which is a measured answer.
    pub fn importer_rows<'a, I>(
        &self,
        edges: I,
    ) -> Result<Vec<(String, Option<bool>)>, ModuleQueryError>
    where
        I: IntoIterator<Item = &'a ModuleDependencyEdge>,
    {
        let mut files: BTreeSet<&str> = BTreeSet::new();
        for e in edges {
            let key = (e.source_module_uid.clone(), e.target_module_uid.clone());
            let set = self.edge_importers.get(&key).ok_or_else(|| {
                ModuleQueryError::PartitionEvidenceMissing(format!(
                    "no importer set for the module edge {} -> {}",
                    e.source_canonical_path, e.target_canonical_path
                ))
            })?;
            files.extend(set.iter().map(String::as_str));
        }
        files
            .into_iter()
            .map(|f| {
                self.importer_files.get(f).cloned().ok_or_else(|| {
                    ModuleQueryError::PartitionEvidenceMissing(format!(
                        "no test-status row for the importing file {f}"
                    ))
                })
            })
            .collect()
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
///
/// TEST-EDGE-SCOPE-1B (RG-REQ-004-L12, D-TESB-READERS-1): `view` selects the partitions the edges
/// admit — `modules list`/`modules deps`/orient pass the request's view (DEFAULT when no flag),
/// `modules show` and governance pass `ImportView::CERTAIN_WITH_TESTS` (their subject is
/// resolution, not test scope). The facts carry the view's remainder, each edge's four partition
/// counts, the inferred edges governance counts without judging, and the production importers.
pub fn load_module_graph_facts(
    storage: &StorageConnection,
    snapshot_uid: &str,
    view: ImportView,
) -> Result<ModuleGraphFacts, ModuleQueryError> {
    // 1. Load module context (no fallback after Phase 4)
    let context = ModuleQueryContext::load(storage, snapshot_uid)?;

    // 2. Load the partitioned import rows (every resolution class; the importer's test status)
    let imports = storage.file_imports_with_partition(snapshot_uid)?;

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
        .iter()
        .map(|i| ResolvedImportFact {
            source_file_uid: i.source_file_uid.clone(),
            target_file_uid: i.target_file_uid.clone(),
            partition: i.partition,
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

    // 4. The production importer files of each relation's admitted imports (D-TESB-09).
    let owner: BTreeMap<&str, &str> = ownership_facts
        .iter()
        .map(|o| (o.file_uid.as_str(), o.module_uid.as_str()))
        .collect();
    let mut edge_importers: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut importer_files: BTreeMap<String, (String, Option<bool>)> = BTreeMap::new();
    for i in &imports {
        if !view.admits(i.partition) || !i.partition.status.is_production_partition() {
            continue;
        }
        let (Some(src), Some(tgt)) = (
            owner.get(i.source_file_uid.as_str()),
            owner.get(i.target_file_uid.as_str()),
        ) else {
            continue;
        };
        if src == tgt {
            continue;
        }
        edge_importers
            .entry((src.to_string(), tgt.to_string()))
            .or_default()
            .insert(i.source_file_uid.clone());
        importer_files
            .entry(i.source_file_uid.clone())
            .or_insert_with(|| {
                (
                    i.source_path
                        .clone()
                        .unwrap_or_else(|| i.source_file_uid.clone()),
                    i.source_is_test,
                )
            });
    }

    // 5. Derive module edges: the view's, and every relation's inferred cells (governance counts
    //    what it does not judge).
    let derivation_input = ModuleEdgeDerivationInput {
        imports: import_facts,
        ownership: ownership_facts,
        modules: module_refs.clone(),
    };

    let all = derive_module_dependency_edges_in_view(derivation_input.clone(), ImportView::ALL)
        .map_err(|e| ModuleQueryError::EdgeDerivation(e.to_string()))?;
    let missing = |e: &ModuleDependencyEdge| {
        ModuleQueryError::PartitionEvidenceMissing(format!(
            "no partition counts for the module edge {} -> {}",
            e.source_canonical_path, e.target_canonical_path
        ))
    };
    let relations: Vec<ModuleRelationPartitions> = all
        .edges
        .iter()
        .map(|e| {
            Ok(ModuleRelationPartitions {
                source_canonical_path: e.source_canonical_path.clone(),
                target_canonical_path: e.target_canonical_path.clone(),
                partitions: all.partitions_of(e).ok_or_else(|| missing(e))?,
            })
        })
        .collect::<Result<_, ModuleQueryError>>()?;
    let mut inferred_edges: Vec<ModuleDependencyEdge> = Vec::new();
    for e in &all.edges {
        let p = all.partitions_of(e).ok_or_else(|| missing(e))?;
        let n = p.production_inferred + p.test_inferred;
        if n > 0 {
            inferred_edges.push(ModuleDependencyEdge {
                import_count: n,
                ..e.clone()
            });
        }
    }
    let derivation_result = derive_module_dependency_edges_in_view(derivation_input, view)
        .map_err(|e| ModuleQueryError::EdgeDerivation(e.to_string()))?;
    edge_importers.retain(|k, _| derivation_result.edge_partitions.contains_key(k));
    // Every edge of the view has a (possibly empty) measured importer set: an edge whose
    // admitted imports all come from test files has no production importer, which is a
    // measured empty set, distinct from missing evidence (D-TESB-17 row U3).
    for k in derivation_result.edge_partitions.keys() {
        edge_importers.entry(k.clone()).or_default();
    }

    Ok(ModuleGraphFacts {
        context,
        edges: derivation_result.edges,
        diagnostics: derivation_result.diagnostics,
        module_refs,
        view,
        remainder: derivation_result.remainder,
        edge_partitions: derivation_result.edge_partitions,
        relations,
        inferred_edges,
        edge_importers,
        importer_files,
    })
}

#[cfg(test)]
pub(crate) mod tests {
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

    // ── TEST-EDGE-SCOPE-1B: the view ───────────────────────────────

    /// An in-memory store with module candidates `app`, `core`, `util`; files `app/a.ts`,
    /// `app/a.test.ts` (test), `core/c.ts`, `util/u.py`; imports app/a → core (static),
    /// app/a.test → core (static), app/a.test → util (static), app/a → util (inferred).
    pub(crate) fn partitioned_store() -> (StorageConnection, String) {
        let storage = StorageConnection::open_in_memory().unwrap();
        let sql = |q: String| {
            storage.execute_raw(&q).unwrap();
        };
        sql("INSERT INTO repos (repo_uid, name, root_path, created_at) \
             VALUES ('r1', 'r1', '/tmp/r1', '2026-01-01T00:00:00Z')"
            .to_string());
        sql(
            "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) \
             VALUES ('s1', 'r1', 'full', 'ready', '2026-01-01T00:00:00Z')"
                .to_string(),
        );
        for (uid, root) in [("mc_app", "app"), ("mc_core", "core"), ("mc_util", "util")] {
            sql(format!(
                "INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, \
                 module_key, module_kind, canonical_root_path, confidence) \
                 VALUES ('{uid}', 's1', 'r1', 'dir:{root}', 'directory', '{root}', 1.0)"
            ));
        }
        for (path, module, is_test) in [
            ("app/a.ts", "mc_app", 0),
            ("app/a.test.ts", "mc_app", 1),
            ("core/c.ts", "mc_core", 0),
            ("util/u.py", "mc_util", 0),
        ] {
            sql(format!(
                "INSERT INTO files (file_uid, repo_uid, path, is_test, is_generated, is_excluded) \
                 VALUES ('r1:{path}', 'r1', '{path}', {is_test}, 0, 0)"
            ));
            sql(format!(
                "INSERT INTO module_file_ownership (snapshot_uid, repo_uid, file_uid, \
                 module_candidate_uid, assignment_kind, confidence) \
                 VALUES ('s1', 'r1', 'r1:{path}', '{module}', 'directory', 1.0)"
            ));
            sql(format!(
                "INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, name, \
                 qualified_name, file_uid) \
                 VALUES ('n:{path}', 's1', 'r1', 'r1:{path}:FILE', 'FILE', '{path}', '{path}', \
                 'r1:{path}')"
            ));
        }
        for (i, (from, to, res)) in [
            ("app/a.ts", "core/c.ts", "static"),
            ("app/a.test.ts", "core/c.ts", "static"),
            ("app/a.test.ts", "util/u.py", "static"),
            ("app/a.ts", "util/u.py", "inferred"),
        ]
        .iter()
        .enumerate()
        {
            sql(format!(
                "INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
                 target_node_uid, type, resolution, extractor) \
                 VALUES ('e{i}', 's1', 'r1', 'n:{from}', 'n:{to}', 'IMPORTS', '{res}', 't:1')"
            ));
        }
        (storage, "s1".to_string())
    }

    fn pairs(facts: &ModuleGraphFacts) -> Vec<(String, String, u64)> {
        facts
            .edges
            .iter()
            .map(|e| {
                (
                    e.source_canonical_path.clone(),
                    e.target_canonical_path.clone(),
                    e.import_count,
                )
            })
            .collect()
    }

    #[test]
    fn facts_default_view_drops_test_and_inferred_imports_and_counts_them() {
        let (storage, snap) = partitioned_store();
        let facts = load_module_graph_facts(&storage, &snap, ImportView::DEFAULT).unwrap();
        assert_eq!(pairs(&facts), vec![("app".into(), "core".into(), 1)]);
        assert_eq!(facts.view, ImportView::DEFAULT);
        // app→core keeps its test import in its partition counts.
        let p = facts
            .partitions_of(&facts.edges[0])
            .expect("an edge of the facts");
        assert_eq!((p.production_certain, p.test_certain), (1, 1));
        // app→util exists only through excluded imports: one test certain, one inferred.
        assert_eq!(facts.remainder.tests.imports, 2);
        assert_eq!(
            facts.remainder.tests.edges, 1,
            "app→util first shows under --include-tests"
        );
        assert_eq!(facts.remainder.inferred.imports, 1);
        // The importer universe: production files importing across modules in the view.
        let rows = facts.importer_rows(&facts.edges).unwrap();
        assert_eq!(rows, vec![("app/a.ts".to_string(), Some(false))]);
        // Governance counts the inferred relation without judging it.
        assert_eq!(facts.inferred_edges.len(), 1);
        assert_eq!(facts.inferred_edges[0].target_canonical_path, "util");
    }

    #[test]
    fn facts_certain_with_tests_view_keeps_test_imports_and_drops_inferred_ones() {
        let (storage, snap) = partitioned_store();
        let facts =
            load_module_graph_facts(&storage, &snap, ImportView::CERTAIN_WITH_TESTS).unwrap();
        assert_eq!(
            pairs(&facts),
            vec![
                ("app".into(), "core".into(), 2),
                ("app".into(), "util".into(), 1)
            ]
        );
        assert_eq!(facts.remainder.tests.imports, 0);
        assert_eq!(facts.remainder.inferred.imports, 1);
        assert_eq!(facts.remainder.inferred.edges, 0, "app→util is shown");
    }

    #[test]
    fn facts_partitions_of_an_edge_outside_the_facts_is_absent_never_zero() {
        // D-TESB-17 row U3: app→util exists only through excluded imports, so it is not an edge
        // of the DEFAULT facts; its counts are absent there, measured in the wider facts.
        let (storage, snap) = partitioned_store();
        let default = load_module_graph_facts(&storage, &snap, ImportView::DEFAULT).unwrap();
        let wide = load_module_graph_facts(&storage, &snap, ImportView::ALL).unwrap();
        let outside = wide
            .edges
            .iter()
            .find(|e| e.target_canonical_path == "util")
            .expect("app→util is an edge of the widest view");
        assert_eq!(default.partitions_of(outside), None);
        let measured = wide
            .partitions_of(outside)
            .expect("measured in the wide facts");
        assert_eq!(
            (measured.test_certain, measured.production_inferred),
            (1, 1)
        );
    }

    #[test]
    fn facts_importer_rows_fail_on_an_edge_without_its_importer_evidence() {
        let (storage, snap) = partitioned_store();
        let default = load_module_graph_facts(&storage, &snap, ImportView::DEFAULT).unwrap();
        let wide = load_module_graph_facts(&storage, &snap, ImportView::ALL).unwrap();
        let outside = wide
            .edges
            .iter()
            .find(|e| e.target_canonical_path == "util")
            .unwrap();
        // An edge without its importer set: a named error, never a shorter list.
        let err = default
            .importer_rows(std::iter::once(outside))
            .expect_err("an edge outside the facts has no importer evidence");
        assert!(
            matches!(&err, ModuleQueryError::PartitionEvidenceMissing(m) if m.contains("app -> util")),
            "{err}"
        );
        // An importer without its flag row: a named error too.
        let mut broken = default.clone();
        broken.importer_files.clear();
        let err = broken
            .importer_rows(&broken.edges)
            .expect_err("an importer without its flag row");
        assert!(
            matches!(&err, ModuleQueryError::PartitionEvidenceMissing(m) if m.contains("importing file")),
            "{err}"
        );
        // The complete facts answer.
        assert_eq!(default.importer_rows(&default.edges).unwrap().len(), 1);
    }
}
