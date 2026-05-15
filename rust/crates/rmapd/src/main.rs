//! rmapd — repo-graph daemon binary
//!
//! Long-lived service process for repo-graph. Accepts NDJSON requests
//! via Unix domain socket (default) or stdin/stdout (debug mode).
//!
//! Usage:
//!   rmapd              Start daemon (Unix socket, resident process)
//!   rmapd --stdio      Start in debug mode (stdin/stdout, exits on EOF)
//!   rmapd --version    Print version and exit
//!   rmapd --help       Print usage and exit
//!
//! Protocol: NDJSON (newline-delimited JSON).
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
    rmapd              Start daemon (Unix socket, stays alive)
    rmapd --stdio      Start in debug mode (stdin/stdout, exits on EOF)
    rmapd --version    Print version and exit
    rmapd --help       Print this help and exit

SOCKET MODE (default):
    The daemon binds a Unix domain socket and accepts client connections.
    It stays alive as a resident process until SIGTERM/SIGINT.

    Socket location:
      macOS: ~/Library/Application Support/repo-graph/daemon.sock
      Linux: ~/.local/share/rmap/daemon.sock

STDIO MODE (--stdio):
    Debug/test mode. Reads NDJSON from stdin, writes to stdout.
    Exits when stdin reaches EOF. NOT for production use.

The daemon maintains per-repo state and coordinates concurrent access.

Exit codes:
    0    Clean shutdown
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

    // Check for --stdio flag (debug/test mode)
    let stdio_mode = args.iter().any(|a| a == "--stdio");

    // Validate arguments
    for arg in &args {
        if arg != "--stdio" {
            eprintln!("error: unknown argument '{}'", arg);
            eprintln!("Run 'rmapd --help' for usage.");
            return ExitCode::FAILURE;
        }
    }

    // Run daemon in appropriate mode
    if stdio_mode {
        eprintln!("warning: --stdio mode is for debug/test only, not production");
        match repo_graph_daemon_runtime::run_daemon_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("daemon error: {}", e);
                ExitCode::FAILURE
            }
        }
    } else {
        match repo_graph_daemon_runtime::run_daemon() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("daemon error: {}", e);
                ExitCode::FAILURE
            }
        }
    }
}
