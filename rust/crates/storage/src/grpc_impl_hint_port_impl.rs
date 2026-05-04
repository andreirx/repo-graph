//! gRPC implementation hint port implementation (GR-1A).
//!
//! Implements `GrpcImplHintReadPort` and `GrpcImplHintStorePort` from the
//! indexer crate for `StorageConnection`.
//!
//! This bridges the storage queries in `grpc_impl_hint_impl.rs` with the
//! port traits defined in `repo-graph-indexer::storage_port`.

use rusqlite::params;

use repo_graph_indexer::grpc_impl_hint::{ImplBaseExtensionInput, ImplBaseMappingInput};
use repo_graph_indexer::storage_port::{
    GrpcImplContractInput, GrpcImplHintReadPort, GrpcImplHintStorePort, GrpcImplSurfaceInput,
};

use crate::connection::StorageConnection;
use crate::error::StorageError;

impl GrpcImplHintReadPort for StorageConnection {
    type Error = StorageError;

    fn query_impl_base_extensions(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ImplBaseExtensionInput>, StorageError> {
        // Call the existing storage query and convert types
        let storage_results = self.query_impl_base_extensions_raw(snapshot_uid)?;

        Ok(storage_results
            .into_iter()
            .map(|ext| ImplBaseExtensionInput {
                impl_class_key: ext.impl_class_key,
                impl_class_name: ext.impl_class_name,
                impl_class_qualified_name: ext.impl_class_qualified_name,
                impl_base_target: ext.impl_base_target,
                source_file: ext.source_file,
                line_start: ext.line_start.unwrap_or(0),
                col_start: ext.col_start.unwrap_or(0),
            })
            .collect())
    }

    fn query_impl_base_mappings(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ImplBaseMappingInput>, StorageError> {
        // Call the existing storage query and convert types
        let storage_results = self.query_impl_base_mappings_raw(snapshot_uid)?;

        Ok(storage_results
            .into_iter()
            .map(|mapping| ImplBaseMappingInput {
                mapping_uid: mapping.mapping_uid,
                schema_element_uid: mapping.schema_element_uid,
                generated_symbol_key: mapping.generated_symbol_key,
                confidence: mapping.confidence,
            })
            .collect())
    }
}

impl GrpcImplHintStorePort for StorageConnection {
    type Error = StorageError;

    fn insert_grpc_impl_surfaces(
        &mut self,
        surfaces: &[GrpcImplSurfaceInput],
    ) -> Result<usize, StorageError> {
        if surfaces.is_empty() {
            return Ok(0);
        }

        let conn = self.connection_mut();
        let mut stmt = conn.prepare(
            r#"
            INSERT OR IGNORE INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                transport_class, provenance, confidence_basis,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality,
                symbol_stable_key, source_file, line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )?;

        let mut inserted = 0;
        for surface in surfaces {
            let result = stmt.execute(params![
                surface.surface_uid,
                surface.snapshot_uid,
                surface.repo_uid,
                "unknown",             // boundary_scope - unknown until endpoint proven
                "grpc_channel",        // channel_kind (matches ChannelKind::GrpcChannel)
                "provider",            // direction - server implementation
                "schema_rpc",          // transport_class
                "inferred",            // provenance - hint-grade
                "extends_impl_base",   // confidence_basis
                "grpc",                // protocol
                "rpc",                 // protocol_family
                "unknown",             // interaction_pattern
                "unknown",             // endpoint_locality
                surface.symbol_stable_key,
                surface.source_file,
                surface.line_start,
                surface.line_end,
                surface.col_start,
                surface.col_end,
                "grpc_impl_hint_java", // extractor
                "extends_impl_base",   // basis
                0.85f64,               // confidence - hint-grade
                surface.evidence_json,
            ])?;
            inserted += result;
        }

        Ok(inserted)
    }

    fn insert_grpc_impl_contracts(
        &mut self,
        contracts: &[GrpcImplContractInput],
    ) -> Result<usize, StorageError> {
        if contracts.is_empty() {
            return Ok(0);
        }

        let conn = self.connection_mut();
        let mut stmt = conn.prepare(
            r#"
            INSERT OR IGNORE INTO boundary_contracts (
                association_uid, surface_uid, contract_element_uid,
                contract_kind, association_basis, confidence, evidence_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )?;

        let mut inserted = 0;
        for contract in contracts {
            let result = stmt.execute(params![
                contract.association_uid,
                contract.surface_uid,
                contract.contract_element_uid,
                "grpc_service",            // contract_kind
                "generated_code_mapping",  // association_basis - via CS-2A
                0.85f64,                   // confidence - inherited from CS-2A
                contract.evidence_json,
            ])?;
            inserted += result;
        }

        Ok(inserted)
    }
}

// ── GR-1B: Registration proof port implementation ─────────────────────

impl repo_graph_indexer::storage_port::GrpcRegistrationProofPort for StorageConnection {
    type Error = StorageError;

    fn query_add_service_calls(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<repo_graph_indexer::storage_port::AddServiceCallInput>, StorageError> {
        let storage_results = self.query_add_service_calls_raw(snapshot_uid)?;

        Ok(storage_results
            .into_iter()
            .map(|call| repo_graph_indexer::storage_port::AddServiceCallInput {
                source_method_key: call.source_method_key,
                source_method_name: call.source_method_name,
                source_file: call.source_file,
                line_start: call.line_start,
                call_pattern: call.call_pattern,
            })
            .collect())
    }

    fn find_grpc_impl_surface_by_class(
        &self,
        snapshot_uid: &str,
        class_name: &str,
        registration_source_file: Option<&str>,
    ) -> Result<Option<repo_graph_indexer::storage_port::GrpcImplSurfaceMatch>, StorageError> {
        let result = self.find_grpc_impl_surface_by_class_in_context(
            snapshot_uid,
            class_name,
            registration_source_file,
        )?;

        Ok(result.map(|s| repo_graph_indexer::storage_port::GrpcImplSurfaceMatch {
            surface_uid: s.surface_uid,
            symbol_stable_key: s.symbol_stable_key,
            source_file: s.source_file,
            confidence: s.confidence,
        }))
    }

    fn boost_grpc_impl_confidence(
        &mut self,
        surface_uid: &str,
        registration_site: &repo_graph_indexer::storage_port::RegistrationSiteInput,
    ) -> Result<bool, StorageError> {
        let site = crate::grpc_impl_hint_impl::RegistrationSite {
            file: registration_site.file.clone(),
            line: registration_site.line,
            method: registration_site.method.clone(),
            pattern: registration_site.pattern.clone(),
        };
        self.boost_grpc_impl_confidence(surface_uid, &site)
    }
}

// ── GR-2A: Client hint port implementation ────────────────────────────────

impl repo_graph_indexer::storage_port::GrpcClientHintReadPort for StorageConnection {
    type Error = StorageError;

    fn query_grpc_stub_creations(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<repo_graph_indexer::storage_port::StubCreationInput>, StorageError> {
        let storage_results = self.query_grpc_stub_creations_raw(snapshot_uid)?;

        Ok(storage_results
            .into_iter()
            .map(|call| repo_graph_indexer::storage_port::StubCreationInput {
                creator_stable_key: call.creator_stable_key,
                creator_name: call.creator_name,
                source_file: call.source_file,
                line_start: call.line_start.unwrap_or(0),
                col_start: call.col_start.unwrap_or(0),
                call_pattern: call.call_pattern,
            })
            .collect())
    }

    fn query_grpc_service_mappings(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<repo_graph_indexer::storage_port::GrpcServiceMappingInput>, StorageError> {
        // Query proto service elements that have at least one CS-2A mapping
        // to a gRPC stub class. Join contract_elements with generated_code_mappings.
        //
        // This fixes the inner-class problem: CS-2A maps inner classes like
        // GreeterImplBase, GreeterBlockingStub, etc. — all pointing to the same
        // service element. We group by service to get one row per service.
        let conn = self.connection();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                ce.element_uid AS service_element_uid,
                ce.name AS service_name,
                MIN(gcm.mapping_uid) AS mapping_uid,
                MAX(gcm.confidence) AS confidence
            FROM contract_elements ce
            JOIN generated_code_mappings gcm ON gcm.schema_element_uid = ce.element_uid
            WHERE gcm.snapshot_uid = ?
              AND ce.element_kind = 'service'
              AND gcm.generated_symbol_key LIKE '%Grpc%'
            GROUP BY ce.element_uid, ce.name
            "#,
        )?;

        let rows = stmt.query_map(rusqlite::params![snapshot_uid], |row| {
            Ok(repo_graph_indexer::storage_port::GrpcServiceMappingInput {
                service_element_uid: row.get(0)?,
                service_name: row.get(1)?,
                mapping_uid: row.get(2)?,
                confidence: row.get(3)?,
            })
        })?;

        let mut services = Vec::new();
        for row_result in rows {
            services.push(row_result?);
        }

        Ok(services)
    }
}

impl repo_graph_indexer::storage_port::GrpcClientHintStorePort for StorageConnection {
    type Error = StorageError;

    fn insert_grpc_client_surfaces(
        &mut self,
        surfaces: &[repo_graph_indexer::storage_port::GrpcClientSurfaceInput],
    ) -> Result<usize, StorageError> {
        if surfaces.is_empty() {
            return Ok(0);
        }

        let conn = self.connection_mut();
        let mut stmt = conn.prepare(
            r#"
            INSERT OR IGNORE INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                transport_class, provenance, confidence_basis,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality,
                symbol_stable_key, source_file, line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )?;

        let mut inserted = 0;
        for surface in surfaces {
            let result = stmt.execute(rusqlite::params![
                surface.surface_uid,
                surface.snapshot_uid,
                surface.repo_uid,
                "unknown",             // boundary_scope - unknown until endpoint proven
                "grpc_channel",        // channel_kind (matches ChannelKind::GrpcChannel)
                "consumer",            // direction - CLIENT stub creation
                "schema_rpc",          // transport_class
                "inferred",            // provenance - hint-grade
                "stub_creation",       // confidence_basis
                "grpc",                // protocol
                "rpc",                 // protocol_family
                "unknown",             // interaction_pattern
                "unknown",             // endpoint_locality
                surface.symbol_stable_key,
                surface.source_file,
                surface.line_start,
                surface.line_end,
                surface.col_start,
                surface.col_end,
                "grpc_client_hint_java", // extractor
                "stub_creation",       // basis
                0.85f64,               // confidence - hint-grade
                surface.evidence_json,
            ])?;
            inserted += result;
        }

        Ok(inserted)
    }

    fn insert_grpc_client_contracts(
        &mut self,
        contracts: &[repo_graph_indexer::storage_port::GrpcClientContractInput],
    ) -> Result<usize, StorageError> {
        if contracts.is_empty() {
            return Ok(0);
        }

        let conn = self.connection_mut();
        let mut stmt = conn.prepare(
            r#"
            INSERT OR IGNORE INTO boundary_contracts (
                association_uid, surface_uid, contract_element_uid,
                contract_kind, association_basis, confidence, evidence_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )?;

        let mut inserted = 0;
        for contract in contracts {
            let result = stmt.execute(rusqlite::params![
                contract.association_uid,
                contract.surface_uid,
                contract.contract_element_uid,
                "grpc_service",            // contract_kind
                "generated_code_mapping",  // association_basis - via CS-2A
                0.85f64,                   // confidence - inherited from CS-2A
                contract.evidence_json,
            ])?;
            inserted += result;
        }

        Ok(inserted)
    }
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

    fn setup_test_db_with_mapping() -> StorageConnection {
        let mut conn = setup_test_db();

        // Insert contract schema and element
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

        conn
    }

    #[test]
    fn query_impl_base_extensions_via_port() {
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

        let extensions: Vec<ImplBaseExtensionInput> =
            GrpcImplHintReadPort::query_impl_base_extensions(&conn, "s1").unwrap();

        assert_eq!(extensions.len(), 1);
        assert_eq!(extensions[0].impl_class_name, "GreeterImpl");
        assert_eq!(extensions[0].impl_base_target, "GreeterGrpc.GreeterImplBase");
        assert_eq!(extensions[0].line_start, 10);
        assert_eq!(extensions[0].col_start, 1);
    }

    #[test]
    fn query_impl_base_mappings_via_port() {
        let mut conn = setup_test_db_with_mapping();

        // Insert a generated code mapping for ImplBase
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

        let mappings: Vec<ImplBaseMappingInput> =
            GrpcImplHintReadPort::query_impl_base_mappings(&conn, "s1").unwrap();

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].schema_element_uid, "ce1");
        assert!(mappings[0].generated_symbol_key.contains("GreeterImplBase"));
        assert!((mappings[0].confidence - 0.85).abs() < 0.001);
    }

    #[test]
    fn insert_grpc_impl_surfaces_via_port() {
        let mut conn = setup_test_db();

        let surfaces = vec![GrpcImplSurfaceInput {
            surface_uid: "surf-1".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            symbol_stable_key: "r1:GreeterImpl:CLASS".to_string(),
            source_file: "src/GreeterImpl.java".to_string(),
            line_start: 10,
            line_end: 50,
            col_start: 1,
            col_end: 1,
            evidence_json: r#"{"impl_base":"GreeterGrpc.GreeterImplBase"}"#.to_string(),
        }];

        let inserted = GrpcImplHintStorePort::insert_grpc_impl_surfaces(&mut conn, &surfaces).unwrap();
        assert_eq!(inserted, 1);

        // Verify persisted values
        let (scope, kind, direction, confidence): (String, String, String, f64) = conn
            .connection()
            .query_row(
                r#"SELECT boundary_scope, channel_kind, direction, confidence
                   FROM boundary_interaction_surfaces WHERE surface_uid = 'surf-1'"#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();

        assert_eq!(scope, "unknown");
        assert_eq!(kind, "grpc_channel");
        assert_eq!(direction, "provider");
        assert!((confidence - 0.85).abs() < 0.001);
    }

    #[test]
    fn insert_grpc_impl_contracts_via_port() {
        let mut conn = setup_test_db_with_mapping();

        // First insert a surface (required FK)
        let surfaces = vec![GrpcImplSurfaceInput {
            surface_uid: "surf-2".to_string(),
            snapshot_uid: "s1".to_string(),
            repo_uid: "r1".to_string(),
            symbol_stable_key: "r1:GreeterImpl:CLASS".to_string(),
            source_file: "src/GreeterImpl.java".to_string(),
            line_start: 10,
            line_end: 50,
            col_start: 1,
            col_end: 1,
            evidence_json: "{}".to_string(),
        }];
        GrpcImplHintStorePort::insert_grpc_impl_surfaces(&mut conn, &surfaces).unwrap();

        // Now insert the contract
        let contracts = vec![GrpcImplContractInput {
            association_uid: "bc-1".to_string(),
            surface_uid: "surf-2".to_string(),
            contract_element_uid: "ce1".to_string(),
            evidence_json: r#"{"mapping_uid":"m1"}"#.to_string(),
        }];

        let inserted = GrpcImplHintStorePort::insert_grpc_impl_contracts(&mut conn, &contracts).unwrap();
        assert_eq!(inserted, 1);

        // Verify persisted values
        let (kind, basis): (String, String) = conn
            .connection()
            .query_row(
                r#"SELECT contract_kind, association_basis
                   FROM boundary_contracts WHERE association_uid = 'bc-1'"#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(kind, "grpc_service");
        assert_eq!(basis, "generated_code_mapping");
    }

    #[test]
    fn empty_inserts_return_zero() {
        let mut conn = setup_test_db();

        let surfaces_count = GrpcImplHintStorePort::insert_grpc_impl_surfaces(&mut conn, &[]).unwrap();
        assert_eq!(surfaces_count, 0);

        let contracts_count = GrpcImplHintStorePort::insert_grpc_impl_contracts(&mut conn, &[]).unwrap();
        assert_eq!(contracts_count, 0);
    }

    // ── End-to-end orchestration test ─────────────────────────────────

    /// Sets up a full test scenario with:
    /// - Implementing class node
    /// - IMPLEMENTS edge (extends *ImplBase)
    /// - Contract schema + element (proto service)
    /// - CS-2A mapping linking ImplBase to proto service
    fn setup_full_scenario() -> StorageConnection {
        let mut conn = setup_test_db_with_mapping();

        // Insert implementing class node
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n1', 's1', 'r1',
                    'r1:src/GreeterImpl.java#GreeterImpl:SYMBOL:CLASS',
                    'SYMBOL', 'CLASS',
                    'GreeterImpl', 'com.example.GreeterImpl', 'f1'
                )"#,
                [],
            )
            .unwrap();

        // Insert IMPLEMENTS edge (extends GreeterGrpc.GreeterImplBase)
        conn.connection_mut()
            .execute(
                r#"INSERT INTO extraction_edges (
                    edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
                    type, resolution, extractor, line_start, col_start, metadata_json
                ) VALUES (
                    'e1', 's1', 'r1', 'n1',
                    'GreeterGrpc.GreeterImplBase',
                    'IMPLEMENTS', 'STATIC', 'java-core:0.1.0', 15, 1,
                    '{"relation":"extends"}'
                )"#,
                [],
            )
            .unwrap();

        // Insert CS-2A mapping linking ImplBase to proto service
        conn.connection_mut()
            .execute(
                r#"INSERT INTO generated_code_mappings (
                    mapping_uid, snapshot_uid, schema_element_uid, generated_symbol_key,
                    language, generated_file, mapping_basis, confidence, created_at
                ) VALUES (
                    'map-implbase', 's1', 'ce1',
                    'r1:build/gen/GreeterGrpc.java#GreeterGrpc.GreeterImplBase:SYMBOL:CLASS',
                    'java', 'build/gen/GreeterGrpc.java',
                    'filename_convention', 0.85, datetime('now')
                )"#,
                [],
            )
            .unwrap();

        conn
    }

    #[test]
    fn end_to_end_detection_via_run_grpc_impl_hint_detection() {
        use repo_graph_indexer::run_grpc_impl_hint_detection;

        let mut conn = setup_full_scenario();

        // Run the full detection pipeline
        let result = run_grpc_impl_hint_detection(&mut conn, "s1", "r1");

        // Verify no errors
        assert!(
            !result.has_error(),
            "Detection had errors: ext={:?}, map={:?}, surf={:?}, contract={:?}",
            result.extension_query_error,
            result.mapping_query_error,
            result.surface_storage_error,
            result.contract_storage_error
        );

        // Verify hints were emitted
        assert_eq!(result.hints_emitted, 1, "Expected 1 hint");
        assert_eq!(result.contracts_emitted, 1, "Expected 1 contract");

        // Verify surface was stored with correct semantics
        let (boundary_scope, channel_kind, direction, provenance, confidence): (
            String,
            String,
            String,
            String,
            f64,
        ) = conn
            .connection()
            .query_row(
                r#"SELECT boundary_scope, channel_kind, direction, provenance, confidence
                   FROM boundary_interaction_surfaces WHERE snapshot_uid = 's1'"#,
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(boundary_scope, "unknown", "Hint-grade: scope unknown");
        assert_eq!(channel_kind, "grpc_channel");
        assert_eq!(direction, "provider");
        assert_eq!(provenance, "inferred", "Hint-grade: inferred provenance");
        assert!((confidence - 0.85).abs() < 0.001);

        // Verify contract links to proto service
        let (contract_kind, association_basis, contract_element_uid): (String, String, String) =
            conn.connection()
                .query_row(
                    r#"SELECT contract_kind, association_basis, contract_element_uid
                       FROM boundary_contracts bc
                       JOIN boundary_interaction_surfaces bis ON bc.surface_uid = bis.surface_uid
                       WHERE bis.snapshot_uid = 's1'"#,
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();

        assert_eq!(contract_kind, "grpc_service");
        assert_eq!(association_basis, "generated_code_mapping");
        assert_eq!(contract_element_uid, "ce1", "Links to proto service element");
    }

    #[test]
    fn detection_is_idempotent() {
        use repo_graph_indexer::run_grpc_impl_hint_detection;

        let mut conn = setup_full_scenario();

        // Run detection twice
        let result1 = run_grpc_impl_hint_detection(&mut conn, "s1", "r1");
        let result2 = run_grpc_impl_hint_detection(&mut conn, "s1", "r1");

        // First run inserts
        assert_eq!(result1.hints_emitted, 1);
        assert_eq!(result1.contracts_emitted, 1);

        // Second run is idempotent (INSERT OR IGNORE → 0 inserted)
        assert!(!result2.has_error());
        assert_eq!(result2.hints_emitted, 0, "Idempotent: second run inserts 0");

        // Total in DB should still be 1
        let count: i64 = conn
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM boundary_interaction_surfaces WHERE snapshot_uid = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn detection_with_no_mapping_match_emits_nothing() {
        let mut conn = setup_test_db();

        // Insert implementing class but no matching CS-2A mapping
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n1', 's1', 'r1', 'key1', 'SYMBOL', 'CLASS',
                    'FooImpl', 'com.FooImpl', 'f1'
                )"#,
                [],
            )
            .unwrap();

        conn.connection_mut()
            .execute(
                r#"INSERT INTO extraction_edges (
                    edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
                    type, resolution, extractor, line_start, col_start, metadata_json
                ) VALUES (
                    'e1', 's1', 'r1', 'n1', 'FooGrpc.FooImplBase',
                    'IMPLEMENTS', 'STATIC', 'java-core:0.1.0', 10, 1,
                    '{"relation":"extends"}'
                )"#,
                [],
            )
            .unwrap();

        // No CS-2A mapping for FooGrpc.FooImplBase

        use repo_graph_indexer::run_grpc_impl_hint_detection;
        let result = run_grpc_impl_hint_detection(&mut conn, "s1", "r1");

        assert!(!result.has_error());
        assert_eq!(result.hints_emitted, 0, "No hints without matching CS-2A mapping");
        assert_eq!(result.contracts_emitted, 0);
    }

    // ── GR-2A: Client stub hint tests ─────────────────────────────────

    /// Sets up a GR-2A test scenario with:
    /// - Client class node
    /// - CALLS edge to GreeterGrpc.newBlockingStub
    /// - Contract schema + element (proto service)
    /// - CS-2A mappings linking gRPC inner classes (ImplBase, BlockingStub, etc.) to proto service
    ///
    /// This mirrors real CS-2A output: CS-2A maps inner classes, not the outer Grpc class.
    fn setup_client_scenario() -> StorageConnection {
        let mut conn = setup_test_db_with_mapping();

        // Add a client file
        conn.connection_mut()
            .execute(
                r#"INSERT INTO files (file_uid, repo_uid, path, language)
                   VALUES ('f2', 'r1', 'src/HelloWorldClient.java', 'java')"#,
                [],
            )
            .unwrap();

        // Insert client class node
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, qualified_name, file_uid
                ) VALUES (
                    'n-client', 's1', 'r1',
                    'r1:src/HelloWorldClient.java#HelloWorldClient.init:SYMBOL:METHOD',
                    'SYMBOL', 'METHOD',
                    'init', 'com.example.HelloWorldClient.init', 'f2'
                )"#,
                [],
            )
            .unwrap();

        // Insert CALLS edge for stub creation
        conn.connection_mut()
            .execute(
                r#"INSERT INTO extraction_edges (
                    edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
                    type, resolution, extractor, line_start, col_start
                ) VALUES (
                    'e-stub', 's1', 'r1', 'n-client',
                    'GreeterGrpc.newBlockingStub(channel)',
                    'CALLS', 'STATIC', 'java-core:0.1.0', 20, 5
                )"#,
                [],
            )
            .unwrap();

