//! CLI tests for the `cycles` command.
//!
//! # REG-1 Contract
//!
//! With REG-1, the `cycles` command requires daemon and resolves repo from cwd.
//! The old positional `<db_path> <repo_uid>` contract is obsolete.
//!
//! New contract: `rmap cycles`
//!
//! Full integration tests for the new contract are in daemon_dispatch.rs
//! since they require daemon coordination.

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

#[test]
fn cycles_no_args_is_valid() {
    // With REG-1, cycles takes no positional arguments - repo comes from cwd
    let output = Command::new(binary_path())
        .args(["cycles"])
        .output()
        .unwrap();

    // Exit code 2 = runtime error (daemon unavailable or repo not indexed)
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should have error message");
}

#[test]
fn cycles_unexpected_args_is_usage_error() {
    // With REG-1, cycles takes no positional arguments
    let output = Command::new(binary_path())
        .args(["cycles", "unexpected_arg"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "stderr: {}",
        stderr
    );
}
