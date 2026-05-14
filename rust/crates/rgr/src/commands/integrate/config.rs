//! Pure configuration transformation logic.
//!
//! This module handles:
//! - Parse existing JSON configuration
//! - Detect repo-graph-managed hooks
//! - Compute merge/update/remove plan
//! - Preserve non-repo-graph hooks
//! - Serialize result
//!
//! This is intentionally pure transformation code with no I/O.
//! All file operations happen in the caller (claude_code.rs).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

/// A single hook handler entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookHandler {
    #[serde(rename = "type")]
    pub handler_type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

/// A matcher group containing hooks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MatcherGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub hooks: Vec<HookHandler>,
}

/// Result of analyzing existing configuration.
#[derive(Debug, Clone)]
pub struct ConfigAnalysis {
    /// Whether the config file exists
    pub file_exists: bool,
    /// Whether the JSON is valid
    pub json_valid: bool,
    /// Parse error if JSON is invalid
    pub parse_error: Option<String>,
    /// Events that have repo-graph hooks installed
    pub repo_graph_events: Vec<String>,
    /// Events that have non-repo-graph hooks
    pub other_events: Vec<String>,
    /// Detected profile (minimal, full, or custom)
    pub profile: InstalledProfile,
}

/// Detected installation profile.
#[derive(Debug, Clone, PartialEq)]
pub enum InstalledProfile {
    /// No repo-graph hooks detected
    NotInstalled,
    /// Only SessionStart + Stop
    Minimal,
    /// All 5 hooks
    Full,
    /// Some subset that doesn't match minimal or full
    Custom(Vec<String>),
}

impl std::fmt::Display for InstalledProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstalledProfile::NotInstalled => write!(f, "not installed"),
            InstalledProfile::Minimal => write!(f, "minimal"),
            InstalledProfile::Full => write!(f, "full"),
            InstalledProfile::Custom(events) => write!(f, "custom ({})", events.join(", ")),
        }
    }
}

/// Planned change for a single event.
#[derive(Debug, Clone)]
pub enum EventChange {
    /// Add new repo-graph hooks to this event
    Add,
    /// Update existing repo-graph hooks
    Update,
    /// Remove repo-graph hooks from this event
    Remove,
    /// No change needed
    NoChange,
}

/// A merge plan describing what changes will be made.
#[derive(Debug, Clone)]
pub struct MergePlan {
    /// Changes per event
    pub changes: Vec<(String, EventChange)>,
    /// Whether any repo-graph hooks already exist
    pub existing_hooks_found: bool,
    /// Human-readable summary
    pub summary: String,
}

/// Check if a command string is a repo-graph hook.
pub fn is_repo_graph_hook(command: &str) -> bool {
    command.contains("rmap hook")
}

/// Hook definitions for a specific host.
///
/// Each host (Claude Code, Codex) provides its own hook definitions.
/// This trait allows config.rs to remain generic.
pub trait HookDefinitions {
    /// Get minimal profile hooks.
    fn minimal() -> Vec<(&'static str, MatcherGroup)>;

    /// Get full profile hooks.
    fn full() -> Vec<(&'static str, MatcherGroup)>;

    /// Get minimal profile event names.
    fn minimal_events() -> Vec<&'static str>;

    /// Get full profile event names.
    fn full_events() -> Vec<&'static str>;
}

/// Claude Code hook definitions.
///
/// - Minimal: SessionStart + Stop
/// - Full: SessionStart + UserPromptSubmit + PostToolUse + PreCompact + Stop
pub struct ClaudeCodeHooks;

impl HookDefinitions for ClaudeCodeHooks {
    fn minimal() -> Vec<(&'static str, MatcherGroup)> {
        vec![
            (
                "SessionStart",
                MatcherGroup {
                    matcher: None,
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook session-start --from-stdin".to_string(),
                        timeout: Some(30),
                    }],
                },
            ),
            (
                "Stop",
                MatcherGroup {
                    matcher: None,
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook stop --from-stdin".to_string(),
                        timeout: Some(30),
                    }],
                },
            ),
        ]
    }

    fn full() -> Vec<(&'static str, MatcherGroup)> {
        vec![
            (
                "SessionStart",
                MatcherGroup {
                    matcher: None,
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook session-start --from-stdin".to_string(),
                        timeout: Some(30),
                    }],
                },
            ),
            (
                "UserPromptSubmit",
                MatcherGroup {
                    matcher: None,
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook prompt-submit --from-stdin".to_string(),
                        timeout: Some(10),
                    }],
                },
            ),
            (
                "PostToolUse",
                MatcherGroup {
                    matcher: Some("Edit|Write".to_string()),
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook post-edit --from-stdin".to_string(),
                        timeout: Some(60),
                    }],
                },
            ),
            (
                "PreCompact",
                MatcherGroup {
                    matcher: Some("auto|manual".to_string()),
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook pre-compact --from-stdin".to_string(),
                        timeout: Some(10),
                    }],
                },
            ),
            (
                "Stop",
                MatcherGroup {
                    matcher: None,
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook stop --from-stdin".to_string(),
                        timeout: Some(30),
                    }],
                },
            ),
        ]
    }

    fn minimal_events() -> Vec<&'static str> {
        vec!["SessionStart", "Stop"]
    }

    fn full_events() -> Vec<&'static str> {
        vec![
            "SessionStart",
            "UserPromptSubmit",
            "PostToolUse",
            "PreCompact",
            "Stop",
        ]
    }
}

