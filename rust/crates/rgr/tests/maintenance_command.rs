//! Tests for the `maintenance` command family.
//!
//! MAINTENANCE-CLI-1: Explicit maintenance operations for retention cleanup.
//!
//! # Test Strategy
//!
//! - CLI argument parsing (unit-level, no daemon needed)
//! - Help output verification
//! - Daemon-unavailable behavior (uses isolated socket)
//! - Live daemon tests require daemon to be running (marked #[ignore])
//!
//! # REG-1 Contract
//!
//! The maintenance command uses daemon-based repo discovery:
//! - Repo is resolved from cwd via daemon registry
//! - Usage: `rmap maintenance prune [--json]`

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
// CLI ARGUMENT PARSING - MAINTENANCE
// =============================================================================

#[test]
fn maintenance_no_subcommand_shows_usage() {
    let output = Command::new(binary_path())
        .args(["maintenance"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "no subcommand should cause usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage:") && stderr.contains("maintenance"),
        "expected usage help in stderr: {}",
        stderr
    );
}

#[test]
fn maintenance_help_shows_usage() {
    let output = Command::new(binary_path())
        .args(["maintenance", "--help"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "--help should succeed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("prune"),
        "help should mention prune subcommand: {}",
        stderr
    );
}

#[test]
fn maintenance_h_shows_usage() {
    let output = Command::new(binary_path())
        .args(["maintenance", "-h"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "-h should succeed");
}

#[test]
fn maintenance_unknown_subcommand_is_error() {
    let output = Command::new(binary_path())
        .args(["maintenance", "unknown"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "unknown subcommand should cause error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown subcommand"),
        "expected 'unknown subcommand' in stderr: {}",
        stderr
    );
}

// =============================================================================
// CLI ARGUMENT PARSING - PRUNE
// =============================================================================

#[test]
fn maintenance_prune_help_shows_usage() {
    let output = Command::new(binary_path())
        .args(["maintenance", "prune", "--help"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "prune --help should succeed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("prune") && stderr.contains("--json"),
        "help should describe prune options: {}",
        stderr
    );
}

#[test]
fn maintenance_prune_unknown_flag_is_error() {
    let output = Command::new(binary_path())
        .args(["maintenance", "prune", "--unknown-flag"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "unknown flag should cause error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown option"),
        "expected 'unknown option' in stderr: {}",
        stderr
    );
}

// =============================================================================
// DAEMON-UNAVAILABLE BEHAVIOR
// =============================================================================

#[test]
fn maintenance_prune_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["maintenance", "prune"])
        .output()
        .unwrap();

    // Exit code 1 because daemon connection fails during execute_prune
    assert!(
        output.status.code() == Some(1) || output.status.code() == Some(2),
        "expected exit 1 or 2 for daemon-unavailable, got {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon") || stderr.contains("connect") || stderr.contains("error"),
        "expected daemon connection error message on stderr: {}",
        stderr
    );
}

#[test]
fn maintenance_prune_json_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["maintenance", "prune", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.code() == Some(1) || output.status.code() == Some(2),
        "expected exit 1 or 2 for daemon-unavailable with --json"
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
// Marked #[ignore] - run manually with: cargo test -p repo-graph-rgr --test maintenance_command -- --ignored

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn maintenance_prune_noop_when_nothing_prunable() {
    // After prune completes, running prune again should be a no-op
    // Verify: pruned_count == 0, retention stats unchanged
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn maintenance_prune_actual_prune_when_backlog_exists() {
    // With prunable snapshots, prune should delete them
    // Verify: pruned_count > 0, prunable_count decreases
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn maintenance_prune_protected_snapshots_preserved() {
    // Prune must never delete: current, parent, baseline_auto, baseline_user
    // Verify: retention stats for protected classes unchanged after prune
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn maintenance_prune_json_output_format() {
    // JSON output should include: repo_path, classified, pruned_count, duration_ms, retention
    // Verify: valid JSON, all fields present
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn maintenance_prune_human_output_format() {
    // Human output should show pruned count and retention stats
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn maintenance_prune_duration_reported() {
    // Duration should be reported in output
    // Verify: duration_ms > 0 in JSON output
    unimplemented!("requires daemon harness");
}
