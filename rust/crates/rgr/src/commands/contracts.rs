//! Contracts command family.
//!
//! Contract schema discovery and inspection (CS-1+).
//!
//! # REG-1 Contract
//!
//! All subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Boundary rules
//!
//! This module owns contracts command-family behavior:
//! - command handlers
//! - daemon request dispatch
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - storage queries (belongs in storage crate via daemon)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

// ── contracts command ────────────────────────────────────────────

pub fn run_contracts(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_contracts_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_contracts_list(&args[1..]),
        "show" => run_contracts_show(&args[1..]),
        "elements" => run_contracts_elements(&args[1..]),
        "usages" => run_contracts_usages(&args[1..]),
        other => {
            eprintln!("unknown contracts subcommand: {}", other);
            print_contracts_usage();
            ExitCode::from(1)
        }
    }
}

fn print_contracts_usage() {
    eprintln!("usage:");
    eprintln!("  rmap contracts list [--kind protobuf]");
    eprintln!("  rmap contracts show <file_path>");
    eprintln!(
        "  rmap contracts elements [--kind message|enum|service|method|field] [--file <path>]"
    );
    eprintln!("  rmap contracts usages [--element <element_uid>] [--min-confidence <0.0-1.0>]");
    eprintln!();
    eprintln!("Run from within a repo directory.");
}

// ── contracts list command ───────────────────────────────────────

fn run_contracts_list(args: &[String]) -> ExitCode {
    // Parse optional --kind filter
    let mut kind_filter: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--kind" {
            if i + 1 >= args.len() {
                eprintln!("--kind requires a value");
                return ExitCode::from(1);
            }
            kind_filter = Some(args[i + 1].clone());
            i += 2;
        } else {
            eprintln!("unknown option: {}", args[i]);
            return ExitCode::from(1);
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

    match client.request("contracts_list", Some(params)) {
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

// ── contracts show command ───────────────────────────────────────

fn run_contracts_show(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: rmap contracts show <file_path>");
        return ExitCode::from(1);
    }

    let file_path = &args[0];

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

    // Send request
    let params = serde_json::json!({
        "repo": repo_path,
        "file": file_path,
    });

    match client.request("contracts_show", Some(params)) {
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

// ── contracts elements command ───────────────────────────────────

fn run_contracts_elements(args: &[String]) -> ExitCode {
    // Parse optional filters
    let mut kind_filter: Option<String> = None;
    let mut file_filter: Option<String> = None;
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
            "--file" => {
                if i + 1 >= args.len() {
                    eprintln!("--file requires a value");
                    return ExitCode::from(1);
                }
                file_filter = Some(args[i + 1].clone());
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

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if let Some(kind) = kind_filter {
        params["kind"] = serde_json::json!(kind);
    }
    if let Some(file) = file_filter {
        params["file"] = serde_json::json!(file);
    }

    match client.request("contracts_elements", Some(params)) {
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

// ── contracts usages command ─────────────────────────────────────

fn run_contracts_usages(args: &[String]) -> ExitCode {
    // Parse optional filters
    let mut element_filter: Option<String> = None;
    let mut min_confidence: Option<f64> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--element" => {
                if i + 1 >= args.len() {
                    eprintln!("--element requires a value");
                    return ExitCode::from(1);
                }
                element_filter = Some(args[i + 1].clone());
                i += 2;
            }
            "--min-confidence" => {
                if i + 1 >= args.len() {
                    eprintln!("--min-confidence requires a value");
                    return ExitCode::from(1);
                }
                match args[i + 1].parse::<f64>() {
                    Ok(v) if (0.0..=1.0).contains(&v) => min_confidence = Some(v),
                    _ => {
                        eprintln!("--min-confidence must be a number between 0.0 and 1.0");
                        return ExitCode::from(1);
                    }
                }
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

    // Build request params
    let mut params = serde_json::json!({ "repo": repo_path });
    if let Some(element) = element_filter {
        params["element"] = serde_json::json!(element);
    }
    if let Some(conf) = min_confidence {
        params["min_confidence"] = serde_json::json!(conf);
    }

    match client.request("contracts_usages", Some(params)) {
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
