//! TEST-EDGE-SCOPE-1B (RG-REQ-004-L12, RG-REQ-002-L11; D-TESB-05/06/08/09): the daemon's import
//! view — the strict `include_tests` / `include_inferred` request parameters, the additive JSON of a
//! partitioned answer (`import_view`, `import_remainder`, per-edge/per-cycle `partitions`,
//! `excluded_cycles`, the importer UNDETERMINED block), the partitioned SQLite `cycles` body over the
//! query-time directory graph, and the LiveGraph cycles eligibility (the view excludes nothing on the
//! directory population).
//!
//! The partition POLICY is `repo_graph_classification::import_partition`; the derivations are in
//! storage and classification. This module only reads parameters and shapes JSON.

use repo_graph_agent::TestStatusUniverse;
use repo_graph_classification::import_partition::{
    ImportRemainder, ImportView, PartitionCounts, FLAG_INCLUDE_INFERRED, FLAG_INCLUDE_TESTS,
};
use repo_graph_daemon_transport::{ErrorCode, ErrorDetail};
use repo_graph_storage::directory_module_edges::{DirectoryModuleGraph, ExcludedCycle};
use repo_graph_storage::error::StorageError;
use repo_graph_storage::StorageConnection;
use serde_json::{json, Value};

/// Read the strict view parameters: each of `include_tests` / `include_inferred` is absent
/// (`false`) or a boolean; a present non-boolean is refused with a named invalid-request error —
/// never read as `false` (RG-REQ-002-L04).
pub(crate) fn read_import_view(params: &Value) -> Result<ImportView, ErrorDetail> {
    let flag = |name: &str| -> Result<bool, ErrorDetail> {
        match params.get(name) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(other) => Err(ErrorDetail::new(
                ErrorCode::InvalidRequest,
                format!("invalid '{name}' parameter: expected a boolean, got {other}"),
            )),
        }
    };
    Ok(ImportView {
        include_tests: flag(FLAG_INCLUDE_TESTS)?,
        include_inferred: flag(FLAG_INCLUDE_INFERRED)?,
    })
}

/// `import_view`: `{"include_tests": b, "include_inferred": b}`.
pub(crate) fn view_json(view: ImportView) -> Value {
    json!({
        "include_tests": view.include_tests,
        "include_inferred": view.include_inferred,
    })
}

/// `import_remainder`: every group present, `{"imports": n, "edges": e}` each.
pub(crate) fn remainder_json(r: &ImportRemainder) -> Value {
    json!({
        "tests": {"imports": r.tests.imports, "edges": r.tests.edges},
        "inferred": {"imports": r.inferred.imports, "edges": r.inferred.edges},
        "tests_and_inferred": {
            "imports": r.tests_and_inferred.imports,
            "edges": r.tests_and_inferred.edges,
        },
    })
}

/// `partitions`: the four cells and the unknown-test-status tally.
pub(crate) fn partitions_json(p: &PartitionCounts) -> Value {
    json!({
        "production_certain": p.production_certain,
        "test_certain": p.test_certain,
        "production_inferred": p.production_inferred,
        "test_inferred": p.test_inferred,
        "unknown_test_status": p.unknown_test_status,
    })
}

/// `excluded_cycles`: members (qualified, sorted), length, the complete flag set that shows the
/// cycle, the shown cycles it contains, and its partition counts.
pub(crate) fn excluded_cycles_json(cycles: &[ExcludedCycle]) -> Value {
    Value::Array(
        cycles
            .iter()
            .map(|c| {
                json!({
                    "members": c.members,
                    "length": c.length(),
                    "flags": c.flags,
                    "contains_shown": c.contains_shown,
                    "partitions": partitions_json(&c.partitions),
                })
            })
            .collect(),
    )
}

/// The importer UNDETERMINED block (1A's one function) over production importer rows.
pub(crate) fn importer_block<'a, I>(universe: TestStatusUniverse, rows: I) -> Value
where
    I: IntoIterator<Item = (&'a str, Option<bool>)>,
{
    crate::test_status_undetermined::to_json(&crate::test_status_undetermined::block(
        universe, rows,
    ))
}

/// D-TESB-06 / F-TESB-CYCLE: the LiveGraph cycle routes may serve only when the requested view
/// excludes no import of the DIRECTORY-module population `cycles` runs over (never the
/// module-candidate population).
pub(crate) fn livegraph_cycles_eligible(graph: &DirectoryModuleGraph, view: ImportView) -> bool {
    graph.excluded_import_count(view) == 0
}

/// Attach the partition keys to a `cycles` answer over `graph`: `import_view`, `import_remainder`,
/// `excluded_cycles`, `importer_test_status_undetermined` (cross-directory importers) and each
/// cycle's `partitions` (every member-to-member import in its cell). A cycle node is matched to its
/// directory MODULE by `node_id` (a node uid on the SQLite route) or by qualified path (the
/// LiveGraph route).
pub(crate) fn attach_cycle_partition(
    value: &mut Value,
    graph: &DirectoryModuleGraph,
    view: ImportView,
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<(), StorageError> {
    let excluded = graph.excluded_cycles(view, cancel)?;
    let by_qualified: std::collections::HashMap<&str, &str> = graph
        .qualified_names()
        .iter()
        .map(|(uid, q)| (q.as_str(), uid.as_str()))
        .collect();
    if let Some(cycles) = value.get_mut("cycles").and_then(Value::as_array_mut) {
        for cycle in cycles {
            // D-TESB-17 rows U6/W7: every member must resolve to a directory MODULE of the graph;
            // one that does not (or has no string `node_id`) makes the counts unknown — `null`
            // beside a reason naming the member, never a subset's counts, never zeros.
            let resolved = cycle_member_uids(cycle, graph, &by_qualified).and_then(|uids| {
                graph
                    .member_partitions(uids.iter().map(String::as_str))
                    .ok_or_else(|| {
                        "partition counts unknown: a member is not a directory module of the \
                         partitioned graph"
                            .to_string()
                    })
            });
            match resolved {
                Ok(parts) => cycle["partitions"] = partitions_json(&parts),
                Err(reason) => {
                    cycle["partitions"] = Value::Null;
                    cycle["partitions_unavailable"] = json!(reason);
                }
            }
        }
    }
    let importers = graph.production_importers(view);
    let obj = value
        .as_object_mut()
        .expect("a cycles answer is a JSON object");
    obj.insert("import_view".into(), view_json(view));
    obj.insert(
        "import_remainder".into(),
        remainder_json(&graph.remainder(view)),
    );
    obj.insert("excluded_cycles".into(), excluded_cycles_json(&excluded));
    obj.insert(
        "importer_test_status_undetermined".into(),
        importer_block(
            TestStatusUniverse::CrossDirectoryImporters,
            importers.iter().map(|i| (i.path.as_str(), i.is_test)),
        ),
    );
    Ok(())
}

/// The directory MODULE uids of one `cycles` item's members, matched by `node_id` (a node uid on
/// the SQLite route, a qualified path on the LiveGraph route). `Err` names the first member that
/// has no string `node_id` or matches no directory MODULE of `graph` (D-TESB-17 rows U6, W7).
fn cycle_member_uids(
    cycle: &Value,
    graph: &DirectoryModuleGraph,
    by_qualified: &std::collections::HashMap<&str, &str>,
) -> Result<Vec<String>, String> {
    let Some(nodes) = cycle.get("nodes").and_then(Value::as_array) else {
        return Err("partition counts unknown: the cycle lists no member nodes".to_string());
    };
    nodes
        .iter()
        .map(|n| {
            let Some(id) = n.get("node_id").and_then(Value::as_str) else {
                return Err(format!(
                    "partition counts unknown: a member has no string node_id ({})",
                    n.get("node_id").unwrap_or(&Value::Null)
                ));
            };
            if graph.qualified_names().contains_key(id) {
                Ok(id.to_string())
            } else if let Some(u) = by_qualified.get(id) {
                Ok(u.to_string())
            } else {
                Err(format!(
                    "partition counts unknown: the member {id} matches no directory module of \
                     the partitioned graph"
                ))
            }
        })
        .collect()
}

/// The partitioned SQLite `cycles` body: the SCCs of the view's directory graph in the canonical
/// output (CYCLES-OUTPUT-CONTRACT-1), with the REAL intra-SCC edges of the view (CYCLE-HONESTY-1),
/// the test-only classification and the per-cycle type-only verdict over the view's edges — the
/// same steps the persisted route took, over the view's graph. Returns the fields every SQLite
/// route shares: `cycles`, `count`, `module_count`, `module_edge_count`, `ts_type_only_caveat` and
/// the partition keys.
pub(crate) fn sqlite_cycles_body(
    conn: &StorageConnection,
    repo_uid: &str,
    graph: &DirectoryModuleGraph,
    view: ImportView,
    cancel: &mut dyn FnMut() -> std::ops::ControlFlow<()>,
) -> Result<serde_json::Map<String, Value>, StorageError> {
    let sqlite_cycles = graph.find_cycles(view, &mut *cancel)?;
    let qualified = graph.qualified_names();
    let module_edges = graph.view_edges(view);
    let mut cycles = crate::cycle_output::sqlite_module_cycles_json_with_edges(
        &sqlite_cycles,
        qualified,
        &module_edges,
    );
    // FIXTURE-POLLUTION-1 §2.2/§2.3: classify test-only cycles from the stored `is_test` fact
    // (CLASSIFIED read — a genuine error propagates).
    let tracked = conn.get_files_by_repo(repo_uid)?;
    let files: Vec<(&str, bool)> = tracked
        .iter()
        .map(|f| (f.path.as_str(), f.is_test))
        .collect();
    crate::cycle_output::label_test_only_cycles(&mut cycles, &files);
    // TYPE-ONLY-IMPORTS-1: the per-cycle verdict over the view's edges (the disposition aggregated
    // over the admitted contributors).
    let all_module_dirs: Vec<String> = qualified.values().cloned().collect();
    let files_by_lang: Vec<(&str, Option<&str>)> = tracked
        .iter()
        .map(|f| (f.path.as_str(), f.language.as_deref()))
        .collect();
    crate::cycle_output::attach_type_only_labels(
        &mut cycles,
        &module_edges,
        &files_by_lang,
        &all_module_dirs,
    );
    let count = cycles.len();
    let mut value = json!({
        "cycles": cycles,
        "count": count,
        // The blanket caveat is RETIRED on the SQLite route (per-cycle `type_only`).
        "ts_type_only_caveat": false,
        // IMPORT-RESOLUTION-RUST-1 §2.5: the module count and the view's directory edge count.
        "module_count": graph.module_count(),
        "module_edge_count": module_edges.len(),
    });
    attach_cycle_partition(&mut value, graph, view, cancel)?;
    match value {
        Value::Object(map) => Ok(map),
        _ => unreachable!("json! object"),
    }
}

/// The additive JSON of a module-candidate answer (`modules list`, `modules deps`, orient's top
/// edges): `import_view`, `import_remainder`, and the importer UNDETERMINED block over the
/// production importers of `edges`.
pub(crate) fn module_edge_partition_fields(
    facts: &repo_graph_module_queries::ModuleGraphFacts,
    edges: &[&repo_graph_classification::module_edges::ModuleDependencyEdge],
) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    out.insert("import_view".into(), view_json(facts.view));
    out.insert("import_remainder".into(), remainder_json(&facts.remainder));
    // D-TESB-17 row U3: missing importer evidence is `null` beside a reason — unreadable
    // downstream — never a block over a shorter universe.
    match facts.importer_rows(edges.iter().copied()) {
        Ok(rows) => {
            out.insert(
                "importer_test_status_undetermined".into(),
                importer_block(
                    TestStatusUniverse::CrossModuleImporters,
                    rows.iter().map(|(p, f)| (p.as_str(), *f)),
                ),
            );
        }
        Err(e) => {
            out.insert("importer_test_status_undetermined".into(), Value::Null);
            out.insert(
                "importer_test_status_undetermined_unavailable".into(),
                json!(e.to_string()),
            );
        }
    }
    out
}

/// D-TESB-17 row U6: one module edge's `partitions` — the four counts, or `null` beside
/// `partitions_unavailable` naming the edge when the facts hold no counts for it. Never zeros,
/// never the key omitted.
pub(crate) fn edge_partition_fields(
    facts: &repo_graph_module_queries::ModuleGraphFacts,
    edge: &repo_graph_classification::module_edges::ModuleDependencyEdge,
) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    match facts.partitions_of(edge) {
        Some(p) => {
            out.insert("partitions".into(), partitions_json(&p));
        }
        None => {
            out.insert("partitions".into(), Value::Null);
            out.insert(
                "partitions_unavailable".into(),
                json!(format!(
                    "partition counts unknown: the module edge {} -> {} is not an edge of the \
                     partitioned facts",
                    edge.source_canonical_path, edge.target_canonical_path
                )),
            );
        }
    }
    out
}

