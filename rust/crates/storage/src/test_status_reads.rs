//! TEST-EDGE-SCOPE-1A — plain reads of the facts the UNDETERMINED test status is
//! computed from (RG-REQ-001-L07; D-TESA-DERIVED-1).
//!
//! Nothing here is stored or written: the status is a pure function of a tracked
//! file's path and its stored `files.is_test`
//! (`repo_graph_classification::test_path::undetermined_test_word`), computed when a
//! surface asks. These reads hand the surfaces the rows they need:
//!
//! - [`StorageConnection::query_tracked_file_test_flags`] — the snapshot's tracked
//!   files (`files` ⋈ `file_versions`), each with its stored flag (strict `== 1`,
//!   the `TrackedFile::from_row` mapping) and whether the snapshot holds a FILE node
//!   for it (tracked-only config/contract/unreadable files have none);
//! - [`StorageConnection::query_call_source_file_test_flags`] — the distinct source
//!   files of the snapshot's CALLS edges and CALLS-family unresolved rows, i.e. the
//!   files whose calls `reliability` counts, with the flag its partition uses.

use repo_graph_agent::TrackedFileTestFlag;
use repo_graph_classification::types::UnresolvedEdgeCategory;

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// The four CALLS-family unresolved categories `reliability` counts
/// (`call_resolution_reads::CALLS_CATEGORIES`, the `assemble_trust_report` filter),
/// serialized at query time from the typed enum so the filter tracks the vocabulary.
const CALL_CATEGORIES: [UnresolvedEdgeCategory; 4] = [
    UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo,
    UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext,
    UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
    UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
];

fn category_sql(c: &UnresolvedEdgeCategory) -> Result<String, StorageError> {
    match serde_json::to_value(c) {
        Ok(serde_json::Value::String(s)) => Ok(s),
        _ => Err(StorageError::Sqlite(
            rusqlite::Error::ToSqlConversionFailure(
                "unresolved category did not serialize to a string".into(),
            ),
        )),
    }
}