/// Codex CLI hook definitions.
///
/// Verified May 2026 from https://developers.openai.com/codex/hooks
///
/// - Minimal: SessionStart (startup|resume) + Stop
/// - Full: SessionStart + UserPromptSubmit + PostToolUse + Stop
/// - No PreCompact (Codex doesn't support this event)
/// - PostToolUse includes apply_patch tool name
pub struct CodexHooks;

impl HookDefinitions for CodexHooks {
    fn minimal() -> Vec<(&'static str, MatcherGroup)> {
        vec![
            (
                "SessionStart",
                MatcherGroup {
                    // Skip "clear" events - only orient on startup/resume
                    matcher: Some("startup|resume".to_string()),
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook session-start --from-stdin".to_string(),
                        timeout: Some(30),
                    }],
                },
            ),
            (
                "Stop",
                MatcherGroup {
                    matcher: None,
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook stop --from-stdin".to_string(),
                        timeout: Some(30),
                    }],
                },
            ),
        ]
    }

    fn full() -> Vec<(&'static str, MatcherGroup)> {
        vec![
            (
                "SessionStart",
                MatcherGroup {
                    matcher: Some("startup|resume".to_string()),
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook session-start --from-stdin".to_string(),
                        timeout: Some(30),
                    }],
                },
            ),
            (
                "UserPromptSubmit",
                MatcherGroup {
                    matcher: None,
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook prompt-submit --from-stdin".to_string(),
                        timeout: Some(10),
                    }],
                },
            ),
            (
                "PostToolUse",
                MatcherGroup {
                    // Codex uses apply_patch in addition to Edit|Write
                    matcher: Some("Edit|Write|apply_patch".to_string()),
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook post-edit --from-stdin".to_string(),
                        timeout: Some(60),
                    }],
                },
            ),
            (
                "Stop",
                MatcherGroup {
                    matcher: None,
                    hooks: vec![HookHandler {
                        handler_type: "command".to_string(),
                        command: "rmap hook stop --from-stdin".to_string(),
                        timeout: Some(30),
                    }],
                },
            ),
        ]
    }

    fn minimal_events() -> Vec<&'static str> {
        vec!["SessionStart", "Stop"]
    }

    fn full_events() -> Vec<&'static str> {
        vec!["SessionStart", "UserPromptSubmit", "PostToolUse", "Stop"]
    }
}

/// Legacy alias for backward compatibility.
pub type RepoGraphHooks = ClaudeCodeHooks;

/// Parse a Claude Code settings.json file content.
pub fn parse_settings(content: &str) -> Result<Value, String> {
    serde_json::from_str(content).map_err(|e| format!("invalid JSON: {}", e))
}

