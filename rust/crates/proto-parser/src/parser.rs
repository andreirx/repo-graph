//! Core parser implementation using tree-sitter-proto.
//!
//! Parses `.proto` source text into `ProtoFile` domain objects.
//! Handles both proto2 and proto3 syntax.

use contract_schema::{
    ProtoEnum, ProtoEnumValue, ProtoField, ProtoFieldLabel, ProtoFile, ProtoMessage, ProtoMethod,
    ProtoOneof, ProtoOption, ProtoService,
};
use tree_sitter::{Node, Parser, Tree};

/// Result of parsing a `.proto` file.
pub type ParseResult = Result<ProtoFile, ParseError>;

/// Error during proto parsing.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Error message.
    pub message: String,
    /// Source file path (for diagnostics).
    pub path: String,
    /// Line number where error occurred (if known).
    pub line: Option<u32>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{}: {}", self.path, line, self.message),
            None => write!(f, "{}: {}", self.path, self.message),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a `.proto` file into a `ProtoFile` domain object.
///
/// # Arguments
///
/// * `path` - Repo-relative path to the `.proto` file (for diagnostics and identity)
/// * `source` - Source text of the `.proto` file
///
/// # Returns
///
/// `Ok(ProtoFile)` on success, `Err(ParseError)` on parse failure.
///
/// # Example
///
/// ```
/// use repo_graph_proto_parser::parse_proto;
///
/// let source = r#"
/// syntax = "proto3";
/// package api.v1;
///
/// message User {
///     string name = 1;
///     int32 age = 2;
/// }
/// "#;
///
/// let result = parse_proto("api/v1/user.proto", source).unwrap();
/// assert_eq!(result.package, "api.v1");
/// assert_eq!(result.messages.len(), 1);
/// assert_eq!(result.messages[0].name, "User");
/// ```
pub fn parse_proto(path: &str, source: &str) -> ParseResult {
    let mut parser = Parser::new();
    let language = tree_sitter_proto::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| ParseError {
            message: format!("Failed to set language: {}", e),
            path: path.to_string(),
            line: None,
        })?;

    let tree = parser.parse(source, None).ok_or_else(|| ParseError {
        message: "Failed to parse proto file".to_string(),
        path: path.to_string(),
        line: None,
    })?;

    extract_proto_file(path, source, &tree)
}

/// Extract a `ProtoFile` from a parsed tree-sitter tree.
fn extract_proto_file(path: &str, source: &str, tree: &Tree) -> ParseResult {
    let root = tree.root_node();

    let mut syntax = "proto3".to_string(); // Default
    let mut package = String::new();
    let mut imports = Vec::new();
    let mut options = Vec::new();
    let mut messages = Vec::new();
    let mut enums = Vec::new();
    let mut services = Vec::new();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "syntax" => {
                syntax = extract_syntax(&child, source);
            }
            "package" => {
                package = extract_package(&child, source);
            }
            "import" => {
                if let Some(import_path) = extract_import(&child, source) {
                    imports.push(import_path);
                }
            }
            "option" => {
                if let Some(opt) = extract_option(&child, source) {
                    options.push(opt);
                }
            }
            "message" => {
                if let Some(msg) = extract_message(&child, source, &package) {
                    messages.push(msg);
                }
            }
            "enum" => {
                if let Some(e) = extract_enum(&child, source, &package) {
                    enums.push(e);
                }
            }
            "service" => {
                if let Some(svc) = extract_service(&child, source, &package) {
                    services.push(svc);
                }
            }
            _ => {
                // Skip comments, whitespace, errors
            }
        }
    }

    Ok(ProtoFile {
        path: path.to_string(),
        package,
        syntax,
        imports,
        messages,
        enums,
        services,
        options,
    })
}

