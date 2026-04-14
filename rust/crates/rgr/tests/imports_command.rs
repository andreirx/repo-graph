//! Deterministic tests for the `imports` command.
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. File not found
//!   5. No imports (file exists but imports nothing)
//!   6. Exact imports result on a known fixture
//!   7. Envelope contract

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"))
}

/// Build a fixture with known import relationships.
///
/// Layout:
///   src/index.ts  — import { serve } from "./server";
///   src/server.ts — import { helper } from "./util";
///                    export function serve() { helper(); }
///   src/util.ts   — export function helper() {}
///
/// FILE-level IMPORTS edges:
///   FILE:src/index.ts  --IMPORTS--> FILE:src/server.ts
///   FILE:src/server.ts --IMPORTS--> FILE:src/util.ts
///   (src/util.ts imports nothing)
fn build_imports_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/index.ts"),
		"import { serve } from \"./server\";\nserve();\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/server.ts"),
		"import { helper } from \"./util\";\nexport function serve() { helper(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/util.ts"),
		"export function helper() {}\n",
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
	assert_eq!(result.files_total, 3);

	(repo_dir, db_dir, db_path)
}

fn run_cmd(args: &[&str]) -> std::process::Output {
	Command::new(binary_path())
		.args(args)
		.output()
		.unwrap()
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
	let stdout = String::from_utf8_lossy(&output.stdout);
	serde_json::from_str(&stdout).unwrap_or_else(|e| {
		panic!("invalid JSON: {}\nstdout: {}", e, stdout)
	})
}

// -- 1. Usage error ---------------------------------------------------

#[test]
fn imports_usage_error() {
	let output = run_cmd(&["imports"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn imports_missing_db() {
	let output = run_cmd(&["imports", "/nonexistent.db", "r1", "src/index.ts"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn imports_repo_not_found() {
	let (_r, _d, db) = build_imports_db();
	let output = run_cmd(&["imports", db.to_str().unwrap(), "nonexistent", "src/index.ts"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("no snapshot"), "stderr: {}", stderr);
}

// -- 4. File not found ------------------------------------------------

#[test]
fn imports_file_not_found() {
	let (_r, _d, db) = build_imports_db();
	let output = run_cmd(&["imports", db.to_str().unwrap(), "r1", "src/nonexistent.ts"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("file not found"), "stderr: {}", stderr);
}

// -- 5. No imports (file exists but imports nothing) ------------------

#[test]
fn imports_empty_when_no_imports() {
	let (_r, _d, db) = build_imports_db();
	let db_str = db.to_str().unwrap();

	// src/util.ts has no import statements.
	let output = run_cmd(&["imports", db_str, "r1", "src/util.ts"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());

	let result = parse_json(&output);
	assert_eq!(result["count"], 0, "util.ts has no imports");
	assert_eq!(result["results"].as_array().unwrap().len(), 0);
}

// -- 6. Exact imports result ------------------------------------------

#[test]
fn imports_exact_results() {
	let (_r, _d, db) = build_imports_db();
	let db_str = db.to_str().unwrap();

	// src/index.ts imports from ./server → FILE:src/server.ts.
	let output = run_cmd(&["imports", db_str, "r1", "src/index.ts"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());

	let result = parse_json(&output);
	assert_eq!(result["count"], 1, "index.ts imports 1 file, got: {}", result);

	let imports = result["results"].as_array().unwrap();
	assert_eq!(imports.len(), 1);

	let imported = &imports[0];

	// Assert TS-compatible NodeResult wire format fields.
	assert!(imported["node_id"].is_string(), "node_id must be present");
	assert!(
		imported["symbol"].as_str().unwrap().contains("server.ts"),
		"should import server.ts, got: {}",
		imported["symbol"]
	);
	assert_eq!(imported["kind"], "FILE");
	assert!(
		imported["file"].as_str().unwrap().contains("server.ts"),
		"file field should contain server.ts"
	);
	assert_eq!(imported["edge_type"], "IMPORTS");
	assert!(imported["resolution"].is_string());
	assert!(imported["evidence"].is_array());
	assert_eq!(imported["depth"], 1);

	// Also verify server.ts imports util.ts.
	let output2 = run_cmd(&["imports", db_str, "r1", "src/server.ts"]);
	assert_eq!(output2.status.code(), Some(0));

	let result2 = parse_json(&output2);
	assert_eq!(result2["count"], 1, "server.ts imports 1 file");

	let imports2 = result2["results"].as_array().unwrap();
	assert!(
		imports2[0]["symbol"].as_str().unwrap().contains("util.ts"),
		"server.ts should import util.ts, got: {}",
		imports2[0]["symbol"]
	);
	assert_eq!(imports2[0]["depth"], 1);
}

// -- 7. Envelope contract ---------------------------------------------

#[test]
fn imports_envelope_contract() {
	let (_r, _d, db) = build_imports_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["imports", db_str, "r1", "src/index.ts"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);

	// Full TS-compatible QueryResult envelope.
	assert_eq!(result["command"], "graph imports");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(
		result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental"
	);
	assert!(
		result["basis_commit"].is_null() || result["basis_commit"].is_string()
	);
	assert!(result["stale"].is_boolean());
	assert!(result["results"].is_array());
	assert!(result["count"].is_number());
}
