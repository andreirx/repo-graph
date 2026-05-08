//! Proto schema indexing — CS-1 protobuf schema extraction.
//!
//! This module handles parsing and persisting `.proto` files. It is
//! separate from the main language extraction pipeline because:
//!
//! 1. Proto files produce schema facts (messages, services, fields),
//!    not nodes/edges like language extractors
//! 2. Schema facts are stored in different tables (contract_schemas,
//!    contract_elements)
//! 3. No edge resolution is needed — proto facts are self-contained
//!
//! ## Usage
//!
//! ```ignore
//! use repo_graph_indexer::proto_indexer::{index_proto_files, ProtoFileInput};
//!
//! let files = vec![ProtoFileInput {
//!     rel_path: "api/v1/user.proto".to_string(),
//!     content: source_text,
//!     content_hash: "sha256:...".to_string(),
//! }];
//!
//! let result = index_proto_files(&mut storage, "repo-1", "snap-1", &files)?;
//! ```
//!
//! ## Design Context
//!
//! Normative contract: `docs/slices/cs-1-protobuf-schema.md`
//!
//! The proto indexer is invoked separately from `index_repo`. The CLI
//! can call both in sequence, or proto indexing can be run standalone.

use repo_graph_proto_parser::{parse_proto, ProtoEnum, ProtoMessage, ProtoService};
use uuid::Uuid;

use crate::storage_port::{ProtoElementInput, ProtoSchemaInput, ProtoSchemaStorePort};

// ── Constants ────────────────────────────────────────────────────

/// Version string stamped on all proto schema records.
const PROTO_PARSER_VERSION: &str = "proto-parser:0.1.0";

/// Schema kind for protobuf files.
const SCHEMA_KIND_PROTOBUF: &str = "protobuf";

// ── Input types ──────────────────────────────────────────────────

/// A proto file provided for indexing.
#[derive(Debug, Clone)]
pub struct ProtoFileInput {
    /// Repo-relative path to the .proto file.
    pub rel_path: String,
    /// UTF-8 source text of the file.
    pub content: String,
    /// Pre-computed content hash (e.g., SHA-256 hex).
    pub content_hash: String,
}

// ── Result types ─────────────────────────────────────────────────

/// Result of proto schema indexing.
#[derive(Debug, Clone, Default)]
pub struct ProtoIndexResult {
    /// Number of schema files successfully parsed and stored.
    pub schemas_indexed: usize,
    /// Number of elements (messages, enums, services, methods, fields) stored.
    pub elements_indexed: usize,
    /// Files that failed to parse, with error messages.
    pub parse_failures: Vec<ProtoParseFailure>,
}

/// A proto file that failed to parse.
#[derive(Debug, Clone)]
pub struct ProtoParseFailure {
    /// The file path.
    pub file_path: String,
    /// The error message.
    pub error: String,
}

// ── Main indexing function ───────────────────────────────────────

