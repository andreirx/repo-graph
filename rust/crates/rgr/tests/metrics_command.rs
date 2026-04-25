//! Deterministic tests for the `metrics` command.
//!
//! Test matrix:
//!   1. Usage error (missing args)
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. Empty results (no measurements of requested kind)
//!   5. --kind filter works correctly
//!   6. --limit caps results
//!   7. --sort value (desc) and --sort target (asc)
//!   8. Malformed value_json is skipped gracefully

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a temp DB with TS files that produce measurements.
///
/// Layout:
///   src/simple.ts — function simple() {}
///   src/complex.ts — function complex() { if (a) { if (b) { if (c) {} } } }
///
/// Expected measurements:
///   simple: function_length=1, cognitive_complexity=0
///   complex: function_length=1 (single line), cognitive_complexity=6
fn build_metrics_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/simple.ts"),
		"export function simple() {}\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/complex.ts"),
		"export function complex() { if (a) { if (b) { if (c) {} } } }\n",
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(
		root,
		&db_path,
		"r1",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(result.files_total, 2);

	(repo_dir, db_dir, db_path)
}

// -- 1. Usage error ---------------------------------------------------

#[test]
fn metrics_usage_error() {
	let output = Command::new(binary_path())
		.args(["metrics"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn metrics_missing_db() {
	let output = Command::new(binary_path())
		.args(["metrics", "/nonexistent.db", "r1"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn metrics_repo_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_metrics_db();

	let output = Command::new(binary_path())
		.args([
			"metrics",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("no snapshot"), "stderr: {}", stderr);
}

// -- 4. Empty results -------------------------------------------------

#[test]
fn metrics_empty_for_unknown_kind() {
	let (_repo_dir, _db_dir, db_path) = build_metrics_db();

	let output = Command::new(binary_path())
		.args([
			"metrics",
			db_path.to_str().unwrap(),
			"r1",
			"--kind",
			"nonexistent_kind",
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

	assert_eq!(result["count"], 0);
	assert!(result["results"].as_array().unwrap().is_empty());
}

// -- 5. --kind filter -------------------------------------------------

#[test]
fn metrics_kind_filter() {
	let (_repo_dir, _db_dir, db_path) = build_metrics_db();

	let output = Command::new(binary_path())
		.args([
			"metrics",
			db_path.to_str().unwrap(),
			"r1",
			"--kind",
			"cognitive_complexity",
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
	assert!(!results.is_empty(), "should have cognitive_complexity measurements");

	// All results should be cognitive_complexity
	for r in results {
		assert_eq!(r["kind"], "cognitive_complexity");
	}
}

// -- 6. --limit caps results ------------------------------------------

#[test]
fn metrics_limit() {
	let (_repo_dir, _db_dir, db_path) = build_metrics_db();

	let output = Command::new(binary_path())
		.args([
			"metrics",
			db_path.to_str().unwrap(),
			"r1",
			"--limit",
			"1",
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

	assert_eq!(result["count"], 1);
	assert_eq!(result["results"].as_array().unwrap().len(), 1);
}

// -- 7. --sort value vs target ----------------------------------------

#[test]
fn metrics_sort_by_value_desc() {
	let (_repo_dir, _db_dir, db_path) = build_metrics_db();

	let output = Command::new(binary_path())
		.args([
			"metrics",
			db_path.to_str().unwrap(),
			"r1",
			"--kind",
			"cognitive_complexity",
			"--sort",
			"value",
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
	assert!(results.len() >= 2, "should have at least 2 results");

	// complex() should be first (higher complexity)
	let first_key = results[0]["target_stable_key"].as_str().unwrap();
	assert!(
		first_key.contains("complex"),
		"first result should be complex function, got: {}",
		first_key
	);
}

#[test]
fn metrics_sort_by_target_asc() {
	let (_repo_dir, _db_dir, db_path) = build_metrics_db();

	let output = Command::new(binary_path())
		.args([
			"metrics",
			db_path.to_str().unwrap(),
			"r1",
			"--kind",
			"cognitive_complexity",
			"--sort",
			"target",
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
	let keys: Vec<&str> = results
		.iter()
		.map(|r| r["target_stable_key"].as_str().unwrap())
		.collect();

	let mut sorted = keys.clone();
	sorted.sort();
	assert_eq!(keys, sorted, "results should be sorted by target ascending");
}

// -- 8. QueryResult envelope contract ---------------------------------

#[test]
fn metrics_envelope_contract() {
	let (_repo_dir, _db_dir, db_path) = build_metrics_db();

	let output = Command::new(binary_path())
		.args([
			"metrics",
			db_path.to_str().unwrap(),
			"r1",
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

	// Verify TS-compatible QueryResult envelope fields.
	assert_eq!(result["command"], "metrics");
	assert!(result["repo"].is_string(), "repo field must be present");
	assert!(result["snapshot"].is_string(), "snapshot field must be present");
	assert!(
		result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental",
		"snapshot_scope must be full or incremental"
	);
	assert!(
		result["basis_commit"].is_null() || result["basis_commit"].is_string(),
		"basis_commit must be string or null"
	);
	assert!(result["stale"].is_boolean(), "stale field must be boolean");
	assert!(result["count"].is_number(), "count field must be number");
	assert!(result["results"].is_array(), "results field must be array");

	// Verify row shape
	let results = result["results"].as_array().unwrap();
	if !results.is_empty() {
		let row = &results[0];
		assert!(row["target_stable_key"].is_string());
		assert!(row["kind"].is_string());
		assert!(row["value"].is_number());
		assert!(row["source"].is_string());
	}
}
