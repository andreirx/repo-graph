//! Generated code mapping read storage implementation (CS-2A).
//!
//! Implements `GeneratedCodeMappingReadPort` (from indexer) for `StorageConnection`.
//! Provides query data for the java_code_mapper.

use rusqlite::params;

use repo_graph_indexer::java_code_mapper::{ContractElementContext, JavaSymbol, ProtoOptions};
use repo_graph_indexer::storage_port::GeneratedCodeMappingReadPort;

use crate::connection::StorageConnection;
use crate::error::StorageError;

impl GeneratedCodeMappingReadPort for StorageConnection {
    type Error = StorageError;

    fn query_contract_elements_with_options(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ContractElementContext>, StorageError> {
        let conn = self.connection();

        // Query top-level elements (message, enum, service) joined with schema options.
        // Note: parent_element_uid IS NULL ensures we only get top-level elements.
        let mut stmt = conn.prepare(
            r#"
            SELECT
                ce.element_uid,
                ce.element_kind,
                ce.name,
                ce.full_name,
                cs.file_path,
                cs.package_name,
                cs.options_json
            FROM contract_elements ce
            JOIN contract_schemas cs ON ce.schema_uid = cs.schema_uid
            WHERE cs.snapshot_uid = ?
              AND ce.element_kind IN ('message', 'enum', 'service')
              AND ce.parent_element_uid IS NULL
            "#,
        )?;

        let rows = stmt.query_map(params![snapshot_uid], |row| {
            let element_uid: String = row.get(0)?;
            let element_kind: String = row.get(1)?;
            let name: String = row.get(2)?;
            let full_name: String = row.get(3)?;
            let schema_file: String = row.get(4)?;
            let proto_package: Option<String> = row.get(5)?;
            let options_json: Option<String> = row.get(6)?;

            Ok((
                element_uid,
                element_kind,
                name,
                full_name,
                schema_file,
                proto_package,
                options_json,
            ))
        })?;

        let mut elements = Vec::new();
        for row_result in rows {
            let (element_uid, element_kind, name, full_name, schema_file, proto_package, options_json) =
                row_result?;

            let options = options_json
                .as_deref()
                .map(ProtoOptions::from_json)
                .unwrap_or_default();

            elements.push(ContractElementContext {
                element_uid,
                element_kind,
                name,
                full_name,
                schema_file,
                proto_package,
                options,
            });
        }

        Ok(elements)
    }

    fn query_java_symbols(&self, snapshot_uid: &str) -> Result<Vec<JavaSymbol>, StorageError> {
        let conn = self.connection();

        // Query Java CLASS/INTERFACE nodes from the snapshot.
        // Join with files to get language and filter to Java.
        let mut stmt = conn.prepare(
            r#"
            SELECT
                n.stable_key,
                n.name,
                n.qualified_name,
                n.subtype,
                f.path
            FROM nodes n
            JOIN files f ON n.file_uid = f.file_uid
            WHERE n.snapshot_uid = ?
              AND f.language = 'java'
              AND n.subtype IN ('CLASS', 'INTERFACE')
              AND n.qualified_name IS NOT NULL
            "#,
        )?;

        let rows = stmt.query_map(params![snapshot_uid], |row| {
            let stable_key: String = row.get(0)?;
            let name: String = row.get(1)?;
            let qualified_name: String = row.get(2)?;
            let subtype: String = row.get(3)?;
            let file_path: String = row.get(4)?;

            Ok(JavaSymbol {
                stable_key,
                name,
                qualified_name,
                subtype,
                file_path,
            })
        })?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        Ok(symbols)
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

        conn
    }

    #[test]
    fn query_contract_elements_with_options_returns_top_level_only() {
        let mut conn = setup_test_db();

        // Insert a contract schema with options
        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_schemas (
                    schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                    package_name, content_hash, extractor, parsed_at, options_json
                ) VALUES (
                    'schema-1', 's1', 'r1', 'protobuf', 'test.proto',
                    'test.package', 'hash1', 'proto-parser:0.1.0', datetime('now'),
                    '{"java_package":"org.test","java_outer_classname":"TestProtos"}'
                )"#,
                [],
            )
            .unwrap();

        // Insert top-level message
        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_elements (
                    element_uid, schema_uid, element_kind, name, full_name, parent_element_uid
                ) VALUES (
                    'elem-msg-1', 'schema-1', 'message', 'MyMessage', 'test.package.MyMessage', NULL
                )"#,
                [],
            )
            .unwrap();

