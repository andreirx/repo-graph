//! Stdin JSON transport for hook commands.
//!
//! HOOK-1A: Generic transport layer for hosts that pass hook context as JSON on stdin.
//!
//! Claude Code is the first consumer of this transport. The payload structure is
//! designed to be generic enough for future hosts that use stdin JSON.
//!
//! # Transport Model
//!
//! Some hosts (Claude Code) pass hook context as a JSON object on stdin rather than
//! environment variables. This module parses that JSON and normalizes it into the
//! same `HookContext` used by policy handlers.
//!
//! # Precedence
//!
//! When `--from-stdin` is used:
//! 1. Explicit --db/--repo arguments still take priority
//! 2. Stdin payload provides: cwd (repo path), session_id, event-specific data
//! 3. RMAP_* environment variables are checked as fallback
//! 4. Discovery is last resort

use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;

/// JSON payload from stdin transport.
///
/// This structure is intentionally generic. Field names align with Claude Code's
/// current schema but are not Claude-specific in the type definition.
///
/// Hosts provide different subsets of fields depending on the hook event.
#[derive(Debug, Clone, Deserialize)]
pub struct StdinPayload {
    /// Session identifier (optional, generated if missing).
    #[serde(default)]
    pub session_id: Option<String>,

    /// Current working directory / project root.
    /// This becomes the repo_path in HookContext.
    pub cwd: PathBuf,

    /// Hook event name (e.g., "SessionStart", "PostToolUse").
    /// Used for validation and logging, not for dispatch (command already known).
    #[serde(default)]
    #[allow(dead_code)] // Parsed for completeness, may be used for logging/validation
    pub hook_event_name: Option<String>,

    // --- Event-specific fields ---
    /// User prompt text (UserPromptSubmit event).
    #[serde(default)]
    pub prompt: Option<String>,

    /// Tool name that was invoked (PostToolUse event).
    #[serde(default)]
    pub tool_name: Option<String>,

    /// Tool input parameters (PostToolUse event).
    /// Structure varies by tool; kept as raw JSON for flexibility.
    #[serde(default)]
    pub tool_input: Option<serde_json::Value>,

    /// Tool output / result (PostToolUse event).
    /// May contain file paths or other structured data.
    #[serde(default)]
    pub tool_output: Option<String>,

    /// Compaction trigger type (PreCompact event): "auto" or "manual".
    #[serde(default)]
    #[allow(dead_code)] // Parsed for completeness, may be used for pre-compact policy
    pub compaction_type: Option<String>,

    // --- Extended fields for future use ---
    /// Transcript path (if host provides it).
    #[serde(default)]
    pub transcript_path: Option<PathBuf>,

    /// Permission mode (if host provides it).
    #[serde(default)]
    #[allow(dead_code)] // Parsed for completeness, may be used for enforcement policy
    pub permission_mode: Option<String>,
}

impl StdinPayload {
    /// Read and parse JSON from stdin.
    ///
    /// Returns an error if:
    /// - stdin cannot be read
    /// - stdin is empty
    /// - stdin contains invalid JSON
    pub fn from_stdin() -> Result<Self, String> {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| format!("failed to read stdin: {}", e))?;

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("no input on stdin (expected JSON payload)".to_string());
        }

        serde_json::from_str(trimmed).map_err(|e| format!("invalid JSON on stdin: {}", e))
    }

    /// Extract file paths from tool_input if present.
    ///
    /// PostToolUse events may include edited file paths in tool_input.
    /// This extracts the file_path field if present.
    pub fn extract_file_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Try to extract file_path from tool_input
        if let Some(ref input) = self.tool_input {
            if let Some(file_path) = input.get("file_path").and_then(|v| v.as_str()) {
                paths.push(PathBuf::from(file_path));
            }
        }

        // tool_output may also contain file paths (depends on host)
        // For now, don't parse tool_output as it's host-specific

        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_session_start_payload() {
        let json = r#"{
            "session_id": "test-123",
            "cwd": "/path/to/project",
            "hook_event_name": "SessionStart"
        }"#;

        let payload: StdinPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.session_id, Some("test-123".to_string()));
        assert_eq!(payload.cwd, PathBuf::from("/path/to/project"));
        assert_eq!(payload.hook_event_name, Some("SessionStart".to_string()));
        assert!(payload.prompt.is_none());
        assert!(payload.tool_name.is_none());
    }

    #[test]
    fn parse_user_prompt_submit_payload() {
        let json = r#"{
            "session_id": "test-456",
            "cwd": "/path/to/project",
            "hook_event_name": "UserPromptSubmit",
            "prompt": "Implement the login feature"
        }"#;

        let payload: StdinPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.prompt, Some("Implement the login feature".to_string()));
    }

    #[test]
    fn parse_post_tool_use_payload() {
        let json = r#"{
            "session_id": "test-789",
            "cwd": "/path/to/project",
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/path/to/file.rs",
                "old_string": "foo",
                "new_string": "bar"
            },
            "tool_output": "File edited successfully"
        }"#;

        let payload: StdinPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.tool_name, Some("Edit".to_string()));
        assert!(payload.tool_input.is_some());
        assert_eq!(payload.tool_output, Some("File edited successfully".to_string()));

        let files = payload.extract_file_paths();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], PathBuf::from("/path/to/file.rs"));
    }

    #[test]
    fn parse_pre_compact_payload() {
        let json = r#"{
            "cwd": "/path/to/project",
            "hook_event_name": "PreCompact",
            "compaction_type": "auto"
        }"#;

        let payload: StdinPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.compaction_type, Some("auto".to_string()));
        assert!(payload.session_id.is_none()); // Optional field
    }

    #[test]
    fn parse_minimal_payload() {
        // Only required field is cwd
        let json = r#"{"cwd": "/tmp"}"#;

        let payload: StdinPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.cwd, PathBuf::from("/tmp"));
        assert!(payload.session_id.is_none());
        assert!(payload.hook_event_name.is_none());
    }

    #[test]
    fn missing_cwd_fails() {
        let json = r#"{"session_id": "test"}"#;
        let result: Result<StdinPayload, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn empty_json_fails() {
        let json = r#"{}"#;
        let result: Result<StdinPayload, _> = serde_json::from_str(json);
        assert!(result.is_err()); // cwd is required
    }

    #[test]
    fn extract_file_paths_from_edit_tool() {
        let json = r#"{
            "cwd": "/project",
            "tool_input": {"file_path": "/project/src/main.rs"}
        }"#;

        let payload: StdinPayload = serde_json::from_str(json).unwrap();
        let files = payload.extract_file_paths();
        assert_eq!(files, vec![PathBuf::from("/project/src/main.rs")]);
    }

    #[test]
    fn extract_file_paths_no_tool_input() {
        let json = r#"{"cwd": "/project"}"#;

        let payload: StdinPayload = serde_json::from_str(json).unwrap();
        let files = payload.extract_file_paths();
        assert!(files.is_empty());
    }
}
