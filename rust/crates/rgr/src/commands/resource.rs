//! Resource command family (SB-5, SB-7A).
//!
//! Queries resource readers and writers from the graph.
//!
//! # REG-1 Contract
//!
//! All subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # CLI-OUT-5 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw daemon response)
//! - Deterministic ordering (by kind+name for list, by file+line for readers/writers)
//! - Full output, no truncation

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;
use crate::presentation::resources::{
    AccessDirection, ResourceAccessResponse, ResourceListResponse,
};

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

/// Run the `rmap resource` command dispatcher.
///
/// Usage:
/// - `rmap resource list    [--kind <kind>] [--json]`
/// - `rmap resource readers <resource_stable_key> [--json]`
/// - `rmap resource writers <resource_stable_key> [--json]`
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
    eprintln!("  rmap resource list    [--kind <kind>] [--json]");
    eprintln!("  rmap resource readers <resource_stable_key> [--json]");
    eprintln!("  rmap resource writers <resource_stable_key> [--json]");
    eprintln!();
    eprintln!("kinds: FS_PATH, DB_RESOURCE, BLOB, STATE");
    eprintln!();
    eprintln!("Run from within a repo directory.");
}

/// Parse --json flag and --kind filter from args.
fn parse_list_args(args: &[String]) -> (bool, Option<String>, Vec<String>) {
    let mut json_mode = false;
    let mut kind_filter: Option<String> = None;
    let mut remaining = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_mode = true;
                i += 1;
            }
            "--kind" => {
                if i + 1 < args.len() {
                    kind_filter = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    remaining.push(args[i].clone());
                    i += 1;
                }
            }
            _ => {
                remaining.push(args[i].clone());
                i += 1;
            }
        }
    }

    (json_mode, kind_filter, remaining)
}

/// Parse --json flag from args for readers/writers.
fn parse_access_args(args: &[String]) -> (bool, Vec<String>) {
    let mut json_mode = false;
    let mut remaining = Vec::new();

    for arg in args {
        if arg == "--json" {
            json_mode = true;
        } else {
            remaining.push(arg.clone());
        }
    }

    (json_mode, remaining)
}

/// Run `rmap resource list`.
fn run_resource_list(args: &[String]) -> ExitCode {
    let (json_mode, kind_filter, remaining) = parse_list_args(args);

    if !remaining.is_empty() {
        eprintln!("usage: rmap resource list [--kind <kind>] [--json]");
        return ExitCode::from(1);
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

    match client.request("resource_list", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Raw JSON output
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
                // Human-readable output
                match serde_json::from_value::<ResourceListResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse response: {}", e);
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

/// Run `rmap resource readers`.
fn run_resource_readers(args: &[String]) -> ExitCode {
    let (json_mode, remaining) = parse_access_args(args);

    if remaining.len() != 1 {
        eprintln!("usage: rmap resource readers <resource_stable_key> [--json]");
        return ExitCode::from(1);
    }

    let resource_key = &remaining[0];
    run_resource_access(
        resource_key,
        "resource_readers",
        AccessDirection::Readers,
        json_mode,
    )
}

/// Run `rmap resource writers`.
fn run_resource_writers(args: &[String]) -> ExitCode {
    let (json_mode, remaining) = parse_access_args(args);

    if remaining.len() != 1 {
        eprintln!("usage: rmap resource writers <resource_stable_key> [--json]");
        return ExitCode::from(1);
    }

    let resource_key = &remaining[0];
    run_resource_access(
        resource_key,
        "resource_writers",
        AccessDirection::Writers,
        json_mode,
    )
}

/// Shared implementation for readers/writers.
fn run_resource_access(
    resource_key: &str,
    method: &str,
    direction: AccessDirection,
    json_mode: bool,
) -> ExitCode {
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

    match client.request(method, Some(params)) {
        Ok(result) => {
            if json_mode {
                // Raw JSON output
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
                // Human-readable output
                match serde_json::from_value::<ResourceAccessResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human(direction));
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse response: {}", e);
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
