//! `PolicyFactsStorageRead` and `PolicyFactsStorageWrite` implementation
//! for `StorageConnection`.
//!
//! This module implements the policy-facts storage port traits on top of
//! the storage adapter's rusqlite connection.
//!
//! PF-1 scope: STATUS_MAPPING facts only.
//!
//! **Schema:** Uses the `status_mappings` table added by migration 021.
//!
//! **Error handling:** All methods propagate errors through the
//! `PolicyFactsStorageError` type defined by the policy-facts crate.

use repo_graph_policy_facts::{
    CaseMapping, PolicyFactsStorageError, PolicyFactsStorageRead, PolicyFactsStorageWrite,
    StatusMapping,
};
use rusqlite::params;
use uuid::Uuid;

use crate::connection::StorageConnection;

impl PolicyFactsStorageWrite for StorageConnection {
    fn insert_status_mappings(
        &mut self,
        snapshot_uid: &str,
        mappings: &[StatusMapping],
    ) -> Result<usize, PolicyFactsStorageError> {
        // Verify snapshot exists before starting transaction.
        let snapshot_exists: bool = self
            .connection()
            .query_row(
                "SELECT 1 FROM snapshots WHERE snapshot_uid = ?",
                [snapshot_uid],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !snapshot_exists {
            return Err(PolicyFactsStorageError::SnapshotNotFound(
                snapshot_uid.to_string(),
            ));
        }

        // Atomic replace: delete + insert in a single transaction.
        // If any insert or JSON serialization fails, the entire operation
        // rolls back and the previous state is preserved.
        let tx = self
            .connection_mut()
            .transaction()
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        // Delete existing mappings for this snapshot.
        tx.execute(
            "DELETE FROM status_mappings WHERE snapshot_uid = ?",
            [snapshot_uid],
        )
        .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        // Insert new mappings.
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO status_mappings (
                        uid, snapshot_uid, symbol_key, function_name, file_path,
                        line_start, line_end, source_type, target_type,
                        mappings_json, default_output
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

            for mapping in mappings {
                let uid = Uuid::new_v4().to_string();
                let mappings_json = serde_json::to_string(&mapping.mappings)
                    .map_err(|e| PolicyFactsStorageError::JsonError(e.to_string()))?;

                stmt.execute(params![
                    uid,
                    snapshot_uid,
                    mapping.symbol_key,
                    mapping.function_name,
                    mapping.file_path,
                    mapping.line_start,
                    mapping.line_end,
                    mapping.source_type,
                    mapping.target_type,
                    mappings_json,
                    mapping.default_output,
                ])
                .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        Ok(mappings.len())
    }
}

impl PolicyFactsStorageRead for StorageConnection {
    fn query_status_mappings(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
    ) -> Result<Vec<StatusMapping>, PolicyFactsStorageError> {
        let conn = self.connection();

        let (sql, params): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match file_filter {
            Some(prefix) => (
                "SELECT symbol_key, function_name, file_path, line_start, line_end,
                        source_type, target_type, mappings_json, default_output
                 FROM status_mappings
                 WHERE snapshot_uid = ? AND file_path LIKE ?
                 ORDER BY file_path, function_name",
                vec![
                    Box::new(snapshot_uid.to_string()),
                    Box::new(format!("{}%", prefix)),
                ],
            ),
            None => (
                "SELECT symbol_key, function_name, file_path, line_start, line_end,
                        source_type, target_type, mappings_json, default_output
                 FROM status_mappings
                 WHERE snapshot_uid = ?
                 ORDER BY file_path, function_name",
                vec![Box::new(snapshot_uid.to_string())],
            ),
        };

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let symbol_key: String = row.get(0)?;
                let function_name: String = row.get(1)?;
                let file_path: String = row.get(2)?;
                let line_start: u32 = row.get(3)?;
                let line_end: u32 = row.get(4)?;
                let source_type: String = row.get(5)?;
                let target_type: String = row.get(6)?;
                let mappings_json: String = row.get(7)?;
                let default_output: Option<String> = row.get(8)?;

                Ok((
                    symbol_key,
                    function_name,
                    file_path,
                    line_start,
                    line_end,
                    source_type,
                    target_type,
                    mappings_json,
                    default_output,
                ))
            })
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        let mut results = Vec::new();
        for row_result in rows {
            let (
                symbol_key,
                function_name,
                file_path,
                line_start,
                line_end,
                source_type,
                target_type,
                mappings_json,
                default_output,
            ) = row_result.map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

            let mappings: Vec<CaseMapping> = serde_json::from_str(&mappings_json)
                .map_err(|e| PolicyFactsStorageError::JsonError(e.to_string()))?;

            results.push(StatusMapping {
                symbol_key,
                function_name,
                file_path,
                line_start,
                line_end,
                source_type,
                target_type,
                mappings,
                default_output,
            });
        }

        Ok(results)
    }

    fn count_status_mappings(&self, snapshot_uid: &str) -> Result<usize, PolicyFactsStorageError> {
        let conn = self.connection();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM status_mappings WHERE snapshot_uid = ?",
                [snapshot_uid],
                |row| row.get(0),
            )
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageConnection;

    fn create_test_db() -> StorageConnection {
        let mut conn = StorageConnection::open_in_memory().unwrap();

        // Create a test repo and snapshot.
        conn.connection_mut()
            .execute_batch(
                "INSERT INTO repos (repo_uid, name, root_path, created_at)
                 VALUES ('test-repo', 'Test', '/tmp/test', datetime('now'));
                 INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at)
                 VALUES ('snap-1', 'test-repo', 'full', 'ready', datetime('now'));",
            )
            .unwrap();

        conn
    }

