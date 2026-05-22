//! CLI-level output mode tests for CLI-OUT-7: Governance Output.
//!
//! Tests that verify the CLI binary produces correct output for:
//!
//! ## Group 1: Assess
//! - `rmap assess` (human and --json)
//!
//! ## Group 2: Violations
//! - `rmap violations`
//!
//! ## Group 3: Gate
//! - `rmap gate`
//!
//! # Test Strategy
//!
//! All governance commands use REG-1 daemon contract:
//! - assess, violations, gate: cwd auto-discovery via daemon
//! - No db_path/repo_uid positional arguments
//!
//! Tests focus on:
//! - Argument parsing and error handling
//! - Output format switching (human vs --json)
//! - Flag acceptance
//!
//! # Running
//!
//! ```
//! cargo test -p repo-graph-rgr --test cli_out_7_governance -- --ignored
//! ```
//!
//! # Technical Debt
//!
//! **TD-CLI-OUT-7-A: Manual pre-build requirement**
//!
//! Same pattern as other CLI-OUT tests. These tests require binaries built first.
//! They are marked `#[ignore]` and run opt-in.

use std::path::PathBuf;
use std::process::{Command, Output};

fn rmap_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Run rmap command with given args.
fn run_rmap(args: &[&str]) -> Output {
    Command::new(rmap_binary_path())
        .args(args)
        .output()
        .expect("failed to execute rmap")
}

// ── Group 1: Assess (REG-1) ──────────────────────────────────────────────────

// Note: repo-not-found path tested in assess_command.rs::assess_from_temp_dir_fails

#[test]
#[ignore] // Requires binary pre-built
fn assess_shows_error_for_unknown_flag() {
    let output = run_rmap(&["assess", "--unknown-flag"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown flag") || stderr.contains("error:"),
        "expected unknown flag error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn assess_accepts_json_flag() {
    // --json should be accepted (will fail on daemon/repo, not unknown flag)
    let output = run_rmap(&["assess", "--json"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT complain about --json being unknown
    assert!(
        !stderr.contains("unknown flag: --json"),
        "expected --json to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn assess_accepts_baseline_flag() {
    let output = run_rmap(&["assess", "--baseline", "snap_123"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT complain about --baseline being unknown
    assert!(
        !stderr.contains("unknown flag: --baseline"),
        "expected --baseline to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn assess_accepts_all_flags_together() {
    let output = run_rmap(&["assess", "--baseline", "snap_123", "--json"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not complain about any unknown flags
    assert!(
        !stderr.contains("unknown flag"),
        "expected all flags to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn assess_baseline_requires_value() {
    let output = run_rmap(&["assess", "--baseline"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires") || stderr.contains("argument"),
        "expected error about missing baseline value, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn assess_unexpected_positional_is_error() {
    // REG-1: No positional arguments expected
    let output = run_rmap(&["assess", "unexpected_arg"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "expected unexpected argument error, got: {}",
        stderr
    );
}

// ── Group 2: Violations (REG-1) ──────────────────────────────────────────────

// Note: repo-not-found path tested in violations_command.rs::violations_from_temp_dir_fails

#[test]
#[ignore] // Requires binary pre-built
fn violations_shows_error_for_unknown_flag() {
    let output = run_rmap(&["violations", "--unknown-flag"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown flag") || stderr.contains("error:"),
        "expected unknown flag error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn violations_accepts_json_flag() {
    // --json should be accepted (will fail on daemon/repo, not unknown flag)
    let output = run_rmap(&["violations", "--json"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT complain about --json being unknown
    assert!(
        !stderr.contains("unknown flag: --json"),
        "expected --json to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn violations_unexpected_positional_is_error() {
    // REG-1: No positional arguments expected
    let output = run_rmap(&["violations", "unexpected_arg"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "expected unexpected argument error, got: {}",
        stderr
    );
}

// ── Group 3: Gate ────────────────────────────────────────────────────────────

#[test]
#[ignore] // Requires binary pre-built
fn gate_shows_error_for_unknown_argument() {
    let output = run_rmap(&["gate", "unexpected_arg"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("usage"),
        "expected unexpected argument error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn gate_shows_error_for_unknown_flag() {
    let output = run_rmap(&["gate", "--unknown-flag"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown flag"),
        "expected unknown flag error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn gate_accepts_json_flag() {
    // --json should be accepted (will fail on daemon unavailable, not unknown flag)
    let output = run_rmap(&["gate", "--json"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT complain about --json being unknown
    assert!(
        !stderr.contains("unknown flag: --json"),
        "expected --json to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn gate_accepts_strict_flag() {
    let output = run_rmap(&["gate", "--strict"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT complain about --strict being unknown
    assert!(
        !stderr.contains("unknown flag: --strict"),
        "expected --strict to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn gate_accepts_advisory_flag() {
    let output = run_rmap(&["gate", "--advisory"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT complain about --advisory being unknown
    assert!(
        !stderr.contains("unknown flag: --advisory"),
        "expected --advisory to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn gate_strict_advisory_mutually_exclusive() {
    let output = run_rmap(&["gate", "--strict", "--advisory"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mutually exclusive"),
        "expected mutually exclusive error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn gate_accepts_all_flags_together() {
    let output = run_rmap(&["gate", "--strict", "--json"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not complain about unknown flags
    assert!(
        !stderr.contains("unknown flag"),
        "expected all flags to be accepted, got: {}",
        stderr
    );
}
