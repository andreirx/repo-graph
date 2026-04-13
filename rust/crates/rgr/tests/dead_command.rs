//! Deterministic tests for the `dead` command.
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. Invalid kind filter (typo → exit 1)
//!   5. Empty result (all symbols are referenced)
//!   6. Exact dead symbols on a known fixture
//!   7. Kind filter narrows results to SYMBOL only

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"))
}

/// Build a temp DB by indexing an inline three-file fixture.
///
/// Layout:
///   src/index.ts  — imports serve from ./server, defines main() which calls serve()
///   src/server.ts — export function serve() {}, export function unused() {}
///   src/utils.ts  — export function helper() { return 1; }
///
/// Edge map (resolved):
///   FILE:index.ts  --IMPORTS--> FILE:server.ts
///   SYMBOL:main    --CALLS-->   SYMBOL:serve
///
/// Dead symbols (no incoming reference edges):
///   - main     (exported, nobody calls it)
///   - unused   (exported, nobody calls or imports it)
///   - helper   (exported, nobody calls or imports it)
///
/// Alive symbols:
///   - serve    (called by main)
///
/// Dead FILE nodes:
///   - FILE:index.ts   (nothing imports it)
///   - FILE:utils.ts   (nothing imports it)
///
/// Alive FILE nodes:
///   - FILE:server.ts  (imported by index.ts)
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
	std::fs::write(
		root.join("src/utils.ts"),
		"export function helper() { return 1; }\n",
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

// -- 1. Usage error ---------------------------------------------------

#[test]
fn dead_usage_error() {
	let output = Command::new(binary_path())
		.args(["dead"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn dead_missing_db() {
	let output = Command::new(binary_path())
		.args(["dead", "/nonexistent.db", "r1"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn dead_repo_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"dead",
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

// -- 4. Invalid kind filter -------------------------------------------

#[test]
fn dead_invalid_kind_is_usage_error() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"dead",
			db_path.to_str().unwrap(),
			"r1",
			"SYMOBL", // typo
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("unknown kind"),
		"expected kind validation error, stderr: {}",
		stderr
	);
}

// -- 5. Empty result (all symbols referenced) -------------------------

#[test]
fn dead_empty_when_all_referenced() {
	// Build a minimal fixture where every symbol is referenced.
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{}}"#,
	)
	.unwrap();
	// a.ts imports b's work function and calls it.
	// b.ts exports work. Both files are imported/called.
	std::fs::write(
		root.join("src/a.ts"),
		"import { work } from \"./b\";\nwork();\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/b.ts"),
		"export function work() {}\n",
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();

	let output = Command::new(binary_path())
		.args([
			"dead",
			db_path.to_str().unwrap(),
			"r1",
			"SYMBOL",
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
	// work() is called from a.ts → it has an incoming CALLS edge → alive.
	// The FILE-level top-level call from a.ts to work means work is referenced.
	// With SYMBOL filter, the only symbol is work, and it's alive.
	assert_eq!(
		result["count"], 0,
		"all symbols referenced, got: {}",
		stdout
	);
}

// -- 5. Exact dead symbols on known fixture ---------------------------

#[test]
fn dead_exact_results() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	// No kind filter — returns all dead nodes (SYMBOL + FILE + MODULE).
	let output = Command::new(binary_path())
		.args([
			"dead",
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

	// Collect all dead node names for assertion.
	let dead_symbols: Vec<&str> = result["results"]
		.as_array()
		.unwrap()
		.iter()
		.map(|r| r["symbol"].as_str().unwrap())
		.collect();

	// Dead symbols: main, unused, helper (no incoming reference edges).
	assert!(
		dead_symbols.contains(&"main"),
		"main should be dead, got: {:?}",
		dead_symbols
	);
	assert!(
		dead_symbols.contains(&"unused"),
		"unused should be dead, got: {:?}",
		dead_symbols
	);
	assert!(
		dead_symbols.contains(&"helper"),
		"helper should be dead, got: {:?}",
		dead_symbols
	);

	// serve should NOT be dead (called by main).
	assert!(
		!dead_symbols.contains(&"serve"),
		"serve should be alive (called by main), got: {:?}",
		dead_symbols
	);
}

// -- 6. Kind filter narrows to SYMBOL only ----------------------------

#[test]
fn dead_kind_filter_symbol() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"dead",
			db_path.to_str().unwrap(),
			"r1",
			"SYMBOL",
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

	assert_eq!(result["kind_filter"], "SYMBOL");

	let results = result["results"].as_array().unwrap();

	// Every result must be kind=SYMBOL.
	for r in results {
		assert_eq!(
			r["kind"], "SYMBOL",
			"kind filter should restrict to SYMBOL, got: {}",
			r
		);
	}

	// Exact dead SYMBOL set: main, unused, helper.
	let dead_names: Vec<&str> = results
		.iter()
		.map(|r| r["symbol"].as_str().unwrap())
		.collect();
	assert_eq!(
		dead_names.len(),
		3,
		"expected 3 dead symbols, got {:?}",
		dead_names
	);
	assert!(dead_names.contains(&"main"));
	assert!(dead_names.contains(&"unused"));
	assert!(dead_names.contains(&"helper"));

	// serve is alive — must not appear.
	assert!(!dead_names.contains(&"serve"));
}