/// Analyze existing configuration to determine current state.
pub fn analyze_config(content: Option<&str>) -> ConfigAnalysis {
    let Some(content) = content else {
        return ConfigAnalysis {
            file_exists: false,
            json_valid: false,
            parse_error: None,
            repo_graph_events: Vec::new(),
            other_events: Vec::new(),
            profile: InstalledProfile::NotInstalled,
        };
    };

    let parsed = match parse_settings(content) {
        Ok(v) => v,
        Err(e) => {
            return ConfigAnalysis {
                file_exists: true,
                json_valid: false,
                parse_error: Some(e),
                repo_graph_events: Vec::new(),
                other_events: Vec::new(),
                profile: InstalledProfile::NotInstalled,
            };
        }
    };

    let hooks = parsed.get("hooks").and_then(|h| h.as_object());

    let Some(hooks) = hooks else {
        return ConfigAnalysis {
            file_exists: true,
            json_valid: true,
            parse_error: None,
            repo_graph_events: Vec::new(),
            other_events: Vec::new(),
            profile: InstalledProfile::NotInstalled,
        };
    };

    let mut repo_graph_events = Vec::new();
    let mut other_events = Vec::new();

    for (event_name, event_value) in hooks {
        let has_repo_graph = event_has_repo_graph_hooks(event_value);
        let has_other = event_has_non_repo_graph_hooks(event_value);

        if has_repo_graph {
            repo_graph_events.push(event_name.clone());
        }
        if has_other {
            other_events.push(event_name.clone());
        }
    }

    let profile = determine_profile(&repo_graph_events);

    ConfigAnalysis {
        file_exists: true,
        json_valid: true,
        parse_error: None,
        repo_graph_events,
        other_events,
        profile,
    }
}

