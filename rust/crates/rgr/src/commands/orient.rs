//! Orient command family.
//!
//! Agent-facing discovery surfaces: orient, check, explain.
//!
//! # REG-1 Contract
//!
//! All commands in this family resolve repo from current working directory
//! via the daemon registry. No positional `<db_path> <repo_uid>` arguments.
//!
//! ```text
//! rmap orient [--focus <path>] [--budget small|medium|large]
//! rmap check
//! rmap explain <target> [--budget medium|large]
//! ```

use std::process::ExitCode;

use crate::daemon_client::{daemon_unavailable_message, DaemonClient, DaemonClientError};

// ── orient command (REG-1) ───────────────────────────────────────────
//
// `rmap orient [--budget small|medium|large] [--focus <string>]`
//
// Resolves repo from cwd via daemon registry. Output is JSON on stdout.
//
// Exit codes:
//   0 — success, JSON on stdout
//   1 — usage error (unknown flag, invalid budget, etc.)
//   2 — runtime error (daemon unavailable, repo not indexed, etc.)

pub fn run_orient(args: &[String]) -> ExitCode {
    // ── Parse args ───────────────────────────────────────────
    let mut budget_raw: Option<String> = None;
    let mut focus_raw: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--budget" => {
                if budget_raw.is_some() {
                    eprintln!("error: --budget specified more than once");
                    print_orient_usage();
                    return ExitCode::from(1);
                }
                i += 1;
                let value = match args.get(i) {
                    Some(v) => v,
                    None => {
                        eprintln!("error: --budget requires a value");
                        print_orient_usage();
                        return ExitCode::from(1);
                    }
                };
                if value.starts_with("--") {
                    eprintln!("error: --budget requires a value, got flag: {}", value);
                    print_orient_usage();
                    return ExitCode::from(1);
                }
                budget_raw = Some(value.clone());
            }
            "--focus" => {
                if focus_raw.is_some() {
                    eprintln!("error: --focus specified more than once");
                    print_orient_usage();
                    return ExitCode::from(1);
                }
                i += 1;
                let value = match args.get(i) {
                    Some(v) => v,
                    None => {
                        eprintln!("error: --focus requires a value");
                        print_orient_usage();
                        return ExitCode::from(1);
                    }
                };
                if value.starts_with("--") {
                    eprintln!("error: --focus requires a value, got flag: {}", value);
                    print_orient_usage();
                    return ExitCode::from(1);
                }
                focus_raw = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                print_orient_usage();
                return ExitCode::from(1);
            }
            other => {
                // No positional args in REG-1 contract
                eprintln!("error: unexpected argument: {}", other);
                print_orient_usage();
                return ExitCode::from(1);
            }
        }
        i += 1;
    }

    // ── Validate budget ──────────────────────────────────────
    let budget = match budget_raw.as_deref() {
        None => "small",
        Some("small") => "small",
        Some("medium") => "medium",
        Some("large") => "large",
        Some(other) => {
            eprintln!(
                "error: invalid --budget value: {} (expected small|medium|large)",
                other
            );
            print_orient_usage();
            return ExitCode::from(1);
        }
    };

    // ── Resolve repo from cwd ────────────────────────────────
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot get current directory: {}", e);
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

    // ── Connect to daemon ────────────────────────────────────
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path(), "orient"));
        return ExitCode::from(2);
    }

    // ── Build request ────────────────────────────────────────
    let mut params = serde_json::json!({
        "repo": repo_path,
        "budget": budget,
    });

    if let Some(focus) = focus_raw {
        params["focus"] = serde_json::Value::String(focus);
    }

    // ── Execute request ──────────────────────────────────────
    match client.request("orient", Some(params)) {
        Ok(result) => {
            match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    println!("{}", json);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    ExitCode::from(2)
                }
            }
        }
        Err(DaemonClientError::DaemonError { code, message }) => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed");
                eprintln!("hint: run 'rmap index .' to index this repo");
            } else {
                eprintln!("error: {}: {}", code, message);
            }
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

fn print_orient_usage() {
    eprintln!("usage: rmap orient [--focus <path>] [--budget small|medium|large]");
}

// ── check command (REG-1) ────────────────────────────────────────────
//
// `rmap check`
//
// Resolves repo from cwd via daemon registry.

