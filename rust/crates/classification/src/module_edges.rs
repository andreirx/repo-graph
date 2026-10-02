//! Module edge derivation — pure policy core (RS-MG-2).
//!
//! Derives cross-module dependency edges from:
//! - Resolved import facts
//! - File ownership facts
//! - Module identity refs
//!
//! This is pure policy. No DB access. No side effects.
//!
//! Design decisions:
//! - Input DTOs are normalized (not storage row shapes)
//! - Duplicate file ownership is an explicit error
//! - Output is deterministically sorted
//! - Counts use u64
//! - Each import carries its partition (TEST-EDGE-SCOPE-1B): an edge counts only
//!   the imports its view admits, carries the four partition counts of all its
//!   imports, and the result carries the view's excluded remainder. The partition
//!   policy itself lives in `import_partition.rs`.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::import_partition::{ImportPartition, ImportRemainder, ImportView, PartitionCounts};

// ── Input DTOs ─────────────────────────────────────────────────────

/// A resolved import between two files.
///
/// Minimal normalized fact for derivation, with the partition of the import
/// (its resolution class and its importer's test status).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImportFact {
    pub source_file_uid: String,
    pub target_file_uid: String,
    pub partition: ImportPartition,
}

/// A file ownership assignment.
///
/// Minimal normalized fact for derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOwnershipFact {
    pub file_uid: String,
    pub module_uid: String,
}

/// Module identity reference.
///
/// Minimal normalized ref for derivation output enrichment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRef {
    pub module_uid: String,
    pub canonical_path: String,
}

/// Input bundle for module edge derivation.
#[derive(Debug, Clone)]
pub struct ModuleEdgeDerivationInput {
    pub imports: Vec<ResolvedImportFact>,
    pub ownership: Vec<FileOwnershipFact>,
    pub modules: Vec<ModuleRef>,
}

// ── Output DTOs ────────────────────────────────────────────────────

/// A derived cross-module dependency edge.
///
/// `import_count` and `source_file_count` count only the imports the derivation's
/// view admits; the partition counts of every cross-module import of the relation
/// are carried by the derivation result ([`ModuleEdgeDerivationResult::partitions_of`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDependencyEdge {
    pub source_module_uid: String,
    pub source_canonical_path: String,
    pub target_module_uid: String,
    pub target_canonical_path: String,
    pub import_count: u64,
    pub source_file_count: u64,
}

// ── Errors ─────────────────────────────────────────────────────────

/// Error during module edge derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleEdgeDerivationError {
    /// A file has multiple ownership assignments.
    ///
    /// Module edge derivation requires single-valued ownership.
    /// Duplicate ownership produces unstable architecture facts.
    ///
    /// This hard error is retained as the invariant witness (MODULE-OWNERSHIP-
    /// DUPLICATE-1): generation resolves the known npm-vs-inferred collision (npm
    /// wins, `repo-index` compose), so this should not fire on the module command
    /// path. The module-command surface pre-detects duplicate ownership in
    /// `module-queries::load_module_graph_facts` and degrades honestly (naming the
    /// affected files) BEFORE reaching this function; this error stays the
    /// last-line guard for other callers (e.g. gate evaluation) and as
    /// defense-in-depth.
    DuplicateOwnership {
        file_uid: String,
        module_uids: Vec<String>,
    },
}

impl std::fmt::Display for ModuleEdgeDerivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleEdgeDerivationError::DuplicateOwnership {
                file_uid,
                module_uids,
            } => {
                write!(
                    f,
                    "file {} has duplicate ownership: {:?}",
                    file_uid, module_uids
                )
            }
        }
    }
}

impl std::error::Error for ModuleEdgeDerivationError {}

// ── Derivation result ──────────────────────────────────────────────

/// Result of module edge derivation.
#[derive(Debug, Clone)]
pub struct ModuleEdgeDerivationResult {
    /// Derived cross-module edges of the view (relations with at least one admitted
    /// import), sorted by (source_path, target_path).
    pub edges: Vec<ModuleDependencyEdge>,
    /// Diagnostic counts over the imports the view admits.
    pub diagnostics: ModuleEdgeDiagnostics,
    /// The view the edges answer.
    pub view: ImportView,
    /// The cross-module imports the view excludes, and the relations that exist
    /// only through them, per remainder group.
    pub remainder: ImportRemainder,
    /// The four partition counts of every edge in `edges`, keyed by its
    /// (source module uid, target module uid): every cross-module import of the
    /// relation in its own cell, admitted or not.
    pub edge_partitions: BTreeMap<(String, String), PartitionCounts>,
}

