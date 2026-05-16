//! Deterministic tests for the `surfaces` command family.
//!
//! REG-1 Contract:
//!   - `rmap surfaces list [--kind <kind>] [--runtime <rt>] [--source <src>] [--module <m>]` — list surfaces (from cwd)
//!   - `rmap surfaces show <surface_ref>` — show surface detail (from cwd)
//!
//! Test matrix:
//!   1-5. Usage errors (no subcommand, unknown subcommand, missing args, invalid options)
//!   6-7. Daemon required tests
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - surfaces_list_returns_envelope
//!   - surfaces_list_repo_not_indexed_returns_error
//!   - surfaces_list_with_kind_filter
//!   - surfaces_show_returns_detail
//!   - surfaces_show_surface_not_found_returns_error

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
fn surfaces_usage_error_no_subcommand() {
    let output = Command::new(binary_path())
        .args(["surfaces"])
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
fn surfaces_usage_error_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["surfaces", "unknown"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown"), "stderr: {}", stderr);
}

#[test]
fn surfaces_show_usage_error_missing_surface_ref() {
    let output = Command::new(binary_path())
        .args(["surfaces", "show"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn surfaces_list_unknown_option() {
    let output = Command::new(binary_path())
        .args(["surfaces", "list", "--unknown-flag", "value"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {}", stderr);
}

#[test]
fn surfaces_list_kind_requires_value() {
    let output = Command::new(binary_path())
        .args(["surfaces", "list", "--kind"])
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
fn surfaces_list_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["surfaces", "list"])
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
fn surfaces_show_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["surfaces", "show", "some-surface-ref"])
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
