//! Tests for the `rmap policy` command (REG-1 contract).
//!
//! LEGACY-CONTRACT-MIGRATION-1D: Migrated from legacy db_path/repo_uid contract.
//!
//! # REG-1 Contract
//!
//! The policy command uses daemon-based repo discovery:
//! - Repo is resolved from cwd via daemon registry
//! - No db_path or repo_uid positional arguments
//! - Usage: `rmap policy [--kind ...] [--file ...] [--callee ...] [--fate ...] [--json]`
//!
//! # Test Strategy
//!
//! - CLI argument parsing (no daemon needed)
//! - Daemon-unavailable behavior
//! - Full integration tests require daemon running + indexed repo (marked #[ignore])
//!
//! # Running Integration Tests
//!
//! For full integration tests:
//! 1. Start daemon: `rmapd`
//! 2. Index test repo: `cd <repo> && rmap index .`
//! 3. Run tests: `cargo test -p repo-graph-rgr --test policy_command -- --ignored`

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

fn run_cmd(args: &[&str]) -> std::process::Output {
    Command::new(binary_path()).args(args).output().unwrap()
}

// =============================================================================
// CLI ARGUMENT PARSING (no daemon needed)
// =============================================================================

#[test]
fn policy_unknown_flag_is_usage_error() {
    let output = run_cmd(&["policy", "--unknown-flag"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "unknown flag should be usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown flag"),
        "should mention unknown flag: {}",
        stderr
    );
}

#[test]
fn policy_unexpected_positional_is_usage_error() {
    // REG-1: No positional arguments expected
    let output = run_cmd(&["policy", "unexpected_arg"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "unexpected positional should be usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected"),
        "should mention unexpected argument: {}",
        stderr
    );
}

#[test]
fn policy_json_flag_accepted() {
    // Running from non-repo will fail, but --json should not be "unknown flag"
    let output = run_cmd(&["policy", "--json"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag: --json"),
        "--json should be accepted: {}",
        stderr
    );
}

#[test]
fn policy_kind_flag_accepted() {
    let output = run_cmd(&["policy", "--kind", "RETURN_FATE"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag: --kind"),
        "--kind should be accepted: {}",
        stderr
    );
}

#[test]
fn policy_file_flag_accepted() {
    let output = run_cmd(&["policy", "--file", "src/main.c"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag: --file"),
        "--file should be accepted: {}",
        stderr
    );
}

#[test]
fn policy_callee_flag_accepted() {
    let output = run_cmd(&["policy", "--kind", "RETURN_FATE", "--callee", "get_status"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag: --callee"),
        "--callee should be accepted: {}",
        stderr
    );
}

#[test]
fn policy_fate_flag_accepted() {
    let output = run_cmd(&["policy", "--kind", "RETURN_FATE", "--fate", "CHECKED"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag: --fate"),
        "--fate should be accepted: {}",
        stderr
    );
}

#[test]
fn policy_invalid_kind_is_usage_error() {
    let output = run_cmd(&["policy", "--kind", "INVALID_KIND"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "invalid kind should be usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported policy kind"),
        "should mention unsupported kind: {}",
        stderr
    );
}

#[test]
fn policy_kind_requires_value() {
    let output = run_cmd(&["policy", "--kind"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "--kind without value should be usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires"),
        "should mention requires argument: {}",
        stderr
    );
}

#[test]
fn policy_file_requires_value() {
    let output = run_cmd(&["policy", "--file"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "--file without value should be usage error"
    );
}

#[test]
fn policy_callee_requires_value() {
    let output = run_cmd(&["policy", "--callee"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "--callee without value should be usage error"
    );
}

#[test]
fn policy_fate_requires_value() {
    let output = run_cmd(&["policy", "--fate"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "--fate without value should be usage error"
    );
}

#[test]
fn policy_all_flags_accepted() {
    let output = run_cmd(&[
        "policy",
        "--kind",
        "RETURN_FATE",
        "--file",
        "src/main.c",
        "--callee",
        "get_status",
        "--fate",
        "CHECKED",
        "--json",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag"),
        "all flags should be accepted: {}",
        stderr
    );
}

// =============================================================================
// DAEMON UNAVAILABLE
// =============================================================================

#[test]
fn policy_from_temp_dir_fails() {
    // Running from a temp directory (not a repo) should fail
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .current_dir(temp.path())
        .args(["policy"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(2),
        "should be runtime error (daemon/repo not found)"
    );
}

// =============================================================================
// INTEGRATION TESTS (require daemon running + indexed repo)
// =============================================================================

// Note: The following tests require:
// 1. A running daemon (`rmapd`)
// 2. A repo indexed and registered with the daemon
// 3. Running the test from that repo's directory
//
// These tests verify:
// - STATUS_MAPPING query returns correct envelope
// - BEHAVIORAL_MARKER query returns correct envelope
// - RETURN_FATE query returns correct envelope with summary
// - File filter restricts results
// - Callee filter restricts RETURN_FATE results
// - Fate filter restricts RETURN_FATE results
// - Human output format
//
// The tests are marked #[ignore] because they require daemon infrastructure.
// Run with: cargo test -p repo-graph-rgr --test policy_command -- --ignored

#[test]
#[ignore]
fn policy_status_mapping_envelope() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with STATUS_MAPPING facts (C code with status translation)
    //
    // Expected: JSON with repo, snapshot, kind, count, facts fields
    unimplemented!("requires daemon harness with C repo containing status mappings");
}

#[test]
#[ignore]
fn policy_behavioral_marker_envelope() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with BEHAVIORAL_MARKER facts (C code with retry loops)
    //
    // Expected: JSON with repo, snapshot, kind, count, facts fields
    unimplemented!("requires daemon harness with C repo containing behavioral markers");
}

#[test]
#[ignore]
fn policy_return_fate_envelope() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with RETURN_FATE facts (C code with return value handling)
    //
    // Expected: JSON with repo, snapshot, kind, count, facts, summary fields
    unimplemented!("requires daemon harness with C repo containing return fates");
}

#[test]
#[ignore]
fn policy_file_filter() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with policy facts in multiple files
    //
    // Expected: --file filter restricts results to matching file
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn policy_callee_filter() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with RETURN_FATE facts
    //
    // Expected: --callee filter restricts results to matching callee name
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn policy_fate_filter() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with RETURN_FATE facts of various kinds
    //
    // Expected: --fate filter restricts results to matching fate kind
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn policy_human_output_format() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with policy facts
    //
    // Expected: human output with "Policy Facts:" header
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn policy_empty_results() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo without policy facts
    //
    // Expected: exit 0, count = 0, hint message
    unimplemented!("requires daemon harness");
}
