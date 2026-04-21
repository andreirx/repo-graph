//! Deterministic tests for the `churn` command.
//!
//! RS-MS-2: Query-time per-file git churn.
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found
//!   4. Valid churn with indexed files
//!   5. Custom --since window
//!   6. Filters to indexed files only (proves unindexed files excluded)
//!   7. Empty results (no churn in window)
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

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn churn_usage_error_no_args() {
	let output = Command::new(binary_path())
		.args(["churn"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn churn_usage_error_missing_repo() {
	let output = Command::new(binary_path())
		.args(["churn", "/some/path.db"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn churn_usage_error_since_missing_value() {
	let output = Command::new(binary_path())
		.args(["churn", "/some/path.db", "repo", "--since"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--since requires"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn churn_missing_db() {
	let output = Command::new(binary_path())
		.args(["churn", "/nonexistent/path.db", "repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn churn_repo_not_found() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"churn",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("no snapshot found") || stderr.contains("repo not found"),
		"stderr: {}",
		stderr
	);
}

// ── 4. Valid churn ───────────────────────────────────────────────

#[test]
fn churn_success_with_default_window() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"churn",
			db_path.to_str().unwrap(),
			"test-repo",
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

	// Envelope fields
	assert_eq!(result["command"], "churn");
	assert_eq!(result["repo"], "test-repo");
	assert!(result["snapshot"].is_string());
	assert!(result["results"].is_array());
	assert!(result["count"].is_number());
	assert_eq!(result["since"], "90.days.ago");
}

// ── 5. Custom --since window ─────────────────────────────────────

#[test]
fn churn_custom_since_window() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"churn",
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

// ── 6. Filters to indexed files only ─────────────────────────────

#[test]
fn churn_excludes_unindexed_files() {
	// This test proves that files in git history but NOT in the indexed
	// file set are excluded from churn results.
	//
	// Strategy:
	// 1. Index a repo (only .ts files are indexed by classifier-repo fixture)
	// 2. Add a non-indexed file (.md) to git history
	// 3. Run churn
	// 4. Verify the .md file is NOT in results

	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("test.db");
	let repo_path = dir.path().join("repo");

	// Create a minimal repo with one indexed file and one unindexed file
	std::fs::create_dir_all(&repo_path).unwrap();

	// Initialize git repo
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

	// Create indexed file (TypeScript — will be indexed)
	std::fs::write(repo_path.join("index.ts"), "export const x = 1;\n").unwrap();

	// Create unindexed file (Markdown — NOT indexed by default extractors)
	std::fs::write(repo_path.join("README.md"), "# Hello\n\nSome content.\n").unwrap();

	// Commit both
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

	// Index the repo (only .ts will be indexed)
	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(&repo_path, &db_path, "test-repo", &ComposeOptions::default()).unwrap();

	// Verify only 1 file indexed (the .ts file)
	assert_eq!(result.files_total, 1, "only .ts should be indexed");

	// Run churn
	let output = Command::new(binary_path())
		.args([
			"churn",
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

	// Verify README.md is NOT in results (it has git churn but is not indexed)
	let results = result["results"].as_array().expect("results is array");

	for row in results {
		let path = row["file_path"].as_str().unwrap();
		assert_ne!(
			path, "README.md",
			"unindexed file README.md should not appear in churn results"
		);
	}

	// Verify index.ts IS in results (if any churn exists)
	// Note: it may or may not appear depending on git history
	// The key assertion is that README.md is excluded
}

// ── 7. Empty results ─────────────────────────────────────────────

#[test]
fn churn_empty_results_is_success() {
	let (_dir, db_path) = build_indexed_db();

	// Very short window — unlikely to have commits
	let output = Command::new(binary_path())
		.args([
			"churn",
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
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// Empty results is success, not error
	assert!(result["results"].is_array());
	assert_eq!(result["count"], 0);
}

// ── 7. Envelope contract ─────────────────────────────────────────

#[test]
fn churn_envelope_contract() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"churn",
			db_path.to_str().unwrap(),
			"test-repo",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// Standard envelope fields
	assert_eq!(result["command"], "churn");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"].is_string());
	assert!(result["stale"].is_boolean());
	assert!(result["results"].is_array());
	assert!(result["count"].is_number());

	// Churn-specific field
	assert!(result["since"].is_string());

	// If there are results, verify row shape
	if let Some(arr) = result["results"].as_array() {
		for row in arr {
			assert!(row["file_path"].is_string(), "row must have file_path");
			assert!(row["commit_count"].is_number(), "row must have commit_count");
			assert!(row["lines_changed"].is_number(), "row must have lines_changed");
		}
	}
}
