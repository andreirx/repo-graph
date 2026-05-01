//! Protocol Buffers schema model.
//!
//! Domain types representing the structure of .proto files. These types
//! are the output of CS-1 (protobuf schema extraction) and the input to
//! CS-2 (generated code mapping) and GR-1/2 (gRPC detection).
//!
//! ## Design Notes
//!
//! - Full qualification via package namespace (`full_name` = `package.Name`)
//! - Nested messages preserve hierarchy
//! - Services and methods are first-class for gRPC support
//! - Line numbers for source mapping

use serde::{Deserialize, Serialize};

/// A parsed .proto file.
///
/// Root container for all schema elements defined in a single file.
/// Multiple files may share the same package namespace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoFile {
    /// Path to the .proto file (repo-relative).
    pub path: String,

    /// Package namespace (e.g., "google.protobuf", "myapp.api.v1").
    /// Empty string for files without package declaration.
    pub package: String,

    /// Syntax version ("proto2" or "proto3").
    pub syntax: String,

    /// Import statements (paths to other .proto files).
    pub imports: Vec<String>,

    /// Top-level message definitions.
    pub messages: Vec<ProtoMessage>,

    /// Top-level enum definitions.
    pub enums: Vec<ProtoEnum>,

    /// Service definitions (for gRPC).
    pub services: Vec<ProtoService>,

    /// File-level options (e.g., java_package, go_package).
    pub options: Vec<ProtoOption>,
}

impl ProtoFile {
    /// Build the full name for an element in this file's package.
    pub fn qualify_name(&self, name: &str) -> String {
        if self.package.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", self.package, name)
        }
    }
}

/// A Protocol Buffers message definition.
///
/// Messages are the primary data structure in protobuf. They can
/// contain nested messages, enums, and fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoMessage {
    /// Short name (without package prefix).
    pub name: String,

    /// Fully qualified name (package.OuterMessage.InnerMessage).
    pub full_name: String,

    /// Field definitions.
    pub fields: Vec<ProtoField>,

    /// Nested message definitions.
    pub nested_messages: Vec<ProtoMessage>,

    /// Nested enum definitions.
    pub nested_enums: Vec<ProtoEnum>,

    /// Oneof groups.
    pub oneofs: Vec<ProtoOneof>,

    /// Message-level options.
    pub options: Vec<ProtoOption>,

    /// Source line where message definition starts.
    pub line_start: u32,

    /// Source line where message definition ends.
    pub line_end: u32,
}

/// A Protocol Buffers field definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoField {
    /// Field name.
    pub name: String,

    /// Field number (tag).
    pub number: i32,

    /// Field type (scalar, message reference, enum reference).
    /// For message/enum types, this is the fully qualified name.
    pub field_type: String,

    /// Label: optional, required (proto2), repeated.
    pub label: ProtoFieldLabel,

    /// True if this field is part of a map<K,V>.
    pub is_map: bool,

    /// For map fields: key type.
    pub map_key_type: Option<String>,

    /// For map fields: value type.
    pub map_value_type: Option<String>,

    /// Oneof group name if this field is part of a oneof.
    pub oneof_name: Option<String>,

    /// Default value (proto2).
    pub default_value: Option<String>,

    /// Field-level options.
    pub options: Vec<ProtoOption>,

    /// Source line number.
    pub line: u32,
}

/// Field label (cardinality).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtoFieldLabel {
    /// Optional field (proto3 default, proto2 explicit).
    Optional,

    /// Required field (proto2 only, deprecated).
    Required,

    /// Repeated field (list/array).
    Repeated,
}

impl ProtoFieldLabel {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProtoFieldLabel::Optional => "optional",
            ProtoFieldLabel::Required => "required",
            ProtoFieldLabel::Repeated => "repeated",
        }
    }
}

/// A oneof group within a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoOneof {
    /// Oneof group name.
    pub name: String,

    /// Field names belonging to this oneof.
    pub field_names: Vec<String>,
}

/// A Protocol Buffers enum definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoEnum {
    /// Short name (without package prefix).
    pub name: String,

    /// Fully qualified name.
    pub full_name: String,

    /// Enum values.
    pub values: Vec<ProtoEnumValue>,

    /// Enum-level options.
    pub options: Vec<ProtoOption>,

    /// Source line where enum definition starts.
    pub line_start: u32,

    /// Source line where enum definition ends.
    pub line_end: u32,
}

