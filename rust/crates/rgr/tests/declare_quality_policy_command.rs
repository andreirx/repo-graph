//! Deterministic tests for the `declare quality-policy` command.
//!
//! Test matrix:
//!   1. Missing args => usage error, exit 1
//!   2. Missing --measurement => usage error, exit 1
//!   3. Missing --policy-kind => usage error, exit 1
//!   4. Missing --threshold => usage error, exit 1
//!   5. Invalid --measurement (unsupported kind) => validation error, exit 1
//!   6. Invalid --policy-kind => validation error, exit 1
//!   7. Non-finite --threshold => validation error, exit 1
//!   8. Invalid --scope-clause format => usage error, exit 1
//!   9. Missing DB => storage error, exit 2
//!  10. Insert success => JSON output, exit 0
//!  11. Idempotent repeated insert => inserted=false, exit 0
//!  12. Exact JSON output shape
//!  13. --version defaults to 1
//!  14. Incompatible policy_kind/measurement_kind => validation error, exit 1
//!      (coverage + no_new, coverage + no_worsened rejected; absolute_min allowed)

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a minimal fixture for quality-policy testing.
fn build_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(root.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
	std::fs::write(root.join("src/app.ts"), "export function main() {}\n").unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();
	assert_eq!(result.files_total, 1);

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

// -- 1. Missing args => usage error --------------------------------------

#[test]
fn declare_quality_policy_missing_args() {
	let output = run_cmd(&["declare", "quality-policy"]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "expected usage message in stderr");
}

#[test]
fn declare_quality_policy_missing_policy_id() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1",
		// missing policy_id
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
	]);
	assert_eq!(output.status.code(), Some(1));
}

// -- 2. Missing --measurement => usage error -----------------------------

#[test]
fn declare_quality_policy_missing_measurement() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--measurement is required"), "stderr: {}", stderr);
}

// -- 3. Missing --policy-kind => usage error -----------------------------

#[test]
fn declare_quality_policy_missing_policy_kind() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--threshold", "15",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--policy-kind is required"), "stderr: {}", stderr);
}

// -- 4. Missing --threshold => usage error -------------------------------

#[test]
fn declare_quality_policy_missing_threshold() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--threshold is required"), "stderr: {}", stderr);
}

// -- 5. Invalid --measurement (unsupported kind) -------------------------

#[test]
fn declare_quality_policy_invalid_measurement() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "bogus_metric",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("unknown measurement kind"), "stderr: {}", stderr);
}

// -- 6. Invalid --policy-kind ---------------------------------------------

#[test]
fn declare_quality_policy_invalid_policy_kind() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "invalid_kind",
		"--threshold", "15",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("invalid --policy-kind"), "stderr: {}", stderr);
}

// -- 7. Non-finite --threshold -------------------------------------------

#[test]
fn declare_quality_policy_nan_threshold() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "NaN",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("threshold must be finite"), "stderr: {}", stderr);
}

#[test]
fn declare_quality_policy_infinity_threshold() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "inf",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("threshold must be finite"), "stderr: {}", stderr);
}

// -- 8. Invalid --scope-clause format ------------------------------------

#[test]
fn declare_quality_policy_invalid_scope_clause_format() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
		"--scope-clause", "invalid_format", // missing colon
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("invalid --scope-clause format"), "stderr: {}", stderr);
}

#[test]
fn declare_quality_policy_invalid_scope_clause_type() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
		"--scope-clause", "invalid_type:src/core",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("invalid scope clause type"), "stderr: {}", stderr);
}

// -- 9. Missing DB => storage error, exit 2 ------------------------------

#[test]
fn declare_quality_policy_missing_db() {
	let output = run_cmd(&[
		"declare", "quality-policy", "/nonexistent/path.db", "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 10. Insert success => JSON output, exit 0 ---------------------------

#[test]
fn declare_quality_policy_success() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
	]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));

	let json = parse_json(&output);
	assert_eq!(json["kind"], "quality_policy");
	assert_eq!(json["policy_id"], "QP-001");
	assert_eq!(json["version"], 1);
	assert_eq!(json["measurement"], "cognitive_complexity");
	assert_eq!(json["policy_kind"], "absolute_max");
	assert_eq!(json["threshold"], 15.0);
	assert_eq!(json["inserted"], true);
	assert!(json["declaration_uid"].as_str().is_some());
}

