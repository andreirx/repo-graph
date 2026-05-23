//! Policy command family.
//!
//! PF-1: STATUS_MAPPING extraction from C files.
//! PF-2: BEHAVIORAL_MARKER extraction from C files.
//! PF-3: RETURN_FATE extraction from C files.
//!
//! Facts are populated automatically during `rmap index` / `rmap refresh`
//! via the policy-facts postpass in repo-index composition.
//!
//! # REG-1 Contract (LEGACY-CONTRACT-MIGRATION-1D)
//!
//! Migrated from legacy `<db_path> <repo_uid>` contract to REG-1:
//! - Repo resolved from cwd via daemon
//! - No storage paths in user-facing contract
//!
//! # CLI-OUT-5 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw JSON)
//! - Deterministic ordering (by file, line)
//! - Full output, no truncation
//! - Kind-specific rendering for each fact type

use std::process::ExitCode;

use crate::daemon_command::{
    execute_repo_request, output_result, print_daemon_error, EXIT_RUNTIME_ERROR, EXIT_USAGE_ERROR,
};
use crate::presentation::policy::{
    BehavioralMarkerResponse, ReturnFateResponse, StatusMappingResponse,
};

// ── CLI ──────────────────────────────────────────────────────────────────────

fn print_usage() {
    eprintln!("usage: rmap policy [options]");
    eprintln!();
    eprintln!("Query policy facts extracted from the repository.");
    eprintln!("Repository is resolved from current working directory.");
    eprintln!();
    eprintln!("options:");
    eprintln!("  --kind STATUS_MAPPING|BEHAVIORAL_MARKER|RETURN_FATE  (default: STATUS_MAPPING)");
    eprintln!("  --file <path>      filter by file path");
    eprintln!("  --callee <name>    filter by callee name (RETURN_FATE only)");
    eprintln!("  --fate <kind>      filter by fate (RETURN_FATE only)");
    eprintln!("                     values: IGNORED, CHECKED, PROPAGATED, TRANSFORMED, STORED");
    eprintln!("  --json             output raw JSON instead of human-readable");
}

/// Parsed arguments for policy command.
struct PolicyArgs {
    kind: String,
    file: Option<String>,
    callee: Option<String>,
    fate: Option<String>,
    json_mode: bool,
}

fn parse_args(args: &[String]) -> Result<PolicyArgs, ExitCode> {
    let mut kind = "STATUS_MAPPING".to_string();
    let mut file: Option<String> = None;
    let mut callee: Option<String> = None;
    let mut fate: Option<String> = None;
    let mut json_mode = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                json_mode = true;
                i += 1;
            }
            "--kind" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --kind requires an argument");
                    return Err(ExitCode::from(EXIT_USAGE_ERROR));
                }
                kind = args[i + 1].to_uppercase();
                i += 2;
            }
            "--file" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --file requires an argument");
                    return Err(ExitCode::from(EXIT_USAGE_ERROR));
                }
                file = Some(args[i + 1].clone());
                i += 2;
            }
            "--callee" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --callee requires an argument");
                    return Err(ExitCode::from(EXIT_USAGE_ERROR));
                }
                callee = Some(args[i + 1].clone());
                i += 2;
            }
            "--fate" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --fate requires an argument");
                    return Err(ExitCode::from(EXIT_USAGE_ERROR));
                }
                fate = Some(args[i + 1].to_uppercase());
                i += 2;
            }
            flag if flag.starts_with("--") => {
                eprintln!("error: unknown flag: {}", flag);
                print_usage();
                return Err(ExitCode::from(EXIT_USAGE_ERROR));
            }
            other => {
                eprintln!("error: unexpected argument: {}", other);
                print_usage();
                return Err(ExitCode::from(EXIT_USAGE_ERROR));
            }
        }
    }

    // Validate kind
    if kind != "STATUS_MAPPING" && kind != "BEHAVIORAL_MARKER" && kind != "RETURN_FATE" {
        eprintln!(
            "error: unsupported policy kind: {} (supported: STATUS_MAPPING, BEHAVIORAL_MARKER, RETURN_FATE)",
            kind
        );
        return Err(ExitCode::from(EXIT_USAGE_ERROR));
    }

    Ok(PolicyArgs {
        kind,
        file,
        callee,
        fate,
        json_mode,
    })
}

// ── Command handler ──────────────────────────────────────────────────────────

/// Run the `rmap policy` command.
///
/// Usage: `rmap policy [--kind STATUS_MAPPING|BEHAVIORAL_MARKER|RETURN_FATE] [--file <path>] [--callee <name>] [--fate <kind>] [--json]`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed, query failure)
pub fn run_policy(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(code) => return code,
    };

    // Build params for daemon request
    let mut params = serde_json::json!({
        "kind": parsed.kind,
    });

    if let Some(file) = &parsed.file {
        params["file"] = serde_json::json!(file);
    }
    if let Some(callee) = &parsed.callee {
        params["callee"] = serde_json::json!(callee);
    }
    if let Some(fate) = &parsed.fate {
        params["fate"] = serde_json::json!(fate);
    }

    // Execute request via daemon
    let result = match execute_repo_request("policy", Some(params)) {
        Ok(r) => r,
        Err(e) => {
            print_daemon_error(&e, "policy");
            return ExitCode::from(EXIT_RUNTIME_ERROR);
        }
    };

    // Output result based on kind
    // The response structure depends on the kind, so we need to deserialize to the correct type
    match parsed.kind.as_str() {
        "STATUS_MAPPING" => {
            output_result::<StatusMappingResponse, _>(result, parsed.json_mode, |response| {
                response.render_human()
            })
        }
        "BEHAVIORAL_MARKER" => {
            output_result::<BehavioralMarkerResponse, _>(result, parsed.json_mode, |response| {
                response.render_human()
            })
        }
        "RETURN_FATE" => {
            output_result::<ReturnFateResponse, _>(result, parsed.json_mode, |response| {
                response.render_human()
            })
        }
        _ => unreachable!(), // Already validated above
    }
}
