//! Deterministic tests for the `risk` command.
//!
//! RS-MS-4: Query-time risk analysis (hotspot x coverage gap).
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found
//!   4. Valid risk (with hotspot + coverage)
//!   5. Custom --since window
//!   6. Files without coverage excluded (NOT degraded)
//!   7. Files without hotspot excluded
//!   8. Empty results (no overlap) is success
//!   9. Envelope contract
//!  10. Formula verification: risk = hotspot * (1 - coverage)
//!  11. High coverage reduces risk

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	let mut path = PathBuf::from(env!("CARGO_BIN_EXE_rmap"));
	if !path.exists() {
		path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("..")
			.join("..")
			.join("target")
			.join("debug")
			.join("rmap");
	}
	path
}

/// Build a minimal repo with git history, index it, return paths.
fn build_repo_with_git_history() -> (tempfile::TempDir, PathBuf, PathBuf) {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("test.db");
	let repo_path = dir.path().join("repo");

	std::fs::create_dir_all(&repo_path).unwrap();

	// Initialize git
	std::process::Command::new("git")
		.args(["init"])
		.current_dir(&repo_path)
		.output()
		.expect("git init");

	std::process::Command::new("git")
		.args(["config", "user.email", "test@example.com"])
		.current_dir(&repo_path)
		.output()
		.expect("git config email");

	std::process::Command::new("git")
		.args(["config", "user.name", "Test"])
		.current_dir(&repo_path)
		.output()
		.expect("git config name");

	// Create TypeScript file
	std::fs::write(
		repo_path.join("index.ts"),
		"export function greet(name: string) { return `Hello, ${name}`; }\n",
	)
	.unwrap();

	// Commit
	std::process::Command::new("git")
		.args(["add", "-A"])
		.current_dir(&repo_path)
		.output()
		.expect("git add");

	std::process::Command::new("git")
		.args(["commit", "-m", "initial"])
		.current_dir(&repo_path)
		.output()
		.expect("git commit");

	// Index the repo
	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	index_path(&repo_path, &db_path, "test-repo", &ComposeOptions::default()).unwrap();

	(dir, db_path, repo_path)
}

/// Get the snapshot UID for a repo.
fn get_snapshot_uid(db_path: &std::path::Path, repo_uid: &str) -> String {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.query_row(
		"SELECT snapshot_uid FROM snapshots WHERE repo_uid = ? ORDER BY created_at DESC LIMIT 1",
		[repo_uid],
		|row| row.get(0),
	)
	.expect("get snapshot uid")
}