// -- 11. Idempotent repeated insert --------------------------------------

#[test]
fn declare_quality_policy_idempotent() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	// First insert.
	let output1 = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
	]);
	assert_eq!(output1.status.code(), Some(0));
	let json1 = parse_json(&output1);
	assert_eq!(json1["inserted"], true);
	let uid1 = json1["declaration_uid"].as_str().unwrap();

	// Second insert with same identity.
	let output2 = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-001",
		"--measurement", "cognitive_complexity",
		"--policy-kind", "absolute_max",
		"--threshold", "15",
	]);
	assert_eq!(output2.status.code(), Some(0));
	let json2 = parse_json(&output2);
	assert_eq!(json2["inserted"], false);
	let uid2 = json2["declaration_uid"].as_str().unwrap();

	assert_eq!(uid1, uid2, "same identity => same UID");
}

// -- 12. Exact JSON output shape -----------------------------------------

#[test]
fn declare_quality_policy_json_shape() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-002",
		"--measurement", "function_length",
		"--policy-kind", "absolute_max",
		"--threshold", "50",
		"--version", "2",
		"--severity", "advisory",
		"--scope-clause", "module:src/core",
		"--description", "Max function length",
	]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));

	let json = parse_json(&output);
	let obj = json.as_object().unwrap();

	// Verify all expected keys are present.
	assert!(obj.contains_key("declaration_uid"));
	assert!(obj.contains_key("kind"));
	assert!(obj.contains_key("policy_id"));
	assert!(obj.contains_key("version"));
	assert!(obj.contains_key("measurement"));
	assert!(obj.contains_key("policy_kind"));
	assert!(obj.contains_key("threshold"));
	assert!(obj.contains_key("inserted"));

	// Verify values.
	assert_eq!(json["kind"], "quality_policy");
	assert_eq!(json["policy_id"], "QP-002");
	assert_eq!(json["version"], 2);
	assert_eq!(json["measurement"], "function_length");
	assert_eq!(json["policy_kind"], "absolute_max");
	assert_eq!(json["threshold"], 50.0);
}

// -- 13. --version defaults to 1 -----------------------------------------

#[test]
fn declare_quality_policy_version_default() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-003",
		"--measurement", "max_nesting_depth",
		"--policy-kind", "absolute_max",
		"--threshold", "5",
		// no --version
	]);
	assert_eq!(output.status.code(), Some(0));

	let json = parse_json(&output);
	assert_eq!(json["version"], 1, "version should default to 1");
}

// -- Valid scope clause types --------------------------------------------

#[test]
fn declare_quality_policy_scope_clause_types() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	// All three valid types.
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-004",
		"--measurement", "cyclomatic_complexity",
		"--policy-kind", "no_new",
		"--threshold", "10",
		"--scope-clause", "module:src/core",
		"--scope-clause", "file:*.test.ts",
		"--scope-clause", "symbol_kind:FUNCTION",
	]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
}

// -- 14. Incompatible policy_kind / measurement_kind combinations ----------

#[test]
fn declare_quality_policy_coverage_with_no_new_rejected() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-005",
		"--measurement", "line_coverage",
		"--policy-kind", "no_new",
		"--threshold", "0.8",
	]);
	assert_eq!(output.status.code(), Some(1), "should reject no_new for coverage");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("incompatible") || stderr.contains("IncompatiblePolicyKind"),
		"stderr should mention incompatibility: {}", stderr);
}

#[test]
fn declare_quality_policy_coverage_with_no_worsened_rejected() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-006",
		"--measurement", "line_coverage",
		"--policy-kind", "no_worsened",
		"--threshold", "0.8",
	]);
	assert_eq!(output.status.code(), Some(1), "should reject no_worsened for coverage");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("incompatible") || stderr.contains("IncompatiblePolicyKind"),
		"stderr should mention incompatibility: {}", stderr);
}

#[test]
fn declare_quality_policy_coverage_with_absolute_min_allowed() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"declare", "quality-policy", db_str, "r1", "QP-007",
		"--measurement", "line_coverage",
		"--policy-kind", "absolute_min",
		"--threshold", "0.8",
	]);
	assert_eq!(output.status.code(), Some(0), "absolute_min should be allowed for coverage: stderr={}",
		String::from_utf8_lossy(&output.stderr));
}
