//! Deterministic tests for the `modules unowned` command.
//!
//! REG-1 Contract:
//!   - `rmap modules unowned` — from cwd
//!
//! Test matrix:
//!   1. Usage error (unexpected args)
//!   2. Daemon required
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - modules_unowned_returns_envelope
//!   - modules_unowned_repo_not_indexed_returns_error

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
fn modules_unowned_usage_error_unexpected_args() {
    let output = Command::new(binary_path())
        .args(["modules", "unowned", "extra-arg"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// DAEMON REQUIRED
// ══════════════════════════════════════════════════════════════════

#[test]
fn modules_unowned_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["modules", "unowned"])
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
