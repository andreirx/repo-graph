//! Deterministic tests for the `path` command.
//!
//! # REG-1 Contract
//!
//! With REG-1, the `path` command requires daemon and resolves repo from cwd.
//! New contract: `rmap path <from> <to>`
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

// -- 1. Usage error ---------------------------------------------------

#[test]
fn path_usage_error() {
    // REG-1: path <from> <to>
    let output = run_cmd_isolated(&["path"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn path_usage_error_missing_to() {
    // REG-1: path <from> <to>
    let output = run_cmd_isolated(&["path", "main"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Daemon required -----------------------------------------------

#[test]
fn path_daemon_required() {
    // REG-1: path requires daemon for repo resolution
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    std::fs::create_dir_all(repo_path.join("src")).unwrap();
    std::fs::write(repo_path.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
    std::fs::write(
        repo_path.join("src/index.ts"),
        "import { serve } from \"./server\";\nexport function main() { serve(); }\n",
    )
    .unwrap();
    std::fs::write(
        repo_path.join("src/server.ts"),
        "export function serve() {}\n",
    )
    .unwrap();

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["path", "main", "serve"])
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
// These tests verify path command behavior which requires:
// 1. A running daemon
// 2. An indexed repo registered in daemon
// 3. REG-1 resolution working
//
// TODO: Move these to daemon_dispatch.rs where proper daemon setup exists
// ══════════════════════════════════════════════════════════════════════

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn path_missing_db() {
    // With REG-1, no db_path argument
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn path_repo_not_found() {
    // With REG-1, no repo_uid argument - repo not indexed case
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn path_from_symbol_not_found() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn path_to_symbol_not_found() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn path_no_route() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn path_exact_result() {
    unimplemented!("requires daemon");
}

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn path_envelope_contract() {
    unimplemented!("requires daemon");
}