/// A Protocol Buffers enum value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoEnumValue {
    /// Value name (e.g., "UNKNOWN", "ACTIVE").
    pub name: String,

    /// Integer value.
    pub number: i32,

    /// Value-level options.
    pub options: Vec<ProtoOption>,

    /// Source line number.
    pub line: u32,
}

/// A Protocol Buffers service definition (for gRPC).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoService {
    /// Short name (without package prefix).
    pub name: String,

    /// Fully qualified name (package.ServiceName).
    pub full_name: String,

    /// RPC method definitions.
    pub methods: Vec<ProtoMethod>,

    /// Service-level options.
    pub options: Vec<ProtoOption>,

    /// Source line where service definition starts.
    pub line_start: u32,

    /// Source line where service definition ends.
    pub line_end: u32,
}

/// A Protocol Buffers RPC method definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoMethod {
    /// Method name.
    pub name: String,

    /// Fully qualified name (package.Service.Method).
    pub full_name: String,

    /// Input message type (fully qualified).
    pub input_type: String,

    /// Output message type (fully qualified).
    pub output_type: String,

    /// True if client sends a stream of messages.
    pub client_streaming: bool,

    /// True if server sends a stream of messages.
    pub server_streaming: bool,

    /// Method-level options.
    pub options: Vec<ProtoOption>,

    /// Source line number.
    pub line: u32,
}

impl ProtoMethod {
    /// Classify the streaming pattern.
    pub fn streaming_pattern(&self) -> StreamingPattern {
        match (self.client_streaming, self.server_streaming) {
            (false, false) => StreamingPattern::Unary,
            (true, false) => StreamingPattern::ClientStream,
            (false, true) => StreamingPattern::ServerStream,
            (true, true) => StreamingPattern::BidirectionalStream,
        }
    }
}

/// RPC streaming pattern classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamingPattern {
    /// Single request, single response.
    Unary,

    /// Stream of requests, single response.
    ClientStream,

    /// Single request, stream of responses.
    ServerStream,

    /// Stream of requests, stream of responses.
    BidirectionalStream,
}

impl StreamingPattern {
    /// SQL/storage string representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            StreamingPattern::Unary => "unary",
            StreamingPattern::ClientStream => "client_stream",
            StreamingPattern::ServerStream => "server_stream",
            StreamingPattern::BidirectionalStream => "bidirectional_stream",
        }
    }
}

/// A Protocol Buffers option (key-value pair).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtoOption {
    /// Option name (e.g., "java_package", "deprecated").
    pub name: String,

    /// Option value as string (complex values serialized as JSON).
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_file_qualify_name_with_package() {
        let file = ProtoFile {
            path: "api/v1/user.proto".to_string(),
            package: "api.v1".to_string(),
            syntax: "proto3".to_string(),
            imports: vec![],
            messages: vec![],
            enums: vec![],
            services: vec![],
            options: vec![],
        };

        assert_eq!(file.qualify_name("User"), "api.v1.User");
    }

    #[test]
    fn proto_file_qualify_name_without_package() {
        let file = ProtoFile {
            path: "user.proto".to_string(),
            package: String::new(),
            syntax: "proto3".to_string(),
            imports: vec![],
            messages: vec![],
            enums: vec![],
            services: vec![],
            options: vec![],
        };

        assert_eq!(file.qualify_name("User"), "User");
    }

    #[test]
    fn proto_method_streaming_pattern() {
        let unary = ProtoMethod {
            name: "GetUser".to_string(),
            full_name: "api.UserService.GetUser".to_string(),
            input_type: "api.GetUserRequest".to_string(),
            output_type: "api.GetUserResponse".to_string(),
            client_streaming: false,
            server_streaming: false,
            options: vec![],
            line: 10,
        };
        assert_eq!(unary.streaming_pattern(), StreamingPattern::Unary);

        let bidi = ProtoMethod {
            name: "Chat".to_string(),
            full_name: "api.ChatService.Chat".to_string(),
            input_type: "api.ChatMessage".to_string(),
            output_type: "api.ChatMessage".to_string(),
            client_streaming: true,
            server_streaming: true,
            options: vec![],
            line: 20,
        };
        assert_eq!(
            bidi.streaming_pattern(),
            StreamingPattern::BidirectionalStream
        );
    }

    #[test]
    fn streaming_pattern_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&StreamingPattern::Unary).unwrap(),
            "\"unary\""
        );
        assert_eq!(
            serde_json::to_string(&StreamingPattern::BidirectionalStream).unwrap(),
            "\"bidirectional_stream\""
        );
    }
}
