//! Deterministic tests for the `explain` command.
//!
//! # REG-1 Contract
//!
//! With REG-1, the `explain` command requires daemon and resolves repo from cwd.
//! New contract: `rmap explain <target> [--budget medium|large]`
//!
//! ## Test Categories
//!
//! 1. **Usage error tests**: Test CLI parsing without daemon
//! 2. **Success tests**: IGNORED - require daemon infrastructure
//!    These should be moved to daemon_dispatch.rs for proper testing

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

// ── 1. Usage error: wrong arg count ────────────────────────────

#[test]
fn explain_usage_error_no_args() {
    // REG-1: explain <target> [--budget medium|large]
    let output = run_cmd_isolated(&["explain"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage"), "stderr: {}", stderr);
}

#[test]
fn explain_usage_error_unexpected_args() {
    // REG-1: explain <target> - no db_path or repo_uid
    // Passing extra positional args should be usage error
    let output = run_cmd_isolated(&["explain", "src/a.ts", "extra_arg"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "stderr: {}",
        stderr
    );
}

// ── 2. Runtime errors (exit 2) - daemon required ────────────────

#[test]
fn explain_daemon_required() {
    // REG-1: explain requires daemon for repo resolution
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    std::fs::create_dir_all(repo_path.join("src")).unwrap();
    std::fs::write(
        repo_path.join("package.json"),
        r#"{"name":"tiny","dependencies":{}}"#,
    )
    .unwrap();
    std::fs::write(
        repo_path.join("src/a.ts"),
        "export function hello() { return 1; }\n",
    )
    .unwrap();

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["explain", "src/a.ts"])
        .output()
        .unwrap();

    // Exit code 2 = runtime error (daemon unavailable)
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should have error message on stderr");
}

// ══════════════════════════════════════════════════════════════════════
// SUCCESS-PATH TESTS
//
// Explain success behavior tests are in daemon_dispatch.rs where daemon
// infrastructure is available. See:
// - explain_missing_target_returns_invalid_request
// - explain_rejects_small_budget
// Stub tests deleted as part of REG-1 cleanup.
// ══════════════════════════════════════════════════════════════════════
