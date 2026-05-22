//! Assess command family.
//!
//! Quality policy assessment for snapshots.
//!
//! # CLI-OUT-7 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw JSON)
//! - Domain verdicts preserved (pass, fail, not_applicable, not_comparable)
//! - Baseline hint when required but missing
//!
//! # REG-1 Contract (LEGACY-CONTRACT-MIGRATION-1C)
//!
//! This command uses REG-1 daemon contract:
//! - Repo resolved from cwd (auto-discovery)
//! - Daemon handles storage access
//! - CLI handles argument parsing and output rendering
//!
//! # Boundary rules
//!
//! This module owns assess command-family behavior:
//! - `run_assess` handler
//! - assess-local argument parsing (inline)
//! - mode switching (human vs --json)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::daemon_command`)
//! - assessment domain logic (belongs in daemon)
//! - human output rendering (lives in `presentation::assess`)

use std::process::ExitCode;

use crate::daemon_command::{
    execute_repo_request, output_result, print_daemon_error, EXIT_RUNTIME_ERROR, EXIT_USAGE_ERROR,
};

// ── assess command ───────────────────────────────────────────────

/// Run quality policy assessment for a snapshot.
///
/// Full-snapshot recomputation: evaluates all active quality policies
/// against the target snapshot's measurements and persists assessments
/// atomically (replaces existing assessments for the snapshot).
///
/// Exit codes:
///   0 — success (assessments persisted)
///   1 — usage error
///   2 — runtime error (daemon unavailable, repo not found, assessment failure)
pub fn run_assess(args: &[String]) -> ExitCode {
    // Parse flags.
    let mut baseline_snapshot_uid: Option<String> = None;
    let mut json_mode = false;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--baseline" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --baseline requires a snapshot_uid argument");
                    eprintln!("usage: rmap assess [--baseline <snapshot_uid>] [--json]");
                    return ExitCode::from(EXIT_USAGE_ERROR);
                }
                baseline_snapshot_uid = Some(args[i + 1].clone());
                i += 2;
            }
            "--json" => {
                json_mode = true;
                i += 1;
            }
            _ if arg.starts_with('-') => {
                eprintln!("error: unknown flag: {}", arg);
                eprintln!("usage: rmap assess [--baseline <snapshot_uid>] [--json]");
                return ExitCode::from(EXIT_USAGE_ERROR);
            }
            _ => {
                eprintln!("error: unexpected argument: {}", arg);
                eprintln!("usage: rmap assess [--baseline <snapshot_uid>] [--json]");
                eprintln!();
                eprintln!("Run from within a repo directory.");
                return ExitCode::from(EXIT_USAGE_ERROR);
            }
        }
    }

    // Build request params
    let params = baseline_snapshot_uid
        .as_ref()
        .map(|baseline| serde_json::json!({"baseline": baseline}));

    // Execute via daemon
    match execute_repo_request("assess", params) {
        Ok(result) => output_result(
            result,
            json_mode,
            |response: crate::presentation::assess::AssessResponse| response.render_human(),
        ),
        Err(err) => {
            print_daemon_error(&err, "assess");
            ExitCode::from(EXIT_RUNTIME_ERROR)
        }
    }
}
