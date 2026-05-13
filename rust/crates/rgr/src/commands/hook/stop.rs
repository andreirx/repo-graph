//! `rmap hook stop` command.
//!
//! Validate and summarize at task completion.
//!
//! Actions:
//! 1. Check what validation was run during session
//! 2. Produce validation summary
//! 3. Report session statistics

use std::process::ExitCode;

use serde::Serialize;

use super::env::HookContext;
use super::output::{output_result, HookResult, HookStatus, HumanReadable};
use super::session::load_or_create_session;

/// Stop output data.
#[derive(Debug, Clone, Serialize)]
pub struct StopOutput {
    pub validation: ValidationSummary,
    pub summary: SessionSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationSummary {
    pub trust_check: ValidationState,
    pub refresh: ValidationState,
    pub gate: ValidationState,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationState {
    Passed,
    Failed,
    NotRun,
}

impl std::fmt::Display for ValidationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationState::Passed => write!(f, "passed"),
            ValidationState::Failed => write!(f, "failed"),
            ValidationState::NotRun => write!(f, "not run"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub files_changed: u64,
    pub refreshes_run: u64,
    pub duration_seconds: Option<u64>,
}

impl HumanReadable for HookResult<StopOutput> {
    fn print_human(&self) {
        println!("session complete");
        println!();
        println!("  Validation:");
        println!("    Trust check: {}", self.data.validation.trust_check);
        println!("    Refresh: {}", self.data.validation.refresh);
        println!("    Gate: {}", self.data.validation.gate);

        println!();
        println!("  Summary:");
        println!("    Files changed: {}", self.data.summary.files_changed);
        println!("    Refreshes run: {}", self.data.summary.refreshes_run);
        if let Some(duration) = self.data.summary.duration_seconds {
            let minutes = duration / 60;
            let seconds = duration % 60;
            if minutes > 0 {
                println!("    Duration: {}m {}s", minutes, seconds);
            } else {
                println!("    Duration: {}s", seconds);
            }
        }

        if let Some(ref path) = self.data.transcript_path {
            println!();
            println!("  Transcript: {}", path);
        }

        if !self.warnings.is_empty() {
            println!();
            for warning in &self.warnings {
                eprintln!("  Warning: {}", warning);
            }
        }
    }
}

/// Run the stop hook.
pub fn run_hook_stop(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_stop_usage();
        return ExitCode::SUCCESS;
    }

    let ctx = match HookContext::from_args(args) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("error: {}", e);
            print_stop_usage();
            return ExitCode::from(1);
        }
    };

    let (result, status) = execute_stop(&ctx);
    output_result(&result, ctx.json_output);
    ExitCode::from(status.exit_code())
}

fn execute_stop(ctx: &HookContext) -> (HookResult<StopOutput>, HookStatus) {
    let mut warnings = Vec::new();

    // Load session state
    let session = match load_or_create_session(&ctx.session_id) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("could not load session state: {}", e));
            super::session::SessionState::new(ctx.session_id.clone())
        }
    };

    // Build validation summary from session state
    let validation = ValidationSummary {
        trust_check: session
            .validations
            .trust_check
            .as_ref()
            .map(|v| match v.result {
                super::session::ValidationResult::Passed => ValidationState::Passed,
                super::session::ValidationResult::Failed => ValidationState::Failed,
                super::session::ValidationResult::Incomplete => ValidationState::NotRun,
            })
            .unwrap_or(ValidationState::NotRun),
        refresh: session
            .validations
            .refresh
            .as_ref()
            .map(|v| match v.result {
                super::session::ValidationResult::Passed => ValidationState::Passed,
                super::session::ValidationResult::Failed => ValidationState::Failed,
                super::session::ValidationResult::Incomplete => ValidationState::NotRun,
            })
            .unwrap_or(ValidationState::NotRun),
        gate: session
            .validations
            .gate
            .as_ref()
            .map(|v| match v.result {
                super::session::ValidationResult::Passed => ValidationState::Passed,
                super::session::ValidationResult::Failed => ValidationState::Failed,
                super::session::ValidationResult::Incomplete => ValidationState::NotRun,
            })
            .unwrap_or(ValidationState::NotRun),
    };

    // Check if required validations weren't run
    let validations_missing = !session.has_required_validations();
    if validations_missing {
        warnings.push("Required validations (trust, refresh) were not run".to_string());
    }

    // Calculate session duration
    let duration_seconds = {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(session.started_at);
        if duration.num_seconds() > 0 {
            Some(duration.num_seconds() as u64)
        } else {
            None
        }
    };

    let summary = SessionSummary {
        files_changed: session.files_edited.len() as u64,
        refreshes_run: session.refreshes.len() as u64,
        duration_seconds,
    };

    // Write transcript if requested
    let transcript_path = if let Some(ref path) = ctx.transcript_path {
        match write_transcript(path, &session, &validation, &summary) {
            Ok(()) => Some(path.display().to_string()),
            Err(e) => {
                warnings.push(format!("could not write transcript: {}", e));
                None
            }
        }
    } else {
        None
    };

    // Determine status based on --require-validation flag
    let status = if ctx.require_validation && validations_missing {
        HookStatus::Error
    } else if warnings.is_empty() {
        HookStatus::Ok
    } else {
        HookStatus::Warning
    };

    let output = StopOutput {
        validation,
        summary,
        transcript_path,
    };

    let result = if status == HookStatus::Error {
        HookResult::error(output, "Required validations not run".to_string())
    } else if warnings.is_empty() {
        HookResult::ok(output)
    } else {
        HookResult::warning(output, warnings)
    };

    (result, status)
}

/// Write session transcript to file.
fn write_transcript(
    path: &std::path::Path,
    session: &super::session::SessionState,
    validation: &ValidationSummary,
    summary: &SessionSummary,
) -> Result<(), String> {
    use serde::Serialize;

    #[derive(Serialize)]
    struct Transcript<'a> {
        session_id: &'a str,
        started_at: &'a chrono::DateTime<chrono::Utc>,
        ended_at: chrono::DateTime<chrono::Utc>,
        db_path: Option<String>,
        repo_path: Option<String>,
        files_edited: Vec<String>,
        validation: &'a ValidationSummary,
        summary: &'a SessionSummary,
    }

    let transcript = Transcript {
        session_id: &session.session_id,
        started_at: &session.started_at,
        ended_at: chrono::Utc::now(),
        db_path: session.db_path.as_ref().map(|p| p.display().to_string()),
        repo_path: session.repo_path.as_ref().map(|p| p.display().to_string()),
        files_edited: session
            .files_edited
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        validation,
        summary,
    };

    let content = serde_json::to_string_pretty(&transcript)
        .map_err(|e| format!("failed to serialize transcript: {}", e))?;

    std::fs::write(path, content).map_err(|e| format!("failed to write transcript file: {}", e))?;

    Ok(())
}

fn print_stop_usage() {
    eprintln!("usage: rmap hook stop [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --from-env              Read context from host environment variables");
    eprintln!("  --db <path>             Path to database file");
    eprintln!("  --repo <path>           Path to repository root");
    eprintln!("  --require-validation    Fail if required validation not run");
    eprintln!("  --transcript <path>     Write transcript to file");
    eprintln!("  --json                  Output JSON instead of human-readable text");
    eprintln!("  --help                  Show this help message");
}
