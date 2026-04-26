//! Deterministic tests for the `rmap assess` command.
//!
//! Test matrix:
//!   1. Missing args => usage error, exit 1
//!   2. Missing DB => storage error, exit 2
//!   3. No policies => empty assessment, exit 0
//!   4. Absolute policy evaluation => PASS/FAIL counted, exit 0
//!   5. Comparative policy without --baseline => error, exit 2
//!   6. Comparative policy with --baseline => evaluation succeeds, exit 0
//!   7. JSON output shape validation

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a minimal fixture with a repo and snapshot.
fn build_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let repo_dir = tempfile::tempdir().unwrap();
    let root = repo_dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
    std::fs::write(
        root.join("src/app.ts"),
        r#"
export function main() {
    // some code
    if (true) {
        console.log("hello");
    }
}
"#,
    )
    .unwrap();

    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();
    assert!(result.files_total >= 1);

    (repo_dir, db_dir, db_path)
}

fn run_cmd(args: &[&str]) -> std::process::Output {
    Command::new(binary_path()).args(args).output().unwrap()
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("invalid JSON: {}\nstdout: {}", e, stdout)
    })
}

// -- 1. Missing args => usage error ------------------------------------------

#[test]
fn assess_missing_args() {
    let output = run_cmd(&["assess"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "expected usage message in stderr");
}

#[test]
fn assess_missing_repo_uid() {
    let (_r, _d, db) = build_db();
    let db_str = db.to_str().unwrap();
    let output = run_cmd(&["assess", db_str]);
    assert_eq!(output.status.code(), Some(1));
}

// -- 2. Missing DB => storage error ------------------------------------------

#[test]
fn assess_missing_db() {
    let output = run_cmd(&["assess", "/nonexistent/path.db", "r1"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 3. No policies => empty assessment --------------------------------------

#[test]
fn assess_no_policies() {
    let (_r, _d, db) = build_db();
    let db_str = db.to_str().unwrap();

    let output = run_cmd(&["assess", db_str, "r1"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let json = parse_json(&output);
    assert_eq!(json["command"], "assess");
    assert_eq!(json["repo"], "r1");
    assert_eq!(json["assessments"]["total"], 0);
}

// -- 4. Absolute policy evaluation -------------------------------------------

#[test]
fn assess_absolute_policy_pass() {
    let (_r, _d, db) = build_db();
    let db_str = db.to_str().unwrap();

    // Declare a policy that the code will pass (threshold high enough).
    let declare_output = run_cmd(&[
        "declare", "quality-policy", db_str, "r1", "QP-001",
        "--measurement", "cognitive_complexity",
        "--policy-kind", "absolute_max",
        "--threshold", "100",  // Very high, should pass
    ]);
    assert_eq!(declare_output.status.code(), Some(0), "declare failed: {}",
        String::from_utf8_lossy(&declare_output.stderr));

    // Run assessment.
    let output = run_cmd(&["assess", db_str, "r1"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let json = parse_json(&output);
    assert_eq!(json["command"], "assess");
    assert_eq!(json["assessments"]["total"], 1);
    // Pin exact verdict: high threshold means pass.
    assert_eq!(json["assessments"]["pass"], 1, "expected pass=1");
    assert_eq!(json["assessments"]["fail"], 0, "expected fail=0");

    // Verify persistence: read back from quality_assessments table.
    let conn = repo_graph_storage::StorageConnection::open(&db).unwrap();
    let snap = conn.get_latest_snapshot("r1").unwrap().unwrap();
    let assessments = conn.get_quality_assessments_for_snapshot(&snap.snapshot_uid).unwrap();
    assert_eq!(assessments.len(), 1, "expected 1 persisted assessment");
    assert_eq!(assessments[0].computed_verdict, "PASS");
}

#[test]
fn assess_absolute_policy_fail() {
    let (_r, _d, db) = build_db();
    let db_str = db.to_str().unwrap();

    // Declare a policy with very low threshold that will fail.
    let declare_output = run_cmd(&[
        "declare", "quality-policy", db_str, "r1", "QP-002",
        "--measurement", "cognitive_complexity",
        "--policy-kind", "absolute_max",
        "--threshold", "0",  // Threshold 0 means any complexity fails
    ]);
    assert_eq!(declare_output.status.code(), Some(0));

    // Run assessment.
    let output = run_cmd(&["assess", db_str, "r1"]);
    // Assessment should still succeed (exit 0) even if policies fail.
    // The assessment persisted successfully; verdicts are informational.
    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let json = parse_json(&output);
    assert_eq!(json["assessments"]["total"], 1);
    // Pin exact verdict: threshold 0 means fail.
    assert_eq!(json["assessments"]["fail"], 1, "expected fail=1");
    assert_eq!(json["assessments"]["pass"], 0, "expected pass=0");

    // Verify persistence: read back from quality_assessments table.
    let conn = repo_graph_storage::StorageConnection::open(&db).unwrap();
    let snap = conn.get_latest_snapshot("r1").unwrap().unwrap();
    let assessments = conn.get_quality_assessments_for_snapshot(&snap.snapshot_uid).unwrap();
    assert_eq!(assessments.len(), 1, "expected 1 persisted assessment");
    assert_eq!(assessments[0].computed_verdict, "FAIL");
}

// -- 5. Comparative policy without --baseline => error -----------------------

#[test]
fn assess_comparative_policy_missing_baseline() {
    let (_r, _d, db) = build_db();
    let db_str = db.to_str().unwrap();

    // Declare a comparative policy.
    let declare_output = run_cmd(&[
        "declare", "quality-policy", db_str, "r1", "QP-003",
        "--measurement", "cognitive_complexity",
        "--policy-kind", "no_new",
        "--threshold", "10",
    ]);
    assert_eq!(declare_output.status.code(), Some(0));

    // Run assessment without --baseline.
    let output = run_cmd(&["assess", db_str, "r1"]);
    // Should fail because comparative policy requires baseline.
    assert_eq!(output.status.code(), Some(2), "expected exit 2 for missing baseline");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("baseline"), "stderr should mention baseline: {}", stderr);
}

#[test]
fn assess_comparative_policy_invalid_baseline() {
    let (_r, _d, db) = build_db();
    let db_str = db.to_str().unwrap();

    // Declare a comparative policy.
    let declare_output = run_cmd(&[
        "declare", "quality-policy", db_str, "r1", "QP-INV",
        "--measurement", "cognitive_complexity",
        "--policy-kind", "no_new",
        "--threshold", "10",
    ]);
    assert_eq!(declare_output.status.code(), Some(0));

    // Run assessment with a nonexistent baseline.
    let output = run_cmd(&["assess", db_str, "r1", "--baseline", "nonexistent-snap"]);
    // Should fail because baseline doesn't exist.
    assert_eq!(output.status.code(), Some(2), "expected exit 2 for invalid baseline");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist") || stderr.contains("invalid"),
        "stderr should mention invalid baseline: {}", stderr);
}

// -- 6. Comparative policy with --baseline -----------------------------------

#[test]
fn assess_comparative_policy_with_baseline() {
    let (_r, _d, db) = build_db();
    let db_str = db.to_str().unwrap();

    // Get the current snapshot UID to use as baseline.
    // For this test, we use the same snapshot as both target and baseline.
    // This is a degenerate case but tests the mechanics.
    let conn = repo_graph_storage::StorageConnection::open(&db).unwrap();
    let snap = conn.get_latest_snapshot("r1").unwrap().unwrap();
    let snapshot_uid = snap.snapshot_uid.clone();

    // Declare a comparative policy.
    let declare_output = run_cmd(&[
        "declare", "quality-policy", db_str, "r1", "QP-004",
        "--measurement", "cognitive_complexity",
        "--policy-kind", "no_worsened",
        "--threshold", "10",
    ]);
    assert_eq!(declare_output.status.code(), Some(0));

    // Run assessment with --baseline.
    let output = run_cmd(&["assess", db_str, "r1", "--baseline", &snapshot_uid]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let json = parse_json(&output);
    assert_eq!(json["command"], "assess");
    assert_eq!(json["baseline_snapshot"], snapshot_uid);
    assert_eq!(json["assessments"]["total"], 1);
}

// -- 7. JSON output shape validation -----------------------------------------

#[test]
fn assess_json_shape() {
    let (_r, _d, db) = build_db();
    let db_str = db.to_str().unwrap();

    let output = run_cmd(&["assess", db_str, "r1"]);
    assert_eq!(output.status.code(), Some(0));

    let json = parse_json(&output);

    // Required top-level fields.
    assert!(json.get("command").is_some(), "missing 'command' field");
    assert!(json.get("repo").is_some(), "missing 'repo' field");
    assert!(json.get("snapshot").is_some(), "missing 'snapshot' field");
    assert!(json.get("baseline_snapshot").is_some(), "missing 'baseline_snapshot' field");
    assert!(json.get("assessments").is_some(), "missing 'assessments' field");
    assert!(json.get("baseline_required_count").is_some(), "missing 'baseline_required_count' field");

    // Assessments sub-object.
    let assessments = &json["assessments"];
    assert!(assessments.get("total").is_some(), "missing 'total' in assessments");
    assert!(assessments.get("pass").is_some(), "missing 'pass' in assessments");
    assert!(assessments.get("fail").is_some(), "missing 'fail' in assessments");
    assert!(assessments.get("not_applicable").is_some(), "missing 'not_applicable' in assessments");
    assert!(assessments.get("not_comparable").is_some(), "missing 'not_comparable' in assessments");
}
