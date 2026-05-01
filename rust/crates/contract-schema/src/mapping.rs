//! Generated code provenance mapping.
//!
//! Types for tracking the relationship between schema elements and
//! the code generated from them. This is the output of CS-2 (generated
//! code mapping) and enables cross-language linking in GR-3.
//!
//! ## Design Context
//!
//! Generated code mapping solves a core problem: when we detect a gRPC
//! client stub in Python calling `user_pb2_grpc.UserServiceStub()`, we
//! need to link it back to `package.UserService` in the .proto schema,
//! and from there to the Java server implementing the same service.
//!
//! The mapping is language-specific because each protobuf/gRPC plugin
//! generates code with different naming conventions:
//!
//! - C++: `package::ServiceName`, `package::ServiceName::Stub`
//! - Rust: `package::service_name_server::ServiceName`
//! - Python: `package_pb2_grpc.ServiceNameStub`
//! - Java: `package.ServiceNameGrpc.ServiceNameBlockingStub`
//! - TypeScript: `ServiceNameClient`

use serde::{Deserialize, Serialize};

/// Mapping between a schema element and generated code.
///
/// Created during CS-2 extraction by analyzing generated code patterns
/// and correlating with parsed schema elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedCodeMapping {
    /// Unique identifier for this mapping.
    pub mapping_uid: String,

    /// Snapshot this mapping belongs to.
    pub snapshot_uid: String,

    /// Path to the schema file that defines the element.
    pub schema_file: String,

    /// Full name of the schema element (e.g., "api.v1.UserService").
    pub schema_element: String,

    /// Kind of schema element being mapped.
    pub element_kind: MappedElementKind,

    /// Stable key of the generated code symbol.
    pub generated_symbol_key: String,

    /// Language of the generated code.
    pub language: String,

    /// Path to the generated file.
    pub generated_file: String,

    /// How the mapping was determined.
    pub mapping_basis: MappingBasis,

    /// Confidence score (0.0 to 1.0).
    pub confidence: f64,
}

impl GeneratedCodeMapping {
    /// Build a deterministic mapping UID.
    ///
    /// Format: `gcm:<schema_element>:<language>:<symbol_key_hash>`
    pub fn build_uid(schema_element: &str, language: &str, generated_symbol_key: &str) -> String {
        // Use a simple hash of the symbol key to keep UID length reasonable
        let key_hash = simple_hash(generated_symbol_key);
        format!("gcm:{}:{}:{:08x}", schema_element, language, key_hash)
    }

    /// Validate the mapping.
    pub fn validate(&self) -> Result<(), String> {
        if self.mapping_uid.is_empty() {
            return Err("mapping_uid is empty".to_string());
        }
        if self.schema_file.is_empty() {
            return Err("schema_file is empty".to_string());
        }
        if self.schema_element.is_empty() {
            return Err("schema_element is empty".to_string());
        }
        if self.generated_symbol_key.is_empty() {
            return Err("generated_symbol_key is empty".to_string());
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(format!(
                "confidence {} out of range [0.0, 1.0]",
                self.confidence
            ));
        }
        Ok(())
    }
}

/// Kind of schema element being mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappedElementKind {
    /// Message type.
    Message,

    /// Enum type.
    Enum,

    /// Service definition.
    Service,

    /// RPC method.
    Method,

    /// Server stub/implementation base class.
    ServerStub,

    /// Client stub.
    ClientStub,
}

impl MappedElementKind {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            MappedElementKind::Message => "message",
            MappedElementKind::Enum => "enum",
            MappedElementKind::Service => "service",
            MappedElementKind::Method => "method",
            MappedElementKind::ServerStub => "server_stub",
            MappedElementKind::ClientStub => "client_stub",
        }
    }
}

/// How the mapping was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingBasis {
    /// File path pattern matching (e.g., `*_pb2.py` in same directory as `.proto`).
    FilePattern,

    /// Symbol name transformation rules (e.g., `UserService` -> `user_service_pb2_grpc`).
    NameTransform,

    /// Import statement tracing.
    ImportTrace,

    /// Build system configuration (e.g., protoc plugin output).
    BuildConfig,

    /// Explicit annotation in code or schema.
    Annotation,
}

impl MappingBasis {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            MappingBasis::FilePattern => "file_pattern",
            MappingBasis::NameTransform => "name_transform",
            MappingBasis::ImportTrace => "import_trace",
            MappingBasis::BuildConfig => "build_config",
            MappingBasis::Annotation => "annotation",
        }
    }

    /// Default confidence for this mapping basis.
    pub const fn default_confidence(self) -> f64 {
        match self {
            MappingBasis::Annotation => 0.99,
            MappingBasis::BuildConfig => 0.95,
            MappingBasis::ImportTrace => 0.90,
            MappingBasis::NameTransform => 0.85,
            MappingBasis::FilePattern => 0.80,
        }
    }
}

