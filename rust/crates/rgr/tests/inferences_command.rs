//! Deterministic tests for the `inferences list` command.
//!
//! REG-1 Contract:
//!   - `rmap inferences list [--kind <kind>]` — list inferences (from cwd)
//!
//! Test matrix:
//!   1-4. Usage errors (no subcommand, unknown subcommand, unknown option, missing value)
//!   5. Daemon required
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - inferences_list_returns_envelope
//!   - inferences_list_repo_not_indexed_returns_error
//!   - inferences_list_with_kind_filter

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
fn inferences_usage_error_no_subcommand() {
    let output = Command::new(binary_path())
        .args(["inferences"])
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
fn inferences_usage_error_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["inferences", "unknown"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown"), "stderr: {}", stderr);
}

#[test]
fn inferences_list_unknown_option() {
    let output = Command::new(binary_path())
        .args(["inferences", "list", "--unknown-flag", "value"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {}", stderr);
}

#[test]
fn inferences_list_kind_requires_value() {
    let output = Command::new(binary_path())
        .args(["inferences", "list", "--kind"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a value"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// DAEMON REQUIRED
// ══════════════════════════════════════════════════════════════════

#[test]
fn inferences_list_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["inferences", "list"])
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
