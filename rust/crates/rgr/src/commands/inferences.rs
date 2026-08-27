//! Inferences command family.
//!
//! FD-SUPPORT-2: Query surface for Layer 3 inferences.
//!
//! # REG-1 Contract
//!
//! All subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.

use std::process::ExitCode;

use crate::commands::inferences_render;
use crate::daemon_client::DaemonClient;

// ── inferences command ───────────────────────────────────────────

fn print_list_usage() {
    eprintln!("usage: rmap inferences list [--kind <kind>] [--limit <N>] [--json]");
    eprintln!("  Layer-3 framework inferences (React components/hooks, Spring beans).");
    eprintln!("  Default: grouped summary (what was inferred, by which detectors).");
    eprintln!("  --limit N: compact per-record detail (up to N; truncation is stated).");
    eprintln!("  --json:    machine contract (full records unless --limit).");
}

pub fn run_inferences(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_list_usage();
        eprintln!();
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_inferences_list(&args[1..]),
        other => {
            eprintln!("unknown inferences subcommand: {}", other);
            print_list_usage();
            ExitCode::from(1)
        }
    }
}

// ── inferences list command ──────────────────────────────────────

fn run_inferences_list(args: &[String]) -> ExitCode {
    let mut kind_filter: Option<String> = None;
    let mut limit: Option<u64> = None;
    let mut json_mode = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_mode = true;
                i += 1;
            }
            "--kind" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --kind requires a value");
                    return ExitCode::from(1);
                }
                kind_filter = Some(args[i + 1].clone());
                i += 2;
            }
            "--limit" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --limit requires a value");
                    return ExitCode::from(1);
                }
                match args[i + 1].parse::<u64>() {
                    Ok(n) => limit = Some(n),
                    Err(_) => {
                        eprintln!("error: --limit must be a non-negative integer");
                        return ExitCode::from(1);
                    }
                }
                i += 2;
            }
            "--help" | "-h" => {
                print_list_usage();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("error: unknown option: {}", other);
                print_list_usage();
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

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if let Some(kind) = kind_filter {
        params["kind"] = serde_json::json!(kind);
    }
    if let Some(n) = limit {
        params["limit"] = serde_json::json!(n);
    }

    match client.request("inferences_list", Some(params)) {
        Ok(result) => {
            if json_mode {
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
                print!("{}", inferences_render::render(&result, limit.is_some()));
                ExitCode::SUCCESS
            }
        }
        // Preserve the original error surface (behaviour-preserving): the daemon's
        // Display already carries the daemon/not-indexed context. This slice is a
        // SURFACE change to the success payload, not the error contract.
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
