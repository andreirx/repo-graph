//! Minimal Rust CLI for repo-graph.
//!
//! ## REG-1 Contract (Primary)
//!
//! Most commands resolve repo from current working directory via daemon registry.
//! Explicit `--repo` or `--repo-path` overrides are available for multi-repo scenarios.
//!
//! ```text
//! # Indexing
//! rmap index [repo_path] [--alias <name>]   # index repo, daemon allocates db
//!
//! # Repo management
//! rmap repo list                            # list all registered repos
//! rmap repo info [repo]                     # show repo details (default: cwd)
//! rmap repo alias <repo> <alias>            # set or change alias
//! rmap repo remove <repo> [--delete-db]     # remove from registry
//!
//! # Query commands (resolve repo from cwd)
//! rmap orient [--focus <path>] [--budget small|medium|large]
//! rmap check
//! rmap explain <target>
//! rmap callers <symbol>
//! rmap callees <symbol>
//! rmap trust
//! ```
//!
//! ## Legacy/Transitional Commands
//!
//! Some commands still use explicit `<db_path> <repo_uid>` until full REG-1 migration.
//! These will be updated in subsequent slices:
//!
//! ```text
//! rmap refresh <db_path> <repo_uid>
//! rmap gate    <db_path> <repo_uid>
//! rmap assess  <db_path> <repo_uid>
//! ... (see --help for full list)
//! ```
//!
//! ## Exit codes
//!
//! - 0: success
//! - 1: usage error / check fail / violations found
//! - 2: runtime error (daemon unavailable, storage failure, etc.)

// Gate policy was relocated out of this binary crate into
// `repo-graph-gate` during Rust-43A. The `run_gate` function
// below now calls into the new crate through the
// `GateStorageRead` impl in `repo-graph-storage`. No local
// `mod gate;` declaration.

use repo_graph_rgr::cli::print_usage;
use repo_graph_rgr::commands::{
    run_assess, run_boundaries, run_callees, run_callers, run_check_cmd, run_churn, run_contracts,
    run_coverage, run_cycles, run_dead, run_declare, run_deps, run_docs, run_doctor, run_enrich,
    run_explain_cmd, run_gate, run_hook, run_hotspots, run_imports, run_index, run_inferences,
    run_integrate, run_metrics, run_modules, run_orient, run_path, run_perf, run_policy,
    run_refresh, run_repo, run_resource, run_risk, run_stats, run_surfaces, run_trust,
    run_uninstall, run_violations,
};
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return ExitCode::from(1);
    }

    match args[1].as_str() {
        "--version" | "-V" => {
            println!("rmap {}", VERSION);
            return ExitCode::SUCCESS;
        }
        "--help" | "-h" => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    match args[1].as_str() {
        "index" => run_index(&args[2..]),
        "refresh" => run_refresh(&args[2..]),
        "trust" => run_trust(&args[2..]),
        "callers" => run_callers(&args[2..]),
        "callees" => run_callees(&args[2..]),
        "path" => run_path(&args[2..]),
        "imports" => run_imports(&args[2..]),
        "violations" => run_violations(&args[2..]),
        "gate" => run_gate(&args[2..]),
        "orient" => run_orient(&args[2..]),
        "check" => run_check_cmd(&args[2..]),
        "churn" => run_churn(&args[2..]),
        "hotspots" => run_hotspots(&args[2..]),
        "metrics" => run_metrics(&args[2..]),
        "coverage" => run_coverage(&args[2..]),
        "risk" => run_risk(&args[2..]),
        "assess" => run_assess(&args[2..]),
        "explain" => run_explain_cmd(&args[2..]),
        "dead" => run_dead(&args[2..]),
        "deps" => run_deps(&args[2..]),
        "cycles" => run_cycles(&args[2..]),
        "stats" => run_stats(&args[2..]),
        "declare" => run_declare(&args[2..]),
        "docs" => run_docs(&args[2..]),
        "enrich" => run_enrich(&args[2..]),
        "hook" => run_hook(&args[2..]),
        "inferences" => run_inferences(&args[2..]),
        "resource" => run_resource(&args[2..]),
        "modules" => run_modules(&args[2..]),
        "surfaces" => run_surfaces(&args[2..]),
        "boundaries" => run_boundaries(&args[2..]),
        "contracts" => run_contracts(&args[2..]),
        "perf" => run_perf(&args[2..]),
        "policy" => run_policy(&args[2..]),
        "repo" => run_repo(&args[2..]),
        "doctor" => run_doctor(&args[2..]),
        "uninstall" => run_uninstall(&args[2..]),
        "integrate" => run_integrate(&args[2..]),
        "daemon" => {
            // DEPRECATED: Use `rmapd` binary instead of `rmap daemon`.
            // This compatibility shim will be removed in a future release.
            eprintln!("warning: 'rmap daemon' is deprecated. Use 'rmapd' instead.");
            eprintln!("         The rmapd binary is the dedicated daemon executable.");
            eprintln!();
            match repo_graph_daemon_runtime::run_daemon() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("daemon error: {}", e);
                    ExitCode::from(2)
                }
            }
        }
        other => {
            eprintln!("unknown command: {}", other);
            print_usage();
            ExitCode::from(1)
        }
    }
}
