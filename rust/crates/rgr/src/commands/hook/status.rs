//! `rmap hook status` command.
//!
//! Show current hook state and configuration.

use std::process::ExitCode;

use serde::Serialize;

use crate::cli::paths;

use super::config::ConfigStatus;
use super::env::{HostContext, HostType};
use super::output::{output_result, HookResult, HookStatus, HumanReadable};

/// Status output data.
#[derive(Debug, Clone, Serialize)]
pub struct StatusOutput {
    pub config_dir: Option<String>,
    pub sessions_dir: Option<String>,
    pub logs_dir: Option<String>,
    pub configuration: ConfigStatus,
    pub integrations: Vec<IntegrationStatus>,
    pub detected_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_session: Option<ActiveSessionInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationStatus {
    pub host: String,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveSessionInfo {
    pub session_id: String,
    pub db_path: Option<String>,
    pub files_edited: u64,
}

impl HumanReadable for HookResult<StatusOutput> {
    fn print_human(&self) {
        println!("rmap hook status");
        println!();

        println!("Directories:");
        if let Some(ref dir) = self.data.config_dir {
            println!("  Config: {}", dir);
        } else {
            println!("  Config: (not determined)");
        }
        if let Some(ref dir) = self.data.sessions_dir {
            println!("  Sessions: {}", dir);
        }
        if let Some(ref dir) = self.data.logs_dir {
            println!("  Logs: {}", dir);
        }

        println!();
        println!("Configuration:");
        if let Some(ref path) = self.data.configuration.path {
            println!("  File: {}", path);
        }
        if self.data.configuration.exists {
            if self.data.configuration.valid {
                println!("  Status: loaded");
                if let Some(ref config) = self.data.configuration.config {
                    println!("  Settings:");
                    println!(
                        "    session.stale_threshold_minutes: {}",
                        config.session.stale_threshold_minutes
                    );
                    println!(
                        "    post_edit.batch_window_seconds: {}",
                        config.post_edit.batch_window_seconds
                    );
                    println!(
                        "    stop.required_validations: {:?}",
                        config.stop.required_validations
                    );
                    println!("    stop.enforcement: {}", config.stop.enforcement);
                }
            } else {
                println!("  Status: invalid");
                if let Some(ref err) = self.data.configuration.error {
                    println!("  Error: {}", err);
                }
            }
        } else {
            println!("  Status: not found (using defaults)");
            if let Some(ref config) = self.data.configuration.config {
                println!("  Defaults:");
                println!(
                    "    session.stale_threshold_minutes: {}",
                    config.session.stale_threshold_minutes
                );
                println!(
                    "    post_edit.batch_window_seconds: {}",
                    config.post_edit.batch_window_seconds
                );
            }
        }

        println!();
        println!("Integrations:");
        for integration in &self.data.integrations {
            let status = if integration.installed {
                "installed"
            } else {
                "not installed"
            };
            println!("  {}: {}", integration.host, status);
            if let Some(ref path) = integration.config_path {
                println!("    Config: {}", path);
            }
            if !integration.hooks.is_empty() {
                println!("    Hooks: {}", integration.hooks.join(", "));
            }
        }

        if let Some(ref host) = self.data.detected_host {
            println!();
            println!("Detected host: {}", host);
        }

        if let Some(ref session) = self.data.active_session {
            println!();
            println!("Active session:");
            println!("  ID: {}", session.session_id);
            if let Some(ref db) = session.db_path {
                println!("  Database: {}", db);
            }
            println!("  Files edited: {}", session.files_edited);
        }
    }
}

/// Run the status command.
pub fn run_hook_status(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_status_usage();
        return ExitCode::SUCCESS;
    }

    let json_output = args.iter().any(|a| a == "--json");

    let (result, status) = execute_status();
    output_result(&result, json_output);
    ExitCode::from(status.exit_code())
}

fn execute_status() -> (HookResult<StatusOutput>, HookStatus) {
    // Get platform directories
    let config_dir = paths::config_dir().map(|p| p.display().to_string());
    let sessions_dir = paths::sessions_dir().map(|p| p.display().to_string());
    let logs_dir = paths::logs_dir().map(|p| p.display().to_string());

    // Check configuration status
    let configuration = ConfigStatus::check();

    // Check for integrations
    let integrations = vec![
        check_claude_code_integration(),
        check_codex_integration(),
        check_cursor_integration(),
    ];

    // Detect current host from environment
    let host_ctx = HostContext::from_env();
    let detected_host = match host_ctx.host_type {
        HostType::ClaudeCode => Some("Claude Code".to_string()),
        HostType::Codex => Some("Codex".to_string()),
        HostType::Unknown => None,
    };

    let output = StatusOutput {
        config_dir,
        sessions_dir,
        logs_dir,
        configuration,
        integrations,
        detected_host,
        active_session: None, // Would need session ID to load
    };

    (HookResult::ok(output), HookStatus::Ok)
}

fn check_claude_code_integration() -> IntegrationStatus {
    let global_config = dirs::home_dir().map(|h| h.join(".claude").join("settings.json"));

    let (installed, config_path, hooks) = match global_config {
        Some(path) if path.exists() => {
            // Check if repo-graph hooks are present
            let hooks = check_hooks_in_config(&path);
            let installed = !hooks.is_empty();
            (installed, Some(path.display().to_string()), hooks)
        }
        Some(path) => (false, Some(path.display().to_string()), Vec::new()),
        None => (false, None, Vec::new()),
    };

    IntegrationStatus {
        host: "Claude Code".to_string(),
        installed,
        config_path,
        hooks,
    }
}

fn check_codex_integration() -> IntegrationStatus {
    let global_config = dirs::home_dir().map(|h| h.join(".codex").join("hooks.json"));

    let (installed, config_path, hooks) = match global_config {
        Some(path) if path.exists() => {
            let hooks = check_hooks_in_config(&path);
            let installed = !hooks.is_empty();
            (installed, Some(path.display().to_string()), hooks)
        }
        Some(path) => (false, Some(path.display().to_string()), Vec::new()),
        None => (false, None, Vec::new()),
    };

    IntegrationStatus {
        host: "Codex".to_string(),
        installed,
        config_path,
        hooks,
    }
}

fn check_cursor_integration() -> IntegrationStatus {
    let mcp_config = dirs::home_dir().map(|h| h.join(".cursor").join("mcp.json"));

    let (installed, config_path) = match mcp_config {
        Some(path) if path.exists() => {
            // Check if repo-graph MCP is configured
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let installed = content.contains("rmap") || content.contains("repo-graph");
            (installed, Some(path.display().to_string()))
        }
        Some(path) => (false, Some(path.display().to_string())),
        None => (false, None),
    };

    IntegrationStatus {
        host: "Cursor".to_string(),
        installed,
        config_path,
        hooks: Vec::new(), // Cursor doesn't use hooks
    }
}

/// Check if a config file contains repo-graph hooks.
fn check_hooks_in_config(path: &std::path::Path) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut hooks = Vec::new();

    if content.contains("rmap hook session-start") {
        hooks.push("session-start".to_string());
    }
    if content.contains("rmap hook post-edit") {
        hooks.push("post-edit".to_string());
    }
    if content.contains("rmap hook pre-compact") {
        hooks.push("pre-compact".to_string());
    }
    if content.contains("rmap hook stop") {
        hooks.push("stop".to_string());
    }
    if content.contains("rmap hook prompt-submit") {
        hooks.push("prompt-submit".to_string());
    }

    hooks
}

fn print_status_usage() {
    eprintln!("usage: rmap hook status [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --json              Output JSON instead of human-readable text");
    eprintln!("  --help              Show this help message");
}
