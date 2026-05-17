//! Deterministic tests for `--edge-types` filter on callers/callees.
//!
//! # REG-1 Contract
//!
//! With REG-1, callers/callees commands require daemon and resolve repo from cwd.
//! New contract: `rmap callers <symbol> [--edge-types <types>]`
//!
//! ## Test Categories
//!
//! 1. **Usage error tests (1-4)**: Test CLI parsing without daemon
//! 2. **Success tests (5-10)**: IGNORED - require daemon infrastructure
//!    These should be moved to daemon_dispatch.rs for proper testing
//!
//! Test matrix:
//!   1.  Invalid edge type → usage error
//!   2.  Missing --edge-types value → usage error
//!   3.  Repeated --edge-types flag → usage error
//!   4.  Empty --edge-types value → usage error
//!   5.  Default (no flag) = CALLS only
//!   6.  Explicit --edge-types CALLS = same as default
//!   7.  --edge-types INSTANTIATES only
//!   8.  --edge-types CALLS,INSTANTIATES = union
//!   9.  Callees symmetry: --edge-types INSTANTIATES
//!   10. Callees symmetry: --edge-types CALLS,INSTANTIATES

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Returns a non-existent socket path for test isolation.
fn isolated_socket_path(dir: &std::path::Path) -> PathBuf {
    dir.join("nonexistent-daemon.sock")
}

fn run_cmd_isolated(args: &[&str]) -> std::process::Output {
    let dir = tempfile::tempdir().unwrap();
    Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(dir.path()))
        .args(args)
        .output()
        .unwrap()
}

// ── 1. Invalid edge type ────────────────────────────────────────

#[test]
fn callers_invalid_edge_type() {
    // REG-1: callers <symbol> [--edge-types <types>]
    let output = run_cmd_isolated(&["callers", "helper", "--edge-types", "BOGUS"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown edge type"),
        "expected unknown edge type error, stderr: {}",
        stderr
    );
}

// ── 2. Missing --edge-types value ───────────────────────────────

#[test]
fn callers_missing_edge_types_value() {
    // REG-1: callers <symbol> [--edge-types <types>]
    let output = run_cmd_isolated(&["callers", "helper", "--edge-types"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing value"),
        "expected missing value error, stderr: {}",
        stderr
    );
}

// ── 3. Repeated --edge-types flag ───────────────────────────────

#[test]
fn callers_repeated_edge_types_flag() {
    // REG-1: callers <symbol> [--edge-types <types>]
    let output = run_cmd_isolated(&[
        "callers",
        "helper",
        "--edge-types",
        "CALLS",
        "--edge-types",
        "INSTANTIATES",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("repeated"),
        "expected repeated flag error, stderr: {}",
        stderr
    );
}

// ── 4. Empty --edge-types value ─────────────────────────────────

#[test]
fn callers_empty_edge_types_value() {
    // REG-1: callers <symbol> [--edge-types <types>]
    let output = run_cmd_isolated(&["callers", "helper", "--edge-types", ""]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

// ══════════════════════════════════════════════════════════════════════
// SUCCESS-PATH TESTS
//
// Edge-type filtering success tests (5-10) belong in daemon_dispatch.rs
// where daemon infrastructure is available. The tests would verify:
// - Default (no flag) = CALLS only
// - Explicit --edge-types CALLS = same as default
// - --edge-types INSTANTIATES only
// - --edge-types CALLS,INSTANTIATES = union
// - Callees symmetry
//
// Stub tests deleted as part of REG-1 cleanup.
// ══════════════════════════════════════════════════════════════════════
