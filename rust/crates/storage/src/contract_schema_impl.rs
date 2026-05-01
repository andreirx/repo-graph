//! Contract schema storage implementation.
//!
//! Implements `ContractSchemaStoragePort` for `StorageConnection`.

use rusqlite::{params, OptionalExtension, Row};

use crate::connection::StorageConnection;
use crate::contract_schema_port::{
    ContractElementInput, ContractElementRow, ContractSchemaInput, ContractSchemaRow,
    ContractSchemaStoragePort, GeneratedCodeMappingRow,
};
use crate::error::StorageError;

impl ContractSchemaStoragePort for StorageConnection {
    fn insert_contract_schema(&mut self, input: &ContractSchemaInput) -> Result<(), StorageError> {
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

    fn insert_contract_elements(
        &mut self,
        elements: &[ContractElementInput],
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

    fn list_contract_schemas(
        &self,
        snapshot_uid: &str,
        kind_filter: Option<&str>,
    ) -> Result<Vec<ContractSchemaRow>, StorageError> {
        let conn = self.connection();

        fn map_row(row: &Row<'_>) -> rusqlite::Result<ContractSchemaRow> {
            Ok(ContractSchemaRow {
                schema_uid: row.get(0)?,
                snapshot_uid: row.get(1)?,
                repo_uid: row.get(2)?,
                schema_kind: row.get(3)?,
                file_path: row.get(4)?,
                package_name: row.get(5)?,
                syntax_version: row.get(6)?,
                content_hash: row.get(7)?,
                extractor: row.get(8)?,
                parsed_at: row.get(9)?,
            })
        }

        let mut result = Vec::new();

        match kind_filter {
            Some(kind) => {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                           package_name, syntax_version, content_hash, extractor, parsed_at
                    FROM contract_schemas
                    WHERE snapshot_uid = ? AND schema_kind = ?
                    ORDER BY file_path
                    "#,
                )?;
                let rows = stmt.query_map(params![snapshot_uid, kind], map_row)?;
                for row in rows {
                    result.push(row?);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                           package_name, syntax_version, content_hash, extractor, parsed_at
                    FROM contract_schemas
                    WHERE snapshot_uid = ?
                    ORDER BY file_path
                    "#,
                )?;
                let rows = stmt.query_map(params![snapshot_uid], map_row)?;
                for row in rows {
                    result.push(row?);
                }
            }
        }

        Ok(result)
    }

    fn get_schema_by_file(
        &self,
        snapshot_uid: &str,
        file_path: &str,
    ) -> Result<Option<ContractSchemaRow>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            r#"
            SELECT schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                   package_name, syntax_version, content_hash, extractor, parsed_at
            FROM contract_schemas
            WHERE snapshot_uid = ? AND file_path = ?
            "#,
        )?;

