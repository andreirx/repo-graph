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
    /// The proto service element UID (snapshot-local, for FK only)
    pub schema_element_uid: String,
    /// The generated symbol key (contains the ImplBase class)
    pub generated_symbol_key: String,
    /// Confidence of the CS-2A mapping
    pub confidence: f64,
    /// Fully qualified element name (e.g., "example.Greeter") — for stable provenance
    pub element_full_name: String,
    /// Element kind (e.g., "service") — for stable provenance
    pub element_kind: String,
    /// Proto schema file path (e.g., "greeter.proto") — for stable provenance
    pub schema_file_path: String,
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
    /// Joins contract_elements and contract_schemas to get stable provenance data.
    /// Used by `GrpcImplHintReadPort` impl which converts to indexer types.
    pub fn query_impl_base_mappings_raw(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<ImplBaseMapping>, StorageError> {
        let conn = self.connection();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                gcm.mapping_uid,
                gcm.schema_element_uid,
                gcm.generated_symbol_key,
                gcm.confidence,
                ce.full_name,
                ce.element_kind,
                cs.file_path
            FROM generated_code_mappings gcm
            JOIN contract_elements ce ON ce.element_uid = gcm.schema_element_uid
            JOIN contract_schemas cs ON cs.schema_uid = ce.schema_uid
            WHERE gcm.snapshot_uid = ?
              AND gcm.generated_symbol_key LIKE '%ImplBase%'
            "#,
        )?;

        let rows = stmt.query_map(params![snapshot_uid], |row| {
            Ok(ImplBaseMapping {
                mapping_uid: row.get(0)?,
                schema_element_uid: row.get(1)?,
                generated_symbol_key: row.get(2)?,
                confidence: row.get(3)?,
                element_full_name: row.get(4)?,
                element_kind: row.get(5)?,
                schema_file_path: row.get(6)?,
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

    // ── GR-1B: Registration proof queries ─────────────────────────────────

    /// Query for addService/bindService calls in Java files.
    ///
    /// Finds CALLS edges where target_key contains "addService(" or "bindService(".
    /// Returns the source method, file, line, and the raw call pattern.
    ///
    /// GR-1B uses this to find registration sites and match them to GR-1A surfaces.
    pub fn query_add_service_calls_raw(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<AddServiceCall>, StorageError> {
        let conn = self.connection();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                n.stable_key AS source_method_key,
                n.name AS source_method_name,
                f.path AS source_file,
                ee.line_start,
                ee.target_key AS call_pattern
            FROM extraction_edges ee
            JOIN nodes n ON ee.source_node_uid = n.node_uid
            JOIN files f ON n.file_uid = f.file_uid
            WHERE ee.snapshot_uid = ?
              AND ee.type = 'CALLS'
              AND (ee.target_key LIKE '%addService(%' OR ee.target_key LIKE '%bindService(%')
              AND f.language = 'java'
            "#,
        )?;

        let rows = stmt.query_map(params![snapshot_uid], |row| {
            Ok(AddServiceCall {
                source_method_key: row.get(0)?,
                source_method_name: row.get(1)?,
                source_file: row.get(2)?,
                line_start: row.get(3)?,
                call_pattern: row.get(4)?,
            })
        })?;

        let mut calls = Vec::new();
        for row_result in rows {
            calls.push(row_result?);
        }

        Ok(calls)
    }

    /// Find GR-1A boundary surfaces by implementation class name and source file.
    ///
    /// Used by GR-1B to match addService arguments to existing surfaces.
    ///
    /// **Matching strategy (disambiguation):**
    /// 1. First try same-file match: surface in the same source file as the registration
    ///    (handles inner class pattern, most common for gRPC)
    /// 2. If no same-file match, fall back to any file with that class name
    ///    (handles separate-file implementations, with documented ambiguity risk)
    ///
    /// Delimiter-aware matching prevents `GreeterImpl` from matching `MyGreeterImpl`.
    pub fn find_grpc_impl_surface_by_class(
        &self,
        snapshot_uid: &str,
        class_name: &str,
    ) -> Result<Option<GrpcImplSurface>, StorageError> {
        // This is the legacy signature without source_file context.
        // Delegate to the full signature with None for source_file.
        self.find_grpc_impl_surface_by_class_in_context(snapshot_uid, class_name, None)
    }

    /// Find GR-1A boundary surfaces by class name with source file context.
    ///
    /// **Matching strategy:**
    /// 1. If `registration_source_file` provided, try same-file match first (inner class pattern)
    /// 2. Fall back to cross-file match, but ONLY if exactly one surface matches
    /// 3. If multiple surfaces match the class name across files, return None (refuse to boost)
    ///
    /// This prevents false-positive boosts when multiple classes share the same simple name.
    pub fn find_grpc_impl_surface_by_class_in_context(
        &self,
        snapshot_uid: &str,
        class_name: &str,
        registration_source_file: Option<&str>,
    ) -> Result<Option<GrpcImplSurface>, StorageError> {
        let conn = self.connection();

        // Delimiter-aware exact match patterns:
        // - #ClassName:SYMBOL:CLASS (top-level class after path)
        // - .ClassName:SYMBOL:CLASS (inner class after container)
        let pattern_toplevel = format!("%#{}:SYMBOL:CLASS", class_name);
        let pattern_inner = format!("%.{}:SYMBOL:CLASS", class_name);

        // Strategy 1: Try same-file match first (if source file context provided)
        if let Some(src_file) = registration_source_file {
            let mut stmt = conn.prepare(
                r#"
                SELECT
                    surface_uid,
                    symbol_stable_key,
                    source_file,
                    confidence,
                    evidence_json
                FROM boundary_interaction_surfaces
                WHERE snapshot_uid = ?
                  AND extractor = 'grpc_impl_hint_java'
                  AND source_file = ?
                  AND (symbol_stable_key LIKE ? OR symbol_stable_key LIKE ?)
                ORDER BY surface_uid ASC
                LIMIT 1
                "#,
            )?;

            let mut rows = stmt.query(params![
                snapshot_uid,
                src_file,
                pattern_toplevel,
                pattern_inner
            ])?;

            if let Some(row) = rows.next()? {
                return Ok(Some(GrpcImplSurface {
                    surface_uid: row.get(0)?,
                    symbol_stable_key: row.get(1)?,
                    source_file: row.get(2)?,
                    confidence: row.get(3)?,
                    evidence_json: row.get(4)?,
                }));
            }
        }

        // Strategy 2: Fall back to any-file match, but only if unambiguous
        // Query without LIMIT to detect ambiguity
        let mut stmt = conn.prepare(
            r#"
            SELECT
                surface_uid,
                symbol_stable_key,
                source_file,
                confidence,
                evidence_json
            FROM boundary_interaction_surfaces
            WHERE snapshot_uid = ?
              AND extractor = 'grpc_impl_hint_java'
              AND (symbol_stable_key LIKE ? OR symbol_stable_key LIKE ?)
            ORDER BY surface_uid ASC
            "#,
        )?;

        let mut rows = stmt.query(params![snapshot_uid, pattern_toplevel, pattern_inner])?;

        // Collect all matches to detect ambiguity
        let first_match = match rows.next()? {
            Some(row) => Some(GrpcImplSurface {
                surface_uid: row.get(0)?,
                symbol_stable_key: row.get(1)?,
                source_file: row.get(2)?,
                confidence: row.get(3)?,
                evidence_json: row.get(4)?,
            }),
            None => return Ok(None),
        };

        // Check if there's a second match (ambiguity)
        if rows.next()?.is_some() {
            // Ambiguous: multiple surfaces share the same class name across files.
            // Refuse to boost — caller should record this as degradation.
            return Ok(None);
        }

        // Unambiguous single match
        Ok(first_match)
    }

    /// Boost confidence for a GR-1A surface and append registration evidence.
    ///
    /// GR-1B calls this when it finds a registration site for an implementation.
    /// - Raises confidence from 0.85 to 0.90
    /// - Appends registration site to evidence_json
    ///
    /// Returns true if the update affected a row.
    pub fn boost_grpc_impl_confidence(
        &mut self,
        surface_uid: &str,
        registration_site: &RegistrationSite,
    ) -> Result<bool, StorageError> {
        let conn = self.connection_mut();

        // First, get current evidence_json
        let current_evidence: Option<String> = conn.query_row(
            "SELECT evidence_json FROM boundary_interaction_surfaces WHERE surface_uid = ?",
            params![surface_uid],
            |row| row.get(0),
        )?;

        // Parse and update evidence
        let new_evidence =
            merge_registration_evidence(current_evidence.as_deref(), registration_site);

        // Update confidence and evidence
        let updated = conn.execute(
            r#"
            UPDATE boundary_interaction_surfaces
            SET confidence = 0.90,
                basis = 'extends_impl_base_registered',
                evidence_json = ?
            WHERE surface_uid = ?
              AND extractor = 'grpc_impl_hint_java'
            "#,
            params![new_evidence, surface_uid],
        )?;

        Ok(updated > 0)
    }
}

/// An addService/bindService call site.
#[derive(Debug, Clone)]
pub struct AddServiceCall {
    pub source_method_key: String,
    pub source_method_name: String,
    pub source_file: String,
    pub line_start: Option<i64>,
    pub call_pattern: String,
}

/// A GR-1A boundary surface (minimal fields for GR-1B matching).
#[derive(Debug, Clone)]
pub struct GrpcImplSurface {
    pub surface_uid: String,
    pub symbol_stable_key: String,
    pub source_file: String,
    pub confidence: f64,
    pub evidence_json: Option<String>,
}

/// Registration site evidence for GR-1B.
#[derive(Debug, Clone)]
pub struct RegistrationSite {
    pub file: String,
    pub line: i64,
    pub method: String,
    pub pattern: String,
}

/// Merge registration evidence into existing evidence_json.
fn merge_registration_evidence(current: Option<&str>, site: &RegistrationSite) -> String {
    use serde_json::{json, Value};

    let mut evidence: Value = current
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| json!({}));

    // Ensure registration_sites is an array
    let sites = evidence
        .as_object_mut()
        .unwrap()
        .entry("registration_sites")
        .or_insert_with(|| json!([]));

    if let Some(arr) = sites.as_array_mut() {
        arr.push(json!({
            "file": site.file,
            "line": site.line,
            "method": site.method,
            "pattern": site.pattern
        }));
    }

    serde_json::to_string(&evidence).unwrap_or_else(|_| "{}".to_string())
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

// ── GR-2A: Client stub creation queries ───────────────────────────────

/// A gRPC client stub creation site.
#[derive(Debug, Clone)]
pub struct StubCreationCall {
    /// Stable key of the method/class creating the stub
    pub creator_stable_key: String,
    /// Name of the creator (method or class)
    pub creator_name: String,
    /// Source file path
    pub source_file: String,
    /// Line number
    pub line_start: Option<i64>,
    /// Column number
    pub col_start: Option<i64>,
    /// The raw call pattern (e.g., "GreeterGrpc.newBlockingStub(channel)")
    pub call_pattern: String,
}

impl StorageConnection {
    /// Query for gRPC stub creation calls in Java files.
    ///
    /// Finds CALLS edges where target_key matches `*Grpc.newBlockingStub`,
    /// `*Grpc.newFutureStub`, or `*Grpc.newStub` patterns.
    ///
    /// GR-2A uses this to find client stub creations.
    pub fn query_grpc_stub_creations_raw(
        &self,
        snapshot_uid: &str,
    ) -> Result<Vec<StubCreationCall>, StorageError> {
        let conn = self.connection();

        let mut stmt = conn.prepare(
            r#"
            SELECT
                n.stable_key AS creator_key,
                n.name AS creator_name,
                f.path AS source_file,
                ee.line_start,
                ee.col_start,
                ee.target_key AS call_pattern
            FROM extraction_edges ee
            JOIN nodes n ON ee.source_node_uid = n.node_uid
            JOIN files f ON n.file_uid = f.file_uid
            WHERE ee.snapshot_uid = ?
              AND ee.type = 'CALLS'
              AND f.language = 'java'
              AND (
                ee.target_key LIKE '%Grpc.newBlockingStub%'
                OR ee.target_key LIKE '%Grpc.newFutureStub%'
                OR ee.target_key LIKE '%Grpc.newStub(%'
              )
            "#,
        )?;

        let rows = stmt.query_map(params![snapshot_uid], |row| {
            Ok(StubCreationCall {
                creator_stable_key: row.get(0)?,
                creator_name: row.get(1)?,
                source_file: row.get(2)?,
                line_start: row.get(3)?,
                col_start: row.get(4)?,
                call_pattern: row.get(5)?,
            })
        })?;

        let mut calls = Vec::new();
        for row_result in rows {
            calls.push(row_result?);
        }

        Ok(calls)
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
        assert_eq!(
            extensions[0].impl_base_target,
            "GreeterGrpc.GreeterImplBase"
        );
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
