//! Claude Code integration policy.
//!
//! This module decides:
//! - Target config path (global vs project)
//! - Install/remove/status behavior
//! - Minimal vs full hook set
//! - Dry-run reporting
//! - Force semantics
//!
//! It delegates JSON manipulation to config.rs.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use super::config::{
    self, analyze_config, apply_install, apply_remove, backup_path, plan_install, plan_remove,
    ConfigAnalysis, RepoGraphHooks,
};
use super::manifest::{self, get_integration, record_integration, remove_integration_record};

/// Host identifier for manifest recording.
pub const HOST_ID: &str = "claude-code";

/// Scope for global installation.
pub const SCOPE_GLOBAL: &str = "global";

/// Scope for project installation.
pub const SCOPE_PROJECT: &str = "project";

/// Get the global Claude Code settings path.
pub fn global_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude/settings.json"))
}

/// Get the project Claude Code settings path.
pub fn project_config_path() -> PathBuf {
    PathBuf::from(".claude/settings.json")
}

/// Options for the install command.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub global: bool,
    pub project: bool,
    pub full: bool,
    pub dry_run: bool,
    pub force: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            global: true, // default to global
            project: false,
            full: false,  // default to minimal
            dry_run: false,
            force: false,
        }
    }
}

/// Options for the remove command.
#[derive(Debug, Clone)]
pub struct RemoveOptions {
    pub global: bool,
    pub project: bool,
}

impl Default for RemoveOptions {
    fn default() -> Self {
        Self {
            global: true,
            project: false,
        }
    }
}

/// Options for the status command.
#[derive(Debug, Clone)]
pub struct StatusOptions {
    pub global: bool,
    pub project: bool,
    pub json: bool,
}

impl Default for StatusOptions {
    fn default() -> Self {
        Self {
            global: true,
            project: false,
            json: false,
        }
    }
}

/// Status output for JSON mode.
#[derive(Debug, Clone, Serialize)]
pub struct StatusOutput {
    pub scope: String,
    pub config_path: String,
    pub status: String,
    pub profile: Option<String>,
    pub hooks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execute the install command.
pub fn execute_install(opts: &InstallOptions) -> ExitCode {
    let (config_path, scope) = resolve_config_path(opts.global, opts.project);

    let Some(config_path) = config_path else {
        eprintln!("error: could not determine Claude Code config path");
        return ExitCode::from(2);
    };

    if opts.dry_run {
        return execute_install_dry_run(&config_path, scope, opts);
    }

    // Read existing content
    let existing_content = if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("error: failed to read {}: {}", config_path.display(), e);
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };

    // Plan the install
    let plan = match plan_install(existing_content.as_deref(), opts.full, opts.force) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    // Create backup if file exists
    let backup = if existing_content.is_some() {
        let backup_file = backup_path(&config_path);
        if let Err(e) = std::fs::copy(&config_path, &backup_file) {
            eprintln!(
                "error: failed to create backup at {}: {}",
                backup_file.display(),
                e
            );
            return ExitCode::from(2);
        }
        println!("  Backup: {}", backup_file.display());
        Some(backup_file)
    } else {
        None
    };

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "error: failed to create directory {}: {}",
                    parent.display(),
                    e
                );
                return ExitCode::from(2);
            }
        }
    }

    // Apply the install
    let new_content = match apply_install(existing_content.as_deref(), opts.full) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to generate config: {}", e);
            return ExitCode::from(2);
        }
    };

    // Write the new config
    if let Err(e) = std::fs::write(&config_path, &new_content) {
        eprintln!("error: failed to write {}: {}", config_path.display(), e);
        return ExitCode::from(2);
    }

    // Record in manifest
    let profile = if opts.full { "full" } else { "minimal" };
    let hooks_installed: Vec<String> = if opts.full {
        RepoGraphHooks::full_events()
            .into_iter()
            .map(String::from)
            .collect()
    } else {
        RepoGraphHooks::minimal_events()
            .into_iter()
            .map(String::from)
            .collect()
    };

    if let Err(e) = record_integration(
        HOST_ID,
        scope,
        &config_path,
        backup.as_deref(),
        hooks_installed.clone(),
        profile,
    ) {
        eprintln!("warning: failed to record in manifest: {}", e);
    }

    // Report success
    println!("Claude Code integration installed ({} profile)", profile);
    println!("  Config: {}", config_path.display());
    println!("  Hooks: {}", hooks_installed.join(", "));
    if plan.existing_hooks_found {
        println!("  Note: existing repo-graph hooks were updated");
    }

    ExitCode::SUCCESS
}

