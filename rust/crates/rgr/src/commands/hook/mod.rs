//! Hook command family for agent host integration.
//!
//! Implements HOOK-1: `rmap hook` CLI surface.
//! Extended by HOOK-1A: stdin JSON transport.
//!
//! These commands are called by thin host shims (Claude Code hooks, Codex hooks)
//! and contain all orientation, refresh, and validation logic. The shims are
//! minimal — policy lives here.
//!
//! # Commands
//!
//! - `session-start` — orient agent at session start
//! - `prompt-submit` — inject task-relevant context before prompt
//! - `post-edit` — refresh index after file edits
//! - `pre-compact` — checkpoint state before context compaction
//! - `stop` — validate and summarize at task completion
//! - `status` — show hook configuration state
//!
//! # Payload Transport
//!
//! Two transport modes are supported:
//!
//! - `--from-env`: Read from host environment variables (Codex, legacy)
//! - `--from-stdin`: Read JSON payload from stdin (Claude Code)
//!
//! Claude Code passes context as JSON on stdin. Codex uses environment variables.
//! Both modes normalize to the same `HookContext` for policy handlers.
//!
//! # Exit Codes
//!
//! - 0: Success
//! - 1: Warning (non-fatal issues)
//! - 2: Error (fatal issues)

mod config;
mod env;
mod output;
mod post_edit;
mod pre_compact;
mod prompt_submit;
mod session;
mod session_start;
mod status;
mod stop;
mod transport;

use std::process::ExitCode;

use post_edit::run_hook_post_edit;
use pre_compact::run_hook_pre_compact;
use prompt_submit::run_hook_prompt_submit;
use session_start::run_hook_session_start;
use status::run_hook_status;
use stop::run_hook_stop;

/// Dispatcher for `rmap hook <subcommand>`.
pub fn run_hook(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_hook_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "session-start" => run_hook_session_start(&args[1..]),
        "prompt-submit" => run_hook_prompt_submit(&args[1..]),
        "post-edit" => run_hook_post_edit(&args[1..]),
        "pre-compact" => run_hook_pre_compact(&args[1..]),
        "stop" => run_hook_stop(&args[1..]),
        "status" => run_hook_status(&args[1..]),
        "--help" | "-h" => {
            print_hook_usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown hook subcommand: {}", other);
            print_hook_usage();
            ExitCode::from(1)
        }
    }
}

fn print_hook_usage() {
    eprintln!("usage:");
    eprintln!("  rmap hook session-start [--from-stdin | --from-env | --db <path> --repo <path>]");
    eprintln!("  rmap hook prompt-submit [--from-stdin | --from-env | --db <path> --repo <path>]");
    eprintln!("  rmap hook post-edit [--from-stdin | --from-env | --db <path> --repo <path> --files <paths>]");
    eprintln!("  rmap hook pre-compact [--from-stdin | --from-env | --db <path> --repo <path>]");
    eprintln!("  rmap hook stop [--from-stdin | --from-env | --db <path> --repo <path>]");
    eprintln!("  rmap hook status");
    eprintln!();
    eprintln!("Transport modes:");
    eprintln!("  --from-stdin  Read JSON payload from stdin (Claude Code)");
    eprintln!("  --from-env    Read from host environment variables (Codex)");
    eprintln!();
    eprintln!("Exit codes:");
    eprintln!("  0 — success");
    eprintln!("  1 — warning (non-fatal)");
    eprintln!("  2 — error (fatal)");
}
