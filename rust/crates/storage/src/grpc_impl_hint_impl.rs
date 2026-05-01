//! gRPC implementation hint storage queries (GR-1A).
//!
//! Provides queries needed for detecting Java gRPC server implementation hints:
//! 1. Find classes extending *ImplBase (via IMPLEMENTS edges)
//! 2. Link ImplBase classes to proto services (via generated_code_mappings)

use rusqlite::params;

use crate::connection::StorageConnection;
use crate::error::StorageError;

/// A Java class that extends a gRPC ImplBase class.
#[derive(Debug, Clone)]
pub struct ImplBaseExtension {
    /// The stable_key of the implementing class
    pub impl_class_key: String,
    /// The name of the implementing class
    pub impl_class_name: String,
    /// The qualified name of the implementing class
    pub impl_class_qualified_name: Option<String>,
    /// The target of the extends (e.g., "GreeterGrpc.GreeterImplBase")
    pub impl_base_target: String,
    /// Source file path
    pub source_file: String,
    /// Line number of the class declaration
    pub line_start: Option<i64>,
    /// Column number
    pub col_start: Option<i64>,
}

/// A generated_code_mapping for an ImplBase class.
#[derive(Debug, Clone)]
pub struct ImplBaseMapping {
    /// The mapping UID
    pub mapping_uid: String,
    /// The proto service element UID
    pub schema_element_uid: String,
    /// The generated symbol key (contains the ImplBase class)
    pub generated_symbol_key: String,
    /// Confidence of the CS-2A mapping
    pub confidence: f64,
}

impl StorageConnection {
    /// Query Java classes that extend *Grpc.*ImplBase (raw storage types).
    ///
    /// Looks for IMPLEMENTS edges where:
    /// - edge type = 'IMPLEMENTS'
    /// - metadata_json contains "extends" (not implements)
    /// - target_key ends with "ImplBase"
    ///
    /// Returns information about the implementing class and what it extends.
    /// Used by `GrpcImplHintReadPort` impl which converts to indexer types.
    pub fn query_impl_base_extensions_raw(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ImplBaseExtension>, StorageError> {
        let conn = self.connection();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                n.stable_key,
                n.name,
                n.qualified_name,
                ee.target_key,
                f.path,
                ee.line_start,
                ee.col_start
            FROM extraction_edges ee
            JOIN nodes n ON ee.source_node_uid = n.node_uid
            JOIN files f ON n.file_uid = f.file_uid
            WHERE ee.snapshot_uid = ?
              AND ee.type = 'IMPLEMENTS'
              AND ee.metadata_json LIKE '%"relation":"extends"%'
              AND ee.target_key LIKE '%ImplBase'
              AND f.language = 'java'
            "#,
        )?;

        let rows = stmt.query_map(params![snapshot_uid], |row| {
            Ok(ImplBaseExtension {
                impl_class_key: row.get(0)?,
                impl_class_name: row.get(1)?,
                impl_class_qualified_name: row.get(2)?,
                impl_base_target: row.get(3)?,
                source_file: row.get(4)?,
                line_start: row.get(5)?,
                col_start: row.get(6)?,
            })
        })?;

        let mut extensions = Vec::new();
        for row_result in rows {
            extensions.push(row_result?);
        }

        Ok(extensions)
    }

    /// Query generated_code_mappings for ImplBase classes (raw storage types).
    ///
    /// Finds CS-2A mappings where the generated_symbol_key contains "ImplBase".
    /// These mappings link generated gRPC stub classes to proto service elements.
    /// Used by `GrpcImplHintReadPort` impl which converts to indexer types.
    pub fn query_impl_base_mappings_raw(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ImplBaseMapping>, StorageError> {
        let conn = self.connection();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                mapping_uid,
                schema_element_uid,
                generated_symbol_key,
                confidence
            FROM generated_code_mappings
            WHERE snapshot_uid = ?
              AND generated_symbol_key LIKE '%ImplBase%'
            "#,
        )?;

        let rows = stmt.query_map(params![snapshot_uid], |row| {
            Ok(ImplBaseMapping {
                mapping_uid: row.get(0)?,
                schema_element_uid: row.get(1)?,
                generated_symbol_key: row.get(2)?,
                confidence: row.get(3)?,
            })
        })?;

        let mut mappings = Vec::new();
        for row_result in rows {
            mappings.push(row_result?);
        }

        Ok(mappings)
    }

