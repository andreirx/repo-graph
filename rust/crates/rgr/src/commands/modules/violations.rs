//! Violations commands.
//!
//! Contains:
//! - `run_violations` — unified violations command (declared + discovered-module)
//! - `run_modules_violations` — discovered-module violations only (REG-1)
//!
//! RS-MG-12b: Boundary violation diagnostics.
//! CLI-OUT-4: Human-readable output with `--json` for machine mode (modules violations).
//! CLI-OUT-7: Human-readable output with `--json` for machine mode (top-level violations).
//!
//! # REG-1 Contract (LEGACY-CONTRACT-MIGRATION-1C)
//!
//! Both commands now use REG-1 daemon contract:
//! - Repo resolved from cwd (auto-discovery)
//! - Daemon handles storage access
//! - CLI handles argument parsing and output rendering
//!
//! # Presentation Split
//!
//! - `run_violations`: Uses `presentation::violations` (CLI-OUT-7)
//! - `run_modules_violations`: Uses `presentation::modules_violations` (CLI-OUT-4)
//!
//! # Boundary rules
//!
//! This module owns violations command behavior:
//! - command handlers
//! - argument parsing for violations command
//! - mode switching (human vs --json)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::daemon_command`)
//! - module graph loading (lives in daemon via module-queries)
//! - boundary evaluation logic (lives in daemon via classification)
//! - human output rendering (lives in `presentation::*`)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;
use crate::daemon_command::{
    execute_repo_request, output_result, print_daemon_error, EXIT_RUNTIME_ERROR, EXIT_USAGE_ERROR,
};

// ── unified violations command (REG-1) ───────────────────────────
//
// LEGACY-CONTRACT-MIGRATION-1C: Migrated from legacy db_path + repo_uid
// to REG-1 daemon contract (cwd auto-discovery).

pub fn run_violations(args: &[String]) -> ExitCode {
    // Parse args: [--json]
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with('-') => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap violations [--json]");
                return ExitCode::from(EXIT_USAGE_ERROR);
            }
            _ => {
                eprintln!("error: unexpected argument: {}", arg);
                eprintln!("usage: rmap violations [--json]");
                eprintln!();
                eprintln!("Run from within a repo directory.");
                return ExitCode::from(EXIT_USAGE_ERROR);
            }
        }
    }

    // Execute via daemon
    match execute_repo_request("violations", None) {
        Ok(result) => output_result(
            result,
            json_mode,
            |response: crate::presentation::violations::ViolationsResponse| response.render_human(),
        ),
        Err(err) => {
            print_daemon_error(&err, "violations");
            ExitCode::from(EXIT_RUNTIME_ERROR)
        }
    }
}

// ── modules violations command (REG-1) ───────────────────────────
//
// `rmap modules violations [--json]`
//
// Human mode (default): plain text with violation diagnostics.
// Machine mode (--json): full envelope.
//
// Exit codes:
// - 0: no violations
// - 1: violations found (stale declarations alone do not force exit 1)
// - 2: runtime error

pub(super) fn run_modules_violations(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json) ──────────────────────────
    let mut json_mode = false;
    let mut unexpected: Option<&String> = None;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap modules violations [--json]");
                return ExitCode::from(EXIT_USAGE_ERROR);
            }
            _ => {
                if unexpected.is_none() {
                    unexpected = Some(arg);
                }
            }
        }
    }

    if let Some(arg) = unexpected {
        eprintln!("error: unexpected argument: {}", arg);
        eprintln!("usage: rmap modules violations [--json]");
        eprintln!();
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(EXIT_USAGE_ERROR);
    }

    // Get cwd for repo resolution
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {}", e);
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    let repo_path = match cwd.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot canonicalize current directory: {}", e);
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    // Connect to daemon
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    // Build request params
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("modules_violations", Some(params)) {
        Ok(result) => {
            // Extract violation count for exit code
            let violation_count = result.get("count").and_then(|v| v.as_u64()).unwrap_or(0);

            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        // Exit code: 0 if no violations, 1 if violations
                        if violation_count > 0 {
                            ExitCode::from(1)
                        } else {
                            ExitCode::SUCCESS
                        }
                    }
                    Err(e) => {
                        eprintln!("error: failed to serialize result: {}", e);
                        ExitCode::from(EXIT_RUNTIME_ERROR)
                    }
                }
            } else {
                // Human mode: parse and render (CLI-OUT-4)
                use crate::presentation::modules_violations::ModulesViolationsResponse;
                match serde_json::from_value::<ModulesViolationsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        // Exit code: 0 if no violations, 1 if violations
                        if violation_count > 0 {
                            ExitCode::from(1)
                        } else {
                            ExitCode::SUCCESS
                        }
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse modules violations response: {}", e);
                        ExitCode::from(EXIT_RUNTIME_ERROR)
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(EXIT_RUNTIME_ERROR)
        }
    }
}
