//! Modules files command.
//!
//! Lists files owned by a specific module.
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
//! - modules files argument parsing
//! - modules files output rendering
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (belongs in daemon via module-queries)
//! - file ownership queries (belongs in daemon via storage)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

// ── modules files command ────────────────────────────────────────

pub(super) fn run_modules_files(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: rmap modules files <module>");
        eprintln!();
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(1);
    }

    let module_arg = &args[0];

    // Check for unexpected args
    if args.len() > 1 {
        eprintln!("error: unexpected argument: {}", args[1]);
        eprintln!("usage: rmap modules files <module>");
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
        "module": module_arg,
    });

    match client.request("modules_files", Some(params)) {
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
            // Check for "module not found" error
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
