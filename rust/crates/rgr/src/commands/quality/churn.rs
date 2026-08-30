//! Churn command.
//!
//! RS-MS-2: Query-time per-file git churn for indexed files.
//! No persistence. Git is the authoritative history source.
//!
//! # CLI-OUT-6 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw JSON)
//! - Deterministic ordering (by lines_changed desc, then path asc)
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
//! This module owns churn command behavior:
//! - `run_churn` handler
//! - CLI argument parsing for `--since` and `--json`
//! - Presentation DTO (via presentation::churn)
//!
//! This module does **not** own:
//! - Shared daemon support (lives in `crate::daemon_command`)
//! - Git churn extraction (belongs in `repo-graph-git`)

use std::process::ExitCode;

use crate::daemon_command::{
    execute_repo_request, output_result, print_daemon_error, EXIT_RUNTIME_ERROR, EXIT_USAGE_ERROR,
};
use crate::presentation::churn::ChurnResponse;

fn print_usage() {
    eprintln!("usage: rmap churn [--since <expr>] [--full] [--json]");
    eprintln!();
    eprintln!("Show file churn (commits and lines changed) for the repository.");
    eprintln!("Repository is resolved from current working directory.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --since <expr>  Time window (default: 90.days.ago)");
    eprintln!("  --full          Render every row (default: top 25 + a remainder line)");
    eprintln!("  --json          Output raw JSON instead of human-readable text");
}

/// Parsed arguments for churn command.
struct ChurnArgs {
    since: String,
    full: bool,
    json_mode: bool,
}

fn parse_args(args: &[String]) -> Result<ChurnArgs, ExitCode> {
    let mut since = "90.days.ago".to_string();
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

    Ok(ChurnArgs {
        since,
        full,
        json_mode,
    })
}

/// Run the `rmap churn` command.
///
/// Usage: `rmap churn [--since <expr>] [--json]`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed, computation failure)
pub fn run_churn(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(code) => return code,
    };

    // Build params for daemon request
    let params = serde_json::json!({
        "since": parsed.since,
    });

    // Execute request via daemon
    let result = match execute_repo_request("churn", Some(params)) {
        Ok(r) => r,
        Err(e) => {
            print_daemon_error(&e, "churn");
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    // Output result
    output_result::<ChurnResponse, _>(result, parsed.json_mode, |response| {
        response.render_human(parsed.full)
    })
}
