//! Tests for daemon integration behavior (RMAPD-2).
//!
//! Verifies:
//! - Daemon-required commands fail with actionable error when daemon unavailable
//! - Read-only commands fall back to direct access when daemon unavailable
//!
//! These tests run WITHOUT a daemon to verify fallback behavior.
//! Uses RMAP_SOCKET_PATH env var to point CLI at a non-existent socket,
//! ensuring isolation from any real daemon running on the system.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_rmap"));
    if !path.exists() {
        path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("debug")
            .join("rmap");
    }
    path
}

/// Returns a non-existent socket path for test isolation.
/// Using this ensures tests don't accidentally connect to a real daemon.
fn isolated_socket_path(dir: &std::path::Path) -> PathBuf {
    dir.join("nonexistent-daemon.sock")
}

/// Create a minimal TypeScript repo for testing.
fn create_minimal_repo(dir: &std::path::Path) {
    let src = dir.join("index.ts");
    let mut f = File::create(&src).unwrap();
    writeln!(f, "export function main() {{}}").unwrap();
}

// =============================================================================
// DAEMON-REQUIRED COMMANDS
// =============================================================================

#[test]
fn index_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    // REG-1: index no longer takes db_path - daemon allocates
    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .args(["index", repo_path.to_str().unwrap()])
        .output()
        .unwrap();

    // Should fail with exit code 2 (runtime error)
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-required op, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should have actionable error message
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Daemon unavailable"),
        "expected 'Daemon unavailable' in stderr: {}",
        stderr
    );
    assert!(
        stderr.contains("index"),
        "expected operation name in stderr: {}",
        stderr
    );
}

#[test]
fn refresh_fails_when_daemon_unavailable() {
    // REG-1: refresh resolves repo from cwd, no positional args
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["refresh"])
        .output()
        .unwrap();

    // Should fail with exit code 2 (runtime error)
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-required op, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should have error message (daemon unavailable)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "expected error message on stderr"
    );
}

// =============================================================================
// DAEMON-REQUIRED QUERY COMMANDS (REG-1)
// =============================================================================

#[test]
fn stats_fails_when_daemon_unavailable() {
    // REG-1: stats is daemon-required (no fallback)
    // Resolves repo from cwd via daemon registry
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["stats"])
        .output()
        .unwrap();

    // Should fail with exit code 2 (runtime error - daemon unavailable)
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-required op, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should have error message
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "expected error message on stderr"
    );
}

// =============================================================================
// STATIC COMMANDS (always work without daemon)
// =============================================================================

#[test]
fn version_succeeds_without_daemon() {
    let output = Command::new(binary_path())
        .args(["--version"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "--version should always succeed"
    );
}

#[test]
fn help_succeeds_without_daemon() {
    let output = Command::new(binary_path())
        .args(["--help"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "--help should always succeed"
    );
}

#[test]
fn doctor_succeeds_without_daemon() {
    // Doctor is read-only and works in degraded mode
    let output = Command::new(binary_path())
        .args(["doctor"])
        .output()
        .unwrap();

    // Doctor returns exit 1 if checks fail, but should not exit 2
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "doctor should succeed (exit 0 or 1), got: {}",
        code
    );
}
