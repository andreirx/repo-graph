//! Risk command.
//!
//! RS-MS-4: Query-time risk analysis (hotspot x coverage gap).
//! Only files with BOTH hotspot AND coverage data are included.
//! Missing coverage = file excluded (not degraded to risk = hotspot).
//!
//! # CLI-OUT-6 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw JSON)
//! - Deterministic ordering (by risk_score desc, then path asc)
//! - Full output, no truncation
//! - **No invented verdict labels** (no CRITICAL/HIGH/MEDIUM/LOW)
//!
//! # REG-1 Contract (LEGACY-CONTRACT-MIGRATION-1B)
//!
//! Migrated from legacy `<db_path> <repo_uid>` contract to REG-1:
//! - Repo resolved from cwd via daemon
//! - No storage paths in user-facing contract
//!
//! # Boundary rules
//!
//! This module owns risk command behavior:
//! - `run_risk` handler
//! - CLI argument parsing
//! - Presentation DTO (via presentation::risk)
//!
//! This module does **not** own:
//! - Shared daemon support (lives in `crate::daemon_command`)
//! - Risk scoring (belongs in `repo-graph-classification`)
//! - Hotspot scoring (belongs in `repo-graph-classification`)
//! - Git churn extraction (belongs in `repo-graph-git`)

use std::process::ExitCode;

use crate::daemon_command::{
    execute_repo_request, output_result, print_daemon_error, EXIT_RUNTIME_ERROR, EXIT_USAGE_ERROR,
};
use crate::presentation::risk::RiskResponse;

fn print_usage() {
    eprintln!("usage: rmap risk [--since <expr>] [--json]");
    eprintln!();
    eprintln!("Show risk-scored files (hotspot x coverage gap) for the repository.");
    eprintln!("Repository is resolved from current working directory.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --since <expr>  Time window for churn analysis (default: 90.days.ago)");
    eprintln!("  --json          Output raw JSON instead of human-readable text");
}

/// Parsed arguments for risk command.
struct RiskArgs {
    since: String,
    json_mode: bool,
}

fn parse_args(args: &[String]) -> Result<RiskArgs, ExitCode> {
    let mut since = "90.days.ago".to_string();
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

    Ok(RiskArgs { since, json_mode })
}

/// Run the `rmap risk` command.
///
/// Usage: `rmap risk [--since <expr>] [--json]`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed, computation failure)
pub fn run_risk(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(code) => return code,
    };

    // Build params for daemon request
    let params = serde_json::json!({
        "since": parsed.since,
    });

    // Execute request via daemon
    let result = match execute_repo_request("risk", Some(params)) {
        Ok(r) => r,
        Err(e) => {
            print_daemon_error(&e, "risk");
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    // Output result
    output_result::<RiskResponse, _>(result, parsed.json_mode, |response| response.render_human())
}
