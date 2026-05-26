//! Gate command family.
//!
//! Requirement/obligation gate evaluation for CI integration.
//!
//! # REG-1 Contract
//!
//! With REG-1, gate resolves repo from cwd via daemon.
//! New contract: `rmap gate [--strict | --advisory] [--json]`
//!
//! # CLI-OUT-7 Group 3
//!
//! Human-readable output by default. Use `--json` for machine mode.
//! Domain verdicts (PASS/FAIL/WAIVED/MISSING_EVIDENCE/UNSUPPORTED) are
//! preserved exactly in human output.

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn print_usage() {
    eprintln!("usage: rmap gate [--strict | --advisory] [--json]");
    eprintln!();
    eprintln!("Evaluates the gate for the repository.");
    eprintln!("Repository is resolved from current working directory.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --strict    Strict mode (fail on any unmet requirement)");
    eprintln!("  --advisory  Advisory mode (warn but don't fail)");
    eprintln!("  --json      Output raw JSON instead of human-readable text");
}

/// Run the `rmap gate` command.
///
/// Usage: `rmap gate [--strict | --advisory] [--json]`
///
/// Exit codes:
/// - 0: gate pass
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed)
/// - 3: gate fail (strict mode or violations present)
pub fn run_gate(args: &[String]) -> ExitCode {
    // Parse optional mode flags
    let mut strict = false;
    let mut advisory = false;
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--strict" => strict = true,
            "--advisory" => advisory = true,
            "--json" => json_mode = true,
            flag if flag.starts_with('-') => {
                eprintln!("error: unknown flag: {}", flag);
                print_usage();
                return ExitCode::from(1);
            }
            _ => {
                eprintln!("error: unexpected argument: {}", arg);
                print_usage();
                return ExitCode::from(1);
            }
        }
    }

    if strict && advisory {
        eprintln!("error: --strict and --advisory are mutually exclusive");
        return ExitCode::from(1);
    }

    // Resolve repo from cwd
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot get current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    let repo_path = match cwd.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot canonicalize current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    // Connect to daemon
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Determine mode for daemon request
    let mode = if strict {
        "strict"
    } else if advisory {
        "advisory"
    } else {
        "strict" // Default to strict for CI use cases
    };

    // Send gate request
    let params = serde_json::json!({
        "repo": repo_path,
        "mode": mode,
    });

    match client.request("gate", Some(params)) {
        Ok(result) => {
            // Determine exit code from gate outcome
            let exit_code = result
                .get("gate")
                .and_then(|g| g.get("exit_code"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0) as u8;

            if json_mode {
                // Machine mode: raw JSON output
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::from(exit_code)
                    }
                    Err(e) => {
                        eprintln!("error: failed to serialize result: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: render readable output (CLI-OUT-7)
                use crate::presentation::gate::GateResponse;

                match serde_json::from_value::<GateResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::from(exit_code)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse gate response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