/// Index proto files into the storage.
///
/// Parses each `.proto` file and persists the schema facts (messages,
/// enums, services, methods, fields) to the contract tables.
///
/// Non-fatal: parse failures are collected and returned in the result,
/// not propagated as errors. Only storage failures abort the operation.
pub fn index_proto_files<S: ProtoSchemaStorePort>(
    storage: &mut S,
    repo_uid: &str,
    snapshot_uid: &str,
    files: &[ProtoFileInput],
) -> Result<ProtoIndexResult, S::Error> {
    let mut result = ProtoIndexResult::default();

    for file in files {
        // Parse the proto file
        let parsed = match parse_proto(&file.rel_path, &file.content) {
            Ok(p) => p,
            Err(e) => {
                result.parse_failures.push(ProtoParseFailure {
                    file_path: file.rel_path.clone(),
                    error: e.message,
                });
                continue;
            }
        };

        // Build schema UID as a fresh UUID per snapshot.
        // This allows the same proto file to exist in multiple snapshots,
        // which is required for refresh operations. The deterministic
        // identity of a schema across snapshots is (repo_uid, file_path, content_hash),
        // but each snapshot needs its own row with a unique schema_uid.
        let schema_uid = Uuid::new_v4().to_string();

        // Serialize imports and options to JSON
        let imports_json = if !parsed.imports.is_empty() {
            Some(serde_json::to_string(&parsed.imports).unwrap_or_default())
        } else {
            None
        };

        let options_json = if !parsed.options.is_empty() {
            let opts: std::collections::BTreeMap<&str, &str> = parsed
                .options
                .iter()
                .map(|o| (o.name.as_str(), o.value.as_str()))
                .collect();
            Some(serde_json::to_string(&opts).unwrap_or_default())
        } else {
            None
        };

        // Syntax version is already a string in the parsed ProtoFile
        let syntax_version = if parsed.syntax.is_empty() {
            None
        } else {
            Some(parsed.syntax.clone())
        };

        // Build schema input (convert empty package string to None)
        let package_name = if parsed.package.is_empty() {
            None
        } else {
            Some(parsed.package.clone())
        };

        let schema_input = ProtoSchemaInput {
            schema_uid: schema_uid.clone(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            schema_kind: SCHEMA_KIND_PROTOBUF.to_string(),
            file_path: file.rel_path.clone(),
            package_name,
            syntax_version,
            content_hash: file.content_hash.clone(),
            imports_json,
            options_json,
            extractor: PROTO_PARSER_VERSION.to_string(),
        };

        // Insert schema
        storage.insert_proto_schema(&schema_input)?;
        result.schemas_indexed += 1;

        // Extract all elements
        let mut elements = Vec::new();
        let package_prefix = parsed.package.as_str();

        // Process messages (recursively for nested)
        for msg in &parsed.messages {
            extract_message_elements(
                &mut elements,
                msg,
                &schema_uid,
                package_prefix,
                None,
            );
        }

        // Process enums
        for enum_def in &parsed.enums {
            extract_enum_elements(
                &mut elements,
                enum_def,
                &schema_uid,
                package_prefix,
                None,
            );
        }

        // Process services
        for service in &parsed.services {
            extract_service_elements(
                &mut elements,
                service,
                &schema_uid,
                package_prefix,
            );
        }

        // Insert elements
        if !elements.is_empty() {
            let count = storage.insert_proto_elements(&elements)?;
            result.elements_indexed += count;
        }
    }

    Ok(result)
}

// ── Element extraction helpers ───────────────────────────────────

fn extract_message_elements(
    elements: &mut Vec<ProtoElementInput>,
    msg: &ProtoMessage,
    schema_uid: &str,
    package_prefix: &str,
    parent_uid: Option<&str>,
) {
    let element_uid = Uuid::new_v4().to_string();

    // Build full name
    let full_name = if package_prefix.is_empty() {
        msg.name.clone()
    } else {
        format!("{}.{}", package_prefix, msg.name)
    };

    // Metadata: field count, oneof names
    let metadata = serde_json::json!({
        "fields_count": msg.fields.len(),
        "oneofs": msg.oneofs.iter().map(|o| &o.name).collect::<Vec<_>>(),
    });

    elements.push(ProtoElementInput {
        element_uid: element_uid.clone(),
        schema_uid: schema_uid.to_string(),
        element_kind: "message".to_string(),
        name: msg.name.clone(),
        full_name: full_name.clone(),
        parent_element_uid: parent_uid.map(|s| s.to_string()),
        line_start: Some(msg.line_start),
        line_end: Some(msg.line_end),
        metadata_json: Some(metadata.to_string()),
    });

    // Process fields
    for field in &msg.fields {
        let field_uid = Uuid::new_v4().to_string();
        let field_full_name = format!("{}.{}", full_name, field.name);

        let field_metadata = serde_json::json!({
            "number": field.number,
            "label": field.label.as_str(),
            "field_type": field.field_type,
            "is_map": field.is_map,
            "default_value": field.default_value,
        });

        elements.push(ProtoElementInput {
            element_uid: field_uid,
            schema_uid: schema_uid.to_string(),
            element_kind: "field".to_string(),
            name: field.name.clone(),
            full_name: field_full_name,
            parent_element_uid: Some(element_uid.clone()),
            line_start: Some(field.line),
            line_end: Some(field.line),
            metadata_json: Some(field_metadata.to_string()),
        });
    }

    // Process nested messages
    for nested in &msg.nested_messages {
        extract_message_elements(
            elements,
            nested,
            schema_uid,
            &full_name,
            Some(&element_uid),
        );
    }

    // Process nested enums
    for nested in &msg.nested_enums {
        extract_enum_elements(
            elements,
            nested,
            schema_uid,
            &full_name,
            Some(&element_uid),
        );
    }
}

fn extract_enum_elements(
    elements: &mut Vec<ProtoElementInput>,
    enum_def: &ProtoEnum,
    schema_uid: &str,
    package_prefix: &str,
    parent_uid: Option<&str>,
) {
    let element_uid = Uuid::new_v4().to_string();

    // Build full name
    let full_name = if package_prefix.is_empty() {
        enum_def.name.clone()
    } else {
        format!("{}.{}", package_prefix, enum_def.name)
    };

    // Metadata: allow_alias option
    let allow_alias = enum_def
        .options
        .iter()
        .any(|o| o.name == "allow_alias" && o.value == "true");
    let metadata = serde_json::json!({
        "values_count": enum_def.values.len(),
        "allow_alias": allow_alias,
    });

    elements.push(ProtoElementInput {
        element_uid: element_uid.clone(),
        schema_uid: schema_uid.to_string(),
        element_kind: "enum".to_string(),
        name: enum_def.name.clone(),
        full_name: full_name.clone(),
        parent_element_uid: parent_uid.map(|s| s.to_string()),
        line_start: Some(enum_def.line_start),
        line_end: Some(enum_def.line_end),
        metadata_json: Some(metadata.to_string()),
    });

    // Process enum values
    for value in &enum_def.values {
        let value_uid = Uuid::new_v4().to_string();
        let value_full_name = format!("{}.{}", full_name, value.name);

        let value_metadata = serde_json::json!({
            "number": value.number,
        });

        elements.push(ProtoElementInput {
            element_uid: value_uid,
            schema_uid: schema_uid.to_string(),
            element_kind: "enum_value".to_string(),
            name: value.name.clone(),
            full_name: value_full_name,
            parent_element_uid: Some(element_uid.clone()),
            line_start: Some(value.line),
            line_end: Some(value.line),
            metadata_json: Some(value_metadata.to_string()),
        });
    }
}

fn extract_service_elements(
    elements: &mut Vec<ProtoElementInput>,
    service: &ProtoService,
    schema_uid: &str,
    package_prefix: &str,
) {
    let element_uid = Uuid::new_v4().to_string();

    // Build full name
    let full_name = if package_prefix.is_empty() {
        service.name.clone()
    } else {
        format!("{}.{}", package_prefix, service.name)
    };

    let metadata = serde_json::json!({
        "methods_count": service.methods.len(),
    });

    elements.push(ProtoElementInput {
        element_uid: element_uid.clone(),
        schema_uid: schema_uid.to_string(),
        element_kind: "service".to_string(),
        name: service.name.clone(),
        full_name: full_name.clone(),
        parent_element_uid: None,
        line_start: Some(service.line_start),
        line_end: Some(service.line_end),
        metadata_json: Some(metadata.to_string()),
    });

    // Process methods
    for method in &service.methods {
        let method_uid = Uuid::new_v4().to_string();
        let method_full_name = format!("{}.{}", full_name, method.name);

        let method_metadata = serde_json::json!({
            "input_type": method.input_type,
            "output_type": method.output_type,
            "client_streaming": method.client_streaming,
            "server_streaming": method.server_streaming,
            "streaming_pattern": method.streaming_pattern().as_str(),
        });

        elements.push(ProtoElementInput {
            element_uid: method_uid,
            schema_uid: schema_uid.to_string(),
            element_kind: "method".to_string(),
            name: method.name.clone(),
            full_name: method_full_name,
            parent_element_uid: Some(element_uid.clone()),
            line_start: Some(method.line),
            line_end: Some(method.line),
            metadata_json: Some(method_metadata.to_string()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Mock storage ─────────────────────────────────────────────

    #[derive(Default)]
    struct MockProtoStorage {
        schemas: Vec<ProtoSchemaInput>,
        elements: Vec<ProtoElementInput>,
    }

    impl ProtoSchemaStorePort for MockProtoStorage {
        type Error = String;

        fn insert_proto_schema(&mut self, input: &ProtoSchemaInput) -> Result<(), String> {
            self.schemas.push(input.clone());
            Ok(())
        }

        fn insert_proto_elements(&mut self, elements: &[ProtoElementInput]) -> Result<usize, String> {
            let count = elements.len();
            self.elements.extend(elements.iter().cloned());
            Ok(count)
        }
    }

    // ── Tests ────────────────────────────────────────────────────

    #[test]
    fn indexes_simple_message() {
        let mut storage = MockProtoStorage::default();

        let files = vec![ProtoFileInput {
            rel_path: "api/user.proto".to_string(),
            content: r#"
                syntax = "proto3";
                package api;
                message User {
                    string name = 1;
                    int32 age = 2;
                }
            "#.to_string(),
            content_hash: "abc123".to_string(),
        }];

        let result = index_proto_files(&mut storage, "repo-1", "snap-1", &files).unwrap();

        assert_eq!(result.schemas_indexed, 1);
        assert_eq!(result.parse_failures.len(), 0);

        // 1 message + 2 fields = 3 elements
        assert!(result.elements_indexed >= 3);

        // Check schema
        assert_eq!(storage.schemas.len(), 1);
        assert_eq!(storage.schemas[0].package_name, Some("api".to_string()));
        assert_eq!(storage.schemas[0].syntax_version, Some("proto3".to_string()));

        // Check elements
        let messages: Vec<_> = storage.elements.iter()
            .filter(|e| e.element_kind == "message")
            .collect();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].name, "User");
        assert_eq!(messages[0].full_name, "api.User");
    }

    #[test]
    fn indexes_service_with_methods() {
        let mut storage = MockProtoStorage::default();

        let files = vec![ProtoFileInput {
            rel_path: "api/service.proto".to_string(),
            content: r#"
                syntax = "proto3";
                package api;

                message Request {}
                message Response {}

                service UserService {
                    rpc GetUser(Request) returns (Response);
                    rpc StreamUsers(Request) returns (stream Response);
                }
            "#.to_string(),
            content_hash: "def456".to_string(),
        }];

        let result = index_proto_files(&mut storage, "repo-1", "snap-1", &files).unwrap();

        assert_eq!(result.schemas_indexed, 1);

        // Check service
        let services: Vec<_> = storage.elements.iter()
            .filter(|e| e.element_kind == "service")
            .collect();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].full_name, "api.UserService");

        // Check methods
        let methods: Vec<_> = storage.elements.iter()
            .filter(|e| e.element_kind == "method")
            .collect();
        assert_eq!(methods.len(), 2);

        // Verify streaming method has correct metadata
        let stream_method = methods.iter()
            .find(|m| m.name == "StreamUsers")
            .unwrap();
        let meta: serde_json::Value = serde_json::from_str(
            stream_method.metadata_json.as_ref().unwrap()
        ).unwrap();
        assert_eq!(meta["server_streaming"], true);
    }

    #[test]
    fn indexes_nested_messages() {
        let mut storage = MockProtoStorage::default();

        let files = vec![ProtoFileInput {
            rel_path: "nested.proto".to_string(),
            content: r#"
                syntax = "proto3";
                message Outer {
                    message Inner {
                        string value = 1;
                    }
                    Inner nested = 1;
                }
            "#.to_string(),
            content_hash: "nested123".to_string(),
        }];

        let result = index_proto_files(&mut storage, "repo-1", "snap-1", &files).unwrap();

        assert_eq!(result.schemas_indexed, 1);

        // Check nested message full name
        let inner = storage.elements.iter()
            .find(|e| e.name == "Inner")
            .unwrap();
        assert_eq!(inner.full_name, "Outer.Inner");
        assert!(inner.parent_element_uid.is_some());
    }

    #[test]
    fn handles_multiple_files() {
        let mut storage = MockProtoStorage::default();

        // Tree-sitter parsers are error-tolerant, so malformed input may still
        // produce a valid (but empty) ProtoFile. This test verifies we can
        // handle multiple files in a single indexing call.
        let files = vec![
            ProtoFileInput {
                rel_path: "good.proto".to_string(),
                content: r#"
                    syntax = "proto3";
                    message Good { string x = 1; }
                "#.to_string(),
                content_hash: "good".to_string(),
            },
            ProtoFileInput {
                rel_path: "also_good.proto".to_string(),
                content: r#"
                    syntax = "proto3";
                    message AlsoGood { int32 y = 1; }
                "#.to_string(),
                content_hash: "also_good".to_string(),
            },
        ];

        let result = index_proto_files(&mut storage, "repo-1", "snap-1", &files).unwrap();

        // Both files indexed
        assert_eq!(result.schemas_indexed, 2);
        assert_eq!(result.parse_failures.len(), 0);

        // Verify we have both schemas
        assert_eq!(storage.schemas.len(), 2);
        let paths: Vec<_> = storage.schemas.iter().map(|s| s.file_path.as_str()).collect();
        assert!(paths.contains(&"good.proto"));
        assert!(paths.contains(&"also_good.proto"));
    }

    #[test]
    fn indexes_enum_with_values() {
        let mut storage = MockProtoStorage::default();

        let files = vec![ProtoFileInput {
            rel_path: "status.proto".to_string(),
            content: r#"
                syntax = "proto3";
                package api;
                enum Status {
                    UNKNOWN = 0;
                    ACTIVE = 1;
                    INACTIVE = 2;
                }
            "#.to_string(),
            content_hash: "enum123".to_string(),
        }];

        let result = index_proto_files(&mut storage, "repo-1", "snap-1", &files).unwrap();

        assert_eq!(result.schemas_indexed, 1);

        // Check enum
        let enums: Vec<_> = storage.elements.iter()
            .filter(|e| e.element_kind == "enum")
            .collect();
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].full_name, "api.Status");

        // Check enum values
        let values: Vec<_> = storage.elements.iter()
            .filter(|e| e.element_kind == "enum_value")
            .collect();
        assert_eq!(values.len(), 3);
    }
}
