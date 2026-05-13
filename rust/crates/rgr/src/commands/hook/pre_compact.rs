//! `rmap hook pre-compact` command.
//!
//! Checkpoint session state before context compaction.
//!
//! Actions:
//! 1. Capture current session state
//! 2. Write to session state file
//! 3. Output checkpoint confirmation

use std::process::ExitCode;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::env::HookContext;
use super::output::{output_result, HookResult, HookStatus, HumanReadable};
use super::session::load_or_create_session;

/// Pre-compact output data.
#[derive(Debug, Clone, Serialize)]
pub struct PreCompactOutput {
    pub checkpoint: CheckpointInfo,
    pub state_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointInfo {
    pub timestamp: DateTime<Utc>,
    pub db_path: Option<String>,
    pub repo_path: Option<String>,
    pub changed_files: Vec<String>,
    pub trust_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_slice: Option<String>,
}

impl HumanReadable for HookResult<PreCompactOutput> {
    fn print_human(&self) {
        println!("pre-compact checkpoint");
        println!(
            "  Timestamp: {}",
            self.data
                .checkpoint
                .timestamp
                .format("%Y-%m-%d %H:%M:%S UTC")
        );

        if let Some(ref db) = self.data.checkpoint.db_path {
            println!("  Database: {}", db);
        }

        if let Some(ref repo) = self.data.checkpoint.repo_path {
            println!("  Repository: {}", repo);
        }

        let file_count = self.data.checkpoint.changed_files.len();
        println!("  Changed files: {}", file_count);

        println!("  Trust: {}", self.data.checkpoint.trust_summary);

        if let Some(ref slice) = self.data.checkpoint.current_slice {
            println!("  Current slice: {}", slice);
        }

        if let Some(ref path) = self.data.state_file {
            println!();
            println!("  State file: {}", path);
        }

        if !self.warnings.is_empty() {
            println!();
            for warning in &self.warnings {
                eprintln!("  Warning: {}", warning);
            }
        }
    }
}

/// Run the pre-compact hook.
pub fn run_hook_pre_compact(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_pre_compact_usage();
        return ExitCode::SUCCESS;
    }

    let ctx = match HookContext::from_args(args) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("error: {}", e);
            print_pre_compact_usage();
            return ExitCode::from(1);
        }
    };

    let (result, status) = execute_pre_compact(&ctx);
    output_result(&result, ctx.json_output);
    ExitCode::from(status.exit_code())
}

fn execute_pre_compact(ctx: &HookContext) -> (HookResult<PreCompactOutput>, HookStatus) {
    let mut warnings = Vec::new();

    // Load session state
    let session = match load_or_create_session(&ctx.session_id) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("could not load session state: {}", e));
            super::session::SessionState::new(ctx.session_id.clone())
        }
    };

    // Determine trust summary from session validations
    let trust_summary = if let Some(ref trust) = session.validations.trust_check {
        format!("{:?}", trust.result).to_lowercase()
    } else {
        "not checked".to_string()
    };

    // Create checkpoint info
    let checkpoint = CheckpointInfo {
        timestamp: Utc::now(),
        db_path: session.db_path.as_ref().map(|p| p.display().to_string()),
        repo_path: session.repo_path.as_ref().map(|p| p.display().to_string()),
        changed_files: session
            .files_edited
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        trust_summary,
        current_slice: None, // Would read from CURRENT_SLICE.md
    };

    // Save session state (already contains all info)
    let state_file = match session.save() {
        Ok(path) => Some(path.display().to_string()),
        Err(e) => {
            warnings.push(format!("could not save checkpoint: {}", e));
            None
        }
    };

    let status = if warnings.is_empty() {
        HookStatus::Ok
    } else {
        HookStatus::Warning
    };

    let output = PreCompactOutput {
        checkpoint,
        state_file,
    };

    let result = if warnings.is_empty() {
        HookResult::ok(output)
    } else {
        HookResult::warning(output, warnings)
    };

    (result, status)
}

fn print_pre_compact_usage() {
    eprintln!("usage: rmap hook pre-compact [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --from-stdin        Read JSON payload from stdin (Claude Code)");
    eprintln!("  --from-env          Read from host environment variables (Codex, testing)");
    eprintln!("  --db <path>         Path to database file");
    eprintln!("  --repo <path>       Path to repository root");
    eprintln!("  --json              Output JSON instead of human-readable text");
    eprintln!("  --help              Show this help message");
}
