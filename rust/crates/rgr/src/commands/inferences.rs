//! Inferences command family.
//!
//! FD-SUPPORT-2: Query surface for Layer 3 inferences.
//!
//! # REG-1 Contract
//!
//! All subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

// ── inferences command ───────────────────────────────────────────

pub fn run_inferences(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage:");
        eprintln!("  rmap inferences list [--kind <kind>]");
        eprintln!();
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_inferences_list(&args[1..]),
        other => {
            eprintln!("unknown inferences subcommand: {}", other);
            eprintln!("usage:");
            eprintln!("  rmap inferences list [--kind <kind>]");
            ExitCode::from(1)
        }
    }
}

// ── inferences list command ──────────────────────────────────────

fn run_inferences_list(args: &[String]) -> ExitCode {
    // Parse optional --kind filter
    let mut kind_filter: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" => {
                if i + 1 >= args.len() {
                    eprintln!("--kind requires a value");
                    return ExitCode::from(1);
                }
                kind_filter = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                eprintln!("unknown option: {}", other);
                return ExitCode::from(1);
            }
        }
    }

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

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path()));
        return ExitCode::from(2);
    }

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if let Some(kind) = kind_filter {
        params["kind"] = serde_json::json!(kind);
    }

    match client.request("inferences_list", Some(params)) {
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