/// Insert a complexity measurement for testing.
fn insert_complexity_measurement(
	db_path: &std::path::Path,
	snapshot_uid: &str,
	repo_uid: &str,
	file_path: &str,
	symbol_name: &str,
	complexity: u64,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();

	// Ensure file row exists
	let file_uid = format!("{}:{}", repo_uid, file_path);
	conn.execute(
		"INSERT OR IGNORE INTO files (file_uid, repo_uid, path, language, is_test)
		 VALUES (?, ?, ?, 'typescript', 0)",
		rusqlite::params![file_uid, repo_uid, file_path],
	)
	.expect("insert file");

	// Insert node with proper stable_key format
	let node_uid = format!("node-{}-{}", file_path, symbol_name);
	let stable_key = format!("{}:{}#{}:SYMBOL:FUNCTION", repo_uid, file_path, symbol_name);
	conn.execute(
		"INSERT OR IGNORE INTO nodes
		 (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, file_uid)
		 VALUES (?, ?, ?, ?, 'SYMBOL', 'FUNCTION', ?, ?)",
		rusqlite::params![node_uid, snapshot_uid, repo_uid, stable_key, symbol_name, file_uid],
	)
	.expect("insert node");

	// Insert measurement targeting the node's stable_key
	let measurement_uid = format!("m-{}-{}", file_path, symbol_name);
	let value_json = format!(r#"{{"value":{}}}"#, complexity);
	let now = "2026-01-01T00:00:00Z";

	conn.execute(
		"INSERT INTO measurements
		 (measurement_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, source, created_at)
		 VALUES (?, ?, ?, ?, 'cyclomatic_complexity', ?, 'test', ?)",
		rusqlite::params![measurement_uid, snapshot_uid, repo_uid, stable_key, value_json, now],
	)
	.expect("insert measurement");
}

/// Insert a coverage measurement for testing.
fn insert_coverage_measurement(
	db_path: &std::path::Path,
	snapshot_uid: &str,
	repo_uid: &str,
	file_path: &str,
	line_coverage: f64,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();

	// target_stable_key format: {repo_uid}:{file_path}:FILE
	let target_stable_key = format!("{}:{}:FILE", repo_uid, file_path);
	let measurement_uid = format!("cov-{}", file_path.replace('/', "_"));
	let value_json = format!(r#"{{"value":{},"covered":8,"total":10}}"#, line_coverage);
	let now = "2026-01-01T00:00:00Z";

	conn.execute(
		"INSERT INTO measurements
		 (measurement_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, source, created_at)
		 VALUES (?, ?, ?, ?, 'line_coverage', ?, 'coverage-istanbul:0.1.0', ?)",
		rusqlite::params![measurement_uid, snapshot_uid, repo_uid, target_stable_key, value_json, now],
	)
	.expect("insert coverage measurement");
}

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn risk_usage_error_no_args() {
	let output = Command::new(binary_path())
		.args(["risk"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn risk_usage_error_missing_repo() {
	let output = Command::new(binary_path())
		.args(["risk", "/some/path.db"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn risk_missing_db() {
	let output = Command::new(binary_path())
		.args(["risk", "/nonexistent/path.db", "repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn risk_repo_not_found() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("no snapshot found") || stderr.contains("repo not found"),
		"stderr: {}",
		stderr
	);
}

// ── 4. Valid risk (with hotspot + coverage) ──────────────────────

#[test]
fn risk_success_with_hotspot_and_coverage() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert complexity (for hotspot)
	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "greet", 5);

	// Insert coverage
	insert_coverage_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", 0.8);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

	assert_eq!(result["command"], "risk");
	assert!(result["results"].is_array());
	assert_eq!(result["formula"], "hotspot_score * (1 - line_coverage)");

	let results = result["results"].as_array().unwrap();
	assert!(
		!results.is_empty(),
		"expected non-empty risk results after inserting complexity + coverage"
	);
}

// ── 5. Custom --since window ─────────────────────────────────────

#[test]
fn risk_custom_since_window() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "greet", 5);
	insert_coverage_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", 0.5);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"30.days.ago",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	assert_eq!(result["since"], "30.days.ago");
}

// ── 6. Files without coverage excluded (NOT degraded) ────────────

#[test]
fn risk_excludes_files_without_coverage() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert complexity (creates hotspot)
	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "greet", 5);

	// Do NOT insert coverage

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// CRITICAL: no coverage = no risk (NOT degraded to risk = hotspot)
	assert_eq!(result["count"], 0);
	assert_eq!(result["joined_files"], 0);

	// But we should still see hotspot and coverage file counts
	assert!(result["hotspot_files"].as_u64().unwrap() > 0);
	assert_eq!(result["coverage_files"], 0);
}

// ── 7. Files without hotspot excluded ────────────────────────────

#[test]
fn risk_excludes_files_without_hotspot() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert coverage for a different file than the one with churn
	insert_coverage_measurement(&db_path, &snapshot_uid, "test-repo", "other.ts", 0.1);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// No hotspot for the covered file = no risk
	assert_eq!(result["count"], 0);
}

// ── 8. Empty results is success ──────────────────────────────────

#[test]
fn risk_empty_results_is_success() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();

	// No complexity, no coverage

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.second.ago",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"empty results must succeed, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	assert_eq!(result["count"], 0);
}

// ── 9. Envelope contract ─────────────────────────────────────────

#[test]
fn risk_envelope_contract() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "greet", 5);
	insert_coverage_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", 0.5);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// Standard envelope fields
	assert_eq!(result["command"], "risk");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"].is_string());
	assert!(result["stale"].is_boolean());
	assert!(result["results"].is_array());
	assert!(result["count"].is_number());

	// Risk-specific fields
	assert!(result["since"].is_string());
	assert!(result["formula"].is_string());
	assert!(result["hotspot_files"].is_number());
	assert!(result["coverage_files"].is_number());
	assert!(result["joined_files"].is_number());

	// Non-vacuous: verify results are produced and check row shape
	let arr = result["results"].as_array().unwrap();
	assert!(
		!arr.is_empty(),
		"expected non-empty results for envelope contract test"
	);
	for row in arr {
		assert!(row["file_path"].is_string(), "row must have file_path");
		assert!(row["risk_score"].is_number(), "row must have risk_score");
		assert!(row["hotspot_score"].is_number(), "row must have hotspot_score");
		assert!(row["line_coverage"].is_number(), "row must have line_coverage");
		assert!(row["lines_changed"].is_number(), "row must have lines_changed");
		assert!(row["sum_complexity"].is_number(), "row must have sum_complexity");
	}
}

// ── 10. Formula verification ─────────────────────────────────────

#[test]
fn risk_formula_is_hotspot_times_coverage_gap() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Complexity = 10
	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "greet", 10);

	// Coverage = 0.6 (60%), so coverage_gap = 0.4
	insert_coverage_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", 0.6);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	let results = result["results"].as_array().unwrap();
	assert!(!results.is_empty());

	let row = &results[0];
	let hotspot_score = row["hotspot_score"].as_u64().unwrap() as f64;
	let line_coverage = row["line_coverage"].as_f64().unwrap();
	let risk_score = row["risk_score"].as_f64().unwrap();

	// Formula: risk = hotspot * (1 - coverage)
	let expected_risk = hotspot_score * (1.0 - line_coverage);

	assert!(
		(risk_score - expected_risk).abs() < 0.01,
		"risk_score {} should equal hotspot {} * (1 - coverage {}) = {}",
		risk_score,
		hotspot_score,
		line_coverage,
		expected_risk
	);
}

