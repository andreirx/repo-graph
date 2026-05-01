//! Java generated-code provenance mapper (CS-2A).
//!
//! Maps checked-in Java generated protobuf/gRPC artifacts to top-level
//! contract elements (messages, enums, services).
//!
//! # Architecture
//!
//! This module runs AFTER source extraction and contract indexing complete.
//! It requires:
//! - Contract elements from CS-1 (messages, enums, services with options)
//! - Java source symbols from the ts/java extractor
//!
//! # Confidence Tiers
//!
//! | Basis | Confidence | Description |
//! |-------|------------|-------------|
//! | `exact_option_match` | 0.95 | java_package + outer_classname match |
//! | `option_package_match` | 0.90 | java_package matches, classname follows convention |
//! | `filename_convention` | 0.85 | Generated file pattern + symbol name match |
//! | `symbol_normalized_match` | 0.75 | Symbol name normalizes to schema element |
//! | `weak_wrapper_match` | 0.50 | Partial match via outer class wrapper |
//!
//! # Usage
//!
//! ```ignore
//! let mappings = map_java_generated_code(
//!     &contract_elements,
//!     &java_symbols,
//!     &schema_options,
//! );
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Confidence floor for persisting mappings.
pub const CONFIDENCE_FLOOR: f64 = 0.50;

/// Preferred minimum confidence for high-quality mappings.
pub const CONFIDENCE_PREFERRED: f64 = 0.75;

/// Mapping basis tiers (explicit, not hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingBasis {
    /// java_package + java_outer_classname match exactly
    ExactOptionMatch,
    /// java_package matches, classname follows convention
    OptionPackageMatch,
    /// Generated file pattern + symbol name match
    FilenameConvention,
    /// Symbol name normalizes to schema element
    SymbolNormalizedMatch,
    /// Partial match via outer class wrapper
    WeakWrapperMatch,
}

impl MappingBasis {
    /// Get the confidence score for this basis.
    pub fn confidence(&self) -> f64 {
        match self {
            Self::ExactOptionMatch => 0.95,
            Self::OptionPackageMatch => 0.90,
            Self::FilenameConvention => 0.85,
            Self::SymbolNormalizedMatch => 0.75,
            Self::WeakWrapperMatch => 0.50,
        }
    }

    /// Get the string representation for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExactOptionMatch => "exact_option_match",
            Self::OptionPackageMatch => "option_package_match",
            Self::FilenameConvention => "filename_convention",
            Self::SymbolNormalizedMatch => "symbol_normalized_match",
            Self::WeakWrapperMatch => "weak_wrapper_match",
        }
    }
}

/// A generated code mapping candidate.
#[derive(Debug, Clone)]
pub struct GeneratedCodeMapping {
    /// Schema element UID (from contract_elements)
    pub schema_element_uid: String,
    /// Stable key of the generated Java symbol
    pub generated_symbol_key: String,
    /// Path to the generated file
    pub generated_file: String,
    /// Mapping basis (determines confidence)
    pub basis: MappingBasis,
    /// Evidence for the mapping
    pub evidence: MappingEvidence,
}

/// Evidence supporting a mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingEvidence {
    /// Proto package (e.g., "hadoop.common")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proto_package: Option<String>,
    /// java_package option value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_package_option: Option<String>,
    /// java_outer_classname option value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_outer_classname_option: Option<String>,
    /// Java package extracted from symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_package_actual: Option<String>,
    /// Java outer class name extracted from symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_outer_class_actual: Option<String>,
    /// Schema element name
    pub schema_element_name: String,
    /// Java inner class name
    pub java_class_name: String,
}

/// Options extracted from a proto schema.
#[derive(Debug, Clone, Default)]
pub struct ProtoOptions {
    pub java_package: Option<String>,
    pub java_outer_classname: Option<String>,
    pub java_multiple_files: bool,
}

