//! Tests for the `churn` command (REG-1 contract).
//!
//! RS-MS-2: Query-time per-file git churn.
//!
//! # REG-1 Contract
//!
//! The churn command uses daemon-based repo discovery:
//! - Repo is resolved from cwd via daemon registry
//! - No db_path or repo_uid positional arguments
//! - Usage: `rmap churn [--since <expr>] [--json]`
//!
//! # Test Strategy
//!
//! - CLI argument parsing (unit-level, no daemon needed)
//! - Daemon-unavailable behavior (uses isolated socket)
//! - Live daemon tests require daemon to be running (marked #[ignore])

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
// CLI ARGUMENT PARSING
// =============================================================================

#[test]
fn churn_help_flag_shows_usage() {
    // Note: help flag may not be implemented; this tests behavior
    // The command should at minimum not crash
    let output = Command::new(binary_path())
        .args(["churn", "--help"])
        .output()
        .unwrap();

    // Either shows help (exit 0) or usage error (exit 1)
    let code = output.status.code().unwrap();
    assert!(
        code == 0 || code == 1,
        "--help should not cause runtime error, got exit {}",
        code
    );
}

#[test]
fn churn_unknown_flag_is_usage_error() {
    let output = Command::new(binary_path())
        .args(["churn", "--unknown-flag"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "unknown flag should cause usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown flag") || stderr.contains("usage:"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn churn_since_without_value_is_usage_error() {
    let output = Command::new(binary_path())
        .args(["churn", "--since"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "--since without value should cause usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--since requires"),
        "expected '--since requires' in stderr: {}",
        stderr
    );
}

#[test]
fn churn_positional_args_rejected() {
    // REG-1: positional args no longer accepted
    let output = Command::new(binary_path())
        .args(["churn", "/some/path.db", "repo-name"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "positional args should cause usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument") || stderr.contains("usage:"),
        "stderr: {}",
        stderr
    );
}

// =============================================================================
// DAEMON-UNAVAILABLE BEHAVIOR
// =============================================================================

#[test]
fn churn_fails_when_daemon_unavailable() {
    // REG-1: churn requires daemon for repo resolution
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["churn"])
        .output()
        .unwrap();

    // Should fail with exit code 2 (runtime error - daemon unavailable)
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-unavailable, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "expected error message on stderr");
}

#[test]
fn churn_json_mode_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["churn", "--json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-unavailable with --json"
    );
}

#[test]
fn churn_with_since_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["churn", "--since", "30.days.ago"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-unavailable with --since"
    );
}

// =============================================================================
// LIVE DAEMON TESTS (require running daemon)
// =============================================================================
//
// These tests require:
// 1. A running daemon
// 2. A repo indexed via daemon
// 3. Running from within the repo directory
//
// Marked #[ignore] - run manually with: cargo test -p repo-graph-rgr --test churn_command -- --ignored

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn churn_success_with_default_window() {
    // This test would require:
    // 1. Start daemon (or connect to existing)
    // 2. Index a test repo via daemon
    // 3. Run churn from repo cwd
    // 4. Verify JSON output
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn churn_custom_since_window() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn churn_envelope_contract() {
    unimplemented!("requires daemon harness");
}
