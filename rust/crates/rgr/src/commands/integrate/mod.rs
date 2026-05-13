//! `rmap integrate` command family.
//!
//! Dispatcher for host integrations. This module owns:
//! - Subcommand routing
//! - Top-level help
//!
//! This module does NOT own:
//! - Host-specific policy (lives in claude_code.rs, etc.)
//! - Config transformation (lives in config.rs)
//! - Manifest recording (lives in manifest.rs)

mod claude_code;
mod config;
mod manifest;

use std::process::ExitCode;

/// Run the integrate command.
pub fn run_integrate(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        "claude-code" => run_claude_code(&args[1..]),
        other => {
            eprintln!("error: unknown host: {}", other);
            eprintln!();
            print_usage();
            ExitCode::from(1)
        }
    }
}

/// Route claude-code subcommands.
fn run_claude_code(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_claude_code_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "--help" | "-h" => {
            print_claude_code_usage();
            ExitCode::SUCCESS
        }
        "install" => run_claude_code_install(&args[1..]),
        "remove" => run_claude_code_remove(&args[1..]),
        "status" => run_claude_code_status(&args[1..]),
        other => {
            eprintln!("error: unknown action: {}", other);
            eprintln!();
            print_claude_code_usage();
            ExitCode::from(1)
        }
    }
}

/// Run claude-code install.
fn run_claude_code_install(args: &[String]) -> ExitCode {
    match claude_code::parse_install_args(args) {
        Ok(opts) => claude_code::execute_install(&opts),
        Err(e) if e == "help" => {
            claude_code::print_install_usage();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            claude_code::print_install_usage();
            ExitCode::from(1)
        }
    }
}

/// Run claude-code remove.
fn run_claude_code_remove(args: &[String]) -> ExitCode {
    match claude_code::parse_remove_args(args) {
        Ok(opts) => claude_code::execute_remove(&opts),
        Err(e) if e == "help" => {
            claude_code::print_remove_usage();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            claude_code::print_remove_usage();
            ExitCode::from(1)
        }
    }
}

/// Run claude-code status.
fn run_claude_code_status(args: &[String]) -> ExitCode {
    match claude_code::parse_status_args(args) {
        Ok(opts) => claude_code::execute_status(&opts),
        Err(e) if e == "help" => {
            claude_code::print_status_usage();
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            claude_code::print_status_usage();
            ExitCode::from(1)
        }
    }
}

/// Print top-level integrate usage.
fn print_usage() {
    eprintln!("usage: rmap integrate <host> <action> [OPTIONS]");
    eprintln!();
    eprintln!("Hosts:");
    eprintln!("  claude-code    Claude Code integration");
    eprintln!();
    eprintln!("Actions:");
    eprintln!("  install        Install repo-graph hooks");
    eprintln!("  remove         Remove repo-graph hooks");
    eprintln!("  status         Check integration status");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  rmap integrate claude-code install");
    eprintln!("  rmap integrate claude-code install --full");
    eprintln!("  rmap integrate claude-code remove");
    eprintln!("  rmap integrate claude-code status");
    eprintln!();
    eprintln!("Run 'rmap integrate <host> --help' for host-specific options.");
}

/// Print claude-code specific usage.
fn print_claude_code_usage() {
    eprintln!("usage: rmap integrate claude-code <action> [OPTIONS]");
    eprintln!();
    eprintln!("Actions:");
    eprintln!("  install    Install repo-graph hooks to Claude Code");
    eprintln!("  remove     Remove repo-graph hooks from Claude Code");
    eprintln!("  status     Check Claude Code integration status");
    eprintln!();
    eprintln!("Install options:");
    eprintln!("  --global    Install to ~/.claude/settings.json (default)");
    eprintln!("  --project   Install to ./.claude/settings.json");
    eprintln!("  --full      Install all hooks (default: minimal - SessionStart + Stop)");
    eprintln!("  --dry-run   Show changes without applying");
    eprintln!("  --force     Overwrite existing repo-graph hooks");
    eprintln!();
    eprintln!("Run 'rmap integrate claude-code <action> --help' for action-specific help.");
}
