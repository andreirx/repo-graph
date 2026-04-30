//! `PolicyFactsStorageRead` and `PolicyFactsStorageWrite` implementation
//! for `StorageConnection`.
//!
//! This module implements the policy-facts storage port traits on top of
//! the storage adapter's rusqlite connection.
//!
//! **Schema:**
//! - PF-1: `status_mappings` table (migration 021)
//! - PF-2: `behavioral_markers` table (migration 022)
//! - PF-3: `return_fates` table (migration 023)
//!
//! **Error handling:** All methods propagate errors through the
//! `PolicyFactsStorageError` type defined by the policy-facts crate.

use repo_graph_policy_facts::{
    BehavioralMarker, CaseMapping, FateEvidence, FateKind, MarkerEvidence, MarkerKind,
    PolicyFactsStorageError, PolicyFactsStorageRead, PolicyFactsStorageWrite, ReturnFate,
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

    fn insert_behavioral_markers(
        &mut self,
        snapshot_uid: &str,
        markers: &[BehavioralMarker],
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
        let tx = self
            .connection_mut()
            .transaction()
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        // Delete existing markers for this snapshot.
        tx.execute(
            "DELETE FROM behavioral_markers WHERE snapshot_uid = ?",
            [snapshot_uid],
        )
        .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        // Insert new markers.
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO behavioral_markers (
                        uid, snapshot_uid, symbol_key, function_name, file_path,
                        line_start, line_end, kind, evidence_json
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

            for marker in markers {
                let uid = uuid::Uuid::new_v4().to_string();
                let kind_str = marker.kind.to_string();
                let evidence_json = serde_json::to_string(&marker.evidence)
                    .map_err(|e| PolicyFactsStorageError::JsonError(e.to_string()))?;

                stmt.execute(params![
                    uid,
                    snapshot_uid,
                    marker.symbol_key,
                    marker.function_name,
                    marker.file_path,
                    marker.line_start,
                    marker.line_end,
                    kind_str,
                    evidence_json,
                ])
                .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        Ok(markers.len())
    }

    fn insert_return_fates(
        &mut self,
        snapshot_uid: &str,
        fates: &[ReturnFate],
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
        let tx = self
            .connection_mut()
            .transaction()
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        // Delete existing fates for this snapshot.
        tx.execute(
            "DELETE FROM return_fates WHERE snapshot_uid = ?",
            [snapshot_uid],
        )
        .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        // Insert new fates.
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO return_fates (
                        uid, snapshot_uid, callee_key, callee_name, caller_key,
                        caller_name, file_path, line, col, fate, evidence_json
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

            for fate in fates {
                let uid = uuid::Uuid::new_v4().to_string();
                let fate_str = fate.fate.to_string();
                let evidence_json = serde_json::to_string(&fate.evidence)
                    .map_err(|e| PolicyFactsStorageError::JsonError(e.to_string()))?;

                stmt.execute(params![
                    uid,
                    snapshot_uid,
                    fate.callee_key,
                    fate.callee_name,
                    fate.caller_key,
                    fate.caller_name,
                    fate.file_path,
                    fate.line,
                    fate.column,
                    fate_str,
                    evidence_json,
                ])
                .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;
            }
        }

        tx.commit()
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        Ok(fates.len())
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

    fn query_behavioral_markers(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
        kind_filter: Option<MarkerKind>,
    ) -> Result<Vec<BehavioralMarker>, PolicyFactsStorageError> {
        let conn = self.connection();

        // Build query dynamically based on filters
        let mut sql = String::from(
            "SELECT symbol_key, function_name, file_path, line_start, line_end,
                    kind, evidence_json
             FROM behavioral_markers
             WHERE snapshot_uid = ?",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(snapshot_uid.to_string())];

        if let Some(prefix) = file_filter {
            sql.push_str(" AND file_path LIKE ?");
            params.push(Box::new(format!("{}%", prefix)));
        }

        if let Some(kind) = kind_filter {
            sql.push_str(" AND kind = ?");
            params.push(Box::new(kind.to_string()));
        }

        sql.push_str(" ORDER BY file_path, line_start");

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let symbol_key: String = row.get(0)?;
                let function_name: String = row.get(1)?;
                let file_path: String = row.get(2)?;
                let line_start: u32 = row.get(3)?;
                let line_end: u32 = row.get(4)?;
                let kind_str: String = row.get(5)?;
                let evidence_json: String = row.get(6)?;

                Ok((
                    symbol_key,
                    function_name,
                    file_path,
                    line_start,
                    line_end,
                    kind_str,
                    evidence_json,
                ))
            })
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        let mut results = Vec::new();
        for row_result in rows {
            let (symbol_key, function_name, file_path, line_start, line_end, kind_str, evidence_json) =
                row_result.map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

            let kind: MarkerKind = kind_str
                .parse()
                .map_err(|e: String| PolicyFactsStorageError::JsonError(e))?;

            let evidence: MarkerEvidence = serde_json::from_str(&evidence_json)
                .map_err(|e| PolicyFactsStorageError::JsonError(e.to_string()))?;

            results.push(BehavioralMarker {
                symbol_key,
                function_name,
                file_path,
                line_start,
                line_end,
                kind,
                evidence,
            });
        }

        Ok(results)
    }

    fn count_behavioral_markers(&self, snapshot_uid: &str) -> Result<usize, PolicyFactsStorageError> {
        let conn = self.connection();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM behavioral_markers WHERE snapshot_uid = ?",
                [snapshot_uid],
                |row| row.get(0),
            )
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        Ok(count as usize)
    }

    fn query_return_fates(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
        callee_filter: Option<&str>,
        fate_filter: Option<FateKind>,
    ) -> Result<Vec<ReturnFate>, PolicyFactsStorageError> {
        let conn = self.connection();

        // Build query dynamically based on filters
        let mut sql = String::from(
            "SELECT callee_key, callee_name, caller_key, caller_name, file_path,
                    line, col, fate, evidence_json
             FROM return_fates
             WHERE snapshot_uid = ?",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(snapshot_uid.to_string())];

        if let Some(prefix) = file_filter {
            sql.push_str(" AND file_path LIKE ?");
            params.push(Box::new(format!("{}%", prefix)));
        }

        if let Some(callee) = callee_filter {
            sql.push_str(" AND callee_name = ?");
            params.push(Box::new(callee.to_string()));
        }

        if let Some(fate) = fate_filter {
            sql.push_str(" AND fate = ?");
            params.push(Box::new(fate.to_string()));
        }

        sql.push_str(" ORDER BY file_path, line, col");

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let callee_key: Option<String> = row.get(0)?;
                let callee_name: String = row.get(1)?;
                let caller_key: String = row.get(2)?;
                let caller_name: String = row.get(3)?;
                let file_path: String = row.get(4)?;
                let line: u32 = row.get(5)?;
                let column: u32 = row.get(6)?;
                let fate_str: String = row.get(7)?;
                let evidence_json: String = row.get(8)?;

                Ok((
                    callee_key,
                    callee_name,
                    caller_key,
                    caller_name,
                    file_path,
                    line,
                    column,
                    fate_str,
                    evidence_json,
                ))
            })
            .map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

        let mut results = Vec::new();
        for row_result in rows {
            let (
                callee_key,
                callee_name,
                caller_key,
                caller_name,
                file_path,
                line,
                column,
                fate_str,
                evidence_json,
            ) = row_result.map_err(|e| PolicyFactsStorageError::DatabaseError(e.to_string()))?;

            let fate: FateKind = fate_str
                .parse()
                .map_err(|e: String| PolicyFactsStorageError::JsonError(e))?;

            let evidence: FateEvidence = serde_json::from_str(&evidence_json)
                .map_err(|e| PolicyFactsStorageError::JsonError(e.to_string()))?;

            results.push(ReturnFate {
                callee_key,
                callee_name,
                caller_key,
                caller_name,
                file_path,
                line,
                column,
                fate,
                evidence,
            });
        }

        Ok(results)
    }

    fn count_return_fates(&self, snapshot_uid: &str) -> Result<usize, PolicyFactsStorageError> {
        let conn = self.connection();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM return_fates WHERE snapshot_uid = ?",
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
    use repo_graph_policy_facts::{BehavioralMarker, FateKind, MarkerKind, ReturnFate};

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

    // =========================================================================
    // BEHAVIORAL_MARKER tests
    // =========================================================================

    #[test]
    fn insert_and_query_behavioral_markers() {
        use repo_graph_policy_facts::MarkerEvidence;

        let mut conn = create_test_db();

        let markers = vec![
            BehavioralMarker {
                symbol_key: "test-repo:file.c#retry_func:SYMBOL:FUNCTION".to_string(),
                function_name: "retry_func".to_string(),
                file_path: "file.c".to_string(),
                line_start: 10,
                line_end: 25,
                kind: MarkerKind::RetryLoop,
                evidence: MarkerEvidence::RetryLoop {
                    loop_kind: "for".to_string(),
                    sleep_call: Some("sleep".to_string()),
                    delay_ms: Some(1000),
                    max_attempts: Some(3),
                    break_condition: Some("result > 0".to_string()),
                },
            },
            BehavioralMarker {
                symbol_key: "test-repo:file.c#download_func:SYMBOL:FUNCTION".to_string(),
                function_name: "download_func".to_string(),
                file_path: "file.c".to_string(),
                line_start: 50,
                line_end: 50,
                kind: MarkerKind::ResumeOffset,
                evidence: MarkerEvidence::ResumeOffset {
                    api_call: "curl_easy_setopt".to_string(),
                    option_name: Some("CURLOPT_RESUME_FROM_LARGE".to_string()),
                    offset_source: Some("offset".to_string()),
                },
            },
        ];

        let count = conn.insert_behavioral_markers("snap-1", &markers).unwrap();
        assert_eq!(count, 2);

        let results = conn
            .query_behavioral_markers("snap-1", None, None)
            .unwrap();
        assert_eq!(results.len(), 2);

        // Results should be sorted by file_path, line_start
        assert_eq!(results[0].function_name, "retry_func");
        assert_eq!(results[0].line_start, 10);
        assert_eq!(results[1].function_name, "download_func");
        assert_eq!(results[1].line_start, 50);
    }

    #[test]
    fn query_behavioral_markers_with_kind_filter() {
        use repo_graph_policy_facts::MarkerEvidence;

        let mut conn = create_test_db();

        let markers = vec![
            BehavioralMarker {
                symbol_key: "test-repo:file.c#func1:SYMBOL:FUNCTION".to_string(),
                function_name: "func1".to_string(),
                file_path: "file.c".to_string(),
                line_start: 10,
                line_end: 20,
                kind: MarkerKind::RetryLoop,
                evidence: MarkerEvidence::RetryLoop {
                    loop_kind: "while".to_string(),
                    sleep_call: Some("sleep".to_string()),
                    delay_ms: None,
                    max_attempts: None,
                    break_condition: None,
                },
            },
            BehavioralMarker {
                symbol_key: "test-repo:file.c#func2:SYMBOL:FUNCTION".to_string(),
                function_name: "func2".to_string(),
                file_path: "file.c".to_string(),
                line_start: 30,
                line_end: 30,
                kind: MarkerKind::ResumeOffset,
                evidence: MarkerEvidence::ResumeOffset {
                    api_call: "curl_easy_setopt".to_string(),
                    option_name: Some("CURLOPT_RESUME_FROM".to_string()),
                    offset_source: None,
                },
            },
        ];

        conn.insert_behavioral_markers("snap-1", &markers).unwrap();

        // Filter by RETRY_LOOP
        let retry_results = conn
            .query_behavioral_markers("snap-1", None, Some(MarkerKind::RetryLoop))
            .unwrap();
        assert_eq!(retry_results.len(), 1);
        assert_eq!(retry_results[0].kind, MarkerKind::RetryLoop);

        // Filter by RESUME_OFFSET
        let resume_results = conn
            .query_behavioral_markers("snap-1", None, Some(MarkerKind::ResumeOffset))
            .unwrap();
        assert_eq!(resume_results.len(), 1);
        assert_eq!(resume_results[0].kind, MarkerKind::ResumeOffset);
    }

    #[test]
    fn query_behavioral_markers_with_file_filter() {
        use repo_graph_policy_facts::MarkerEvidence;

        let mut conn = create_test_db();

        let markers = vec![
            BehavioralMarker {
                symbol_key: "test-repo:src/a.c#func:SYMBOL:FUNCTION".to_string(),
                function_name: "func".to_string(),
                file_path: "src/a.c".to_string(),
                line_start: 10,
                line_end: 20,
                kind: MarkerKind::RetryLoop,
                evidence: MarkerEvidence::RetryLoop {
                    loop_kind: "for".to_string(),
                    sleep_call: Some("sleep".to_string()),
                    delay_ms: None,
                    max_attempts: None,
                    break_condition: None,
                },
            },
            BehavioralMarker {
                symbol_key: "test-repo:lib/b.c#func:SYMBOL:FUNCTION".to_string(),
                function_name: "func".to_string(),
                file_path: "lib/b.c".to_string(),
                line_start: 10,
                line_end: 20,
                kind: MarkerKind::RetryLoop,
                evidence: MarkerEvidence::RetryLoop {
                    loop_kind: "while".to_string(),
                    sleep_call: Some("usleep".to_string()),
                    delay_ms: None,
                    max_attempts: None,
                    break_condition: None,
                },
            },
        ];

        conn.insert_behavioral_markers("snap-1", &markers).unwrap();

        // Filter by src/ prefix
        let results = conn
            .query_behavioral_markers("snap-1", Some("src/"), None)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/a.c");
    }

    #[test]
    fn insert_behavioral_markers_replaces_existing() {
        use repo_graph_policy_facts::MarkerEvidence;

        let mut conn = create_test_db();

        let markers1 = vec![BehavioralMarker {
            symbol_key: "test-repo:file.c#old:SYMBOL:FUNCTION".to_string(),
            function_name: "old".to_string(),
            file_path: "file.c".to_string(),
            line_start: 1,
            line_end: 10,
            kind: MarkerKind::RetryLoop,
            evidence: MarkerEvidence::RetryLoop {
                loop_kind: "for".to_string(),
                sleep_call: Some("sleep".to_string()),
                delay_ms: None,
                max_attempts: None,
                break_condition: None,
            },
        }];

        conn.insert_behavioral_markers("snap-1", &markers1).unwrap();
        assert_eq!(conn.count_behavioral_markers("snap-1").unwrap(), 1);

        // Insert new set - should replace
        let markers2 = vec![
            BehavioralMarker {
                symbol_key: "test-repo:file.c#new1:SYMBOL:FUNCTION".to_string(),
                function_name: "new1".to_string(),
                file_path: "file.c".to_string(),
                line_start: 1,
                line_end: 10,
                kind: MarkerKind::RetryLoop,
                evidence: MarkerEvidence::RetryLoop {
                    loop_kind: "while".to_string(),
                    sleep_call: Some("sleep".to_string()),
                    delay_ms: None,
                    max_attempts: None,
                    break_condition: None,
                },
            },
            BehavioralMarker {
                symbol_key: "test-repo:file.c#new2:SYMBOL:FUNCTION".to_string(),
                function_name: "new2".to_string(),
                file_path: "file.c".to_string(),
                line_start: 20,
                line_end: 20,
                kind: MarkerKind::ResumeOffset,
                evidence: MarkerEvidence::ResumeOffset {
                    api_call: "curl_easy_setopt".to_string(),
                    option_name: None,
                    offset_source: None,
                },
            },
        ];

        conn.insert_behavioral_markers("snap-1", &markers2).unwrap();
        assert_eq!(conn.count_behavioral_markers("snap-1").unwrap(), 2);

        let results = conn
            .query_behavioral_markers("snap-1", None, None)
            .unwrap();
        assert!(results.iter().all(|m| m.function_name != "old"));
    }

    #[test]
    fn insert_behavioral_markers_fails_for_missing_snapshot() {
        use repo_graph_policy_facts::MarkerEvidence;

        let mut conn = create_test_db();

        let markers = vec![BehavioralMarker {
            symbol_key: "test:file.c#f:SYMBOL:FUNCTION".to_string(),
            function_name: "f".to_string(),
            file_path: "file.c".to_string(),
            line_start: 1,
            line_end: 5,
            kind: MarkerKind::RetryLoop,
            evidence: MarkerEvidence::RetryLoop {
                loop_kind: "for".to_string(),
                sleep_call: Some("sleep".to_string()),
                delay_ms: None,
                max_attempts: None,
                break_condition: None,
            },
        }];

        let result = conn.insert_behavioral_markers("nonexistent", &markers);
        assert!(matches!(
            result,
            Err(PolicyFactsStorageError::SnapshotNotFound(_))
        ));
    }

    #[test]
    fn count_behavioral_markers_empty() {
        let conn = create_test_db();
        assert_eq!(conn.count_behavioral_markers("snap-1").unwrap(), 0);
    }

    // =========================================================================
    // RETURN_FATE tests
    // =========================================================================

    #[test]
    fn insert_and_query_return_fates() {
        use repo_graph_policy_facts::FateEvidence;

        let mut conn = create_test_db();

        let fates = vec![
            ReturnFate {
                callee_key: None,
                callee_name: "get_status".to_string(),
                caller_key: "test-repo:file.c#main:SYMBOL:FUNCTION".to_string(),
                caller_name: "main".to_string(),
                file_path: "file.c".to_string(),
                line: 10,
                column: 5,
                fate: FateKind::Checked,
                evidence: FateEvidence::Checked {
                    check_kind: "if".to_string(),
                    operator: Some("==".to_string()),
                    compared_to: Some("OK".to_string()),
                },
            },
            ReturnFate {
                callee_key: Some("test-repo:lib.c#helper:SYMBOL:FUNCTION".to_string()),
                callee_name: "helper".to_string(),
                caller_key: "test-repo:file.c#main:SYMBOL:FUNCTION".to_string(),
                caller_name: "main".to_string(),
                file_path: "file.c".to_string(),
                line: 20,
                column: 8,
                fate: FateKind::Stored,
                evidence: FateEvidence::Stored {
                    variable_name: "result".to_string(),
                    immediately_checked: false,
                },
            },
        ];

        let count = conn.insert_return_fates("snap-1", &fates).unwrap();
        assert_eq!(count, 2);

        let results = conn
            .query_return_fates("snap-1", None, None, None)
            .unwrap();
        assert_eq!(results.len(), 2);

        // Results should be sorted by file_path, line, col
        assert_eq!(results[0].line, 10);
        assert_eq!(results[1].line, 20);
    }

    #[test]
    fn query_return_fates_with_fate_filter() {
        use repo_graph_policy_facts::FateEvidence;

        let mut conn = create_test_db();

        let fates = vec![
            ReturnFate {
                callee_key: None,
                callee_name: "func1".to_string(),
                caller_key: "test:file.c#caller:SYMBOL:FUNCTION".to_string(),
                caller_name: "caller".to_string(),
                file_path: "file.c".to_string(),
                line: 10,
                column: 5,
                fate: FateKind::Ignored,
                evidence: FateEvidence::Ignored {
                    explicit_void_cast: false,
                },
            },
            ReturnFate {
                callee_key: None,
                callee_name: "func2".to_string(),
                caller_key: "test:file.c#caller:SYMBOL:FUNCTION".to_string(),
                caller_name: "caller".to_string(),
                file_path: "file.c".to_string(),
                line: 20,
                column: 5,
                fate: FateKind::Stored,
                evidence: FateEvidence::Stored {
                    variable_name: "x".to_string(),
                    immediately_checked: false,
                },
            },
        ];

        conn.insert_return_fates("snap-1", &fates).unwrap();

        // Filter by IGNORED
        let ignored = conn
            .query_return_fates("snap-1", None, None, Some(FateKind::Ignored))
            .unwrap();
        assert_eq!(ignored.len(), 1);
        assert_eq!(ignored[0].fate, FateKind::Ignored);

        // Filter by STORED
        let stored = conn
            .query_return_fates("snap-1", None, None, Some(FateKind::Stored))
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].fate, FateKind::Stored);
    }

    #[test]
    fn query_return_fates_with_callee_filter() {
        use repo_graph_policy_facts::FateEvidence;

        let mut conn = create_test_db();

        let fates = vec![
            ReturnFate {
                callee_key: None,
                callee_name: "get_status".to_string(),
                caller_key: "test:file.c#caller:SYMBOL:FUNCTION".to_string(),
                caller_name: "caller".to_string(),
                file_path: "file.c".to_string(),
                line: 10,
                column: 5,
                fate: FateKind::Checked,
                evidence: FateEvidence::Checked {
                    check_kind: "if".to_string(),
                    operator: None,
                    compared_to: None,
                },
            },
            ReturnFate {
                callee_key: None,
                callee_name: "do_work".to_string(),
                caller_key: "test:file.c#caller:SYMBOL:FUNCTION".to_string(),
                caller_name: "caller".to_string(),
                file_path: "file.c".to_string(),
                line: 20,
                column: 5,
                fate: FateKind::Ignored,
                evidence: FateEvidence::Ignored {
                    explicit_void_cast: true,
                },
            },
        ];

        conn.insert_return_fates("snap-1", &fates).unwrap();

        // Filter by callee name
        let results = conn
            .query_return_fates("snap-1", None, Some("get_status"), None)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].callee_name, "get_status");
    }

    #[test]
    fn insert_return_fates_replaces_existing() {
        use repo_graph_policy_facts::FateEvidence;

        let mut conn = create_test_db();

        let fates1 = vec![ReturnFate {
            callee_key: None,
            callee_name: "old_func".to_string(),
            caller_key: "test:file.c#caller:SYMBOL:FUNCTION".to_string(),
            caller_name: "caller".to_string(),
            file_path: "file.c".to_string(),
            line: 10,
            column: 5,
            fate: FateKind::Ignored,
            evidence: FateEvidence::Ignored {
                explicit_void_cast: false,
            },
        }];

        conn.insert_return_fates("snap-1", &fates1).unwrap();
        assert_eq!(conn.count_return_fates("snap-1").unwrap(), 1);

        // Insert new set - should replace
        let fates2 = vec![
            ReturnFate {
                callee_key: None,
                callee_name: "new_func1".to_string(),
                caller_key: "test:file.c#caller:SYMBOL:FUNCTION".to_string(),
                caller_name: "caller".to_string(),
                file_path: "file.c".to_string(),
                line: 10,
                column: 5,
                fate: FateKind::Stored,
                evidence: FateEvidence::Stored {
                    variable_name: "x".to_string(),
                    immediately_checked: false,
                },
            },
            ReturnFate {
                callee_key: None,
                callee_name: "new_func2".to_string(),
                caller_key: "test:file.c#caller:SYMBOL:FUNCTION".to_string(),
                caller_name: "caller".to_string(),
                file_path: "file.c".to_string(),
                line: 20,
                column: 5,
                fate: FateKind::Propagated,
                evidence: FateEvidence::Propagated {
                    wrapped: false,
                    wrapper: None,
                },
            },
        ];

        conn.insert_return_fates("snap-1", &fates2).unwrap();
        assert_eq!(conn.count_return_fates("snap-1").unwrap(), 2);

        let results = conn
            .query_return_fates("snap-1", None, None, None)
            .unwrap();
        assert!(results.iter().all(|f| f.callee_name != "old_func"));
    }

    #[test]
    fn insert_return_fates_fails_for_missing_snapshot() {
        use repo_graph_policy_facts::FateEvidence;

        let mut conn = create_test_db();

        let fates = vec![ReturnFate {
            callee_key: None,
            callee_name: "func".to_string(),
            caller_key: "test:file.c#caller:SYMBOL:FUNCTION".to_string(),
            caller_name: "caller".to_string(),
            file_path: "file.c".to_string(),
            line: 10,
            column: 5,
            fate: FateKind::Ignored,
            evidence: FateEvidence::Ignored {
                explicit_void_cast: false,
            },
        }];

        let result = conn.insert_return_fates("nonexistent", &fates);
        assert!(matches!(
            result,
            Err(PolicyFactsStorageError::SnapshotNotFound(_))
        ));
    }

    #[test]
    fn count_return_fates_empty() {
        let conn = create_test_db();
        assert_eq!(conn.count_return_fates("snap-1").unwrap(), 0);
    }
}
