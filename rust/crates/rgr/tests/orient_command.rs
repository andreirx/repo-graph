//! Deterministic tests for the `orient` command (Rust-43B).
//!
//! # REG-1 Contract
//!
//! With REG-1, the `orient` command requires daemon and resolves repo from cwd.
//! New contract: `rmap orient [--focus <path>] [--budget small|medium|large]`
//!
//! ## Test Categories
//!
//! 1. **Usage error tests**: Test CLI parsing without daemon
//! 2. **Daemon required test**: Verify daemon is needed
//! 3. **Success tests**: IGNORED - require daemon infrastructure

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Returns a non-existent socket path for test isolation.
fn isolated_socket_path(dir: &std::path::Path) -> PathBuf {
    dir.join("nonexistent-daemon.sock")
}

fn run_cmd_isolated(args: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .args(args)
        .output()
        .expect("failed to spawn rmap")
}

// ── 1. Usage error: no positional args needed with REG-1 ────────

#[test]
fn orient_no_args_daemon_required() {
    // REG-1: orient takes no positional args - repo from cwd
    // Without daemon, should exit 2 (runtime error)
    let output = run_cmd_isolated(&["orient"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should have error message on stderr");
}

#[test]
fn orient_unexpected_positional_arg_usage_error() {
    // REG-1: orient <positional_arg> is now a usage error
    let output = run_cmd_isolated(&["orient", "/some/path.db"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "stderr: {}",
        stderr
    );
}

// ── 2. Usage error: flag validation ─────────────────────────────

#[test]
fn orient_unknown_flag_usage_error() {
    // REG-1: orient [--focus <path>] [--budget ...]
    let output = run_cmd_isolated(&["orient", "--bogus"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown flag"), "stderr: {}", stderr);
}

#[test]
fn orient_budget_missing_value_usage_error() {
    let output = run_cmd_isolated(&["orient", "--budget"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--budget requires a value"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn orient_budget_unknown_value_usage_error() {
    let output = run_cmd_isolated(&["orient", "--budget", "enormous"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --budget value"),
        "stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("small|medium|large"),
        "stderr must list accepted values: {}",
        stderr
    );
}

#[test]
fn orient_budget_repeated_usage_error() {
    let output = run_cmd_isolated(&["orient", "--budget", "small", "--budget", "medium"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--budget specified more than once"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn orient_budget_case_sensitive_usage_error() {
    let output = run_cmd_isolated(&["orient", "--budget", "Small"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid --budget value"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn orient_focus_repeated_usage_error() {
    let output = run_cmd_isolated(&["orient", "--focus", "a", "--focus", "b"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--focus specified more than once"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn orient_focus_flag_as_value_usage_error() {
    // The parser must not consume a flag as a value
    let output = run_cmd_isolated(&["orient", "--focus", "--bogus"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "flag-as-value must be a usage error (exit 1), not a runtime error (exit 2). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--focus requires a value") && stderr.contains("--bogus"),
        "stderr must name the offending flag token: {}",
        stderr
    );
}

#[test]
fn orient_budget_flag_as_value_usage_error() {
    let output = run_cmd_isolated(&["orient", "--budget", "--focus", "x"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "flag-as-value must be a usage error. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--budget requires a value") && stderr.contains("--focus"),
        "stderr must name the offending flag token: {}",
        stderr
    );
}

// ══════════════════════════════════════════════════════════════════════
// IGNORED TESTS - Require daemon infrastructure
//
// These tests verify orient command success behavior which requires:
// 1. A running daemon
// 2. An indexed repo registered in daemon
// 3. REG-1 resolution working
//
// TODO: Move these to daemon_dispatch.rs where proper daemon setup exists
// ══════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_missing_args_usage_error() {
    // Old test - with REG-1, no positional args needed
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_one_positional_arg_usage_error() {
    // Old test - with REG-1, no positional args needed
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_missing_db_runtime_error() {
    // With REG-1, no db_path argument
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_missing_repo_runtime_error() {
    // With REG-1, no repo_uid argument - repo not indexed case
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_missing_snapshot_runtime_error() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_focus_flag_returns_no_match_for_unknown_path() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_default_budget_small_succeeds() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_medium_budget_succeeds() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_large_budget_succeeds() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn orient_smoke_emits_informational_signals_and_gate_limit() {
    unimplemented!("requires daemon");
}
