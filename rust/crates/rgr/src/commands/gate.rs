//! Gate command family.
//!
//! Requirement/obligation gate evaluation for CI integration.
//!
//! # REG-1 Contract
//!
//! With REG-1, gate resolves repo from cwd via daemon.
//! New contract: `rmap gate [--strict | --advisory]`

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn print_usage() {
    eprintln!("usage: rmap gate [--strict | --advisory]");
    eprintln!();
    eprintln!("Evaluates the gate for the repository.");
    eprintln!("Repository is resolved from current working directory.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --strict    Strict mode (fail on any unmet requirement)");
    eprintln!("  --advisory  Advisory mode (warn but don't fail)");
}

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

/// Run the `rmap gate` command.
///
/// Usage: `rmap gate [--strict | --advisory]`
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

    for arg in args {
        match arg.as_str() {
            "--strict" => strict = true,
            "--advisory" => advisory = true,
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

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path()));
        return ExitCode::from(2);
    }

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
            // Pretty-print JSON to stdout
            match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    println!("{}", json);

                    // Determine exit code from gate outcome
                    let exit_code = result
                        .get("gate")
                        .and_then(|g| g.get("exit_code"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0) as u8;

                    ExitCode::from(exit_code)
                }
                Err(e) => {
                    eprintln!("error: failed to serialize result: {}", e);
                    ExitCode::from(2)
                }
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
