//! Modules unowned command.
//!
//! RS-MG-12b: Lists source files that are not assigned to any module.
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
//! - `run_modules_unowned` handler
//! - argument parsing for unowned command
//! - mode switching (human vs --json)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - unowned classification logic (lives in daemon - TECH DEBT: should be in shared module)
//! - human output rendering (lives in `presentation::module_inventory`)

use std::process::ExitCode;

use crate::daemon_client::{daemon_unavailable_message, DaemonClient};

// ── modules unowned command ──────────────────────────────────────────
//
// `rmap modules unowned [--json]`
//
// Human mode (default): plain text with unowned file inventory grouped by reason.
// Machine mode (--json): full envelope.

pub(super) fn run_modules_unowned(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json) ──────────────────────────
    let mut json_mode = false;
    let mut unexpected: Option<&String> = None;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap modules unowned [--json]");
                return ExitCode::from(1);
            }
            _ => {
                if unexpected.is_none() {
                    unexpected = Some(arg);
                }
            }
        }
    }

    if let Some(arg) = unexpected {
        eprintln!("error: unexpected argument: {}", arg);
        eprintln!("usage: rmap modules unowned [--json]");
        eprintln!();
        eprintln!("Run from within a repo directory.");
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
            daemon_unavailable_message(client.socket_path(), "modules unowned")
        );
        return ExitCode::from(2);
    }

    // Build request params
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("modules_unowned", Some(params)) {
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
                use crate::presentation::module_inventory::ModulesUnownedResponse;
                match serde_json::from_value::<ModulesUnownedResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse modules unowned response: {}", e);
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
