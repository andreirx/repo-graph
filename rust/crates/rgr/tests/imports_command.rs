//! Deterministic tests for the `imports` command.
//!
//! # REG-1 Contract
//!
//! With REG-1, the `imports` command requires daemon and resolves repo from cwd.
//! New contract: `rmap imports <file>`
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
fn imports_usage_error() {
    // REG-1: imports <file>
    let output = run_cmd_isolated(&["imports"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Daemon required -----------------------------------------------

#[test]
fn imports_daemon_required() {
    // REG-1: imports requires daemon for repo resolution
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    std::fs::create_dir_all(repo_path.join("src")).unwrap();
    std::fs::write(repo_path.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
    std::fs::write(
        repo_path.join("src/index.ts"),
        "import { serve } from \"./server\";\nserve();\n",
    )
    .unwrap();

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["imports", "src/index.ts"])
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
// Imports success behavior tests belong in daemon_dispatch.rs where daemon
// infrastructure is available. See imports_missing_repo_returns_invalid_request.
// Stub tests deleted as part of REG-1 cleanup.
// ══════════════════════════════════════════════════════════════════════
