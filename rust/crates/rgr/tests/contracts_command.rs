//! Deterministic tests for the `contracts` command family.
//!
//! REG-1 Contract:
//!   - `rmap contracts list [--kind protobuf]` — list schemas (from cwd)
//!   - `rmap contracts show <file_path>` — show schema detail (from cwd)
//!   - `rmap contracts elements [--kind|--file]` — list elements (from cwd)
//!   - `rmap contracts usages [--element|--min-confidence]` — list mappings (from cwd)
//!
//! Test matrix:
//!   1-8. Usage errors (no subcommand, unknown subcommand, missing args, unknown options)
//!   9-12. Daemon required tests
//!
//! Success-path tests are in daemon_dispatch.rs:
//!   - contracts_list_returns_envelope
//!   - contracts_list_repo_not_indexed_returns_error
//!   - contracts_list_with_kind_filter
//!   - contracts_show_returns_schema_detail
//!   - contracts_elements_returns_envelope
//!   - contracts_usages_returns_envelope

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
fn contracts_usage_error_no_subcommand() {
    let output = Command::new(binary_path())
        .args(["contracts"])
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
fn contracts_usage_error_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["contracts", "unknown"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown contracts subcommand"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn contracts_show_usage_error_missing_file_path() {
    let output = Command::new(binary_path())
        .args(["contracts", "show"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn contracts_list_unknown_option_exit_1() {
    let output = Command::new(binary_path())
        .args(["contracts", "list", "--unknown-flag", "value"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {}", stderr);
}

#[test]
fn contracts_elements_unknown_option_exit_1() {
    let output = Command::new(binary_path())
        .args(["contracts", "elements", "--unknown-flag", "value"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {}", stderr);
}

#[test]
fn contracts_usages_unknown_option_exit_1() {
    let output = Command::new(binary_path())
        .args(["contracts", "usages", "--unknown-flag", "value"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {}", stderr);
}

#[test]
fn contracts_usages_element_requires_value() {
    let output = Command::new(binary_path())
        .args(["contracts", "usages", "--element"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a value"), "stderr: {}", stderr);
}

#[test]
fn contracts_usages_min_confidence_requires_value() {
    let output = Command::new(binary_path())
        .args(["contracts", "usages", "--min-confidence"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a value"), "stderr: {}", stderr);
}

#[test]
fn contracts_usages_min_confidence_invalid_value() {
    let output = Command::new(binary_path())
        .args(["contracts", "usages", "--min-confidence", "1.5"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("between 0.0 and 1.0"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// DAEMON REQUIRED
// ══════════════════════════════════════════════════════════════════

#[test]
fn contracts_list_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["contracts", "list"])
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
fn contracts_show_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["contracts", "show", "api.proto"])
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
fn contracts_elements_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["contracts", "elements"])
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
fn contracts_usages_daemon_required() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .args(["contracts", "usages"])
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