    /// Insert boundary contract associations.
    ///
    /// Links a boundary_interaction_surface to a contract_element.
    pub fn insert_boundary_contracts(
        &mut self,
        contracts: &[BoundaryContract],
    ) -> Result<usize, StorageError> {
        if contracts.is_empty() {
            return Ok(0);
        }

        let conn = self.connection_mut();
        let mut stmt = conn.prepare(
            "INSERT OR IGNORE INTO boundary_contracts (
                association_uid, surface_uid, contract_element_uid,
                contract_kind, association_basis, confidence, evidence_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )?;

        let mut inserted = 0;
        for contract in contracts {
            let result = stmt.execute(params![
                contract.association_uid,
                contract.surface_uid,
                contract.contract_element_uid,
                contract.contract_kind,
                contract.association_basis,
                contract.confidence,
                contract.evidence_json,
            ])?;
            inserted += result;
        }

        Ok(inserted)
    }
}

/// A boundary-to-contract association.
#[derive(Debug, Clone)]
pub struct BoundaryContract {
    pub association_uid: String,
    pub surface_uid: String,
    pub contract_element_uid: Option<String>,
    pub contract_kind: String,
    pub association_basis: String,
    pub confidence: f64,
    pub evidence_json: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> StorageConnection {
        let mut conn = StorageConnection::open_in_memory().unwrap();

        conn.connection_mut()
            .execute_batch(
                r#"
                INSERT INTO repos (repo_uid, name, root_path, created_at)
                VALUES ('r1', 'test', '/test', datetime('now'));

                INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at)
                VALUES ('s1', 'r1', 'full', 'ready', datetime('now'));

                INSERT INTO files (file_uid, repo_uid, path, language)
                VALUES ('f1', 'r1', 'src/GreeterImpl.java', 'java');
                "#,
            )
            .unwrap();

