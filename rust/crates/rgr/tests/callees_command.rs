//! CLI tests for the `callees` command.
//!
//! # REG-1 Contract
//!
//! With REG-1, the `callees` command requires daemon and resolves repo from cwd.
//! The old positional `<db_path> <repo_uid>` contract is obsolete.
//!
//! New contract: `rmap callees <symbol> [--edge-types <types>]`
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
fn callees_usage_error_no_args() {
    // With REG-1, callees requires a symbol argument
    let output = Command::new(binary_path())
        .args(["callees"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn callees_daemon_required() {
    // With REG-1, callees requires daemon. Returns exit code 2 when:
    // - Daemon unavailable: error mentions "daemon" or "connect"
    // - Repo not indexed: error mentions "repo" or "not indexed"
    // - Invalid request: any daemon error
    let output = Command::new(binary_path())
        .args(["callees", "someSymbol"])
        .output()
        .unwrap();

    // Exit code 2 = runtime error
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify we got some error output (daemon-related or repo-related)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.is_empty(), "Should have error message on stderr");
}
