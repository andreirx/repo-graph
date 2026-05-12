//! gRPC server implementation hint detector (GR-1A).
//!
//! Surfaces hints for Java gRPC server implementations by detecting classes
//! that extend `*Grpc.*ImplBase` and linking them to proto services via CS-2A.
//!
//! This is a **discovery slice**, not a runtime-proof slice. The output tells
//! an agent "this class strongly suggests gRPC server implementation" — it does
//! NOT prove registration, endpoint binding, or liveness.
//!
//! # Evidence chain
//!
//! ```text
//! Java class extends *Grpc.*ImplBase
//!     ↓ (IMPLEMENTS edge, relation=extends)
//! *ImplBase class in generated_code_mappings (CS-2A)
//!     ↓ (schema_element_uid)
//! Proto service in contract_elements
//! ```
//!
//! # Emitted semantics
//!
//! - `transport_class = schema_rpc`
//! - `boundary_scope = unknown` (no endpoint evidence)
//! - `provenance = inferred` (inheritance hint, not registration proof)
//! - `confidence = 0.85` (hint-grade, not certainty)

use std::collections::HashMap;

use artifact_contracts::{Provenance, ProvenanceAnchor};
use sha2::{Digest, Sha256};

use crate::storage_port::{
    GrpcImplContractInput, GrpcImplHintReadPort, GrpcImplHintStorePort, GrpcImplSurfaceInput,
};

/// A detected gRPC server implementation hint.
#[derive(Debug, Clone)]
pub struct GrpcImplHint {
    /// Stable key of the implementing class
    pub impl_class_key: String,
    /// Name of the implementing class
    pub impl_class_name: String,
    /// Qualified name of the implementing class
    pub impl_class_qualified_name: Option<String>,
    /// The ImplBase class being extended (e.g., "GreeterGrpc.GreeterImplBase")
    pub impl_base_target: String,
    /// Source file path
    pub source_file: String,
    /// Line number
    pub line_start: i64,
    /// Column number
    pub col_start: i64,
    /// Proto service element UID (from CS-2A mapping) — for FK insert only
    pub proto_service_uid: String,
    /// Proto service name
    pub proto_service_name: String,
    /// CS-2A mapping UID used for association
    pub mapping_uid: String,
    /// Confidence from CS-2A mapping
    pub mapping_confidence: f64,
    /// Proto service full name (e.g., "example.Greeter") — for stable provenance
    pub proto_service_full_name: String,
    /// Proto service element kind (e.g., "service") — for stable provenance
    pub proto_service_kind: String,
    /// Proto schema file path (e.g., "greeter.proto") — for stable provenance
    pub proto_schema_file: String,
}

/// Input: a class extending an ImplBase (from storage query).
#[derive(Debug, Clone)]
pub struct ImplBaseExtensionInput {
    pub impl_class_key: String,
    pub impl_class_name: String,
    pub impl_class_qualified_name: Option<String>,
    pub impl_base_target: String,
    pub source_file: String,
    pub line_start: i64,
    pub col_start: i64,
}

/// Input: a CS-2A mapping for an ImplBase class (from storage query).
#[derive(Debug, Clone)]
pub struct ImplBaseMappingInput {
    pub mapping_uid: String,
    pub schema_element_uid: String,
    pub generated_symbol_key: String,
    pub confidence: f64,
    /// Fully qualified element name (e.g., "example.Greeter") — for stable provenance
    pub element_full_name: String,
    /// Element kind (e.g., "service") — for stable provenance
    pub element_kind: String,
    /// Proto schema file path (e.g., "greeter.proto") — for stable provenance
    pub schema_file_path: String,
}