        // Insert nested field (should be excluded)
        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_elements (
                    element_uid, schema_uid, element_kind, name, full_name, parent_element_uid
                ) VALUES (
                    'elem-field-1', 'schema-1', 'field', 'name', 'test.package.MyMessage.name', 'elem-msg-1'
                )"#,
                [],
            )
            .unwrap();

        // Insert top-level enum
        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_elements (
                    element_uid, schema_uid, element_kind, name, full_name, parent_element_uid
                ) VALUES (
                    'elem-enum-1', 'schema-1', 'enum', 'MyEnum', 'test.package.MyEnum', NULL
                )"#,
                [],
            )
            .unwrap();

        let elements = conn.query_contract_elements_with_options("s1").unwrap();

        // Should have 2 top-level elements (message + enum), not the field
        assert_eq!(elements.len(), 2);

        let msg = elements.iter().find(|e| e.name == "MyMessage").unwrap();
        assert_eq!(msg.element_kind, "message");
        assert_eq!(msg.full_name, "test.package.MyMessage");
        assert_eq!(msg.proto_package, Some("test.package".to_string()));
        assert_eq!(msg.options.java_package, Some("org.test".to_string()));
        assert_eq!(
            msg.options.java_outer_classname,
            Some("TestProtos".to_string())
        );

        let enm = elements.iter().find(|e| e.name == "MyEnum").unwrap();
        assert_eq!(enm.element_kind, "enum");
    }

    #[test]
    fn query_java_symbols_filters_to_class_interface() {
        let mut conn = setup_test_db();

        // Insert a Java file
        conn.connection_mut()
            .execute(
                "INSERT INTO files (file_uid, repo_uid, path, language) VALUES ('f1', 'r1', 'src/Test.java', 'java')",
                [],
            )
            .unwrap();

        // Insert CLASS node
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n1', 's1', 'r1', 'r1:Test:CLASS', 'SYMBOL', 'CLASS',
                    'TestClass', 'com.example.TestClass', 'f1'
                )"#,
                [],
            )
            .unwrap();

        // Insert INTERFACE node
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n2', 's1', 'r1', 'r1:TestIface:INTERFACE', 'SYMBOL', 'INTERFACE',
                    'TestInterface', 'com.example.TestInterface', 'f1'
                )"#,
                [],
            )
            .unwrap();

        // Insert METHOD node (should be excluded)
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n3', 's1', 'r1', 'r1:doSomething:METHOD', 'SYMBOL', 'METHOD',
                    'doSomething', 'com.example.TestClass.doSomething', 'f1'
                )"#,
                [],
            )
            .unwrap();

        let symbols = conn.query_java_symbols("s1").unwrap();

        // Should have 2 symbols (class + interface), not the method
        assert_eq!(symbols.len(), 2);

        let class_sym = symbols.iter().find(|s| s.name == "TestClass").unwrap();
        assert_eq!(class_sym.subtype, "CLASS");
        assert_eq!(class_sym.qualified_name, "com.example.TestClass");
        assert_eq!(class_sym.file_path, "src/Test.java");

        let iface_sym = symbols.iter().find(|s| s.name == "TestInterface").unwrap();
        assert_eq!(iface_sym.subtype, "INTERFACE");
    }

    #[test]
    fn query_java_symbols_excludes_non_java_files() {
        let mut conn = setup_test_db();

        // Insert a TypeScript file
        conn.connection_mut()
            .execute(
                "INSERT INTO files (file_uid, repo_uid, path, language) VALUES ('f1', 'r1', 'src/test.ts', 'typescript')",
                [],
            )
            .unwrap();

        // Insert CLASS node (in TS file - should be excluded)
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n1', 's1', 'r1', 'r1:Test:CLASS', 'SYMBOL', 'CLASS',
                    'TestClass', 'TestClass', 'f1'
                )"#,
                [],
            )
            .unwrap();

        let symbols = conn.query_java_symbols("s1").unwrap();

        // Should have 0 symbols (TS file excluded)
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn empty_snapshot_returns_empty() {
        let conn = setup_test_db();

        let elements = conn.query_contract_elements_with_options("s1").unwrap();
        assert!(elements.is_empty());

        let symbols = conn.query_java_symbols("s1").unwrap();
        assert!(symbols.is_empty());
    }
}
