//! rmapd — repo-graph daemon binary
//!
//! Long-lived service process for repo-graph. Accepts NDJSON requests
//! on stdin, writes NDJSON responses to stdout.
//!
//! Usage:
//!   rmapd              Start daemon (reads NDJSON from stdin)
//!   rmapd --version    Print version and exit
//!   rmapd --help       Print usage and exit
//!   rmapd --config P   Start daemon with config file (reserved, not yet used)
//!
//! Protocol: NDJSON (newline-delimited JSON) over stdin/stdout.
//! See daemon-transport crate for protocol details.
//!
//! This binary is wiring only. All daemon logic lives in
//! repo-graph-daemon-runtime.

use std::env;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    eprintln!(
        "rmapd {} — repo-graph daemon

USAGE:
    rmapd              Start daemon (NDJSON on stdin/stdout)
    rmapd --version    Print version and exit
    rmapd --help       Print this help and exit
    rmapd --config P   Start daemon with config file P (reserved)

The daemon reads NDJSON requests from stdin and writes responses to stdout.
It maintains per-repo state and coordinates concurrent access.

Exit codes:
    0    Clean shutdown (stdin EOF)
    1    Runtime error
",
        VERSION
    );
}

fn print_version() {
    println!("rmapd {}", VERSION);
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    // Handle flags
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }

    if args.iter().any(|a| a == "--version" || a == "-V") {
        print_version();
        return ExitCode::SUCCESS;
    }

    // Handle --config (reserved for future use)
    let mut config_path: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--config" {
            if i + 1 < args.len() {
                config_path = Some(args[i + 1].clone());
                i += 2;
            } else {
                eprintln!("error: --config requires a path argument");
                return ExitCode::FAILURE;
            }
        } else {
            eprintln!("error: unknown argument '{}'", args[i]);
            eprintln!("Run 'rmapd --help' for usage.");
            return ExitCode::FAILURE;
        }
    }

    // Config path is reserved but not yet used
    if config_path.is_some() {
        eprintln!("note: --config is reserved for future use, currently ignored");
    }

    // Run daemon
    match repo_graph_daemon_runtime::run_daemon() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("daemon error: {}", e);
            ExitCode::FAILURE
        }
    }
}