// ── 11. High coverage reduces risk ───────────────────────────────

#[test]
fn risk_high_coverage_reduces_risk() {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("test.db");
	let repo_path = dir.path().join("repo");

	std::fs::create_dir_all(&repo_path).unwrap();

	std::process::Command::new("git")
		.args(["init"])
		.current_dir(&repo_path)
		.output()
		.expect("git init");

	std::process::Command::new("git")
		.args(["config", "user.email", "test@example.com"])
		.current_dir(&repo_path)
		.output()
		.expect("git config email");

	std::process::Command::new("git")
		.args(["config", "user.name", "Test"])
		.current_dir(&repo_path)
		.output()
		.expect("git config name");

	// Create two files
	std::fs::write(repo_path.join("well_tested.ts"), "export const x = 1;\n").unwrap();
	std::fs::write(repo_path.join("poorly_tested.ts"), "export const y = 2;\n").unwrap();

	std::process::Command::new("git")
		.args(["add", "-A"])
		.current_dir(&repo_path)
		.output()
		.expect("git add");

	std::process::Command::new("git")
		.args(["commit", "-m", "initial"])
		.current_dir(&repo_path)
		.output()
		.expect("git commit");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	index_path(&repo_path, &db_path, "test-repo", &ComposeOptions::default()).unwrap();

	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Same complexity (hotspot will depend on churn)
	insert_complexity_measurement(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"well_tested.ts",
		"x",
		10,
	);
	insert_complexity_measurement(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"poorly_tested.ts",
		"y",
		10,
	);

	// Different coverage
	insert_coverage_measurement(&db_path, &snapshot_uid, "test-repo", "well_tested.ts", 0.95);
	insert_coverage_measurement(&db_path, &snapshot_uid, "test-repo", "poorly_tested.ts", 0.1);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	let results = result["results"].as_array().unwrap();

	// Find both files
	let well_tested = results
		.iter()
		.find(|r| r["file_path"].as_str().unwrap().contains("well_tested"));
	let poorly_tested = results
		.iter()
		.find(|r| r["file_path"].as_str().unwrap().contains("poorly_tested"));

	if let (Some(wt), Some(pt)) = (well_tested, poorly_tested) {
		let wt_risk = wt["risk_score"].as_f64().unwrap();
		let pt_risk = pt["risk_score"].as_f64().unwrap();

		// Poorly tested should have higher risk despite similar hotspot
		assert!(
			pt_risk > wt_risk,
			"poorly_tested risk {} should be > well_tested risk {}",
			pt_risk,
			wt_risk
		);
	}
}

// ── 12. Malformed coverage JSON aborts ───────────────────────────

/// Insert a raw coverage measurement row (for malformed data tests).
fn insert_raw_coverage_measurement(
	db_path: &std::path::Path,
	snapshot_uid: &str,
	repo_uid: &str,
	target_stable_key: &str,
	value_json: &str,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	let measurement_uid = format!("cov-raw-{}", target_stable_key.replace(':', "_"));
	let now = "2026-01-01T00:00:00Z";

	conn.execute(
		"INSERT INTO measurements
		 (measurement_uid, snapshot_uid, repo_uid, target_stable_key, kind, value_json, source, created_at)
		 VALUES (?, ?, ?, ?, 'line_coverage', ?, 'test', ?)",
		rusqlite::params![measurement_uid, snapshot_uid, repo_uid, target_stable_key, value_json, now],
	)
	.expect("insert raw coverage measurement");
}

#[test]
fn risk_malformed_coverage_json_aborts() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert complexity (for hotspot)
	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "greet", 5);

	// Insert malformed coverage JSON
	insert_raw_coverage_measurement(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"test-repo:index.ts:FILE",
		"not valid json",
	);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2), "malformed JSON must abort");
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("malformed coverage measurement JSON"),
		"stderr: {}",
		stderr
	);
}

// ── 13. Coverage missing value field aborts ──────────────────────

#[test]
fn risk_coverage_missing_value_field_aborts() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert complexity (for hotspot)
	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "greet", 5);

	// Insert coverage JSON missing "value" field
	insert_raw_coverage_measurement(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"test-repo:index.ts:FILE",
		r#"{"covered":8,"total":10}"#, // missing "value"
	);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2), "missing value field must abort");
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("missing 'value' field"),
		"stderr: {}",
		stderr
	);
}

// ── 14. Malformed target_stable_key aborts ───────────────────────

#[test]
fn risk_malformed_target_stable_key_aborts() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert complexity (for hotspot)
	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "greet", 5);

	// Insert coverage with malformed target_stable_key (wrong format)
	insert_raw_coverage_measurement(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"wrong-repo:index.ts:FILE", // Wrong repo prefix
		r#"{"value":0.8,"covered":8,"total":10}"#,
	);

	let output = Command::new(binary_path())
		.args([
			"risk",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.year.ago",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2), "malformed target_stable_key must abort");
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("malformed coverage measurement target_stable_key"),
		"stderr: {}",
		stderr
	);
}
