//! Modules deps command.
//!
//! RS-MG-12b: Shows module dependency edges (import-based).
//! CLI-OUT-4: Human-readable output with `--json` for machine mode.
//!
//! # REG-1 Contract
//!
//! Resolves repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_modules_deps` handler
//! - argument parsing for deps command
//! - mode switching (human vs --json)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (belongs in daemon via module-queries)
//! - edge derivation (belongs in daemon via classification)
//! - human output rendering (lives in `presentation::modules_deps`)

use std::process::ExitCode;

use crate::daemon_client::{daemon_unavailable_message, DaemonClient};

// ── modules deps command ─────────────────────────────────────────
//
// `rmap modules deps [module] [--outbound|--inbound] [--json]`
//
// Human mode (default): plain text with dependency summary.
// Machine mode (--json): full envelope.

pub(super) fn run_modules_deps(args: &[String]) -> ExitCode {
    // Parse args: [module] [--outbound|--inbound] [--json]
    let (module_filter, direction, json_mode) = match parse_deps_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap modules deps [module] [--outbound|--inbound] [--json]");
            eprintln!();
            eprintln!("Run from within a repo directory.");
            return ExitCode::from(1);
        }
    };

    // Direction flag requires module filter
    if direction != "all" && module_filter.is_none() {
        eprintln!("error: --outbound and --inbound require a module argument");
        eprintln!("usage: rmap modules deps <module> --outbound");
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
        eprintln!(
            "{}",
            daemon_unavailable_message(client.socket_path(), "modules deps")
        );
        return ExitCode::from(2);
    }

    // Build request params
    let mut params = serde_json::json!({
        "repo": repo_path,
        "direction": direction,
    });

    if let Some(ref module) = module_filter {
        params["module"] = serde_json::json!(module);
    }

    match client.request("modules_deps", Some(params)) {
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
                use crate::presentation::modules_deps::ModulesDepsResponse;
                match serde_json::from_value::<ModulesDepsResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse modules deps response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => {
            // Check for "module not found" error (exit 1, not 2)
            let err_str = e.to_string();
            if err_str.contains("module not found") {
                eprintln!("error: {}", err_str);
                return ExitCode::from(1);
            }
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

/// Parse [module] [--outbound|--inbound] [--json] args.
///
/// Returns (module_filter, direction_string, json_mode).
fn parse_deps_args(args: &[String]) -> Result<(Option<String>, &'static str, bool), String> {
    let mut module_filter = None;
    let mut direction: &'static str = "all";
    let mut direction_set = false;
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            "--outbound" => {
                if direction_set {
                    return Err("cannot specify both --outbound and --inbound".to_string());
                }
                direction = "outbound";
                direction_set = true;
            }
            "--inbound" => {
                if direction_set {
                    return Err("cannot specify both --outbound and --inbound".to_string());
                }
                direction = "inbound";
                direction_set = true;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {}", other));
            }
            _ => {
                if module_filter.is_some() {
                    return Err(format!("unexpected argument: {}", arg));
                }
                module_filter = Some(arg.clone());
            }
        }
    }

    Ok((module_filter, direction, json_mode))
}