/// Execute install in dry-run mode.
fn execute_install_dry_run(config_path: &Path, scope: &str, opts: &InstallOptions) -> ExitCode {
    let existing_content = if config_path.exists() {
        std::fs::read_to_string(config_path).ok()
    } else {
        None
    };

    let plan = match plan_install(existing_content.as_deref(), opts.full, opts.force) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    let profile = if opts.full { "full" } else { "minimal" };
    println!("Dry run: Claude Code integration ({} profile)", profile);
    println!("  Scope: {}", scope);
    println!("  Config: {}", config_path.display());
    println!();
    println!("  Plan: {}", plan.summary);
    for (event, change) in &plan.changes {
        let action = match change {
            config::EventChange::Add => "add",
            config::EventChange::Update => "update",
            config::EventChange::Remove => "remove",
            config::EventChange::NoChange => "no change",
        };
        println!("    {}: {}", event, action);
    }

    if existing_content.is_some() {
        println!();
        println!("  Would create backup: {}", backup_path(config_path).display());
    }

    println!();
    println!("No changes made (dry run).");

    ExitCode::SUCCESS
}

/// Execute the remove command.
pub fn execute_remove(opts: &RemoveOptions) -> ExitCode {
    let (config_path, scope) = resolve_config_path(opts.global, opts.project);

    let Some(config_path) = config_path else {
        eprintln!("error: could not determine Claude Code config path");
        return ExitCode::from(2);
    };

    if !config_path.exists() {
        println!("Claude Code config not found: {}", config_path.display());
        println!("No repo-graph hooks to remove.");
        return ExitCode::SUCCESS;
    }

    // Read existing content
    let existing_content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to read {}: {}", config_path.display(), e);
            return ExitCode::from(2);
        }
    };

    // Plan the removal
    let plan = match plan_remove(Some(&existing_content)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    if !plan.existing_hooks_found {
        println!("No repo-graph hooks found in {}", config_path.display());
        return ExitCode::SUCCESS;
    }

    // Apply the removal
    let new_content = match apply_remove(&existing_content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to generate config: {}", e);
            return ExitCode::from(2);
        }
    };

    // Write the new config
    if let Err(e) = std::fs::write(&config_path, &new_content) {
        eprintln!("error: failed to write {}: {}", config_path.display(), e);
        return ExitCode::from(2);
    }

    // Remove from manifest
    if let Err(e) = remove_integration_record(HOST_ID, scope) {
        eprintln!("warning: failed to update manifest: {}", e);
    }

    // Report success
    println!("Claude Code integration removed");
    println!("  Config: {}", config_path.display());
    println!(
        "  Removed: {} events",
        plan.changes
            .iter()
            .filter(|(_, c)| matches!(c, config::EventChange::Remove))
            .count()
    );

    ExitCode::SUCCESS
}

/// Execute the status command.
pub fn execute_status(opts: &StatusOptions) -> ExitCode {
    let (config_path, scope) = resolve_config_path(opts.global, opts.project);

    let Some(config_path) = config_path else {
        if opts.json {
            let output = StatusOutput {
                scope: scope.to_string(),
                config_path: "unknown".to_string(),
                status: "error".to_string(),
                profile: None,
                hooks: Vec::new(),
                backup_path: None,
                installed_at: None,
                error: Some("could not determine config path".to_string()),
            };
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        } else {
            eprintln!("error: could not determine Claude Code config path");
        }
        return ExitCode::from(2);
    };

    let existing_content = if config_path.exists() {
        std::fs::read_to_string(&config_path).ok()
    } else {
        None
    };

    let analysis = analyze_config(existing_content.as_deref());

    // Get manifest record for additional info
    let manifest_record = get_integration(HOST_ID, scope).ok().flatten();

    if opts.json {
        let output = build_status_output(&config_path, scope, &analysis, manifest_record.as_ref());
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        print_status_human(&config_path, scope, &analysis, manifest_record.as_ref());
    }

    ExitCode::SUCCESS
}