/// Find gRPC server implementation hints by joining extensions with CS-2A mappings.
///
/// For each class extending `*ImplBase`, looks for a CS-2A mapping where the
/// generated_symbol_key contains the ImplBase class name. If found, produces
/// a hint linking the implementation class to the proto service.
pub fn find_grpc_impl_hints(
    extensions: &[ImplBaseExtensionInput],
    mappings: &[ImplBaseMappingInput],
) -> Vec<GrpcImplHint> {
    // Index mappings by the ImplBase class name extracted from generated_symbol_key.
    // generated_symbol_key looks like: "repo:path#OuterClass.ImplBaseClass:SYMBOL:CLASS"
    // We want to match on the class name part (e.g., "GreeterGrpc.GreeterImplBase")
    let mut mapping_by_impl_base: HashMap<String, &ImplBaseMappingInput> = HashMap::new();

    for mapping in mappings {
        if let Some(class_name) = extract_class_name_from_symbol_key(&mapping.generated_symbol_key)
        {
            mapping_by_impl_base.insert(class_name, mapping);
        }
    }

    let mut hints = Vec::new();

    for ext in extensions {
        // Try to find a mapping for this ImplBase target
        if let Some(mapping) = mapping_by_impl_base.get(&ext.impl_base_target) {
            // Extract service name from ImplBase (e.g., "GreeterImplBase" -> "Greeter")
            let service_name = extract_service_name_from_impl_base(&ext.impl_base_target)
                .unwrap_or_else(|| ext.impl_base_target.clone());

            hints.push(GrpcImplHint {
                impl_class_key: ext.impl_class_key.clone(),
                impl_class_name: ext.impl_class_name.clone(),
                impl_class_qualified_name: ext.impl_class_qualified_name.clone(),
                impl_base_target: ext.impl_base_target.clone(),
                source_file: ext.source_file.clone(),
                line_start: ext.line_start,
                col_start: ext.col_start,
                proto_service_uid: mapping.schema_element_uid.clone(),
                proto_service_name: service_name,
                mapping_uid: mapping.mapping_uid.clone(),
                mapping_confidence: mapping.confidence,
                proto_service_full_name: mapping.element_full_name.clone(),
                proto_service_kind: mapping.element_kind.clone(),
                proto_schema_file: mapping.schema_file_path.clone(),
            });
        }
    }

    hints
}

/// Extract the class name from a generated_symbol_key.
///
/// Input: "repo:path/File.java#OuterClass.InnerClass:SYMBOL:CLASS"
/// Output: Some("OuterClass.InnerClass")
fn extract_class_name_from_symbol_key(key: &str) -> Option<String> {
    // Find the # separator
    let hash_pos = key.find('#')?;
    let after_hash = &key[hash_pos + 1..];

    // Find the :SYMBOL: or first : after the class name
    let colon_pos = after_hash.find(':')?;
    let class_part = &after_hash[..colon_pos];

    Some(class_part.to_string())
}

/// Extract service name from an ImplBase class name.
///
/// Input: "GreeterGrpc.GreeterImplBase" -> "Greeter"
/// Input: "GreeterImplBase" -> "Greeter"
fn extract_service_name_from_impl_base(impl_base: &str) -> Option<String> {
    // Get the last part after any dots
    let class_name = impl_base.rsplit('.').next()?;

    // Strip "ImplBase" suffix
    if class_name.ends_with("ImplBase") && class_name.len() > 8 {
        Some(class_name[..class_name.len() - 8].to_string())
    } else {
        None
    }
}