/// RG-REQ-002-L11 (D-TESB-05): partition an `imports <file>` answer's rows by their resolution
/// class — the view keeps CERTAIN rows and, with `include_inferred`, the INFERRED ones; the rows it
/// leaves out are counted in `import_remainder.inferred` (never listed as certain, never dropped
/// silently). `count` follows the kept rows; `import_view` states the view. A row without a
/// `resolution` key (a LiveGraph row — the LiveGraph answers only under a GREEN import certificate,
/// i.e. when every import is static) is kept. A `resolution` that is present but not a string, or a
/// string outside the vocabulary, is refused with a named error — never kept as certain
/// (D-TESB-17 rows U8, W8; D-PSI-R1-VOCAB).
pub(crate) fn partition_import_rows(
    value: &mut Value,
    view: ImportView,
) -> Result<(), ErrorDetail> {
    use repo_graph_classification::import_partition::ImportClass;
    let Some(obj) = value.as_object_mut() else {
        return Ok(());
    };
    let mut excluded = 0u64;
    if let Some(Value::Array(rows)) = obj.get_mut("imports") {
        let mut kept = Vec::with_capacity(rows.len());
        for row in rows.drain(..) {
            let class = match row.get("resolution") {
                None => ImportClass::Certain,
                Some(Value::String(r)) => ImportClass::from_resolution(r)
                    .map_err(|e| unreadable_import_row_error(&row, &format!("{e}")))?,
                Some(other) => {
                    return Err(unreadable_import_row_error(
                        &row,
                        &format!("resolution {other} is not a string"),
                    ))
                }
            };
            if class == ImportClass::Inferred && !view.include_inferred {
                excluded += 1;
            } else {
                kept.push(row);
            }
        }
        *rows = kept;
        let kept = rows.len();
        if obj.contains_key("count") {
            obj.insert("count".into(), json!(kept));
        }
    }
    let mut remainder = ImportRemainder::default();
    remainder.inferred.imports = excluded;
    obj.insert("import_view".into(), view_json(view));
    obj.insert("import_remainder".into(), remainder_json(&remainder));
    Ok(())
}

/// IMPORTS-UNRESOLVED-REMAINDER-1 (RG-REQ-006-L12, RG-REQ-002-L06): the import forms the
/// TypeScript/JavaScript extractor READS but gives no IMPORTS edge, so `imports <file>` omits them
/// — one table, per language. Each item restates a site of `ts-extractor/src/extractor.rs`:
/// - `:191-225`: only top-level `import` statements and `export … from` statements reach
///   `extract_import`, and only through an `import_statement`/`export_statement` `source` field
///   (`import x = require("y")` keeps its source on `import_require_clause`);
/// - `:1513-1527`: a type-only import or a re-export whose specifier has no leading dot returns
///   before an edge is made (JavaScript has no type-only import);
/// - `:1565-1592`: a dynamic `import()` is recorded as an observation only;
/// - `:1612-1640`: `require()` produces bindings only.
///
/// Any other language — or an unknown one — has NO recorded list (`None` → JSON `null`); an empty
/// list, which would read as complete coverage, is never emitted (F-IUR-08).
const LISTING_OMITTED_FORMS: &[(&[&str], &[&str])] = &[
    (
        &["typescript", "tsx"],
        &[
            "type-only imports and re-exports from a specifier without a leading dot",
            "dynamic import() and require() calls",
            "import … = require(…) statements",
            "import statements below the top level",
        ],
    ),
    (
        &["javascript", "jsx"],
        &[
            "re-exports from a specifier without a leading dot",
            "dynamic import() and require() calls",
        ],
    ),
];

/// The recorded omitted forms of `language` ([`LISTING_OMITTED_FORMS`]), `None` when none is
/// recorded for it.
fn listing_omitted_forms(language: Option<&str>) -> Option<&'static [&'static str]> {
    let language = language?;
    LISTING_OMITTED_FORMS
        .iter()
        .find(|(languages, _)| languages.contains(&language))
        .map(|(_, forms)| *forms)
}

/// IMPORTS-UNRESOLVED-REMAINDER-1 (RG-REQ-006-L12, RG-REQ-002-L04, RG-REQ-002-L11; D-IUR-FASTPATH):
/// the ONE attach of a single-file `imports` answer's rows without a confirmed target, run after
/// [`partition_import_rows`] on the `auto` and `sqlite` answers only (the `livegraph`/`compare`
/// engines return before it — D-IUR-ENGINE-SCOPE). Adds:
/// - `unresolved`: the file's `unresolved_edges` IMPORTS rows as stored (`target_key`,
///   `recorded_specifier`, `decoded_target_path`, `line`, `category`, `classification`,
///   `basis_code`, `basis`, `candidates` — every key present; `candidates` is `null` when not
///   recorded, an array of paths, or `{"unreadable": text}`);
/// - `unresolved_count`: its length;
/// - `language`: the file's first language, `null` when none;
/// - `listing_coverage`: `{"omitted_forms": [..] | null}` from [`LISTING_OMITTED_FORMS`];
/// - `unresolved_source`: `"sqlite"` — the rows are read from SQLite on every route, so no answer
///   claims a LiveGraph-only origin for them (`backend_used` names the resolved rows' source only).
///
/// A failed read is the request's `InternalError` naming the read — never an empty array.
pub(crate) fn attach_unresolved_imports(
    value: &mut Value,
    storage: &dyn repo_graph_agent::AgentStorageRead,
    snapshot_uid: &str,
    file_path: &str,
) -> Result<(), ErrorDetail> {
    use repo_graph_agent::UnresolvedCandidates;
    let internal = |e: repo_graph_agent::AgentStorageError| {
        ErrorDetail::new(ErrorCode::InternalError, e.to_string())
    };
    let rows = storage
        .find_unresolved_file_imports(snapshot_uid, file_path)
        .map_err(internal)?;
    let summary = storage
        .compute_file_summary(snapshot_uid, file_path)
        .map_err(internal)?;
    let language = summary.languages.first().cloned();
    let omitted_forms = listing_omitted_forms(language.as_deref());
    let unresolved: Vec<Value> = rows
        .iter()
        .map(|r| {
            let candidates = match &r.candidates {
                UnresolvedCandidates::NotRecorded => Value::Null,
                UnresolvedCandidates::Paths(paths) => json!(paths),
                UnresolvedCandidates::Unreadable(text) => json!({ "unreadable": text }),
            };
            json!({
                "target_key": r.target_key,
                "recorded_specifier": r.recorded_specifier,
                "decoded_target_path": r.decoded_target_path,
                "line": r.line,
                "category": r.category,
                "classification": r.classification,
                "basis_code": r.basis_code,
                "basis": r.basis,
                "candidates": candidates,
            })
        })
        .collect();
    let Some(obj) = value.as_object_mut() else {
        return Err(ErrorDetail::new(
            ErrorCode::InternalError,
            "imports: the answer is not a JSON object; the rows without a confirmed target cannot \
             be attached",
        ));
    };
    obj.insert("unresolved_count".into(), json!(unresolved.len()));
    obj.insert("unresolved".into(), Value::Array(unresolved));
    obj.insert("language".into(), json!(language));
    obj.insert(
        "listing_coverage".into(),
        json!({ "omitted_forms": omitted_forms }),
    );
    obj.insert("unresolved_source".into(), json!("sqlite"));
    Ok(())
}

/// A named error for an `imports` row whose resolution cannot be classified.
fn unreadable_import_row_error(row: &Value, why: &str) -> ErrorDetail {
    ErrorDetail::new(
        ErrorCode::StateUnavailable,
        format!(
            "imports: the import row to {} is unreadable: {why} — the certain and inferred \
             imports cannot be told apart; run `rmap repo rebuild <path>`",
            row.get("file")
                .and_then(Value::as_str)
                .unwrap_or("<unknown file>")
        ),
    )
}

