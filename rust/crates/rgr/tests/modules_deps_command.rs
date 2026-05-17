//! Deterministic tests for the `modules deps` command.
//!
//! REG-1 Contract:
//!   - `rmap modules deps [module] [--outbound|--inbound]` — from cwd
//!
//! Test matrix:
//!   1-4. Usage errors (conflicting flags, unknown flags, direction without module, extra args)
//!   5. Daemon required
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - modules_deps_returns_envelope
//!   - modules_deps_with_module_filter
//!   - modules_deps_direction_without_module_error
//!   - modules_deps_module_not_found_returns_error

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
fn modules_deps_usage_error_conflicting_flags() {
    let output = Command::new(binary_path())
        .args(["modules", "deps", "some-module", "--outbound", "--inbound"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot specify both"), "stderr: {}", stderr);
}

#[test]
fn modules_deps_usage_error_unknown_flag() {
    let output = Command::new(binary_path())
        .args(["modules", "deps", "--unknown-flag"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown flag"), "stderr: {}", stderr);
}

#[test]
fn modules_deps_usage_error_direction_without_module() {
    // This test runs in a temp dir without daemon, so it will fail at daemon
    // connection before validating direction. The real validation test is in
    // daemon_dispatch.rs where we have a running daemon.
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["modules", "deps", "--outbound"])
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--outbound") || stderr.contains("require a module"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn modules_deps_usage_error_extra_positional_args() {
    let output = Command::new(binary_path())
        .args(["modules", "deps", "module1", "module2"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// DAEMON REQUIRED
// ══════════════════════════════════════════════════════════════════

#[test]
fn modules_deps_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["modules", "deps"])
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
