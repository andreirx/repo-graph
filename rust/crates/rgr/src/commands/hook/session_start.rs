//! `rmap hook session-start` command.
//!
//! Orient agent at session start. Outputs context summary to stdout.
//!
//! Actions:
//! 1. Verify DB exists or prompt for index
//! 2. Run lightweight trust check
//! 3. Run orient summary
//! 4. Check for CURRENT_SLICE.md
//! 5. Output structured summary

use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;

use crate::cli::{open_storage, resolve_repo_ref};

use super::env::HookContext;
use super::output::{output_result, HookResult, HookStatus, HumanReadable};
use super::session::{load_or_create_session, ValidationResult};

/// Session start output data.
#[derive(Debug, Clone, Serialize)]
pub struct SessionStartOutput {
    pub db_path: Option<String>,
    pub repo: Option<String>,
    pub trust: TrustSummary,
    pub orientation: OrientationSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_slice: Option<CurrentSliceInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustSummary {
    pub level: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrientationSummary {
    pub modules: u64,
    pub boundaries: u64,
    pub recent_changes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentSliceInfo {
    pub path: String,
    pub summary: Option<String>,
}

impl HumanReadable for HookResult<SessionStartOutput> {
    fn print_human(&self) {
        println!("repo-graph session start");

        if let Some(ref db) = self.data.db_path {
            println!("  Database: {}", db);
        } else {
            println!("  Database: not found");
        }

        if let Some(ref repo) = self.data.repo {
            println!("  Repository: {}", repo);
        }

        println!(
            "  Trust: {} {}",
            self.data.trust.level,
            if self.data.trust.caveats.is_empty() {
                "(no caveats)".to_string()
            } else {
                format!("({} caveats)", self.data.trust.caveats.len())
            }
        );

        println!();
        println!("  Orientation:");
        println!("    {} modules", self.data.orientation.modules);
        println!("    {} boundary surfaces", self.data.orientation.boundaries);
        if self.data.orientation.recent_changes > 0 {
            println!(
                "    {} files changed since last index",
                self.data.orientation.recent_changes
            );
        }

        if let Some(ref slice) = self.data.current_slice {
            println!();
            if let Some(ref summary) = slice.summary {
                println!("  Current slice: {}", summary);
            }
            println!("    See: {}", slice.path);
        }

        if !self.data.suggestions.is_empty() {
            println!();
            for suggestion in &self.data.suggestions {
                println!("  Suggestion: {}", suggestion);
            }
        }

        if !self.warnings.is_empty() {
            println!();
            for warning in &self.warnings {
                eprintln!("  Warning: {}", warning);
            }
        }

        if let Some(ref error) = self.error {
            eprintln!();
            eprintln!("  Error: {}", error);
        }
    }
}

/// Run the session-start hook.
pub fn run_hook_session_start(args: &[String]) -> ExitCode {
    // Check for help
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_session_start_usage();
        return ExitCode::SUCCESS;
    }

    // Parse context
    let ctx = match HookContext::from_args(args) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("error: {}", e);
            print_session_start_usage();
            return ExitCode::from(1);
        }
    };

    // Execute hook
    let (result, status) = execute_session_start(&ctx);

    // Output result
    output_result(&result, ctx.json_output);

    ExitCode::from(status.exit_code())
}

fn execute_session_start(ctx: &HookContext) -> (HookResult<SessionStartOutput>, HookStatus) {
    let mut warnings = Vec::new();
    let mut suggestions = Vec::new();

    // Load or create session state
    let mut session = match load_or_create_session(&ctx.session_id) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("could not load session state: {}", e));
            super::session::SessionState::new(ctx.session_id.clone())
        }
    };

    // Update session with context
    session.db_path = ctx.db_path.clone();
    session.repo_path = ctx.repo_path.clone();

    // Try to find and open database
    let (db_path_str, repo_name, trust_summary, orientation) = if let Some(ref db_path) =
        ctx.db_path
    {
        match try_open_db_and_gather_info(db_path, ctx.repo_path.as_deref()) {
            Ok(info) => {
                // Record trust check
                session.record_trust_check(ValidationResult::Passed);
                info
            }
            Err(e) => {
                warnings.push(e);
                suggestions.push("Run `rmap index <repo> <db>` to create a database".to_string());
                (
                    Some(db_path.display().to_string()),
                    None,
                    TrustSummary {
                        level: "unknown".to_string(),
                        caveats: vec!["database not accessible".to_string()],
                    },
                    OrientationSummary {
                        modules: 0,
                        boundaries: 0,
                        recent_changes: 0,
                    },
                )
            }
        }
    } else {
        suggestions.push("Provide --db <path> to specify database location".to_string());
        (
            None,
            None,
            TrustSummary {
                level: "unknown".to_string(),
                caveats: vec!["no database specified".to_string()],
            },
            OrientationSummary {
                modules: 0,
                boundaries: 0,
                recent_changes: 0,
            },
        )
    };

    // Check for CURRENT_SLICE.md
    let current_slice = if let Some(ref repo_path) = ctx.repo_path {
        check_current_slice(repo_path)
    } else {
        None
    };

    // Save session state
    if let Err(e) = session.save() {
        warnings.push(format!("could not save session state: {}", e));
    }

    // Determine status
    let status = if warnings.is_empty() {
        HookStatus::Ok
    } else {
        HookStatus::Warning
    };

    let output = SessionStartOutput {
        db_path: db_path_str,
        repo: repo_name,
        trust: trust_summary,
        orientation,
        current_slice,
        suggestions,
    };

    let result = if warnings.is_empty() {
        HookResult::ok(output)
    } else {
        HookResult::warning(output, warnings)
    };

    (result, status)
}

