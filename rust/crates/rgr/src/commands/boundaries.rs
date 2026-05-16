//! Boundaries command family.
//!
//! Boundary interaction discovery and inspection.
//!
//! # REG-1 Contract
//!
//! All subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Commands
//!
//! - `rmap boundaries list [--kind <kind>] [--scope <scope>] [--direction <dir>] [--family <fam>] [--file <path>] [--file-prefix <prefix>] [--symbol <key>]`
//! - `rmap boundaries show <surface_uid>`
//! - `rmap boundaries summary`
//! - `rmap boundaries links [--service <name>]`
//!
//! # Boundary rules
//!
//! This module owns boundaries command-family behavior:
//! - `run_boundaries`, `run_boundaries_list`, `run_boundaries_show`, etc.
//! - boundaries-family argument parsing
//! - boundaries-family output rendering
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - storage queries (belongs in daemon)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

// ── boundaries command ───────────────────────────────────────────────

pub fn run_boundaries(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_boundaries_list(&args[1..]),
        "show" => run_boundaries_show(&args[1..]),
        "summary" => run_boundaries_summary(&args[1..]),
        "links" => run_boundaries_links(&args[1..]),
        other => {
            eprintln!("unknown boundaries subcommand: {}", other);
            print_usage();
            ExitCode::from(1)
        }
    }
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  rmap boundaries list [--kind <kind>] [--scope <scope>] [--direction <dir>] [--family <fam>] [--file <path>] [--file-prefix <prefix>] [--symbol <key>]");
    eprintln!("  rmap boundaries show <surface_uid>");
    eprintln!("  rmap boundaries summary");
    eprintln!("  rmap boundaries links [--service <name>]");
    eprintln!();
    eprintln!("Run from within a repo directory.");
}

// ── boundaries list ──────────────────────────────────────────────────

fn run_boundaries_list(args: &[String]) -> ExitCode {
    // Parse filters
    let filters = match parse_list_filters(args) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap boundaries list [--kind <kind>] [--scope <scope>] [--direction <dir>] [--family <fam>] [--file <path>] [--file-prefix <prefix>] [--symbol <key>]");
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
    if let Some(k) = filters.kind {
        params["kind"] = serde_json::json!(k);
    }
    if let Some(s) = filters.scope {
        params["scope"] = serde_json::json!(s);
    }
    if let Some(d) = filters.direction {
        params["direction"] = serde_json::json!(d);
    }
    if let Some(f) = filters.family {
        params["family"] = serde_json::json!(f);
    }
    if let Some(f) = filters.file {
        params["file"] = serde_json::json!(f);
    }
    if let Some(p) = filters.file_prefix {
        params["file_prefix"] = serde_json::json!(p);
    }
    if let Some(s) = filters.symbol {
        params["symbol"] = serde_json::json!(s);
    }

    match client.request("boundaries_list", Some(params)) {
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

struct ListFilters {
    kind: Option<String>,
    scope: Option<String>,
    direction: Option<String>,
    family: Option<String>,
    file: Option<String>,
    file_prefix: Option<String>,
    symbol: Option<String>,
}

fn parse_list_filters(args: &[String]) -> Result<ListFilters, String> {
    let mut filters = ListFilters {
        kind: None,
        scope: None,
        direction: None,
        family: None,
        file: None,
        file_prefix: None,
        symbol: None,
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" => {
                if i + 1 >= args.len() {
                    return Err("--kind requires a value".to_string());
                }
                filters.kind = Some(args[i + 1].clone());
                i += 2;
            }
            "--scope" => {
                if i + 1 >= args.len() {
                    return Err("--scope requires a value".to_string());
                }
                filters.scope = Some(args[i + 1].clone());
                i += 2;
            }
            "--direction" => {
                if i + 1 >= args.len() {
                    return Err("--direction requires a value".to_string());
                }
                filters.direction = Some(args[i + 1].clone());
                i += 2;
            }
            "--family" => {
                if i + 1 >= args.len() {
                    return Err("--family requires a value".to_string());
                }
                filters.family = Some(args[i + 1].clone());
                i += 2;
            }
            "--file" => {
                if i + 1 >= args.len() {
                    return Err("--file requires a value".to_string());
                }
                filters.file = Some(args[i + 1].clone());
                i += 2;
            }
            "--file-prefix" => {
                if i + 1 >= args.len() {
                    return Err("--file-prefix requires a value".to_string());
                }
                filters.file_prefix = Some(args[i + 1].clone());
                i += 2;
            }
            "--symbol" => {
                if i + 1 >= args.len() {
                    return Err("--symbol requires a value".to_string());
                }
                filters.symbol = Some(args[i + 1].clone());
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

    Ok(filters)
}

// ── boundaries show ──────────────────────────────────────────────────

fn run_boundaries_show(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: rmap boundaries show <surface_uid>");
        return ExitCode::from(1);
    }

    let surface_uid = &args[0];

    // Check for unexpected args
    if args.len() > 1 {
        eprintln!("error: unexpected argument: {}", args[1]);
        eprintln!("usage: rmap boundaries show <surface_uid>");
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
    let params = serde_json::json!({
        "repo": repo_path,
        "surface": surface_uid,
    });

    match client.request("boundaries_show", Some(params)) {
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
            // Check for "surface not found" error
            let err_str = e.to_string();
            if err_str.contains("not found") {
                eprintln!("error: {}", err_str);
                return ExitCode::from(1);
            }
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── boundaries summary ───────────────────────────────────────────────

fn run_boundaries_summary(args: &[String]) -> ExitCode {
    // Check for unexpected args
    if !args.is_empty() {
        eprintln!("error: unexpected argument: {}", args[0]);
        eprintln!("usage: rmap boundaries summary");
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
    let params = serde_json::json!({ "repo": repo_path });

    match client.request("boundaries_summary", Some(params)) {
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

// ── boundaries links ─────────────────────────────────────────────────

fn run_boundaries_links(args: &[String]) -> ExitCode {
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
            return ExitCode::from(2)
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
    if let Some(s) = service {
        params["service"] = serde_json::json!(s);
    }

    match client.request("boundaries_links", Some(params)) {
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
