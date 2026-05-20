//! Modules files command.
//!
//! RS-MG-12b: Files owned by a specific module.
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
//! - `run_modules_files` handler
//! - argument parsing for files command
//! - mode switching (human vs --json)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (belongs in daemon via module-queries)
//! - file ownership queries (belongs in daemon via storage)
//! - human output rendering (lives in `presentation::module_inventory`)

use std::process::ExitCode;

use crate::daemon_client::{daemon_unavailable_message, DaemonClient};

// ── modules files command ────────────────────────────────────────
//
// `rmap modules files <module> [--json]`
//
// Human mode (default): plain text with file inventory.
// Machine mode (--json): full envelope.

pub(super) fn run_modules_files(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json) ──────────────────────────
    let mut json_mode = false;
    let mut positional: Vec<&String> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap modules files <module> [--json]");
                return ExitCode::from(1);
            }
            _ => {
                positional.push(arg);
            }
        }
    }

    // Require exactly one positional argument (module ref)
    if positional.is_empty() {
        eprintln!("error: missing module argument");
        eprintln!("usage: rmap modules files <module> [--json]");
        eprintln!();
        eprintln!("Module can be: canonical_root_path, module_key, or module_uid");
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(1);
    }

    if positional.len() > 1 {
        eprintln!("error: unexpected argument: {}", positional[1]);
        eprintln!("usage: rmap modules files <module> [--json]");
        return ExitCode::from(1);
    }

    let module_ref = positional[0];

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
            daemon_unavailable_message(client.socket_path(), "modules files")
        );
        return ExitCode::from(2);
    }

    // Build request params
    let params = serde_json::json!({
        "repo": repo_path,
        "module": module_ref,
    });

    match client.request("modules_files", Some(params)) {
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
                use crate::presentation::module_inventory::ModulesFilesResponse;
                match serde_json::from_value::<ModulesFilesResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse modules files response: {}", e);
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