/// Build JSON status output.
fn build_status_output(
    config_path: &Path,
    scope: &str,
    analysis: &ConfigAnalysis,
    manifest_record: Option<&manifest::HostIntegration>,
) -> StatusOutput {
    let status = if !analysis.file_exists {
        "not found".to_string()
    } else if !analysis.json_valid {
        "invalid json".to_string()
    } else if analysis.repo_graph_events.is_empty() {
        "not installed".to_string()
    } else {
        "installed".to_string()
    };

    let profile = if analysis.repo_graph_events.is_empty() {
        None
    } else {
        Some(analysis.profile.to_string())
    };

    let backup = manifest_record.and_then(|r| r.backup_path.clone());
    let installed_at = manifest_record.map(|r| r.installed_at.to_rfc3339());

    StatusOutput {
        scope: scope.to_string(),
        config_path: config_path.display().to_string(),
        status,
        profile,
        hooks: analysis.repo_graph_events.clone(),
        backup_path: backup,
        installed_at,
        error: analysis.parse_error.clone(),
    }
}

/// Print human-readable status.
fn print_status_human(
    config_path: &Path,
    scope: &str,
    analysis: &ConfigAnalysis,
    manifest_record: Option<&manifest::HostIntegration>,
) {
    println!("Claude Code Integration Status");
    println!();
    println!(
        "{} ({}):",
        if scope == SCOPE_GLOBAL {
            "Global"
        } else {
            "Project"
        },
        config_path.display()
    );

    if !analysis.file_exists {
        println!("  Status: config file not found");
        return;
    }

    if !analysis.json_valid {
        println!("  Status: invalid JSON");
        if let Some(ref err) = analysis.parse_error {
            println!("  Error: {}", err);
        }
        return;
    }

    if analysis.repo_graph_events.is_empty() {
        println!("  Status: not installed");
        return;
    }

    println!("  Status: installed ({})", analysis.profile);
    println!("  Hooks:");
    for event in &analysis.repo_graph_events {
        println!("    {}", event);
    }

    if let Some(record) = manifest_record {
        if let Some(ref backup) = record.backup_path {
            println!("  Backup: {}", backup);
        }
        println!("  Installed: {}", record.installed_at.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    if !analysis.other_events.is_empty() {
        println!();
        println!("  Other hooks present: {}", analysis.other_events.join(", "));
    }
}

/// Resolve config path based on global/project flags.
fn resolve_config_path(_global: bool, project: bool) -> (Option<PathBuf>, &'static str) {
    if project {
        (Some(project_config_path()), SCOPE_PROJECT)
    } else {
        // Default to global
        (global_config_path(), SCOPE_GLOBAL)
    }
}

/// Parse install command arguments.
pub fn parse_install_args(args: &[String]) -> Result<InstallOptions, String> {
    let mut opts = InstallOptions::default();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--global" => {
                opts.global = true;
                opts.project = false;
            }
            "--project" => {
                opts.project = true;
                opts.global = false;
            }
            "--full" => opts.full = true,
            "--minimal" => opts.full = false,
            "--dry-run" => opts.dry_run = true,
            "--force" => opts.force = true,
            "--help" | "-h" => return Err("help".to_string()),
            other => return Err(format!("unknown option: {}", other)),
        }
        i += 1;
    }

    Ok(opts)
}

