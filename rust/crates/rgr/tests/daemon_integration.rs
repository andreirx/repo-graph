//! Tests for daemon integration behavior (RMAPD-2).
//!
//! Verifies:
//! - Daemon-required commands fail with actionable error when daemon unavailable
//! - Read-only commands fall back to direct access when daemon unavailable
//!
//! These tests run WITHOUT a daemon to verify fallback behavior.

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

    let db_path = dir.path().join("test.db");

    let output = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
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
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // Create empty DB file to pass path validation
    File::create(&db_path).unwrap();

    let output = Command::new(binary_path())
        .args(["refresh", db_path.to_str().unwrap(), "test-repo"])
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
}

// =============================================================================
// READ-ONLY COMMANDS WITH FALLBACK
// =============================================================================

#[test]
fn stats_succeeds_via_fallback_when_daemon_unavailable() {
    // First, create a database with indexed content using direct library call
    // (since `index` CLI now requires daemon)
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_minimal_repo(&repo_path);

    let db_path = dir.path().join("test.db");

    // Index directly via library (bypassing CLI daemon requirement)
    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let options = ComposeOptions::default();
    index_path(&repo_path, &db_path, "repo", &options).expect("direct index failed");

    // Now test CLI stats command (read-only, should fallback)
    let output = Command::new(binary_path())
        .args(["stats", db_path.to_str().unwrap(), "repo"])
        .output()
        .unwrap();

    // Should succeed via fallback (exit code 0)
    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0 for read-only fallback, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Should produce valid JSON output
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"command\""),
        "expected JSON output, got: {}",
        stdout
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