impl ModuleEdgeDerivationResult {
    /// The partition counts of one edge of this result, or `None` when the pair is
    /// not an edge of the result — absent evidence, never a measured zero
    /// (RG-REQ-002-L04; D-TESB-17 row U2).
    pub fn partitions_of(&self, edge: &ModuleDependencyEdge) -> Option<PartitionCounts> {
        self.edge_partitions
            .get(&(
                edge.source_module_uid.clone(),
                edge.target_module_uid.clone(),
            ))
            .copied()
    }
}

/// Diagnostic counts from derivation, over the imports the view admits.
#[derive(Debug, Clone, Default)]
pub struct ModuleEdgeDiagnostics {
    /// Total resolved import facts processed.
    pub imports_total: u64,
    /// Imports where source file has no ownership (excluded).
    pub imports_source_unowned: u64,
    /// Imports where target file has no ownership (excluded).
    pub imports_target_unowned: u64,
    /// Imports within the same module (excluded).
    pub imports_intra_module: u64,
    /// Imports crossing module boundaries (included).
    pub imports_cross_module: u64,
}

// ── Pure derivation function ───────────────────────────────────────

/// Derive cross-module dependency edges from raw facts, admitting every import
/// passed in whatever its partition (the view [`ImportView::ALL`]).
///
/// Callers that answer a partitioned surface use
/// [`derive_module_dependency_edges_in_view`].
pub fn derive_module_dependency_edges(
    input: ModuleEdgeDerivationInput,
) -> Result<ModuleEdgeDerivationResult, ModuleEdgeDerivationError> {
    derive_module_dependency_edges_in_view(input, ImportView::ALL)
}

/// Derive the cross-module dependency edges of one view from raw facts.
///
/// Algorithm:
/// 1. Build file → module ownership index (error on duplicates)
/// 2. Build module_uid → canonical_path lookup
/// 3. For each resolved import whose source and target files are owned by
///    different modules, count it in its relation's partition cell; when the
///    view admits it, also count it in the relation's import count, its source
///    file set and the diagnostics
/// 4. Emit the relations with at least one admitted import; the others form the
///    remainder (with every excluded import, per group)
/// 5. Sort deterministically
///
/// Returns error if any file has duplicate ownership assignments.
pub fn derive_module_dependency_edges_in_view(
    input: ModuleEdgeDerivationInput,
    view: ImportView,
) -> Result<ModuleEdgeDerivationResult, ModuleEdgeDerivationError> {
    // 1. Build ownership index, detecting duplicates
    let ownership_index = build_ownership_index(&input.ownership)?;

    // 2. Build module lookup
    let module_lookup: HashMap<&str, &str> = input
        .modules
        .iter()
        .map(|m| (m.module_uid.as_str(), m.canonical_path.as_str()))
        .collect();

    // 3. Process imports and aggregate
    let mut diagnostics = ModuleEdgeDiagnostics::default();
    let mut edge_aggregates: HashMap<(&str, &str), EdgeAggregate> = HashMap::new();

    for import in &input.imports {
        let admitted = view.admits(import.partition);
        if admitted {
            diagnostics.imports_total += 1;
        }

        // Look up source module
        let source_module = match ownership_index.get(import.source_file_uid.as_str()) {
            Some(m) => *m,
            None => {
                if admitted {
                    diagnostics.imports_source_unowned += 1;
                }
                continue;
            }
        };

        // Look up target module
        let target_module = match ownership_index.get(import.target_file_uid.as_str()) {
            Some(m) => *m,
            None => {
                if admitted {
                    diagnostics.imports_target_unowned += 1;
                }
                continue;
            }
        };

        // Skip intra-module imports
        if source_module == target_module {
            if admitted {
                diagnostics.imports_intra_module += 1;
            }
            continue;
        }

        // Aggregate: every cross-module import lands in its partition cell.
        let agg = edge_aggregates
            .entry((source_module, target_module))
            .or_default();
        agg.partitions.add(import.partition);
        if admitted {
            diagnostics.imports_cross_module += 1;
            agg.import_count += 1;
            agg.source_files.insert(&import.source_file_uid);
        }
    }

    // 4. The remainder over every relation, then the view's edges.
    let remainder =
        ImportRemainder::from_relations(view, edge_aggregates.values().map(|a| &a.partitions));
    let mut edge_partitions = BTreeMap::new();
    let mut edges: Vec<ModuleDependencyEdge> = edge_aggregates
        .into_iter()
        .filter(|(_, agg)| agg.import_count > 0)
        .filter_map(|((source_uid, target_uid), agg)| {
            let source_path = module_lookup.get(source_uid)?;
            let target_path = module_lookup.get(target_uid)?;
            edge_partitions.insert(
                (source_uid.to_string(), target_uid.to_string()),
                agg.partitions,
            );
            Some(ModuleDependencyEdge {
                source_module_uid: source_uid.to_string(),
                source_canonical_path: (*source_path).to_string(),
                target_module_uid: target_uid.to_string(),
                target_canonical_path: (*target_path).to_string(),
                import_count: agg.import_count,
                source_file_count: agg.source_files.len() as u64,
            })
        })
        .collect();

    // 5. Sort deterministically
    edges.sort_by(|a, b| {
        a.source_canonical_path
            .cmp(&b.source_canonical_path)
            .then_with(|| a.target_canonical_path.cmp(&b.target_canonical_path))
    });

    Ok(ModuleEdgeDerivationResult {
        edges,
        diagnostics,
        view,
        remainder,
        edge_partitions,
    })
}

