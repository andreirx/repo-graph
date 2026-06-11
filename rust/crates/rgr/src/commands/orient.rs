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
//! rmap orient [--focus <path>] [--budget small|medium|large] [--json]
//! rmap check [--json]
//! rmap explain <target> [--budget medium|large] [--json]
//! ```
//!
//! # CLI-OUT-1 Output Modes
//!
//! - **Human mode (default)**: Plain text output optimized for reading.
//!   Internal envelope fields are hidden. Signals grouped by severity.
//!
//! - **Machine mode (--json)**: Full daemon envelope as pretty-printed JSON.
//!   Backward compatible with pre-CLI-OUT-1 behavior.

use std::process::ExitCode;

use crate::daemon_client::{DaemonClient, DaemonClientError};
use repo_graph_coherence::CoherenceEnvelope;

use crate::presentation::check::{check_exit_code, render_check_envelope, CheckResponse};
use crate::presentation::explain::ExplainResponse;
use crate::presentation::orient::{render_orient_envelope, OrientResponse};

// ── orient command (REG-1 + CLI-OUT-1) ───────────────────────────────
//
// `rmap orient [--budget small|medium|large] [--focus <string>] [--json]`
//
// Resolves repo from cwd via daemon registry.
// Default: human-readable plain text. --json: full envelope.
//
// Exit codes:
//   0 — success
//   1 — usage error (unknown flag, invalid budget, etc.)
//   2 — runtime error (daemon unavailable, repo not indexed, etc.)

pub fn run_orient(args: &[String]) -> ExitCode {
    // ── Parse args ───────────────────────────────────────────
    let mut budget_raw: Option<String> = None;
    let mut focus_raw: Option<String> = None;
    let mut json_mode = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
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
            if json_mode {
                // Machine mode: print full envelope
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
            } else {
                // Human mode: parse the CoherenceEnvelope<CoherentOrientResult> wrapper and render the
                // inner value + the certainty footer (ORIENT-LIVEGRAPH-IMPL). The `--json` path above is
                // unchanged: it prints the raw daemon JSON (now the wrapper) verbatim.
                match serde_json::from_value::<CoherenceEnvelope<OrientResponse>>(result) {
                    Ok(envelope) => {
                        println!("{}", render_orient_envelope(&envelope));
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse orient response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
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
    eprintln!("usage: rmap orient [--focus <path>] [--budget small|medium|large] [--json]");
}

// ── check command (REG-1) ────────────────────────────────────────────
//
// `rmap check [--json]`
//
// Resolves repo from cwd via daemon registry.

pub fn run_check_cmd(args: &[String]) -> ExitCode {
    // ── Parse args ───────────────────────────────────────────
    let mut json_mode = false;

    for arg in args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                eprintln!("usage: rmap check [--json]");
                return ExitCode::from(1);
            }
            other => {
                eprintln!("error: unexpected argument: {}", other);
                eprintln!("usage: rmap check [--json]");
                return ExitCode::from(1);
            }
        }
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

    // ── Execute request ──────────────────────────────────────
    let params = serde_json::json!({
        "repo": repo_path,
    });

    match client.request("check", Some(params)) {
        Ok(result) => {
            // CHECK-LIVEGRAPH-IMPL §3e: the daemon now returns `CoherenceEnvelope<CoherentOrientResult>`,
            // so `signals` moved UNDER `value` and each signal leaf carries its own `.value`. The exit code
            // is read from `result["value"]["signals"][*]["value"]["code"]` — see `check_exit_code` for the
            // anti-silent-break rationale (reading the now-dead top-level `result["signals"]` path would
            // return exit 2 for EVERY check, INCLUDING a PASS). Computed ONCE here, before the mode branch,
            // so the human and `--json` paths share the identical exit code; the value mapping is preserved
            // verbatim: CHECK_PASS=0 / CHECK_FAIL=1 / CHECK_INCOMPLETE=2 / not-found=2.
            let exit_code = check_exit_code(&result);

            if json_mode {
                // Machine mode: print full wrapped envelope verbatim
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
            } else {
                // Human mode: parse the CoherenceEnvelope<CoherentOrientResult> wrapper and render the
                // inner value + the verdict-line freshness suffix (§3e / §5 W2). The exit code is computed
                // ABOVE, independent of mode, so it cannot drift from the rendered verdict.
                match serde_json::from_value::<CoherenceEnvelope<CheckResponse>>(result) {
                    Ok(envelope) => {
                        println!("{}", render_check_envelope(&envelope));
                        ExitCode::from(exit_code)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse check response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
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
// `rmap explain <target> [--budget medium|large] [--json]`
//
// Resolves repo from cwd via daemon registry.

pub fn run_explain_cmd(args: &[String]) -> ExitCode {
    // Parse args: one positional (target), optional --budget, optional --json
    let mut target: Option<String> = None;
    let mut budget_raw: Option<String> = None;
    let mut json_mode = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
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

    // ── Execute request ──────────────────────────────────────
    let params = serde_json::json!({
        "repo": repo_path,
        "target": target,
        "budget": budget,
    });

    // Transport selection (socket vs stdio) happens in request() via ensure_connected()
    match client.request("explain", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Machine mode: print full envelope
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
            } else {
                // Human mode: parse and render
                match serde_json::from_value::<ExplainResponse>(result) {
                    Ok(response) => {
                        println!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse explain response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
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
    eprintln!("usage: rmap explain <target> [--budget medium|large] [--json]");
}
