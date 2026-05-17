//! Modules show command.
//!
//! RS-MG-12c: Single module detail view with neighbors and violations.
//!
//! # REG-1 Contract
//!
//! Resolves repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_modules_show` handler
//! - argument parsing for show command
//! - output rendering for show command
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in daemon via module-queries)
//! - rollup computation (lives in daemon via classification)
//! - neighbor computation (lives in daemon via classification)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

// ── modules show command ─────────────────────────────────────────

pub(super) fn run_modules_show(args: &[String]) -> ExitCode {
    // Parse module argument (required)
    if args.is_empty() {
        eprintln!("error: missing module argument");
        eprintln!("usage: rmap modules show <module>");
        eprintln!();
        eprintln!("Module can be: canonical_root_path, module_key, or module_uid");
        eprintln!("Run from within a repo directory.");
        return ExitCode::from(1);
    }

    if args.len() > 1 {
        eprintln!("error: unexpected argument: {}", args[1]);
        eprintln!("usage: rmap modules show <module>");
        return ExitCode::from(1);
    }

    let module_ref = &args[0];

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
        "module": module_ref,
    });

    match client.request("modules_show", Some(params)) {
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
