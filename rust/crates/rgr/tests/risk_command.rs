//! Tests for the `risk` command (REG-1 contract).
//!
//! RS-MS-4: Query-time risk analysis (hotspot x coverage gap).
//!
//! # REG-1 Contract
//!
//! The risk command uses daemon-based repo discovery:
//! - Repo is resolved from cwd via daemon registry
//! - No db_path or repo_uid positional arguments
//! - Usage: `rmap risk [--since <expr>] [--json]`
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
fn risk_unknown_flag_is_usage_error() {
    let output = Command::new(binary_path())
        .args(["risk", "--unknown-flag"])
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
fn risk_since_without_value_is_usage_error() {
    let output = Command::new(binary_path())
        .args(["risk", "--since"])
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
fn risk_positional_args_rejected() {
    // REG-1: positional args no longer accepted
    let output = Command::new(binary_path())
        .args(["risk", "/some/path.db", "repo-name"])
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
fn risk_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["risk"])
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
fn risk_json_mode_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["risk", "--json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit 2 for daemon-unavailable with --json"
    );
}

#[test]
fn risk_with_since_fails_when_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .current_dir(&repo_path)
        .args(["risk", "--since", "30.days.ago"])
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
// Marked #[ignore] - run manually with: cargo test -p repo-graph-rgr --test risk_command -- --ignored

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_success_with_hotspot_and_coverage() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_custom_since_window() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_excludes_files_without_coverage() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_excludes_files_without_hotspot() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_empty_results_is_success() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_envelope_contract() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_formula_is_hotspot_times_coverage_gap() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_high_coverage_reduces_risk() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_malformed_coverage_json_aborts() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_coverage_missing_value_field_aborts() {
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore = "requires running daemon with indexed repo"]
fn risk_malformed_target_stable_key_aborts() {
    unimplemented!("requires daemon harness");
}
