//! Boundaries show command.
//!
//! RS-MG-12b: Single boundary detail view.
//! CLI-OUT-4: Human-readable output with `--json` for machine mode.
//!
//! # REG-1 Contract
//!
//! Resolves repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;

pub(super) fn run_boundaries_show(args: &[String]) -> ExitCode {
    // Parse args: <surface_uid> [--json]
    let (surface_uid, json_mode) = match parse_show_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap boundaries show <surface_uid> [--json]");
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
    let params = serde_json::json!({
        "repo": repo_path,
        "surface": surface_uid,
    });

    match client.request("boundaries_show", Some(params)) {
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
                use crate::presentation::boundaries_show::BoundariesShowResponse;
                match serde_json::from_value::<BoundariesShowResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse boundaries show response: {}", e);
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

// ── Argument parsing ─────────────────────────────────────────────────────────

fn parse_show_args(args: &[String]) -> Result<(String, bool), String> {
    let mut surface_uid: Option<String> = None;
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
                if surface_uid.is_some() {
                    return Err(format!("unexpected argument: {}", arg));
                }
                surface_uid = Some(arg.clone());
            }
        }
    }

    match surface_uid {
        Some(uid) => Ok((uid, json_mode)),
        None => Err("missing surface_uid argument".to_string()),
    }
}