/// Extract syntax version from a `syntax` node.
fn extract_syntax(node: &Node, source: &str) -> String {
    // syntax = "proto3";
    // The string literal node kind is the actual quoted string like "proto2" or "proto3"
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        // The node kind for string literals is the literal itself with quotes
        if kind.starts_with('"') || kind == "string" {
            let text = node_text(&child, source);
            // Remove quotes
            return text.trim_matches('"').trim_matches('\'').to_string();
        }
    }
    "proto3".to_string()
}

/// Extract package name from a `package` node.
fn extract_package(node: &Node, source: &str) -> String {
    // package api.v1;
    // Look for the full_ident child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "full_ident" {
            return node_text(&child, source);
        }
    }
    String::new()
}

/// Extract import path from an `import` node.
fn extract_import(node: &Node, source: &str) -> Option<String> {
    // import "path/to/file.proto";
    // import public "path/to/file.proto";
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string" {
            let text = node_text(&child, source);
            return Some(text.trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

/// Extract an option from an `option` node.
fn extract_option(node: &Node, source: &str) -> Option<ProtoOption> {
    // option java_package = "com.example";
    let mut name = None;
    let mut value = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "option_name" | "full_ident" | "identifier" => {
                if name.is_none() {
                    name = Some(node_text(&child, source));
                }
            }
            "constant" | "string" | "int_lit" | "bool" => {
                let text = node_text(&child, source);
                value = Some(text.trim_matches('"').trim_matches('\'').to_string());
            }
            _ => {}
        }
    }

    match (name, value) {
        (Some(n), Some(v)) => Some(ProtoOption { name: n, value: v }),
        (Some(n), None) => Some(ProtoOption {
            name: n,
            value: String::new(),
        }),
        _ => None,
    }
}

