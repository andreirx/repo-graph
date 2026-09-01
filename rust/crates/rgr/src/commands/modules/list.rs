//! Modules list command.
//!
//! RS-MG-12b: Module list with rollup statistics.
//! Phase 3.1: Module sanity metrics for trust surface.
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
//! - `run_modules_list` handler
//! - argument parsing for list command
//! - mode switching (human vs --json)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in daemon via module-queries)
//! - rollup computation (lives in daemon via classification)
//! - sanity metrics computation (lives in daemon)
//! - human output rendering (lives in `presentation::modules_list`)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

// ── modules list command ─────────────────────────────────────────
//
// `rmap modules list [--json]`
//
// Human mode (default): plain text with module catalog.
// Machine mode (--json): full envelope.

pub(super) fn run_modules_list(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json / --full) ─────────────────
    let mut json_mode = false;
    // MODULE-EDGES-1 §2.1: `--full` uncaps the cross-module edge list (default
    // budgets it with an honest "(+N more — --full)"); the COMPLETE set always rides
    // `--json`.
    let mut full = false;
    let mut unexpected: Option<&String> = None;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            "--full" => {
                full = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap modules list [--json] [--full]");
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
        eprintln!("usage: rmap modules list [--json] [--full]");
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

    // Build request params
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("modules_list", Some(params)) {
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
                use crate::presentation::modules_list::ModulesListResponse;
                match serde_json::from_value::<ModulesListResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human_budgeted(full));
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse modules list response: {}", e);
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
