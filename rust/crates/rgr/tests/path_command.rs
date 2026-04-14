//! Deterministic tests for the `path` command.
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. From symbol not found
//!   5. To symbol not found
//!   6. No path exists (both resolve, no route)
//!   7. Exact path result with pinned steps
//!   8. Envelope contract

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"))
}

/// Build a fixture with a known call chain for path testing.
///
/// Layout:
///   src/index.ts  — import { serve } from "./server";
///                    export function main() { serve(); }
///   src/server.ts — import { helper } from "./util";
///                    export function serve() { helper(); }
///   src/util.ts   — export function helper() {}
///                    export function isolated() {}
///
/// Call chain: main --CALLS--> serve --CALLS--> helper
/// isolated() has no edges to/from the chain.
///
/// Expected paths:
///   main → serve:       length 1 (direct CALLS)
///   main → helper:      length 2 (main→serve→helper)
///   main → isolated:    no path
///   helper → main:      no path (edges are directional)
fn build_path_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
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
		"import { serve } from \"./server\";\nexport function main() { serve(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/server.ts"),
		"import { helper } from \"./util\";\nexport function serve() { helper(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/util.ts"),
		"export function helper() {}\nexport function isolated() {}\n",
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
fn path_usage_error() {
	let output = run_cmd(&["path"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn path_missing_db() {
	let output = run_cmd(&["path", "/nonexistent.db", "r1", "main", "serve"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn path_repo_not_found() {
	let (_r, _d, db) = build_path_db();
	let output = run_cmd(&["path", db.to_str().unwrap(), "nonexistent", "main", "serve"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("no snapshot"), "stderr: {}", stderr);
}

// -- 4. From symbol not found -----------------------------------------

#[test]
fn path_from_symbol_not_found() {
	let (_r, _d, db) = build_path_db();
	let output = run_cmd(&["path", db.to_str().unwrap(), "r1", "nonexistent", "serve"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

// -- 5. To symbol not found -------------------------------------------

#[test]
fn path_to_symbol_not_found() {
	let (_r, _d, db) = build_path_db();
	let output = run_cmd(&["path", db.to_str().unwrap(), "r1", "serve", "nonexistent"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

// -- 6. No path exists ------------------------------------------------

#[test]
fn path_no_route() {
	let (_r, _d, db) = build_path_db();
	let db_str = db.to_str().unwrap();

	// main → isolated: no call chain connects them.
	let output = run_cmd(&["path", db_str, "r1", "main", "isolated"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());

	let result = parse_json(&output);
	assert_eq!(result["count"], 0);
	let results = result["results"].as_array().unwrap();
	assert_eq!(results.len(), 1);
	assert_eq!(results[0]["found"], false);
	assert_eq!(results[0]["path_length"], 0);
	assert_eq!(results[0]["path"].as_array().unwrap().len(), 0);
}

// -- 7. Exact path result ---------------------------------------------

#[test]
fn path_exact_result() {
	let (_r, _d, db) = build_path_db();
	let db_str = db.to_str().unwrap();

	// main → helper: should find path main --CALLS--> serve --CALLS--> helper
	let output = run_cmd(&["path", db_str, "r1", "main", "helper"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());

	let result = parse_json(&output);
	assert_eq!(result["count"], 1);

	let path_result = &result["results"].as_array().unwrap()[0];
	assert_eq!(path_result["found"], true);
	assert_eq!(path_result["path_length"], 2);

	let steps = path_result["path"].as_array().unwrap();
	assert_eq!(steps.len(), 3, "3 nodes: main → serve → helper");

	// Step 0: main (start node, edge_type is empty string).
	assert_eq!(steps[0]["symbol"], "main");
	assert_eq!(steps[0]["edge_type"], "");
	assert!(steps[0]["node_id"].is_string());
	assert!(steps[0]["file"].is_string());

	// Step 1: serve (reached via CALLS).
	assert_eq!(steps[1]["symbol"], "serve");
	assert_eq!(steps[1]["edge_type"], "CALLS");

	// Step 2: helper (reached via CALLS).
	assert_eq!(steps[2]["symbol"], "helper");
	assert_eq!(steps[2]["edge_type"], "CALLS");
}

// -- 8. Envelope contract ---------------------------------------------

#[test]
fn path_envelope_contract() {
	let (_r, _d, db) = build_path_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["path", db_str, "r1", "main", "serve"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);

	// Full TS-compatible QueryResult envelope.
	assert_eq!(result["command"], "graph path");
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

	// Path-specific: results is a 1-element array containing PathResult.
	let results = result["results"].as_array().unwrap();
	assert_eq!(results.len(), 1);
	assert_eq!(results[0]["found"], true);
	assert!(results[0]["path_length"].is_number());
	assert!(results[0]["path"].is_array());
}