/// Language-specific generated code pattern.
///
/// Lookup table for correlating schema elements to generated symbols.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedCodePattern {
    /// Target language.
    pub language: String,

    /// File suffix pattern (e.g., "_pb2.py", ".pb.h").
    pub file_suffix: String,

    /// gRPC file suffix (e.g., "_pb2_grpc.py", ".grpc.pb.h").
    pub grpc_file_suffix: Option<String>,

    /// Message class name transform (from schema message name).
    /// Placeholder: `{name}` = message name, `{package}` = package with separators.
    pub message_pattern: String,

    /// Service class name transform.
    pub service_pattern: String,

    /// Server stub class pattern.
    pub server_stub_pattern: Option<String>,

    /// Client stub class pattern.
    pub client_stub_pattern: Option<String>,
}

/// Well-known generated code patterns by language.
pub mod patterns {
    use super::GeneratedCodePattern;

    /// Python protobuf/gRPC patterns.
    pub fn python() -> GeneratedCodePattern {
        GeneratedCodePattern {
            language: "python".to_string(),
            file_suffix: "_pb2.py".to_string(),
            grpc_file_suffix: Some("_pb2_grpc.py".to_string()),
            message_pattern: "{name}".to_string(),
            service_pattern: "{name}Servicer".to_string(),
            server_stub_pattern: Some("{name}Servicer".to_string()),
            client_stub_pattern: Some("{name}Stub".to_string()),
        }
    }

    /// C++ protobuf/gRPC patterns.
    pub fn cpp() -> GeneratedCodePattern {
        GeneratedCodePattern {
            language: "cpp".to_string(),
            file_suffix: ".pb.h".to_string(),
            grpc_file_suffix: Some(".grpc.pb.h".to_string()),
            message_pattern: "{package}::{name}".to_string(),
            service_pattern: "{package}::{name}".to_string(),
            server_stub_pattern: Some("{package}::{name}::Service".to_string()),
            client_stub_pattern: Some("{package}::{name}::Stub".to_string()),
        }
    }

    /// Rust tonic patterns.
    pub fn rust() -> GeneratedCodePattern {
        GeneratedCodePattern {
            language: "rust".to_string(),
            file_suffix: ".rs".to_string(),
            grpc_file_suffix: None, // Same file
            message_pattern: "{name}".to_string(),
            service_pattern: "{name}".to_string(),
            server_stub_pattern: Some("{name}Server".to_string()),
            client_stub_pattern: Some("{name}Client".to_string()),
        }
    }

    /// Java protobuf/gRPC patterns.
    pub fn java() -> GeneratedCodePattern {
        GeneratedCodePattern {
            language: "java".to_string(),
            file_suffix: ".java".to_string(),
            grpc_file_suffix: Some("Grpc.java".to_string()),
            message_pattern: "{name}".to_string(),
            service_pattern: "{name}Grpc".to_string(),
            server_stub_pattern: Some("{name}Grpc.{name}ImplBase".to_string()),
            client_stub_pattern: Some("{name}Grpc.{name}BlockingStub".to_string()),
        }
    }

    /// TypeScript grpc-js patterns.
    pub fn typescript() -> GeneratedCodePattern {
        GeneratedCodePattern {
            language: "typescript".to_string(),
            file_suffix: "_pb.ts".to_string(),
            grpc_file_suffix: Some("_grpc_pb.ts".to_string()),
            message_pattern: "{name}".to_string(),
            service_pattern: "I{name}Server".to_string(),
            server_stub_pattern: Some("I{name}Server".to_string()),
            client_stub_pattern: Some("{name}Client".to_string()),
        }
    }
}

/// Simple non-cryptographic hash for UID generation.
fn simple_hash(s: &str) -> u32 {
    let mut h: u32 = 0;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u32);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_uid_is_deterministic() {
        let uid1 = GeneratedCodeMapping::build_uid(
            "api.v1.UserService",
            "python",
            "api:v1:user_pb2_grpc.UserServiceStub",
        );
        let uid2 = GeneratedCodeMapping::build_uid(
            "api.v1.UserService",
            "python",
            "api:v1:user_pb2_grpc.UserServiceStub",
        );
        assert_eq!(uid1, uid2);
        assert!(uid1.starts_with("gcm:"));
    }

    #[test]
    fn mapping_uid_varies_by_language() {
        let py_uid = GeneratedCodeMapping::build_uid(
            "api.v1.UserService",
            "python",
            "api:v1:user_pb2_grpc.UserServiceStub",
        );
        let java_uid = GeneratedCodeMapping::build_uid(
            "api.v1.UserService",
            "java",
            "api.v1.UserServiceGrpc.UserServiceBlockingStub",
        );
        assert_ne!(py_uid, java_uid);
    }

    #[test]
    fn mapping_basis_confidence_ordering() {
        assert!(
            MappingBasis::Annotation.default_confidence()
                > MappingBasis::FilePattern.default_confidence()
        );
    }

    #[test]
    fn python_pattern_has_grpc_suffix() {
        let py = patterns::python();
        assert_eq!(py.grpc_file_suffix, Some("_pb2_grpc.py".to_string()));
    }

    #[test]
    fn mapped_element_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&MappedElementKind::ServerStub).unwrap(),
            "\"server_stub\""
        );
    }
}
