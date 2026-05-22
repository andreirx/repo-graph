//! Coverage command.
//!
//! RS-MS-4-prereq-b/c: Import Istanbul/c8 coverage into measurements.
//! Delete-before-insert for idempotency. Reports matched/unmatched counts.
//!
//! # CLI-OUT-6 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw JSON)
//! - Imported file rows: full output, no truncation
//! - Sample-path diagnostics: backend-bounded (max 10), labeled as samples
//! - Deterministic ordering (by file_path)
//!
//! # REG-1 Contract (LEGACY-CONTRACT-MIGRATION-1B)
//!
//! Migrated from legacy `<db_path> <repo_uid>` contract to REG-1:
//! - Repo resolved from cwd via daemon
//! - Only `<report>` positional arg required
//! - No storage paths in user-facing contract
//!
//! # Boundary rules
//!
//! This module owns coverage command behavior:
//! - `run_coverage` handler
//! - CLI argument parsing
//! - Presentation DTO (via presentation::coverage)
//!
//! This module does **not** own:
//! - Shared daemon support (lives in `crate::daemon_command`)
//! - Coverage matching orchestration (lives in `repo-graph-classification::coverage_matcher`)
//! - Coverage report parsing (belongs in `repo-graph-coverage`)

use std::path::Path;
use std::process::ExitCode;

use crate::daemon_command::{
    execute_repo_request, output_result, print_daemon_error, EXIT_RUNTIME_ERROR, EXIT_USAGE_ERROR,
};
use crate::presentation::coverage::CoverageResponse;

fn print_usage() {
    eprintln!("usage: rmap coverage <report> [--json]");
    eprintln!();
    eprintln!("Import Istanbul/c8 coverage report into the repository.");
    eprintln!("Repository is resolved from current working directory.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  <report>  Path to coverage-final.json or similar Istanbul/c8 report");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --json    Output raw JSON instead of human-readable text");
}

/// Parsed arguments for coverage command.
struct CoverageArgs {
    report_path: String,
    json_mode: bool,
}

fn parse_args(args: &[String]) -> Result<CoverageArgs, ExitCode> {
    let mut report_path: Option<String> = None;
    let mut json_mode = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_mode = true;
                i += 1;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                print_usage();
                return Err(ExitCode::from(EXIT_USAGE_ERROR));
            }
            other => {
                // Positional argument: report path
                if report_path.is_some() {
                    eprintln!("error: unexpected argument: {}", other);
                    print_usage();
                    return Err(ExitCode::from(EXIT_USAGE_ERROR));
                }
                report_path = Some(other.to_string());
                i += 1;
            }
        }
    }

    let report_path = match report_path {
        Some(p) => p,
        None => {
            eprintln!("error: missing <report> argument");
            print_usage();
            return Err(ExitCode::from(EXIT_USAGE_ERROR));
        }
    };

    Ok(CoverageArgs {
        report_path,
        json_mode,
    })
}

/// Run the `rmap coverage` command.
///
/// Usage: `rmap coverage <report> [--json]`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed, import failure)
pub fn run_coverage(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(code) => return code,
    };

    // Validate report exists before sending to daemon
    let report_path = Path::new(&parsed.report_path);
    if !report_path.is_file() {
        eprintln!(
            "error: coverage report not found: {}",
            report_path.display()
        );
        return ExitCode::from(EXIT_USAGE_ERROR);
    }

    // Canonicalize report path for daemon (daemon doesn't know CLI's cwd)
    let report_path_abs = match report_path.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot resolve report path: {}", e);
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    // Build params for daemon request
    let params = serde_json::json!({
        "report_path": report_path_abs,
    });

    // Execute request via daemon
    let result = match execute_repo_request("coverage", Some(params)) {
        Ok(r) => r,
        Err(e) => {
            print_daemon_error(&e, "coverage");
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    // Output result
    output_result::<CoverageResponse, _>(result, parsed.json_mode, |response| {
        response.render_human()
    })
}
