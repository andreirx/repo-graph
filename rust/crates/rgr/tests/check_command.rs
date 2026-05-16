//! CLI tests for the `check` command.
//!
//! # REG-1 Contract
//!
//! With REG-1, the `check` command requires daemon and resolves repo from cwd.
//! The old positional `<db_path> <repo_uid>` contract is obsolete.
//!
//! New contract: `rmap check`
//!
//! Full integration tests for the new contract are in daemon_dispatch.rs
//! since they require daemon coordination.

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

// -- REG-1: Usage tests ---------------------------------------------------

#[test]
fn check_no_args_is_valid() {
    // With REG-1, check takes no positional arguments - repo comes from cwd
    // Running without daemon returns exit 2 (runtime error), not exit 1 (usage)
    let output = Command::new(binary_path())
        .args(["check"])
        .output()
        .unwrap();

    // Exit code 2 = runtime error (daemon unavailable or repo not indexed)
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify we got some error output
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "Should have error message on stderr"
    );
}

#[test]
fn check_unexpected_args_is_usage_error() {
    // With REG-1, check takes no positional arguments
    let output = Command::new(binary_path())
        .args(["check", "unexpected_arg"])
        .output()
        .unwrap();

    // Exit code 1 = usage error
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "stderr should mention unexpected args or usage: {}",
        stderr
    );
}