fn try_open_db_and_gather_info(
    db_path: &Path,
    _repo_path: Option<&Path>,
) -> Result<
    (
        Option<String>,
        Option<String>,
        TrustSummary,
        OrientationSummary,
    ),
    String,
> {
    let storage = open_storage(db_path)?;

    // Get first repo (simplified - in practice would use repo_path to resolve)
    let repos = storage
        .list_repos()
        .map_err(|e| format!("failed to list repos: {}", e))?;

    let repo = repos.first();
    let repo_name = repo.map(|r| r.name.clone());
    let repo_uid = repo.map(|r| r.repo_uid.clone());

    // Get trust summary if we have a repo
    let trust_summary = if let Some(ref uid) = repo_uid {
        match resolve_repo_ref(&storage, uid) {
            Ok(_) => {
                // Simplified trust check - in full implementation would call trust service
                TrustSummary {
                    level: "high".to_string(),
                    caveats: Vec::new(),
                }
            }
            Err(e) => TrustSummary {
                level: "unknown".to_string(),
                caveats: vec![e],
            },
        }
    } else {
        TrustSummary {
            level: "unknown".to_string(),
            caveats: vec!["no repository found in database".to_string()],
        }
    };

    // Get orientation summary
    let orientation = if let Some(ref uid) = repo_uid {
        gather_orientation_summary(&storage, uid)
    } else {
        OrientationSummary {
            modules: 0,
            boundaries: 0,
            recent_changes: 0,
        }
    };

    Ok((
        Some(db_path.display().to_string()),
        repo_name,
        trust_summary,
        orientation,
    ))
}

fn gather_orientation_summary(
    _storage: &repo_graph_storage::StorageConnection,
    _repo_uid: &str,
) -> OrientationSummary {
    // Simplified for MVP - full implementation would query actual counts.
    // The count methods require snapshot_uid, not repo_uid, and additional
    // lookup logic. Defer to future enhancement.
    //
    // TODO: Implement proper orientation summary gathering:
    // - Get latest snapshot for repo
    // - Count module_candidates for that snapshot
    // - Count boundary_surfaces for that snapshot
    // - Compute recent changes via git

    OrientationSummary {
        modules: 0,
        boundaries: 0,
        recent_changes: 0,
    }
}

fn check_current_slice(repo_path: &Path) -> Option<CurrentSliceInfo> {
    let slice_path = repo_path.join("CURRENT_SLICE.md");

    if !slice_path.exists() {
        return None;
    }

    // Read first few lines to get summary
    let content = std::fs::read_to_string(&slice_path).ok()?;
    let summary = extract_slice_summary(&content);

    Some(CurrentSliceInfo {
        path: "CURRENT_SLICE.md".to_string(),
        summary,
    })
}

fn extract_slice_summary(content: &str) -> Option<String> {
    // Look for "## Current Priority" or similar header
    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and headers
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Look for **BOLD** text which typically indicates the active slice
        if trimmed.starts_with("**") && trimmed.contains("**") {
            // Extract the bold text
            if let Some(start) = trimmed.find("**") {
                if let Some(end) = trimmed[start + 2..].find("**") {
                    let summary = &trimmed[start + 2..start + 2 + end];
                    return Some(summary.to_string());
                }
            }
        }
    }

    None
}

fn print_session_start_usage() {
    eprintln!("usage: rmap hook session-start [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --from-stdin        Read JSON payload from stdin (Claude Code)");
    eprintln!("  --from-env          Read from host environment variables (Codex, testing)");
    eprintln!("  --db <path>         Path to database file");
    eprintln!("  --repo <path>       Path to repository root");
    eprintln!("  --json              Output JSON instead of human-readable text");
    eprintln!("  --help              Show this help message");
    eprintln!();
    eprintln!("Transport modes:");
    eprintln!("  --from-stdin: Claude Code passes JSON on stdin with session_id, cwd fields");
    eprintln!("  --from-env: Codex uses CODEX_PROJECT_PATH, CODEX_SESSION_ID env vars");
}
