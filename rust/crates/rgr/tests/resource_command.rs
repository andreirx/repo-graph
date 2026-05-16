//! Deterministic tests for the `resource` command family.
//!
//! REG-1 Contract:
//!   - `rmap resource list [--kind <kind>]` — list resources (from cwd)
//!   - `rmap resource readers <resource>` — find readers (from cwd)
//!   - `rmap resource writers <resource>` — find writers (from cwd)
//!
//! Test matrix:
//!   1-4. Usage errors (no subcommand, unknown subcommand, missing args)
//!   5-7. Daemon required
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - resource_list_returns_envelope
//!   - resource_list_repo_not_indexed_returns_error
//!   - resource_readers_returns_envelope_or_not_found
//!   - resource_writers_returns_envelope_or_not_found

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

// ══════════════════════════════════════════════════════════════════
// USAGE ERRORS
// ══════════════════════════════════════════════════════════════════

#[test]
fn resource_usage_error_no_subcommand() {
    let output = Command::new(binary_path())
        .args(["resource"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn resource_usage_error_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["resource", "invalid"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown"), "stderr: {}", stderr);
}

#[test]
fn resource_readers_usage_error_missing_arg() {
    let output = Command::new(binary_path())
        .args(["resource", "readers"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn resource_writers_usage_error_missing_arg() {
    let output = Command::new(binary_path())
        .args(["resource", "writers"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// DAEMON REQUIRED
// ══════════════════════════════════════════════════════════════════

#[test]
fn resource_list_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["resource", "list"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon") || stderr.contains("connect") || stderr.contains("Daemon"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn resource_readers_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["resource", "readers", "some:key"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon") || stderr.contains("connect") || stderr.contains("Daemon"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn resource_writers_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["resource", "writers", "some:key"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon") || stderr.contains("connect") || stderr.contains("Daemon"),
        "stderr: {}",
        stderr
    );
}
