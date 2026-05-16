//! CLI tests for the `callers` command.
//!
//! # REG-1 Contract
//!
//! With REG-1, the `callers` command requires daemon and resolves repo from cwd.
//! The old positional `<db_path> <repo_uid>` contract is obsolete.
//!
//! New contract: `rmap callers <symbol> [--edge-types <types>]`
//!
//! Full integration tests for the new contract are in daemon_dispatch.rs
//! since they require daemon coordination.

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

// -- REG-1: Usage error ---------------------------------------------------

#[test]
fn callers_usage_error_no_args() {
    // With REG-1, callers requires a symbol argument
    let output = Command::new(binary_path())
        .args(["callers"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn callers_daemon_required() {
    // With REG-1, callers requires daemon. Returns exit code 2 when:
    // - Daemon unavailable
    // - Repo not indexed
    // - Any other runtime error
    let output = Command::new(binary_path())
        .args(["callers", "someSymbol"])
        .output()
        .unwrap();

    // Exit code 2 = runtime error
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