// ── Internal helpers ───────────────────────────────────────────────

/// Aggregation state for a single (source, target) module pair.
#[derive(Debug, Default)]
struct EdgeAggregate<'a> {
    /// Admitted imports.
    import_count: u64,
    /// Source files of the admitted imports.
    source_files: HashSet<&'a str>,
    /// Every cross-module import of the relation, in its partition cell.
    partitions: PartitionCounts,
}

/// Build file → module ownership index, erroring on duplicates.
fn build_ownership_index(
    ownership: &[FileOwnershipFact],
) -> Result<HashMap<&str, &str>, ModuleEdgeDerivationError> {
    // First pass: collect all assignments per file
    let mut file_to_modules: HashMap<&str, Vec<&str>> = HashMap::new();
    for fact in ownership {
        file_to_modules
            .entry(fact.file_uid.as_str())
            .or_default()
            .push(fact.module_uid.as_str());
    }

    // Second pass: detect duplicates and build index
    let mut index: HashMap<&str, &str> = HashMap::new();
    for (file_uid, module_uids) in file_to_modules {
        if module_uids.len() > 1 {
            return Err(ModuleEdgeDerivationError::DuplicateOwnership {
                file_uid: file_uid.to_string(),
                module_uids: module_uids.into_iter().map(String::from).collect(),
            });
        }
        index.insert(file_uid, module_uids[0]);
    }

    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_partition::{ImportClass, ImporterStatus, RemainderCount};

    const PROD_CERTAIN: ImportPartition = ImportPartition {
        class: ImportClass::Certain,
        status: ImporterStatus::Production,
    };
    const TEST_CERTAIN: ImportPartition = ImportPartition {
        class: ImportClass::Certain,
        status: ImporterStatus::Test,
    };
    const PROD_INFERRED: ImportPartition = ImportPartition {
        class: ImportClass::Inferred,
        status: ImporterStatus::Production,
    };
    const TEST_INFERRED: ImportPartition = ImportPartition {
        class: ImportClass::Inferred,
        status: ImporterStatus::Test,
    };
    const UNKNOWN_CERTAIN: ImportPartition = ImportPartition {
        class: ImportClass::Certain,
        status: ImporterStatus::Unknown,
    };

    fn make_import(source: &str, target: &str) -> ResolvedImportFact {
        make_partitioned_import(source, target, PROD_CERTAIN)
    }

    fn make_partitioned_import(
        source: &str,
        target: &str,
        partition: ImportPartition,
    ) -> ResolvedImportFact {
        ResolvedImportFact {
            source_file_uid: source.to_string(),
            target_file_uid: target.to_string(),
            partition,
        }
    }

    /// Three modules: app (files a1, a2, at), core (c), util (u).
    fn three_module_input(imports: Vec<ResolvedImportFact>) -> ModuleEdgeDerivationInput {
        ModuleEdgeDerivationInput {
            imports,
            ownership: vec![
                make_ownership("a1", "mod-app"),
                make_ownership("a2", "mod-app"),
                make_ownership("at", "mod-app"),
                make_ownership("c", "mod-core"),
                make_ownership("u", "mod-util"),
            ],
            modules: vec![
                make_module("mod-app", "app"),
                make_module("mod-core", "core"),
                make_module("mod-util", "util"),
            ],
        }
    }

    fn make_ownership(file: &str, module: &str) -> FileOwnershipFact {
        FileOwnershipFact {
            file_uid: file.to_string(),
            module_uid: module.to_string(),
        }
    }

    fn make_module(uid: &str, path: &str) -> ModuleRef {
        ModuleRef {
            module_uid: uid.to_string(),
            canonical_path: path.to_string(),
        }
    }

    // ── Basic derivation ───────────────────────────────────────────

    #[test]
    fn empty_input_produces_empty_output() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![],
            ownership: vec![],
            modules: vec![],
        };

        let result = derive_module_dependency_edges(input).expect("derivation");

        assert!(result.edges.is_empty());
        assert_eq!(result.diagnostics.imports_total, 0);
    }

    #[test]
    fn cross_module_import_produces_edge() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![make_import("file-a", "file-b")],
            ownership: vec![
                make_ownership("file-a", "mod-app"),
                make_ownership("file-b", "mod-core"),
            ],
            modules: vec![
                make_module("mod-app", "packages/app"),
                make_module("mod-core", "packages/core"),
            ],
        };

        let result = derive_module_dependency_edges(input).expect("derivation");

        assert_eq!(result.edges.len(), 1);
        let edge = &result.edges[0];
        assert_eq!(edge.source_module_uid, "mod-app");
        assert_eq!(edge.source_canonical_path, "packages/app");
        assert_eq!(edge.target_module_uid, "mod-core");
        assert_eq!(edge.target_canonical_path, "packages/core");
        assert_eq!(edge.import_count, 1);
        assert_eq!(edge.source_file_count, 1);
    }

    #[test]
    fn intra_module_import_excluded() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![make_import("file-a", "file-b")],
            ownership: vec![
                make_ownership("file-a", "mod-app"),
                make_ownership("file-b", "mod-app"), // same module
            ],
            modules: vec![make_module("mod-app", "packages/app")],
        };

        let result = derive_module_dependency_edges(input).expect("derivation");

        assert!(result.edges.is_empty());
        assert_eq!(result.diagnostics.imports_intra_module, 1);
        assert_eq!(result.diagnostics.imports_cross_module, 0);
    }

    #[test]
    fn unowned_source_excluded() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![make_import("unowned-file", "file-b")],
            ownership: vec![make_ownership("file-b", "mod-core")],
            modules: vec![make_module("mod-core", "packages/core")],
        };

        let result = derive_module_dependency_edges(input).expect("derivation");

        assert!(result.edges.is_empty());
        assert_eq!(result.diagnostics.imports_source_unowned, 1);
    }

    #[test]
    fn unowned_target_excluded() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![make_import("file-a", "unowned-file")],
            ownership: vec![make_ownership("file-a", "mod-app")],
            modules: vec![make_module("mod-app", "packages/app")],
        };

        let result = derive_module_dependency_edges(input).expect("derivation");

        assert!(result.edges.is_empty());
        assert_eq!(result.diagnostics.imports_target_unowned, 1);
    }

    // ── Aggregation ────────────────────────────────────────────────

    #[test]
    fn multiple_imports_aggregated() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![
                make_import("file-a1", "file-b"),
                make_import("file-a2", "file-b"),
                make_import("file-a1", "file-b"), // duplicate import from same file
            ],
            ownership: vec![
                make_ownership("file-a1", "mod-app"),
                make_ownership("file-a2", "mod-app"),
                make_ownership("file-b", "mod-core"),
            ],
            modules: vec![
                make_module("mod-app", "packages/app"),
                make_module("mod-core", "packages/core"),
            ],
        };

        let result = derive_module_dependency_edges(input).expect("derivation");

        assert_eq!(result.edges.len(), 1);
        let edge = &result.edges[0];
        assert_eq!(edge.import_count, 3); // all 3 imports counted
        assert_eq!(edge.source_file_count, 2); // only 2 distinct source files
    }

    // ── Duplicate ownership error ──────────────────────────────────

    #[test]
    fn duplicate_ownership_returns_error() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![],
            ownership: vec![
                make_ownership("shared-file", "mod-1"),
                make_ownership("shared-file", "mod-2"), // duplicate
            ],
            modules: vec![
                make_module("mod-1", "packages/mod1"),
                make_module("mod-2", "packages/mod2"),
            ],
        };

        let result = derive_module_dependency_edges(input);

        assert!(result.is_err());
        match result.unwrap_err() {
            ModuleEdgeDerivationError::DuplicateOwnership {
                file_uid,
                module_uids,
            } => {
                assert_eq!(file_uid, "shared-file");
                assert_eq!(module_uids.len(), 2);
                assert!(module_uids.contains(&"mod-1".to_string()));
                assert!(module_uids.contains(&"mod-2".to_string()));
            }
        }
    }

    // ── Deterministic ordering ─────────────────────────────────────

    #[test]
    fn edges_sorted_by_source_then_target_path() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![
                make_import("file-c", "file-a"),
                make_import("file-a", "file-c"),
                make_import("file-b", "file-c"),
            ],
            ownership: vec![
                make_ownership("file-a", "mod-a"),
                make_ownership("file-b", "mod-b"),
                make_ownership("file-c", "mod-c"),
            ],
            modules: vec![
                make_module("mod-a", "packages/alpha"),
                make_module("mod-b", "packages/beta"),
                make_module("mod-c", "packages/charlie"),
            ],
        };

        let result = derive_module_dependency_edges(input).expect("derivation");

        assert_eq!(result.edges.len(), 3);
        // Sorted by (source_path, target_path)
        assert_eq!(result.edges[0].source_canonical_path, "packages/alpha");
        assert_eq!(result.edges[0].target_canonical_path, "packages/charlie");
        assert_eq!(result.edges[1].source_canonical_path, "packages/beta");
        assert_eq!(result.edges[1].target_canonical_path, "packages/charlie");
        assert_eq!(result.edges[2].source_canonical_path, "packages/charlie");
        assert_eq!(result.edges[2].target_canonical_path, "packages/alpha");
    }

    // ── Diagnostics ────────────────────────────────────────────────

    #[test]
    fn diagnostics_counts_all_categories() {
        let input = ModuleEdgeDerivationInput {
            imports: vec![
                make_import("file-a", "file-b"),  // cross-module
                make_import("file-a", "file-a2"), // intra-module
                make_import("unowned", "file-b"), // source unowned
                make_import("file-a", "unowned"), // target unowned
                make_import("file-b", "file-a"),  // cross-module
            ],
            ownership: vec![
                make_ownership("file-a", "mod-app"),
                make_ownership("file-a2", "mod-app"),
                make_ownership("file-b", "mod-core"),
            ],
            modules: vec![
                make_module("mod-app", "packages/app"),
                make_module("mod-core", "packages/core"),
            ],
        };

        let result = derive_module_dependency_edges(input).expect("derivation");

        assert_eq!(result.diagnostics.imports_total, 5);
        assert_eq!(result.diagnostics.imports_cross_module, 2);
        assert_eq!(result.diagnostics.imports_intra_module, 1);
        assert_eq!(result.diagnostics.imports_source_unowned, 1);
        assert_eq!(result.diagnostics.imports_target_unowned, 1);
    }

    // ── Partitions (TEST-EDGE-SCOPE-1B) ────────────────────────────

    #[test]
    fn module_edge_carries_its_four_partition_counts_and_the_unknown_tally() {
        let input = three_module_input(vec![
            make_partitioned_import("a1", "c", PROD_CERTAIN),
            make_partitioned_import("a2", "c", UNKNOWN_CERTAIN),
            make_partitioned_import("at", "c", TEST_CERTAIN),
            make_partitioned_import("a1", "c", PROD_INFERRED),
            make_partitioned_import("at", "c", TEST_INFERRED),
        ]);
        let result =
            derive_module_dependency_edges_in_view(input, ImportView::DEFAULT).expect("derivation");
        assert_eq!(result.edges.len(), 1);
        assert_eq!(
            result.partitions_of(&result.edges[0]),
            Some(PartitionCounts {
                production_certain: 2,
                test_certain: 1,
                production_inferred: 1,
                test_inferred: 1,
                unknown_test_status: 1,
            })
        );
        assert_eq!(result.edges[0].import_count, 2, "only the admitted cells");
    }

    #[test]
    fn a_relation_in_two_partitions_counts_each_import_in_its_own_partition() {
        let imports = vec![
            make_partitioned_import("a1", "c", PROD_CERTAIN),
            make_partitioned_import("at", "c", TEST_CERTAIN),
            make_partitioned_import("at", "c", TEST_CERTAIN),
        ];
        let default = derive_module_dependency_edges_in_view(
            three_module_input(imports.clone()),
            ImportView::DEFAULT,
        )
        .expect("derivation");
        let with_tests = derive_module_dependency_edges_in_view(
            three_module_input(imports),
            ImportView::CERTAIN_WITH_TESTS,
        )
        .expect("derivation");
        assert_eq!(default.edges.len(), 1);
        assert_eq!(with_tests.edges.len(), 1);
        let parts = default
            .partitions_of(&default.edges[0])
            .expect("an edge of the result");
        assert_eq!(Some(parts), with_tests.partitions_of(&with_tests.edges[0]));
        assert_eq!(parts.production_certain, 1);
        assert_eq!(parts.test_certain, 2);
        assert_eq!(default.edges[0].import_count, 1);
        assert_eq!(with_tests.edges[0].import_count, 3);
        // The relation is in the view, so it is not a remainder relation; its test
        // imports are remainder imports.
        assert_eq!(
            default.remainder.tests,
            RemainderCount {
                imports: 2,
                edges: 0
            }
        );
        assert!(with_tests.remainder.is_empty());
    }

    #[test]
    fn view_edge_counts_only_admitted_imports_and_their_source_files() {
        let input = three_module_input(vec![
            make_partitioned_import("a1", "c", PROD_CERTAIN),
            make_partitioned_import("a1", "c", PROD_CERTAIN),
            make_partitioned_import("a2", "c", PROD_INFERRED),
            make_partitioned_import("at", "c", TEST_CERTAIN),
        ]);
        let result =
            derive_module_dependency_edges_in_view(input, ImportView::DEFAULT).expect("derivation");
        let edge = &result.edges[0];
        assert_eq!(edge.import_count, 2);
        assert_eq!(
            edge.source_file_count, 1,
            "a2 and at contribute no admitted import"
        );
        assert_eq!(result.diagnostics.imports_cross_module, 2);
        assert_eq!(result.diagnostics.imports_total, 2);
        assert_eq!(result.view, ImportView::DEFAULT);
    }

    #[test]
    fn a_relation_only_through_excluded_imports_leaves_the_view_and_is_counted_in_the_remainder() {
        let input = || {
            three_module_input(vec![
                make_partitioned_import("a1", "c", PROD_CERTAIN),
                make_partitioned_import("at", "u", TEST_CERTAIN),
                make_partitioned_import("at", "u", TEST_CERTAIN),
                make_partitioned_import("c", "u", PROD_INFERRED),
                make_partitioned_import("u", "a1", TEST_INFERRED),
            ])
        };
        let default = derive_module_dependency_edges_in_view(input(), ImportView::DEFAULT)
            .expect("derivation");
        let pairs: Vec<(&str, &str)> = default
            .edges
            .iter()
            .map(|e| {
                (
                    e.source_canonical_path.as_str(),
                    e.target_canonical_path.as_str(),
                )
            })
            .collect();
        assert_eq!(pairs, vec![("app", "core")]);
        assert_eq!(
            default.remainder.tests,
            RemainderCount {
                imports: 2,
                edges: 1
            }
        );
        assert_eq!(
            default.remainder.inferred,
            RemainderCount {
                imports: 1,
                edges: 1
            }
        );
        assert_eq!(
            default.remainder.tests_and_inferred,
            RemainderCount {
                imports: 1,
                edges: 1
            }
        );
        let all =
            derive_module_dependency_edges_in_view(input(), ImportView::ALL).expect("derivation");
        assert_eq!(all.edges.len(), 4);
        assert!(all.remainder.is_empty());
        // The one-argument derivation admits everything it is given.
        let unscoped = derive_module_dependency_edges(input()).expect("derivation");
        assert_eq!(unscoped.edges, all.edges);
    }

    #[test]
    fn partitions_of_a_pair_outside_the_result_is_absent_never_zero() {
        // D-TESB-17 row U2: a relation that exists only through test imports is not an
        // edge of the default view, so its counts are absent from that result — never a
        // measured all-zero record.
        let input = || {
            three_module_input(vec![
                make_partitioned_import("a1", "c", PROD_CERTAIN),
                make_partitioned_import("at", "u", TEST_CERTAIN),
            ])
        };
        let default = derive_module_dependency_edges_in_view(input(), ImportView::DEFAULT)
            .expect("derivation");
        let all =
            derive_module_dependency_edges_in_view(input(), ImportView::ALL).expect("derivation");
        let outside = all
            .edges
            .iter()
            .find(|e| e.target_canonical_path == "util")
            .expect("the test-only relation is an edge of the widest view");
        assert_eq!(default.partitions_of(outside), None);
        assert_eq!(
            all.partitions_of(outside).map(|p| p.test_certain),
            Some(1),
            "the same pair inside a result carries its measured counts"
        );
        assert!(default.partitions_of(&default.edges[0]).is_some());
    }
}
