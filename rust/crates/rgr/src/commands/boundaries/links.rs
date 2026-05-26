//! Boundaries links command.
//!
//! Service link discovery.
//!
//! # Out of CLI-OUT-4 Scope
//!
//! This command is preserved from the original boundaries.rs but is
//! explicitly NOT part of CLI-OUT-4 Group 5. It retains JSON-only output
//! until a future slice addresses it.
//!
//! # REG-1 Contract
//!
//! Resolves repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

pub(super) fn run_boundaries_links(args: &[String]) -> ExitCode {
    // Parse filters
    let service = match parse_links_filters(args) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap boundaries links [--service <name>]");
            return ExitCode::from(1);
        }
    };

    // Get cwd for repo resolution
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {}", e);
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

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if let Some(s) = service {
        params["service"] = serde_json::json!(s);
    }

    match client.request("boundaries_links", Some(params)) {
        Ok(result) => match serde_json::to_string_pretty(&result) {
            Ok(json) => {
                println!("{}", json);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: failed to serialize result: {}", e);
                ExitCode::from(2)
            }
        },
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── Argument parsing ─────────────────────────────────────────────────────────

fn parse_links_filters(args: &[String]) -> Result<Option<String>, String> {
    let mut service = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--service" => {
                if i + 1 >= args.len() {
                    return Err("--service requires a value".to_string());
                }
                service = Some(args[i + 1].clone());
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option: {}", other));
            }
            other => {
                return Err(format!("unexpected argument: {}", other));
            }
        }
    }

    Ok(service)
}
