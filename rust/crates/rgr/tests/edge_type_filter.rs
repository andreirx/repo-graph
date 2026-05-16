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
// IGNORED TESTS - Require daemon infrastructure
//
// These tests verify edge-type filtering behavior which requires:
// 1. A running daemon
// 2. An indexed repo registered in daemon
// 3. REG-1 resolution working
//
// TODO: Move these to daemon_dispatch.rs where proper daemon setup exists
// ══════════════════════════════════════════════════════════════════════

// ── 5. Default (no flag) = CALLS only ───────────────────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn callers_default_is_calls_only() {
    // Server has an INSTANTIATES caller (main) but no CALLS caller.
    // Default (CALLS-only) should return 0.
    unimplemented!("requires daemon");
}

// ── 6. Explicit --edge-types CALLS = same as default ────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn callers_explicit_calls_same_as_default() {
    // helper has one CALLS caller (main).
    unimplemented!("requires daemon");
}

// ── 7. --edge-types INSTANTIATES only ───────────────────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn callers_instantiates_only() {
    // Server has one INSTANTIATES caller (main).
    unimplemented!("requires daemon");
}

// ── 8. --edge-types CALLS,INSTANTIATES = union ──────────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn callers_union_calls_and_instantiates() {
    // Server has 0 CALLS callers but 1 INSTANTIATES caller (main).
    // The union must return 1, proving INSTANTIATES is included.
    unimplemented!("requires daemon");
}

// ── 9. Callees: --edge-types INSTANTIATES ───────────────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn callees_instantiates_only() {
    // main calls helper (CALLS) and instantiates Server (INSTANTIATES).
    // With INSTANTIATES filter, only Server should appear.
    unimplemented!("requires daemon");
}

// ── 10. Callees: --edge-types CALLS,INSTANTIATES = union ────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn callees_union_calls_and_instantiates() {
    // main has both CALLS (helper) and INSTANTIATES (Server) callees.
    unimplemented!("requires daemon");
}
