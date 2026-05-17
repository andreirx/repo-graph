//! Unowned files analysis command (REG-1).
//!
//! Lists source files that are not assigned to any module, grouped by reason.
//! This is a diagnostic command for understanding ownership gaps.
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
//! - modules unowned output rendering
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - unowned classification logic (lives in daemon - TECH DEBT: should be in shared module)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}

// ── modules unowned command ──────────────────────────────────────────

pub(super) fn run_modules_unowned(args: &[String]) -> ExitCode {
    // Check for unexpected args
    if !args.is_empty() {
        eprintln!("error: unexpected argument: {}", args[0]);
        eprintln!("usage: rmap modules unowned");
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
        eprintln!("{}", daemon_unavailable_message(client.socket_path()));
        return ExitCode::from(2);
    }

    // Build request params
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("modules_unowned", Some(params)) {
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
