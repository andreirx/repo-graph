//! Deterministic tests for the `resource` command family.
//!
//! SB-7A: Tests for `rmap resource list` CLI surface.
//!
//! Test matrix:
//!   1. Usage error (missing args)
//!   2. Missing DB / open failure
//!   3. Repo not found
//!   4. Empty results (no resources)
//!   5. List all resources with correct counts
//!   6. Filter by kind

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a temp DB with FS resources by indexing an inline fixture.
///
/// Layout:
///   src/reader.ts — imports readFile from "fs", calls it in a function
///   src/writer.ts — imports writeFile from "fs", calls it in a function
///
/// Expected resources after indexing:
///   - 2 FS_PATH nodes (/etc/config, /var/log/app.log)
///   - 1 READS edge, 1 WRITES edge
fn build_fs_resource_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(root.join("package.json"), r#"{"name":"test"}"#).unwrap();
	std::fs::write(
		root.join("src/reader.ts"),
		r#"import { readFile } from "fs";
export function loadConfig() {
  readFile("/etc/config", () => {});
}
"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/writer.ts"),
		r#"import { writeFile } from "fs";
export function saveLog() {
  writeFile("/var/log/app.log", "data", () => {});
}
"#,
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();
	assert_eq!(result.files_total, 2);

	(repo_dir, db_dir, db_path)
}

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn resource_list_usage_error() {
	let output = Command::new(binary_path())
		.args(["resource", "list"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. Missing DB ────────────────────────────────────────────────

#[test]
fn resource_list_missing_db() {
	let output = Command::new(binary_path())
		.args(["resource", "list", "/nonexistent/path.db", "r1"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("error:"),
		"expected error in stderr: {}",
		stderr
	);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn resource_list_repo_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_fs_resource_db();

	let output = Command::new(binary_path())
		.args([
			"resource",
			"list",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("error:"), "stderr: {}", stderr);
}

// ── 4. List returns correct resources and counts ─────────────────

#[test]
fn resource_list_returns_fs_path_with_counts() {
	let (_repo_dir, _db_dir, db_path) = build_fs_resource_db();

	let output = Command::new(binary_path())
		.args(["resource", "list", db_path.to_str().unwrap(), "r1"])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	// Should have 2 FS_PATH resources.
	assert_eq!(json["count"], 2, "stdout: {}", stdout);

	// Check totals.
	assert_eq!(json["total_reads"], 1, "stdout: {}", stdout);
	assert_eq!(json["total_writes"], 1, "stdout: {}", stdout);

	// Verify the resources exist.
	let results = json["results"].as_array().unwrap();
	assert_eq!(results.len(), 2);

	let config = results
		.iter()
		.find(|r| r["name"].as_str().unwrap().contains("config"))
		.expect("should have /etc/config resource");
	assert_eq!(config["kind"], "FS_PATH");
	assert_eq!(config["readers"], 1);
	assert_eq!(config["writers"], 0);

	let log = results
		.iter()
		.find(|r| r["name"].as_str().unwrap().contains("app.log"))
		.expect("should have /var/log/app.log resource");
	assert_eq!(log["kind"], "FS_PATH");
	assert_eq!(log["readers"], 0);
	assert_eq!(log["writers"], 1);
}

// ── 5. Filter by kind ────────────────────────────────────────────

#[test]
fn resource_list_filter_by_kind() {
	let (_repo_dir, _db_dir, db_path) = build_fs_resource_db();

	// Filter for FS_PATH (should return 2).
	let output = Command::new(binary_path())
		.args([
			"resource",
			"list",
			db_path.to_str().unwrap(),
			"r1",
			"--kind",
			"FS_PATH",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let json: serde_json::Value =
		serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
	assert_eq!(json["count"], 2);

	// Filter for DB_RESOURCE (should return 0).
	let output = Command::new(binary_path())
		.args([
			"resource",
			"list",
			db_path.to_str().unwrap(),
			"r1",
			"--kind",
			"DB_RESOURCE",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let json: serde_json::Value =
		serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
	assert_eq!(json["count"], 0);
}