/// Parse remove command arguments.
pub fn parse_remove_args(args: &[String]) -> Result<RemoveOptions, String> {
    let mut opts = RemoveOptions::default();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--global" => {
                opts.global = true;
                opts.project = false;
            }
            "--project" => {
                opts.project = true;
                opts.global = false;
            }
            "--help" | "-h" => return Err("help".to_string()),
            other => return Err(format!("unknown option: {}", other)),
        }
        i += 1;
    }

    Ok(opts)
}

/// Parse status command arguments.
pub fn parse_status_args(args: &[String]) -> Result<StatusOptions, String> {
    let mut opts = StatusOptions::default();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--global" => {
                opts.global = true;
                opts.project = false;
            }
            "--project" => {
                opts.project = true;
                opts.global = false;
            }
            "--json" => opts.json = true,
            "--help" | "-h" => return Err("help".to_string()),
            other => return Err(format!("unknown option: {}", other)),
        }
        i += 1;
    }

    Ok(opts)
}

/// Print install usage.
pub fn print_install_usage() {
    eprintln!("usage: rmap integrate claude-code install [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --global    Install to ~/.claude/settings.json (default)");
    eprintln!("  --project   Install to ./.claude/settings.json");
    eprintln!("  --full      Install all hooks (default: minimal - SessionStart + Stop)");
    eprintln!("  --dry-run   Show changes without applying");
    eprintln!("  --force     Overwrite existing repo-graph hooks");
    eprintln!("  --help      Show this help message");
}

/// Print remove usage.
pub fn print_remove_usage() {
    eprintln!("usage: rmap integrate claude-code remove [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --global    Remove from ~/.claude/settings.json (default)");
    eprintln!("  --project   Remove from ./.claude/settings.json");
    eprintln!("  --help      Show this help message");
}

/// Print status usage.
pub fn print_status_usage() {
    eprintln!("usage: rmap integrate claude-code status [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --global    Check ~/.claude/settings.json (default)");
    eprintln!("  --project   Check ./.claude/settings.json");
    eprintln!("  --json      Output JSON instead of human-readable text");
    eprintln!("  --help      Show this help message");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_install_args_defaults() {
        let opts = parse_install_args(&[]).unwrap();
        assert!(opts.global);
        assert!(!opts.project);
        assert!(!opts.full);
        assert!(!opts.dry_run);
        assert!(!opts.force);
    }

    #[test]
    fn test_parse_install_args_full() {
        let opts = parse_install_args(&["--full".to_string()]).unwrap();
        assert!(opts.full);
    }

    #[test]
    fn test_parse_install_args_project() {
        let opts = parse_install_args(&["--project".to_string()]).unwrap();
        assert!(opts.project);
        assert!(!opts.global);
    }

    #[test]
    fn test_parse_install_args_dry_run() {
        let opts = parse_install_args(&["--dry-run".to_string()]).unwrap();
        assert!(opts.dry_run);
    }

    #[test]
    fn test_parse_install_args_force() {
        let opts = parse_install_args(&["--force".to_string()]).unwrap();
        assert!(opts.force);
    }

    #[test]
    fn test_parse_install_args_help() {
        let result = parse_install_args(&["--help".to_string()]);
        assert_eq!(result.unwrap_err(), "help");
    }

    #[test]
    fn test_parse_install_args_unknown() {
        let result = parse_install_args(&["--unknown".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown option"));
    }

    #[test]
    fn test_parse_remove_args() {
        let opts = parse_remove_args(&["--project".to_string()]).unwrap();
        assert!(opts.project);
    }

    #[test]
    fn test_parse_status_args() {
        let opts = parse_status_args(&["--json".to_string()]).unwrap();
        assert!(opts.json);
    }

    #[test]
    fn test_resolve_config_path_global() {
        let (path, scope) = resolve_config_path(true, false);
        assert!(path.is_some());
        assert_eq!(scope, SCOPE_GLOBAL);
    }

    #[test]
    fn test_resolve_config_path_project() {
        let (path, scope) = resolve_config_path(false, true);
        assert!(path.is_some());
        assert_eq!(scope, SCOPE_PROJECT);
        assert_eq!(path.unwrap().to_string_lossy(), ".claude/settings.json");
    }
}
