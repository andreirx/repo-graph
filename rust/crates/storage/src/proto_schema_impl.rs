//! Proto schema storage implementation.
//!
//! Implements `ProtoSchemaStorePort` (from indexer) for `StorageConnection`.
//! This is the write-side port for proto schema indexing (CS-1).
//!
//! The read-side queries are provided by `ContractSchemaStoragePort` in
//! `contract_schema_impl.rs` for CLI usage.

use rusqlite::params;

use repo_graph_indexer::storage_port::{ProtoElementInput, ProtoSchemaInput, ProtoSchemaStorePort};

use crate::connection::StorageConnection;
use crate::error::StorageError;

impl ProtoSchemaStorePort for StorageConnection {
    type Error = StorageError;

    fn insert_proto_schema(&mut self, input: &ProtoSchemaInput) -> Result<(), StorageError> {
        self.connection_mut().execute(
            r#"
            INSERT OR IGNORE INTO contract_schemas (
                schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                package_name, syntax_version, content_hash, imports_json,
                options_json, extractor, parsed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
            params![
                input.schema_uid,
                input.snapshot_uid,
                input.repo_uid,
                input.schema_kind,
                input.file_path,
                input.package_name,
                input.syntax_version,
                input.content_hash,
                input.imports_json,
                input.options_json,
                input.extractor,
            ],
        )?;
        Ok(())
    }

    fn insert_proto_elements(
        &mut self,
        elements: &[ProtoElementInput],
    ) -> Result<usize, StorageError> {
        let conn = self.connection_mut();
        let tx = conn.transaction()?;
        let mut count = 0;

        {
            let mut stmt = tx.prepare(
                r#"
                INSERT OR IGNORE INTO contract_elements (
                    element_uid, schema_uid, element_kind, name, full_name,
                    parent_element_uid, line_start, line_end, metadata_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )?;

            for element in elements {
                let rows = stmt.execute(params![
                    element.element_uid,
                    element.schema_uid,
                    element.element_kind,
                    element.name,
                    element.full_name,
                    element.parent_element_uid,
                    element.line_start,
                    element.line_end,
                    element.metadata_json,
                ])?;
                count += rows;
            }
        }

        tx.commit()?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> StorageConnection {
        let mut conn = StorageConnection::open_in_memory().unwrap();

        conn.connection_mut()
            .execute(
                "INSERT INTO repos (repo_uid, name, root_path, created_at) VALUES ('r1', 'test', '/test', datetime('now'))",
                [],
            )
            .unwrap();
        conn.connection_mut()
            .execute(
                "INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at) VALUES ('s1', 'r1', 'full', 'ready', datetime('now'))",
                [],
            )
            .unwrap();

        conn
    }

    #[test]
    fn insert_proto_schema_and_elements() {
        let mut conn = setup_test_db();

        // Insert schema via ProtoSchemaStorePort
        let schema = ProtoSchemaInput {
            schema_uid: "proto-schema-1".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            schema_kind: "protobuf".to_string(),
            file_path: "api/user.proto".to_string(),
            package_name: Some("api".to_string()),
            syntax_version: Some("proto3".to_string()),
            content_hash: "hash123".to_string(),
            imports_json: None,
            options_json: None,
            extractor: "proto-parser:0.1.0".to_string(),
        };

        conn.insert_proto_schema(&schema).unwrap();

        // Insert elements
        let elements = vec![
            ProtoElementInput {
                element_uid: "elem-1".to_string(),
                schema_uid: "proto-schema-1".to_string(),
                element_kind: "message".to_string(),
                name: "User".to_string(),
                full_name: "api.User".to_string(),
                parent_element_uid: None,
                line_start: Some(5),
                line_end: Some(10),
                metadata_json: Some(r#"{"fields_count": 2}"#.to_string()),
            },
            ProtoElementInput {
                element_uid: "elem-2".to_string(),
                schema_uid: "proto-schema-1".to_string(),
                element_kind: "field".to_string(),
                name: "id".to_string(),
                full_name: "api.User.id".to_string(),
                parent_element_uid: Some("elem-1".to_string()),
                line_start: Some(6),
                line_end: Some(6),
                metadata_json: Some(r#"{"number": 1}"#.to_string()),
            },
        ];

        let count = conn.insert_proto_elements(&elements).unwrap();
        assert_eq!(count, 2);

        // Verify via direct SQL query
        let conn_ref = conn.connection();
        let schema_count: i64 = conn_ref
            .query_row(
                "SELECT COUNT(*) FROM contract_schemas WHERE snapshot_uid = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(schema_count, 1);

        let element_count: i64 = conn_ref
            .query_row(
                "SELECT COUNT(*) FROM contract_elements WHERE schema_uid = 'proto-schema-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(element_count, 2);
    }

    #[test]
    fn idempotent_schema_insert() {
        let mut conn = setup_test_db();

        let schema = ProtoSchemaInput {
            schema_uid: "proto-schema-dup".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            schema_kind: "protobuf".to_string(),
            file_path: "dup.proto".to_string(),
            package_name: None,
            syntax_version: None,
            content_hash: "hash".to_string(),
            imports_json: None,
            options_json: None,
            extractor: "proto-parser:0.1.0".to_string(),
        };

        // Insert twice
        conn.insert_proto_schema(&schema).unwrap();
        conn.insert_proto_schema(&schema).unwrap();

        // Should still have only one
        let count: i64 = conn
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM contract_schemas WHERE schema_uid = 'proto-schema-dup'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
