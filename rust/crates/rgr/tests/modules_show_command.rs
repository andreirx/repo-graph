//! Deterministic tests for the `modules show` command.
//!
//! REG-1 Contract:
//!   - `rmap modules show <module>` — from cwd
//!
//! Test matrix:
//!   1. Usage error (missing module)
//!   2. Usage error (too many args)
//!   3. Daemon required
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - modules_show_returns_module_identity
//!   - modules_show_returns_rollups
//!   - modules_show_returns_neighbors
//!   - modules_show_module_not_found
//!   - modules_show_repo_not_indexed

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
fn modules_show_usage_error_missing_module() {
    let output = Command::new(binary_path())
        .args(["modules", "show"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing module"), "stderr: {}", stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn modules_show_usage_error_too_many_args() {
    let output = Command::new(binary_path())
        .args(["modules", "show", "module-arg", "extra-arg"])
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
fn modules_show_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["modules", "show", "some-module"])
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
