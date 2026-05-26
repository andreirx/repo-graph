//! Boundaries list command.
//!
//! RS-MG-12b: Boundary catalog with filters.
//! CLI-OUT-4: Human-readable output with `--json` for machine mode.
//!
//! # REG-1 Contract
//!
//! Resolves repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

pub(super) fn run_boundaries_list(args: &[String]) -> ExitCode {
    // Parse filters and --json flag
    let (filters, json_mode) = match parse_list_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap boundaries list [--kind <kind>] [--scope <scope>] [--direction <dir>] [--family <fam>] [--file <path>] [--file-prefix <prefix>] [--symbol <key>] [--json]");
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
            if json_mode {
                // Machine mode: print full envelope
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
                // Human mode: parse and render (CLI-OUT-4)
                use crate::presentation::boundaries_list::BoundariesListResponse;
                match serde_json::from_value::<BoundariesListResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse boundaries list response: {}", e);
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

// ── Argument parsing ─────────────────────────────────────────────────────────

struct ListFilters {
    kind: Option<String>,
    scope: Option<String>,
    direction: Option<String>,
    family: Option<String>,
    file: Option<String>,
    file_prefix: Option<String>,
    symbol: Option<String>,
}

fn parse_list_args(args: &[String]) -> Result<(ListFilters, bool), String> {
    let mut filters = ListFilters {
        kind: None,
        scope: None,
        direction: None,
        family: None,
        file: None,
        file_prefix: None,
        symbol: None,
    };
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

    Ok((filters, json_mode))
}
