//! Deterministic tests for the `callers` command.
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. Symbol not found
//!   5. Ambiguous symbol
//!   6. No callers (symbol exists, nobody calls it)
//!   7. Direct callers with exact pinned results

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a temp DB by indexing an inline two-file fixture.
///
/// Layout:
///   src/index.ts — imports serve from ./server, calls serve()
///   src/server.ts — export function serve() {}
///
/// This is self-contained (no shared fixture dependency) so the
/// test controls the exact source content and edge expectations.
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
		"import { serve } from \"./server\";\nserve();\n",
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

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn callers_usage_error() {
	let output = Command::new(binary_path())
		.args(["callers"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. Missing DB ────────────────────────────────────────────────

#[test]
fn callers_missing_db() {
	let output = Command::new(binary_path())
		.args(["callers", "/nonexistent.db", "r1", "serve"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn callers_repo_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"callers",
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

// ── 4. Symbol not found ──────────────────────────────────────────

#[test]
fn callers_symbol_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"callers",
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

// ── 5. Ambiguous symbol ──────────────────────────────────────────

#[test]
fn callers_ambiguous_symbol() {
	// Build a DB from a fixture where a name matches multiple symbols.
	// The rust-7a-fixture has src/index.ts and src/server.ts — both
	// are FILE nodes with name "index.ts" and "server.ts". But FILE
	// nodes have kind=FILE, and resolve_symbol only matches SYMBOL
	// kind at the name step. So we need two SYMBOLs with the same name.
	//
	// Use the classifier-repo instead: it only has one SYMBOL (standalone).
	// For a true ambiguity test, we need a custom fixture.
	// For now, test that the ambiguous path is reachable by querying
	// a name that doesn't exist (which hits NotFound, not Ambiguous).
	//
	// A real ambiguity test requires a fixture with two functions
	// named the same in different files. Since we can't easily create
	// that with the existing fixtures, we verify the error path works
	// by building a custom DB.
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
			"callers",
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

// ── 6. No callers ────────────────────────────────────────────────

#[test]
fn callers_no_callers() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	// src/server.ts exports `unused()` which nobody calls.
	// This is a real SYMBOL target with zero incoming CALLS edges.
	let output = Command::new(binary_path())
		.args([
			"callers",
			db_path.to_str().unwrap(),
			"r1",
			"unused",
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
	assert_eq!(result["target"]["name"], "unused");
	assert_eq!(result["target"]["kind"], "SYMBOL");
}

// ── 7. Direct callers with exact pinned results ──────────────────

#[test]
fn callers_with_exact_results() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

	// serve() is called from src/index.ts (top-level call).
	let output = Command::new(binary_path())
		.args([
			"callers",
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

	// TS-compatible QueryResult envelope.
	assert_eq!(result["command"], "graph callers");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental");
	assert!(result["basis_commit"].is_null() || result["basis_commit"].is_string());
	assert!(result["stale"].is_boolean());

	// Target is the serve function.
	let target = &result["target"];
	assert_eq!(target["name"], "serve");
	assert!(
		target["stable_key"]
			.as_str()
			.unwrap()
			.contains("#serve:SYMBOL:FUNCTION"),
	);

	// One direct caller: the FILE node for index.ts (top-level call).
	assert_eq!(result["count"], 1);
	let callers = result["results"].as_array().unwrap();
	assert_eq!(callers.len(), 1);

	let caller = &callers[0];
	assert!(
		caller["stable_key"]
			.as_str()
			.unwrap()
			.contains("index.ts"),
		"caller should be from index.ts, got: {}",
		caller["stable_key"]
	);
	assert_eq!(caller["edge_type"], "CALLS");
	assert_eq!(caller["resolution"], "static");
}
