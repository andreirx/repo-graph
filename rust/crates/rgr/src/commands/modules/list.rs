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
// `rmap modules list [--include-tests] [--include-inferred] [--json] [--full]`
//
// Human mode (default): plain text with module catalog.
// Machine mode (--json): full envelope.

const LIST_USAGE: &str =
    "usage: rmap modules list [--include-tests] [--include-inferred] [--json] [--full]";

/// Parse `rmap modules list` arguments (pure): `(json_mode, full, partition flags)`. `Err` carries
/// the stderr text of a usage error (exit 1).
pub(crate) fn parse_list_args(
    args: &[String],
) -> Result<(bool, bool, crate::commands::graph::PartitionFlags), String> {
    // TEST-EDGE-SCOPE-1B (RG-REQ-004-L12): `--include-tests` / `--include-inferred` widen the
    // import view (default: certain imports from production files).
    let (args, flags) = crate::commands::graph::extract_partition_flags(args.to_vec(), true);
    let mut json_mode = false;
    // MODULE-EDGES-1 §2.1: `--full` uncaps the cross-module edge list (default
    // budgets it with an honest "(+N more — --full)"); the COMPLETE set always rides
    // `--json`.
    let mut full = false;
    let mut unexpected: Option<&String> = None;

    for arg in &args {
        match arg.as_str() {
            "--json" => {
                json_mode = true;
            }
            "--full" => {
                full = true;
            }
            flag if flag.starts_with("--") => {
                return Err(format!("error: unknown flag: {flag}\n{LIST_USAGE}"));
            }
            _ => {
                if unexpected.is_none() {
                    unexpected = Some(arg);
                }
            }
        }
    }

    if let Some(arg) = unexpected {
        return Err(format!(
            "error: unexpected argument: {arg}\n{LIST_USAGE}\n\nRun from within a repo directory."
        ));
    }
    Ok((json_mode, full, flags))
}

pub(super) fn run_modules_list(args: &[String]) -> ExitCode {
    // ── Parse args (filter out --json / --full / the partition flags) ─────
    let (json_mode, full, flags) = match parse_list_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(crate::daemon_command::EXIT_USAGE_ERROR);
        }
    };

    // Get cwd for repo resolution
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
        }
    };

    let repo_path = match cwd.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot canonicalize current directory: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
        }
    };

    // Connect to daemon
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
        }
    };

    // Build request params
    let mut params = serde_json::json!({
        "repo": repo_path,
    });
    flags.add_to_params(&mut params);

    match client.request("modules_list", Some(params)) {
        Ok(result) => {
            if json_mode {
                // D-TESB-17: the JSON consumer boundary marks unreadable partition evidence.
                let mut result = result;
                if let Err(e) = crate::presentation::import_partition::mark_partition_evidence(
                    &mut result,
                    crate::presentation::import_partition::JsonSurface::ModulesList,
                ) {
                    eprintln!("error: {e}");
                    return ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR);
                }
                // Machine mode: print full envelope
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to serialize result: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            } else {
                // Human mode: parse and render (CLI-OUT-4)
                use crate::presentation::modules_list::ModulesListResponse;
                match serde_json::from_value::<ModulesListResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human_budgeted(full));
                        ExitCode::from(crate::daemon_command::EXIT_SUCCESS)
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse modules list response: {}", e);
                        ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(crate::daemon_command::EXIT_RUNTIME_ERROR)
        }
    }
}
