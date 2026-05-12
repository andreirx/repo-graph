//! Deterministic tests for the `inferences list` command.
//!
//! Test matrix:
//!   1. inferences list - usage error (wrong args)
//!   2. inferences list - DB open failure (missing file)
//!   3. inferences list - repo not found
//!   4. inferences list - empty result (valid for repos without inferences)
//!   5. inferences list - with kind filter
//!   6. inferences list - output structure validation

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
		.join("react-frontend-corpus")
}

/// Build a temp DB by indexing the react-frontend-corpus fixture.
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
	assert!(result.files_total >= 1);

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

/// Insert an inference for testing.
#[allow(clippy::too_many_arguments)]
fn insert_inference(
	db_path: &std::path::Path,
	inference_uid: &str,
	snapshot_uid: &str,
	repo_uid: &str,
	target_stable_key: &str,
	kind: &str,
	value_json: &str,
	confidence: f64,
	extractor: &str,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.execute(
		"INSERT INTO inferences
		 (inference_uid, snapshot_uid, repo_uid, target_stable_key,
		  kind, value_json, confidence, basis_json, extractor, created_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?, '{}', ?, datetime('now'))",
		rusqlite::params![
			inference_uid,
			snapshot_uid,
			repo_uid,
			target_stable_key,
			kind,
			value_json,
			confidence,
			extractor,
		],
	)
	.expect("insert inference");
}

// ════════════════════════════════════════════════════════════════════
// inferences list tests
// ════════════════════════════════════════════════════════════════════

#[test]
fn inferences_list_usage_error() {
	let output = Command::new(binary_path())
		.args(["inferences", "list"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn inferences_list_missing_db() {
	let output = Command::new(binary_path())
		.args(["inferences", "list", "/nonexistent/path.db", "repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn inferences_list_repo_not_found() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"inferences",
			"list",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("repo not found") || stderr.contains("no snapshot"),
		"stderr: {}",
		stderr
	);
}

#[test]
fn inferences_list_empty_result() {
	let (_dir, db_path) = build_indexed_db();

	// Delete any inferences that may have been created during indexing
	let conn = rusqlite::Connection::open(&db_path).unwrap();
	conn.execute("DELETE FROM inferences WHERE repo_uid = 'test-repo'", [])
		.unwrap();

	let output = Command::new(binary_path())
		.args(["inferences", "list", db_path.to_str().unwrap(), "test-repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0), "exit 0 for empty result");
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["command"], "inferences list");
	assert_eq!(json["repo"], "test-repo");
	assert_eq!(json["results"].as_array().unwrap().len(), 0);
	assert_eq!(json["count"], 0);
}

#[test]
fn inferences_list_with_kind_filter() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Clear any existing inferences
	let conn = rusqlite::Connection::open(&db_path).unwrap();
	conn.execute("DELETE FROM inferences WHERE repo_uid = 'test-repo'", [])
		.unwrap();
	drop(conn);

	// Insert inferences of different kinds
	insert_inference(
		&db_path,
		"inf-001",
		&snapshot_uid,
		"test-repo",
		"test-repo:src/Button.tsx#Button:SYMBOL:FUNCTION",
		"react_component",
		r#"{"style":"function"}"#,
		0.95,
		"react-detector:0.1.0",
	);
	insert_inference(
		&db_path,
		"inf-002",
		&snapshot_uid,
		"test-repo",
		"test-repo:src/Button.tsx#useState:CALL:react",
		"react_hook_usage",
		r#"{"hook":"useState","builtin":true}"#,
		0.9,
		"react-detector:0.1.0",
	);
	insert_inference(
		&db_path,
		"inf-003",
		&snapshot_uid,
		"test-repo",
		"test-repo:src/Form.tsx#Form:SYMBOL:FUNCTION",
		"react_component",
		r#"{"style":"function"}"#,
		0.95,
		"react-detector:0.1.0",
	);

	// Filter by react_component
	let output = Command::new(binary_path())
		.args([
			"inferences",
			"list",
			db_path.to_str().unwrap(),
			"test-repo",
			"--kind",
			"react_component",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["count"], 2);
	assert_eq!(json["filter_kind"], "react_component");

	let results = json["results"].as_array().unwrap();
	assert_eq!(results.len(), 2);

	// All results should be react_component
	for result in results {
		assert_eq!(result["kind"], "react_component");
	}
}

#[test]
fn inferences_list_output_structure() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Clear any existing inferences
	let conn = rusqlite::Connection::open(&db_path).unwrap();
	conn.execute("DELETE FROM inferences WHERE repo_uid = 'test-repo'", [])
		.unwrap();
	drop(conn);

	// Insert a single inference with all fields
	insert_inference(
		&db_path,
		"inf-struct-001",
		&snapshot_uid,
		"test-repo",
		"test-repo:src/App.tsx#App:SYMBOL:FUNCTION",
		"react_component",
		r#"{"style":"function","has_jsx":true}"#,
		0.95,
		"react-detector:0.1.0",
	);

	let output = Command::new(binary_path())
		.args(["inferences", "list", db_path.to_str().unwrap(), "test-repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	// Envelope structure
	assert!(json["command"].is_string());
	assert!(json["repo"].is_string());
	assert!(json["snapshot"].is_string());
	assert!(json["snapshot_scope"].is_string());
	assert!(json["results"].is_array());
	assert!(json["count"].is_number());

	// Result entry structure
	let result = &json["results"][0];
	assert_eq!(result["inference_uid"], "inf-struct-001");
	assert_eq!(result["target_stable_key"], "test-repo:src/App.tsx#App:SYMBOL:FUNCTION");
	assert_eq!(result["kind"], "react_component");
	assert!(result["value"].is_object(), "value should be parsed JSON object");
	assert_eq!(result["value"]["style"], "function");
	assert_eq!(result["value"]["has_jsx"], true);
	assert_eq!(result["confidence"], 0.95);
	assert_eq!(result["extractor"], "react-detector:0.1.0");
	assert!(result["created_at"].is_string());
}
