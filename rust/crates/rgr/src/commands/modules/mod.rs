//! Modules command family.
//!
//! Discovered-module analysis and boundary management:
//! - `list` — module catalog with rollup statistics
//! - `show` — single module detail view
//! - `files` — files owned by a module
//! - `deps` — module dependency edges
//! - `violations` — boundary violation report
//! - `boundary` — create boundary declarations
//!
//! Also exports unified `violations` command that combines
//! declared boundaries (legacy) with discovered-module boundaries.
//!
//! # Boundary rules
//!
//! This module owns modules command-family behavior:
//! - command handlers
//! - family-local DTOs
//! - family-local argument parsing
//! - family-local helpers
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in `repo-graph-module-queries`)
//! - classification algorithms (belong in `repo-graph-classification`)

mod boundary;
mod deps;
mod files;
mod list;
mod shared;
mod show;
mod violations;

use std::process::ExitCode;

use boundary::run_modules_boundary;
use deps::run_modules_deps;
use files::run_modules_files;
use list::run_modules_list;
use show::run_modules_show;
use violations::run_modules_violations;

// Re-export unified violations command
pub use violations::run_violations;

/// Dispatcher for `rmap modules <subcommand>`.
pub fn run_modules(args: &[String]) -> ExitCode {
	if args.is_empty() {
		eprintln!("usage:");
		eprintln!("  rmap modules list <db_path> <repo_uid>");
		eprintln!("  rmap modules show <db_path> <repo_uid> <module>");
		eprintln!("  rmap modules files <db_path> <repo_uid> <module>");
		eprintln!("  rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
		eprintln!("  rmap modules violations <db_path> <repo_uid>");
		eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
		return ExitCode::from(1);
	}

	match args[0].as_str() {
		"list" => run_modules_list(&args[1..]),
		"show" => run_modules_show(&args[1..]),
		"files" => run_modules_files(&args[1..]),
		"deps" => run_modules_deps(&args[1..]),
		"violations" => run_modules_violations(&args[1..]),
		"boundary" => run_modules_boundary(&args[1..]),
		other => {
			eprintln!("unknown modules subcommand: {}", other);
			eprintln!("usage:");
			eprintln!("  rmap modules list <db_path> <repo_uid>");
			eprintln!("  rmap modules show <db_path> <repo_uid> <module>");
			eprintln!("  rmap modules files <db_path> <repo_uid> <module>");
			eprintln!("  rmap modules deps <db_path> <repo_uid> [module] [--outbound|--inbound]");
			eprintln!("  rmap modules violations <db_path> <repo_uid>");
			eprintln!("  rmap modules boundary <db_path> <repo_uid> <source> --forbids <target> [--reason <text>]");
			ExitCode::from(1)
		}
	}
}