        // Insert CS-2A mappings for gRPC inner classes (real CS-2A output shape).
        // CS-2A maps inner classes like GreeterImplBase, GreeterBlockingStub, etc.
        // to the proto service element. All point to the same service.
        conn.connection_mut()
            .execute_batch(
                r#"
                INSERT INTO generated_code_mappings (
                    mapping_uid, snapshot_uid, schema_element_uid, generated_symbol_key,
                    language, generated_file, mapping_basis, confidence, created_at
                ) VALUES
                    ('map-implbase', 's1', 'ce1',
                     'r1:build/gen/GreeterGrpc.java#GreeterGrpc.GreeterImplBase:SYMBOL:CLASS',
                     'java', 'build/gen/GreeterGrpc.java',
                     'filename_convention', 0.85, datetime('now')),
                    ('map-blocking', 's1', 'ce1',
                     'r1:build/gen/GreeterGrpc.java#GreeterGrpc.GreeterBlockingStub:SYMBOL:CLASS',
                     'java', 'build/gen/GreeterGrpc.java',
                     'filename_convention', 0.85, datetime('now')),
                    ('map-future', 's1', 'ce1',
                     'r1:build/gen/GreeterGrpc.java#GreeterGrpc.GreeterFutureStub:SYMBOL:CLASS',
                     'java', 'build/gen/GreeterGrpc.java',
                     'filename_convention', 0.85, datetime('now'));
                "#,
            )
            .unwrap();

