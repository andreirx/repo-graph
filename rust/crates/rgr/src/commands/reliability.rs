//! `rmap reliability` — per-language / per-module call-resolution breakdown
//! (RESOLUTION-BREAKDOWN-CLI-1).
//!
//! Closes the DB-spelunking gap: the per-language call-resolution split the
//! operator previously hand-queried from snapshot SQLite (`edges` CALLS vs
//! `unresolved_edges`, joined through `nodes → files.language`) is now a documented
//! CLI surface. It DECOMPOSES the aggregate reliability figure `trust`/`check`
//! already report — same populations, same rate, same caveats — grouped by
//! language and by owning module.
//!
//! ## Command shape (decide-and-record, RESOLUTION-BREAKDOWN-CLI-1 §3)
//!
//! A NEW focused top-level command, NOT a `check --full` flag. Rationale: `check`'s
//! role is pass/fail validation-before-handoff, and — contrary to the slice's
//! candidate note — `check --full` is a documented no-op and `check` does NOT
//! render the aggregate `CallReliabilityView` block (orient/trust do). A focused
//! `reliability` verb (a) sits beside `trust` as a reliability investigation
//! surface (VISION protocol standard: names imply workflow role), (b) gives an
//! agent one clean JSON payload whose whole content IS the breakdown, and (c)
//! touches ZERO frozen surfaces, so check/trust/orient stay byte-identical (the
//! §4 byte-parity requirement) by construction.
//!
//! ## REG-1
//!
//! Resolves the repo from cwd via the daemon registry — no `db_path`/`repo_uid`.
//!
//! ## Output
//!
//! `--json` prints the full daemon envelope verbatim: BOTH axes, every per-scope
//! figure and caveat — the complete protocol surface an agent parses. The default
//! human view renders both sections; `--by-language` / `--by-module` narrow the
//! HUMAN view only (the JSON is always complete).

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;
use crate::presentation::reliability::{AxisFilter, ReliabilityResponse};

struct ReliabilityArgs {
    json_mode: bool,
    by_language: bool,
    by_module: bool,
}

pub fn run_reliability(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap reliability [--by-language] [--by-module] [--json]");
            return ExitCode::from(1);
        }
    };

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

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let params = serde_json::json!({ "repo": repo_path });

    match client.request("reliability", Some(params)) {
        Ok(result) => {
            if parsed.json_mode {
                // Complete protocol surface: print the daemon envelope verbatim.
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
                match ReliabilityResponse::from_json(result) {
                    Ok(response) => {
                        let axis = AxisFilter::from_flags(parsed.by_language, parsed.by_module);
                        print!("{}", response.render_human(axis));
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse reliability response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: &[String]) -> Result<ReliabilityArgs, String> {
    let mut parsed = ReliabilityArgs {
        json_mode: false,
        by_language: false,
        by_module: false,
    };
    for arg in args {
        match arg.as_str() {
            "--json" => parsed.json_mode = true,
            "--by-language" => parsed.by_language = true,
            "--by-module" => parsed.by_module = true,
            other if other.starts_with("--") => return Err(format!("unknown option: {}", other)),
            other => return Err(format!("unexpected argument: {}", other)),
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_flags() {
        let a = parse_args(&[
            "--json".into(),
            "--by-language".into(),
            "--by-module".into(),
        ])
        .unwrap();
        assert!(a.json_mode && a.by_language && a.by_module);
    }

    #[test]
    fn defaults_are_all_false() {
        let a = parse_args(&[]).unwrap();
        assert!(!a.json_mode && !a.by_language && !a.by_module);
    }

    #[test]
    fn rejects_unknown_option() {
        assert!(parse_args(&["--nope".into()]).is_err());
        assert!(parse_args(&["stray".into()]).is_err());
    }
}