/// Generate a deterministic surface UID for a gRPC impl hint.
pub fn generate_surface_uid(snapshot_uid: &str, impl_class_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"grpc_impl_hint:");
    hasher.update(snapshot_uid.as_bytes());
    hasher.update(b":");
    hasher.update(impl_class_key.as_bytes());
    let hash = hasher.finalize();
    format!(
        "grpc-hint-{:x}",
        &hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate a deterministic association UID for a boundary contract.
pub fn generate_association_uid(surface_uid: &str, contract_element_uid: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"grpc_contract:");
    hasher.update(surface_uid.as_bytes());
    hasher.update(b":");
    hasher.update(contract_element_uid.as_bytes());
    let hash = hasher.finalize();
    format!(
        "grpc-bc-{:x}",
        &hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Result of running gRPC impl hint detection.
///
/// Surfaces detection statistics and any failures for explicit
/// degradation reporting. Attached to `IndexResult` for visibility.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcImplHintResult {
    /// Number of boundary surfaces (hints) emitted.
    pub hints_emitted: usize,
    /// Number of boundary contracts emitted.
    pub contracts_emitted: usize,
    /// Query error when reading IMPLEMENTS edges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_query_error: Option<String>,
    /// Query error when reading CS-2A mappings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_query_error: Option<String>,
    /// Storage error when persisting surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_storage_error: Option<String>,
    /// Storage error when persisting contracts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_storage_error: Option<String>,
}

impl GrpcImplHintResult {
    pub fn has_error(&self) -> bool {
        self.extension_query_error.is_some()
            || self.mapping_query_error.is_some()
            || self.surface_storage_error.is_some()
            || self.contract_storage_error.is_some()
    }
}

/// Run gRPC implementation hint detection for a snapshot.
///
/// This is the top-level orchestration function for GR-1A. It:
/// 1. Queries Java classes extending `*ImplBase` (via read port)
/// 2. Queries CS-2A mappings for ImplBase classes (via read port)
/// 3. Joins them to produce `GrpcImplHint`s
/// 4. Converts hints to boundary surfaces and contracts
/// 5. Persists via the write port
///
/// # Arguments
///
/// * `storage` - Storage connection implementing both read and write ports
/// * `snapshot_uid` - The snapshot to process
/// * `repo_uid` - The repository UID (needed for surface records)
///
/// # Returns
///
/// A `GrpcImplHintResult` summarizing counts and any errors encountered.
/// Errors are collected rather than fail-fast, allowing partial progress.
pub fn run_grpc_impl_hint_detection<S>(
    storage: &mut S,
    snapshot_uid: &str,
    repo_uid: &str,
) -> GrpcImplHintResult
where
    S: GrpcImplHintReadPort + GrpcImplHintStorePort,
    <S as GrpcImplHintReadPort>::Error: ToString,
    <S as GrpcImplHintStorePort>::Error: ToString,
{
    let mut result = GrpcImplHintResult::default();

    // Step 1: Query extensions
    let extensions = match storage.query_impl_base_extensions(snapshot_uid) {
        Ok(exts) => exts,
        Err(e) => {
            result.extension_query_error = Some(e.to_string());
            return result;
        }
    };

    // Step 2: Query CS-2A mappings
    let mappings = match storage.query_impl_base_mappings(snapshot_uid) {
        Ok(maps) => maps,
        Err(e) => {
            result.mapping_query_error = Some(e.to_string());
            return result;
        }
    };

    // Step 3: Join to produce hints
    let hints = find_grpc_impl_hints(&extensions, &mappings);
    if hints.is_empty() {
        return result;
    }

    // Step 4: Convert to surface/contract inputs
    let mut surfaces = Vec::with_capacity(hints.len());
    let mut contracts = Vec::with_capacity(hints.len());

    for hint in &hints {
        let surface_uid = generate_surface_uid(snapshot_uid, &hint.impl_class_key);
        let association_uid = generate_association_uid(&surface_uid, &hint.proto_service_uid);

        // Build evidence JSON
        let evidence = serde_json::json!({
            "impl_base_target": hint.impl_base_target,
            "proto_service_name": hint.proto_service_name,
            "mapping_uid": hint.mapping_uid,
            "mapping_confidence": hint.mapping_confidence,
        });

        surfaces.push(GrpcImplSurfaceInput {
            surface_uid: surface_uid.clone(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            symbol_stable_key: hint.impl_class_key.clone(),
            source_file: hint.source_file.clone(),
            line_start: hint.line_start,
            line_end: hint.line_start, // Single-line for class declaration
            col_start: hint.col_start,
            col_end: hint.col_start,
            evidence_json: evidence.to_string(),
        });

        // Compute provenance from stable anchors (ACR-5)
        // Pattern: {repo}:{proto_file}#{element_kind}:{full_name}
        let contract_element_stable_key = format!(
            "{}:{}#{}:{}",
            repo_uid, hint.proto_schema_file, hint.proto_service_kind, hint.proto_service_full_name,
        );
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("BoundaryInteractionSurfaces", &hint.impl_class_key),
            ProvenanceAnchor::new("ContractElements", &contract_element_stable_key),
        ])
        .with_extractor("grpc_impl_hint_java:1.0");

        contracts.push(GrpcImplContractInput {
            association_uid,
            surface_uid,
            contract_element_uid: hint.proto_service_uid.clone(),
            evidence_json: serde_json::json!({
                "mapping_uid": hint.mapping_uid,
            })
            .to_string(),
            provenance: Some(provenance),
        });
    }

    // Step 5: Store surfaces
    match storage.insert_grpc_impl_surfaces(&surfaces) {
        Ok(count) => result.hints_emitted = count,
        Err(e) => {
            result.surface_storage_error = Some(e.to_string());
            return result;
        }
    }

    // Step 6: Store contracts
    match storage.insert_grpc_impl_contracts(&contracts) {
        Ok(count) => result.contracts_emitted = count,
        Err(e) => {
            result.contract_storage_error = Some(e.to_string());
            // Don't return early - surfaces were already stored
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_class_name_from_symbol_key_works() {
        let key = "repo:src/GreeterGrpc.java#GreeterGrpc.GreeterImplBase:SYMBOL:CLASS";
        assert_eq!(
            extract_class_name_from_symbol_key(key),
            Some("GreeterGrpc.GreeterImplBase".to_string())
        );
    }

    #[test]
    fn extract_class_name_returns_none_for_invalid_key() {
        assert_eq!(extract_class_name_from_symbol_key("no-hash-here"), None);
        assert_eq!(extract_class_name_from_symbol_key("has#but-no-colon"), None);
    }

    #[test]
    fn extract_service_name_from_impl_base_works() {
        assert_eq!(
            extract_service_name_from_impl_base("GreeterGrpc.GreeterImplBase"),
            Some("Greeter".to_string())
        );
        assert_eq!(
            extract_service_name_from_impl_base("GreeterImplBase"),
            Some("Greeter".to_string())
        );
        assert_eq!(
            extract_service_name_from_impl_base("UserServiceImplBase"),
            Some("UserService".to_string())
        );
    }

    #[test]
    fn extract_service_name_returns_none_for_non_impl_base() {
        assert_eq!(extract_service_name_from_impl_base("SomeOtherClass"), None);
        assert_eq!(extract_service_name_from_impl_base("ImplBase"), None); // Too short
    }

    #[test]
    fn find_grpc_impl_hints_joins_extensions_with_mappings() {
        let extensions = vec![ImplBaseExtensionInput {
            impl_class_key: "r1:GreeterImpl:CLASS".to_string(),
            impl_class_name: "GreeterImpl".to_string(),
            impl_class_qualified_name: Some("com.example.GreeterImpl".to_string()),
            impl_base_target: "GreeterGrpc.GreeterImplBase".to_string(),
            source_file: "src/GreeterImpl.java".to_string(),
            line_start: 10,
            col_start: 1,
        }];

        let mappings = vec![ImplBaseMappingInput {
            mapping_uid: "m1".to_string(),
            schema_element_uid: "service-1".to_string(),
            generated_symbol_key:
                "r1:src/GreeterGrpc.java#GreeterGrpc.GreeterImplBase:SYMBOL:CLASS".to_string(),
            confidence: 0.85,
            element_full_name: "example.Greeter".to_string(),
            element_kind: "service".to_string(),
            schema_file_path: "greeter.proto".to_string(),
        }];

        let hints = find_grpc_impl_hints(&extensions, &mappings);

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].impl_class_name, "GreeterImpl");
        assert_eq!(hints[0].proto_service_uid, "service-1");
        assert_eq!(hints[0].proto_service_name, "Greeter");
        assert_eq!(hints[0].mapping_uid, "m1");
    }

    #[test]
    fn find_grpc_impl_hints_skips_unmatched_extensions() {
        let extensions = vec![ImplBaseExtensionInput {
            impl_class_key: "r1:FooImpl:CLASS".to_string(),
            impl_class_name: "FooImpl".to_string(),
            impl_class_qualified_name: None,
            impl_base_target: "FooGrpc.FooImplBase".to_string(), // No mapping for this
            source_file: "src/FooImpl.java".to_string(),
            line_start: 1,
            col_start: 1,
        }];

        let mappings = vec![ImplBaseMappingInput {
            mapping_uid: "m1".to_string(),
            schema_element_uid: "service-1".to_string(),
            generated_symbol_key: "r1:src/BarGrpc.java#BarGrpc.BarImplBase:SYMBOL:CLASS"
                .to_string(),
            confidence: 0.85,
            element_full_name: "example.Bar".to_string(),
            element_kind: "service".to_string(),
            schema_file_path: "bar.proto".to_string(),
        }];

        let hints = find_grpc_impl_hints(&extensions, &mappings);

        assert!(hints.is_empty());
    }

    #[test]
    fn surface_uid_is_deterministic() {
        let uid1 = generate_surface_uid("snap-1", "r1:GreeterImpl:CLASS");
        let uid2 = generate_surface_uid("snap-1", "r1:GreeterImpl:CLASS");
        assert_eq!(uid1, uid2);

        let uid3 = generate_surface_uid("snap-1", "r1:OtherImpl:CLASS");
        assert_ne!(uid1, uid3);
    }

    #[test]
    fn association_uid_is_deterministic() {
        let uid1 = generate_association_uid("surf-1", "elem-1");
        let uid2 = generate_association_uid("surf-1", "elem-1");
        assert_eq!(uid1, uid2);

        let uid3 = generate_association_uid("surf-1", "elem-2");
        assert_ne!(uid1, uid3);
    }
}