    #[test]
    fn insert_and_query_status_mappings() {
        let mut conn = create_test_db();

        let mappings = vec![StatusMapping {
            symbol_key: "test-repo:file.c#translate_code:SYMBOL:FUNCTION".to_string(),
            function_name: "translate_code".to_string(),
            file_path: "file.c".to_string(),
            line_start: 10,
            line_end: 30,
            source_type: "input_code_t".to_string(),
            target_type: "output_code_t".to_string(),
            mappings: vec![CaseMapping {
                inputs: vec!["INPUT_A".to_string(), "INPUT_B".to_string()],
                output: "OUTPUT_X".to_string(),
            }],
            default_output: Some("OUTPUT_DEFAULT".to_string()),
        }];

        let count = conn.insert_status_mappings("snap-1", &mappings).unwrap();
        assert_eq!(count, 1);

        let results = conn.query_status_mappings("snap-1", None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].function_name, "translate_code");
        assert_eq!(results[0].source_type, "input_code_t");
        assert_eq!(results[0].mappings.len(), 1);
        assert_eq!(results[0].mappings[0].inputs.len(), 2);
        assert_eq!(results[0].default_output, Some("OUTPUT_DEFAULT".to_string()));
    }

    #[test]
    fn query_with_file_filter() {
        let mut conn = create_test_db();

        let mappings = vec![
            StatusMapping {
                symbol_key: "test-repo:src/a.c#func_a:SYMBOL:FUNCTION".to_string(),
                function_name: "func_a".to_string(),
                file_path: "src/a.c".to_string(),
                line_start: 1,
                line_end: 10,
                source_type: "type_a".to_string(),
                target_type: "type_b".to_string(),
                mappings: vec![],
                default_output: None,
            },
            StatusMapping {
                symbol_key: "test-repo:lib/b.c#func_b:SYMBOL:FUNCTION".to_string(),
                function_name: "func_b".to_string(),
                file_path: "lib/b.c".to_string(),
                line_start: 1,
                line_end: 10,
                source_type: "type_c".to_string(),
                target_type: "type_d".to_string(),
                mappings: vec![],
                default_output: None,
            },
        ];

        conn.insert_status_mappings("snap-1", &mappings).unwrap();

        // Filter by src/ prefix
        let results = conn.query_status_mappings("snap-1", Some("src/")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/a.c");

        // No filter
        let all_results = conn.query_status_mappings("snap-1", None).unwrap();
        assert_eq!(all_results.len(), 2);
    }

    #[test]
    fn insert_replaces_existing() {
        let mut conn = create_test_db();

        let mappings1 = vec![StatusMapping {
            symbol_key: "test-repo:file.c#old:SYMBOL:FUNCTION".to_string(),
            function_name: "old".to_string(),
            file_path: "file.c".to_string(),
            line_start: 1,
            line_end: 5,
            source_type: "a".to_string(),
            target_type: "b".to_string(),
            mappings: vec![],
            default_output: None,
        }];

        conn.insert_status_mappings("snap-1", &mappings1).unwrap();
        assert_eq!(conn.count_status_mappings("snap-1").unwrap(), 1);

        // Insert new set - should replace
        let mappings2 = vec![
            StatusMapping {
                symbol_key: "test-repo:file.c#new1:SYMBOL:FUNCTION".to_string(),
                function_name: "new1".to_string(),
                file_path: "file.c".to_string(),
                line_start: 1,
                line_end: 5,
                source_type: "c".to_string(),
                target_type: "d".to_string(),
                mappings: vec![],
                default_output: None,
            },
            StatusMapping {
                symbol_key: "test-repo:file.c#new2:SYMBOL:FUNCTION".to_string(),
                function_name: "new2".to_string(),
                file_path: "file.c".to_string(),
                line_start: 10,
                line_end: 15,
                source_type: "e".to_string(),
                target_type: "f".to_string(),
                mappings: vec![],
                default_output: None,
            },
        ];

        conn.insert_status_mappings("snap-1", &mappings2).unwrap();
        assert_eq!(conn.count_status_mappings("snap-1").unwrap(), 2);

        let results = conn.query_status_mappings("snap-1", None).unwrap();
        assert!(results.iter().all(|m| m.function_name != "old"));
    }

    #[test]
    fn insert_fails_for_missing_snapshot() {
        let mut conn = create_test_db();

        let mappings = vec![StatusMapping {
            symbol_key: "test:file.c#f:SYMBOL:FUNCTION".to_string(),
            function_name: "f".to_string(),
            file_path: "file.c".to_string(),
            line_start: 1,
            line_end: 5,
            source_type: "a".to_string(),
            target_type: "b".to_string(),
            mappings: vec![],
            default_output: None,
        }];

        let result = conn.insert_status_mappings("nonexistent", &mappings);
        assert!(matches!(
            result,
            Err(PolicyFactsStorageError::SnapshotNotFound(_))
        ));
    }

    #[test]
    fn count_status_mappings_empty() {
        let conn = create_test_db();
        assert_eq!(conn.count_status_mappings("snap-1").unwrap(), 0);
    }
}
