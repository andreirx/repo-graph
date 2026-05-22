//! Tests for the `rmap assess` command (REG-1 contract).
//!
//! LEGACY-CONTRACT-MIGRATION-1C: Migrated from legacy db_path/repo_uid contract.
//!
//! # REG-1 Contract
//!
//! The assess command uses daemon-based repo discovery:
//! - Repo is resolved from cwd via daemon registry
//! - No db_path or repo_uid positional arguments
//! - Usage: `rmap assess [--baseline <snapshot_uid>] [--json]`
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
//! 3. Run tests: `cargo test -p repo-graph-rgr --test assess_command -- --ignored`

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
fn assess_unknown_flag_is_usage_error() {
    let output = run_cmd(&["assess", "--unknown-flag"]);
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
fn assess_unexpected_positional_is_usage_error() {
    // REG-1: No positional arguments expected
    let output = run_cmd(&["assess", "unexpected_arg"]);
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
fn assess_baseline_without_value_is_usage_error() {
    let output = run_cmd(&["assess", "--baseline"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "--baseline without value should be usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires"),
        "should mention requires argument: {}",
        stderr
    );
}

#[test]
fn assess_json_flag_accepted() {
    // Running from non-repo will fail, but --json should not be "unknown flag"
    let output = run_cmd(&["assess", "--json"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag: --json"),
        "--json should be accepted: {}",
        stderr
    );
}

#[test]
fn assess_baseline_flag_accepted() {
    // Running from non-repo will fail, but --baseline should not be "unknown flag"
    let output = run_cmd(&["assess", "--baseline", "snap-123"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag: --baseline"),
        "--baseline should be accepted: {}",
        stderr
    );
}

#[test]
fn assess_all_flags_accepted() {
    let output = run_cmd(&["assess", "--baseline", "snap-123", "--json"]);
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
fn assess_from_temp_dir_fails() {
    // Running from a temp directory (not a repo) should fail
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .current_dir(temp.path())
        .args(["assess"])
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
// - No policies => empty assessment, exit 0
// - Absolute policy evaluation => PASS/FAIL counted
// - Comparative policy without --baseline => error
// - Comparative policy with --baseline => evaluation succeeds
// - JSON output shape validation
// - Write-path: assessments persisted correctly
//
// The tests are marked #[ignore] because they require daemon infrastructure.
// Run with: cargo test -p repo-graph-rgr --test assess_command -- --ignored

#[test]
#[ignore]
fn assess_no_policies_returns_empty() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with no quality policies
    // 3. Running from that repo's directory
    //
    // Expected: exit 0, assessments.total = 0
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn assess_absolute_policy_pass() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with an absolute_max policy (high threshold)
    // 3. Running from that repo's directory
    //
    // Expected: exit 0, assessments.pass = 1
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn assess_absolute_policy_fail() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with an absolute_max policy (threshold 0)
    // 3. Running from that repo's directory
    //
    // Expected: exit 0, assessments.fail = 1
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn assess_comparative_policy_missing_baseline() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with a no_new policy
    // 3. Running from that repo's directory WITHOUT --baseline
    //
    // Expected: exit 2, error mentions baseline
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn assess_comparative_policy_with_baseline() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with a no_worsened policy
    // 3. Running from that repo's directory WITH --baseline <valid_snap>
    //
    // Expected: exit 0, baseline_snapshot in output
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn assess_json_output_shape() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo
    // 3. Running with --json
    //
    // Expected: JSON with command, repo, snapshot, assessments, baseline_required_count
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn assess_write_path_persists_assessments() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with policies
    // 3. Verify assessments are persisted in storage
    //
    // Expected: quality_assessments table contains assessment rows
    unimplemented!("requires daemon harness");
}
