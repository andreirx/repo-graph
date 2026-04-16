//! Deterministic tests for the `callees` command.
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. Symbol not found
//!   5. Ambiguous symbol
//!   6. No callees (symbol exists, calls nothing)
//!   7. Direct callees with exact pinned results

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a temp DB by indexing an inline two-file fixture.
///
/// Layout:
///   src/index.ts — imports serve from ./server, defines main() which calls serve()
///   src/server.ts — export function serve() {}, export function unused() {}
///
/// Edge map (CALLS only, resolved):
///   main → serve   (function-to-function, static)
///
/// This gives:
///   - main:  1 callee (serve)
///   - serve: 0 callees (empty body)
///   - unused: 0 callees (empty body, nobody calls it either)
fn build_indexed_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{"express":"1"}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/index.ts"),
		"import { serve } from \"./server\";\nexport function main() { serve(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/server.ts"),
		"export function serve() {}\nexport function unused() {}\n",
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
fn callees_usage_error() {
	let output = Command::new(binary_path())
		.args(["callees"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn callees_missing_db() {
	let output = Command::new(binary_path())
		.args(["callees", "/nonexistent.db", "r1", "serve"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn callees_repo_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"callees",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
			"serve",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("no snapshot"), "stderr: {}", stderr);
}

// -- 4. Symbol not found ----------------------------------------------

#[test]
fn callees_symbol_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"callees",
			db_path.to_str().unwrap(),
			"r1",
			"nonexistentSymbol",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

// -- 5. Ambiguous symbol ----------------------------------------------

#[test]
fn callees_ambiguous_symbol() {
	let dir = tempfile::tempdir().unwrap();
	let root = dir.path();
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(
		root.join("src/a.ts"),
		"export function doWork() {}",
	)
	.unwrap();
	std::fs::write(
		root.join("src/b.ts"),
		"export function doWork() {}",
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("ambig.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	index_path(root, &db_path, "ambig", &ComposeOptions::default()).unwrap();

	let output = Command::new(binary_path())
		.args([
			"callees",
			db_path.to_str().unwrap(),
			"ambig",
			"doWork",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("ambiguous"),
		"expected ambiguous error, stderr: {}",
		stderr
	);
}

// -- 6. No callees (symbol exists, calls nothing) ---------------------

#[test]
fn callees_no_callees() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	// serve() has an empty body — no outgoing CALLS edges.
	let output = Command::new(binary_path())
		.args([
			"callees",
			db_path.to_str().unwrap(),
			"r1",
			"serve",
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
	assert_eq!(result["count"], 0);
	assert_eq!(result["results"].as_array().unwrap().len(), 0);
	// Target should still be populated with the resolved SYMBOL.
	assert_eq!(result["target"]["name"], "serve");
	assert_eq!(result["target"]["kind"], "SYMBOL");
}

// -- 7. Direct callees with exact pinned results ----------------------

#[test]
fn callees_with_exact_results() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	// main() calls serve() — one outgoing CALLS edge.
	let output = Command::new(binary_path())
		.args([
			"callees",
			db_path.to_str().unwrap(),
			"r1",
			"main",
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
	assert_eq!(result["command"], "graph callees");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental");
	assert!(result["basis_commit"].is_null() || result["basis_commit"].is_string());
	assert!(result["stale"].is_boolean());

	// Target is the main function.
	let target = &result["target"];
	assert_eq!(target["name"], "main");
	assert!(
		target["stable_key"]
			.as_str()
			.unwrap()
			.contains("#main:SYMBOL:FUNCTION"),
	);

	// One direct callee: the serve function in server.ts.
	assert_eq!(result["count"], 1);
	let callees = result["results"].as_array().unwrap();
	assert_eq!(callees.len(), 1);

	let callee = &callees[0];
	assert_eq!(callee["name"], "serve");
	assert!(
		callee["stable_key"]
			.as_str()
			.unwrap()
			.contains("#serve:SYMBOL:FUNCTION"),
		"callee should be serve function, got: {}",
		callee["stable_key"]
	);
	assert_eq!(callee["edge_type"], "CALLS");
	assert_eq!(callee["resolution"], "static");
}
