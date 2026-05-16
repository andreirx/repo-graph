//! Trust command family.
//!
//! Computes and outputs the trust report for a repository snapshot.
//!
//! # REG-1 Contract
//!
//! With REG-1, trust resolves repo from cwd via daemon.
//! New contract: `rmap trust`

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn print_usage() {
    eprintln!("usage: rmap trust");
    eprintln!();
    eprintln!("Computes and outputs the trust report for the repository.");
    eprintln!("Repository is resolved from current working directory.");
}

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

/// Run the `rmap trust` command.
///
/// Usage: `rmap trust`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed, computation failure)
pub fn run_trust(args: &[String]) -> ExitCode {
    // REG-1: no positional args - repo comes from cwd
    if !args.is_empty() {
        eprintln!("error: unexpected arguments");
        print_usage();
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

    // Send trust request
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("trust", Some(params)) {
        Ok(result) => {
            // Pretty-print JSON to stdout
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
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
