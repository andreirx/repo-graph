//! Deterministic tests for the `stats` command.
//!
//! # REG-1 Contract
//!
//! With REG-1, the `stats` command requires daemon and resolves repo from cwd.
//! New contract: `rmap stats`
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
        .unwrap()
}

// -- 1. Usage error - unexpected args ---------------------------------

#[test]
fn stats_usage_error() {
    // REG-1: stats takes no positional args
    // Passing args should be a usage error
    let output = run_cmd_isolated(&["stats", "unexpected_arg"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "stderr: {}",
        stderr
    );
}

// -- 2. Daemon required -----------------------------------------------

#[test]
fn stats_daemon_required() {
    // REG-1: stats requires daemon for repo resolution
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    std::fs::create_dir_all(repo_path.join("src")).unwrap();
    std::fs::write(repo_path.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
    std::fs::write(
        repo_path.join("src/index.ts"),
        "export function hello() { return 1; }\n",
    )
    .unwrap();

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["stats"])
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
// These tests verify stats command behavior which requires:
// 1. A running daemon
// 2. An indexed repo registered in daemon
// 3. REG-1 resolution working
//
// TODO: Move these to daemon_dispatch.rs where proper daemon setup exists
// ══════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn stats_missing_db() {
    // With REG-1, no db_path argument
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn stats_repo_not_found() {
    // With REG-1, no repo_uid argument - repo not indexed case
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn stats_exact_metrics() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn stats_empty_module_has_zero_symbols() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn stats_results_sorted_by_module_path() {
    unimplemented!("requires daemon");
}
