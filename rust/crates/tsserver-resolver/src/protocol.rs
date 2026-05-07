//! TSServer protocol types.
//!
//! TSServer uses a custom JSON protocol over stdin/stdout:
//! - Newline-delimited JSON (not Content-Length framing like LSP)
//! - Request/response correlation via `seq` / `request_seq`
//! - Events emitted asynchronously
//!
//! This module defines the DTOs for protocol messages.
//! All TSServer-specific types stay in this crate.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Request Types
// ─────────────────────────────────────────────────────────────────────────────

/// A TSServer request message.
#[derive(Debug, Serialize)]
pub struct Request<T: Serialize> {
    pub seq: i32,
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub command: &'static str,
    pub arguments: T,
}

impl<T: Serialize> Request<T> {
    pub fn new(seq: i32, command: &'static str, arguments: T) -> Self {
        Self {
            seq,
            msg_type: "request",
            command,
            arguments,
        }
    }
}

/// Arguments for the `open` command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenArgs {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_kind_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_root_path: Option<String>,
}

/// Arguments for the `quickinfo` command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickInfoArgs {
    pub file: String,
    pub line: u32,
    pub offset: u32,
}

/// Arguments for the `close` command.
#[derive(Debug, Serialize)]
pub struct CloseArgs {
    pub file: String,
}

/// Arguments for the `configure` command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_info: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<ConfigurePreferences>,
}

/// Preferences for the `configure` command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurePreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provide_prefix_and_suffix_text_for_rename: Option<bool>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Response Types
// ─────────────────────────────────────────────────────────────────────────────

/// A TSServer response message.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields are complete for deserialization, not all are read
pub struct Response {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub seq: i32,
    pub request_seq: i32,
    pub command: String,
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

/// QuickInfo response body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Fields are complete for deserialization, not all are read
pub struct QuickInfoBody {
    /// The symbol kind (e.g., "class", "method", "property").
    #[serde(default)]
    pub kind: String,

    /// Modifier flags (e.g., "private", "static").
    #[serde(default)]
    pub kind_modifiers: String,

    /// Start position.
    pub start: Location,

    /// End position.
    pub end: Location,

    /// Display string (full type signature).
    #[serde(default)]
    pub display_string: String,

    /// Documentation string.
    #[serde(default)]
    pub documentation: String,

    /// Tags (e.g., @deprecated).
    #[serde(default)]
    pub tags: Vec<serde_json::Value>,
}

/// Location in a file.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields are complete for deserialization
pub struct Location {
    pub line: u32,
    pub offset: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Event Types
// ─────────────────────────────────────────────────────────────────────────────

/// A TSServer event message.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Present for protocol completeness
pub struct Event {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub seq: i32,
    pub event: String,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Generic Message (for initial parsing)
// ─────────────────────────────────────────────────────────────────────────────

/// A generic TSServer message for initial type detection.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields are complete for deserialization
pub struct GenericMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub seq: i32,
    #[serde(default)]
    pub request_seq: Option<i32>,
    #[serde(default)]
    pub event: Option<String>,
}

impl GenericMessage {
    /// Check if this is a response message.
    pub fn is_response(&self) -> bool {
        self.msg_type == "response"
    }

    /// Check if this is an event message.
    pub fn is_event(&self) -> bool {
        self.msg_type == "event"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

/// TSServer command names.
pub mod commands {
    pub const OPEN: &str = "open";
    pub const CLOSE: &str = "close";
    pub const CONFIGURE: &str = "configure";
    pub const QUICKINFO: &str = "quickinfo";
    pub const EXIT: &str = "exit";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = Request::new(
            1,
            commands::OPEN,
            OpenArgs {
                file: "/path/to/file.ts".to_string(),
                file_content: None,
                script_kind_name: None,
                project_root_path: Some("/path/to".to_string()),
            },
        );

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"seq\":1"));
        assert!(json.contains("\"type\":\"request\""));
        assert!(json.contains("\"command\":\"open\""));
        assert!(json.contains("\"file\":\"/path/to/file.ts\""));
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{
            "type": "response",
            "seq": 0,
            "request_seq": 1,
            "command": "quickinfo",
            "success": true,
            "body": {
                "kind": "property",
                "kindModifiers": "",
                "start": {"line": 10, "offset": 5},
                "end": {"line": 10, "offset": 15},
                "displayString": "(property) MyClass.myProp: string"
            }
        }"#;

        let resp: Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.msg_type, "response");
        assert_eq!(resp.request_seq, 1);
        assert!(resp.success);
        assert!(resp.body.is_some());
    }

    #[test]
    fn test_event_deserialization() {
        let json = r#"{
            "type": "event",
            "seq": 5,
            "event": "projectLoadingFinish",
            "body": {}
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        assert_eq!(event.msg_type, "event");
        assert_eq!(event.event, "projectLoadingFinish");
    }

    #[test]
    fn test_generic_message_type_detection() {
        let response_json = r#"{"type": "response", "seq": 0, "request_seq": 1}"#;
        let event_json = r#"{"type": "event", "seq": 5, "event": "foo"}"#;

        let resp: GenericMessage = serde_json::from_str(response_json).unwrap();
        assert!(resp.is_response());
        assert!(!resp.is_event());

        let evt: GenericMessage = serde_json::from_str(event_json).unwrap();
        assert!(!evt.is_response());
        assert!(evt.is_event());
    }

    #[test]
    fn test_quickinfo_body_parsing() {
        let json = r#"{
            "kind": "method",
            "kindModifiers": "public",
            "start": {"line": 5, "offset": 10},
            "end": {"line": 5, "offset": 20},
            "displayString": "(method) Engine.start(): void",
            "documentation": "Starts the engine."
        }"#;

        let body: QuickInfoBody = serde_json::from_str(json).unwrap();
        assert_eq!(body.kind, "method");
        assert_eq!(body.display_string, "(method) Engine.start(): void");
        assert_eq!(body.start.line, 5);
        assert_eq!(body.start.offset, 10);
    }
}
