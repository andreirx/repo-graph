//! Deterministic tests for the `orient` command.
//!
//! # REG-1 + CLI-OUT-1 Contract
//!
//! With REG-1, the `orient` command requires daemon and resolves repo from cwd.
//! With CLI-OUT-1, output defaults to human-readable; `--json` returns full envelope.
//!
//! New contract: `rmap orient [--focus <path>] [--budget small|medium|large] [--json]`
//!
//! ## Test Categories
//!
//! 1. **Usage error tests**: Test CLI parsing without daemon
//! 2. **Daemon required test**: Verify daemon is needed
//! 3. **--json flag tests**: Verify flag parsing
//! 4. **Success tests**: In daemon_dispatch.rs (require daemon infrastructure)

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

// ── 3. CLI-OUT-1: --json flag parsing ────────────────────────────────

#[test]
fn orient_json_flag_accepted() {
    // --json is a valid flag; should not cause usage error (exit 1)
    // Will still exit 2 (daemon unavailable) but flag is parsed
    let output = run_cmd_isolated(&["orient", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "Should be daemon error (2), not usage error (1). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn orient_json_with_other_flags_accepted() {
    // --json combined with other flags
    let output = run_cmd_isolated(&["orient", "--focus", "src/core", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "Should be daemon error (2), not usage error (1). stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn orient_json_flag_order_independent() {
    // --json first, then other flags
    let output = run_cmd_isolated(&["orient", "--json", "--budget", "large"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "Flag order should not matter. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ══════════════════════════════════════════════════════════════════════
// SUCCESS-PATH TESTS
//
// Orient success behavior tests are in daemon_dispatch.rs where daemon
// infrastructure is available. See:
// - orient_missing_repo_returns_invalid_request
// - orient_repo_not_indexed_returns_error
// - e2e_index_then_orient_works
//
// CLI-OUT-1 output mode tests (human vs JSON) require daemon success
// to produce actual output. Those tests are in daemon_dispatch.rs:
// - e2e_orient_json_mode_returns_envelope
// - e2e_orient_human_mode_hides_internal_fields
// ══════════════════════════════════════════════════════════════════════
