//! `rmap hook post-edit` command.
//!
//! Keep index fresh after file edits.
//!
//! Actions:
//! 1. Parse file paths from --files argument or environment
//! 2. Check if files are in indexed repo
//! 3. Mark files as dirty (full refresh deferred)
//! 4. Record in session state
//! 5. Report impact

use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

use super::env::HookContext;
use super::output::{output_result, HookResult, HookStatus, HumanReadable};
use super::session::load_or_create_session;

/// Post-edit output data.
#[derive(Debug, Clone, Serialize)]
pub struct PostEditOutput {
    pub files_edited: Vec<String>,
    pub files_in_repo: u64,
    pub files_outside_repo: u64,
    pub refresh_triggered: bool,
    pub impact: ImpactSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImpactSummary {
    pub symbols_affected: u64,
    pub edges_affected: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub modules_affected: Vec<String>,
}

impl HumanReadable for HookResult<PostEditOutput> {
    fn print_human(&self) {
        let total = self.data.files_edited.len();
        println!(
            "post-edit: {} file{} recorded",
            total,
            if total == 1 { "" } else { "s" }
        );

        if self.data.files_in_repo > 0 {
            println!("  {} in repository", self.data.files_in_repo);
        }
        if self.data.files_outside_repo > 0 {
            println!("  {} outside repository", self.data.files_outside_repo);
        }

        if self.data.refresh_triggered {
            println!();
            println!("  Refresh: triggered");
            println!(
                "    {} symbols, {} edges affected",
                self.data.impact.symbols_affected, self.data.impact.edges_affected
            );
            if !self.data.impact.modules_affected.is_empty() {
                println!(
                    "    Modules: {}",
                    self.data.impact.modules_affected.join(", ")
                );
            }
        } else {
            println!();
            println!("  Refresh: deferred (files marked dirty)");
        }

        if !self.warnings.is_empty() {
            println!();
            for warning in &self.warnings {
                eprintln!("  Warning: {}", warning);
            }
        }
    }
}

/// Run the post-edit hook.
pub fn run_hook_post_edit(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_post_edit_usage();
        return ExitCode::SUCCESS;
    }

    let ctx = match HookContext::from_args(args) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("error: {}", e);
            print_post_edit_usage();
            return ExitCode::from(1);
        }
    };

    let (result, status) = execute_post_edit(&ctx);
    output_result(&result, ctx.json_output);
    ExitCode::from(status.exit_code())
}

fn execute_post_edit(ctx: &HookContext) -> (HookResult<PostEditOutput>, HookStatus) {
    let mut warnings = Vec::new();

    // Load session state
    let mut session = match load_or_create_session(&ctx.session_id) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("could not load session state: {}", e));
            super::session::SessionState::new(ctx.session_id.clone())
        }
    };

    // Record edited files
    let files = ctx.files.clone();
    session.record_edits(&files);

    // Count files in/outside repo (simplified check)
    let (in_repo, outside_repo) = if let Some(ref repo_path) = ctx.repo_path {
        count_files_in_repo(&files, repo_path)
    } else {
        // Without repo path, assume all files are relevant
        (files.len() as u64, 0)
    };

    // For MVP, we don't trigger actual refresh - just mark dirty
    // Full refresh integration requires calling into repo-index crate
    let refresh_triggered = false;
    let impact = ImpactSummary {
        symbols_affected: 0,
        edges_affected: 0,
        modules_affected: Vec::new(),
    };

    // Save session state
    if let Err(e) = session.save() {
        warnings.push(format!("could not save session state: {}", e));
    }

    let status = if warnings.is_empty() {
        HookStatus::Ok
    } else {
        HookStatus::Warning
    };

    let output = PostEditOutput {
        files_edited: files.iter().map(|p| p.display().to_string()).collect(),
        files_in_repo: in_repo,
        files_outside_repo: outside_repo,
        refresh_triggered,
        impact,
    };

    let result = if warnings.is_empty() {
        HookResult::ok(output)
    } else {
        HookResult::warning(output, warnings)
    };

    (result, status)
}

fn count_files_in_repo(files: &[PathBuf], repo_path: &std::path::Path) -> (u64, u64) {
    let repo_canonical = repo_path.canonicalize().ok();

    let mut in_repo = 0u64;
    let mut outside = 0u64;

    for file in files {
        let file_canonical = file.canonicalize().ok();

        let is_in_repo = match (&repo_canonical, &file_canonical) {
            (Some(repo), Some(f)) => f.starts_with(repo),
            _ => {
                // Fall back to string comparison
                file.starts_with(repo_path)
            }
        };

        if is_in_repo {
            in_repo += 1;
        } else {
            outside += 1;
        }
    }

    (in_repo, outside)
}

fn print_post_edit_usage() {
    eprintln!("usage: rmap hook post-edit [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --from-env          Read context from host environment variables");
    eprintln!("  --db <path>         Path to database file");
    eprintln!("  --repo <path>       Path to repository root");
    eprintln!("  --files <paths>     Comma-separated or JSON array of edited file paths");
    eprintln!("  --json              Output JSON instead of human-readable text");
    eprintln!("  --help              Show this help message");
    eprintln!();
    eprintln!("With --from-env, file paths are read from TOOL_OUTPUT (Claude Code)");
    eprintln!("or CHANGED_FILES (Codex).");
}
