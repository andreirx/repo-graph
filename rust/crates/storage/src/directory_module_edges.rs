//! TEST-EDGE-SCOPE-1B (D-TESB-03): the directory-module graph of one import view, derived at
//! query time.
//!
//! `cycles`, `orient`'s cycle line and `explain`'s Import-cycles block run their SCC over the
//! directory MODULE nodes (the population `stats` calls "directory groups"). The persisted
//! MODULE→MODULE IMPORTS edges carry neither the importing file's test status nor a resolution
//! class, so this module derives the directory edges from the partitioned file→file import rows
//! ([`StorageConnection::file_imports_with_partition`]) × OWNS (the owning directory MODULE of
//! each file), per view:
//!
//! - each directory edge carries the [`PartitionCounts`] of all its imports and a type-only
//!   disposition aggregated over the contributors the view admits by the indexer's ONE
//!   conjunctive rule (`repo_graph_indexer::type_only::aggregate_module_edge_type_only`);
//! - the view's SCCs (the shared Tarjan) as [`CycleResult`]s, the same shape
//!   `find_cycles_cancellable` returns for the persisted graph;
//! - the EXCLUDED cycles: every SCC of a view whose flag set is a strict superset of the requested
//!   view's that is not an SCC of the requested view, deduplicated by member set, with the flag set
//!   of the first such view that has it (single additions before both) and the requested view's
//!   SCCs strictly inside it;
//! - the view's remainder and the count of directory imports it does not admit (the LiveGraph
//!   routes serve only when that count is 0 — D-TESB-06).
//!
//! In the certain-with-tests view the derived edges and dispositions equal the persisted
//! MODULE→MODULE edges and their `is_type_only` stamps (the indexer's `resolved_import_pairs`
//! admits static and dynamic imports and excludes inferred ones) — pinned by test.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use repo_graph_algorithms::{find_sccs_cancellable, CancelCheck, Cancelled, DirectedEdge};
use repo_graph_classification::import_partition::{
    ImportPartition, ImportRemainder, ImportView, PartitionCounts,
};
use repo_graph_indexer::storage_port::TypeOnlyDisposition;
use repo_graph_indexer::type_only::aggregate_module_edge_type_only;

use crate::connection::StorageConnection;
use crate::error::StorageError;
use crate::queries::{CycleNode, CycleResult};

/// Every import of one directory relation, with its partition and type-only disposition.
#[derive(Debug, Clone, Default)]
struct DirectoryRelation {
    partitions: PartitionCounts,
    contributors: Vec<(ImportPartition, Option<TypeOnlyDisposition>)>,
}

/// One cross-directory import's importer (for the importer UNDETERMINED block).
#[derive(Debug, Clone)]
struct CrossDirectoryImport {
    partition: ImportPartition,
    source_file_uid: String,
    source_path: Option<String>,
    source_is_test: Option<bool>,
}

/// A cycle that exists only through imports the requested view excludes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedCycle {
    /// The member directory modules' qualified names, sorted.
    pub members: Vec<String>,
    /// The member MODULE node uids, sorted by qualified name.
    pub member_ids: Vec<String>,
    /// The complete flag set (daemon flag names) of the first wider view whose graph has the cycle.
    pub flags: Vec<&'static str>,
    /// The requested view's cycles strictly inside this one (each a sorted member list).
    pub contains_shown: Vec<Vec<String>>,
    /// Every import between two members, in its partition cell.
    pub partitions: PartitionCounts,
}

impl ExcludedCycle {
    /// The number of member modules.
    pub fn length(&self) -> usize {
        self.members.len()
    }
}

/// A production-partition importer file of the view's cross-directory imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImporterFile {
    /// The importer's repo-relative path (its file uid when it has no `files` row).
    pub path: String,
    /// The stored test flag (`None` = no `files` row).
    pub is_test: Option<bool>,
}

/// The directory-module import graph of one snapshot, all partitions kept; views are taken from it.
#[derive(Debug, Clone)]
pub struct DirectoryModuleGraph {
    relations: BTreeMap<(String, String), DirectoryRelation>,
    imports: Vec<CrossDirectoryImport>,
    names: HashMap<String, String>,
    qualified: HashMap<String, String>,
}