pub fn run_check_cmd(args: &[String]) -> ExitCode {
    // No args expected in REG-1 contract
    if !args.is_empty() {
        eprintln!("error: unexpected arguments");
        eprintln!("usage: rmap check");
        return ExitCode::from(1);
    }

    // ── Resolve repo from cwd ────────────────────────────────
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot get current directory: {}", e);
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

    // ── Connect to daemon ────────────────────────────────────
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path(), "check"));
        return ExitCode::from(2);
    }

    // ── Execute request ──────────────────────────────────────
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("check", Some(params)) {
        Ok(result) => {
            // Map verdict to exit code from the result signals
            let exit_code = result["signals"]
                .as_array()
                .and_then(|signals| {
                    signals.iter().find_map(|s| {
                        s["code"].as_str().and_then(|code| match code {
                            "CHECK_PASS" => Some(0),
                            "CHECK_FAIL" => Some(1),
                            "CHECK_INCOMPLETE" => Some(2),
                            _ => None,
                        })
                    })
                })
                .unwrap_or(2);

            match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    println!("{}", json);
                    ExitCode::from(exit_code)
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    ExitCode::from(2)
                }
            }
        }
        Err(DaemonClientError::DaemonError { code, message }) => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed");
                eprintln!("hint: run 'rmap index .' to index this repo");
            } else {
                eprintln!("error: {}: {}", code, message);
            }
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── explain command (REG-1) ──────────────────────────────────────────
//
// `rmap explain <target> [--budget medium|large]`
//
// Resolves repo from cwd via daemon registry.

pub fn run_explain_cmd(args: &[String]) -> ExitCode {
    // Parse args: one positional (target), optional --budget
    let mut target: Option<String> = None;
    let mut budget_raw: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--budget" => {
                if budget_raw.is_some() {
                    eprintln!("error: --budget specified more than once");
                    print_explain_usage();
                    return ExitCode::from(1);
                }
                i += 1;
                let value = match args.get(i) {
                    Some(v) => v,
                    None => {
                        eprintln!("error: --budget requires a value");
                        print_explain_usage();
                        return ExitCode::from(1);
                    }
                };
                if value.starts_with("--") {
                    eprintln!("error: --budget requires a value, got flag: {}", value);
                    print_explain_usage();
                    return ExitCode::from(1);
                }
                budget_raw = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                print_explain_usage();
                return ExitCode::from(1);
            }
            _ => {
                if target.is_some() {
                    eprintln!("error: unexpected argument: {}", arg);
                    print_explain_usage();
                    return ExitCode::from(1);
                }
                target = Some(arg.clone());
            }
        }
        i += 1;
    }

    let target = match target {
        Some(t) => t,
        None => {
            eprintln!("error: missing target argument");
            print_explain_usage();
            return ExitCode::from(1);
        }
    };

    // Budget: default medium, accept medium or large only.
    let budget = match budget_raw.as_deref() {
        None => "medium",
        Some("medium") => "medium",
        Some("large") => "large",
        Some(other) => {
            eprintln!(
                "error: invalid --budget value: {} (expected medium|large)",
                other
            );
            print_explain_usage();
            return ExitCode::from(1);
        }
    };

    // ── Resolve repo from cwd ────────────────────────────────
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot get current directory: {}", e);
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

    // ── Connect to daemon ────────────────────────────────────
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    if !client.is_available() {
        eprintln!("{}", daemon_unavailable_message(client.socket_path(), "explain"));
        return ExitCode::from(2);
    }

    // ── Execute request ──────────────────────────────────────
    let params = serde_json::json!({
        "repo": repo_path,
        "target": target,
        "budget": budget,
    });

    match client.request("explain", Some(params)) {
        Ok(result) => {
            match serde_json::to_string_pretty(&result) {
                Ok(json) => {
                    println!("{}", json);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    ExitCode::from(2)
                }
            }
        }
        Err(DaemonClientError::DaemonError { code, message }) => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed");
                eprintln!("hint: run 'rmap index .' to index this repo");
            } else {
                eprintln!("error: {}: {}", code, message);
            }
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

fn print_explain_usage() {
    eprintln!("usage: rmap explain <target> [--budget medium|large]");
}
