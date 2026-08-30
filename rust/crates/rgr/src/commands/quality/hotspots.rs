//! Hotspots command.
//!
//! RS-MS-3b: Query-time hotspot analysis (churn x complexity).
//! No persistence. Git is the authoritative churn source.
//! Complexity from stored measurements.
//!
//! # CLI-OUT-6 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw JSON)
//! - Deterministic ordering (by hotspot_score desc, then path asc)
//! - Full output, no truncation
//!
//! # REG-1 Contract (LEGACY-CONTRACT-MIGRATION-1B)
//!
//! Migrated from legacy `<db_path> <repo_uid>` contract to REG-1:
//! - Repo resolved from cwd via daemon
//! - No storage paths in user-facing contract
//!
//! # Boundary rules
//!
//! This module owns hotspots command behavior:
//! - `run_hotspots` handler
//! - CLI argument parsing
//! - Presentation DTO (via presentation::hotspots)
//!
//! This module does **not** own:
//! - Shared daemon support (lives in `crate::daemon_command`)
//! - Hotspot scoring (belongs in `repo-graph-classification`)
//! - Git churn extraction (belongs in `repo-graph-git`)

use std::process::ExitCode;

use crate::daemon_command::{
    execute_repo_request, output_result, print_daemon_error, EXIT_RUNTIME_ERROR, EXIT_USAGE_ERROR,
};
use crate::presentation::hotspots::HotspotsResponse;

fn print_usage() {
    eprintln!(
        "usage: rmap hotspots [--since <expr>] [--exclude-tests] [--exclude-vendored] [--full] [--json]"
    );
    eprintln!();
    eprintln!("Show hotspot files (high churn x complexity) for the repository.");
    eprintln!("Repository is resolved from current working directory.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --since <expr>       Time window (default: 90.days.ago)");
    eprintln!("  --exclude-tests      Exclude test files from results");
    eprintln!("  --exclude-vendored   Exclude vendored directories from results");
    eprintln!("  --full               Render every row (default: top 25 + a remainder line)");
    eprintln!("  --json               Output raw JSON instead of human-readable text");
}

/// Parsed arguments for hotspots command.
struct HotspotsArgs {
    since: String,
    exclude_tests: bool,
    exclude_vendored: bool,
    full: bool,
    json_mode: bool,
}

fn parse_args(args: &[String]) -> Result<HotspotsArgs, ExitCode> {
    let mut since = "90.days.ago".to_string();
    let mut exclude_tests = false;
    let mut exclude_vendored = false;
    let mut full = false;
    let mut json_mode = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--since" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --since requires a value");
                    return Err(ExitCode::from(EXIT_USAGE_ERROR));
                }
                since = args[i + 1].clone();
                i += 2;
            }
            "--exclude-tests" => {
                exclude_tests = true;
                i += 1;
            }
            "--exclude-vendored" => {
                exclude_vendored = true;
                i += 1;
            }
            "--full" => {
                full = true;
                i += 1;
            }
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
                eprintln!("error: unexpected argument: {}", other);
                print_usage();
                return Err(ExitCode::from(EXIT_USAGE_ERROR));
            }
        }
    }

    Ok(HotspotsArgs {
        since,
        exclude_tests,
        exclude_vendored,
        full,
        json_mode,
    })
}

/// Run the `rmap hotspots` command.
///
/// Usage: `rmap hotspots [--since <expr>] [--exclude-tests] [--exclude-vendored] [--json]`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed, computation failure)
pub fn run_hotspots(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(code) => return code,
    };

    // Build params for daemon request
    let params = serde_json::json!({
        "since": parsed.since,
        "exclude_tests": parsed.exclude_tests,
        "exclude_vendored": parsed.exclude_vendored,
    });

    // Execute request via daemon
    let result = match execute_repo_request("hotspots", Some(params)) {
        Ok(r) => r,
        Err(e) => {
            print_daemon_error(&e, "hotspots");
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    // Output result
    output_result::<HotspotsResponse, _>(result, parsed.json_mode, |response| {
        response.render_human(parsed.full)
    })
}