impl StorageConnection {
    /// Read the directory-module import graph of `snapshot_uid` (all partitions).
    pub fn directory_module_graph(
        &self,
        snapshot_uid: &str,
    ) -> Result<DirectoryModuleGraph, StorageError> {
        let rows = self.file_imports_with_partition(snapshot_uid)?;
        let owners: HashMap<String, String> = self
            .get_file_ownership_from_owns_edges(snapshot_uid)?
            .into_iter()
            .map(|o| (o.file_uid, o.module_candidate_uid))
            .collect();
        let mut stmt = self.connection().prepare(
            "SELECT node_uid, name, COALESCE(qualified_name, name) \
             FROM nodes WHERE snapshot_uid = ? AND kind = 'MODULE'",
        )?;
        let mut names = HashMap::new();
        let mut qualified = HashMap::new();
        for row in stmt.query_map([snapshot_uid], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (uid, name, qual) = row?;
            names.insert(uid.clone(), name);
            qualified.insert(uid, qual);
        }
        let mut relations: BTreeMap<(String, String), DirectoryRelation> = BTreeMap::new();
        let mut imports = Vec::new();
        for r in rows {
            let (Some(src), Some(tgt)) = (
                owners.get(&r.source_file_uid),
                owners.get(&r.target_file_uid),
            ) else {
                continue;
            };
            if src == tgt {
                continue;
            }
            let rel = relations.entry((src.clone(), tgt.clone())).or_default();
            rel.partitions.add(r.partition);
            rel.contributors.push((r.partition, r.type_only));
            imports.push(CrossDirectoryImport {
                partition: r.partition,
                source_path: r.source_path,
                source_is_test: r.source_is_test,
                source_file_uid: r.source_file_uid,
            });
        }
        Ok(DirectoryModuleGraph {
            relations,
            imports,
            names,
            qualified,
        })
    }
}

impl DirectoryModuleGraph {
    /// The number of directory MODULE nodes of the snapshot.
    pub fn module_count(&self) -> usize {
        self.qualified.len()
    }

    /// MODULE node uid → qualified path (`COALESCE(qualified_name, name)`).
    pub fn qualified_names(&self) -> &HashMap<String, String> {
        &self.qualified
    }

    /// The view's directory edges `(source uid, target uid, type-only disposition)`: the relations
    /// with at least one admitted import; the disposition is the conjunctive aggregate over the
    /// admitted contributors. Sorted by (source, target).
    pub fn view_edges(
        &self,
        view: ImportView,
    ) -> Vec<(String, String, Option<TypeOnlyDisposition>)> {
        self.relations
            .iter()
            .filter_map(|((s, t), rel)| {
                let admitted: Vec<Option<TypeOnlyDisposition>> = rel
                    .contributors
                    .iter()
                    .filter(|(p, _)| view.admits(*p))
                    .map(|(_, d)| *d)
                    .collect();
                if admitted.is_empty() {
                    None
                } else {
                    Some((
                        s.clone(),
                        t.clone(),
                        aggregate_module_edge_type_only(&admitted),
                    ))
                }
            })
            .collect()
    }

    /// The SCCs (size > 1) of the view's graph as member-uid sets.
    fn scc_sets(
        &self,
        view: ImportView,
        cancel: CancelCheck,
    ) -> Result<Vec<BTreeSet<String>>, StorageError> {
        let edges: Vec<DirectedEdge> = self
            .view_edges(view)
            .into_iter()
            .map(|(s, t, _)| DirectedEdge {
                source: s,
                target: t,
            })
            .collect();
        let result =
            find_sccs_cancellable(&edges, cancel).map_err(|Cancelled| StorageError::Cancelled)?;
        Ok(result
            .cycles
            .iter()
            .map(|scc| scc.members.iter().map(|m| m.to_string()).collect())
            .collect())
    }

