//! Resource command family (SB-5, SB-7A).
//!
//! Queries resource readers and writers from the graph.
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

/// Run the `rmap resource` command dispatcher.
///
/// Usage:
/// - `rmap resource list    [--kind <kind>]`
/// - `rmap resource readers <resource_stable_key>`
/// - `rmap resource writers <resource_stable_key>`
pub fn run_resource(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_resource_list(&args[1..]),
        "readers" => run_resource_readers(&args[1..]),
        "writers" => run_resource_writers(&args[1..]),
        other => {
            eprintln!("unknown resource subcommand: {}", other);
            print_usage();
            ExitCode::from(1)
        }
    }
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  rmap resource list    [--kind <kind>]");
    eprintln!("  rmap resource readers <resource_stable_key>");
    eprintln!("  rmap resource writers <resource_stable_key>");
    eprintln!();
    eprintln!("kinds: FS_PATH, DB_RESOURCE, BLOB, STATE");
    eprintln!();
    eprintln!("Run from within a repo directory.");
}

/// Run `rmap resource list`.
fn run_resource_list(args: &[String]) -> ExitCode {
    // Parse optional --kind filter
    let kind_filter = if args.len() >= 2 && args[0] == "--kind" {
        Some(args[1].as_str())
    } else if !args.is_empty() {
        eprintln!("usage: rmap resource list [--kind <kind>]");
        return ExitCode::from(1);
    } else {
        None
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

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path()));
        return ExitCode::from(2);
    }

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if let Some(kind) = kind_filter {
        params["kind"] = serde_json::json!(kind);
    }

    match client.request("resource_list", Some(params)) {
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

/// Run `rmap resource readers`.
fn run_resource_readers(args: &[String]) -> ExitCode {
    if args.len() != 1 {
        eprintln!("usage: rmap resource readers <resource_stable_key>");
        return ExitCode::from(1);
    }

    let resource_key = &args[0];

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

    // Send request
    let params = serde_json::json!({
        "repo": repo_path,
        "resource": resource_key,
    });

    match client.request("resource_readers", Some(params)) {
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

/// Run `rmap resource writers`.
fn run_resource_writers(args: &[String]) -> ExitCode {
    if args.len() != 1 {
        eprintln!("usage: rmap resource writers <resource_stable_key>");
        return ExitCode::from(1);
    }

    let resource_key = &args[0];

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

    // Send request
    let params = serde_json::json!({
        "repo": repo_path,
        "resource": resource_key,
    });

    match client.request("resource_writers", Some(params)) {
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
