//! TEST-EDGE-SCOPE-1B (D-TESB-02, D-TESB-14): the ONE partitioned import read and the IMPORTS
//! resolution-vocabulary check.
//!
//! [`StorageConnection::file_imports_with_partition`] returns every file→file IMPORTS row of a
//! snapshot with its partition — the resolution class through the classification vocabulary
//! (`repo_graph_classification::import_partition`) and the importing file's stored `is_test`
//! (LEFT JOIN: a missing `files` row is UNKNOWN, never production-by-default and never dropped)
//! — and its type-only disposition through the indexer's one parse
//! (`repo_graph_indexer::type_only::type_only_disposition_of`). It is the row source of every
//! module-edge derivation (module candidates in `module-queries`, gate and trust; directory
//! modules in [`crate::directory_module_edges`]). A resolution outside the vocabulary is a
//! named error, never read as certain and never filtered away.
//!
//! [`StorageConnection::reject_unreadable_import_resolutions`] is the check readers that filter
//! or aggregate IMPORTS in SQL run first (the IMPORTS twin of
//! `reject_unreadable_call_resolutions`); its readable set is the classification constant.

use repo_graph_classification::import_partition::{
    ImportPartition, ImportPartitionError, READABLE_IMPORT_RESOLUTIONS,
};
use repo_graph_indexer::storage_port::TypeOnlyDisposition;
use repo_graph_indexer::type_only::type_only_disposition_of;

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// One file→file IMPORTS row with its partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionedFileImport {
    pub source_file_uid: String,
    pub target_file_uid: String,
    /// The importing file's repo-relative path; `None` when it has no `files` row.
    pub source_path: Option<String>,
    /// The importing file's stored `is_test` (`None` = no `files` row).
    pub source_is_test: Option<bool>,
    pub partition: ImportPartition,
    /// The row's `import type` disposition (TYPE-ONLY-IMPORTS-1), `None` when absent.
    pub type_only: Option<TypeOnlyDisposition>,
}

/// Map a classification error on a stored IMPORTS row to the storage error the readers return.
pub(crate) fn unreadable_import_row(
    reader: &str,
    snapshot_uid: &str,
    e: ImportPartitionError,
) -> StorageError {
    StorageError::SerializationError(format!(
        "{reader}: an IMPORTS row of snapshot {snapshot_uid} is unreadable: {e} — \
         the certain and inferred imports cannot be told apart; run `rmap repo rebuild <path>`"
    ))
}

/// A stored `files.is_test` that is not an integer (D-TESB-17 row W9): named, with the file and
/// the stored value — never read as production.
fn unreadable_importer_flag(snapshot_uid: &str, file_uid: &str, stored: &str) -> StorageError {
    StorageError::SerializationError(format!(
        "file_imports_with_partition: the importing file {file_uid} of snapshot {snapshot_uid} \
         has an unreadable is_test value {stored} (expected 0 or 1) — its imports cannot be \
         placed in the production or test partition; run `rmap repo rebuild <path>`"
    ))
}