        let result = stmt
            .query_row(params![snapshot_uid, file_path], |row: &Row<'_>| {
                Ok(ContractSchemaRow {
                    schema_uid: row.get(0)?,
                    snapshot_uid: row.get(1)?,
                    repo_uid: row.get(2)?,
                    schema_kind: row.get(3)?,
                    file_path: row.get(4)?,
                    package_name: row.get(5)?,
                    syntax_version: row.get(6)?,
                    content_hash: row.get(7)?,
                    extractor: row.get(8)?,
                    parsed_at: row.get(9)?,
                })
            })
            .optional()?;

        Ok(result)
    }

    fn list_elements_for_schema(
        &self,
        schema_uid: &str,
        kind_filter: Option<&str>,
    ) -> Result<Vec<ContractElementRow>, StorageError> {
        let conn = self.connection();

        fn map_row(row: &Row<'_>) -> rusqlite::Result<ContractElementRow> {
            Ok(ContractElementRow {
                element_uid: row.get(0)?,
                schema_uid: row.get(1)?,
                element_kind: row.get(2)?,
                name: row.get(3)?,
                full_name: row.get(4)?,
                parent_element_uid: row.get(5)?,
                line_start: row.get(6)?,
                line_end: row.get(7)?,
                metadata_json: row.get(8)?,
            })
        }

        let mut result = Vec::new();

        match kind_filter {
            Some(kind) => {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT element_uid, schema_uid, element_kind, name, full_name,
                           parent_element_uid, line_start, line_end, metadata_json
                    FROM contract_elements
                    WHERE schema_uid = ? AND element_kind = ?
                    ORDER BY line_start, name
                    "#,
                )?;
                let rows = stmt.query_map(params![schema_uid, kind], map_row)?;
                for row in rows {
                    result.push(row?);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT element_uid, schema_uid, element_kind, name, full_name,
                           parent_element_uid, line_start, line_end, metadata_json
                    FROM contract_elements
                    WHERE schema_uid = ?
                    ORDER BY line_start, name
                    "#,
                )?;
                let rows = stmt.query_map(params![schema_uid], map_row)?;
                for row in rows {
                    result.push(row?);
                }
            }
        }

        Ok(result)
    }

    fn find_element_by_full_name(
        &self,
        snapshot_uid: &str,
        full_name: &str,
    ) -> Result<Option<ContractElementRow>, StorageError> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            r#"
            SELECT ce.element_uid, ce.schema_uid, ce.element_kind, ce.name, ce.full_name,
                   ce.parent_element_uid, ce.line_start, ce.line_end, ce.metadata_json
            FROM contract_elements ce
            JOIN contract_schemas cs ON ce.schema_uid = cs.schema_uid
            WHERE cs.snapshot_uid = ? AND ce.full_name = ?
            "#,
        )?;

        let result = stmt
            .query_row(params![snapshot_uid, full_name], |row: &Row<'_>| {
                Ok(ContractElementRow {
                    element_uid: row.get(0)?,
                    schema_uid: row.get(1)?,
                    element_kind: row.get(2)?,
                    name: row.get(3)?,
                    full_name: row.get(4)?,
                    parent_element_uid: row.get(5)?,
                    line_start: row.get(6)?,
                    line_end: row.get(7)?,
                    metadata_json: row.get(8)?,
                })
            })
            .optional()?;

        Ok(result)
    }

    fn count_schemas(&self, snapshot_uid: &str) -> Result<usize, StorageError> {
        let conn = self.connection();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM contract_schemas WHERE snapshot_uid = ?",
            params![snapshot_uid],
            |row: &Row<'_>| row.get(0),
        )?;
        Ok(count as usize)
    }

    fn count_elements(&self, snapshot_uid: &str) -> Result<usize, StorageError> {
        let conn = self.connection();
        let count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM contract_elements ce
            JOIN contract_schemas cs ON ce.schema_uid = cs.schema_uid
            WHERE cs.snapshot_uid = ?
            "#,
            params![snapshot_uid],
            |row: &Row<'_>| row.get(0),
        )?;
        Ok(count as usize)
    }

    fn list_generated_code_mappings(
        &self,
        snapshot_uid: &str,
        element_uid_filter: Option<&str>,
    ) -> Result<Vec<GeneratedCodeMappingRow>, StorageError> {
        let conn = self.connection();

        fn map_row(row: &Row<'_>) -> rusqlite::Result<GeneratedCodeMappingRow> {
            Ok(GeneratedCodeMappingRow {
                mapping_uid: row.get(0)?,
                snapshot_uid: row.get(1)?,
                schema_element_uid: row.get(2)?,
                generated_symbol_key: row.get(3)?,
                language: row.get(4)?,
                generated_file: row.get(5)?,
                mapping_basis: row.get(6)?,
                confidence: row.get(7)?,
                metadata_json: row.get(8)?,
                created_at: row.get(9)?,
            })
        }

        let mut result = Vec::new();

        match element_uid_filter {
            Some(element_uid) => {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT mapping_uid, snapshot_uid, schema_element_uid, generated_symbol_key,
                           language, generated_file, mapping_basis, confidence, metadata_json, created_at
                    FROM generated_code_mappings
                    WHERE snapshot_uid = ? AND schema_element_uid = ?
                    ORDER BY confidence DESC, generated_file
                    "#,
                )?;
                let rows = stmt.query_map(params![snapshot_uid, element_uid], map_row)?;
                for row in rows {
                    result.push(row?);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT mapping_uid, snapshot_uid, schema_element_uid, generated_symbol_key,
                           language, generated_file, mapping_basis, confidence, metadata_json, created_at
                    FROM generated_code_mappings
                    WHERE snapshot_uid = ?
                    ORDER BY confidence DESC, generated_file
                    "#,
                )?;
                let rows = stmt.query_map(params![snapshot_uid], map_row)?;
                for row in rows {
                    result.push(row?);
                }
            }
        }

        Ok(result)
    }

    fn count_generated_code_mappings(&self, snapshot_uid: &str) -> Result<usize, StorageError> {
        let conn = self.connection();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM generated_code_mappings WHERE snapshot_uid = ?",
            params![snapshot_uid],
            |row: &Row<'_>| row.get(0),
        )?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::StorageConnection;

    fn setup_test_db() -> StorageConnection {
        // open_in_memory already runs migrations
        let mut conn = StorageConnection::open_in_memory().unwrap();

        // Insert test repo and snapshot using connection_mut()
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
    fn insert_and_list_schemas() {
        let mut conn = setup_test_db();

        let input = ContractSchemaInput {
            schema_uid: "schema1".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            schema_kind: "protobuf".to_string(),
            file_path: "api/v1/user.proto".to_string(),
            package_name: Some("api.v1".to_string()),
            syntax_version: Some("proto3".to_string()),
            content_hash: "abc123".to_string(),
            imports_json: Some(r#"["google/protobuf/timestamp.proto"]"#.to_string()),
            options_json: None,
            extractor: "proto-parser:0.1.0".to_string(),
        };

        conn.insert_contract_schema(&input).unwrap();

        let schemas = conn.list_contract_schemas("s1", None).unwrap();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].file_path, "api/v1/user.proto");
        assert_eq!(schemas[0].package_name, Some("api.v1".to_string()));
    }

    #[test]
    fn insert_and_query_elements() {
        let mut conn = setup_test_db();

        // Insert schema
        let schema = ContractSchemaInput {
            schema_uid: "schema1".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            schema_kind: "protobuf".to_string(),
            file_path: "user.proto".to_string(),
            package_name: Some("api".to_string()),
            syntax_version: Some("proto3".to_string()),
            content_hash: "abc".to_string(),
            imports_json: None,
            options_json: None,
            extractor: "proto-parser:0.1.0".to_string(),
        };
        conn.insert_contract_schema(&schema).unwrap();

        // Insert elements
        let elements = vec![
            ContractElementInput {
                element_uid: "e1".to_string(),
                schema_uid: "schema1".to_string(),
                element_kind: "message".to_string(),
                name: "User".to_string(),
                full_name: "api.User".to_string(),
                parent_element_uid: None,
                line_start: Some(5),
                line_end: Some(10),
                metadata_json: None,
            },
            ContractElementInput {
                element_uid: "e2".to_string(),
                schema_uid: "schema1".to_string(),
                element_kind: "field".to_string(),
                name: "name".to_string(),
                full_name: "api.User.name".to_string(),
                parent_element_uid: Some("e1".to_string()),
                line_start: Some(6),
                line_end: Some(6),
                metadata_json: Some(r#"{"number": 1, "type": "string"}"#.to_string()),
            },
        ];

        let count = conn.insert_contract_elements(&elements).unwrap();
        assert_eq!(count, 2);

        // Query elements
        let all_elements = conn.list_elements_for_schema("schema1", None).unwrap();
        assert_eq!(all_elements.len(), 2);

        let messages = conn
            .list_elements_for_schema("schema1", Some("message"))
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].name, "User");

        // Find by full name
        let found = conn.find_element_by_full_name("s1", "api.User").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "User");
    }

    #[test]
    fn idempotent_insert() {
        let mut conn = setup_test_db();

        let input = ContractSchemaInput {
            schema_uid: "schema1".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            schema_kind: "protobuf".to_string(),
            file_path: "test.proto".to_string(),
            package_name: None,
            syntax_version: None,
            content_hash: "abc".to_string(),
            imports_json: None,
            options_json: None,
            extractor: "proto-parser:0.1.0".to_string(),
        };

        // Insert twice
        conn.insert_contract_schema(&input).unwrap();
        conn.insert_contract_schema(&input).unwrap();

        // Should still have only one
        let schemas = conn.list_contract_schemas("s1", None).unwrap();
        assert_eq!(schemas.len(), 1);
    }

    #[test]
    fn count_operations() {
        let mut conn = setup_test_db();

        // Initially empty
        assert_eq!(conn.count_schemas("s1").unwrap(), 0);
        assert_eq!(conn.count_elements("s1").unwrap(), 0);

        // Insert schema
        let schema = ContractSchemaInput {
            schema_uid: "schema1".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            schema_kind: "protobuf".to_string(),
            file_path: "test.proto".to_string(),
            package_name: None,
            syntax_version: None,
            content_hash: "abc".to_string(),
            imports_json: None,
            options_json: None,
            extractor: "proto-parser:0.1.0".to_string(),
        };
        conn.insert_contract_schema(&schema).unwrap();

        assert_eq!(conn.count_schemas("s1").unwrap(), 1);

        // Insert elements
        let elements = vec![ContractElementInput {
            element_uid: "e1".to_string(),
            schema_uid: "schema1".to_string(),
            element_kind: "message".to_string(),
            name: "Test".to_string(),
            full_name: "Test".to_string(),
            parent_element_uid: None,
            line_start: None,
            line_end: None,
            metadata_json: None,
        }];
        conn.insert_contract_elements(&elements).unwrap();

        assert_eq!(conn.count_elements("s1").unwrap(), 1);
    }

    #[test]
    fn filter_by_kind() {
        let mut conn = setup_test_db();

        // Insert two schemas of different kinds
        conn.insert_contract_schema(&ContractSchemaInput {
            schema_uid: "s1".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            schema_kind: "protobuf".to_string(),
            file_path: "a.proto".to_string(),
            package_name: None,
            syntax_version: None,
            content_hash: "a".to_string(),
            imports_json: None,
            options_json: None,
            extractor: "proto-parser:0.1.0".to_string(),
        })
        .unwrap();

        conn.insert_contract_schema(&ContractSchemaInput {
            schema_uid: "s2".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            schema_kind: "grpc".to_string(),
            file_path: "b.proto".to_string(),
            package_name: None,
            syntax_version: None,
            content_hash: "b".to_string(),
            imports_json: None,
            options_json: None,
            extractor: "proto-parser:0.1.0".to_string(),
        })
        .unwrap();

        // Filter by kind
        let protobuf = conn
            .list_contract_schemas("s1", Some("protobuf"))
            .unwrap();
        assert_eq!(protobuf.len(), 1);
        assert_eq!(protobuf[0].file_path, "a.proto");

        let grpc = conn.list_contract_schemas("s1", Some("grpc")).unwrap();
        assert_eq!(grpc.len(), 1);
        assert_eq!(grpc[0].file_path, "b.proto");

        let all = conn.list_contract_schemas("s1", None).unwrap();
        assert_eq!(all.len(), 2);
    }
}
