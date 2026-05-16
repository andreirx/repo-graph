//! Contract tests for the TS-compatible QueryResult JSON envelope.
//!
//! # REG-1 Contract
//!
//! With REG-1, query commands (callers, callees, cycles, stats) require daemon
//! and resolve repo from cwd. These tests use the old contract and need to be
//! migrated to daemon_dispatch.rs for proper daemon-based testing.
//!
//! ## Purpose
//!
//! These tests pin the envelope shape across read-side commands to prevent
//! silent drift from the established TS `formatQueryResult` contract.
//!
//! Each test verifies:
//!   - All 8 envelope fields are present and typed correctly
//!   - command discriminator matches TS naming ("graph <cmd>")
//!   - stdout/stderr discipline (JSON only on stdout, empty stderr)
//!   - exit code 0 on success
//!
//! Added in Rust-16 (consolidation slice).
//!
//! ## Current Status
//!
//! IGNORED - require daemon infrastructure. Tests should be moved to
//! daemon_dispatch.rs to use proper REG-1 daemon resolution.

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Returns a non-existent socket path for test isolation.
fn isolated_socket_path(dir: &Path) -> PathBuf {
    dir.join("nonexistent-daemon.sock")
}

/// Assert the 8 standard QueryResult envelope fields.
#[allow(dead_code)]
fn assert_envelope(result: &serde_json::Value, expected_command: &str) {
    assert_eq!(
        result["command"].as_str().unwrap(),
        expected_command,
        "command discriminator mismatch"
    );
    assert!(
        result["repo"].is_string(),
        "repo must be string, got: {}",
        result["repo"]
    );
    assert!(
        result["snapshot"].is_string(),
        "snapshot must be string, got: {}",
        result["snapshot"]
    );
    let scope = result["snapshot_scope"].as_str().unwrap();
    assert!(
        scope == "full" || scope == "incremental",
        "snapshot_scope must be full or incremental, got: {}",
        scope
    );
    assert!(
        result["basis_commit"].is_null() || result["basis_commit"].is_string(),
        "basis_commit must be string or null, got: {}",
        result["basis_commit"]
    );
    assert!(result["results"].is_array(), "results must be array");
    assert!(result["count"].is_number(), "count must be number");
    assert!(
        result["stale"].is_boolean(),
        "stale must be boolean, got: {}",
        result["stale"]
    );
}

// ══════════════════════════════════════════════════════════════════════
// IGNORED TESTS - Require daemon infrastructure
//
// These tests verify JSON envelope contract which requires:
// 1. A running daemon
// 2. An indexed repo registered in daemon
// 3. REG-1 resolution working
//
// TODO: Move these to daemon_dispatch.rs where proper daemon setup exists
// ══════════════════════════════════════════════════════════════════════

// ── callers envelope ────────────────────────────────────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn callers_envelope_contract() {
    unimplemented!("requires daemon");
}

// ── callees envelope ────────────────────────────────────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn callees_envelope_contract() {
    unimplemented!("requires daemon");
}

// ── dead envelope ───────────────────────────────────────────────

/// Dead command is deliberately disabled (2026-04-27) due to high
/// false-positive rates. This test verifies the disabled contract:
/// - Exit code 2 (runtime error / not available)
/// - Error message on stderr
/// - No JSON on stdout
///
/// When the dead surface is reintroduced with coverage-backed or
/// framework-liveness-backed evidence, this test should be restored
/// to verify the QueryResult envelope shape.
#[test]
fn dead_envelope_contract() {
    let repo_dir = tempfile::tempdir().unwrap();
    let root = repo_dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
    std::fs::write(root.join("src/index.ts"), "export function foo() {}\n").unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();

    let db_str = db_path.to_str().unwrap();

    let output = Command::new(binary_path())
        .env("RMAP_SOCKET_PATH", isolated_socket_path(db_dir.path()))
        .args(["dead", db_str, "r1", "SYMBOL"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "dead command should exit 2 (disabled), got: {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("rmap dead` is disabled"),
        "stderr should explain disabled state"
    );
}

// ── cycles envelope ─────────────────────────────────────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn cycles_envelope_contract() {
    unimplemented!("requires daemon");
}

// ── stats envelope ──────────────────────────────────────────────

#[test]
#[ignore = "REG-1: requires daemon infrastructure - move to daemon_dispatch.rs"]
fn stats_envelope_contract() {
    unimplemented!("requires daemon");
}