impl StorageConnection {
    /// The snapshot's tracked files with their stored test flag and FILE-node
    /// presence, sorted by path; `path = Some(p)` restricts to that one file (zero or
    /// one row). A plain read: nothing new is stored (D-TESA-DERIVED-1).
    pub fn query_tracked_file_test_flags(
        &self,
        snapshot_uid: &str,
        path: Option<&str>,
    ) -> Result<Vec<TrackedFileTestFlag>, StorageError> {
        let mut stmt = self.connection().prepare(
            "SELECT f.path, \
                    CASE WHEN f.is_test = 1 THEN 1 ELSE 0 END AS is_test, \
                    EXISTS (SELECT 1 FROM nodes n \
                            WHERE n.snapshot_uid = ?1 AND n.kind = 'FILE' \
                              AND n.file_uid = f.file_uid) AS has_file_node \
             FROM file_versions v \
             JOIN files f ON f.file_uid = v.file_uid \
             WHERE v.snapshot_uid = ?1 AND (?2 IS NULL OR f.path = ?2) \
             ORDER BY f.path ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![snapshot_uid, path], |row| {
            Ok(TrackedFileTestFlag {
                path: row.get::<_, String>(0)?,
                is_test: row.get::<_, i64>(1)? == 1,
                has_file_node: row.get::<_, i64>(2)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    /// The distinct source files of the snapshot's CALLS edges and CALLS-family
    /// unresolved rows — the files whose calls `query_call_resolution_by_language` /
    /// `_by_module` count — each with its `files.is_test` under the SAME mapping those
    /// reads partition by (`COALESCE(is_test, 0) != 0`). A call whose source has no
    /// `files` row has no path and is not a file of this universe (it stays counted
    /// by reliability's `(unknown)` cell). Sorted by path.
    pub fn query_call_source_file_test_flags(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<(String, bool)>, StorageError> {
        let cats = [
            category_sql(&CALL_CATEGORIES[0])?,
            category_sql(&CALL_CATEGORIES[1])?,
            category_sql(&CALL_CATEGORIES[2])?,
            category_sql(&CALL_CATEGORIES[3])?,
        ];
        let mut stmt = self.connection().prepare(
            "SELECT f.path, COALESCE(f.is_test, 0) != 0 AS is_test \
             FROM edges e \
             JOIN nodes n ON n.node_uid = e.source_node_uid \
             JOIN files f ON f.file_uid = n.file_uid \
             WHERE e.snapshot_uid = ?1 AND e.type = 'CALLS' \
             UNION \
             SELECT f.path, COALESCE(f.is_test, 0) != 0 AS is_test \
             FROM unresolved_edges ue \
             JOIN nodes n ON n.node_uid = ue.source_node_uid \
             JOIN files f ON f.file_uid = n.file_uid \
             WHERE ue.snapshot_uid = ?1 AND ue.category IN (?2, ?3, ?4, ?5) \
             ORDER BY 1 ASC",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![snapshot_uid, cats[0], cats[1], cats[2], cats[3]],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0)),
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }
}

#[cfg(test)]
mod tests {
    use repo_graph_agent::{AgentStorageRead, TrackedFileTestFlag};

    use crate::connection::StorageConnection;

    const SNAP: &str = "snap-tesa";

    fn flag(path: &str, is_test: bool, has_file_node: bool) -> TrackedFileTestFlag {
        TrackedFileTestFlag {
            path: path.into(),
            is_test,
            has_file_node,
        }
    }

    /// Two snapshots of one repo. `SNAP` tracks: `util/testutil.cc` (prod, FILE),
    /// `util/testutil.h` (test, FILE), `pom.xml` under `it-tests/` (tracked only, no
    /// FILE), `src/odd.c` (is_test = 2 — out of range, FILE). `snap-other` tracks only
    /// `src/other.c`, which `SNAP` does not.
    fn fixture() -> StorageConnection {
        let storage = StorageConnection::open_in_memory().unwrap();
        storage
            .connection()
            .execute_batch(&format!(
                "INSERT INTO repos (repo_uid, name, root_path, created_at) \
                   VALUES ('r1', 'r', '/tmp/r1', '2024-01-01T00:00:00Z'); \
                 INSERT INTO snapshots (snapshot_uid, repo_uid, status, kind, created_at) VALUES \
                   ('{SNAP}', 'r1', 'ready', 'full', '2024-01-01T00:00:00Z'), \
                   ('snap-other', 'r1', 'ready', 'full', '2024-01-02T00:00:00Z'); \
                 INSERT INTO files (file_uid, repo_uid, path, language, is_test) VALUES \
                   ('r1:util/testutil.cc', 'r1', 'util/testutil.cc', 'cpp', 0), \
                   ('r1:util/testutil.h', 'r1', 'util/testutil.h', 'cpp', 1), \
                   ('r1:it-tests/pom.xml', 'r1', 'it-tests/pom.xml', NULL, 0), \
                   ('r1:src/odd.c', 'r1', 'src/odd.c', 'c', 2), \
                   ('r1:src/other.c', 'r1', 'src/other.c', 'c', 0); \
                 INSERT INTO file_versions (snapshot_uid, file_uid, content_hash, parse_status, indexed_at) VALUES \
                   ('{SNAP}', 'r1:util/testutil.cc', 'h', 'parsed', 't'), \
                   ('{SNAP}', 'r1:util/testutil.h', 'h', 'parsed', 't'), \
                   ('{SNAP}', 'r1:it-tests/pom.xml', 'h', 'skipped', 't'), \
                   ('{SNAP}', 'r1:src/odd.c', 'h', 'parsed', 't'), \
                   ('snap-other', 'r1:src/other.c', 'h', 'parsed', 't'); \
                 INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, qualified_name, file_uid) VALUES \
                   ('f1', '{SNAP}', 'r1', 'r1:util/testutil.cc:FILE', 'FILE', 'SOURCE', 'testutil.cc', NULL, 'r1:util/testutil.cc'), \
                   ('f2', '{SNAP}', 'r1', 'r1:util/testutil.h:FILE', 'FILE', 'SOURCE', 'testutil.h', NULL, 'r1:util/testutil.h'), \
                   ('f3', '{SNAP}', 'r1', 'r1:src/odd.c:FILE', 'FILE', 'SOURCE', 'odd.c', NULL, 'r1:src/odd.c'), \
                   ('fo', 'snap-other', 'r1', 'r1:src/other.c:FILE', 'FILE', 'SOURCE', 'other.c', NULL, 'r1:src/other.c'), \
                   ('m_util', '{SNAP}', 'r1', 'r1:util:MODULE', 'MODULE', NULL, 'util', 'util', NULL), \
                   ('m_src', '{SNAP}', 'r1', 'r1:src:MODULE', 'MODULE', NULL, 'src', 'src', NULL); \
                 INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, type, resolution, extractor) VALUES \
                   ('o1', '{SNAP}', 'r1', 'm_util', 'f1', 'OWNS', 'static', 't'), \
                   ('o2', '{SNAP}', 'r1', 'm_util', 'f2', 'OWNS', 'static', 't'), \
                   ('o3', '{SNAP}', 'r1', 'm_src', 'f3', 'OWNS', 'static', 't');"
            ))
            .unwrap();
        storage
    }

    #[test]
    fn tracked_file_flags_list_every_tracked_file_of_the_snapshot_with_its_flag_and_file_node() {
        let s = fixture();
        assert_eq!(
            s.query_tracked_file_test_flags(SNAP, None).unwrap(),
            vec![
                flag("it-tests/pom.xml", false, false),
                flag("src/odd.c", false, true),
                flag("util/testutil.cc", false, true),
                flag("util/testutil.h", true, true),
            ],
            "every tracked file of THIS snapshot (not src/other.c), sorted, FILE-less included"
        );
        // Through the agent port: the same rows (the one delegating trait method).
        assert_eq!(
            AgentStorageRead::query_tracked_file_test_flags(&s, SNAP, None).unwrap(),
            s.query_tracked_file_test_flags(SNAP, None).unwrap()
        );
    }

    #[test]
    fn tracked_file_flags_filter_by_path() {
        let s = fixture();
        assert_eq!(
            s.query_tracked_file_test_flags(SNAP, Some("it-tests/pom.xml"))
                .unwrap(),
            vec![flag("it-tests/pom.xml", false, false)]
        );
        assert_eq!(
            s.query_tracked_file_test_flags(SNAP, Some("src/other.c"))
                .unwrap(),
            Vec::<TrackedFileTestFlag>::new(),
            "a path the snapshot does not track yields no row"
        );
    }

    #[test]
    fn tracked_file_flags_use_the_strict_is_test_mapping() {
        let s = fixture();
        let rows = s
            .query_tracked_file_test_flags(SNAP, Some("src/odd.c"))
            .unwrap();
        assert_eq!(
            rows,
            vec![flag("src/odd.c", false, true)],
            "is_test = 2 is not test (strict == 1, TrackedFile::from_row)"
        );
    }

    #[test]
    fn call_source_file_flags_are_the_files_whose_calls_reliability_counts() {
        let s = fixture();
        s.connection()
            .execute_batch(&format!(
                "INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, qualified_name, file_uid) VALUES \
                   ('sa', '{SNAP}', 'r1', 'r1:a#sa', 'SYMBOL', 'FUNCTION', 'sa', NULL, 'r1:util/testutil.cc'), \
                   ('sb', '{SNAP}', 'r1', 'r1:b#sb', 'SYMBOL', 'FUNCTION', 'sb', NULL, 'r1:util/testutil.h'), \
                   ('sc', '{SNAP}', 'r1', 'r1:c#sc', 'SYMBOL', 'FUNCTION', 'sc', NULL, 'r1:src/odd.c'), \
                   ('sx', '{SNAP}', 'r1', 'r1:x#sx', 'SYMBOL', 'FUNCTION', 'sx', NULL, NULL); \
                 INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, type, resolution, extractor) VALUES \
                   ('c1', '{SNAP}', 'r1', 'sa', 'sb', 'CALLS', 'static', 't'), \
                   ('c2', '{SNAP}', 'r1', 'sa', 'sb', 'CALLS', 'inferred', 't'), \
                   ('c3', '{SNAP}', 'r1', 'sx', 'sa', 'CALLS', 'static', 't'), \
                   ('i1', '{SNAP}', 'r1', 'sc', 'sa', 'IMPORTS', 'static', 't');"
            ))
            .unwrap();
        let ue = |uid: &str, src: &str, cat: &str| {
            s.connection()
                .execute(
                    "INSERT INTO unresolved_edges \
                     (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key, type, \
                      resolution, extractor, category, classification, classifier_version, \
                      basis_code, observed_at) \
                     VALUES (?, ?, 'r1', ?, 'tk', 'CALLS', 'unresolved', 't', ?, 'unknown', 1, \
                             'no_supporting_signal', '2024-01-01T00:00:00Z')",
                    rusqlite::params![uid, SNAP, src, cat],
                )
                .unwrap();
        };
        // sb (test file) only has an unresolved CALLS row; sc only an IMPORTS-family one.
        ue("u1", "sb", "calls_function_ambiguous_or_missing");
        ue("u2", "sc", "imports_file_not_found");
        assert_eq!(
            s.query_call_source_file_test_flags(SNAP).unwrap(),
            vec![
                ("util/testutil.cc".to_string(), false),
                ("util/testutil.h".to_string(), true),
            ],
            "the CALLS-edge and CALLS-unresolved source files with their flag; an \
             IMPORTS-only file and a file-less caller are not in the universe"
        );
    }

    #[test]
    fn directory_groups_count_stored_test_files() {
        let s = fixture();
        let groups = AgentStorageRead::list_directory_groups(&s, SNAP).unwrap();
        let got: Vec<(String, u64, u64)> = groups
            .into_iter()
            .map(|g| (g.path, g.file_count, g.test_file_count))
            .collect();
        assert_eq!(
            got,
            vec![("src".to_string(), 1, 0), ("util".to_string(), 2, 1)],
            "test_file_count = owned files with stored is_test = 1 (is_test = 2 is not test)"
        );
    }

    #[test]
    fn directory_groups_error_on_an_owned_file_without_a_files_row() {
        let s = fixture();
        s.connection()
            .execute_batch(&format!(
                "INSERT INTO nodes (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, qualified_name, file_uid) VALUES \
                   ('f_orphan', '{SNAP}', 'r1', 'r1:src/ghost.c:FILE', 'FILE', 'SOURCE', 'ghost.c', NULL, NULL); \
                 INSERT INTO edges (edge_uid, snapshot_uid, repo_uid, source_node_uid, target_node_uid, type, resolution, extractor) VALUES \
                   ('o4', '{SNAP}', 'r1', 'm_src', 'f_orphan', 'OWNS', 'static', 't');"
            ))
            .unwrap();
        let err = AgentStorageRead::list_directory_groups(&s, SNAP)
            .expect_err("an owned FILE node with no files row is an unknown test status");
        assert!(
            err.message.contains("src") && err.message.contains("no files row"),
            "the error names the directory: {}",
            err.message
        );
    }
}
