//! Boundaries command family.
//!
//! Boundary interaction discovery and inspection:
//! - `list` — boundary catalog with filters
//! - `show` — single boundary detail view
//! - `summary` — aggregate boundary statistics
//! - `links` — service link discovery (out of CLI-OUT-4 scope)
//!
//! # REG-1 Contract
//!
//! All subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # Boundary rules
//!
//! This module owns boundaries command-family behavior:
//! - command handlers
//! - family-local DTOs
//! - family-local argument parsing
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - storage queries (belongs in daemon)
//! - human output rendering (lives in `presentation::boundaries_*`)

mod links;
mod list;
mod show;
mod summary;

use std::process::ExitCode;

use links::run_boundaries_links;
use list::run_boundaries_list;
use show::run_boundaries_show;
use summary::run_boundaries_summary;

/// Dispatcher for `rmap boundaries <subcommand>`.
pub fn run_boundaries(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_boundaries_list(&args[1..]),
        "show" => run_boundaries_show(&args[1..]),
        "summary" => run_boundaries_summary(&args[1..]),
        "links" => run_boundaries_links(&args[1..]),
        other => {
            eprintln!("unknown boundaries subcommand: {}", other);
            print_usage();
            ExitCode::from(1)
        }
    }
}

fn print_usage() {
    eprintln!("usage:");
    eprintln!("  rmap boundaries list [--kind <kind>] [--scope <scope>] [--direction <dir>] [--family <fam>] [--file <path>] [--file-prefix <prefix>] [--symbol <key>] [--json]");
    eprintln!("  rmap boundaries show <surface_uid> [--json]");
    eprintln!("  rmap boundaries summary [--json]");
    eprintln!("  rmap boundaries links [--service <name>]");
    eprintln!();
    eprintln!("Run from within a repo directory.");
}

// ── Shared helper ────────────────────────────────────────────────────────────

pub(super) fn daemon_unavailable_message(socket_path: &std::path::Path) -> String {
    format!(
        "Daemon unavailable (socket: {}). Start with: rmapd",
        socket_path.display()
    )
}
