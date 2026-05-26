//! Trust command family.
//!
//! Computes and outputs the trust report for a repository snapshot.
//!
//! # REG-1 Contract
//!
//! With REG-1, trust resolves repo from cwd via daemon.
//! New contract: `rmap trust [--json]`
//!
//! # CLI-OUT-2B Output Modes
//!
//! - **Human mode (default)**: Plain text output with key metrics.
//! - **Machine mode (--json)**: Full daemon envelope as pretty-printed JSON.

use std::process::ExitCode;

use crate::daemon_client::{DaemonClient, DaemonClientError};
use crate::presentation::trust::TrustResponse;

fn print_usage() {
    eprintln!("usage: rmap trust [--json]");
    eprintln!();
    eprintln!("Computes and outputs the trust report for the repository.");
    eprintln!("Repository is resolved from current working directory.");
}

/// Run the `rmap trust` command.
///
/// Usage: `rmap trust [--json]`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed, computation failure)
pub fn run_trust(args: &[String]) -> ExitCode {
    // ── Parse args ───────────────────────────────────────────
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                print_usage();
                return ExitCode::from(1);
            }
            other => {
                eprintln!("error: unexpected argument: {}", other);
                print_usage();
                return ExitCode::from(1);
            }
        }
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

    // Send trust request
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("trust", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to serialize result: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human mode: parse and render
                match serde_json::from_value::<TrustResponse>(result) {
                    Ok(response) => {
                        println!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse trust response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed");
                eprintln!("hint: run 'rmap index .' to index this repo");
            } else {
                eprintln!("error: {}: {}", code, message);
            }
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
