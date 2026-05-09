//! gRPC client stub hint detector (GR-2A).
//!
//! Surfaces hints for Java gRPC client stub creations by detecting calls to
//! `*Grpc.newBlockingStub`, `*Grpc.newFutureStub`, or `*Grpc.newStub` and
//! linking them to proto services via CS-2A.
//!
//! This is a **discovery slice**, not a runtime-proof slice. The output tells
//! an agent "this code constructs a gRPC client stub" — it does NOT prove
//! the client actually calls the service at runtime.
//!
//! # Evidence chain
//!
//! ```text
//! Java method calls *Grpc.newBlockingStub(channel)
//!     ↓ (CALLS edge, target matches stub factory pattern)
//! *Grpc class in generated_code_mappings (CS-2A)
//!     ↓ (schema_element_uid)
//! Proto service in contract_elements
//! ```
//!
//! # Emitted semantics
//!
//! - `transport_class = schema_rpc`
//! - `boundary_scope = unknown` (no endpoint evidence)
//! - `direction = consumer` (client side)
//! - `provenance = inferred` (stub creation hint, not call-site proof)
//! - `confidence = 0.85` (hint-grade, not certainty)

use std::collections::HashMap;

use artifact_contracts::{Provenance, ProvenanceAnchor};
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::storage_port::{
    GrpcClientContractInput, GrpcClientHintReadPort,
    GrpcClientHintStorePort, GrpcClientSurfaceInput, GrpcServiceMappingInput, StubCreationInput,
};

/// A detected gRPC client stub hint.
#[derive(Debug, Clone)]
pub struct GrpcClientHint {
    /// Stable key of the method/class creating the stub
    pub creator_stable_key: String,
    /// Name of the creator
    pub creator_name: String,
    /// Source file path
    pub source_file: String,
    /// Line number
    pub line_start: i64,
    /// Column number
    pub col_start: i64,
    /// The Grpc class name (e.g., "GreeterGrpc")
    pub grpc_class: String,
    /// The stub factory method (e.g., "newBlockingStub")
    pub stub_method: String,
    /// The stub type (blocking, future, async)
    pub stub_type: StubType,
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

/// Type of gRPC client stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StubType {
    /// Synchronous blocking stub (newBlockingStub)
    Blocking,
    /// ListenableFuture-based stub (newFutureStub)
    Future,
    /// Async stub with StreamObserver (newStub)
    Async,
}

impl StubType {
    pub fn as_str(self) -> &'static str {
        match self {
            StubType::Blocking => "blocking",
            StubType::Future => "future",
            StubType::Async => "async",
        }
    }
}

/// Parsed stub creation pattern.
#[derive(Debug, Clone)]
struct ParsedStubCreation {
    /// The Grpc class name (e.g., "GreeterGrpc")
    pub grpc_class: String,
    /// The stub factory method (e.g., "newBlockingStub")
    pub stub_method: String,
    /// The stub type
    pub stub_type: StubType,
}

/// Parse a stub creation call pattern to extract service info.
///
/// Input: "GreeterGrpc.newBlockingStub(channel)"
/// Output: ParsedStubCreation { grpc_class: "GreeterGrpc", stub_method: "newBlockingStub", stub_type: Blocking }
fn parse_stub_creation(call_pattern: &str) -> Option<ParsedStubCreation> {
    // Regex: (\w+)Grpc\.new(Blocking|Future)?Stub
    // Captures: 1=service prefix, 2=stub type (optional)
    let re = Regex::new(r"(\w+Grpc)\.new(Blocking|Future)?Stub").ok()?;
    let caps = re.captures(call_pattern)?;

    let grpc_class = caps.get(1)?.as_str().to_string();
    let stub_type_str = caps.get(2).map(|m| m.as_str());

    let stub_type = match stub_type_str {
        Some("Blocking") => StubType::Blocking,
        Some("Future") => StubType::Future,
        None => StubType::Async, // newStub (no prefix) is async
        _ => return None,
    };

    let stub_method = match stub_type {
        StubType::Blocking => "newBlockingStub",
        StubType::Future => "newFutureStub",
        StubType::Async => "newStub",
    };

    Some(ParsedStubCreation {
        grpc_class,
        stub_method: stub_method.to_string(),
        stub_type,
    })
}