    /// The view's cycles in the shape `find_cycles_cancellable` returns (`cycle-N` ids in SCC
    /// order, members in traversal order, short names from the MODULE nodes).
    pub fn find_cycles(
        &self,
        view: ImportView,
        cancel: CancelCheck,
    ) -> Result<Vec<CycleResult>, StorageError> {
        let edges: Vec<DirectedEdge> = self
            .view_edges(view)
            .into_iter()
            .map(|(s, t, _)| DirectedEdge {
                source: s,
                target: t,
            })
            .collect();
        let result = find_sccs_cancellable(&edges, &mut *cancel)
            .map_err(|Cancelled| StorageError::Cancelled)?;
        let mut out = Vec::new();
        for (idx, scc) in result.cycles.iter().enumerate() {
            if cancel().is_break() {
                return Err(StorageError::Cancelled);
            }
            out.push(CycleResult {
                cycle_id: format!("cycle-{}", idx + 1),
                length: scc.size(),
                nodes: scc
                    .members
                    .iter()
                    .map(|uid| CycleNode {
                        node_id: uid.to_string(),
                        name: self
                            .names
                            .get(uid.as_str())
                            .cloned()
                            .unwrap_or_else(|| uid.to_string()),
                        file: None,
                    })
                    .collect(),
            });
        }
        out.sort_by(|a, b| a.cycle_id.cmp(&b.cycle_id));
        Ok(out)
    }

    fn qualified_of(&self, uid: &str) -> String {
        self.qualified
            .get(uid)
            .cloned()
            .unwrap_or_else(|| uid.to_string())
    }

    fn sorted_qualified(&self, members: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
        let mut pairs: Vec<(String, String)> = members
            .iter()
            .map(|m| (self.qualified_of(m), m.clone()))
            .collect();
        pairs.sort();
        pairs.into_iter().unzip()
    }