/// Extract a message from a `message` node.
fn extract_message(node: &Node, source: &str, parent_full_name: &str) -> Option<ProtoMessage> {
    let mut name = String::new();
    let mut fields = Vec::new();
    let mut nested_messages = Vec::new();
    let mut nested_enums = Vec::new();
    let mut oneofs = Vec::new();
    let mut options = Vec::new();

    let line_start = node.start_position().row as u32 + 1;
    let line_end = node.end_position().row as u32 + 1;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "message_name" | "identifier" => {
                if name.is_empty() {
                    name = node_text(&child, source);
                }
            }
            "message_body" => {
                // Process message body children
                let mut body_cursor = child.walk();
                for body_child in child.children(&mut body_cursor) {
                    match body_child.kind() {
                        "field" => {
                            if let Some(field) = extract_field(&body_child, source) {
                                fields.push(field);
                            }
                        }
                        "message" => {
                            let nested_parent = if parent_full_name.is_empty() {
                                name.clone()
                            } else {
                                format!("{}.{}", parent_full_name, name)
                            };
                            if let Some(msg) = extract_message(&body_child, source, &nested_parent)
                            {
                                nested_messages.push(msg);
                            }
                        }
                        "enum" => {
                            let nested_parent = if parent_full_name.is_empty() {
                                name.clone()
                            } else {
                                format!("{}.{}", parent_full_name, name)
                            };
                            if let Some(e) = extract_enum(&body_child, source, &nested_parent) {
                                nested_enums.push(e);
                            }
                        }
                        "oneof" => {
                            if let Some(o) = extract_oneof(&body_child, source) {
                                // Mark fields belonging to this oneof
                                for field_name in &o.field_names {
                                    for field in &mut fields {
                                        if &field.name == field_name {
                                            field.oneof_name = Some(o.name.clone());
                                        }
                                    }
                                }
                                oneofs.push(o);
                            }
                        }
                        "option" => {
                            if let Some(opt) = extract_option(&body_child, source) {
                                options.push(opt);
                            }
                        }
                        "map_field" => {
                            if let Some(field) = extract_map_field(&body_child, source) {
                                fields.push(field);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    if name.is_empty() {
        return None;
    }

    let full_name = if parent_full_name.is_empty() {
        name.clone()
    } else {
        format!("{}.{}", parent_full_name, name)
    };

    Some(ProtoMessage {
        name,
        full_name,
        fields,
        nested_messages,
        nested_enums,
        oneofs,
        options,
        line_start,
        line_end,
    })
}

/// Extract a field from a `field` node.
fn extract_field(node: &Node, source: &str) -> Option<ProtoField> {
    let mut name = String::new();
    let mut number = 0;
    let mut field_type = String::new();
    let mut label = ProtoFieldLabel::Optional;
    let mut options = Vec::new();

    let line = node.start_position().row as u32 + 1;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "field_name" | "identifier" => {
                // The field name is typically after the type
                let text = node_text(&child, source);
                if field_type.is_empty() {
                    // This might be a label or type
                    match text.as_str() {
                        "optional" => label = ProtoFieldLabel::Optional,
                        "required" => label = ProtoFieldLabel::Required,
                        "repeated" => label = ProtoFieldLabel::Repeated,
                        _ => {
                            if name.is_empty() && !text.is_empty() {
                                // Could be type or name, context-dependent
                                // For now, treat as name if we already have a type
                            }
                        }
                    }
                } else if name.is_empty() {
                    name = text;
                }
            }
            "type" | "message_or_enum_type" => {
                field_type = node_text(&child, source);
            }
            "field_number" | "int_lit" => {
                if let Ok(n) = node_text(&child, source).parse() {
                    number = n;
                }
            }
            "field_options" => {
                let mut opt_cursor = child.walk();
                for opt_child in child.children(&mut opt_cursor) {
                    if opt_child.kind() == "field_option" {
                        if let Some(opt) = extract_field_option(&opt_child, source) {
                            options.push(opt);
                        }
                    }
                }
            }
            _ => {
                // Check if this is a label keyword
                let text = node_text(&child, source);
                match text.as_str() {
                    "optional" => label = ProtoFieldLabel::Optional,
                    "required" => label = ProtoFieldLabel::Required,
                    "repeated" => label = ProtoFieldLabel::Repeated,
                    _ => {}
                }
            }
        }
    }

    // Try to extract from the raw text if structured extraction failed
    if name.is_empty() || field_type.is_empty() {
        let text = node_text(node, source);
        if let Some((parsed_label, parsed_type, parsed_name, parsed_num)) = parse_field_text(&text)
        {
            if name.is_empty() {
                name = parsed_name;
            }
            if field_type.is_empty() {
                field_type = parsed_type;
            }
            if number == 0 {
                number = parsed_num;
            }
            label = parsed_label;
        }
    }

    if name.is_empty() || field_type.is_empty() {
        return None;
    }

    Some(ProtoField {
        name,
        number,
        field_type,
        label,
        is_map: false,
        map_key_type: None,
        map_value_type: None,
        oneof_name: None,
        default_value: None,
        options,
        line,
    })
}

/// Parse a field from its text representation (fallback).
fn parse_field_text(text: &str) -> Option<(ProtoFieldLabel, String, String, i32)> {
    let text = text.trim();
    let parts: Vec<&str> = text.split_whitespace().collect();

    if parts.len() < 3 {
        return None;
    }

    let mut idx = 0;
    let label = match *parts.get(idx)? {
        "optional" => {
            idx += 1;
            ProtoFieldLabel::Optional
        }
        "required" => {
            idx += 1;
            ProtoFieldLabel::Required
        }
        "repeated" => {
            idx += 1;
            ProtoFieldLabel::Repeated
        }
        _ => ProtoFieldLabel::Optional,
    };

    let field_type = parts.get(idx)?.to_string();
    idx += 1;

    let name = parts.get(idx)?.trim_end_matches(';').to_string();
    idx += 1;

    // Skip '='
    if parts.get(idx) == Some(&"=") {
        idx += 1;
    }

    let number: i32 = parts
        .get(idx)?
        .trim_end_matches(';')
        .trim_end_matches('[')
        .parse()
        .ok()?;

    Some((label, field_type, name, number))
}

/// Extract a field option.
fn extract_field_option(node: &Node, source: &str) -> Option<ProtoOption> {
    let mut name = None;
    let mut value = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "option_name" | "identifier" => {
                if name.is_none() {
                    name = Some(node_text(&child, source));
                }
            }
            "constant" | "string" | "int_lit" | "bool" => {
                value = Some(node_text(&child, source));
            }
            _ => {}
        }
    }

    Some(ProtoOption {
        name: name.unwrap_or_default(),
        value: value.unwrap_or_default(),
    })
}

/// Extract a map field from a `map_field` node.
fn extract_map_field(node: &Node, source: &str) -> Option<ProtoField> {
    // map<key_type, value_type> name = number;
    let mut name = String::new();
    let mut number = 0;
    let mut key_type = String::new();
    let mut value_type = String::new();

    let line = node.start_position().row as u32 + 1;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "key_type" => {
                key_type = node_text(&child, source);
            }
            "type" | "message_or_enum_type" => {
                if key_type.is_empty() {
                    key_type = node_text(&child, source);
                } else {
                    value_type = node_text(&child, source);
                }
            }
            "map_name" | "identifier" => {
                name = node_text(&child, source);
            }
            "field_number" | "int_lit" => {
                if let Ok(n) = node_text(&child, source).parse() {
                    number = n;
                }
            }
            _ => {}
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(ProtoField {
        name,
        number,
        field_type: format!("map<{}, {}>", key_type, value_type),
        label: ProtoFieldLabel::Repeated,
        is_map: true,
        map_key_type: Some(key_type),
        map_value_type: Some(value_type),
        oneof_name: None,
        default_value: None,
        options: vec![],
        line,
    })
}

/// Extract a oneof from a `oneof` node.
fn extract_oneof(node: &Node, source: &str) -> Option<ProtoOneof> {
    let mut name = String::new();
    let mut field_names = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "oneof_name" | "identifier" => {
                if name.is_empty() {
                    name = node_text(&child, source);
                }
            }
            "oneof_body" => {
                let mut body_cursor = child.walk();
                for body_child in child.children(&mut body_cursor) {
                    if body_child.kind() == "oneof_field" {
                        if let Some(field_name) = extract_oneof_field_name(&body_child, source) {
                            field_names.push(field_name);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(ProtoOneof { name, field_names })
}

/// Extract the field name from a oneof field.
fn extract_oneof_field_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "field_name" || child.kind() == "identifier" {
            let text = node_text(&child, source);
            // Skip type keywords
            if !matches!(
                text.as_str(),
                "string"
                    | "int32"
                    | "int64"
                    | "uint32"
                    | "uint64"
                    | "bool"
                    | "bytes"
                    | "float"
                    | "double"
            ) {
                return Some(text);
            }
        }
    }
    None
}

/// Extract an enum from an `enum` node.
fn extract_enum(node: &Node, source: &str, parent_full_name: &str) -> Option<ProtoEnum> {
    let mut name = String::new();
    let mut values = Vec::new();
    let mut options = Vec::new();

    let line_start = node.start_position().row as u32 + 1;
    let line_end = node.end_position().row as u32 + 1;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "enum_name" | "identifier" => {
                if name.is_empty() {
                    name = node_text(&child, source);
                }
            }
            "enum_body" => {
                let mut body_cursor = child.walk();
                for body_child in child.children(&mut body_cursor) {
                    match body_child.kind() {
                        "enum_field" | "enum_value" => {
                            if let Some(v) = extract_enum_value(&body_child, source) {
                                values.push(v);
                            }
                        }
                        "option" => {
                            if let Some(opt) = extract_option(&body_child, source) {
                                options.push(opt);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    if name.is_empty() {
        return None;
    }

    let full_name = if parent_full_name.is_empty() {
        name.clone()
    } else {
        format!("{}.{}", parent_full_name, name)
    };

    Some(ProtoEnum {
        name,
        full_name,
        values,
        options,
        line_start,
        line_end,
    })
}

/// Extract an enum value.
fn extract_enum_value(node: &Node, source: &str) -> Option<ProtoEnumValue> {
    let mut name = String::new();
    let mut number = 0;
    let options = Vec::new(); // Enum value options not extracted yet

    let line = node.start_position().row as u32 + 1;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                if name.is_empty() {
                    name = node_text(&child, source);
                }
            }
            "int_lit" | "-" => {
                // Handle negative numbers
                let text = node_text(&child, source);
                if text == "-" {
                    // Next sibling should be the number
                    continue;
                }
                if let Ok(n) = text.parse() {
                    number = n;
                }
            }
            "enum_value_options" => {
                // Extract options
            }
            _ => {}
        }
    }

    // Fallback: parse from text
    if name.is_empty() {
        let text = node_text(node, source).trim().to_string();
        let parts: Vec<&str> = text.split('=').collect();
        if let Some(n) = parts.first() {
            name = n.trim().to_string();
        }
        if let Some(v) = parts.get(1) {
            if let Ok(n) = v.trim().trim_end_matches(';').parse() {
                number = n;
            }
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(ProtoEnumValue {
        name,
        number,
        options,
        line,
    })
}

/// Extract a service from a `service` node.
fn extract_service(node: &Node, source: &str, package: &str) -> Option<ProtoService> {
    let mut name = String::new();
    let mut methods = Vec::new();
    let mut options = Vec::new();

    let line_start = node.start_position().row as u32 + 1;
    let line_end = node.end_position().row as u32 + 1;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "service_name" => {
                // service_name contains an identifier child
                let mut name_cursor = child.walk();
                for name_child in child.children(&mut name_cursor) {
                    if name_child.kind() == "identifier" {
                        name = node_text(&name_child, source);
                        break;
                    }
                }
                if name.is_empty() {
                    name = node_text(&child, source);
                }
            }
            // rpc nodes are direct children of service, not inside service_body
            "rpc" => {
                let service_full = if package.is_empty() {
                    name.clone()
                } else {
                    format!("{}.{}", package, name)
                };
                if let Some(m) = extract_method(&child, source, &service_full) {
                    methods.push(m);
                }
            }
            "option" => {
                if let Some(opt) = extract_option(&child, source) {
                    options.push(opt);
                }
            }
            // Also handle service_body for grammars that use it
            "service_body" => {
                let mut body_cursor = child.walk();
                for body_child in child.children(&mut body_cursor) {
                    match body_child.kind() {
                        "rpc" => {
                            let service_full = if package.is_empty() {
                                name.clone()
                            } else {
                                format!("{}.{}", package, name)
                            };
                            if let Some(m) = extract_method(&body_child, source, &service_full) {
                                methods.push(m);
                            }
                        }
                        "option" => {
                            if let Some(opt) = extract_option(&body_child, source) {
                                options.push(opt);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    if name.is_empty() {
        return None;
    }

    let full_name = if package.is_empty() {
        name.clone()
    } else {
        format!("{}.{}", package, name)
    };

    Some(ProtoService {
        name,
        full_name,
        methods,
        options,
        line_start,
        line_end,
    })
}

/// Extract an RPC method from an `rpc` node.
fn extract_method(node: &Node, source: &str, service_full_name: &str) -> Option<ProtoMethod> {
    let mut name = String::new();
    let mut input_type = String::new();
    let mut output_type = String::new();
    let mut client_streaming = false;
    let mut server_streaming = false;
    let mut options = Vec::new();

    let line = node.start_position().row as u32 + 1;

    let text = node_text(node, source);

    // Parse streaming markers
    if text.contains("stream") {
        // Need to determine which is streaming
        // rpc Method(stream Input) returns (Output)
        // rpc Method(Input) returns (stream Output)
        let parts: Vec<&str> = text.split("returns").collect();
        if let Some(input_part) = parts.first() {
            if input_part.contains("stream") {
                client_streaming = true;
            }
        }
        if let Some(output_part) = parts.get(1) {
            if output_part.contains("stream") {
                server_streaming = true;
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "rpc_name" | "identifier" => {
                if name.is_empty() {
                    name = node_text(&child, source);
                }
            }
            "message_type" | "message_or_enum_type" | "type" => {
                let type_text = node_text(&child, source);
                if input_type.is_empty() {
                    input_type = type_text;
                } else if output_type.is_empty() {
                    output_type = type_text;
                }
            }
            "rpc_body" => {
                let mut body_cursor = child.walk();
                for body_child in child.children(&mut body_cursor) {
                    if body_child.kind() == "option" {
                        if let Some(opt) = extract_option(&body_child, source) {
                            options.push(opt);
                        }
                    }
                }
            }
            _ => {
                // Check for stream keyword
                let text = node_text(&child, source);
                if text == "stream" {
                    // Context determines which stream this is
                }
            }
        }
    }

    // Fallback: parse from text
    if name.is_empty() || input_type.is_empty() || output_type.is_empty() {
        if let Some((parsed_name, parsed_input, parsed_output, cs, ss)) = parse_rpc_text(&text) {
            if name.is_empty() {
                name = parsed_name;
            }
            if input_type.is_empty() {
                input_type = parsed_input;
            }
            if output_type.is_empty() {
                output_type = parsed_output;
            }
            client_streaming = cs;
            server_streaming = ss;
        }
    }

    if name.is_empty() {
        return None;
    }

    let full_name = format!("{}.{}", service_full_name, name);

    Some(ProtoMethod {
        name,
        full_name,
        input_type,
        output_type,
        client_streaming,
        server_streaming,
        options,
        line,
    })
}

/// Parse an RPC definition from its text (fallback).
fn parse_rpc_text(text: &str) -> Option<(String, String, String, bool, bool)> {
    // rpc MethodName(InputType) returns (OutputType);
    // rpc MethodName(stream InputType) returns (stream OutputType);
    let text = text.trim();
    if !text.starts_with("rpc") {
        return None;
    }

    let text = text.trim_start_matches("rpc").trim();

    // Extract method name
    let name_end = text.find('(')?;
    let name = text[..name_end].trim().to_string();

    // Extract input
    let input_start = name_end + 1;
    let input_end = text.find(')')?;
    let input_text = text[input_start..input_end].trim();
    let (client_streaming, input_type) = if input_text.starts_with("stream") {
        (
            true,
            input_text.trim_start_matches("stream").trim().to_string(),
        )
    } else {
        (false, input_text.to_string())
    };

    // Extract output
    let returns_idx = text.find("returns")?;
    let output_start = text[returns_idx..].find('(')? + returns_idx + 1;
    let output_end = text[output_start..].find(')')? + output_start;
    let output_text = text[output_start..output_end].trim();
    let (server_streaming, output_type) = if output_text.starts_with("stream") {
        (
            true,
            output_text.trim_start_matches("stream").trim().to_string(),
        )
    } else {
        (false, output_text.to_string())
    };

    Some((
        name,
        input_type,
        output_type,
        client_streaming,
        server_streaming,
    ))
}

/// Get the text content of a node.
fn node_text(node: &Node, source: &str) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    if start < source.len() && end <= source.len() && start <= end {
        source[start..end].to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_proto3() {
        let source = r#"
syntax = "proto3";
package api.v1;

message User {
    string name = 1;
    int32 age = 2;
}
"#;
        let result = parse_proto("test.proto", source).unwrap();
        assert_eq!(result.syntax, "proto3");
        assert_eq!(result.package, "api.v1");
        assert_eq!(result.messages.len(), 1);

        let msg = &result.messages[0];
        assert_eq!(msg.name, "User");
        assert_eq!(msg.full_name, "api.v1.User");
        assert_eq!(msg.fields.len(), 2);

        assert_eq!(msg.fields[0].name, "name");
        assert_eq!(msg.fields[0].field_type, "string");
        assert_eq!(msg.fields[0].number, 1);

        assert_eq!(msg.fields[1].name, "age");
        assert_eq!(msg.fields[1].field_type, "int32");
        assert_eq!(msg.fields[1].number, 2);
    }

    #[test]
    fn parse_proto2_with_required() {
        let source = r#"
syntax = "proto2";
package legacy;

message OldMessage {
    required string id = 1;
    optional string name = 2;
    repeated int32 values = 3;
}
"#;
        let result = parse_proto("legacy.proto", source).unwrap();
        assert_eq!(result.syntax, "proto2");

        let msg = &result.messages[0];
        assert_eq!(msg.fields[0].label, ProtoFieldLabel::Required);
        assert_eq!(msg.fields[1].label, ProtoFieldLabel::Optional);
        assert_eq!(msg.fields[2].label, ProtoFieldLabel::Repeated);
    }

    #[test]
    fn parse_enum() {
        let source = r#"
syntax = "proto3";

enum Status {
    UNKNOWN = 0;
    ACTIVE = 1;
    INACTIVE = 2;
}
"#;
        let result = parse_proto("status.proto", source).unwrap();
        assert_eq!(result.enums.len(), 1);

        let e = &result.enums[0];
        assert_eq!(e.name, "Status");
        assert_eq!(e.values.len(), 3);
        assert_eq!(e.values[0].name, "UNKNOWN");
        assert_eq!(e.values[0].number, 0);
        assert_eq!(e.values[1].name, "ACTIVE");
        assert_eq!(e.values[1].number, 1);
    }

    #[test]
    fn parse_service() {
        let source = r#"
syntax = "proto3";
package api;

service UserService {
    rpc GetUser(GetUserRequest) returns (GetUserResponse);
    rpc ListUsers(ListUsersRequest) returns (stream User);
    rpc CreateUsers(stream CreateUserRequest) returns (CreateUsersResponse);
    rpc Chat(stream ChatMessage) returns (stream ChatMessage);
}

message GetUserRequest {}
message GetUserResponse {}
message ListUsersRequest {}
message User {}
message CreateUserRequest {}
message CreateUsersResponse {}
message ChatMessage {}
"#;
        let result = parse_proto("service.proto", source).unwrap();
        assert_eq!(result.services.len(), 1);

        let svc = &result.services[0];
        assert_eq!(svc.name, "UserService");
        assert_eq!(svc.full_name, "api.UserService");
        assert_eq!(svc.methods.len(), 4);

        // Unary
        assert_eq!(svc.methods[0].name, "GetUser");
        assert!(!svc.methods[0].client_streaming);
        assert!(!svc.methods[0].server_streaming);

        // Server streaming
        assert_eq!(svc.methods[1].name, "ListUsers");
        assert!(!svc.methods[1].client_streaming);
        assert!(svc.methods[1].server_streaming);

        // Client streaming
        assert_eq!(svc.methods[2].name, "CreateUsers");
        assert!(svc.methods[2].client_streaming);
        assert!(!svc.methods[2].server_streaming);

        // Bidirectional
        assert_eq!(svc.methods[3].name, "Chat");
        assert!(svc.methods[3].client_streaming);
        assert!(svc.methods[3].server_streaming);
    }

    #[test]
    fn parse_nested_message() {
        let source = r#"
syntax = "proto3";
package api;

message Outer {
    string name = 1;

    message Inner {
        int32 value = 1;
    }

    Inner inner = 2;
}
"#;
        let result = parse_proto("nested.proto", source).unwrap();

        let outer = &result.messages[0];
        assert_eq!(outer.name, "Outer");
        assert_eq!(outer.full_name, "api.Outer");
        assert_eq!(outer.nested_messages.len(), 1);

        let inner = &outer.nested_messages[0];
        assert_eq!(inner.name, "Inner");
        assert_eq!(inner.full_name, "api.Outer.Inner");
    }

    #[test]
    fn parse_imports() {
        let source = r#"
syntax = "proto3";
package api;

import "google/protobuf/timestamp.proto";
import "common/types.proto";

message Event {
    google.protobuf.Timestamp created_at = 1;
}
"#;
        let result = parse_proto("event.proto", source).unwrap();
        assert_eq!(result.imports.len(), 2);
        assert!(result
            .imports
            .contains(&"google/protobuf/timestamp.proto".to_string()));
        assert!(result.imports.contains(&"common/types.proto".to_string()));
    }

    #[test]
    fn parse_options() {
        let source = r#"
syntax = "proto3";
package api.v1;

option java_package = "com.example.api.v1";
option go_package = "example.com/api/v1";

message User {
    string name = 1;
}
"#;
        let result = parse_proto("options.proto", source).unwrap();
        assert!(result.options.len() >= 2);

        let java_pkg = result.options.iter().find(|o| o.name == "java_package");
        assert!(java_pkg.is_some());
        assert_eq!(java_pkg.unwrap().value, "com.example.api.v1");
    }

    #[test]
    fn parse_no_package() {
        let source = r#"
syntax = "proto3";

message Simple {
    string value = 1;
}
"#;
        let result = parse_proto("simple.proto", source).unwrap();
        assert_eq!(result.package, "");
        assert_eq!(result.messages[0].full_name, "Simple");
    }

    #[test]
    fn line_numbers_captured() {
        let source = r#"syntax = "proto3";
package api;

message User {
    string name = 1;
}
"#;
        let result = parse_proto("lines.proto", source).unwrap();

        let msg = &result.messages[0];
        assert!(msg.line_start > 0);
        assert!(msg.line_end >= msg.line_start);
        assert!(msg.fields[0].line > 0);
    }

    #[test]
    fn parse_repeated_field() {
        let source = r#"
syntax = "proto3";

message Container {
    repeated string items = 1;
}
"#;
        let result = parse_proto("container.proto", source).unwrap();
        let msg = &result.messages[0];
        assert_eq!(msg.fields[0].label, ProtoFieldLabel::Repeated);
        assert_eq!(msg.fields[0].name, "items");
    }

    #[test]
    fn parse_message_type_field() {
        let source = r#"
syntax = "proto3";
package api;

message Address {
    string street = 1;
}

message User {
    string name = 1;
    Address address = 2;
}
"#;
        let result = parse_proto("user.proto", source).unwrap();
        assert_eq!(result.messages.len(), 2);

        let user = &result.messages[1];
        assert_eq!(user.fields[1].field_type, "Address");
    }

    #[test]
    fn parse_empty_message() {
        let source = r#"
syntax = "proto3";

message Empty {}
"#;
        let result = parse_proto("empty.proto", source).unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].name, "Empty");
        assert_eq!(result.messages[0].fields.len(), 0);
    }

    #[test]
    fn parse_deeply_nested() {
        let source = r#"
syntax = "proto3";
package deep;

message Level1 {
    message Level2 {
        message Level3 {
            string value = 1;
        }
    }
}
"#;
        let result = parse_proto("deep.proto", source).unwrap();
        let l1 = &result.messages[0];
        assert_eq!(l1.full_name, "deep.Level1");
        assert_eq!(l1.nested_messages.len(), 1);

        let l2 = &l1.nested_messages[0];
        assert_eq!(l2.full_name, "deep.Level1.Level2");
        assert_eq!(l2.nested_messages.len(), 1);

        let l3 = &l2.nested_messages[0];
        assert_eq!(l3.full_name, "deep.Level1.Level2.Level3");
    }
}
