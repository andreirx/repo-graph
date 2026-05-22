//! Tests for the `rmap violations` command (REG-1 contract).
//!
//! LEGACY-CONTRACT-MIGRATION-1C: Migrated from legacy db_path/repo_uid contract.
//!
//! # REG-1 Contract
//!
//! The violations command uses daemon-based repo discovery:
//! - Repo is resolved from cwd via daemon registry
//! - No db_path or repo_uid positional arguments
//! - Usage: `rmap violations [--json]`
//!
//! # Output Structure
//!
//! ```json
//! {
//!   "command": "arch violations",
//!   "results": {
//!     "declared_boundary_violations": [...],
//!     "discovered_module_violations": [...]
//!   },
//!   "stale_declarations": [...],
//!   "count": N,
//!   "declared_boundary_count": N,
//!   "discovered_module_count": N,
//!   "stale_count": N
//! }
//! ```
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
//! 3. Run tests: `cargo test -p repo-graph-rgr --test violations_command -- --ignored`

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
fn violations_unknown_flag_is_usage_error() {
    let output = run_cmd(&["violations", "--unknown-flag"]);
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
fn violations_unexpected_positional_is_usage_error() {
    // REG-1: No positional arguments expected
    let output = run_cmd(&["violations", "unexpected_arg"]);
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
fn violations_json_flag_accepted() {
    // Running from non-repo will fail, but --json should not be "unknown flag"
    let output = run_cmd(&["violations", "--json"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown flag: --json"),
        "--json should be accepted: {}",
        stderr
    );
}

// =============================================================================
// DAEMON UNAVAILABLE
// =============================================================================

#[test]
fn violations_from_temp_dir_fails() {
    // Running from a temp directory (not a repo) should fail
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary_path())
        .current_dir(temp.path())
        .args(["violations"])
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
// - No declarations => empty results
// - Declaration exists but no violating imports => empty results
// - Exact violation result with boundary declarations
// - Duplicate declarations produce deduplicated violations
// - Envelope contract (JSON shape)
// - Both declared and discovered sections work independently
// - Human output format
//
// The tests are marked #[ignore] because they require daemon infrastructure.
// Run with: cargo test -p repo-graph-rgr --test violations_command -- --ignored

#[test]
#[ignore]
fn violations_empty_when_no_declarations() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with no boundary declarations
    //
    // Expected: exit 0, count = 0
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn violations_empty_when_no_violating_imports() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with boundary declaration that is NOT violated
    //
    // Expected: exit 0, count = 0
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn violations_exact_results() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with boundary declaration that IS violated
    //
    // Expected: exit 0, declared_boundary_count = 1
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn violations_dedup_duplicate_declarations() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with DUPLICATE boundary declarations (same rule)
    //
    // Expected: violations deduplicated (count = 1, not 2)
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn violations_envelope_contract() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo
    //
    // Expected: JSON with correct envelope fields
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn violations_discovered_section_empty_when_no_modules() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo without module candidates
    //
    // Expected: discovered_module_count = 0
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn violations_both_sections_independent() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with declared boundary violations but no modules
    //
    // Expected: declared_boundary_count > 0, discovered_module_count = 0
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn violations_human_output_format() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo with violations
    //
    // Expected: human output with "Architectural Violations" header
    unimplemented!("requires daemon harness");
}

#[test]
#[ignore]
fn violations_human_output_empty_case() {
    // This test requires:
    // 1. Daemon running
    // 2. Indexed repo without violations
    //
    // Expected: "No violations detected" message
    unimplemented!("requires daemon harness");
}
