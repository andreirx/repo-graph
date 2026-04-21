//! Deterministic tests for the `hotspots` command.
//!
//! RS-MS-3b: Query-time hotspot analysis (churn × complexity).
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found
//!   4. Valid hotspots (with both churn and complexity)
//!   5. Custom --since window
//!   6. Files without complexity excluded
//!   7. Empty results (no overlap) is success
//!   8. Envelope contract

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

fn fixture_path() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("..")
		.join("..")
		.join("..")
		.join("test")
		.join("fixtures")
		.join("typescript")
		.join("classifier-repo")
}

/// Build a temp DB by indexing the classifier-repo fixture.
fn build_indexed_db() -> (tempfile::TempDir, PathBuf) {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(
		&fixture_path(),
		&db_path,
		"test-repo",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(result.files_total, 1);

	(dir, db_path)
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
///
/// RS-MS-3a P2 fix: Creates proper file + node + measurement chain so that
/// `query_complexity_by_file` join works. The stable_key format must be
/// `{repo}:{path}#{symbol}:SYMBOL:{subtype}` to match real indexed data.
fn insert_complexity_measurement(
	db_path: &std::path::Path,
	snapshot_uid: &str,
	repo_uid: &str,
	file_path: &str,
	symbol_name: &str,
	complexity: u64,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();

	// 1. Ensure file row exists (matches nodes.file_uid join target)
	let file_uid = format!("{}:{}", repo_uid, file_path);
	conn.execute(
		"INSERT OR IGNORE INTO files (file_uid, repo_uid, path, language, is_test)
		 VALUES (?, ?, ?, 'typescript', 0)",
		rusqlite::params![file_uid, repo_uid, file_path],
	)
	.expect("insert file");

	// 2. Insert node with proper stable_key format: {repo}:{path}#{symbol}:SYMBOL:{subtype}
	let node_uid = format!("node-{}-{}", file_path, symbol_name);
	let stable_key = format!("{}:{}#{}:SYMBOL:FUNCTION", repo_uid, file_path, symbol_name);
	conn.execute(
		"INSERT OR IGNORE INTO nodes
		 (node_uid, snapshot_uid, repo_uid, stable_key, kind, subtype, name, file_uid)
		 VALUES (?, ?, ?, ?, 'SYMBOL', 'FUNCTION', ?, ?)",
		rusqlite::params![node_uid, snapshot_uid, repo_uid, stable_key, symbol_name, file_uid],
	)
	.expect("insert node");

	// 3. Insert measurement targeting the node's stable_key
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

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn hotspots_usage_error_no_args() {
	let output = Command::new(binary_path())
		.args(["hotspots"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn hotspots_usage_error_missing_repo() {
	let output = Command::new(binary_path())
		.args(["hotspots", "/some/path.db"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn hotspots_missing_db() {
	let output = Command::new(binary_path())
		.args(["hotspots", "/nonexistent/path.db", "repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn hotspots_repo_not_found() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"hotspots",
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

/// Create a minimal repo with its own git history for churn tests.
///
/// The classifier-repo fixture inherits git history from the parent repo-graph,
/// making churn paths misalign. This helper creates an isolated repo.
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

// ── 4. Valid hotspots ────────────────────────────────────────────

#[test]
fn hotspots_success_with_complexity() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert complexity measurement for the indexed file
	insert_complexity_measurement(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"index.ts",
		"greet",
		5,
	);

	let output = Command::new(binary_path())
		.args([
			"hotspots",
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

	assert_eq!(result["command"], "hotspots");
	assert!(result["results"].is_array());
	assert_eq!(result["formula"], "lines_changed * sum_complexity");

	// Non-vacuous: verify the complexity measurement was actually joined
	// through nodes → files and produced a result.
	let results = result["results"].as_array().unwrap();
	assert!(
		!results.is_empty(),
		"expected non-empty hotspot results after inserting complexity; \
		 if empty, the nodes→files join may be broken"
	);
}

// ── 5. Custom --since window ─────────────────────────────────────

#[test]
fn hotspots_custom_since_window() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"hotspots",
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

// ── 6. Files without complexity excluded ─────────────────────────

#[test]
fn hotspots_excludes_files_without_complexity() {
	let (_dir, db_path) = build_indexed_db();
	// Do NOT insert any complexity measurements

	let output = Command::new(binary_path())
		.args([
			"hotspots",
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

	// No complexity = no hotspots (even if there's churn)
	assert_eq!(result["count"], 0);
	assert!(result["results"].as_array().unwrap().is_empty());
}

// ── 7. Empty results is success ──────────────────────────────────

#[test]
fn hotspots_empty_results_is_success() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert complexity for a file that won't have churn (very short window)
	insert_complexity_measurement(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"src/index.ts",
		"main",
		10,
	);

	let output = Command::new(binary_path())
		.args([
			"hotspots",
			db_path.to_str().unwrap(),
			"test-repo",
			"--since",
			"1.second.ago", // No churn in this window
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

// ── 8. Envelope contract ─────────────────────────────────────────

#[test]
fn hotspots_envelope_contract() {
	let (_dir, db_path, _repo_path) = build_repo_with_git_history();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_complexity_measurement(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"index.ts",
		"greet",
		5,
	);

	let output = Command::new(binary_path())
		.args([
			"hotspots",
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
	assert_eq!(result["command"], "hotspots");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"].is_string());
	assert!(result["stale"].is_boolean());
	assert!(result["results"].is_array());
	assert!(result["count"].is_number());

	// Hotspots-specific fields
	assert!(result["since"].is_string());
	assert!(result["formula"].is_string());

	// Non-vacuous: verify results are produced and check row shape
	let arr = result["results"].as_array().unwrap();
	assert!(
		!arr.is_empty(),
		"expected non-empty results for envelope contract test; \
		 if empty, complexity join through nodes→files is broken"
	);
	for row in arr {
		assert!(row["file_path"].is_string(), "row must have file_path");
		assert!(row["commit_count"].is_number(), "row must have commit_count");
		assert!(row["lines_changed"].is_number(), "row must have lines_changed");
		assert!(row["sum_complexity"].is_number(), "row must have sum_complexity");
		assert!(row["hotspot_score"].is_number(), "row must have hotspot_score");
	}
}

// ── 9. Verify score formula ──────────────────────────────────────

#[test]
fn hotspots_score_is_lines_times_complexity() {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("test.db");
	let repo_path = dir.path().join("repo");

	// Create a minimal repo
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

	// Create file with known churn
	std::fs::write(
		repo_path.join("index.ts"),
		"export const x = 1;\nexport const y = 2;\nexport const z = 3;\n", // 3 lines
	)
	.unwrap();

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

	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert complexity: 7 total (sum of multiple symbols)
	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "x", 3);
	insert_complexity_measurement(&db_path, &snapshot_uid, "test-repo", "index.ts", "y", 4);

	let output = Command::new(binary_path())
		.args([
			"hotspots",
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

	// Non-vacuous: the test MUST produce results to verify the formula.
	// If empty, the nodes→files join is broken.
	assert!(
		!results.is_empty(),
		"expected non-empty hotspot results; if empty, complexity join through nodes→files is broken"
	);

	let row = &results[0];
	let lines_changed = row["lines_changed"].as_u64().unwrap();
	let sum_complexity = row["sum_complexity"].as_u64().unwrap();
	let hotspot_score = row["hotspot_score"].as_u64().unwrap();

	// Verify formula: score = lines_changed * sum_complexity
	assert_eq!(
		hotspot_score,
		lines_changed * sum_complexity,
		"hotspot_score must equal lines_changed * sum_complexity"
	);

	// sum_complexity should be 3 + 4 = 7
	assert_eq!(sum_complexity, 7, "sum_complexity should aggregate multiple symbols");
}