/// Extract service name from Grpc class name.
///
/// Input: "GreeterGrpc" -> "Greeter"
fn extract_service_name_from_grpc_class(grpc_class: &str) -> Option<String> {
    if grpc_class.ends_with("Grpc") && grpc_class.len() > 4 {
        Some(grpc_class[..grpc_class.len() - 4].to_string())
    } else {
        None
    }
}

/// Find gRPC client stub hints by joining stub creations with proto services.
///
/// For each stub creation call, extracts the service name from the Grpc class
/// (e.g., `GreeterGrpc.newBlockingStub` → `Greeter`) and looks for a matching
/// proto service element that has CS-2A mappings.
///
/// **Why this approach?**
/// CS-2A maps inner classes (`GreeterImplBase`, `GreeterBlockingStub`, etc.),
/// not the outer `GreeterGrpc` class. All inner classes point to the same
/// service element, so we join through contract_elements by service name.
///
/// **Disambiguation strategy:**
/// When multiple proto services share the same simple name (e.g., `api.v1.Greeter`
/// and `legacy.Greeter`), refuse to link rather than risk binding to the wrong
/// service. This follows the same "refuse-on-ambiguity" pattern as GR-1B.
pub fn find_grpc_client_hints(
    creations: &[StubCreationInput],
    services: &[GrpcServiceMappingInput],
) -> Vec<GrpcClientHint> {
    // Index services by name. Collect ALL services per name to detect ambiguity.
    let mut services_by_name: HashMap<String, Vec<&GrpcServiceMappingInput>> = HashMap::new();

    for service in services {
        services_by_name
            .entry(service.service_name.clone())
            .or_default()
            .push(service);
    }

    let mut hints = Vec::new();

    for creation in creations {
        // Parse the stub creation pattern
        let parsed = match parse_stub_creation(&creation.call_pattern) {
            Some(p) => p,
            None => continue,
        };

        // Extract service name from Grpc class (e.g., "GreeterGrpc" -> "Greeter")
        let service_name = match extract_service_name_from_grpc_class(&parsed.grpc_class) {
            Some(name) => name,
            None => continue,
        };

        // Find matching services by name
        let matching_services = match services_by_name.get(&service_name) {
            Some(s) => s,
            None => continue,
        };

        // Refuse on ambiguity: if multiple services share the same simple name,
        // we can't determine which service the client is actually calling.
        if matching_services.len() != 1 {
            // Ambiguous: multiple proto services with same simple name (different packages).
            // Skip this stub creation rather than risk linking to the wrong service.
            continue;
        }

        let service = matching_services[0];

        hints.push(GrpcClientHint {
            creator_stable_key: creation.creator_stable_key.clone(),
            creator_name: creation.creator_name.clone(),
            source_file: creation.source_file.clone(),
            line_start: creation.line_start,
            col_start: creation.col_start,
            grpc_class: parsed.grpc_class,
            stub_method: parsed.stub_method,
            stub_type: parsed.stub_type,
            proto_service_uid: service.service_element_uid.clone(),
            proto_service_name: service.service_name.clone(),
            mapping_uid: service.mapping_uid.clone(),
            mapping_confidence: service.confidence,
            proto_service_full_name: service.service_full_name.clone(),
            proto_service_kind: service.element_kind.clone(),
            proto_schema_file: service.schema_file_path.clone(),
        });
    }

    hints
}

/// Generate a deterministic surface UID for a gRPC client hint.
///
/// Identity includes: snapshot, creator, grpc_class, stub_method, and line_start.
/// This ensures each distinct stub creation call site produces a unique surface,
/// even when the same method creates multiple stubs of the same type for
/// different services.
pub fn generate_surface_uid(
    snapshot_uid: &str,
    creator_stable_key: &str,
    grpc_class: &str,
    stub_method: &str,
    line_start: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"grpc_client_hint:");
    hasher.update(snapshot_uid.as_bytes());
    hasher.update(b":");
    hasher.update(creator_stable_key.as_bytes());
    hasher.update(b":");
    hasher.update(grpc_class.as_bytes());
    hasher.update(b":");
    hasher.update(stub_method.as_bytes());
    hasher.update(b":");
    hasher.update(line_start.to_string().as_bytes());
    let hash = hasher.finalize();
    format!("grpc-client-{:x}", &hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64))
}