/// The SQL list of the CERTAIN resolution values, built from the classification constant (one
/// definition): `('static', 'dynamic')`. Readers that filter certain imports in SQL run
/// [`StorageConnection::reject_unreadable_import_resolutions`] first.
pub(crate) fn certain_import_resolutions_sql() -> String {
    format!(
        "({})",
        repo_graph_classification::import_partition::CERTAIN_IMPORT_RESOLUTIONS
            .iter()
            .map(|r| format!("'{r}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

impl StorageConnection {
    /// Every file→file IMPORTS row of `snapshot_uid` (both endpoint nodes carry a file), with the
    /// importer's path, stored test flag, partition and type-only disposition. Raw multiplicity is
    /// kept (the derivations count imports). Ordered by (source file, target file).
    ///
    /// Fails with a named error on a resolution outside `static | dynamic | inferred` or an
    /// importer flag other than 0/1.
    pub fn file_imports_with_partition(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<PartitionedFileImport>, StorageError> {
        let mut stmt = self.connection().prepare(
            "SELECT src_node.file_uid, tgt_node.file_uid, f.path, f.is_test, e.resolution, \
                    e.metadata_json \
             FROM edges e \
             JOIN nodes src_node ON e.source_node_uid = src_node.node_uid \
             JOIN nodes tgt_node ON e.target_node_uid = tgt_node.node_uid \
             LEFT JOIN files f ON f.file_uid = src_node.file_uid \
             WHERE e.snapshot_uid = ? \
               AND e.type = 'IMPORTS' \
               AND src_node.file_uid IS NOT NULL \
               AND tgt_node.file_uid IS NOT NULL \
             ORDER BY src_node.file_uid ASC, tgt_node.file_uid ASC",
        )?;
        let raw = stmt
            .query_map([snapshot_uid], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, rusqlite::types::Value>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(src, tgt, path, stored_flag, resolution, metadata)| {
                // D-TESB-17 row W9: the stored flag is an integer or absent (no `files` row);
                // any other stored type is a named unreadable error, never production.
                let flag = match stored_flag {
                    rusqlite::types::Value::Null => None,
                    rusqlite::types::Value::Integer(i) => Some(i),
                    rusqlite::types::Value::Real(r) => {
                        return Err(unreadable_importer_flag(snapshot_uid, &src, &r.to_string()))
                    }
                    rusqlite::types::Value::Text(t) => {
                        return Err(unreadable_importer_flag(
                            snapshot_uid,
                            &src,
                            &format!("{t:?}"),
                        ))
                    }
                    rusqlite::types::Value::Blob(_) => {
                        return Err(unreadable_importer_flag(snapshot_uid, &src, "a blob"))
                    }
                };
                let partition = ImportPartition::classify(&resolution, flag).map_err(|e| {
                    unreadable_import_row("file_imports_with_partition", snapshot_uid, e)
                })?;
                Ok(PartitionedFileImport {
                    source_file_uid: src,
                    target_file_uid: tgt,
                    source_path: path,
                    source_is_test: flag.map(|f| f == 1),
                    partition,
                    type_only: type_only_disposition_of(metadata.as_deref()),
                })
            })
            .collect()
    }

    /// Fail with a named error when any IMPORTS edge of `snapshot_uid` carries a `resolution`
    /// outside [`READABLE_IMPORT_RESOLUTIONS`] (D-PSI-R1-VOCAB, D-TESB-14).
    ///
    /// Readers that filter or aggregate IMPORTS in SQL (`path`'s walk, the persisted-graph cycle
    /// and statistics reads, trust's path-prefix rule) call this first, so such a value is refused
    /// with its count and one example instead of being filtered away (a measured absence) or
    /// counted as certain. `reader` names the read in the error.
    pub fn reject_unreadable_import_resolutions(
        &self,
        snapshot_uid: &str,
        reader: &str,
    ) -> Result<(), StorageError> {
        let readable = READABLE_IMPORT_RESOLUTIONS
            .iter()
            .map(|r| format!("'{r}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let (count, example): (i64, Option<String>) = self.connection().query_row(
            &format!(
                "SELECT COUNT(*), MIN(resolution) FROM edges \
                 WHERE snapshot_uid = ?1 AND type = 'IMPORTS' \
                   AND resolution NOT IN ({readable})"
            ),
            rusqlite::params![snapshot_uid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if count == 0 {
            return Ok(());
        }
        Err(StorageError::SerializationError(format!(
            "{reader}: {count} IMPORTS edge(s) of snapshot {snapshot_uid} carry an unreadable \
             resolution (e.g. {:?}; readable: {}) — the certain and inferred imports \
             cannot be told apart; run `rmap repo rebuild <path>`",
            example.unwrap_or_default(),
            READABLE_IMPORT_RESOLUTIONS.join(" | ")
        )))
    }
}

/// The distinct target file paths `file_path` imports through IMPORTS edges of `class`, sorted
/// (`explain <file>`'s Imports section and its inferred remainder). Refuses an out-of-vocabulary
/// resolution first, so a SQL filter never drops one silently.
pub(crate) fn file_import_targets(
    conn: &StorageConnection,
    snapshot_uid: &str,
    file_path: &str,
    class: repo_graph_classification::import_partition::ImportClass,
) -> Result<Vec<repo_graph_agent::AgentImportEntry>, StorageError> {
    use repo_graph_classification::import_partition::ImportClass;
    conn.reject_unreadable_import_resolutions(snapshot_uid, "find_file_imports")?;
    let resolutions = match class {
        ImportClass::Certain => certain_import_resolutions_sql(),
        ImportClass::Inferred => "('inferred')".to_string(),
    };
    let mut stmt = conn.connection().prepare(&format!(
        "SELECT DISTINCT tgt_f.path AS target_file \
         FROM edges e \
         JOIN nodes src_n ON e.source_node_uid = src_n.node_uid \
         JOIN files src_f ON src_n.file_uid = src_f.file_uid \
         JOIN nodes tgt_n ON e.target_node_uid = tgt_n.node_uid \
         JOIN files tgt_f ON tgt_n.file_uid = tgt_f.file_uid \
         WHERE e.snapshot_uid = ? AND e.type = 'IMPORTS' AND src_f.path = ? \
           AND e.resolution IN {resolutions} \
         ORDER BY tgt_f.path ASC"
    ))?;
    let rows = stmt.query_map(rusqlite::params![snapshot_uid, file_path], |row| {
        Ok(repo_graph_agent::AgentImportEntry {
            target_file: row.get(0)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::crud::test_helpers::{fresh_storage, make_edge, make_file, make_node, make_repo};
    use crate::types::{CreateSnapshotInput, GraphNode};
    use repo_graph_classification::import_partition::{ImportClass, ImporterStatus};

    pub(crate) fn setup_test_snapshot(conn: &StorageConnection) -> (String, String) {
        let repo = make_repo("test-repo");
        conn.add_repo(&repo).expect("add repo");
        let snapshot = conn
            .create_snapshot(&CreateSnapshotInput {
                repo_uid: repo.repo_uid.clone(),
                kind: "full".to_string(),
                basis_ref: None,
                basis_commit: None,
                parent_snapshot_uid: None,
                label: None,
                toolchain_json: None,
            })
            .expect("create snapshot");
        (repo.repo_uid, snapshot.snapshot_uid)
    }

    /// Per module candidate path, its (fan_in, fan_out) over the module-edge derivation's
    /// DEFAULT view — the `modules deps` computation, for seam tests.
    pub(crate) fn default_view_fans(
        conn: &StorageConnection,
        snapshot_uid: &str,
    ) -> std::collections::HashMap<String, (u64, u64)> {
        use repo_graph_classification::import_partition::ImportView;
        use repo_graph_classification::module_edges::{
            derive_module_dependency_edges_in_view, FileOwnershipFact, ModuleEdgeDerivationInput,
            ModuleRef, ResolvedImportFact,
        };
        let candidates = conn
            .get_module_candidates_for_snapshot(snapshot_uid)
            .unwrap();
        let result = derive_module_dependency_edges_in_view(
            ModuleEdgeDerivationInput {
                imports: conn
                    .file_imports_with_partition(snapshot_uid)
                    .unwrap()
                    .into_iter()
                    .map(|i| ResolvedImportFact {
                        source_file_uid: i.source_file_uid,
                        target_file_uid: i.target_file_uid,
                        partition: i.partition,
                    })
                    .collect(),
                ownership: conn
                    .get_file_ownership_for_snapshot(snapshot_uid)
                    .unwrap()
                    .into_iter()
                    .map(|o| FileOwnershipFact {
                        file_uid: o.file_uid,
                        module_uid: o.module_candidate_uid,
                    })
                    .collect(),
                modules: candidates
                    .iter()
                    .map(|m| ModuleRef {
                        module_uid: m.module_candidate_uid.clone(),
                        canonical_path: m.canonical_root_path.clone(),
                    })
                    .collect(),
            },
            ImportView::DEFAULT,
        )
        .unwrap();
        let mut out: std::collections::HashMap<String, (u64, u64)> = Default::default();
        for e in &result.edges {
            out.entry(e.source_canonical_path.clone()).or_default().1 += 1;
            out.entry(e.target_canonical_path.clone()).or_default().0 += 1;
        }
        out
    }

    fn file_node(uid: &str, snapshot_uid: &str, repo_uid: &str, file_uid: &str) -> GraphNode {
        let mut n = make_node(
            uid,
            snapshot_uid,
            repo_uid,
            &format!("{uid}:FILE"),
            file_uid,
            uid,
        );
        n.kind = "FILE".to_string();
        n.subtype = None;
        n
    }

    /// Two files (`src/a.ts` production, `test/a.test.ts` test) importing `src/b.ts`; returns
    /// (repo, snapshot, file uids a, t, b).
    fn two_importers(conn: &mut StorageConnection) -> (String, String, String, String, String) {
        let (repo_uid, snapshot_uid) = setup_test_snapshot(conn);
        let a = make_file(&repo_uid, "src/a.ts");
        let mut t = make_file(&repo_uid, "test/a.test.ts");
        t.is_test = true;
        let b = make_file(&repo_uid, "src/b.ts");
        conn.upsert_files(&[a.clone(), t.clone(), b.clone()])
            .unwrap();
        conn.insert_nodes(&[
            file_node("na", &snapshot_uid, &repo_uid, &a.file_uid),
            file_node("nt", &snapshot_uid, &repo_uid, &t.file_uid),
            file_node("nb", &snapshot_uid, &repo_uid, &b.file_uid),
        ])
        .unwrap();
        (repo_uid, snapshot_uid, a.file_uid, t.file_uid, b.file_uid)
    }

    fn imports_edge(
        uid: &str,
        snap: &str,
        repo: &str,
        s: &str,
        t: &str,
        res: &str,
    ) -> crate::types::GraphEdge {
        let mut e = make_edge(uid, snap, repo, s, t);
        e.edge_type = "IMPORTS".to_string();
        e.resolution = res.to_string();
        e
    }

    #[test]
    fn file_imports_with_partition_tag_each_row_with_its_class_and_importer_status() {
        let mut conn = fresh_storage();
        let (repo, snap, a, t, b) = two_importers(&mut conn);
        let mut typed = imports_edge("e1", &snap, &repo, "na", "nb", "static");
        typed.metadata_json = Some(r#"{"isTypeOnly":true}"#.to_string());
        conn.insert_edges(&[
            typed,
            imports_edge("e2", &snap, &repo, "nt", "nb", "dynamic"),
            imports_edge("e3", &snap, &repo, "na", "nb", "inferred"),
        ])
        .unwrap();
        let rows = conn.file_imports_with_partition(&snap).unwrap();
        assert_eq!(rows.len(), 3);
        let of = |src: &str, class: ImportClass| {
            rows.iter()
                .find(|r| r.source_file_uid == src && r.partition.class == class)
                .cloned()
                .expect("row")
        };
        let a_static = of(&a, ImportClass::Certain);
        assert_eq!(a_static.partition.status, ImporterStatus::Production);
        assert_eq!(a_static.source_path.as_deref(), Some("src/a.ts"));
        assert_eq!(a_static.source_is_test, Some(false));
        assert_eq!(a_static.target_file_uid, b);
        assert_eq!(a_static.type_only, Some(TypeOnlyDisposition::TypeOnly));
        let t_dynamic = of(&t, ImportClass::Certain);
        assert_eq!(t_dynamic.partition.status, ImporterStatus::Test);
        assert_eq!(t_dynamic.source_is_test, Some(true));
        assert_eq!(t_dynamic.type_only, None);
        let a_inferred = of(&a, ImportClass::Inferred);
        assert_eq!(a_inferred.partition.status, ImporterStatus::Production);
    }

    #[test]
    fn file_imports_with_partition_error_on_a_resolution_outside_the_vocabulary() {
        let mut conn = fresh_storage();
        let (repo, snap, _, _, _) = two_importers(&mut conn);
        conn.insert_edges(&[imports_edge("e1", &snap, &repo, "na", "nb", "resolved")])
            .unwrap();
        let err = conn
            .file_imports_with_partition(&snap)
            .expect_err("an out-of-vocabulary resolution is refused");
        let msg = err.to_string();
        assert!(msg.contains("\"resolved\""), "names the value: {msg}");
        assert!(
            msg.contains("file_imports_with_partition"),
            "names the reader: {msg}"
        );
    }

    /// D-TESB-17 row W9: an importer flag stored as a non-integer (text, a real) is a named
    /// unreadable error naming the reader and the value — never production by default.
    #[test]
    fn file_imports_with_partition_error_on_a_non_integer_importer_flag() {
        for stored in ["'yes'", "1.5"] {
            let mut conn = fresh_storage();
            let (repo, snap, a, _, _) = two_importers(&mut conn);
            conn.insert_edges(&[imports_edge("e1", &snap, &repo, "na", "nb", "static")])
                .unwrap();
            conn.connection()
                .execute(
                    &format!("UPDATE files SET is_test = {stored} WHERE file_uid = ?1"),
                    [&a],
                )
                .unwrap();
            let err = conn
                .file_imports_with_partition(&snap)
                .expect_err("a non-integer importer flag is refused");
            let msg = err.to_string();
            assert!(
                msg.contains("file_imports_with_partition"),
                "names the reader: {msg}"
            );
            assert!(msg.contains("is_test"), "names the field: {msg}");
            let shown = stored.trim_matches('\'');
            assert!(msg.contains(shown), "names the value {shown}: {msg}");
        }
    }

    #[test]
    fn file_imports_with_partition_keep_an_importer_without_a_files_row_as_unknown() {
        let mut conn = fresh_storage();
        let (repo, snap, _, _, b) = two_importers(&mut conn);
        // A FILE node whose file has no `files` row: the schema's foreign key forbids it, so the
        // orphan is simulated with the check off (a store written by another source).
        conn.connection()
            .execute_batch("PRAGMA foreign_keys = OFF;")
            .unwrap();
        conn.insert_nodes(&[file_node("ng", &snap, &repo, "test-repo:gen/ghost.ts")])
            .unwrap();
        conn.insert_edges(&[imports_edge("e1", &snap, &repo, "ng", "nb", "static")])
            .unwrap();
        let rows = conn.file_imports_with_partition(&snap).unwrap();
        assert_eq!(rows.len(), 1, "never dropped");
        assert_eq!(rows[0].partition.status, ImporterStatus::Unknown);
        assert_eq!(rows[0].source_path, None);
        assert_eq!(rows[0].source_is_test, None);
        assert_eq!(rows[0].target_file_uid, b);
    }

    #[test]
    fn reject_unreadable_import_resolutions_names_the_reader_and_the_value() {
        let mut conn = fresh_storage();
        let (repo, snap, _, _, _) = two_importers(&mut conn);
        conn.insert_edges(&[imports_edge("e1", &snap, &repo, "na", "nb", "static")])
            .unwrap();
        conn.reject_unreadable_import_resolutions(&snap, "some_reader")
            .expect("readable");
        conn.insert_edges(&[imports_edge("e2", &snap, &repo, "nt", "nb", "resolved")])
            .unwrap();
        let msg = conn
            .reject_unreadable_import_resolutions(&snap, "some_reader")
            .expect_err("unreadable")
            .to_string();
        assert!(msg.contains("some_reader: 1 IMPORTS edge(s)"), "{msg}");
        assert!(msg.contains("\"resolved\""), "{msg}");
        assert!(msg.contains("static | dynamic | inferred"), "{msg}");
        // A CALLS edge with an odd value is not this check's business.
        let mut calls = make_edge("e3", &snap, &repo, "na", "nb");
        calls.resolution = "weird".to_string();
        let mut conn2 = fresh_storage();
        let (repo2, snap2, _, _, _) = two_importers(&mut conn2);
        calls.snapshot_uid = snap2.clone();
        calls.repo_uid = repo2;
        conn2.insert_edges(&[calls]).unwrap();
        conn2
            .reject_unreadable_import_resolutions(&snap2, "some_reader")
            .expect("only IMPORTS rows are checked");
    }

    // ── The five identities of the replaced `get_resolved_imports_*` tests ──────────

    #[test]
    fn file_imports_with_partition_return_empty_for_empty_snapshot() {
        let conn = fresh_storage();
        let (_, snap) = setup_test_snapshot(&conn);
        assert!(conn.file_imports_with_partition(&snap).unwrap().is_empty());
    }

    /// Replaces `get_resolved_imports_returns_only_resolved_imports`, whose static-only
    /// expectation this slice reverses: every readable resolution is returned, tagged.
    #[test]
    fn file_imports_with_partition_return_every_resolved_file_import() {
        let mut conn = fresh_storage();
        let (repo, snap, a, _, b) = two_importers(&mut conn);
        conn.insert_edges(&[
            imports_edge("e1", &snap, &repo, "na", "nb", "static"),
            imports_edge("e2", &snap, &repo, "na", "nb", "dynamic"),
            imports_edge("e3", &snap, &repo, "na", "nb", "inferred"),
        ])
        .unwrap();
        let rows = conn.file_imports_with_partition(&snap).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .all(|r| r.source_file_uid == a && r.target_file_uid == b));
        let inferred = rows
            .iter()
            .filter(|r| r.partition.class == ImportClass::Inferred)
            .count();
        assert_eq!(inferred, 1);
    }

    /// An unresolved import lives in `unresolved_edges`, never in `edges`: it is not a row here.
    #[test]
    fn file_imports_with_partition_exclude_unresolved_edges() {
        let mut conn = fresh_storage();
        let (repo, snap, _, _, _) = two_importers(&mut conn);
        conn.connection()
            .execute(
                "INSERT INTO unresolved_edges \
                 (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, \
                  resolution, extractor, category, classification, classifier_version, \
                  basis_code, observed_at) \
                 VALUES ('u1', ?, ?, 'na', './missing', 'IMPORTS', 'unresolved', 't', \
                  'imports_file_not_found', 'unknown', 1, 'no_supporting_signal', \
                  '2024-01-01T00:00:00Z')",
                rusqlite::params![snap, repo],
            )
            .unwrap();
        assert!(conn.file_imports_with_partition(&snap).unwrap().is_empty());
    }

    #[test]
    fn file_imports_with_partition_exclude_non_import_edges() {
        let mut conn = fresh_storage();
        let (repo, snap, _, _, _) = two_importers(&mut conn);
        let mut calls = make_edge("e1", &snap, &repo, "na", "nb");
        calls.edge_type = "CALLS".to_string();
        conn.insert_edges(&[calls]).unwrap();
        assert!(conn.file_imports_with_partition(&snap).unwrap().is_empty());
    }

    #[test]
    fn file_imports_with_partition_preserve_multiplicity() {
        // Raw import edges without deduplication: the derivation counts imports.
        let mut conn = fresh_storage();
        let (repo_uid, snapshot_uid) = setup_test_snapshot(&conn);
        let file_a = make_file(&repo_uid, "src/a.ts");
        let file_b = make_file(&repo_uid, "src/b.ts");
        conn.upsert_files(&[file_a.clone(), file_b.clone()])
            .unwrap();
        conn.insert_nodes(&[
            make_node(
                "node-a1",
                &snapshot_uid,
                &repo_uid,
                "key-a1",
                &file_a.file_uid,
                "fn1",
            ),
            make_node(
                "node-a2",
                &snapshot_uid,
                &repo_uid,
                "key-a2",
                &file_a.file_uid,
                "fn2",
            ),
            make_node(
                "node-b",
                &snapshot_uid,
                &repo_uid,
                "key-b",
                &file_b.file_uid,
                "b",
            ),
        ])
        .unwrap();
        conn.insert_edges(&[
            imports_edge(
                "edge-1",
                &snapshot_uid,
                &repo_uid,
                "node-a1",
                "node-b",
                "static",
            ),
            imports_edge(
                "edge-2",
                &snapshot_uid,
                &repo_uid,
                "node-a2",
                "node-b",
                "static",
            ),
        ])
        .unwrap();
        let rows = conn.file_imports_with_partition(&snapshot_uid).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.source_file_uid == file_a.file_uid));
        assert!(rows.iter().all(|r| r.target_file_uid == file_b.file_uid));
    }

    // ── Every reader carries or checks the class (D-TESB-14) ─────────────────────────

    /// `src/a.ts` imports `src/b.ts` statically and `lib/c.ts` inferred; `test/a.test.ts` imports
    /// `src/b.ts`. Returns (repo, snapshot).
    fn mixed_imports(conn: &mut StorageConnection) -> (String, String) {
        let (repo, snap, _, _, _) = two_importers(conn);
        let c = make_file(&repo, "lib/c.ts");
        conn.upsert_files(std::slice::from_ref(&c)).unwrap();
        conn.insert_nodes(&[file_node("nc", &snap, &repo, &c.file_uid)])
            .unwrap();
        conn.insert_edges(&[
            imports_edge("e1", &snap, &repo, "na", "nb", "static"),
            imports_edge("e2", &snap, &repo, "na", "nc", "inferred"),
            imports_edge("e3", &snap, &repo, "nt", "nb", "static"),
        ])
        .unwrap();
        (repo, snap)
    }

    fn add_unreadable(conn: &mut StorageConnection, repo: &str, snap: &str) {
        conn.insert_edges(&[imports_edge("bad", snap, repo, "nt", "nc", "resolved")])
            .unwrap();
    }

    #[test]
    fn find_imports_rows_carry_a_checked_resolution_class() {
        let mut conn = fresh_storage();
        let (repo, snap) = mixed_imports(&mut conn);
        let rows = conn.find_imports(&snap, "na:FILE").unwrap();
        assert_eq!(rows.len(), 2);
        let classes: Vec<ImportClass> = rows
            .iter()
            .map(|r| r.checked_import_class().unwrap())
            .collect();
        assert!(classes.contains(&ImportClass::Certain));
        assert!(classes.contains(&ImportClass::Inferred));
        add_unreadable(&mut conn, &repo, &snap);
        let err = conn
            .find_imports(&snap, "nt:FILE")
            .expect_err("unreadable row refused");
        assert!(err.to_string().contains("\"resolved\""), "{err}");
    }

    #[test]
    fn find_imports_between_paths_rows_carry_a_checked_resolution_class() {
        let mut conn = fresh_storage();
        let (repo, snap) = mixed_imports(&mut conn);
        let rows = conn
            .find_imports_between_paths(&snap, "src", "lib")
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].import_class, ImportClass::Inferred);
        let rows = conn
            .find_imports_between_paths(&snap, "src", "src")
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].import_class, ImportClass::Certain);
        add_unreadable(&mut conn, &repo, &snap);
        assert!(conn
            .find_imports_between_paths(&snap, "test", "lib")
            .is_err());
    }

    #[test]
    fn find_file_imports_rows_carry_a_checked_resolution_class() {
        use repo_graph_agent::AgentStorageRead;
        let mut conn = fresh_storage();
        let (repo, snap) = mixed_imports(&mut conn);
        let certain = AgentStorageRead::find_file_imports(&conn, &snap, "src/a.ts").unwrap();
        assert_eq!(
            certain
                .iter()
                .map(|e| e.target_file.as_str())
                .collect::<Vec<_>>(),
            vec!["src/b.ts"]
        );
        let inferred =
            AgentStorageRead::find_inferred_file_imports(&conn, &snap, "src/a.ts").unwrap();
        assert_eq!(
            inferred
                .iter()
                .map(|e| e.target_file.as_str())
                .collect::<Vec<_>>(),
            vec!["lib/c.ts"]
        );
        // The file is the subject: a test file keeps its own imports.
        let test_file =
            AgentStorageRead::find_file_imports(&conn, &snap, "test/a.test.ts").unwrap();
        assert_eq!(test_file.len(), 1);
        add_unreadable(&mut conn, &repo, &snap);
        assert!(AgentStorageRead::find_file_imports(&conn, &snap, "src/a.ts").is_err());
    }

    #[test]
    fn map_dependency_edges_keep_certain_imports_and_count_inferred_ones() {
        let mut conn = fresh_storage();
        let (repo, snap) = mixed_imports(&mut conn);
        let edges = conn.map_resolved_dep_edges_in_path(&snap, "").unwrap();
        let imports: Vec<(&str, &str)> = edges
            .iter()
            .filter(|e| e.edge_type == "IMPORTS")
            .map(|e| (e.source_file.as_str(), e.target_file.as_str()))
            .collect();
        assert_eq!(
            imports,
            vec![("src/a.ts", "src/b.ts"), ("test/a.test.ts", "src/b.ts")]
        );
        let inferred = conn.map_inferred_import_counts_in_path(&snap, "").unwrap();
        assert_eq!(inferred.get("src/a.ts"), Some(&1));
        assert_eq!(inferred.len(), 1);
        add_unreadable(&mut conn, &repo, &snap);
        assert!(conn.map_resolved_dep_edges_in_path(&snap, "").is_err());
        assert!(conn.map_inferred_import_counts_in_path(&snap, "").is_err());
    }

    #[test]
    fn gate_boundary_imports_carry_the_resolution_class() {
        use repo_graph_gate::GateStorageRead;
        let mut conn = fresh_storage();
        let (_, snap) = mixed_imports(&mut conn);
        let certain = GateStorageRead::find_boundary_imports(&conn, &snap, "src", "lib").unwrap();
        assert!(certain.is_empty(), "the only src→lib import is inferred");
        let inferred =
            GateStorageRead::find_inferred_boundary_imports(&conn, &snap, "src", "lib").unwrap();
        assert_eq!(inferred.len(), 1);
        assert_eq!(inferred[0].source_file, "src/a.ts");
        // A test file's certain import is judged (governance keeps test imports).
        let test_imports =
            GateStorageRead::find_boundary_imports(&conn, &snap, "test", "src").unwrap();
        assert_eq!(test_imports.len(), 1);
    }

    #[test]
    fn gate_module_violations_read_certain_imports_with_tests() {
        use repo_graph_gate::GateStorageRead;
        let mut conn = fresh_storage();
        let (repo, snap) = mixed_imports(&mut conn);
        // Candidates: `test` forbids `src` (certain, from a test file — judged), `src` forbids
        // `lib` (inferred only — counted, not judged).
        for (m, root) in [("mc_src", "src"), ("mc_lib", "lib"), ("mc_test", "test")] {
            conn.connection()
                .execute(
                    "INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, \
                     module_key, module_kind, canonical_root_path, confidence) \
                     VALUES (?, ?, ?, ?, 'directory', ?, 1.0)",
                    rusqlite::params![m, snap, repo, format!("dir:{root}"), root],
                )
                .unwrap();
        }
        for (file, m) in [
            ("src/a.ts", "mc_src"),
            ("src/b.ts", "mc_src"),
            ("lib/c.ts", "mc_lib"),
            ("test/a.test.ts", "mc_test"),
        ] {
            conn.connection()
                .execute(
                    "INSERT INTO module_file_ownership (snapshot_uid, repo_uid, file_uid, \
                     module_candidate_uid, assignment_kind, confidence) \
                     VALUES (?, ?, ?, ?, 'directory', 1.0)",
                    rusqlite::params![snap, repo, format!("{repo}:{file}"), m],
                )
                .unwrap();
        }
        for (from, forbids) in [("test", "src"), ("src", "lib")] {
            conn.insert_declaration(&crate::crud::declarations::DeclarationInsert {
                identity_key: format!("discovered_module:{repo}:{from}:{forbids}"),
                repo_uid: repo.clone(),
                target_stable_key: format!("{repo}:{from}:MODULE"),
                kind: "boundary".to_string(),
                value_json: serde_json::json!({
                    "selectorDomain": "discovered_module",
                    "source": { "canonicalRootPath": from },
                    "forbids": { "canonicalRootPath": forbids },
                })
                .to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                created_by: Some("test".to_string()),
                supersedes_uid: None,
                authored_basis_json: None,
            })
            .unwrap();
        }
        let evidence = GateStorageRead::evaluate_module_violations(&conn, &repo, &snap).unwrap();
        assert_eq!(evidence.violations_count, 1, "test → src is judged");
        assert_eq!(evidence.inferred_not_judged.len(), 1);
        assert_eq!(evidence.inferred_not_judged[0].source_module, "src");
        assert_eq!(evidence.inferred_not_judged[0].target_module, "lib");
        assert_eq!(evidence.inferred_not_judged[0].import_count, 1);
    }

    #[test]
    fn find_shortest_path_fails_loudly_on_an_imports_resolution_outside_the_vocabulary() {
        let mut conn = fresh_storage();
        let (repo, snap) = mixed_imports(&mut conn);
        conn.find_shortest_path(&snap, "na:FILE", "nb:FILE", 3, false)
            .expect("readable store walks");
        add_unreadable(&mut conn, &repo, &snap);
        let err = conn
            .find_shortest_path(&snap, "na:FILE", "nb:FILE", 3, false)
            .expect_err("never filtered away silently");
        assert!(err.to_string().contains("find_shortest_path"), "{err}");
        assert!(err.to_string().contains("IMPORTS"), "{err}");
    }

    #[test]
    fn persisted_graph_readers_fail_loudly_on_an_imports_resolution_outside_the_vocabulary() {
        use repo_graph_trust::storage_port::TrustStorageRead;
        let mut conn = fresh_storage();
        let (repo, snap) = mixed_imports(&mut conn);
        add_unreadable(&mut conn, &repo, &snap);
        // One module candidate owning a file, so trust's module read reaches the imports.
        conn.connection()
            .execute(
                "INSERT INTO module_candidates (module_candidate_uid, snapshot_uid, repo_uid, \
                 module_key, module_kind, canonical_root_path, confidence) \
                 VALUES ('mc_src', ?, ?, 'dir:src', 'directory', 'src', 1.0)",
                rusqlite::params![snap, repo],
            )
            .unwrap();
        conn.connection()
            .execute(
                "INSERT INTO module_file_ownership (snapshot_uid, repo_uid, file_uid, \
                 module_candidate_uid, assignment_kind, confidence) \
                 VALUES (?, ?, ?, 'mc_src', 'directory', 1.0)",
                rusqlite::params![snap, repo, format!("{repo}:src/a.ts")],
            )
            .unwrap();
        let msg = |e: StorageError| e.to_string();
        assert!(msg(conn.find_cycles(&snap, "module").unwrap_err()).contains("find_cycles"));
        assert!(msg(conn.compute_module_stats(&snap).unwrap_err()).contains("compute_module_stats"));
        assert!(
            msg(TrustStorageRead::find_path_prefix_module_cycles(&conn, &snap).unwrap_err())
                .contains("find_path_prefix_module_cycles")
        );
        assert!(
            msg(TrustStorageRead::compute_module_stats(&conn, &snap).unwrap_err())
                .contains("unreadable")
        );
    }
}
