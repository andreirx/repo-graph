//! Docs command family.
//!
//! Documentation discovery and semantic fact extraction:
//! - `list` — documentation inventory (primary surface)
//! - `extract` — semantic fact extraction (secondary hints)
//!
//! Docs are primary; semantic_facts are secondary derived hints.
//!
//! # REG-1 Contract
//!
//! Both subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Boundary rules
//!
//! This module owns docs command-family behavior:
//! - command handlers
//! - daemon request dispatch
//!
//! This module does **not** own:
//! - doc discovery (lives in `repo-graph-doc-facts` via daemon)
//! - semantic fact storage (lives in `repo-graph-storage` via daemon)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

/// Dispatcher for `rmap docs <subcommand>`.
pub fn run_docs(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_docs_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_docs_list(&args[1..]),
        "extract" => run_docs_extract(&args[1..]),
        other => {
            eprintln!("unknown docs subcommand: {}", other);
            print_docs_usage();
            ExitCode::from(1)
        }
    }
}

fn print_docs_usage() {
    eprintln!("usage:");
    eprintln!("  rmap docs list     — documentation inventory (run from repo)");
    eprintln!("  rmap docs extract  — extract semantic hints (run from repo)");
}

/// List documentation inventory (primary documentation surface).
///
/// REG-1 contract: resolves repo from cwd via daemon.
fn run_docs_list(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: rmap docs list");
        eprintln!("       (run from within a repo directory)");
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

    // Send docs_list request with repo path
    let params = serde_json::json!({ "repo": repo_path });
    match client.request("docs_list", Some(params)) {
        Ok(result) => {
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

/// Extract semantic facts from documentation (secondary hints).
///
/// REG-1 contract: resolves repo from cwd via daemon.
fn run_docs_extract(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: rmap docs extract");
        eprintln!("       (run from within a repo directory)");
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

    // Send docs_extract request with repo path
    let params = serde_json::json!({ "repo": repo_path });
    match client.request("docs_extract", Some(params)) {
        Ok(result) => {
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