/// Check if an event value contains repo-graph hooks.
fn event_has_repo_graph_hooks(event_value: &Value) -> bool {
    let Some(groups) = event_value.as_array() else {
        return false;
    };

    for group in groups {
        if let Some(hooks) = group.get("hooks").and_then(|h| h.as_array()) {
            for hook in hooks {
                if let Some(cmd) = hook.get("command").and_then(|c| c.as_str()) {
                    if is_repo_graph_hook(cmd) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Check if an event value contains non-repo-graph hooks.
fn event_has_non_repo_graph_hooks(event_value: &Value) -> bool {
    let Some(groups) = event_value.as_array() else {
        return false;
    };

    for group in groups {
        if let Some(hooks) = group.get("hooks").and_then(|h| h.as_array()) {
            for hook in hooks {
                if let Some(cmd) = hook.get("command").and_then(|c| c.as_str()) {
                    if !is_repo_graph_hook(cmd) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Determine the installed profile from the list of repo-graph events.
fn determine_profile(repo_graph_events: &[String]) -> InstalledProfile {
    if repo_graph_events.is_empty() {
        return InstalledProfile::NotInstalled;
    }

    let minimal_events: Vec<&str> = RepoGraphHooks::minimal_events();
    let full_events: Vec<&str> = RepoGraphHooks::full_events();

    let mut sorted_events: Vec<&str> = repo_graph_events.iter().map(|s| s.as_str()).collect();
    sorted_events.sort();

    let mut sorted_minimal = minimal_events.clone();
    sorted_minimal.sort();

    let mut sorted_full = full_events.clone();
    sorted_full.sort();

    if sorted_events == sorted_minimal {
        InstalledProfile::Minimal
    } else if sorted_events == sorted_full {
        InstalledProfile::Full
    } else {
        InstalledProfile::Custom(repo_graph_events.to_vec())
    }
}

/// Plan the merge operation for installing hooks (generic version).
pub fn plan_install_with<H: HookDefinitions>(
    existing_content: Option<&str>,
    full_profile: bool,
    force: bool,
) -> Result<MergePlan, String> {
    let analysis = analyze_config(existing_content);

    if !analysis.json_valid && analysis.file_exists {
        return Err(analysis
            .parse_error
            .unwrap_or_else(|| "invalid JSON".to_string()));
    }

    let target_hooks = if full_profile {
        H::full()
    } else {
        H::minimal()
    };

    let existing_hooks_found = !analysis.repo_graph_events.is_empty();

    if existing_hooks_found && !force {
        return Err(format!(
            "repo-graph hooks already installed ({}). Use --force to overwrite.",
            analysis.profile
        ));
    }

    let mut changes = Vec::new();
    for (event_name, _) in &target_hooks {
        let change = if analysis.repo_graph_events.contains(&event_name.to_string()) {
            if force {
                EventChange::Update
            } else {
                EventChange::NoChange
            }
        } else {
            EventChange::Add
        };
        changes.push((event_name.to_string(), change));
    }

    let profile_name = if full_profile { "full" } else { "minimal" };
    let summary = if existing_hooks_found {
        format!(
            "Update to {} profile ({} events)",
            profile_name,
            changes.len()
        )
    } else {
        format!(
            "Install {} profile ({} events)",
            profile_name,
            changes.len()
        )
    };

    Ok(MergePlan {
        changes,
        existing_hooks_found,
        summary,
    })
}

/// Plan the merge operation for installing hooks (Claude Code default).
pub fn plan_install(
    existing_content: Option<&str>,
    full_profile: bool,
    force: bool,
) -> Result<MergePlan, String> {
    plan_install_with::<ClaudeCodeHooks>(existing_content, full_profile, force)
}

/// Plan the remove operation.
pub fn plan_remove(existing_content: Option<&str>) -> Result<MergePlan, String> {
    let analysis = analyze_config(existing_content);

    if !analysis.json_valid && analysis.file_exists {
        return Err(analysis
            .parse_error
            .unwrap_or_else(|| "invalid JSON".to_string()));
    }

    if analysis.repo_graph_events.is_empty() {
        return Ok(MergePlan {
            changes: Vec::new(),
            existing_hooks_found: false,
            summary: "No repo-graph hooks to remove".to_string(),
        });
    }

    let changes: Vec<(String, EventChange)> = analysis
        .repo_graph_events
        .iter()
        .map(|e| (e.clone(), EventChange::Remove))
        .collect();

    let summary = format!("Remove {} repo-graph events", changes.len());

    Ok(MergePlan {
        changes,
        existing_hooks_found: true,
        summary,
    })
}

/// Apply the install plan to produce new configuration (generic version).
pub fn apply_install_with<H: HookDefinitions>(
    existing_content: Option<&str>,
    full_profile: bool,
) -> Result<String, String> {
    let mut root: Value = if let Some(content) = existing_content {
        parse_settings(content)?
    } else {
        Value::Object(Map::new())
    };

    let root_obj = root.as_object_mut().ok_or("root must be an object")?;

    // Ensure hooks object exists
    if !root_obj.contains_key("hooks") {
        root_obj.insert("hooks".to_string(), Value::Object(Map::new()));
    }

    let hooks = root_obj
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .ok_or("hooks must be an object")?;

    let target_hooks = if full_profile {
        H::full()
    } else {
        H::minimal()
    };

    for (event_name, matcher_group) in target_hooks {
        let group_value = serde_json::to_value(&matcher_group)
            .map_err(|e| format!("failed to serialize hook: {}", e))?;

        if let Some(existing_event) = hooks.get_mut(event_name) {
            // Remove existing repo-graph hooks, keep others
            if let Some(groups) = existing_event.as_array_mut() {
                groups.retain(|g| !group_has_repo_graph_hooks(g));
                // Prepend new repo-graph hook
                groups.insert(0, group_value);
            }
        } else {
            // Create new event with just the repo-graph hook
            hooks.insert(event_name.to_string(), Value::Array(vec![group_value]));
        }
    }

    serde_json::to_string_pretty(&root).map_err(|e| format!("failed to serialize: {}", e))
}

/// Apply the install plan to produce new configuration (Claude Code default).
pub fn apply_install(existing_content: Option<&str>, full_profile: bool) -> Result<String, String> {
    apply_install_with::<ClaudeCodeHooks>(existing_content, full_profile)
}

/// Apply the remove plan to produce new configuration.
pub fn apply_remove(existing_content: &str) -> Result<String, String> {
    let mut root: Value = parse_settings(existing_content)?;

    let root_obj = root.as_object_mut().ok_or("root must be an object")?;

    let Some(hooks) = root_obj.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        // No hooks section, nothing to remove
        return serde_json::to_string_pretty(&root)
            .map_err(|e| format!("failed to serialize: {}", e));
    };

    // Remove repo-graph hooks from each event
    let mut empty_events = Vec::new();
    for (event_name, event_value) in hooks.iter_mut() {
        if let Some(groups) = event_value.as_array_mut() {
            groups.retain(|g| !group_has_repo_graph_hooks(g));
            if groups.is_empty() {
                empty_events.push(event_name.clone());
            }
        }
    }

    // Remove events that are now empty
    for event_name in empty_events {
        hooks.remove(&event_name);
    }

    // Remove hooks section if empty
    if hooks.is_empty() {
        root_obj.remove("hooks");
    }

    serde_json::to_string_pretty(&root).map_err(|e| format!("failed to serialize: {}", e))
}

/// Check if a matcher group contains repo-graph hooks.
fn group_has_repo_graph_hooks(group: &Value) -> bool {
    if let Some(hooks) = group.get("hooks").and_then(|h| h.as_array()) {
        for hook in hooks {
            if let Some(cmd) = hook.get("command").and_then(|c| c.as_str()) {
                if is_repo_graph_hook(cmd) {
                    return true;
                }
            }
        }
    }
    false
}

/// Create a backup path for a config file.
pub fn backup_path(config_path: &Path) -> std::path::PathBuf {
    let mut backup = config_path.to_path_buf();
    let file_name = backup
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "settings.json".to_string());
    backup.set_file_name(format!("{}.rmap-backup", file_name));
    backup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_empty_config() {
        let analysis = analyze_config(None);
        assert!(!analysis.file_exists);
        assert_eq!(analysis.profile, InstalledProfile::NotInstalled);
    }

    #[test]
    fn test_analyze_empty_json() {
        let analysis = analyze_config(Some("{}"));
        assert!(analysis.file_exists);
        assert!(analysis.json_valid);
        assert_eq!(analysis.profile, InstalledProfile::NotInstalled);
    }

    #[test]
    fn test_analyze_invalid_json() {
        let analysis = analyze_config(Some("{not valid json"));
        assert!(analysis.file_exists);
        assert!(!analysis.json_valid);
        assert!(analysis.parse_error.is_some());
    }

    #[test]
    fn test_analyze_minimal_profile() {
        let config = r#"{
            "hooks": {
                "SessionStart": [{"hooks": [{"type": "command", "command": "rmap hook session-start --from-stdin"}]}],
                "Stop": [{"hooks": [{"type": "command", "command": "rmap hook stop --from-stdin"}]}]
            }
        }"#;
        let analysis = analyze_config(Some(config));
        assert!(analysis.json_valid);
        assert_eq!(analysis.profile, InstalledProfile::Minimal);
        assert_eq!(analysis.repo_graph_events.len(), 2);
    }

    #[test]
    fn test_analyze_full_profile() {
        let config = r#"{
            "hooks": {
                "SessionStart": [{"hooks": [{"type": "command", "command": "rmap hook session-start --from-stdin"}]}],
                "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "rmap hook prompt-submit --from-stdin"}]}],
                "PostToolUse": [{"hooks": [{"type": "command", "command": "rmap hook post-edit --from-stdin"}]}],
                "PreCompact": [{"hooks": [{"type": "command", "command": "rmap hook pre-compact --from-stdin"}]}],
                "Stop": [{"hooks": [{"type": "command", "command": "rmap hook stop --from-stdin"}]}]
            }
        }"#;
        let analysis = analyze_config(Some(config));
        assert_eq!(analysis.profile, InstalledProfile::Full);
    }

    #[test]
    fn test_analyze_mixed_hooks() {
        let config = r#"{
            "hooks": {
                "SessionStart": [
                    {"hooks": [{"type": "command", "command": "rmap hook session-start --from-stdin"}]},
                    {"hooks": [{"type": "command", "command": "my-custom-hook.sh"}]}
                ]
            }
        }"#;
        let analysis = analyze_config(Some(config));
        assert!(analysis
            .repo_graph_events
            .contains(&"SessionStart".to_string()));
        assert!(analysis.other_events.contains(&"SessionStart".to_string()));
    }

    #[test]
    fn test_plan_install_fresh() {
        let plan = plan_install(None, false, false).unwrap();
        assert!(!plan.existing_hooks_found);
        assert_eq!(plan.changes.len(), 2); // minimal profile
    }

    #[test]
    fn test_plan_install_full() {
        let plan = plan_install(None, true, false).unwrap();
        assert_eq!(plan.changes.len(), 5); // full profile
    }

    #[test]
    fn test_plan_install_existing_requires_force() {
        let config = r#"{"hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "rmap hook session-start --from-stdin"}]}]}}"#;
        let result = plan_install(Some(config), false, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("--force"));
    }

    #[test]
    fn test_plan_install_with_force() {
        let config = r#"{"hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "rmap hook session-start --from-stdin"}]}]}}"#;
        let plan = plan_install(Some(config), false, true).unwrap();
        assert!(plan.existing_hooks_found);
    }

    #[test]
    fn test_apply_install_fresh() {
        let result = apply_install(None, false).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("hooks").is_some());
        let hooks = parsed.get("hooks").unwrap().as_object().unwrap();
        assert!(hooks.contains_key("SessionStart"));
        assert!(hooks.contains_key("Stop"));
        assert!(!hooks.contains_key("UserPromptSubmit"));
    }

    #[test]
    fn test_apply_install_full() {
        let result = apply_install(None, true).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let hooks = parsed.get("hooks").unwrap().as_object().unwrap();
        assert!(hooks.contains_key("SessionStart"));
        assert!(hooks.contains_key("UserPromptSubmit"));
        assert!(hooks.contains_key("PostToolUse"));
        assert!(hooks.contains_key("PreCompact"));
        assert!(hooks.contains_key("Stop"));
    }

    #[test]
    fn test_apply_install_preserves_other_hooks() {
        let existing = r#"{
            "hooks": {
                "SessionStart": [{"hooks": [{"type": "command", "command": "my-hook.sh"}]}]
            }
        }"#;
        let result = apply_install(Some(existing), false).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let session_start = parsed
            .get("hooks")
            .unwrap()
            .get("SessionStart")
            .unwrap()
            .as_array()
            .unwrap();
        // Should have both repo-graph hook and custom hook
        assert_eq!(session_start.len(), 2);
        // repo-graph hook should be first
        let first_cmd = session_start[0].get("hooks").unwrap().as_array().unwrap()[0]
            .get("command")
            .unwrap()
            .as_str()
            .unwrap();
        assert!(first_cmd.contains("rmap hook"));
    }

    #[test]
    fn test_apply_remove() {
        let existing = r#"{
            "hooks": {
                "SessionStart": [
                    {"hooks": [{"type": "command", "command": "rmap hook session-start --from-stdin"}]},
                    {"hooks": [{"type": "command", "command": "my-hook.sh"}]}
                ],
                "Stop": [{"hooks": [{"type": "command", "command": "rmap hook stop --from-stdin"}]}]
            }
        }"#;
        let result = apply_remove(existing).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let hooks = parsed.get("hooks").unwrap().as_object().unwrap();

        // SessionStart should remain with only custom hook
        assert!(hooks.contains_key("SessionStart"));
        let session_start = hooks.get("SessionStart").unwrap().as_array().unwrap();
        assert_eq!(session_start.len(), 1);

        // Stop should be removed entirely (was only repo-graph hook)
        assert!(!hooks.contains_key("Stop"));
    }

    #[test]
    fn test_apply_remove_cleans_empty_hooks() {
        let existing = r#"{
            "hooks": {
                "SessionStart": [{"hooks": [{"type": "command", "command": "rmap hook session-start --from-stdin"}]}]
            },
            "other_setting": true
        }"#;
        let result = apply_remove(existing).unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        // hooks section should be removed
        assert!(parsed.get("hooks").is_none());
        // other settings preserved
        assert_eq!(parsed.get("other_setting").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn test_backup_path() {
        let config = std::path::Path::new("/home/user/.claude/settings.json");
        let backup = backup_path(config);
        assert_eq!(
            backup.to_string_lossy(),
            "/home/user/.claude/settings.json.rmap-backup"
        );
    }

    #[test]
    fn test_is_repo_graph_hook() {
        assert!(is_repo_graph_hook("rmap hook session-start --from-stdin"));
        assert!(is_repo_graph_hook("/usr/local/bin/rmap hook stop"));
        assert!(!is_repo_graph_hook("my-custom-hook.sh"));
        assert!(!is_repo_graph_hook("echo hello"));
    }
}
