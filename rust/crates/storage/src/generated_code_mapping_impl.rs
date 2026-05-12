//! Generated code mapping storage implementation.
//!
//! Implements `GeneratedCodeMappingStorePort` (from indexer) for `StorageConnection`.
//! This is the write-side port for generated code provenance mapping (CS-2A).

use rusqlite::params;

use repo_graph_indexer::storage_port::{GeneratedCodeMappingInput, GeneratedCodeMappingStorePort};

use crate::connection::StorageConnection;
use crate::error::StorageError;

impl GeneratedCodeMappingStorePort for StorageConnection {
    type Error = StorageError;

    fn insert_generated_code_mappings(
        &mut self,
        mappings: &[GeneratedCodeMappingInput],
    ) -> Result<usize, StorageError> {
        if mappings.is_empty() {
            return Ok(0);
        }

        let conn = self.connection_mut();
        let tx = conn.transaction()?;
        let mut count = 0;

        {
            let mut stmt = tx.prepare(
                r#"
                INSERT OR IGNORE INTO generated_code_mappings (
                    mapping_uid, snapshot_uid, schema_element_uid,
                    generated_symbol_key, language, generated_file,
                    mapping_basis, confidence, metadata_json, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                "#,
            )?;

            for mapping in mappings {
                let rows = stmt.execute(params![
                    mapping.mapping_uid,
                    mapping.snapshot_uid,
                    mapping.schema_element_uid,
                    mapping.generated_symbol_key,
                    mapping.language,
                    mapping.generated_file,
                    mapping.mapping_basis,
                    mapping.confidence,
                    mapping.metadata_json,
                ])?;
                count += rows;
            }
        }

        tx.commit()?;
        Ok(count)
    }

    fn delete_generated_code_mappings_for_snapshot(
        &mut self,
        snapshot_uid: &str,
    ) -> Result<(), StorageError> {
        self.connection_mut().execute(
            "DELETE FROM generated_code_mappings WHERE snapshot_uid = ?",
            params![snapshot_uid],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> StorageConnection {
        let mut conn = StorageConnection::open_in_memory().unwrap();

        // Create required rows for foreign keys
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

        // Insert a contract schema and element for foreign key references
        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_schemas (schema_uid, snapshot_uid, repo_uid, schema_kind, file_path, content_hash, extractor, parsed_at)
                   VALUES ('schema-1', 's1', 'r1', 'protobuf', 'test.proto', 'hash', 'proto-parser:0.1.0', datetime('now'))"#,
                [],
            )
            .unwrap();
        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_elements (element_uid, schema_uid, element_kind, name, full_name)
                   VALUES ('elem-1', 'schema-1', 'message', 'TestMessage', 'test.TestMessage')"#,
                [],
            )
            .unwrap();

        conn
    }

    #[test]
    fn insert_generated_code_mapping() {
        let mut conn = setup_test_db();

        let mappings = vec![GeneratedCodeMappingInput {
            mapping_uid: "map-1".to_string(),
            snapshot_uid: "s1".to_string(),
            schema_element_uid: "elem-1".to_string(),
            generated_symbol_key: "test:TestProtos.TestMessage".to_string(),
            language: "java".to_string(),
            generated_file: "generated/TestProtos.java".to_string(),
            mapping_basis: "exact_option_match".to_string(),
            confidence: 0.95,
            metadata_json: Some(r#"{"java_package":"test"}"#.to_string()),
        }];

        let count = conn.insert_generated_code_mappings(&mappings).unwrap();
        assert_eq!(count, 1);

        // Verify via direct query
        let mapping_count: i64 = conn
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM generated_code_mappings WHERE snapshot_uid = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mapping_count, 1);

        // Verify content
        let (basis, confidence): (String, f64) = conn
            .connection()
            .query_row(
                "SELECT mapping_basis, confidence FROM generated_code_mappings WHERE mapping_uid = 'map-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(basis, "exact_option_match");
        assert!((confidence - 0.95).abs() < 0.001);
    }

    #[test]
    fn delete_mappings_for_snapshot() {
        let mut conn = setup_test_db();

        // Insert mappings
        let mappings = vec![
            GeneratedCodeMappingInput {
                mapping_uid: "map-del-1".to_string(),
                snapshot_uid: "s1".to_string(),
                schema_element_uid: "elem-1".to_string(),
                generated_symbol_key: "test:Class1".to_string(),
                language: "java".to_string(),
                generated_file: "Test.java".to_string(),
                mapping_basis: "filename_convention".to_string(),
                confidence: 0.85,
                metadata_json: None,
            },
            GeneratedCodeMappingInput {
                mapping_uid: "map-del-2".to_string(),
                snapshot_uid: "s1".to_string(),
                schema_element_uid: "elem-1".to_string(),
                generated_symbol_key: "test:Class2".to_string(),
                language: "java".to_string(),
                generated_file: "Test.java".to_string(),
                mapping_basis: "filename_convention".to_string(),
                confidence: 0.85,
                metadata_json: None,
            },
        ];

        conn.insert_generated_code_mappings(&mappings).unwrap();

        // Verify 2 mappings exist
        let count: i64 = conn
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM generated_code_mappings WHERE snapshot_uid = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);

        // Delete
        conn.delete_generated_code_mappings_for_snapshot("s1")
            .unwrap();

        // Verify 0 mappings
        let count: i64 = conn
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM generated_code_mappings WHERE snapshot_uid = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn idempotent_mapping_insert() {
        let mut conn = setup_test_db();

        let mapping = GeneratedCodeMappingInput {
            mapping_uid: "map-idem".to_string(),
            snapshot_uid: "s1".to_string(),
            schema_element_uid: "elem-1".to_string(),
            generated_symbol_key: "test:Dup".to_string(),
            language: "java".to_string(),
            generated_file: "Dup.java".to_string(),
            mapping_basis: "symbol_normalized_match".to_string(),
            confidence: 0.75,
            metadata_json: None,
        };

        // Insert twice
        conn.insert_generated_code_mappings(std::slice::from_ref(&mapping))
            .unwrap();
        conn.insert_generated_code_mappings(&[mapping]).unwrap();

        // Should have only one
        let count: i64 = conn
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM generated_code_mappings WHERE mapping_uid = 'map-idem'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn empty_mappings_returns_zero() {
        let mut conn = setup_test_db();
        let count = conn.insert_generated_code_mappings(&[]).unwrap();
        assert_eq!(count, 0);
    }
}
