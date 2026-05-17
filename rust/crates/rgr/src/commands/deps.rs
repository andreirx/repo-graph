//! Package dependency reconciliation commands (DEP-1).
//!
//! Reconciles declared dependencies (from manifests) with observed
//! external references (from imports) to produce module-level
//! dependency summaries.
//!
//! # REG-1 Contract
//!
//! All subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Commands
//!
//! - `rmap deps list [module] [--ecosystem npm|cargo]` — list dependencies
//! - `rmap deps why <package> [--ecosystem npm|cargo]` — explain why a package is used
//! - `rmap deps drift [--ecosystem npm|cargo]` — show dependency drift anomalies
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_deps` handler and subcommand dispatch
//! - CLI argument parsing and error reporting
//!
//! This module does **not** own:
//! - Reconciliation logic (lives in daemon via module-queries)
//! - Storage queries (lives in daemon)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

/// Entry point for `rmap deps` command family.
pub fn run_deps(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage:");
        eprintln!("  rmap deps list [module] [--ecosystem npm|cargo]");
        eprintln!("  rmap deps why <package> [--ecosystem npm|cargo]");
        eprintln!("  rmap deps drift [--ecosystem npm|cargo]");
        eprintln!();
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_deps_list(&args[1..]),
        "why" => run_deps_why(&args[1..]),
        "drift" => run_deps_drift(&args[1..]),
        other => {
            eprintln!("unknown deps subcommand: {}", other);
            eprintln!("subcommands: list, why, drift");
            ExitCode::from(1)
        }
    }
}

// ── deps list command ─────────────────────────────────────────────

fn run_deps_list(args: &[String]) -> ExitCode {
    // Parse args: [module] [--ecosystem npm|cargo]
    let (module_filter, ecosystem) = match parse_deps_list_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap deps list [module] [--ecosystem npm|cargo]");
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

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path()));
        return ExitCode::from(2);
    }

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if let Some(module) = module_filter {
        params["module"] = serde_json::json!(module);
    }
    if ecosystem != "npm" {
        params["ecosystem"] = serde_json::json!(ecosystem);
    }

    match client.request("deps_list", Some(params)) {
        Ok(result) => {
            // Check if module filter was used and no results returned
            if let Some(count) = result.get("count").and_then(|c| c.as_u64()) {
                if count == 0 {
                    if let Some(serde_json::Value::String(filter)) = result.get("module_filter") {
                        eprintln!("error: no dependencies found for module '{}'", filter);
                        eprintln!("hint: use canonical path (e.g., 'packages/app') or check rmap modules list");
                        return ExitCode::from(1);
                    }
                }
            }

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

// ── deps why command ──────────────────────────────────────────────

fn run_deps_why(args: &[String]) -> ExitCode {
    // Parse args: <package> [--ecosystem npm|cargo]
    let (package, ecosystem) = match parse_deps_why_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap deps why <package> [--ecosystem npm|cargo]");
            return ExitCode::from(1);
        }
    };

    let package = match package {
        Some(p) => p,
        None => {
            eprintln!("usage: rmap deps why <package> [--ecosystem npm|cargo]");
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

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path()));
        return ExitCode::from(2);
    }

    // Build request params
    let mut params = serde_json::json!({
        "repo": repo_path,
        "package": package,
    });
    if ecosystem != "npm" {
        params["ecosystem"] = serde_json::json!(ecosystem);
    }

    match client.request("deps_why", Some(params)) {
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
            // Check if it's a "not found" error
            let err_str = e.to_string();
            if err_str.contains("not found") {
                eprintln!("error: {}", err_str);
                eprintln!("hint: check package name or try rmap deps list to see all packages");
                return ExitCode::from(1);
            }
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── deps drift command ────────────────────────────────────────────

fn run_deps_drift(args: &[String]) -> ExitCode {
    // Parse args: [--ecosystem npm|cargo]
    let ecosystem = match parse_deps_drift_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap deps drift [--ecosystem npm|cargo]");
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

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path()));
        return ExitCode::from(2);
    }

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if ecosystem != "npm" {
        params["ecosystem"] = serde_json::json!(ecosystem);
    }

    match client.request("deps_drift", Some(params)) {
        Ok(result) => {
            let drift_count = result.get("count").and_then(|c| c.as_u64()).unwrap_or(0);

            match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    println!("{}", json);
                    // Exit with 1 if there are drift anomalies (matches gate behavior)
                    if drift_count > 0 {
                        ExitCode::from(1)
                    } else {
                        ExitCode::SUCCESS
                    }
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

// ── Argument parsing helpers ──────────────────────────────────────

fn parse_deps_list_args(args: &[String]) -> Result<(Option<String>, String), String> {
    let mut module_filter = None;
    let mut ecosystem = "npm".to_string();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--ecosystem" => {
                if i + 1 >= args.len() {
                    return Err("--ecosystem requires a value (npm or cargo)".to_string());
                }
                ecosystem = args[i + 1].clone();
                if ecosystem != "npm" && ecosystem != "cargo" {
                    return Err(format!(
                        "invalid ecosystem: {} (expected npm or cargo)",
                        ecosystem
                    ));
                }
                i += 2;
            }
            "--format" => {
                if i + 1 >= args.len() {
                    return Err("--format requires a value (json)".to_string());
                }
                let format = &args[i + 1];
                if format != "json" {
                    return Err(format!(
                        "unsupported format: {} (only json is supported)",
                        format
                    ));
                }
                // JSON is the default and only format, so this is a no-op
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {}", other));
            }
            _ => {
                // First positional arg is module filter
                if module_filter.is_none() {
                    module_filter = Some(args[i].clone());
                } else {
                    return Err(format!("unexpected argument: {}", args[i]));
                }
                i += 1;
            }
        }
    }

    Ok((module_filter, ecosystem))
}

fn parse_deps_why_args(args: &[String]) -> Result<(Option<String>, String), String> {
    let mut package = None;
    let mut ecosystem = "npm".to_string();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--ecosystem" => {
                if i + 1 >= args.len() {
                    return Err("--ecosystem requires a value (npm or cargo)".to_string());
                }
                ecosystem = args[i + 1].clone();
                if ecosystem != "npm" && ecosystem != "cargo" {
                    return Err(format!(
                        "invalid ecosystem: {} (expected npm or cargo)",
                        ecosystem
                    ));
                }
                i += 2;
            }
            "--format" => {
                if i + 1 >= args.len() {
                    return Err("--format requires a value (json)".to_string());
                }
                let format = &args[i + 1];
                if format != "json" {
                    return Err(format!(
                        "unsupported format: {} (only json is supported)",
                        format
                    ));
                }
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {}", other));
            }
            _ => {
                // First positional arg is package
                if package.is_none() {
                    package = Some(args[i].clone());
                } else {
                    return Err(format!("unexpected argument: {}", args[i]));
                }
                i += 1;
            }
        }
    }

    Ok((package, ecosystem))
}

fn parse_deps_drift_args(args: &[String]) -> Result<String, String> {
    let mut ecosystem = "npm".to_string();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--ecosystem" => {
                if i + 1 >= args.len() {
                    return Err("--ecosystem requires a value (npm or cargo)".to_string());
                }
                ecosystem = args[i + 1].clone();
                if ecosystem != "npm" && ecosystem != "cargo" {
                    return Err(format!(
                        "invalid ecosystem: {} (expected npm or cargo)",
                        ecosystem
                    ));
                }
                i += 2;
            }
            "--format" => {
                if i + 1 >= args.len() {
                    return Err("--format requires a value (json)".to_string());
                }
                let format = &args[i + 1];
                if format != "json" {
                    return Err(format!(
                        "unsupported format: {} (only json is supported)",
                        format
                    ));
                }
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {}", other));
            }
            other => {
                return Err(format!("unexpected argument: {}", other));
            }
        }
    }

    Ok(ecosystem)
}