        conn
    }

    #[test]
    fn query_impl_base_extensions_finds_extends_impl_base() {
        let mut conn = setup_test_db();

        // Insert a node for the implementing class
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n1', 's1', 'r1', 'r1:GreeterImpl:CLASS', 'SYMBOL', 'CLASS',
                    'GreeterImpl', 'com.example.GreeterImpl', 'f1'
                )"#,
                [],
            )
            .unwrap();

        // Insert an IMPLEMENTS edge with extends relation
        conn.connection_mut()
            .execute(
                r#"INSERT INTO extraction_edges (
                    edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
                    type, resolution, extractor, line_start, col_start, metadata_json
                ) VALUES (
                    'e1', 's1', 'r1', 'n1', 'GreeterGrpc.GreeterImplBase',
                    'IMPLEMENTS', 'STATIC', 'java-core:0.1.0', 10, 1,
                    '{"relation":"extends"}'
                )"#,
                [],
            )
            .unwrap();

        let extensions = conn.query_impl_base_extensions_raw("s1").unwrap();

        assert_eq!(extensions.len(), 1);
        assert_eq!(extensions[0].impl_class_name, "GreeterImpl");
        assert_eq!(extensions[0].impl_base_target, "GreeterGrpc.GreeterImplBase");
        assert_eq!(extensions[0].source_file, "src/GreeterImpl.java");
    }

    #[test]
    fn query_impl_base_extensions_excludes_non_impl_base() {
        let mut conn = setup_test_db();

        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n1', 's1', 'r1', 'r1:Foo:CLASS', 'SYMBOL', 'CLASS',
                    'Foo', 'com.example.Foo', 'f1'
                )"#,
                [],
            )
            .unwrap();

        // Extends something that's NOT ImplBase
        conn.connection_mut()
            .execute(
                r#"INSERT INTO extraction_edges (
                    edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
                    type, resolution, extractor, metadata_json
                ) VALUES (
                    'e1', 's1', 'r1', 'n1', 'SomeOtherClass',
                    'IMPLEMENTS', 'STATIC', 'java-core:0.1.0',
                    '{"relation":"extends"}'
                )"#,
                [],
            )
            .unwrap();

        let extensions = conn.query_impl_base_extensions_raw("s1").unwrap();
        assert!(extensions.is_empty());
    }

    #[test]
    fn query_impl_base_mappings_finds_impl_base_mappings() {
        let mut conn = setup_test_db();

        // Need contract schema and element for FK
        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_schemas (
                    schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                    package_name, content_hash, extractor, parsed_at
                ) VALUES (
                    'cs1', 's1', 'r1', 'protobuf', 'greeter.proto',
                    'example', 'hash1', 'proto-parser:0.1.0', datetime('now')
                )"#,
                [],
            )
            .unwrap();

        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_elements (
                    element_uid, schema_uid, element_kind, name, full_name
                ) VALUES (
                    'ce1', 'cs1', 'service', 'Greeter', 'example.Greeter'
                )"#,
                [],
            )
            .unwrap();

        conn.connection_mut()
            .execute(
                r#"INSERT INTO generated_code_mappings (
                    mapping_uid, snapshot_uid, schema_element_uid, generated_symbol_key,
                    language, generated_file, mapping_basis, confidence, created_at
                ) VALUES (
                    'm1', 's1', 'ce1', 'r1:src/GreeterGrpc.java#GreeterGrpc.GreeterImplBase:SYMBOL:CLASS',
                    'java', 'src/GreeterGrpc.java', 'filename_convention', 0.85, datetime('now')
                )"#,
                [],
            )
            .unwrap();

        let mappings = conn.query_impl_base_mappings_raw("s1").unwrap();

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].schema_element_uid, "ce1");
        assert!(mappings[0].generated_symbol_key.contains("GreeterImplBase"));
    }

    #[test]
    fn insert_boundary_contracts_persists_rows() {
        let mut conn = setup_test_db();

        // Insert contract schema and element (required for FK)
        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_schemas (
                    schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                    package_name, content_hash, extractor, parsed_at
                ) VALUES (
                    'cs1', 's1', 'r1', 'protobuf', 'greeter.proto',
                    'example', 'hash1', 'proto-parser:0.1.0', datetime('now')
                )"#,
                [],
            )
            .unwrap();

        conn.connection_mut()
            .execute(
                r#"INSERT INTO contract_elements (
                    element_uid, schema_uid, element_kind, name, full_name
                ) VALUES (
                    'ce1', 'cs1', 'service', 'Greeter', 'example.Greeter'
                )"#,
                [],
            )
            .unwrap();

        // Insert a boundary surface (required FK)
        conn.connection_mut()
            .execute(
                r#"INSERT INTO boundary_interaction_surfaces (
                    surface_uid, snapshot_uid, repo_uid, boundary_scope, channel_kind,
                    direction, protocol, protocol_family, interaction_pattern, endpoint_locality,
                    symbol_stable_key, source_file, line_start, line_end, col_start, col_end,
                    extractor, basis, confidence, evidence_json
                ) VALUES (
                    'surf1', 's1', 'r1', 'unknown', 'grpc',
                    'provider', 'grpc', 'rpc', 'unknown', 'unknown',
                    'r1:GreeterImpl:CLASS', 'src/GreeterImpl.java', 10, 50, 1, 1,
                    'grpc_impl_hint_java', 'extends_impl_base', 0.85,
                    '{"impl_base":"GreeterGrpc.GreeterImplBase"}'
                )"#,
                [],
            )
            .unwrap();

        let contracts = vec![BoundaryContract {
            association_uid: "bc1".to_string(),
            surface_uid: "surf1".to_string(),
            contract_element_uid: Some("ce1".to_string()),
            contract_kind: "grpc_service".to_string(),
            association_basis: "generated_code_mapping".to_string(),
            confidence: 0.85,
            evidence_json: Some(r#"{"mapping_uid":"m1"}"#.to_string()),
        }];

        let inserted = conn.insert_boundary_contracts(&contracts).unwrap();
        assert_eq!(inserted, 1);

        // Verify it was inserted
        let count: i64 = conn
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM boundary_contracts WHERE association_uid = 'bc1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
