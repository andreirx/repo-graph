//! Deterministic tests for the `deps` command family.
//!
//! REG-1 Contract:
//!   - `rmap deps list [module] [--ecosystem npm|cargo]` — list dependencies (from cwd)
//!   - `rmap deps why <package> [--ecosystem npm|cargo]` — explain why a package is used (from cwd)
//!   - `rmap deps drift [--ecosystem npm|cargo]` — show dependency drift anomalies (from cwd)
//!
//! Test matrix:
//!   1-6. Usage errors (no subcommand, unknown subcommand, missing args, invalid options)
//!   7-9. Daemon required tests
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - deps_list_returns_envelope_with_modules
//!   - deps_list_repo_not_indexed_returns_error
//!   - deps_list_with_module_filter
//!   - deps_list_with_ecosystem_cargo
//!   - deps_why_returns_package_usages
//!   - deps_why_package_not_found_returns_error
//!   - deps_drift_returns_anomalies
//!   - deps_drift_repo_not_indexed_returns_error

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

// ══════════════════════════════════════════════════════════════════
// USAGE ERRORS
// ══════════════════════════════════════════════════════════════════

#[test]
fn deps_usage_error_no_subcommand() {
    let output = Command::new(binary_path())
        .args(["deps"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn deps_usage_error_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["deps", "unknown"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown"), "stderr: {}", stderr);
}

#[test]
fn deps_why_usage_error_missing_package() {
    let output = Command::new(binary_path())
        .args(["deps", "why"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn deps_list_unknown_flag() {
    let output = Command::new(binary_path())
        .args(["deps", "list", "--unknown-flag", "value"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown flag"), "stderr: {}", stderr);
}

#[test]
fn deps_list_ecosystem_invalid() {
    let output = Command::new(binary_path())
        .args(["deps", "list", "--ecosystem", "maven"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid ecosystem"), "stderr: {}", stderr);
}

#[test]
fn deps_list_format_invalid() {
    let output = Command::new(binary_path())
        .args(["deps", "list", "--format", "xml"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported format"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// DAEMON REQUIRED
// ══════════════════════════════════════════════════════════════════

#[test]
fn deps_list_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["deps", "list"])
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
fn deps_why_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["deps", "why", "express"])
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
fn deps_drift_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["deps", "drift"])
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