        conn
    }

    #[test]
    fn gr2a_end_to_end_client_stub_detection() {
        use repo_graph_indexer::run_grpc_client_hint_detection;

        let mut conn = setup_client_scenario();

        // Run the full detection pipeline
        let result = run_grpc_client_hint_detection(&mut conn, "s1", "r1");

        // Verify no errors
        assert!(
            !result.has_error(),
            "Detection had errors: creation={:?}, map={:?}, surf={:?}, contract={:?}",
            result.creation_query_error,
            result.mapping_query_error,
            result.surface_storage_error,
            result.contract_storage_error
        );

        // Verify hints were emitted
        assert_eq!(result.hints_emitted, 1, "Expected 1 client hint");
        assert_eq!(result.contracts_emitted, 1, "Expected 1 contract");

        // Verify surface was stored with correct semantics
        let (boundary_scope, channel_kind, direction, basis, confidence): (
            String,
            String,
            String,
            String,
            f64,
        ) = conn
            .connection()
            .query_row(
                r#"SELECT boundary_scope, channel_kind, direction, basis, confidence
                   FROM boundary_interaction_surfaces
                   WHERE snapshot_uid = 's1' AND extractor = 'grpc_client_hint_java'"#,
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(boundary_scope, "unknown", "Hint-grade: scope unknown");
        assert_eq!(channel_kind, "grpc_channel");
        assert_eq!(direction, "consumer", "Client stub = consumer direction");
        assert_eq!(basis, "stub_creation");
        assert!((confidence - 0.85).abs() < 0.001);

        // Verify evidence contains stub info
        let evidence_json: String = conn
            .connection()
            .query_row(
                r#"SELECT evidence_json FROM boundary_interaction_surfaces
                   WHERE snapshot_uid = 's1' AND extractor = 'grpc_client_hint_java'"#,
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(evidence_json.contains("\"grpc_class\":\"GreeterGrpc\""));
        assert!(evidence_json.contains("\"stub_method\":\"newBlockingStub\""));
        assert!(evidence_json.contains("\"stub_type\":\"blocking\""));
    }

    #[test]
    fn gr2a_different_stub_types_detected() {
        let mut conn = setup_test_db_with_mapping();

        // Add client file
        conn.connection_mut()
            .execute(
                r#"INSERT INTO files (file_uid, repo_uid, path, language)
                   VALUES ('f2', 'r1', 'src/Client.java', 'java')"#,
                [],
            )
            .unwrap();

        // Insert client node
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, file_uid
                ) VALUES (
                    'n1', 's1', 'r1', 'r1:Client.java#Client:SYMBOL:CLASS',
                    'SYMBOL', 'CLASS', 'Client', 'f2'
                )"#,
                [],
            )
            .unwrap();

        // Insert CALLS edges for all stub types
        conn.connection_mut()
            .execute_batch(
                r#"
                INSERT INTO extraction_edges (
                    edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
                    type, resolution, extractor, line_start
                ) VALUES
                    ('e1', 's1', 'r1', 'n1', 'GreeterGrpc.newBlockingStub(ch)', 'CALLS', 'STATIC', 'java-core:0.1.0', 10),
                    ('e2', 's1', 'r1', 'n1', 'GreeterGrpc.newFutureStub(ch)', 'CALLS', 'STATIC', 'java-core:0.1.0', 11),
                    ('e3', 's1', 'r1', 'n1', 'GreeterGrpc.newStub(ch)', 'CALLS', 'STATIC', 'java-core:0.1.0', 12);
                "#,
            )
            .unwrap();

        // Insert CS-2A mappings for gRPC inner classes (real CS-2A output shape)
        conn.connection_mut()
            .execute_batch(
                r#"
                INSERT INTO generated_code_mappings (
                    mapping_uid, snapshot_uid, schema_element_uid, generated_symbol_key,
                    language, generated_file, mapping_basis, confidence, created_at
                ) VALUES
                    ('map-impl', 's1', 'ce1',
                     'r1:GreeterGrpc.java#GreeterGrpc.GreeterImplBase:SYMBOL:CLASS',
                     'java', 'GreeterGrpc.java', 'filename_convention', 0.85, datetime('now')),
                    ('map-stub', 's1', 'ce1',
                     'r1:GreeterGrpc.java#GreeterGrpc.GreeterBlockingStub:SYMBOL:CLASS',
                     'java', 'GreeterGrpc.java', 'filename_convention', 0.85, datetime('now'));
                "#,
            )
            .unwrap();

        use repo_graph_indexer::run_grpc_client_hint_detection;
        let result = run_grpc_client_hint_detection(&mut conn, "s1", "r1");

        assert!(!result.has_error());
        assert_eq!(result.hints_emitted, 3, "All three stub types detected");
        assert_eq!(result.contracts_emitted, 3);
    }

    #[test]
    fn gr2a_no_hints_without_grpc_mapping() {
        let mut conn = setup_test_db();

        // Add client file
        conn.connection_mut()
            .execute(
                r#"INSERT INTO files (file_uid, repo_uid, path, language)
                   VALUES ('f2', 'r1', 'src/Client.java', 'java')"#,
                [],
            )
            .unwrap();

        // Insert client node
        conn.connection_mut()
            .execute(
                r#"INSERT INTO nodes (
                    node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype,
                    name, file_uid
                ) VALUES (
                    'n1', 's1', 'r1', 'r1:Client.java#Client:SYMBOL:CLASS',
                    'SYMBOL', 'CLASS', 'Client', 'f2'
                )"#,
                [],
            )
            .unwrap();

        // Insert CALLS edge to stub creation (but no CS-2A mapping)
        conn.connection_mut()
            .execute(
                r#"INSERT INTO extraction_edges (
                    edge_uid, snapshot_uid, repo_uid, source_node_uid, target_key,
                    type, resolution, extractor
                ) VALUES (
                    'e1', 's1', 'r1', 'n1', 'UnknownGrpc.newBlockingStub(ch)',
                    'CALLS', 'STATIC', 'java-core:0.1.0'
                )"#,
                [],
            )
            .unwrap();

        use repo_graph_indexer::run_grpc_client_hint_detection;
        let result = run_grpc_client_hint_detection(&mut conn, "s1", "r1");

        assert!(!result.has_error());
        assert_eq!(result.hints_emitted, 0, "No hints without matching CS-2A mapping");
    }
}
