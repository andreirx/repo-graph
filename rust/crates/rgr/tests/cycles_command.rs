//! Deterministic tests for the `cycles` command.
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. No cycles (acyclic import graph)
//!   5. Exact cycle detection on a known fixture

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"))
}

/// Build a temp DB with a circular import between two modules.
///
/// Layout:
///   src/a/index.ts — import { foo } from "../b/index";
///   src/b/index.ts — import { bar } from "../a/index";
///
/// Module map:
///   MODULE src/a  --IMPORTS-->  MODULE src/b
///   MODULE src/b  --IMPORTS-->  MODULE src/a
///
/// This produces exactly one cycle: [src/a, src/b].
fn build_cyclic_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src/a")).unwrap();
	std::fs::create_dir_all(root.join("src/b")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/a/index.ts"),
		"import { foo } from \"../b/index\";\nexport const bar = 1;\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/b/index.ts"),
		"import { bar } from \"../a/index\";\nexport const foo = 2;\n",
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

/// Build a temp DB with acyclic imports (no cycles).
///
/// Layout:
///   src/a/index.ts — import { foo } from "../b/index";
///   src/b/index.ts — export const foo = 1; (no imports)
///
/// Module map:
///   MODULE src/a  --IMPORTS-->  MODULE src/b
///   (no reverse edge)
fn build_acyclic_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src/a")).unwrap();
	std::fs::create_dir_all(root.join("src/b")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/a/index.ts"),
		"import { foo } from \"../b/index\";\nexport const bar = 1;\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/b/index.ts"),
		"export const foo = 1;\n",
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
fn cycles_usage_error() {
	let output = Command::new(binary_path())
		.args(["cycles"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn cycles_missing_db() {
	let output = Command::new(binary_path())
		.args(["cycles", "/nonexistent.db", "r1"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn cycles_repo_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_acyclic_db();

	let output = Command::new(binary_path())
		.args([
			"cycles",
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

// -- 4. No cycles (acyclic import graph) ------------------------------

#[test]
fn cycles_none_when_acyclic() {
	let (_repo_dir, _db_dir, db_path) = build_acyclic_db();

	let output = Command::new(binary_path())
		.args([
			"cycles",
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
	assert!(output.stderr.is_empty());

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
	assert_eq!(result["count"], 0, "acyclic graph should have 0 cycles, stdout: {}", stdout);
	assert_eq!(result["results"].as_array().unwrap().len(), 0);
}

// -- 5. Exact cycle detection -----------------------------------------

#[test]
fn cycles_exact_results() {
	let (_repo_dir, _db_dir, db_path) = build_cyclic_db();

	let output = Command::new(binary_path())
		.args([
			"cycles",
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
	assert!(output.stderr.is_empty());

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// TS-compatible QueryResult envelope.
	assert_eq!(result["command"], "graph cycles");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental");
	assert!(result["basis_commit"].is_null() || result["basis_commit"].is_string());
	assert!(result["stale"].is_boolean());

	// Exactly one cycle: src/a <-> src/b.
	assert_eq!(
		result["count"], 1,
		"expected 1 cycle, got: {}",
		stdout
	);
	let cycles = result["results"].as_array().unwrap();
	assert_eq!(cycles.len(), 1);

	let cycle = &cycles[0];
	assert_eq!(cycle["cycle_id"], "cycle-1");
	assert_eq!(cycle["length"], 2);

	let nodes = cycle["nodes"].as_array().unwrap();
	assert_eq!(nodes.len(), 2);

	let names: Vec<&str> = nodes
		.iter()
		.map(|n| n["name"].as_str().unwrap())
		.collect();

	// Both modules must be present (canonicalized order may vary).
	assert!(
		names.contains(&"a") || names.contains(&"src/a"),
		"cycle should contain module a, got: {:?}",
		names
	);
	assert!(
		names.contains(&"b") || names.contains(&"src/b"),
		"cycle should contain module b, got: {:?}",
		names
	);

	// file is always null for MODULE-level cycle nodes (matches TS).
	for node in nodes {
		assert!(
			node["file"].is_null(),
			"MODULE cycle nodes should have file: null, got: {}",
			node
		);
	}
}
