//! Deterministic tests for the `docs` command.
//!
//! REG-1 Contract:
//!   - `rmap docs list` — documentation inventory (from cwd)
//!   - `rmap docs extract` — semantic fact extraction (from cwd)
//!
//! Test matrix:
//!   1-3. Usage errors (no subcommand, unknown subcommand, unexpected args)
//!   4-5. Daemon required
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - docs_list_returns_envelope_with_entries
//!   - docs_list_repo_not_indexed_returns_error
//!   - docs_extract_returns_envelope_with_facts

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
fn docs_usage_error_no_subcommand() {
    let output = Command::new(binary_path()).args(["docs"]).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn docs_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["docs", "invalid"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown"), "stderr: {}", stderr);
}

#[test]
fn docs_list_usage_error_unexpected_args() {
    let output = Command::new(binary_path())
        .args(["docs", "list", "extra-arg"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn docs_extract_usage_error_unexpected_args() {
    let output = Command::new(binary_path())
        .args(["docs", "extract", "extra-arg"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// DAEMON REQUIRED
// ══════════════════════════════════════════════════════════════════

#[test]
fn docs_list_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["docs", "list"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon") || stderr.contains("connect"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn docs_extract_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["docs", "extract"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("daemon") || stderr.contains("connect"),
        "stderr: {}",
        stderr
    );
}
