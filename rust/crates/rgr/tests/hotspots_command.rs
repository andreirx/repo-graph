//! Tests for the `hotspots` command (REG-1 contract).
//!
//! RS-MS-3b: Query-time hotspot analysis (churn × complexity).
//!
//! # REG-1 Contract
//!
//! The hotspots command uses daemon-based repo discovery:
//! - Repo is resolved from cwd via daemon registry
//! - No db_path or repo_uid positional arguments
//! - Usage: `rmap hotspots [--since <expr>] [--exclude-tests] [--exclude-vendored] [--json]`
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
fn hotspots_unknown_flag_is_usage_error() {
    let output = Command::new(binary_path())
        .args(["hotspots", "--unknown-flag"])
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
fn hotspots_since_without_value_is_usage_error() {
    let output = Command::new(binary_path())
        .args(["hotspots", "--since"])
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
fn hotspots_positional_args_rejected() {
    // REG-1: positional args no longer accepted
    let output = Command::new(binary_path())
        .args(["hotspots", "/some/path.db", "repo-name"])
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
fn hotspots_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["hotspots"])
        .output()
        .unwrap();

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
fn hotspots_json_mode_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["hotspots", "--json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-unavailable with --json"
    );
}

#[test]
fn hotspots_with_filters_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["hotspots", "--exclude-tests", "--exclude-vendored"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-unavailable with filters"
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
// Marked #[ignore] - run manually with: cargo test -p repo-graph-rgr --test hotspots_command -- --ignored

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_success_with_complexity() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_custom_since_window() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_excludes_files_without_complexity() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_empty_results_is_success() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_envelope_contract() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_score_is_lines_times_complexity() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_exclude_tests_removes_test_files() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_exclude_vendored_uses_segment_matching() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn hotspots_combined_filters_with_overlap() {
    unimplemented!("requires daemon harness");
}
