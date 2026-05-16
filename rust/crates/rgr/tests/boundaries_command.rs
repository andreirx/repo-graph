//! Deterministic tests for the `boundaries` command family.
//!
//! REG-1 Contract:
//!   - `rmap boundaries list [filters...]` — list boundary interactions (from cwd)
//!   - `rmap boundaries show <surface_uid>` — show boundary detail (from cwd)
//!   - `rmap boundaries summary` — boundary summary (from cwd)
//!   - `rmap boundaries links [--service <name>]` — list links (from cwd)
//!
//! Test matrix:
//!   1-8. Usage errors (no subcommand, unknown subcommand, missing args, invalid options)
//!   9-12. Daemon required tests
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - boundaries_list_returns_envelope
//!   - boundaries_list_repo_not_indexed_returns_error
//!   - boundaries_list_with_filter
//!   - boundaries_show_returns_detail
//!   - boundaries_show_surface_not_found_returns_error
//!   - boundaries_summary_returns_summary
//!   - boundaries_links_returns_envelope

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
fn boundaries_usage_error_no_subcommand() {
    let output = Command::new(binary_path())
        .args(["boundaries"])
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
fn boundaries_usage_error_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["boundaries", "unknown"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown boundaries subcommand"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn boundaries_show_usage_error_missing_surface_uid() {
    let output = Command::new(binary_path())
        .args(["boundaries", "show"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn boundaries_list_unknown_option() {
    let output = Command::new(binary_path())
        .args(["boundaries", "list", "--unknown-flag", "value"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {}", stderr);
}

#[test]
fn boundaries_list_kind_requires_value() {
    let output = Command::new(binary_path())
        .args(["boundaries", "list", "--kind"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a value"), "stderr: {}", stderr);
}

#[test]
fn boundaries_links_unknown_option() {
    let output = Command::new(binary_path())
        .args(["boundaries", "links", "--unknown-flag", "value"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {}", stderr);
}

#[test]
fn boundaries_links_service_requires_value() {
    let output = Command::new(binary_path())
        .args(["boundaries", "links", "--service"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a value"), "stderr: {}", stderr);
}

#[test]
fn boundaries_summary_unexpected_arg() {
    let output = Command::new(binary_path())
        .args(["boundaries", "summary", "extra-arg"])
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
fn boundaries_list_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["boundaries", "list"])
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
fn boundaries_show_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["boundaries", "show", "some-surface-uid"])
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
fn boundaries_summary_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["boundaries", "summary"])
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
fn boundaries_links_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["boundaries", "links"])
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