/// Generate a deterministic association UID for a boundary contract.
pub fn generate_association_uid(surface_uid: &str, contract_element_uid: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"grpc_client_contract:");
    hasher.update(surface_uid.as_bytes());
    hasher.update(b":");
    hasher.update(contract_element_uid.as_bytes());
    let hash = hasher.finalize();
    format!("grpc-client-bc-{:x}", &hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64))
}

/// Result of running gRPC client hint detection.
///
/// Surfaces detection statistics and any failures for explicit
/// degradation reporting. Attached to `IndexResult` for visibility.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcClientHintResult {
    /// Number of boundary surfaces (hints) emitted.
    pub hints_emitted: usize,
    /// Number of boundary contracts emitted.
    pub contracts_emitted: usize,
    /// Query error when reading stub creation calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation_query_error: Option<String>,
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

impl GrpcClientHintResult {
    pub fn has_error(&self) -> bool {
        self.creation_query_error.is_some()
            || self.mapping_query_error.is_some()
            || self.surface_storage_error.is_some()
            || self.contract_storage_error.is_some()
    }
}

/// Run gRPC client hint detection for a snapshot.
///
/// This is the top-level orchestration function for GR-2A. It:
/// 1. Queries stub creation calls (via read port)
/// 2. Queries CS-2A mappings for Grpc classes (via read port)
/// 3. Joins them to produce `GrpcClientHint`s
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
/// A `GrpcClientHintResult` summarizing counts and any errors encountered.
/// Errors are collected rather than fail-fast, allowing partial progress.
pub fn run_grpc_client_hint_detection<S>(
    storage: &mut S,
    snapshot_uid: &str,
    repo_uid: &str,
) -> GrpcClientHintResult
where
    S: GrpcClientHintReadPort + GrpcClientHintStorePort,
    <S as GrpcClientHintReadPort>::Error: ToString,
    <S as GrpcClientHintStorePort>::Error: ToString,
{
    let mut result = GrpcClientHintResult::default();

    // Step 1: Query stub creations
    let creations = match storage.query_grpc_stub_creations(snapshot_uid) {
        Ok(c) => c,
        Err(e) => {
            result.creation_query_error = Some(e.to_string());
            return result;
        }
    };

    // Step 2: Query proto services with CS-2A gRPC mappings
    let services = match storage.query_grpc_service_mappings(snapshot_uid) {
        Ok(s) => s,
        Err(e) => {
            result.mapping_query_error = Some(e.to_string());
            return result;
        }
    };

    // Step 3: Join to produce hints
    let hints = find_grpc_client_hints(&creations, &services);
    if hints.is_empty() {
        return result;
    }

    // Step 4: Convert to surface/contract inputs
    let mut surfaces = Vec::with_capacity(hints.len());
    let mut contracts = Vec::with_capacity(hints.len());

    for hint in &hints {
        let surface_uid = generate_surface_uid(
            snapshot_uid,
            &hint.creator_stable_key,
            &hint.grpc_class,
            &hint.stub_method,
            hint.line_start,
        );
        let association_uid = generate_association_uid(&surface_uid, &hint.proto_service_uid);

        // Build evidence JSON
        let evidence = serde_json::json!({
            "grpc_class": hint.grpc_class,
            "stub_method": hint.stub_method,
            "stub_type": hint.stub_type.as_str(),
            "proto_service_name": hint.proto_service_name,
            "mapping_uid": hint.mapping_uid,
            "mapping_confidence": hint.mapping_confidence,
        });

        surfaces.push(GrpcClientSurfaceInput {
            surface_uid: surface_uid.clone(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            symbol_stable_key: hint.creator_stable_key.clone(),
            source_file: hint.source_file.clone(),
            line_start: hint.line_start,
            line_end: hint.line_start, // Single-line for call site
            col_start: hint.col_start,
            col_end: hint.col_start,
            evidence_json: evidence.to_string(),
        });

        // Compute provenance from stable anchors (ACR-5)
        // Pattern: {repo}:{proto_file}#{element_kind}:{full_name}
        let contract_element_stable_key = format!(
            "{}:{}#{}:{}",
            repo_uid,
            hint.proto_schema_file,
            hint.proto_service_kind,
            hint.proto_service_full_name,
        );
        let provenance = Provenance::from_layer0_items(vec![
            ProvenanceAnchor::new("BoundaryInteractionSurfaces", &hint.creator_stable_key),
            ProvenanceAnchor::new("ContractElements", &contract_element_stable_key),
        ])
        .with_extractor("grpc_client_hint_java:1.0");

        contracts.push(GrpcClientContractInput {
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
    match storage.insert_grpc_client_surfaces(&surfaces) {
        Ok(count) => result.hints_emitted = count,
        Err(e) => {
            result.surface_storage_error = Some(e.to_string());
            return result;
        }
    }

    // Step 6: Store contracts
    match storage.insert_grpc_client_contracts(&contracts) {
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
    use crate::storage_port::{GrpcServiceMappingInput, StubCreationInput};

    #[test]
    fn parse_stub_creation_blocking() {
        let parsed = parse_stub_creation("GreeterGrpc.newBlockingStub(channel)").unwrap();
        assert_eq!(parsed.grpc_class, "GreeterGrpc");
        assert_eq!(parsed.stub_method, "newBlockingStub");
        assert_eq!(parsed.stub_type, StubType::Blocking);
    }

    #[test]
    fn parse_stub_creation_future() {
        let parsed = parse_stub_creation("UserServiceGrpc.newFutureStub(channel)").unwrap();
        assert_eq!(parsed.grpc_class, "UserServiceGrpc");
        assert_eq!(parsed.stub_method, "newFutureStub");
        assert_eq!(parsed.stub_type, StubType::Future);
    }

    #[test]
    fn parse_stub_creation_async() {
        let parsed = parse_stub_creation("GreeterGrpc.newStub(channel)").unwrap();
        assert_eq!(parsed.grpc_class, "GreeterGrpc");
        assert_eq!(parsed.stub_method, "newStub");
        assert_eq!(parsed.stub_type, StubType::Async);
    }

    #[test]
    fn parse_stub_creation_invalid() {
        assert!(parse_stub_creation("SomeClass.newBlockingStub(channel)").is_none());
        assert!(parse_stub_creation("GreeterGrpc.someOtherMethod()").is_none());
    }

    #[test]
    fn extract_service_name_from_grpc_class_works() {
        assert_eq!(
            extract_service_name_from_grpc_class("GreeterGrpc"),
            Some("Greeter".to_string())
        );
        assert_eq!(
            extract_service_name_from_grpc_class("UserServiceGrpc"),
            Some("UserService".to_string())
        );
    }

    #[test]
    fn extract_service_name_from_grpc_class_invalid() {
        assert!(extract_service_name_from_grpc_class("Grpc").is_none()); // Too short
        assert!(extract_service_name_from_grpc_class("SomeClass").is_none()); // No suffix
    }

    #[test]
    fn find_grpc_client_hints_joins_creations_with_services() {
        let creations = vec![StubCreationInput {
            creator_stable_key: "r1:HelloWorldClient:METHOD:init".to_string(),
            creator_name: "init".to_string(),
            source_file: "src/HelloWorldClient.java".to_string(),
            line_start: 20,
            col_start: 5,
            call_pattern: "GreeterGrpc.newBlockingStub(channel)".to_string(),
        }];

        // Service mapping - represents proto service with CS-2A gRPC mappings
        let services = vec![GrpcServiceMappingInput {
            service_element_uid: "service-1".to_string(),
            service_name: "Greeter".to_string(),
            mapping_uid: "m1".to_string(),
            confidence: 0.85,
            service_full_name: "example.Greeter".to_string(),
            element_kind: "service".to_string(),
            schema_file_path: "greeter.proto".to_string(),
        }];

        let hints = find_grpc_client_hints(&creations, &services);

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].grpc_class, "GreeterGrpc");
        assert_eq!(hints[0].stub_method, "newBlockingStub");
        assert_eq!(hints[0].stub_type, StubType::Blocking);
        assert_eq!(hints[0].proto_service_uid, "service-1");
        assert_eq!(hints[0].proto_service_name, "Greeter");
    }

    #[test]
    fn find_grpc_client_hints_skips_unmatched_creations() {
        let creations = vec![StubCreationInput {
            creator_stable_key: "r1:Client:METHOD:init".to_string(),
            creator_name: "init".to_string(),
            source_file: "src/Client.java".to_string(),
            line_start: 10,
            col_start: 1,
            call_pattern: "UnknownGrpc.newBlockingStub(channel)".to_string(),
        }];

        // Service with name "Greeter", but stub call is for "Unknown"
        let services = vec![GrpcServiceMappingInput {
            service_element_uid: "service-1".to_string(),
            service_name: "Greeter".to_string(),
            mapping_uid: "m1".to_string(),
            confidence: 0.85,
            service_full_name: "example.Greeter".to_string(),
            element_kind: "service".to_string(),
            schema_file_path: "greeter.proto".to_string(),
        }];

        let hints = find_grpc_client_hints(&creations, &services);

        assert!(hints.is_empty());
    }

    #[test]
    fn find_grpc_client_hints_refuses_on_ambiguous_services() {
        // Simulate two proto services with same simple name from different packages
        // (e.g., api.v1.Greeter and legacy.Greeter)
        let creations = vec![StubCreationInput {
            creator_stable_key: "r1:Client:METHOD:init".to_string(),
            creator_name: "init".to_string(),
            source_file: "src/Client.java".to_string(),
            line_start: 10,
            col_start: 1,
            call_pattern: "GreeterGrpc.newBlockingStub(channel)".to_string(),
        }];

        let services = vec![
            GrpcServiceMappingInput {
                service_element_uid: "service-v1".to_string(),
                service_name: "Greeter".to_string(), // api.v1.Greeter
                mapping_uid: "m1".to_string(),
                confidence: 0.85,
                service_full_name: "api.v1.Greeter".to_string(),
                element_kind: "service".to_string(),
                schema_file_path: "api/v1/greeter.proto".to_string(),
            },
            GrpcServiceMappingInput {
                service_element_uid: "service-legacy".to_string(),
                service_name: "Greeter".to_string(), // legacy.Greeter
                mapping_uid: "m2".to_string(),
                confidence: 0.85,
                service_full_name: "legacy.Greeter".to_string(),
                element_kind: "service".to_string(),
                schema_file_path: "legacy/greeter.proto".to_string(),
            },
        ];

        let hints = find_grpc_client_hints(&creations, &services);

        // Should refuse to link rather than risk binding to wrong service
        assert!(hints.is_empty(), "Ambiguous services should produce no hints");
    }

    #[test]
    fn surface_uid_is_deterministic() {
        let uid1 = generate_surface_uid("snap-1", "r1:Client:METHOD:init", "GreeterGrpc", "newBlockingStub", 10);
        let uid2 = generate_surface_uid("snap-1", "r1:Client:METHOD:init", "GreeterGrpc", "newBlockingStub", 10);
        assert_eq!(uid1, uid2);

        // Different stub method -> different UID
        let uid3 = generate_surface_uid("snap-1", "r1:Client:METHOD:init", "GreeterGrpc", "newFutureStub", 10);
        assert_ne!(uid1, uid3);

        // Different line -> different UID (same method creating multiple stubs)
        let uid4 = generate_surface_uid("snap-1", "r1:Client:METHOD:init", "GreeterGrpc", "newBlockingStub", 11);
        assert_ne!(uid1, uid4);

        // Different grpc_class -> different UID (same method calling different services)
        let uid5 = generate_surface_uid("snap-1", "r1:Client:METHOD:init", "UserGrpc", "newBlockingStub", 10);
        assert_ne!(uid1, uid5);
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