impl ProtoOptions {
    /// Parse options from JSON string.
    pub fn from_json(json: &str) -> Self {
        let map: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(json).unwrap_or_default();

        Self {
            java_package: map.get("java_package").and_then(|v| v.as_str()).map(String::from),
            java_outer_classname: map
                .get("java_outer_classname")
                .and_then(|v| v.as_str())
                .map(String::from),
            java_multiple_files: map
                .get("java_multiple_files")
                .and_then(|v| v.as_str())
                .map(|s| s == "true")
                .unwrap_or(false),
        }
    }
}

/// A contract element with its schema context.
#[derive(Debug, Clone)]
pub struct ContractElementContext {
    pub element_uid: String,
    pub element_kind: String,
    pub name: String,
    pub full_name: String,
    pub schema_file: String,
    pub proto_package: Option<String>,
    pub options: ProtoOptions,
}

/// A Java symbol candidate for mapping.
#[derive(Debug, Clone)]
pub struct JavaSymbol {
    pub stable_key: String,
    pub name: String,
    pub qualified_name: String,
    pub subtype: String,
    pub file_path: String,
}

impl JavaSymbol {
    /// Extract the Java package from the qualified name.
    pub fn java_package(&self) -> Option<String> {
        // qualified_name is like "com.example.OuterClass.InnerClass"
        // Java package is everything before the first uppercase segment
        let parts: Vec<&str> = self.qualified_name.split('.').collect();
        let package_parts: Vec<&str> = parts
            .iter()
            .take_while(|p| p.chars().next().map(|c| c.is_lowercase()).unwrap_or(false))
            .copied()
            .collect();

        if package_parts.is_empty() {
            None
        } else {
            Some(package_parts.join("."))
        }
    }