/// D-TESB-11: `inferred_imports_not_judged` of a discovered-module evaluation — the relations
/// whose INFERRED imports a boundary would have judged, sorted, with their total import count.
/// Emitted on every evaluation, `{count: 0, relations: []}` included, so its absence can only
/// mean a daemon that predates the partition (D-TESB-17 row U10).
pub(crate) fn not_judged_relations_json(
    violations: &[repo_graph_classification::boundary_evaluator::ModuleBoundaryViolation],
) -> Value {
    let mut rows: Vec<(&str, &str, u64)> = violations
        .iter()
        .map(|v| {
            (
                v.source_canonical_path.as_str(),
                v.target_canonical_path.as_str(),
                v.import_count,
            )
        })
        .collect();
    rows.sort();
    let count: u64 = rows.iter().map(|r| r.2).sum();
    json!({
        "count": count,
        "relations": rows
            .iter()
            .map(|(s, t, n)| json!({"source": s, "target": t, "import_count": n}))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_tests_param_that_is_not_a_boolean_is_refused() {
        assert_eq!(read_import_view(&json!({})).unwrap(), ImportView::DEFAULT);
        assert_eq!(
            read_import_view(&json!({"include_tests": true})).unwrap(),
            ImportView::CERTAIN_WITH_TESTS
        );
        for bad in [json!("true"), json!(1), Value::Null] {
            let err = read_import_view(&json!({ "include_tests": bad })).expect_err("refused");
            assert_eq!(err.code, ErrorCode::InvalidRequest.as_str());
            assert!(err.message.contains("'include_tests'"), "{}", err.message);
        }
    }

    #[test]
    fn include_inferred_param_that_is_not_a_boolean_is_refused() {
        assert_eq!(
            read_import_view(&json!({"include_inferred": true, "include_tests": true})).unwrap(),
            ImportView::ALL
        );
        for bad in [json!("yes"), json!(0), json!([])] {
            let err = read_import_view(&json!({ "include_inferred": bad })).expect_err("refused");
            assert_eq!(err.code, ErrorCode::InvalidRequest.as_str());
            assert!(
                err.message.contains("'include_inferred'"),
                "{}",
                err.message
            );
        }
    }

    #[test]
    fn remainder_json_states_every_group() {
        let mut r = ImportRemainder::default();
        r.tests.imports = 105;
        r.tests.edges = 6;
        assert_eq!(
            remainder_json(&r),
            json!({
                "tests": {"imports": 105, "edges": 6},
                "inferred": {"imports": 0, "edges": 0},
                "tests_and_inferred": {"imports": 0, "edges": 0},
            })
        );
    }

    // ── Handler proofs over a hand-built store, dispatched through the real daemon ──────────────

    use std::path::PathBuf;
    use std::sync::Arc;

    use repo_graph_daemon_transport::{DispatchResult, Dispatcher, NoOpEmitter, Request};
    use repo_graph_storage::types::{CreateSnapshotInput, GraphEdge, GraphNode, Repo, TrackedFile};

    use crate::dispatch::ServiceDispatcher;
    use crate::registry::RepoRegistry;
    use crate::state::DaemonState;

    /// A registered temp repo whose store is built by hand (directory MODULE nodes from each file's
    /// parent directory, OWNS, FILE nodes, file→file IMPORTS, module candidates), served by a real
    /// `ServiceDispatcher` with the automatic re-index off.
    struct Fx {
        _dir: tempfile::TempDir,
        repo_dir: PathBuf,
        repo_uid: String,
        snap: String,
        db_path: PathBuf,
        dispatcher: ServiceDispatcher,
    }

    fn parent(path: &str) -> &str {
        path.rsplit_once('/').map(|(d, _)| d).unwrap_or(".")
    }

    /// `files`: (path, is_test); `candidates`: (root, owned directory prefixes); `imports`: (from,
    /// to, resolution).
    fn fixture(
        files: &[(&str, bool)],
        candidates: &[(&str, &[&str])],
        imports: &[(&str, &str, &str)],
    ) -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let repo_dir = dir.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let mut registry = RepoRegistry::with_state_root(&dir.path().join("state")).unwrap();
        let entry = registry.register(&repo_dir).unwrap().clone();
        let (repo_uid, db_path) = (entry.repo_uid.clone(), entry.db_path.clone());
        let mut conn = StorageConnection::open(&db_path).unwrap();
        conn.add_repo(&Repo {
            repo_uid: repo_uid.clone(),
            name: "fx".into(),
            root_path: repo_dir.to_string_lossy().to_string(),
            default_branch: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            metadata_json: None,
        })
        .unwrap();
        let snap = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo_uid.clone(),
                kind: "full".into(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .unwrap()
            .snapshot_uid;
        conn.execute_raw(&format!(
            "UPDATE snapshots SET status = 'ready' WHERE snapshot_uid = '{snap}'"
        ))
        .unwrap();
        let tracked: Vec<TrackedFile> = files
            .iter()
            .map(|(p, t)| TrackedFile {
                file_uid: format!("{repo_uid}:{p}"),
                repo_uid: repo_uid.clone(),
                path: p.to_string(),
                language: Some("cpp".into()),
                is_test: *t,
                is_generated: false,
                is_excluded: false,
            })
            .collect();
        conn.upsert_files(&tracked).unwrap();
        conn.upsert_file_versions(
            &tracked
                .iter()
                .map(|f| repo_graph_storage::types::FileVersion {
                    snapshot_uid: snap.clone(),
                    file_uid: f.file_uid.clone(),
                    content_hash: "h".into(),
                    ast_hash: None,
                    extractor: Some("test:1".into()),
                    parse_status: "parsed".into(),
                    size_bytes: Some(10),
                    line_count: Some(1),
                    indexed_at: "2026-01-01T00:00:00Z".into(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let node =
            |uid: String, kind: &str, name: &str, qn: &str, file: Option<String>| GraphNode {
                node_uid: uid,
                snapshot_uid: snap.clone(),
                repo_uid: repo_uid.clone(),
                stable_key: format!("{repo_uid}:{qn}:{kind}"),
                kind: kind.into(),
                subtype: None,
                name: name.into(),
                qualified_name: Some(qn.into()),
                file_uid: file,
                parent_node_uid: None,
                location: None,
                signature: None,
                visibility: None,
                doc_comment: None,
                metadata_json: None,
            };
        let mut dirs: Vec<&str> = files.iter().map(|(p, _)| parent(p)).collect();
        dirs.sort();
        dirs.dedup();
        let mut nodes: Vec<GraphNode> = dirs
            .iter()
            .map(|d| {
                node(
                    format!("m:{d}"),
                    "MODULE",
                    d.rsplit('/').next().unwrap(),
                    d,
                    None,
                )
            })
            .collect();
        nodes.extend(files.iter().map(|(p, _)| {
            node(
                format!("f:{p}"),
                "FILE",
                p.rsplit('/').next().unwrap(),
                p,
                Some(format!("{repo_uid}:{p}")),
            )
        }));
        conn.insert_nodes(&nodes).unwrap();
        let edge = |uid: String, s: String, t: String, ty: &str, res: &str| GraphEdge {
            edge_uid: uid,
            snapshot_uid: snap.clone(),
            repo_uid: repo_uid.clone(),
            source_node_uid: s,
            target_node_uid: t,
            edge_type: ty.into(),
            resolution: res.into(),
            extractor: "test:1".into(),
            location: None,
            metadata_json: None,
        };
        let mut edges: Vec<GraphEdge> = files
            .iter()
            .map(|(p, _)| {
                edge(
                    format!("own:{p}"),
                    format!("m:{}", parent(p)),
                    format!("f:{p}"),
                    "OWNS",
                    "static",
                )
            })
            .collect();
        edges.extend(imports.iter().enumerate().map(|(i, (a, b, r))| {
            edge(
                format!("imp{i}"),
                format!("f:{a}"),
                format!("f:{b}"),
                "IMPORTS",
                r,
            )
        }));
        conn.insert_edges(&edges).unwrap();
        for (root, owned) in candidates {
            conn.execute_raw(&format!(
                "INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, \
                 module_key, module_kind, canonical_root_path, confidence) \
                 VALUES ('mc:{root}', '{snap}', '{repo_uid}', 'dir:{root}', 'inferred', '{root}', 0.7)"
            ))
            .unwrap();
            for (p, _) in files {
                if owned.iter().any(|d| p.starts_with(&format!("{d}/"))) {
                    conn.execute_raw(&format!(
                        "INSERT INTO module_file_ownership (snapshot_uid, repo_uid, file_uid, \
                         module_candidate_uid, assignment_kind, confidence) \
                         VALUES ('{snap}', '{repo_uid}', '{repo_uid}:{p}', 'mc:{root}', \
                         'directory', 0.7)"
                    ))
                    .unwrap();
                }
            }
        }
        drop(conn);
        let state = DaemonState::with_registry(registry);
        crate::auto_reindex::mark_stdio_transport(&state);
        Fx {
            _dir: dir,
            repo_dir,
            repo_uid,
            snap,
            db_path,
            dispatcher: ServiceDispatcher::new(Arc::new(state)),
        }
    }

    impl Fx {
        fn call(&self, method: &str, extra: Value) -> Result<Value, ErrorDetail> {
            let mut params = json!({ "repo": self.repo_dir.to_string_lossy() });
            if let (Some(p), Value::Object(e)) = (params.as_object_mut(), extra) {
                p.extend(e);
            }
            let request = Request {
                id: "t".into(),
                method: method.into(),
                params,
            };
            match self.dispatcher.dispatch(&request, &mut NoOpEmitter) {
                DispatchResult::Success(r) => Ok(r.result),
                DispatchResult::Error(e) => Err(e.error),
            }
        }
        fn ok(&self, method: &str, extra: Value) -> Value {
            self.call(method, extra)
                .unwrap_or_else(|e| panic!("{method} failed: {} {}", e.code, e.message))
        }
        fn storage(&self) -> StorageConnection {
            StorageConnection::open(&self.db_path).unwrap()
        }
    }

    /// leveldb in miniature: `db` imports `table` in production; `table`'s test imports `db` back
    /// (a cycle only through a test file's import); `db`'s test imports `table` too; `util`'s
    /// production `testutil.cc` (test status UNDETERMINED) imports `db`; `db` imports `util`
    /// through an INFERRED import only.
    fn mini_leveldb() -> Fx {
        fixture(
            &[
                ("db/db.cc", false),
                ("db/db_test.cc", true),
                ("table/table.cc", false),
                ("table/table_test.cc", true),
                ("util/util.cc", false),
                ("util/testutil.cc", false),
            ],
            &[("db", &["db"]), ("table", &["table"]), ("util", &["util"])],
            &[
                ("db/db.cc", "table/table.cc", "static"),
                ("db/db_test.cc", "table/table.cc", "static"),
                ("table/table_test.cc", "db/db.cc", "static"),
                ("util/testutil.cc", "db/db.cc", "static"),
                ("db/db.cc", "util/util.cc", "inferred"),
            ],
        )
    }

    fn edge_rows(v: &Value, key: &str) -> Vec<(String, String, u64)> {
        let mut out: Vec<(String, String, u64)> = v[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["source"].as_str().unwrap().to_string(),
                    e["target"].as_str().unwrap().to_string(),
                    e["import_count"].as_u64().unwrap(),
                )
            })
            .collect();
        out.sort();
        out
    }

    fn triple(s: &str, t: &str, n: u64) -> (String, String, u64) {
        (s.to_string(), t.to_string(), n)
    }

    #[test]
    fn modules_list_default_view_drops_test_imports_and_states_the_remainder() {
        let fx = mini_leveldb();
        let v = fx.ok("modules_list", json!({}));
        assert_eq!(
            edge_rows(&v, "edges"),
            vec![triple("db", "table", 1), triple("util", "db", 1)]
        );
        assert_eq!(
            v["import_view"],
            json!({"include_tests": false, "include_inferred": false})
        );
        assert_eq!(
            v["import_remainder"]["tests"],
            json!({"imports": 2, "edges": 1})
        );
        assert_eq!(
            v["import_remainder"]["inferred"],
            json!({"imports": 1, "edges": 1})
        );
        let with_tests = fx.ok("modules_list", json!({"include_tests": true}));
        assert_eq!(
            edge_rows(&with_tests, "edges"),
            vec![
                triple("db", "table", 2),
                triple("table", "db", 1),
                triple("util", "db", 1)
            ]
        );
        assert_eq!(
            fx.call("modules_list", json!({"include_tests": "yes"}))
                .unwrap_err()
                .code,
            "InvalidRequest"
        );
    }

    #[test]
    fn modules_list_edges_carry_their_four_partition_counts() {
        let fx = mini_leveldb();
        let v = fx.ok("modules_list", json!({}));
        let db_table = v["edges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["source"] == "db" && e["target"] == "table")
            .unwrap();
        assert_eq!(
            db_table["partitions"],
            json!({"production_certain": 1, "test_certain": 1, "production_inferred": 0,
                   "test_inferred": 0, "unknown_test_status": 0})
        );
        assert_eq!(db_table["import_count"], 1);
    }

    #[test]
    fn modules_deps_include_tests_restores_test_imports() {
        let fx = mini_leveldb();
        let rows = |v: &Value| edge_rows(v, "results");
        let default = fx.ok("modules_deps", json!({"module": "table"}));
        assert_eq!(rows(&default), vec![triple("db", "table", 1)]);
        assert_eq!(
            default["import_remainder"]["tests"],
            json!({"imports": 2, "edges": 1})
        );
        let with_tests = fx.ok(
            "modules_deps",
            json!({"module": "table", "include_tests": true}),
        );
        assert_eq!(
            rows(&with_tests),
            vec![triple("db", "table", 2), triple("table", "db", 1)]
        );
        assert_eq!(
            with_tests["import_remainder"]["tests"],
            json!({"imports": 0, "edges": 0})
        );
    }

    #[test]
    fn modules_show_keeps_test_imports_and_states_its_basis() {
        let fx = mini_leveldb();
        let v = fx.ok("modules_show", json!({"module": "table"}));
        assert_eq!(
            v["import_view"],
            json!({"include_tests": true, "include_inferred": false})
        );
        let outbound = v["outbound_dependencies"].to_string();
        assert!(
            outbound.contains("db"),
            "table → db (a test import) is kept: {outbound}"
        );
    }

    fn declare(fx: &Fx, from: &str, forbids: &str, discovered: bool) {
        let value = if discovered {
            json!({"selectorDomain": "discovered_module",
                   "source": {"canonicalRootPath": from},
                   "forbids": {"canonicalRootPath": forbids}})
        } else {
            json!({"forbids": forbids})
        };
        fx.storage()
            .insert_declaration(&repo_graph_storage::crud::declarations::DeclarationInsert {
                identity_key: format!("{}:{from}:{forbids}", if discovered { "d" } else { "b" }),
                repo_uid: fx.repo_uid.clone(),
                target_stable_key: format!("{}:{from}:MODULE", fx.repo_uid),
                kind: "boundary".into(),
                value_json: value.to_string(),
                created_at: "2026-01-01T00:00:00Z".into(),
                created_by: Some("test".into()),
                supersedes_uid: None,
                authored_basis_json: None,
            })
            .unwrap();
    }

    #[test]
    fn modules_violations_keep_test_imports_and_count_inferred_ones_not_judged() {
        let fx = mini_leveldb();
        declare(&fx, "table", "db", true);
        declare(&fx, "db", "util", true);
        let v = fx.ok("modules_violations", json!({}));
        // table → db exists only through a test import: governance judges it.
        assert_eq!(v["count"], 1);
        assert_eq!(
            v["inferred_imports_not_judged"],
            json!({"count": 1, "relations": [{"source": "db", "target": "util", "import_count": 1}]})
        );
    }

    #[test]
    fn violations_not_judged_lines_carry_the_cited_files() {
        let fx = mini_leveldb();
        declare(&fx, "db", "util", false);
        let v = fx.ok("violations", json!({}));
        assert_eq!(
            v["declared_boundary_count"], 0,
            "the only db → util import is inferred"
        );
        assert_eq!(
            v["inferred_imports_not_judged"]["declared"],
            json!([{"boundary_module": "db", "forbidden_module": "util", "count": 1,
                    "files": ["db/db.cc"]}])
        );
    }

    fn member_sets(v: &Value) -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = v["cycles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                c["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n["qualified_name"].as_str().unwrap().to_string())
                    .collect()
            })
            .collect();
        out.sort();
        out
    }

    /// (members, flags, contains_shown) of one excluded cycle.
    type ExcludedRow = (Vec<String>, Vec<String>, Vec<Vec<String>>);

    fn excluded(v: &Value) -> Vec<ExcludedRow> {
        v["excluded_cycles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                let strs = |x: &Value| -> Vec<String> {
                    x.as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s.as_str().unwrap().to_string())
                        .collect()
                };
                (
                    strs(&e["members"]),
                    strs(&e["flags"]),
                    e["contains_shown"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(strs)
                        .collect(),
                )
            })
            .collect()
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cycles_default_view_names_a_cycle_closed_only_by_test_imports() {
        let fx = mini_leveldb();
        let v = fx.ok("cycles", json!({}));
        assert_eq!(v["count"], 0);
        assert_eq!(
            v["module_edge_count"], 2,
            "db→table and util→db in production"
        );
        let ex = excluded(&v);
        assert!(
            ex.contains(&(names(&["db", "table"]), names(&["include_tests"]), vec![])),
            "{ex:?}"
        );
        assert_eq!(v["import_remainder"]["tests"]["imports"], 2);
    }

    #[test]
    fn importer_block_counts_the_undetermined_files_among_production_importers() {
        let fx = mini_leveldb();
        let cycles = fx.ok("cycles", json!({}));
        assert_eq!(
            cycles["importer_test_status_undetermined"],
            json!({"count": 1, "paths": ["util/testutil.cc"], "universe": "cross_directory_importers",
                   "universe_count": 2, "unknown_count": 0})
        );
        let list = fx.ok("modules_list", json!({}));
        assert_eq!(
            list["importer_test_status_undetermined"]["universe"],
            "cross_module_importers"
        );
        assert_eq!(list["importer_test_status_undetermined"]["count"], 1);
        assert_eq!(
            list["importer_test_status_undetermined"]["universe_count"],
            2
        );
    }

    #[test]
    fn cycles_sqlite_engine_honours_the_view() {
        let fx = mini_leveldb();
        let default = fx.ok("cycles", json!({"engine": "sqlite"}));
        assert!(member_sets(&default).is_empty());
        let with_tests = fx.ok("cycles", json!({"engine": "sqlite", "include_tests": true}));
        assert_eq!(member_sets(&with_tests), vec![names(&["db", "table"])]);
        assert_eq!(with_tests["import_view"]["include_tests"], true);
        assert!(
            with_tests["cycles"][0]["partitions"]["test_certain"]
                .as_u64()
                .unwrap()
                >= 1
        );
    }

    #[test]
    fn cycles_partition_flags_with_livegraph_or_compare_engine_are_refused() {
        let fx = mini_leveldb();
        for engine in ["livegraph", "compare"] {
            for flag in ["include_tests", "include_inferred"] {
                let err = fx
                    .call(
                        "cycles",
                        json!({"engine": engine, "kind": "module-import", flag: true}),
                    )
                    .unwrap_err();
                assert_eq!(err.code, "InvalidRequest", "{engine} {flag}");
                assert!(
                    err.message.contains("--engine livegraph|compare"),
                    "{}",
                    err.message
                );
            }
        }
    }

    #[test]
    fn cycles_fastpath_serves_when_the_view_excludes_nothing() {
        let fx = mini_leveldb();
        let graph = fx.storage().directory_module_graph(&fx.snap).unwrap();
        assert!(!livegraph_cycles_eligible(&graph, ImportView::DEFAULT));
        assert!(livegraph_cycles_eligible(&graph, ImportView::ALL));
        let clean = fixture(
            &[("a/x.cc", false), ("b/y.cc", false)],
            &[],
            &[
                ("a/x.cc", "b/y.cc", "static"),
                ("b/y.cc", "a/x.cc", "static"),
            ],
        );
        let g = clean.storage().directory_module_graph(&clean.snap).unwrap();
        assert!(livegraph_cycles_eligible(&g, ImportView::DEFAULT));
    }

    /// F-TESB-CYCLE: two directories owned by ONE module candidate, a production import one way and
    /// a test import back — the candidate population excludes nothing, the directory population
    /// excludes one import, so the fastpath is refused and SQLite names the excluded cycle.
    #[test]
    fn cycles_fastpath_is_refused_when_an_intra_candidate_cross_directory_test_import_closes_a_cycle(
    ) {
        let fx = fixture(
            &[
                ("lib/a/x.rs", false),
                ("lib/b/y.rs", false),
                ("lib/b/y_test.rs", true),
            ],
            &[("lib", &["lib"])],
            &[
                ("lib/a/x.rs", "lib/b/y.rs", "static"),
                ("lib/b/y_test.rs", "lib/a/x.rs", "static"),
            ],
        );
        let list = fx.ok("modules_list", json!({}));
        assert_eq!(
            list["import_remainder"]["tests"]["imports"], 0,
            "nothing crosses a candidate"
        );
        let graph = fx.storage().directory_module_graph(&fx.snap).unwrap();
        assert!(!livegraph_cycles_eligible(&graph, ImportView::DEFAULT));
        let cycles = fx.ok("cycles", json!({}));
        assert_eq!(cycles["backend_used"], "sqlite");
        assert_eq!(
            excluded(&cycles),
            vec![(
                names(&["lib/a", "lib/b"]),
                names(&["include_tests"]),
                vec![]
            )]
        );
    }

    /// review-1's counterexample: production-inferred A↔B, test-certain B↔C.
    fn mixed_scc() -> Fx {
        fixture(
            &[
                ("a/x.py", false),
                ("b/x.py", false),
                ("b/t_test.py", true),
                ("c/t_test.py", true),
            ],
            &[],
            &[
                ("a/x.py", "b/x.py", "static"),
                ("b/x.py", "a/x.py", "inferred"),
                ("b/t_test.py", "c/t_test.py", "static"),
                ("c/t_test.py", "b/x.py", "static"),
            ],
        )
    }

    #[test]
    fn cycles_default_view_names_the_mixed_scc_counterexample() {
        let fx = mixed_scc();
        let v = fx.ok("cycles", json!({}));
        assert_eq!(v["count"], 0);
        assert_eq!(
            excluded(&v),
            vec![
                (names(&["a", "b"]), names(&["include_inferred"]), vec![]),
                (
                    names(&["a", "b", "c"]),
                    names(&["include_tests", "include_inferred"]),
                    vec![]
                ),
                (names(&["b", "c"]), names(&["include_tests"]), vec![]),
            ]
        );
    }

    #[test]
    fn cycles_include_tests_names_the_merged_cycle_with_the_full_flag_set() {
        let fx = mixed_scc();
        let v = fx.ok("cycles", json!({"include_tests": true}));
        assert_eq!(member_sets(&v), vec![names(&["b", "c"])]);
        assert_eq!(
            excluded(&v),
            vec![(
                names(&["a", "b", "c"]),
                names(&["include_tests", "include_inferred"]),
                vec![names(&["b", "c"])]
            )]
        );
    }

    #[test]
    fn imports_default_lists_certain_rows_and_counts_inferred_ones() {
        let fx = mini_leveldb();
        let v = fx.ok("imports", json!({"file": "db/db.cc", "engine": "sqlite"}));
        let files: Vec<&str> = v["imports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["file"].as_str().unwrap())
            .collect();
        assert_eq!(files, vec!["table/table.cc"]);
        assert_eq!(v["count"], 1);
        assert_eq!(v["import_remainder"]["inferred"]["imports"], 1);
        let auto = fx.ok("imports", json!({"file": "db/db.cc"}));
        assert_eq!(auto["import_remainder"]["inferred"]["imports"], 1);
    }

    #[test]
    fn imports_include_inferred_lists_inferred_rows_on_the_sqlite_route() {
        let fx = mini_leveldb();
        let v = fx.ok(
            "imports",
            json!({"file": "db/db.cc", "include_inferred": true}),
        );
        assert!(
            v.get("backend_used").is_none(),
            "routed to the explicit SQLite listing"
        );
        let mut rows: Vec<(&str, &str)> = v["imports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["file"].as_str().unwrap(),
                    r["resolution"].as_str().unwrap(),
                )
            })
            .collect();
        rows.sort();
        assert_eq!(
            rows,
            vec![("table/table.cc", "static"), ("util/util.cc", "inferred")]
        );
        assert_eq!(v["import_remainder"]["inferred"]["imports"], 0);
        let err = fx
            .call(
                "imports",
                json!({"file": "db/db.cc", "engine": "livegraph", "include_inferred": true}),
            )
            .unwrap_err();
        assert_eq!(err.code, "InvalidRequest");
    }

    // ── IMPORTS-UNRESOLVED-REMAINDER-1 (RG-REQ-006-L12, D-IUR-FASTPATH, D-IUR-ENGINE-SCOPE) ────────

    /// Insert one unresolved IMPORTS row of `source` (a FILE path of the store) into `conn`.
    #[allow(clippy::too_many_arguments)]
    fn insert_unresolved(
        conn: &StorageConnection,
        repo_uid: &str,
        snap: &str,
        uid: &str,
        source_node_uid: &str,
        target_key: &str,
        line: u32,
        category: &str,
        classification: &str,
        basis_code: &str,
        metadata: Option<&str>,
    ) {
        let meta = metadata
            .map(|m| format!("'{}'", m.replace('\'', "''")))
            .unwrap_or_else(|| "NULL".into());
        conn.execute_raw(&format!(
            "INSERT INTO unresolved_edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
             target_key, type, resolution, extractor, line_start, metadata_json, category, \
             classification, classifier_version, basis_code, observed_at) VALUES ('{uid}', '{snap}', \
             '{repo_uid}', '{source_node_uid}', '{target_key}', 'IMPORTS', 'static', 'test:1', {line}, \
             {meta}, '{category}', '{classification}', 1, '{basis_code}', '2026-10-03T00:00:00Z')"
        ))
        .unwrap();
    }

    /// `mini_leveldb()` with three unresolved IMPORTS rows of `db/db.cc`: an external package, a
    /// project alias, and an ambiguous basename with two recorded FILE candidates.
    fn mini_leveldb_with_unresolved() -> Fx {
        let fx = mini_leveldb();
        let conn = fx.storage();
        let (r, snap) = (fx.repo_uid.as_str(), fx.snap.as_str());
        insert_unresolved(
            &conn,
            r,
            snap,
            "u1",
            "f:db/db.cc",
            "snappy",
            3,
            "imports_file_not_found",
            "external_library_candidate",
            "specifier_matches_package_dependency",
            Some(r#"{"rawPath":"snappy"}"#),
        );
        insert_unresolved(
            &conn,
            r,
            snap,
            "u2",
            "f:db/db.cc",
            "@/port/port.h",
            5,
            "imports_file_not_found",
            "internal_candidate",
            "specifier_matches_project_alias",
            None,
        );
        let candidates = format!(
            r#"{{"basis":"ambiguous_basename","candidates":["{r}:util/util.cc:FILE","{r}:table/table.cc:FILE"]}}"#
        );
        insert_unresolved(
            &conn,
            r,
            snap,
            "u3",
            "f:db/db.cc",
            "env.h",
            9,
            "imports_ambiguous_match",
            "unknown",
            "no_supporting_signal",
            Some(&candidates),
        );
        drop(conn);
        fx
    }

    const UNRESOLVED_KEYS: [&str; 5] = [
        "unresolved",
        "unresolved_count",
        "language",
        "listing_coverage",
        "unresolved_source",
    ];

    #[test]
    fn imports_sqlite_route_carries_unresolved_rows_their_count_and_the_language() {
        let fx = mini_leveldb_with_unresolved();
        let v = fx.ok("imports", json!({"file": "db/db.cc", "engine": "sqlite"}));
        // The listing's own keys are unchanged.
        assert_eq!(v["file"], "db/db.cc");
        assert_eq!(v["count"], 1);
        assert_eq!(v["import_remainder"]["inferred"]["imports"], 1);
        assert!(v.get("import_view").is_some());
        assert_eq!(v["unresolved_count"], 3);
        assert_eq!(v["language"], "cpp");
        assert_eq!(
            v["listing_coverage"],
            json!({"omitted_forms": null}),
            "no recorded list for cpp — null, never []"
        );
        assert_eq!(v["unresolved_source"], "sqlite");
        assert_eq!(
            v["unresolved"],
            json!([
                {
                    "target_key": "snappy",
                    "recorded_specifier": "snappy",
                    "decoded_target_path": null,
                    "line": 3,
                    "category": "imports_file_not_found",
                    "classification": "external_library_candidate",
                    "basis_code": "specifier_matches_package_dependency",
                    "basis": null,
                    "candidates": null,
                },
                {
                    "target_key": "@/port/port.h",
                    "recorded_specifier": null,
                    "decoded_target_path": null,
                    "line": 5,
                    "category": "imports_file_not_found",
                    "classification": "internal_candidate",
                    "basis_code": "specifier_matches_project_alias",
                    "basis": null,
                    "candidates": null,
                },
                {
                    "target_key": "env.h",
                    "recorded_specifier": null,
                    "decoded_target_path": null,
                    "line": 9,
                    "category": "imports_ambiguous_match",
                    "classification": "unknown",
                    "basis_code": "no_supporting_signal",
                    "basis": "ambiguous_basename",
                    "candidates": ["util/util.cc", "table/table.cc"],
                },
            ])
        );
        // A file with no unresolved row carries a measured zero, never an absent key.
        let t = fx.ok(
            "imports",
            json!({"file": "table/table.cc", "engine": "sqlite"}),
        );
        assert_eq!(t["unresolved"], json!([]));
        assert_eq!(t["unresolved_count"], 0);
    }

    #[test]
    fn imports_auto_route_carries_the_same_unresolved_rows_as_the_sqlite_route() {
        let fx = mini_leveldb_with_unresolved();
        let sqlite = fx.ok("imports", json!({"file": "db/db.cc", "engine": "sqlite"}));
        let auto = fx.ok("imports", json!({"file": "db/db.cc"}));
        assert!(
            auto.get("backend_used").is_some(),
            "the auto answer keeps its backend label"
        );
        for key in UNRESOLVED_KEYS {
            assert_eq!(auto[key], sqlite[key], "{key}");
        }
        assert_eq!(auto["count"], sqlite["count"]);
    }

    #[test]
    fn imports_unresolved_read_failure_is_an_error_never_an_empty_list() {
        let fx = mini_leveldb_with_unresolved();
        fx.storage()
            .execute_raw("DROP TABLE unresolved_edges")
            .unwrap();
        for extra in [
            json!({"file": "db/db.cc", "engine": "sqlite"}),
            json!({"file": "db/db.cc"}),
            json!({"file": "db/db.cc", "include_inferred": true}),
        ] {
            let err = fx
                .call("imports", extra.clone())
                .expect_err("a failed unresolved read fails the request");
            assert_eq!(err.code, ErrorCode::InternalError.as_str(), "{extra}");
            assert!(
                err.message.contains("find_unresolved_file_imports"),
                "{extra}: the error names the read: {}",
                err.message
            );
        }
    }

    #[test]
    fn imports_livegraph_and_compare_engines_carry_no_unresolved_key() {
        let fx = mini_leveldb_with_unresolved();
        for engine in ["livegraph", "compare"] {
            for extra in [
                json!({"file": "db/db.cc", "engine": engine}),
                json!({"engine": engine}),
            ] {
                let v = fx.ok("imports", extra.clone());
                for key in UNRESOLVED_KEYS {
                    assert!(v.get(key).is_none(), "{extra}: {key} present: {v}");
                }
            }
        }
    }

    /// D-IUR-FASTPATH (human, option A): on a GREEN import certificate the `auto` answer is served
    /// from the resident LiveGraph (`backend_used: "livegraph"`), and its rows without a confirmed
    /// target are still read from SQLite and named as such. The three calls are `handle_imports`'s
    /// `auto` arm in order (IUR-C07 criterion 2 covers that the dispatcher reaches this one attach).
    #[test]
    fn imports_auto_route_on_a_green_cert_reads_the_unresolved_rows_from_sqlite_and_names_the_source(
    ) {
        use crate::callgraph_cert::test_fixture;
        use crate::livegraph_feed::{import_cert_eligibility, imports_auto_response, RequestEpoch};
        use repo_graph_agent::AgentStorageRead;
        let f = test_fixture::build_fixture(false);
        let file = test_fixture::CALLER_PATH;
        {
            let conn = f.state.storage().unwrap();
            // `nf0` is the fixture's FILE node of CALLER_PATH (test_fixture::build_sqlite_mirror).
            insert_unresolved(
                &conn,
                test_fixture::REPO,
                &f.snapshot_uid,
                "u_green",
                "nf0",
                "react",
                2,
                "imports_file_not_found",
                "external_library_candidate",
                "specifier_matches_package_dependency",
                Some(r#"{"rawPath":"react"}"#),
            );
        }
        // F-M-001: the GREEN certificate is obtained through the sanctioned build-then-peek path
        // (`import_cert_eligibility` builds and stores the import cert at the resident fingerprint);
        // this module reads no LiveGraph field (the consolidation witness's reader manifest).
        let storage = f.state.storage().unwrap();
        let serve = |route: &str| -> Value {
            let snapshot = AgentStorageRead::get_latest_snapshot(&storage, test_fixture::REPO)
                .unwrap()
                .unwrap();
            let mut value = if route == "auto" {
                let fingerprint =
                    import_cert_eligibility(&f.state, test_fixture::REPO, &snapshot.snapshot_uid);
                assert!(fingerprint.is_some(), "GREEN import cert -> eligible");
                let epoch = RequestEpoch {
                    snapshot,
                    fingerprint,
                };
                imports_auto_response(&f.state, test_fixture::REPO, &epoch, file)
            } else {
                let rows = storage
                    .find_imports(
                        &f.snapshot_uid,
                        &format!("{}:{file}:FILE", test_fixture::REPO),
                    )
                    .unwrap();
                json!({"file": file, "imports": rows, "count": rows.len()})
            };
            partition_import_rows(&mut value, ImportView::DEFAULT).unwrap();
            attach_unresolved_imports(&mut value, &storage, &f.snapshot_uid, file).unwrap();
            value
        };
        let green = serve("auto");
        assert_eq!(
            green["backend_used"], "livegraph",
            "the resolved rows are served from the LiveGraph"
        );
        assert_eq!(green["comparison"]["source"], "repo_no_loss_certificate");
        assert_eq!(green["unresolved_source"], "sqlite");
        assert_eq!(green["unresolved_count"], 1);
        assert_eq!(green["unresolved"][0]["target_key"], "react");
        let sqlite = serve("sqlite");
        assert_eq!(sqlite["unresolved_source"], "sqlite");
        for key in UNRESOLVED_KEYS {
            assert_eq!(green[key], sqlite[key], "{key}");
        }
        // A storage error on the unresolved read fails the GREEN route by name, too.
        storage.execute_raw("DROP TABLE unresolved_edges").unwrap();
        let snapshot = AgentStorageRead::get_latest_snapshot(&storage, test_fixture::REPO)
            .unwrap()
            .unwrap();
        let fingerprint =
            import_cert_eligibility(&f.state, test_fixture::REPO, &snapshot.snapshot_uid);
        let epoch = RequestEpoch {
            snapshot,
            fingerprint,
        };
        let mut value = imports_auto_response(&f.state, test_fixture::REPO, &epoch, file);
        assert_eq!(value["backend_used"], "livegraph");
        partition_import_rows(&mut value, ImportView::DEFAULT).unwrap();
        let err = attach_unresolved_imports(&mut value, &storage, &f.snapshot_uid, file)
            .expect_err("a failed read is an error, never an empty array");
        assert_eq!(err.code, ErrorCode::InternalError.as_str());
        assert!(
            err.message.contains("find_unresolved_file_imports"),
            "{}",
            err.message
        );
    }

    // ── D-TESB-17: unknown is never zero (rows U6, U8, U9, U10, W7–W9) ─────────────────────

    fn cycle_item(nodes: Value) -> Value {
        json!({"cycles": [{"nodes": nodes}]})
    }

    #[test]
    fn cycle_partitions_are_null_with_a_reason_when_a_member_matches_no_directory_module() {
        let fx = mini_leveldb();
        let graph = fx.storage().directory_module_graph(&fx.snap).unwrap();
        // A member that is a directory module, and one that is not: never the found member's
        // (a subset's) counts, never zeros.
        let mut v = cycle_item(json!([{"node_id": "m:db"}, {"node_id": "nowhere/else"}]));
        attach_cycle_partition(&mut v, &graph, ImportView::DEFAULT, &mut || {
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        let c = &v["cycles"][0];
        assert!(c.get("partitions").is_some_and(Value::is_null), "{c}");
        assert!(
            c["partitions_unavailable"]
                .as_str()
                .is_some_and(|r| r.contains("nowhere/else")),
            "{c}"
        );
        // Both members resolve (one by node uid, one by qualified path): measured counts.
        let mut v = cycle_item(json!([{"node_id": "m:db"}, {"node_id": "table"}]));
        attach_cycle_partition(&mut v, &graph, ImportView::DEFAULT, &mut || {
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(v["cycles"][0]["partitions"]["production_certain"], 1);
        assert_eq!(v["cycles"][0]["partitions"]["test_certain"], 2);
        assert!(v["cycles"][0].get("partitions_unavailable").is_none());
    }

    #[test]
    fn cycle_partitions_are_null_with_a_reason_when_a_member_has_no_node_id() {
        let fx = mini_leveldb();
        let graph = fx.storage().directory_module_graph(&fx.snap).unwrap();
        let mut v = cycle_item(json!([{"node_id": "m:db"}, {"qualified_name": "table"}]));
        attach_cycle_partition(&mut v, &graph, ImportView::DEFAULT, &mut || {
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        let c = &v["cycles"][0];
        assert!(c["partitions"].is_null(), "{c}");
        assert!(c["partitions_unavailable"]
            .as_str()
            .is_some_and(|r| r.contains("node_id")));
    }

    #[test]
    fn cycle_partitions_are_null_with_a_reason_when_a_member_node_id_is_a_number() {
        let fx = mini_leveldb();
        let graph = fx.storage().directory_module_graph(&fx.snap).unwrap();
        let mut v = cycle_item(json!([{"node_id": "m:db"}, {"node_id": 5}]));
        attach_cycle_partition(&mut v, &graph, ImportView::DEFAULT, &mut || {
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        let c = &v["cycles"][0];
        assert!(c["partitions"].is_null(), "{c}");
        assert!(c["partitions_unavailable"]
            .as_str()
            .is_some_and(|r| r.contains("node_id") && r.contains('5')));
    }

    #[test]
    fn module_edge_partitions_absent_serialize_null_with_a_reason_never_zero() {
        let fx = mini_leveldb();
        let storage = fx.storage();
        let default = repo_graph_module_queries::load_module_graph_facts(
            &storage,
            &fx.snap,
            ImportView::DEFAULT,
        )
        .unwrap();
        let all =
            repo_graph_module_queries::load_module_graph_facts(&storage, &fx.snap, ImportView::ALL)
                .unwrap();
        let outside = all
            .edges
            .iter()
            .find(|e| e.source_canonical_path == "table" && e.target_canonical_path == "db")
            .expect("table → db exists only through a test import");
        let fields = edge_partition_fields(&default, outside);
        assert_eq!(fields["partitions"], Value::Null);
        assert!(fields["partitions_unavailable"]
            .as_str()
            .is_some_and(|r| r.contains("table -> db")));
        let inside = &default.edges[0];
        let fields = edge_partition_fields(&default, inside);
        assert!(fields["partitions"].is_object());
        assert!(!fields.contains_key("partitions_unavailable"));
        // The importer block over an edge without its evidence: null beside a reason.
        let block = module_edge_partition_fields(&default, &[outside]);
        assert_eq!(block["importer_test_status_undetermined"], Value::Null);
        assert!(block["importer_test_status_undetermined_unavailable"].is_string());
    }

    #[test]
    fn imports_row_with_a_resolution_outside_the_vocabulary_is_refused_never_kept_as_certain() {
        let mut v = json!({"file": "a.cc", "count": 2, "imports": [
            {"file": "b.cc", "resolution": "static"},
            {"file": "c.cc", "resolution": "resolved"},
        ]});
        let err = partition_import_rows(&mut v, ImportView::DEFAULT)
            .expect_err("an out-of-vocabulary resolution is refused");
        assert!(err.message.contains("c.cc"), "{}", err.message);
        assert!(err.message.contains("\"resolved\""), "{}", err.message);
        // A LiveGraph row (no resolution key) is kept; a readable set partitions.
        let mut v = json!({"count": 2, "imports": [
            {"file": "b.cc"},
            {"file": "d.cc", "resolution": "inferred"},
        ]});
        partition_import_rows(&mut v, ImportView::DEFAULT).unwrap();
        assert_eq!(v["count"], 1);
        assert_eq!(v["import_remainder"]["inferred"]["imports"], 1);
    }

    #[test]
    fn imports_row_with_a_non_string_resolution_is_refused() {
        for bad in [json!(3), json!({"class": "static"}), Value::Null] {
            let mut v = json!({"count": 1, "imports": [{"file": "b.cc", "resolution": bad}]});
            let err = partition_import_rows(&mut v, ImportView::ALL)
                .expect_err("a non-string resolution is refused");
            assert!(err.message.contains("not a string"), "{}", err.message);
        }
    }

    #[test]
    fn partitioned_answers_carry_every_partition_key_even_when_nothing_is_excluded() {
        let fx = fixture(
            &[("a/a.cc", false), ("b/b.cc", false)],
            &[("a", &["a"]), ("b", &["b"])],
            &[
                ("a/a.cc", "b/b.cc", "static"),
                ("b/b.cc", "a/a.cc", "static"),
            ],
        );
        let zero = json!({"tests": {"imports": 0, "edges": 0},
                          "inferred": {"imports": 0, "edges": 0},
                          "tests_and_inferred": {"imports": 0, "edges": 0}});
        let view = json!({"include_tests": false, "include_inferred": false});
        for (method, extra) in [
            ("modules_list", json!({})),
            ("modules_deps", json!({})),
            ("cycles", json!({})),
            ("cycles", json!({"engine": "sqlite"})),
        ] {
            let v = fx.ok(method, extra.clone());
            assert_eq!(v["import_view"], view, "{method} {extra}");
            assert_eq!(v["import_remainder"], zero, "{method} {extra}");
            assert_eq!(
                v["importer_test_status_undetermined"]["count"], 0,
                "{method} {extra}"
            );
        }
        for extra in [json!({}), json!({"engine": "sqlite"})] {
            let v = fx.ok("cycles", extra.clone());
            assert_eq!(v["excluded_cycles"], json!([]), "{extra}");
            assert!(v["cycles"][0]["partitions"].is_object(), "{extra}");
        }
        let v = fx.ok("modules_list", json!({}));
        assert!(v["edges"][0]["partitions"].is_object());
        let v = fx.ok("imports", json!({"file": "a/a.cc", "engine": "sqlite"}));
        assert_eq!(v["import_view"], view);
        assert_eq!(v["import_remainder"], zero);
    }

    #[test]
    fn governance_and_map_partition_counts_are_emitted_at_zero() {
        let fx = fixture(
            &[("a/a.cc", false), ("b/b.cc", false)],
            &[("a", &["a"]), ("b", &["b"])],
            &[("a/a.cc", "b/b.cc", "static")],
        );
        declare(&fx, "a", "b", false);
        declare(&fx, "a", "b", true);
        let v = fx.ok("violations", json!({}));
        assert_eq!(
            v["inferred_imports_not_judged"],
            json!({"declared": [], "discovered": {"count": 0, "relations": []}})
        );
        let v = fx.ok("modules_violations", json!({}));
        assert_eq!(
            v["inferred_imports_not_judged"],
            json!({"count": 0, "relations": []})
        );
        let v = fx.ok("map", json!({"path": ""}));
        for f in v["files"].as_array().unwrap() {
            assert_eq!(f["inferred_import_count"], 0, "{f}");
        }
    }

    #[test]
    fn modules_list_over_a_non_integer_importer_flag_answers_a_named_error() {
        let fx = mini_leveldb();
        fx.storage()
            .execute_raw(&format!(
                "UPDATE files SET is_test = 'yes' WHERE file_uid = '{}:db/db.cc'",
                fx.repo_uid
            ))
            .unwrap();
        // Whichever read meets the value first (the partitioned import read names the file and
        // the value; the owned-file read names the column), the answer is a named error naming
        // `is_test` — never a production-by-default partition.
        for method in ["modules_list", "modules_deps", "cycles"] {
            let err = fx
                .call(method, json!({}))
                .expect_err("a non-integer importer flag never answers");
            assert!(
                err.message.contains("is_test"),
                "{method}: {} {}",
                err.code,
                err.message
            );
        }
    }

    #[test]
    fn map_dependency_edges_are_certain_and_attach_the_inferred_count() {
        let fx = mini_leveldb();
        let v = fx.ok("map", json!({"path": ""}));
        let text = v.to_string();
        let files = v["files"].as_array().expect("files");
        let db = files
            .iter()
            .find(|f| f["path"] == "db/db.cc")
            .expect("db/db.cc");
        assert_eq!(db["inferred_import_count"], 1);
        let table = files
            .iter()
            .find(|f| f["path"] == "table/table.cc")
            .unwrap();
        // D-TESB-17 row U10: a measured zero is emitted, never omitted.
        assert_eq!(table["inferred_import_count"], 0);
        let deps = v["dependency_edges"].as_array().expect("dependency edges");
        assert!(
            !deps
                .iter()
                .any(|e| e["source"] == "db/db.cc" && e["target"] == "util/util.cc"),
            "the inferred import is not a certain dependency edge: {text}"
        );
    }

    #[test]
    fn path_include_inferred_walks_inferred_hops_and_counts_them() {
        let fx = mini_leveldb();
        let storage = fx.storage();
        let from = format!("{}:db/db.cc:FILE", fx.repo_uid);
        let to = format!("{}:util/util.cc:FILE", fx.repo_uid);
        let certain = crate::call_certainty::sqlite_path_value(
            &storage,
            &fx.repo_uid,
            &fx.snap,
            &from,
            &to,
            false,
        )
        .unwrap();
        assert_eq!(certain["found"], false);
        assert_eq!(
            certain["inferred_edges_on_route"], 1,
            "the admitting walk's statement"
        );
        let admitted = crate::call_certainty::sqlite_path_value(
            &storage,
            &fx.repo_uid,
            &fx.snap,
            &from,
            &to,
            true,
        )
        .unwrap();
        assert_eq!(admitted["found"], true);
        assert_eq!(admitted["inferred_edges_on_route"], 1);
        assert_eq!(admitted["include_inferred"], true);
    }

    #[test]
    fn path_include_inferred_with_livegraph_or_compare_engine_is_refused() {
        let fx = mini_leveldb();
        for engine in ["livegraph", "compare"] {
            let err = fx
                .call(
                    "path",
                    json!({"from": "x", "to": "y", "engine": engine, "include_inferred": true}),
                )
                .unwrap_err();
            assert_eq!(err.code, "InvalidRequest", "{engine}");
        }
        let err = fx
            .call(
                "path",
                json!({"from": "x", "to": "y", "include_inferred": "1"}),
            )
            .unwrap_err();
        assert_eq!(err.code, "InvalidRequest");
    }

    /// RG-REQ-002-L02: orient's module-edges line and `modules list` answer the SAME default view —
    /// the same edges (top 3), the same remainder, the same importer block.
    #[test]
    fn orient_top_module_edges_equal_the_modules_list_default_view() {
        let fx = mini_leveldb();
        let list = fx.ok("modules_list", json!({}));
        let orient = fx.ok("orient", json!({}));
        let value = orient.get("value").unwrap_or(&orient);
        let top = &value["top_module_edges"];
        let mut list_rows = edge_rows(&list, "edges");
        list_rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| (&a.0, &a.1).cmp(&(&b.0, &b.1))));
        list_rows.truncate(3);
        let mut top_rows = edge_rows(top, "edges");
        top_rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| (&a.0, &a.1).cmp(&(&b.0, &b.1))));
        assert_eq!(top_rows, list_rows);
        assert_eq!(top["import_remainder"], list["import_remainder"]);
        assert_eq!(
            top["importer_test_status_undetermined"],
            list["importer_test_status_undetermined"]
        );
    }

    // ── The daemon call-site inventory (review-0 F-5, the second guard) ─────────────────────────

    /// The storage IMPORTS readers of §2.3 and the LiveGraph import APIs a daemon call site may
    /// reach. SCOPE — daemon-runtime production sources only (the storage SQL is the storage guard's).
    const IMPORT_READ_APIS: &[&str] = &[
        "file_imports_with_partition",
        "directory_module_graph",
        "find_cycles_cancellable",
        "find_cycles",
        "compute_module_stats",
        "find_path_prefix_module_cycles",
        "find_imports",
        "all_imports",
        "find_shortest_path",
        "map_resolved_dep_edges_in_path",
        "map_inferred_import_counts_in_path",
        "find_imports_between_paths",
        "find_inferred_imports_between_paths",
        "find_file_importers",
        "find_file_imports",
        "find_inferred_file_imports",
        "find_unresolved_file_imports",
        "import_cycle_partition",
        "load_module_graph_facts",
        "find_boundary_imports",
        "find_inferred_boundary_imports",
        "find_module_cycles",
        "find_module_cycles_cancellable",
        "find_cycles_involving_path",
        "find_cycles_involving_path_cancellable",
        "find_cycles_involving_module",
        "find_cycles_involving_module_cancellable",
        "module_import_cycles",
        "module_import_cycles_cancellable",
        "module_import_pairs",
        "file_import_edges",
        "live_import_view",
        "module_stats",
    ];

    /// (file relative to `src`, enclosing fn, API, §2.3 disposition). Every production call site.
    const DAEMON_CALL_SITES: &[(&str, &str, &str, &str)] = &[
        (
            "call_certainty.rs",
            "sqlite_path_value",
            "find_shortest_path",
            "R9: certain walk; include_inferred admits inferred hops",
        ),
        (
            "cycle_completeness_audit.rs",
            "cycle_completeness_audit_response",
            "find_cycles_cancellable",
            "R3: the audit keeps the unpartitioned persisted graph",
        ),
        (
            "cycle_completeness_audit.rs",
            "cycle_completeness_audit_response",
            "module_import_cycles_cancellable",
            "R17: the audit's LiveGraph side, unpartitioned",
        ),
        (
            "dispatch.rs",
            "handle_cycles",
            "directory_module_graph",
            "R3b: forced --engine sqlite answers the requested view",
        ),
        (
            "dispatch.rs",
            "handle_imports",
            "find_imports",
            "R7: rows partitioned by class (partition_import_rows)",
        ),
        (
            "dispatch.rs",
            "handle_modules_deps",
            "load_module_graph_facts",
            "R1: the request's view",
        ),
        (
            "dispatch.rs",
            "handle_modules_list",
            "load_module_graph_facts",
            "R1: the request's view; R1c governance counts CERTAIN_WITH_TESTS",
        ),
        (
            "dispatch.rs",
            "handle_modules_show",
            "load_module_graph_facts",
            "R1b: CERTAIN_WITH_TESTS, basis stated",
        ),
        (
            "dispatch.rs",
            "handle_modules_violations",
            "load_module_graph_facts",
            "R1c: CERTAIN_WITH_TESTS, inferred counted not judged",
        ),
        (
            "explain_lg_serve.rs",
            "serve_imports",
            "live_import_view",
            "R17: served only on a GREEN import cert, which a non-static import turns RED",
        ),
        (
            "handlers/governance/violations.rs",
            "handle_violations",
            "find_imports_between_paths",
            "R11: certain judged, inferred counted with files",
        ),
        (
            "handlers/governance/violations.rs",
            "handle_violations",
            "load_module_graph_facts",
            "R1c: CERTAIN_WITH_TESTS",
        ),
        (
            "handlers/map.rs",
            "handle_map",
            "map_resolved_dep_edges_in_path",
            "R10: certain IMPORTS",
        ),
        (
            "handlers/map.rs",
            "handle_map",
            "map_inferred_import_counts_in_path",
            "R10: per-file inferred count",
        ),
        (
            "import_partition_view.rs",
            "sqlite_cycles_body",
            "find_cycles",
            "R3b: the view's directory graph",
        ),
        (
            "import_partition_view.rs",
            "attach_unresolved_imports",
            "find_unresolved_file_imports",
            "R18: the one per-file unresolved attach, after partition_import_rows, on the auto and sqlite answers only",
        ),
        (
            "livegraph_feed.rs",
            "cancellable_module_stats",
            "compute_module_stats",
            "R5: stats directory fans, basis stated",
        ),
        (
            "livegraph_feed.rs",
            "cycles_auto_response",
            "directory_module_graph",
            "R3b/D-TESB-06: the view and the fastpath's fourth precondition",
        ),
        (
            "livegraph_feed.rs",
            "cycles_auto_response",
            "module_import_cycles_cancellable",
            "R17: served only when the view excludes nothing",
        ),
        (
            "livegraph_feed.rs",
            "imports_auto_response",
            "find_imports",
            "R7: the auto fallback; rows partitioned by the handler",
        ),
        (
            "livegraph_feed.rs",
            "imports_auto_response",
            "live_import_view",
            "R17: import-cert gated (RED on any non-static import)",
        ),
        (
            "livegraph_feed.rs",
            "imports_compare_response",
            "find_imports",
            "R7: --engine compare, unpartitioned diagnostic",
        ),
        (
            "livegraph_feed.rs",
            "imports_compare_response",
            "live_import_view",
            "R17: --engine compare, unpartitioned diagnostic",
        ),
        (
            "livegraph_feed.rs",
            "imports_fastpath_or_compare",
            "find_imports",
            "R7: the auto fallback rows",
        ),
        (
            "livegraph_feed.rs",
            "imports_readiness_response",
            "all_imports",
            "R8: the readiness certificate",
        ),
        (
            "livegraph_feed.rs",
            "imports_readiness_response",
            "live_import_view",
            "R8: the readiness certificate's LiveGraph side",
        ),
        (
            "livegraph_feed.rs",
            "imports_view_response",
            "live_import_view",
            "R17: --engine livegraph, unpartitioned",
        ),
        (
            "livegraph_feed.rs",
            "imports_view_response",
            "module_import_cycles",
            "R17: --engine livegraph completeness fields",
        ),
        (
            "livegraph_feed.rs",
            "module_cycle_compare_data_cancellable",
            "find_cycles_cancellable",
            "R3: the cycles certificate/compare, unpartitioned",
        ),
        (
            "livegraph_feed.rs",
            "module_cycle_compare_data_cancellable",
            "module_import_cycles_cancellable",
            "R3: the cycles certificate/compare, unpartitioned",
        ),
        (
            "livegraph_feed.rs",
            "module_import_cycles_response",
            "module_import_cycles_cancellable",
            "R17: --engine livegraph, unpartitioned",
        ),
        (
            "livegraph_feed.rs",
            "stats_auto_response",
            "module_stats",
            "R17/R5: stats (test imports kept, basis stated)",
        ),
        (
            "livegraph_feed.rs",
            "stats_compare_data",
            "module_stats",
            "R17/R5: the stats certificate",
        ),
        (
            "livegraph_feed.rs",
            "stats_livegraph_response",
            "module_stats",
            "R17/R5: --engine livegraph stats",
        ),
        (
            "orient_additive_fields.rs",
            "inject_modules_method",
            "load_module_graph_facts",
            "R1: module identity only (the method line)",
        ),
        (
            "orient_additive_fields.rs",
            "inject_top_module_edges",
            "load_module_graph_facts",
            "R1: the DEFAULT view",
        ),
        (
            "orient_lg_decisions.rs",
            "orient_cycles_outcome",
            "module_import_cycles",
            "R17b: the IMPORT_CYCLES/EXPLAIN_CYCLES corroboration LABEL (not the value); declined \
             first when the default view excludes an import (D-TESB-16)",
        ),
        (
            "orient_lg_decisions.rs",
            "read_default_view_excludes_nothing",
            "directory_module_graph",
            "R17b: the default view's excluded directory-import count behind the cycle leaves' \
             provenance label (D-TESB-16); a failed read is never 'excludes nothing' and is \
             labelled PartitionEvidenceUnreadable with its error (INPUT-3)",
        ),
        (
            "orient_serve/storage_port_impl.rs",
            "default_view_excludes_nothing",
            "import_cycle_partition",
            "D-TESB-06: the decorator's gate",
        ),
        (
            "orient_serve/storage_port_impl.rs",
            "m2_module_cycles",
            "module_import_cycles",
            "R17: served only when the default view excludes nothing",
        ),
        (
            "orient_serve/storage_port_impl.rs",
            "m2_module_cycles_cancellable",
            "module_import_cycles_cancellable",
            "R17: served only when the default view excludes nothing",
        ),
        (
            "trust_coherence.rs",
            "build_posture_leaf",
            "module_stats",
            "R17/R5: the trust posture leaf's stats",
        ),
    ];

    /// Port delegations: the decorator forwards each port read of the same name to its inner port.
    fn is_port_delegation(file: &str, func: &str, api: &str) -> bool {
        file == "orient_serve/storage_port_impl.rs" && func == api
    }

    fn rs_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }

    /// The files that are test code: a module declared `#[cfg(test)] mod x;` (resolved to `x.rs` /
    /// `x/mod.rs` beside the declaring module, or to its `#[path = …]`), and every module declared
    /// inside test code — a fixpoint over the `mod` declarations.
    fn test_module_files(files: &[PathBuf]) -> std::collections::BTreeSet<PathBuf> {
        // (declaring file, child candidates, declared under #[cfg(test)])
        let mut decls: Vec<(PathBuf, Vec<PathBuf>, bool)> = Vec::new();
        for f in files {
            let src = std::fs::read_to_string(f).unwrap();
            let lines: Vec<&str> = src.lines().collect();
            let stem = f.file_stem().unwrap().to_string_lossy().to_string();
            let here = f.parent().unwrap().to_path_buf();
            let dir = if stem == "mod" || stem == "lib" {
                here.clone()
            } else {
                here.join(&stem)
            };
            for (k, l) in lines.iter().enumerate() {
                let decl = l.trim();
                let name = decl
                    .strip_prefix("mod ")
                    .or_else(|| decl.strip_prefix("pub(crate) mod "))
                    .or_else(|| decl.strip_prefix("pub(super) mod "))
                    .or_else(|| decl.strip_prefix("pub mod "))
                    .and_then(|r| r.strip_suffix(';'));
                let Some(name) = name else { continue };
                // The attributes directly above the declaration.
                let mut cfg_test = false;
                let mut path_attr: Option<String> = None;
                let mut j = k;
                while j > 0 && lines[j - 1].trim_start().starts_with("#[") {
                    j -= 1;
                    let a = lines[j].trim();
                    if a.starts_with("#[cfg(test)]") {
                        cfg_test = true;
                    }
                    if let Some(p) = a
                        .strip_prefix("#[path = \"")
                        .and_then(|r| r.strip_suffix("\"]"))
                    {
                        path_attr = Some(p.to_string());
                    }
                }
                let children = match path_attr {
                    Some(p) => vec![here.join(p)],
                    None => vec![
                        dir.join(format!("{name}.rs")),
                        dir.join(name).join("mod.rs"),
                    ],
                };
                decls.push((f.clone(), children, cfg_test));
            }
        }
        let mut out = std::collections::BTreeSet::new();
        loop {
            let before = out.len();
            for (parent, children, cfg_test) in &decls {
                if *cfg_test || out.contains(parent) {
                    out.extend(children.iter().cloned());
                }
            }
            if out.len() == before {
                return out;
            }
        }
    }

    /// The source with `#[cfg(test)]` items (brace-matched) and line comments removed.
    fn production_lines(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut lines = src.lines();
        while let Some(line) = lines.next() {
            let t = line.trim_start();
            if t.starts_with("#[cfg(test)]") {
                let (mut depth, mut opened) = (0i64, false);
                for l in lines.by_ref() {
                    for c in l.chars() {
                        match c {
                            '{' => {
                                depth += 1;
                                opened = true;
                            }
                            '}' => depth -= 1,
                            _ => {}
                        }
                    }
                    if (opened && depth <= 0) || (!opened && l.trim_end().ends_with(';')) {
                        break;
                    }
                }
                continue;
            }
            out.push(if t.starts_with("//") {
                String::new()
            } else {
                line.to_string()
            });
        }
        out
    }

    fn call_of(line: &str, api: &str) -> bool {
        let mut rest = line;
        while let Some(i) = rest.find(api) {
            let before = rest[..i].chars().last();
            let after = rest[i + api.len()..].trim_start();
            let boundary = !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            let is_decl = rest[..i].trim_end().ends_with("fn");
            if boundary && !is_decl && (after.starts_with('(') || after.starts_with("::<")) {
                return true;
            }
            rest = &rest[i + api.len()..];
        }
        false
    }

    fn enclosing_fn(lines: &[String], at: usize) -> Option<String> {
        (0..=at).rev().find_map(|k| {
            let l = &lines[k];
            let i = l.find("fn ")?;
            if i > 0 && l.as_bytes()[i - 1].is_ascii_alphanumeric() {
                return None;
            }
            let name: String = l[i + 3..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
    }

    #[test]
    fn every_daemon_import_read_call_site_is_in_the_inventory() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rs_files(&root, &mut files);
        let tests = test_module_files(&files);
        let listed: std::collections::BTreeSet<(String, String, String)> = DAEMON_CALL_SITES
            .iter()
            .map(|(f, n, a, _)| (f.to_string(), n.to_string(), a.to_string()))
            .collect();
        let mut found = std::collections::BTreeSet::new();
        for f in files.iter().filter(|f| !tests.contains(*f)) {
            let rel = f
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let lines = production_lines(&std::fs::read_to_string(f).unwrap());
            for (n, l) in lines.iter().enumerate() {
                for api in IMPORT_READ_APIS {
                    if call_of(l, api) {
                        let func = enclosing_fn(&lines, n).unwrap_or_default();
                        if !is_port_delegation(&rel, &func, api) {
                            found.insert((rel.clone(), func, api.to_string()));
                        }
                    }
                }
            }
        }
        let unlisted: Vec<_> = found.difference(&listed).collect();
        assert!(
            unlisted.is_empty(),
            "daemon call sites of an IMPORTS reader that are not in the §2.3 inventory (add each \
             with its disposition, or STOP): {unlisted:?}"
        );
        let stale: Vec<_> = listed.difference(&found).collect();
        assert!(
            stale.is_empty(),
            "inventory rows with no call site any more: {stale:?}"
        );
    }
}
