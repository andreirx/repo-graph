//! Surfaces command family.
//!
//! Project surface discovery and inspection.
//!
//! # REG-1 Contract
//!
//! All subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Commands
//!
//! - `rmap surfaces list [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>] [--json]`
//! - `rmap surfaces show <surface_ref> [--json]`
//!
//! CLI-OUT-4: Human-readable output with `--json` for machine mode.
//!
//! # Boundary rules
//!
//! This module owns surfaces command-family behavior:
//! - `run_surfaces`, `run_surfaces_list`, `run_surfaces_show` handlers
//! - surfaces-family argument parsing
//! - surfaces-family output rendering
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

// ── surfaces command ─────────────────────────────────────────────

pub fn run_surfaces(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage:");
        eprintln!(
            "  rmap surfaces list [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>] [--json]"
        );
        eprintln!("  rmap surfaces show <surface_ref> [--json]");
        eprintln!();
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_surfaces_list(&args[1..]),
        "show" => run_surfaces_show(&args[1..]),
        other => {
            eprintln!("unknown surfaces subcommand: {}", other);
            eprintln!("usage:");
            eprintln!("  rmap surfaces list [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>] [--json]");
            eprintln!("  rmap surfaces show <surface_ref> [--json]");
            ExitCode::from(1)
        }
    }
}

// ── surfaces list command ────────────────────────────────────────

fn run_surfaces_list(args: &[String]) -> ExitCode {
    // Parse filters and --json flag
    let (kind, runtime, source, module, json_mode) = match parse_surfaces_list_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap surfaces list [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>] [--json]");
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
    if let Some(k) = kind {
        params["kind"] = serde_json::json!(k);
    }
    if let Some(r) = runtime {
        params["runtime"] = serde_json::json!(r);
    }
    if let Some(s) = source {
        params["source"] = serde_json::json!(s);
    }
    if let Some(m) = module {
        params["module"] = serde_json::json!(m);
    }

    match client.request("surfaces_list", Some(params)) {
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
                use crate::presentation::surfaces::SurfacesListResponse;
                match serde_json::from_value::<SurfacesListResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse surfaces list response: {}", e);
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

// ── surfaces show command ────────────────────────────────────────

fn run_surfaces_show(args: &[String]) -> ExitCode {
    // Parse args: <surface_ref> [--json]
    let (surface_ref, json_mode) = match parse_surfaces_show_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap surfaces show <surface_ref> [--json]");
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
    let params = serde_json::json!({
        "repo": repo_path,
        "surface": surface_ref,
    });

    match client.request("surfaces_show", Some(params)) {
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
                use crate::presentation::surfaces::SurfacesShowResponse;
                match serde_json::from_value::<SurfacesShowResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse surfaces show response: {}", e);
                        ExitCode::from(2)
                    }
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

// ── Argument parsing helpers ──────────────────────────────────────

/// Parsed filters for surfaces list: (kind, runtime, source, module, json_mode)
type SurfacesListFilters = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
);

fn parse_surfaces_list_args(args: &[String]) -> Result<SurfacesListFilters, String> {
    let mut kind = None;
    let mut runtime = None;
    let mut source = None;
    let mut module = None;
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
                kind = Some(args[i + 1].clone());
                i += 2;
            }
            "--runtime" => {
                if i + 1 >= args.len() {
                    return Err("--runtime requires a value".to_string());
                }
                runtime = Some(args[i + 1].clone());
                i += 2;
            }
            "--source" => {
                if i + 1 >= args.len() {
                    return Err("--source requires a value".to_string());
                }
                source = Some(args[i + 1].clone());
                i += 2;
            }
            "--module" => {
                if i + 1 >= args.len() {
                    return Err("--module requires a value".to_string());
                }
                module = Some(args[i + 1].clone());
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

    Ok((kind, runtime, source, module, json_mode))
}

/// Parsed args for surfaces show: (surface_ref, json_mode)
fn parse_surfaces_show_args(args: &[String]) -> Result<(String, bool), String> {
    let mut surface_ref: Option<String> = None;
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option: {}", other));
            }
            _ => {
                if surface_ref.is_some() {
                    return Err(format!("unexpected argument: {}", arg));
                }
                surface_ref = Some(arg.clone());
            }
        }
    }

    match surface_ref {
        Some(ref_str) => Ok((ref_str, json_mode)),
        None => Err("missing surface reference".to_string()),
    }
}