    /// Extract the outer class name from the qualified name.
    pub fn outer_class(&self) -> Option<String> {
        let parts: Vec<&str> = self.qualified_name.split('.').collect();
        // Find first uppercase segment (outer class)
        parts
            .iter()
            .find(|p| p.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
            .map(|s| s.to_string())
    }

    /// Extract the inner class name (the symbol itself, without outer class).
    pub fn inner_class(&self) -> String {
        self.name.clone()
    }
}

/// Check if a file path looks like a Java protobuf generated file.
pub fn is_java_generated_proto_file(path: &str) -> bool {
    if !path.ends_with(".java") {
        return false;
    }

    // Common patterns for protobuf generated Java files
    let filename = path.rsplit('/').next().unwrap_or(path);

    // Pattern: *Protos.java, *Proto.java
    if filename.ends_with("Protos.java") || filename.ends_with("Proto.java") {
        return true;
    }

    // Pattern: *OuterClass.java (custom outer class naming)
    if filename.ends_with("OuterClass.java") {
        return true;
    }

    // Pattern: file in proto-generated or generated directory
    // Check with and without leading slash for flexibility
    if path.contains("proto-generated/")
        || path.contains("proto2-generated/")
        || path.contains("generated-sources/")
        || path.contains("/proto-generated")
        || path.contains("/proto2-generated")
        || path.contains("/generated-sources")
    {
        return true;
    }

    false
}

/// Check if a file path looks like a Java gRPC generated file.
pub fn is_java_generated_grpc_file(path: &str) -> bool {
    if !path.ends_with(".java") {
        return false;
    }

    let filename = path.rsplit('/').next().unwrap_or(path);

    // Pattern: *Grpc.java
    filename.ends_with("Grpc.java")
}

/// gRPC stub suffixes in order of specificity.
const GRPC_STUB_SUFFIXES: &[&str] = &[
    "ImplBase",
    "BlockingStub",
    "FutureStub",
    "Stub",
];

/// Extract service name from a gRPC stub class name.
///
/// Returns `Some(service_name)` if the class name matches a gRPC stub pattern.
/// Example: "UserServiceImplBase" -> Some("UserService")
pub fn extract_grpc_service_name(class_name: &str) -> Option<String> {
    for suffix in GRPC_STUB_SUFFIXES {
        if class_name.ends_with(suffix) && class_name.len() > suffix.len() {
            return Some(class_name[..class_name.len() - suffix.len()].to_string());
        }
    }
    None
}

/// Check if an outer class name follows the gRPC naming convention.
///
/// Example: "UserServiceGrpc" for service "UserService"
pub fn is_grpc_outer_class(outer_class: &str, service_name: &str) -> bool {
    outer_class == format!("{}Grpc", service_name)
}

/// Convert a proto filename to expected outer class name (without java_outer_classname option).
///
/// Example: "user_service.proto" -> "UserService"
pub fn proto_filename_to_outer_class(proto_filename: &str) -> String {
    let base = proto_filename
        .trim_end_matches(".proto")
        .rsplit('/')
        .next()
        .unwrap_or(proto_filename);

    to_camel_case(base)
}

/// Convert snake_case to CamelCase.
pub fn to_camel_case(s: &str) -> String {
    s.split(|c| c == '_' || c == '-')
        .map(|part| {
            if part.is_empty() {
                return String::new();
            }
            // Lowercase the whole part, then capitalize first char
            let lower = part.to_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Find mappings for Java generated code.
///
/// Returns a list of mappings above the confidence floor.
pub fn find_java_mappings(
    elements: &[ContractElementContext],
    symbols: &[JavaSymbol],
) -> Vec<GeneratedCodeMapping> {
    let mut mappings = Vec::new();

    // Index elements by name for efficient lookup
    let elements_by_name: BTreeMap<&str, Vec<&ContractElementContext>> = {
        let mut map: BTreeMap<&str, Vec<&ContractElementContext>> = BTreeMap::new();
        for elem in elements {
            map.entry(elem.name.as_str()).or_default().push(elem);
        }
        map
    };

    // Index elements by outer class (derived from java_outer_classname or filename)
    let elements_by_outer_class: BTreeMap<String, Vec<&ContractElementContext>> = {
        let mut map: BTreeMap<String, Vec<&ContractElementContext>> = BTreeMap::new();
        for elem in elements {
            let outer_class = elem
                .options
                .java_outer_classname
                .clone()
                .unwrap_or_else(|| proto_filename_to_outer_class(&elem.schema_file));
            map.entry(outer_class).or_default().push(elem);
        }
        map
    };

    // Index service elements by name for gRPC matching
    let services_by_name: BTreeMap<&str, Vec<&ContractElementContext>> = {
        let mut map: BTreeMap<&str, Vec<&ContractElementContext>> = BTreeMap::new();
        for elem in elements {
            if elem.element_kind == "service" {
                map.entry(elem.name.as_str()).or_default().push(elem);
            }
        }
        map
    };

    // For each Java symbol in a generated file, try to find a matching schema element
    for symbol in symbols {
        let is_proto_file = is_java_generated_proto_file(&symbol.file_path);
        let is_grpc_file = is_java_generated_grpc_file(&symbol.file_path);

        // Skip non-generated files
        if !is_proto_file && !is_grpc_file {
            continue;
        }

        // Skip non-class/interface symbols
        if symbol.subtype != "CLASS" && symbol.subtype != "INTERFACE" {
            continue;
        }

        let java_package = symbol.java_package();
        let outer_class = symbol.outer_class();
        let inner_class = symbol.inner_class();

        // Skip the outer class itself (e.g., MyProtos, UserServiceGrpc) - we want inner classes
        if outer_class.as_ref() == Some(&inner_class) {
            continue;
        }

        // For gRPC files, try gRPC stub pattern matching first
        if is_grpc_file {
            if let Some(service_name) = extract_grpc_service_name(&inner_class) {
                // Verify outer class follows gRPC convention
                let outer_matches = outer_class
                    .as_ref()
                    .map(|oc| is_grpc_outer_class(oc, &service_name))
                    .unwrap_or(false);

                if outer_matches {
                    // Look up service element
                    if let Some(candidates) = services_by_name.get(service_name.as_str()) {
                        for elem in candidates {
                            // Check java_package match if available
                            if let Some(jp) = &elem.options.java_package {
                                if java_package.as_deref() == Some(jp.as_str()) {
                                    mappings.push(create_grpc_mapping(elem, symbol, &service_name));
                                    break;
                                }
                            } else {
                                // No java_package option, use filename convention
                                mappings.push(create_grpc_mapping(elem, symbol, &service_name));
                                break;
                            }
                        }
                    }
                }
            }
            // Continue to next symbol; gRPC files don't have proto message classes
            continue;
        }

        // For proto files, try standard message/enum matching
        if let Some(mapping) = find_best_match(
            &inner_class,
            java_package.as_deref(),
            outer_class.as_deref(),
            symbol,
            &elements_by_name,
            &elements_by_outer_class,
        ) {
            mappings.push(mapping);
        }
    }

    mappings
}

/// Create a gRPC service mapping.
fn create_grpc_mapping(
    elem: &ContractElementContext,
    symbol: &JavaSymbol,
    service_name: &str,
) -> GeneratedCodeMapping {
    GeneratedCodeMapping {
        schema_element_uid: elem.element_uid.clone(),
        generated_symbol_key: symbol.stable_key.clone(),
        generated_file: symbol.file_path.clone(),
        basis: MappingBasis::FilenameConvention, // gRPC uses naming convention
        evidence: MappingEvidence {
            proto_package: elem.proto_package.clone(),
            java_package_option: elem.options.java_package.clone(),
            java_outer_classname_option: None, // gRPC doesn't use java_outer_classname
            java_package_actual: symbol.java_package(),
            java_outer_class_actual: symbol.outer_class(),
            schema_element_name: service_name.to_string(),
            java_class_name: symbol.name.clone(),
        },
    }
}

/// Find the best matching schema element for a Java symbol.
fn find_best_match(
    inner_class: &str,
    java_package: Option<&str>,
    outer_class: Option<&str>,
    symbol: &JavaSymbol,
    elements_by_name: &BTreeMap<&str, Vec<&ContractElementContext>>,
    elements_by_outer_class: &BTreeMap<String, Vec<&ContractElementContext>>,
) -> Option<GeneratedCodeMapping> {
    // First, try exact name match
    if let Some(candidates) = elements_by_name.get(inner_class) {
        for elem in candidates {
            // Check java_package option match
            if let Some(jp) = &elem.options.java_package {
                if java_package == Some(jp.as_str()) {
                    // Exact option match
                    if let Some(oc) = &elem.options.java_outer_classname {
                        if outer_class == Some(oc.as_str()) {
                            return Some(create_mapping(
                                elem,
                                symbol,
                                MappingBasis::ExactOptionMatch,
                            ));
                        }
                    }
                    // Package matches, check filename convention for outer class
                    let expected_outer = proto_filename_to_outer_class(&elem.schema_file);
                    if outer_class == Some(expected_outer.as_str()) {
                        return Some(create_mapping(
                            elem,
                            symbol,
                            MappingBasis::OptionPackageMatch,
                        ));
                    }
                }
            }
        }
    }

    // Second, try outer class match (look up elements that would generate to this outer class)
    if let Some(oc) = outer_class {
        if let Some(candidates) = elements_by_outer_class.get(oc) {
            for elem in candidates {
                if elem.name == inner_class {
                    // Filename convention match
                    return Some(create_mapping(
                        elem,
                        symbol,
                        MappingBasis::FilenameConvention,
                    ));
                }
            }
        }
    }

    // Third, try normalized name match
    if let Some(candidates) = elements_by_name.get(inner_class) {
        // Return first match as normalized
        if let Some(elem) = candidates.first() {
            return Some(create_mapping(
                elem,
                symbol,
                MappingBasis::SymbolNormalizedMatch,
            ));
        }
    }

    None
}

/// Create a mapping from element and symbol.
fn create_mapping(
    elem: &ContractElementContext,
    symbol: &JavaSymbol,
    basis: MappingBasis,
) -> GeneratedCodeMapping {
    GeneratedCodeMapping {
        schema_element_uid: elem.element_uid.clone(),
        generated_symbol_key: symbol.stable_key.clone(),
        generated_file: symbol.file_path.clone(),
        basis,
        evidence: MappingEvidence {
            proto_package: elem.proto_package.clone(),
            java_package_option: elem.options.java_package.clone(),
            java_outer_classname_option: elem.options.java_outer_classname.clone(),
            java_package_actual: symbol.java_package(),
            java_outer_class_actual: symbol.outer_class(),
            schema_element_name: elem.name.clone(),
            java_class_name: symbol.name.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("user_service"), "UserService");
        assert_eq!(to_camel_case("proto_buf_engine"), "ProtoBufEngine");
        assert_eq!(to_camel_case("simple"), "Simple");
        assert_eq!(to_camel_case("ALREADY_CAPS"), "AlreadyCaps");
    }

    #[test]
    fn test_proto_filename_to_outer_class() {
        assert_eq!(
            proto_filename_to_outer_class("user_service.proto"),
            "UserService"
        );
        assert_eq!(
            proto_filename_to_outer_class("path/to/my_messages.proto"),
            "MyMessages"
        );
    }

    #[test]
    fn test_is_java_generated_proto_file() {
        assert!(is_java_generated_proto_file("com/example/MyProtos.java"));
        assert!(is_java_generated_proto_file("proto-generated/Test.java"));
        assert!(is_java_generated_proto_file("src/proto2-generated/Foo.java"));
        assert!(!is_java_generated_proto_file("com/example/MyService.java"));
        assert!(!is_java_generated_proto_file("MyProtos.txt"));
    }

    #[test]
    fn test_is_java_generated_grpc_file() {
        assert!(is_java_generated_grpc_file("com/example/MyServiceGrpc.java"));
        assert!(!is_java_generated_grpc_file("com/example/MyService.java"));
    }

    #[test]
    fn test_java_symbol_parsing() {
        let symbol = JavaSymbol {
            stable_key: "test:key".to_string(),
            name: "RequestHeaderProto".to_string(),
            qualified_name: "org.apache.hadoop.ipc.protobuf.ProtobufRpcEngineProtos.RequestHeaderProto".to_string(),
            subtype: "CLASS".to_string(),
            file_path: "proto2-generated/org/apache/hadoop/ipc/protobuf/ProtobufRpcEngineProtos.java".to_string(),
        };

        assert_eq!(
            symbol.java_package(),
            Some("org.apache.hadoop.ipc.protobuf".to_string())
        );
        assert_eq!(
            symbol.outer_class(),
            Some("ProtobufRpcEngineProtos".to_string())
        );
        assert_eq!(symbol.inner_class(), "RequestHeaderProto");
    }

    #[test]
    fn test_mapping_basis_confidence() {
        assert_eq!(MappingBasis::ExactOptionMatch.confidence(), 0.95);
        assert_eq!(MappingBasis::OptionPackageMatch.confidence(), 0.90);
        assert_eq!(MappingBasis::FilenameConvention.confidence(), 0.85);
        assert_eq!(MappingBasis::SymbolNormalizedMatch.confidence(), 0.75);
        assert_eq!(MappingBasis::WeakWrapperMatch.confidence(), 0.50);
    }

    #[test]
    fn test_proto_options_parsing() {
        let json = r#"{"java_package":"org.example","java_outer_classname":"MyProtos","java_multiple_files":"true"}"#;
        let opts = ProtoOptions::from_json(json);

        assert_eq!(opts.java_package, Some("org.example".to_string()));
        assert_eq!(opts.java_outer_classname, Some("MyProtos".to_string()));
        assert!(opts.java_multiple_files);
    }

    #[test]
    fn test_find_java_mappings_exact_option_match() {
        let elements = vec![ContractElementContext {
            element_uid: "elem-1".to_string(),
            element_kind: "message".to_string(),
            name: "RequestHeaderProto".to_string(),
            full_name: "hadoop.common.RequestHeaderProto".to_string(),
            schema_file: "ProtobufRpcEngine.proto".to_string(),
            proto_package: Some("hadoop.common".to_string()),
            options: ProtoOptions {
                java_package: Some("org.apache.hadoop.ipc.protobuf".to_string()),
                java_outer_classname: Some("ProtobufRpcEngineProtos".to_string()),
                java_multiple_files: false,
            },
        }];

        let symbols = vec![JavaSymbol {
            stable_key: "test:RequestHeaderProto".to_string(),
            name: "RequestHeaderProto".to_string(),
            qualified_name: "org.apache.hadoop.ipc.protobuf.ProtobufRpcEngineProtos.RequestHeaderProto".to_string(),
            subtype: "CLASS".to_string(),
            file_path: "proto2-generated/org/apache/hadoop/ipc/protobuf/ProtobufRpcEngineProtos.java".to_string(),
        }];

        let mappings = find_java_mappings(&elements, &symbols);

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].schema_element_uid, "elem-1");
        assert_eq!(mappings[0].basis, MappingBasis::ExactOptionMatch);
        assert_eq!(mappings[0].basis.confidence(), 0.95);
    }

    #[test]
    fn test_extract_grpc_service_name() {
        assert_eq!(
            extract_grpc_service_name("UserServiceImplBase"),
            Some("UserService".to_string())
        );
        assert_eq!(
            extract_grpc_service_name("UserServiceBlockingStub"),
            Some("UserService".to_string())
        );
        assert_eq!(
            extract_grpc_service_name("UserServiceFutureStub"),
            Some("UserService".to_string())
        );
        assert_eq!(
            extract_grpc_service_name("UserServiceStub"),
            Some("UserService".to_string())
        );
        assert_eq!(extract_grpc_service_name("UserService"), None);
        assert_eq!(extract_grpc_service_name("ImplBase"), None);
    }

    #[test]
    fn test_is_grpc_outer_class() {
        assert!(is_grpc_outer_class("UserServiceGrpc", "UserService"));
        assert!(!is_grpc_outer_class("UserServiceGrpc", "OtherService"));
        assert!(!is_grpc_outer_class("UserService", "UserService"));
    }

    #[test]
    fn test_find_java_mappings_grpc_stub() {
        let elements = vec![ContractElementContext {
            element_uid: "service-1".to_string(),
            element_kind: "service".to_string(),
            name: "UserService".to_string(),
            full_name: "api.v1.UserService".to_string(),
            schema_file: "user_service.proto".to_string(),
            proto_package: Some("api.v1".to_string()),
            options: ProtoOptions {
                java_package: Some("com.example.api".to_string()),
                java_outer_classname: None,
                java_multiple_files: false,
            },
        }];

        let symbols = vec![
            JavaSymbol {
                stable_key: "test:UserServiceImplBase".to_string(),
                name: "UserServiceImplBase".to_string(),
                qualified_name: "com.example.api.UserServiceGrpc.UserServiceImplBase".to_string(),
                subtype: "CLASS".to_string(),
                file_path: "generated/com/example/api/UserServiceGrpc.java".to_string(),
            },
            JavaSymbol {
                stable_key: "test:UserServiceBlockingStub".to_string(),
                name: "UserServiceBlockingStub".to_string(),
                qualified_name: "com.example.api.UserServiceGrpc.UserServiceBlockingStub".to_string(),
                subtype: "CLASS".to_string(),
                file_path: "generated/com/example/api/UserServiceGrpc.java".to_string(),
            },
        ];

        let mappings = find_java_mappings(&elements, &symbols);

        // Should match both stub classes to the service
        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings[0].schema_element_uid, "service-1");
        assert_eq!(mappings[0].basis, MappingBasis::FilenameConvention);
        assert_eq!(mappings[1].schema_element_uid, "service-1");
    }

    #[test]
    fn test_find_java_mappings_grpc_outer_class_skipped() {
        // The outer class UserServiceGrpc should be skipped, not mapped
        let elements = vec![ContractElementContext {
            element_uid: "service-1".to_string(),
            element_kind: "service".to_string(),
            name: "UserService".to_string(),
            full_name: "api.v1.UserService".to_string(),
            schema_file: "user_service.proto".to_string(),
            proto_package: Some("api.v1".to_string()),
            options: ProtoOptions::default(),
        }];

        let symbols = vec![JavaSymbol {
            stable_key: "test:UserServiceGrpc".to_string(),
            name: "UserServiceGrpc".to_string(),
            qualified_name: "com.example.api.UserServiceGrpc".to_string(),
            subtype: "CLASS".to_string(),
            file_path: "generated/com/example/api/UserServiceGrpc.java".to_string(),
        }];

        let mappings = find_java_mappings(&elements, &symbols);

        // Outer class should be skipped
        assert_eq!(mappings.len(), 0);
    }
}
