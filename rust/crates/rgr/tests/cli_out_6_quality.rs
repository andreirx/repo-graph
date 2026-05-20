//! CLI-level output mode tests for CLI-OUT-6: Quality/Risk Output (Group 1).
//!
//! Tests that verify the CLI binary produces correct output for:
//!
//! ## Group 1: Volatility/Hotspots
//! - `rmap churn` (human and --json)
//! - `rmap hotspots` (human and --json)
//!
//! # Test Strategy
//!
//! These commands use legacy direct-storage contract (explicit db_path/repo_uid),
//! not REG-1 daemon. Tests focus on:
//! - Argument parsing and error handling
//! - Output format switching (human vs --json)
//!
//! Positive-path tests with real churn data are deferred to corpus validation.
//!
//! # Running
//!
//! ```
//! cargo test -p repo-graph-rgr --test cli_out_6_quality -- --ignored
//! ```
//!
//! # Technical Debt
//!
//! **TD-CLI-OUT-6-A: Manual pre-build requirement**
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

// ── Group 1: Churn ───────────────────────────────────────────────────────────

#[test]
#[ignore] // Requires binary pre-built
fn churn_shows_usage_without_args() {
    let output = run_rmap(&["churn"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage:"),
        "expected usage message, got: {}",
        stderr
    );
    assert!(stderr.contains("db_path"));
}

#[test]
#[ignore] // Requires binary pre-built
fn churn_shows_error_for_invalid_db() {
    let output = run_rmap(&["churn", "/nonexistent/path.db", "fake_repo"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:"),
        "expected error message, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn churn_shows_error_for_unknown_argument() {
    let output = run_rmap(&["churn", "/tmp/test.db", "repo", "--unknown-flag"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown argument") || stderr.contains("error:"),
        "expected unknown argument error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn churn_accepts_json_flag() {
    // Even with invalid db, --json should be accepted as a valid flag
    // (error will be about the db, not about --json being unknown)
    let output = run_rmap(&["churn", "/nonexistent/path.db", "fake_repo", "--json"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT complain about --json being unknown
    assert!(
        !stderr.contains("unknown argument: --json"),
        "expected --json to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn churn_accepts_since_flag() {
    let output = run_rmap(&[
        "churn",
        "/nonexistent/path.db",
        "fake_repo",
        "--since",
        "30.days.ago",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT complain about --since being unknown
    assert!(
        !stderr.contains("unknown argument: --since"),
        "expected --since to be accepted, got: {}",
        stderr
    );
}

// ── Group 1: Hotspots ────────────────────────────────────────────────────────

#[test]
#[ignore] // Requires binary pre-built
fn hotspots_shows_usage_without_args() {
    let output = run_rmap(&["hotspots"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage:"),
        "expected usage message, got: {}",
        stderr
    );
    assert!(stderr.contains("db_path"));
}

#[test]
#[ignore] // Requires binary pre-built
fn hotspots_shows_error_for_invalid_db() {
    let output = run_rmap(&["hotspots", "/nonexistent/path.db", "fake_repo"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:"),
        "expected error message, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn hotspots_shows_error_for_unknown_argument() {
    let output = run_rmap(&["hotspots", "/tmp/test.db", "repo", "--unknown-flag"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown argument") || stderr.contains("error:"),
        "expected unknown argument error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn hotspots_accepts_json_flag() {
    let output = run_rmap(&["hotspots", "/nonexistent/path.db", "fake_repo", "--json"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown argument: --json"),
        "expected --json to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn hotspots_accepts_since_flag() {
    let output = run_rmap(&[
        "hotspots",
        "/nonexistent/path.db",
        "fake_repo",
        "--since",
        "30.days.ago",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown argument: --since"),
        "expected --since to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn hotspots_accepts_exclude_tests_flag() {
    let output = run_rmap(&[
        "hotspots",
        "/nonexistent/path.db",
        "fake_repo",
        "--exclude-tests",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown argument: --exclude-tests"),
        "expected --exclude-tests to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn hotspots_accepts_exclude_vendored_flag() {
    let output = run_rmap(&[
        "hotspots",
        "/nonexistent/path.db",
        "fake_repo",
        "--exclude-vendored",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown argument: --exclude-vendored"),
        "expected --exclude-vendored to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn hotspots_accepts_all_flags_together() {
    let output = run_rmap(&[
        "hotspots",
        "/nonexistent/path.db",
        "fake_repo",
        "--since",
        "7.days.ago",
        "--exclude-tests",
        "--exclude-vendored",
        "--json",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not complain about any unknown flags
    assert!(
        !stderr.contains("unknown argument"),
        "expected all flags to be accepted, got: {}",
        stderr
    );
}

// ── Group 2: Risk ────────────────────────────────────────────────────────────

#[test]
#[ignore] // Requires binary pre-built
fn risk_shows_usage_without_args() {
    let output = run_rmap(&["risk"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage:"),
        "expected usage message, got: {}",
        stderr
    );
    assert!(stderr.contains("db_path"));
}

#[test]
#[ignore] // Requires binary pre-built
fn risk_shows_error_for_invalid_db() {
    let output = run_rmap(&["risk", "/nonexistent/path.db", "fake_repo"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:"),
        "expected error message, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn risk_shows_error_for_unknown_argument() {
    let output = run_rmap(&["risk", "/tmp/test.db", "repo", "--unknown-flag"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown argument") || stderr.contains("error:"),
        "expected unknown argument error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn risk_accepts_json_flag() {
    let output = run_rmap(&["risk", "/nonexistent/path.db", "fake_repo", "--json"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown argument: --json"),
        "expected --json to be accepted, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn risk_accepts_since_flag() {
    let output = run_rmap(&[
        "risk",
        "/nonexistent/path.db",
        "fake_repo",
        "--since",
        "30.days.ago",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown argument: --since"),
        "expected --since to be accepted, got: {}",
        stderr
    );
}

// ── Group 3: Coverage ────────────────────────────────────────────────────────

#[test]
#[ignore] // Requires binary pre-built
fn coverage_shows_usage_without_args() {
    let output = run_rmap(&["coverage"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage:"),
        "expected usage message, got: {}",
        stderr
    );
    assert!(stderr.contains("db_path"));
    assert!(stderr.contains("report_path"));
}

#[test]
#[ignore] // Requires binary pre-built
fn coverage_shows_usage_with_only_two_args() {
    let output = run_rmap(&["coverage", "/tmp/test.db", "repo"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage:"),
        "expected usage message, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn coverage_shows_error_for_missing_report() {
    let output = run_rmap(&[
        "coverage",
        "/nonexistent/path.db",
        "fake_repo",
        "/nonexistent/report.json",
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error:") || stderr.contains("not found"),
        "expected error about missing file, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn coverage_shows_error_for_unknown_argument() {
    let output = run_rmap(&[
        "coverage",
        "/tmp/test.db",
        "repo",
        "/tmp/report.json",
        "--unknown-flag",
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown argument"),
        "expected unknown argument error, got: {}",
        stderr
    );
}

#[test]
#[ignore] // Requires binary pre-built
fn coverage_accepts_json_flag() {
    // Even with nonexistent files, --json should be accepted as a valid flag
    let output = run_rmap(&[
        "coverage",
        "/nonexistent/path.db",
        "fake_repo",
        "/nonexistent/report.json",
        "--json",
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unknown argument: --json"),
        "expected --json to be accepted, got: {}",
        stderr
    );
}