    /// Every import between two of `members` (directory MODULE uids), in its partition cell,
    /// admitted by the view or not. `None` when a member is not a directory MODULE of this graph
    /// — the counts of the members found would be a subset's, so they are absent, never partial
    /// and never zero (D-TESB-17 row U4).
    pub fn member_partitions<'a, I>(&self, members: I) -> Option<PartitionCounts>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let set: BTreeSet<&str> = members.into_iter().collect();
        if set.iter().any(|m| !self.qualified.contains_key(*m)) {
            return None;
        }
        let mut out = PartitionCounts::default();
        for ((s, t), rel) in &self.relations {
            if set.contains(s.as_str()) && set.contains(t.as_str()) {
                out.merge(&rel.partitions);
            }
        }
        Some(out)
    }

    /// The cycles that exist only through imports `view` excludes (see the module doc), sorted
    /// by member list.
    pub fn excluded_cycles(
        &self,
        view: ImportView,
        cancel: CancelCheck,
    ) -> Result<Vec<ExcludedCycle>, StorageError> {
        let wider = view.wider_views();
        if wider.is_empty() {
            return Ok(Vec::new());
        }
        let shown = self.scc_sets(view, &mut *cancel)?;
        let mut seen: BTreeSet<BTreeSet<String>> = BTreeSet::new();
        let mut out = Vec::new();
        for w in wider {
            for scc in self.scc_sets(w, &mut *cancel)? {
                if shown.contains(&scc) || !seen.insert(scc.clone()) {
                    continue;
                }
                let (members, member_ids) = self.sorted_qualified(&scc);
                let mut contains_shown: Vec<Vec<String>> = shown
                    .iter()
                    .filter(|s| s.len() < scc.len() && s.is_subset(&scc))
                    .map(|s| self.sorted_qualified(s).0)
                    .collect();
                contains_shown.sort();
                let partitions = self
                    .member_partitions(scc.iter().map(String::as_str))
                    .ok_or_else(|| {
                        StorageError::SerializationError(format!(
                            "excluded_cycles: a member of the cycle {members:?} is not a \
                             directory module of the graph — its partition counts are unknown"
                        ))
                    })?;
                out.push(ExcludedCycle {
                    partitions,
                    members,
                    member_ids,
                    flags: w.flag_names(),
                    contains_shown,
                });
            }
        }
        out.sort_by(|a, b| a.members.cmp(&b.members));
        Ok(out)
    }

    /// The view's excluded remainder over the directory relations (imports per group; relations
    /// that exist only through each group).
    pub fn remainder(&self, view: ImportView) -> ImportRemainder {
        ImportRemainder::from_relations(view, self.relations.values().map(|r| &r.partitions))
    }

    /// The cross-directory imports the view does not admit (0 ⇔ the view excludes nothing on the
    /// directory population).
    pub fn excluded_import_count(&self, view: ImportView) -> u64 {
        self.relations
            .values()
            .map(|r| r.partitions.total() - r.partitions.admitted(view))
            .sum()
    }

    /// The distinct production-partition importer files of the cross-directory imports the view
    /// admits, sorted by path (the universe of the importer UNDETERMINED block).
    pub fn production_importers(&self, view: ImportView) -> Vec<ImporterFile> {
        let mut by_file: BTreeMap<&str, ImporterFile> = BTreeMap::new();
        for i in &self.imports {
            if !view.admits(i.partition) || !i.partition.status.is_production_partition() {
                continue;
            }
            by_file
                .entry(i.source_file_uid.as_str())
                .or_insert_with(|| ImporterFile {
                    path: i
                        .source_path
                        .clone()
                        .unwrap_or_else(|| i.source_file_uid.clone()),
                    is_test: i.source_is_test,
                });
        }
        let mut out: Vec<ImporterFile> = by_file.into_values().collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crud::test_helpers::{fresh_storage, make_edge, make_file, make_node};
    use crate::import_partition_reads::tests::setup_test_snapshot;
    use crate::types::{GraphEdge, GraphNode};
    use repo_graph_indexer::storage_port::EdgeStorePort;

    /// A fixture of directory modules, their files, and file→file imports.
    struct Fx {
        conn: StorageConnection,
        repo: String,
        snap: String,
        n: usize,
    }

    impl Fx {
        fn new() -> Fx {
            let conn = fresh_storage();
            let (repo, snap) = setup_test_snapshot(&conn);
            Fx {
                conn,
                repo,
                snap,
                n: 0,
            }
        }

        fn module(&mut self, dir: &str) {
            let node = GraphNode {
                node_uid: format!("m:{dir}"),
                snapshot_uid: self.snap.clone(),
                repo_uid: self.repo.clone(),
                stable_key: format!("{}:{dir}:MODULE", self.repo),
                kind: "MODULE".to_string(),
                subtype: None,
                name: dir.rsplit('/').next().unwrap_or(dir).to_string(),
                qualified_name: Some(dir.to_string()),
                file_uid: None,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            };
            self.conn.insert_nodes(&[node]).unwrap();
        }

        /// A file under `dir` (OWNS from `m:{dir}`), test when `is_test`.
        fn file(&mut self, dir: &str, name: &str, is_test: bool) {
            let path = format!("{dir}/{name}");
            let mut f = make_file(&self.repo, &path);
            f.is_test = is_test;
            self.conn.upsert_files(&[f.clone()]).unwrap();
            let mut node = make_node(
                &format!("f:{path}"),
                &self.snap,
                &self.repo,
                &format!("{path}:FILE"),
                &f.file_uid,
                name,
            );
            node.kind = "FILE".to_string();
            node.subtype = None;
            self.conn.insert_nodes(&[node]).unwrap();
            let mut owns = make_edge(
                &format!("owns:{path}"),
                &self.snap,
                &self.repo,
                &format!("m:{dir}"),
                &format!("f:{path}"),
            );
            owns.edge_type = "OWNS".to_string();
            self.conn.insert_edges(&[owns]).unwrap();
        }

        fn import(&mut self, from: &str, to: &str, resolution: &str, metadata: Option<&str>) {
            self.n += 1;
            let mut e: GraphEdge = make_edge(
                &format!("i{}", self.n),
                &self.snap,
                &self.repo,
                &format!("f:{from}"),
                &format!("f:{to}"),
            );
            e.edge_type = "IMPORTS".to_string();
            e.resolution = resolution.to_string();
            e.metadata_json = metadata.map(str::to_string);
            self.conn.insert_edges(&[e]).unwrap();
        }

        /// A persisted MODULE→MODULE IMPORTS edge (the indexer's shape) with its stamp.
        fn persisted(&mut self, from: &str, to: &str, stamp: Option<TypeOnlyDisposition>) {
            self.n += 1;
            let uid = format!("p{}", self.n);
            let mut e = make_edge(
                &uid,
                &self.snap,
                &self.repo,
                &format!("m:{from}"),
                &format!("m:{to}"),
            );
            e.edge_type = "IMPORTS".to_string();
            self.conn.insert_edges(&[e]).unwrap();
            if let Some(d) = stamp {
                self.conn.set_edge_type_only(&[(uid, d)]).unwrap();
            }
        }

        fn graph(&self) -> DirectoryModuleGraph {
            self.conn.directory_module_graph(&self.snap).unwrap()
        }
    }

    fn never() -> impl FnMut() -> std::ops::ControlFlow<()> {
        || std::ops::ControlFlow::Continue(())
    }

    fn cycle_sets(g: &DirectoryModuleGraph, view: ImportView) -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = g
            .find_cycles(view, &mut never())
            .unwrap()
            .iter()
            .map(|c| {
                let mut m: Vec<String> =
                    c.nodes.iter().map(|n| g.qualified_of(&n.node_id)).collect();
                m.sort();
                m
            })
            .collect();
        out.sort();
        out
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// F-TESB-PARITY: static, dynamic, and a pair with one type-only and one runtime contributor.
    #[test]
    fn directory_module_edges_all_certain_with_tests_equal_the_persisted_module_edges() {
        let mut fx = Fx::new();
        for d in ["a", "b", "c"] {
            fx.module(d);
        }
        fx.file("a", "x.ts", false);
        fx.file("a", "y.test.ts", true);
        fx.file("b", "x.ts", false);
        fx.file("c", "x.ts", false);
        fx.import("a/x.ts", "b/x.ts", "static", Some(r#"{"isTypeOnly":true}"#));
        fx.import(
            "a/y.test.ts",
            "b/x.ts",
            "static",
            Some(r#"{"isTypeOnly":false}"#),
        );
        fx.import(
            "b/x.ts",
            "c/x.ts",
            "dynamic",
            Some(r#"{"isTypeOnly":false}"#),
        );
        fx.import(
            "c/x.ts",
            "a/x.ts",
            "inferred",
            Some(r#"{"isTypeOnly":false}"#),
        );
        // What the indexer persists for these facts (inferred excluded).
        fx.persisted("a", "b", Some(TypeOnlyDisposition::Runtime));
        fx.persisted("b", "c", Some(TypeOnlyDisposition::Runtime));
        let g = fx.graph();
        let derived = g.view_edges(ImportView::CERTAIN_WITH_TESTS);
        let mut stmt = fx
            .conn
            .connection()
            .prepare(
                "SELECT e.source_node_uid, e.target_node_uid, e.is_type_only FROM edges e \
                 JOIN nodes s ON s.node_uid = e.source_node_uid \
                 JOIN nodes t ON t.node_uid = e.target_node_uid \
                 WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS' \
                   AND s.kind = 'MODULE' AND t.kind = 'MODULE'",
            )
            .unwrap();
        let mut persisted: Vec<(String, String, Option<TypeOnlyDisposition>)> = stmt
            .query_map([fx.snap.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    TypeOnlyDisposition::from_column_code(row.get::<_, Option<i64>>(2)?),
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        persisted.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        assert_eq!(derived, persisted);
    }

    #[test]
    fn directory_module_edges_type_only_follows_the_admitted_contributors() {
        let mut fx = Fx::new();
        for d in ["a", "b"] {
            fx.module(d);
        }
        fx.file("a", "x.ts", false);
        fx.file("a", "y.test.ts", true);
        fx.file("b", "x.ts", false);
        fx.import("a/x.ts", "b/x.ts", "static", Some(r#"{"isTypeOnly":true}"#));
        fx.import(
            "a/y.test.ts",
            "b/x.ts",
            "static",
            Some(r#"{"isTypeOnly":false}"#),
        );
        let g = fx.graph();
        // Default: only the production type-only import is admitted ⇒ type-only.
        assert_eq!(
            g.view_edges(ImportView::DEFAULT),
            vec![(
                "m:a".into(),
                "m:b".into(),
                Some(TypeOnlyDisposition::TypeOnly)
            )]
        );
        // With tests the runtime test import dominates.
        assert_eq!(
            g.view_edges(ImportView::CERTAIN_WITH_TESTS),
            vec![(
                "m:a".into(),
                "m:b".into(),
                Some(TypeOnlyDisposition::Runtime)
            )]
        );
    }

    /// Production A→B, test B→A (a cycle only through a test import).
    fn test_closed(fx: &mut Fx) {
        for d in ["a", "b"] {
            fx.module(d);
        }
        fx.file("a", "x.cc", false);
        fx.file("b", "x.cc", false);
        fx.file("b", "x_test.cc", true);
        fx.import("a/x.cc", "b/x.cc", "static", None);
        fx.import("b/x_test.cc", "a/x.cc", "static", None);
    }

    #[test]
    fn cycles_in_default_view_name_a_cycle_closed_only_by_a_test_import() {
        let mut fx = Fx::new();
        test_closed(&mut fx);
        let g = fx.graph();
        assert!(cycle_sets(&g, ImportView::DEFAULT).is_empty());
        assert_eq!(
            cycle_sets(&g, ImportView::CERTAIN_WITH_TESTS),
            vec![names(&["a", "b"])]
        );
        let ex = g
            .excluded_cycles(ImportView::DEFAULT, &mut never())
            .unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0].members, names(&["a", "b"]));
        assert_eq!(ex[0].flags, vec!["include_tests"]);
        assert!(ex[0].contains_shown.is_empty());
        assert_eq!(ex[0].length(), 2);
        assert_eq!(ex[0].partitions.production_certain, 1);
        assert_eq!(ex[0].partitions.test_certain, 1);
        let rem = g.remainder(ImportView::DEFAULT);
        assert_eq!((rem.tests.imports, rem.tests.edges), (1, 1));
    }

    #[test]
    fn cycles_in_default_view_name_a_cycle_closed_only_by_an_inferred_import() {
        let mut fx = Fx::new();
        for d in ["a", "b"] {
            fx.module(d);
        }
        fx.file("a", "x.py", false);
        fx.file("b", "x.py", false);
        fx.import("a/x.py", "b/x.py", "static", None);
        fx.import("b/x.py", "a/x.py", "inferred", None);
        let g = fx.graph();
        assert!(cycle_sets(&g, ImportView::DEFAULT).is_empty());
        assert!(cycle_sets(&g, ImportView::CERTAIN_WITH_TESTS).is_empty());
        let ex = g
            .excluded_cycles(ImportView::DEFAULT, &mut never())
            .unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0].flags, vec!["include_inferred"]);
        assert_eq!(ex[0].partitions.production_inferred, 1);
        assert_eq!(
            cycle_sets(&g, ImportView::WITH_INFERRED),
            vec![names(&["a", "b"])]
        );
    }

    #[test]
    fn cycles_in_the_widest_view_equal_the_all_partition_graph() {
        let mut fx = Fx::new();
        for d in ["a", "b", "c"] {
            fx.module(d);
        }
        fx.file("a", "x.py", false);
        fx.file("b", "x.py", false);
        fx.file("c", "t_test.py", true);
        fx.import("a/x.py", "b/x.py", "static", None);
        fx.import("b/x.py", "c/t_test.py", "inferred", None);
        fx.import("c/t_test.py", "a/x.py", "inferred", None);
        let g = fx.graph();
        assert_eq!(
            cycle_sets(&g, ImportView::ALL),
            vec![names(&["a", "b", "c"])]
        );
        assert!(g
            .excluded_cycles(ImportView::ALL, &mut never())
            .unwrap()
            .is_empty());
        assert_eq!(g.excluded_import_count(ImportView::ALL), 0);
        assert!(g.remainder(ImportView::ALL).is_empty());
        let ex = g
            .excluded_cycles(ImportView::DEFAULT, &mut never())
            .unwrap();
        assert_eq!(ex[0].flags, vec!["include_tests", "include_inferred"]);
    }

    #[test]
    fn excluded_directory_imports_count_zero_only_when_the_view_excludes_nothing() {
        let mut fx = Fx::new();
        for d in ["a", "b"] {
            fx.module(d);
        }
        fx.file("a", "x.ts", false);
        fx.file("b", "x.ts", false);
        fx.import("a/x.ts", "b/x.ts", "static", None);
        let g = fx.graph();
        assert_eq!(g.excluded_import_count(ImportView::DEFAULT), 0);
        let mut fx2 = Fx::new();
        test_closed(&mut fx2);
        let g2 = fx2.graph();
        assert_eq!(g2.excluded_import_count(ImportView::DEFAULT), 1);
        assert_eq!(g2.excluded_import_count(ImportView::CERTAIN_WITH_TESTS), 0);
    }

    /// F-TESB-CYCLE: two directories owned by ONE module candidate; a production import one way
    /// and a test import back. The candidate graph excludes nothing cross-module; the directory
    /// population counts the test import.
    #[test]
    fn excluded_directory_imports_count_an_intra_candidate_cross_directory_test_import() {
        let mut fx = Fx::new();
        for d in ["lib/a", "lib/b"] {
            fx.module(d);
        }
        fx.file("lib/a", "x.rs", false);
        fx.file("lib/b", "x.rs", false);
        fx.file("lib/b", "tests.rs", true);
        fx.import("lib/a/x.rs", "lib/b/x.rs", "static", None);
        fx.import("lib/b/tests.rs", "lib/a/x.rs", "static", None);
        // One module candidate owning both directories: nothing crosses a candidate boundary.
        fx.conn
            .connection()
            .execute(
                "INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, \
                 module_key, module_kind, canonical_root_path, confidence, display_name, \
                 metadata_json) VALUES ('mc', ?, ?, 'cargo:lib', 'cargo_crate', 'lib', 1.0, NULL, NULL)",
                rusqlite::params![fx.snap, fx.repo],
            )
            .unwrap();
        let g = fx.graph();
        assert_eq!(g.excluded_import_count(ImportView::DEFAULT), 1);
        let ex = g
            .excluded_cycles(ImportView::DEFAULT, &mut never())
            .unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0].members, names(&["lib/a", "lib/b"]));
    }

    /// review-0's counterexample: production A↔B with test-only B↔C — the view shows {A,B}; the
    /// remainder names {A,B,C} under `include_tests` containing {A,B}.
    #[test]
    fn cycles_in_default_view_name_a_larger_cycle_that_grows_a_shown_one_through_test_imports() {
        let mut fx = Fx::new();
        for d in ["a", "b", "c"] {
            fx.module(d);
        }
        fx.file("a", "x.cc", false);
        fx.file("b", "x.cc", false);
        fx.file("b", "x_test.cc", true);
        fx.file("c", "x_test.cc", true);
        fx.import("a/x.cc", "b/x.cc", "static", None);
        fx.import("b/x.cc", "a/x.cc", "static", None);
        fx.import("b/x_test.cc", "c/x_test.cc", "static", None);
        fx.import("c/x_test.cc", "b/x.cc", "static", None);
        let g = fx.graph();
        assert_eq!(
            cycle_sets(&g, ImportView::DEFAULT),
            vec![names(&["a", "b"])]
        );
        let ex = g
            .excluded_cycles(ImportView::DEFAULT, &mut never())
            .unwrap();
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0].members, names(&["a", "b", "c"]));
        assert_eq!(ex[0].flags, vec!["include_tests"]);
        assert_eq!(ex[0].contains_shown, vec![names(&["a", "b"])]);
    }

    #[test]
    fn cycle_partitions_count_every_member_to_member_import_in_all_four_cells() {
        let mut fx = Fx::new();
        for d in ["a", "b"] {
            fx.module(d);
        }
        fx.file("a", "x.py", false);
        fx.file("a", "t_test.py", true);
        fx.file("b", "x.py", false);
        fx.file("b", "t_test.py", true);
        fx.import("a/x.py", "b/x.py", "static", None);
        fx.import("b/x.py", "a/x.py", "static", None);
        fx.import("a/t_test.py", "b/x.py", "static", None);
        fx.import("b/x.py", "a/x.py", "inferred", None);
        fx.import("b/t_test.py", "a/x.py", "inferred", None);
        let g = fx.graph();
        let cycles = g.find_cycles(ImportView::DEFAULT, &mut never()).unwrap();
        assert_eq!(cycles.len(), 1);
        let p = g.member_partitions(cycles[0].nodes.iter().map(|n| n.node_id.as_str()));
        assert_eq!(
            p,
            Some(PartitionCounts {
                production_certain: 2,
                test_certain: 1,
                production_inferred: 1,
                test_inferred: 1,
                unknown_test_status: 0,
            })
        );
    }

    /// D-TESB-17 row U4: a member that is not a directory MODULE of the graph makes the counts
    /// absent — never the counts of the members found (a subset's), never zero.
    #[test]
    fn member_partitions_of_a_member_outside_the_graph_is_absent_never_zero() {
        let mut fx = Fx::new();
        for d in ["a", "b"] {
            fx.module(d);
        }
        fx.file("a", "x.py", false);
        fx.file("b", "x.py", false);
        fx.import("a/x.py", "b/x.py", "static", None);
        fx.import("b/x.py", "a/x.py", "static", None);
        let g = fx.graph();
        let cycles = g.find_cycles(ImportView::DEFAULT, &mut never()).unwrap();
        let mut ids: Vec<&str> = cycles[0].nodes.iter().map(|n| n.node_id.as_str()).collect();
        assert_eq!(
            g.member_partitions(ids.iter().copied())
                .map(|p| p.production_certain),
            Some(2)
        );
        ids.push("not-a-directory-module");
        assert_eq!(g.member_partitions(ids.iter().copied()), None);
        assert_eq!(g.member_partitions(["not-a-directory-module"]), None);
    }

    /// review-1's counterexample, pinned in all four requested views: production-inferred A↔B
    /// (one direction certain, one inferred) and test-certain B↔C.
    #[test]
    fn excluded_cycles_are_named_relative_to_the_requested_view_in_all_four_views_with_an_scc_merge(
    ) {
        let mut fx = Fx::new();
        for d in ["a", "b", "c"] {
            fx.module(d);
        }
        fx.file("a", "x.py", false);
        fx.file("b", "x.py", false);
        fx.file("b", "t_test.py", true);
        fx.file("c", "t_test.py", true);
        fx.import("a/x.py", "b/x.py", "static", None);
        fx.import("b/x.py", "a/x.py", "inferred", None);
        fx.import("b/t_test.py", "c/t_test.py", "static", None);
        fx.import("c/t_test.py", "b/x.py", "static", None);
        let g = fx.graph();
        type ExcludedKey = (Vec<String>, Vec<&'static str>, Vec<Vec<String>>);
        let ek = |view: ImportView| -> Vec<ExcludedKey> {
            g.excluded_cycles(view, &mut never())
                .unwrap()
                .into_iter()
                .map(|e| (e.members, e.flags, e.contains_shown))
                .collect()
        };
        // Default: {A,B} under inferred, {B,C} under tests, the merged {A,B,C} under both.
        assert_eq!(
            ek(ImportView::DEFAULT),
            vec![
                (names(&["a", "b"]), vec!["include_inferred"], vec![]),
                (
                    names(&["a", "b", "c"]),
                    vec!["include_tests", "include_inferred"],
                    vec![]
                ),
                (names(&["b", "c"]), vec!["include_tests"], vec![]),
            ]
        );
        // --include-tests shows {B,C}; names only {A,B,C} (both) containing it — never {A,B}.
        assert_eq!(
            cycle_sets(&g, ImportView::CERTAIN_WITH_TESTS),
            vec![names(&["b", "c"])]
        );
        assert_eq!(
            ek(ImportView::CERTAIN_WITH_TESTS),
            vec![(
                names(&["a", "b", "c"]),
                vec!["include_tests", "include_inferred"],
                vec![names(&["b", "c"])]
            )]
        );
        // --include-inferred shows {A,B}; names only {A,B,C} containing it.
        assert_eq!(
            cycle_sets(&g, ImportView::WITH_INFERRED),
            vec![names(&["a", "b"])]
        );
        assert_eq!(
            ek(ImportView::WITH_INFERRED),
            vec![(
                names(&["a", "b", "c"]),
                vec!["include_tests", "include_inferred"],
                vec![names(&["a", "b"])]
            )]
        );
        // Both: none.
        assert!(ek(ImportView::ALL).is_empty());
    }

    #[test]
    fn production_importers_are_the_admitted_cross_directory_production_files() {
        let mut fx = Fx::new();
        test_closed(&mut fx);
        let g = fx.graph();
        let rows = g.production_importers(ImportView::DEFAULT);
        assert_eq!(
            rows,
            vec![ImporterFile {
                path: "a/x.cc".into(),
                is_test: Some(false)
            }]
        );
        // A test importer never enters the production universe, even with tests included.
        assert_eq!(
            g.production_importers(ImportView::CERTAIN_WITH_TESTS).len(),
            1
        );
    }
}
