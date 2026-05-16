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
// IGNORED TESTS - Require daemon infrastructure
//
// These tests verify explain command success behavior which requires:
// 1. A running daemon
// 2. An indexed repo registered in daemon
// 3. REG-1 resolution working
//
// TODO: Move these to daemon_dispatch.rs where proper daemon setup exists
// ══════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn explain_missing_db_runtime_error() {
    // With REG-1, no db_path argument - this test concept doesn't apply
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn explain_missing_repo_runtime_error() {
    // With REG-1, no repo_uid argument - this test concept doesn't apply
    // Repo resolution failure would be "repo not indexed"
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn explain_valid_file_target() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn explain_valid_path_target() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn explain_envelope_command_is_explain() {
    unimplemented!("requires daemon");
}
