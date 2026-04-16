//! Deterministic tests for `--edge-types` filter on callers/callees.
//!
//! Test matrix:
//!   1.  Invalid edge type → usage error
//!   2.  Missing --edge-types value → usage error
//!   3.  Repeated --edge-types flag → usage error
//!   4.  Empty --edge-types value → usage error
//!   5.  Default (no flag) = CALLS only
//!   6.  Explicit --edge-types CALLS = same as default
//!   7.  --edge-types INSTANTIATES only
//!   8.  --edge-types CALLS,INSTANTIATES = union
//!   9.  Callees symmetry: --edge-types INSTANTIATES
//!   10. Callees symmetry: --edge-types CALLS,INSTANTIATES

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a fixture that produces both CALLS and INSTANTIATES edges.
///
/// Layout:
///   src/index.ts — import { Server } from "./server";
///                  import { helper } from "./util";
///                  export function main() { const s = new Server(); helper(); }
///   src/server.ts — export class Server {}
///   src/util.ts   — export function helper() {}
///
/// Resolved edges from `main`:
///   main --CALLS--> helper        (function call)
///   main --INSTANTIATES--> Server (new Server())
///
/// Resolved edges TO `main`: none (nobody calls main)
/// Callers of helper: main (CALLS)
/// Callers of Server: main (INSTANTIATES)
fn build_fixture_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
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
		"import { Server } from \"./server\";\nimport { helper } from \"./util\";\nexport function main() { const s = new Server(); helper(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/server.ts"),
		"export class Server {}\n",
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

// ── 1. Invalid edge type ────────────────────────────────────────

#[test]
fn callers_invalid_edge_type() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["callers", db_str, "r1", "helper", "--edge-types", "BOGUS"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("unknown edge type"),
		"expected unknown edge type error, stderr: {}",
		stderr
	);
}

// ── 2. Missing --edge-types value ───────────────────────────────

#[test]
fn callers_missing_edge_types_value() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["callers", db_str, "r1", "helper", "--edge-types"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("missing value"),
		"expected missing value error, stderr: {}",
		stderr
	);
}

// ── 3. Repeated --edge-types flag ───────────────────────────────

#[test]
fn callers_repeated_edge_types_flag() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"callers", db_str, "r1", "helper",
		"--edge-types", "CALLS",
		"--edge-types", "INSTANTIATES",
	]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("repeated"),
		"expected repeated flag error, stderr: {}",
		stderr
	);
}

// ── 4. Empty --edge-types value ─────────────────────────────────

#[test]
fn callers_empty_edge_types_value() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["callers", db_str, "r1", "helper", "--edge-types", ""]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
}

// ── 5. Default (no flag) = CALLS only ───────────────────────────

#[test]
fn callers_default_is_calls_only() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	// Server has an INSTANTIATES caller (main) but no CALLS caller.
	// Default (CALLS-only) should return 0.
	let output = run_cmd(&["callers", db_str, "r1", "Server"]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
	let result = parse_json(&output);
	assert_eq!(result["count"], 0, "Server has no CALLS callers");
}

// ── 6. Explicit --edge-types CALLS = same as default ────────────

#[test]
fn callers_explicit_calls_same_as_default() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	// helper has one CALLS caller (main).
	let output = run_cmd(&["callers", db_str, "r1", "helper", "--edge-types", "CALLS"]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
	let result = parse_json(&output);
	assert_eq!(result["count"], 1);
	let callers = result["results"].as_array().unwrap();
	assert_eq!(callers[0]["edge_type"], "CALLS");
}

// ── 7. --edge-types INSTANTIATES only ───────────────────────────

#[test]
fn callers_instantiates_only() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	// Server has one INSTANTIATES caller (main).
	let output = run_cmd(&["callers", db_str, "r1", "Server", "--edge-types", "INSTANTIATES"]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
	let result = parse_json(&output);
	assert_eq!(result["count"], 1, "Server should have 1 INSTANTIATES caller, got: {}", result);

	let callers = result["results"].as_array().unwrap();
	assert_eq!(callers.len(), 1);
	assert!(
		callers[0]["name"].as_str().unwrap() == "main"
			|| callers[0]["stable_key"].as_str().unwrap().contains("index.ts"),
		"caller should be main, got: {}",
		callers[0]
	);
	assert_eq!(callers[0]["edge_type"], "INSTANTIATES");
}

// ── 8. --edge-types CALLS,INSTANTIATES = union ──────────────────

#[test]
fn callers_union_calls_and_instantiates() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	// Server has 0 CALLS callers but 1 INSTANTIATES caller (main).
	// Test 5 already proved CALLS-only returns 0 for Server.
	// The union must return 1, proving INSTANTIATES is included.
	let output = run_cmd(&["callers", db_str, "r1", "Server", "--edge-types", "CALLS,INSTANTIATES"]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
	let result = parse_json(&output);
	assert_eq!(
		result["count"], 1,
		"Server has 0 CALLS callers + 1 INSTANTIATES caller = 1 under union, got: {}",
		result
	);

	let callers = result["results"].as_array().unwrap();
	assert_eq!(callers[0]["edge_type"], "INSTANTIATES");
}

// ── 9. Callees: --edge-types INSTANTIATES ───────────────────────

#[test]
fn callees_instantiates_only() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	// main calls helper (CALLS) and instantiates Server (INSTANTIATES).
	// With INSTANTIATES filter, only Server should appear.
	let output = run_cmd(&["callees", db_str, "r1", "main", "--edge-types", "INSTANTIATES"]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
	let result = parse_json(&output);
	assert_eq!(result["count"], 1, "main has 1 INSTANTIATES callee, got: {}", result);

	let callees = result["results"].as_array().unwrap();
	assert_eq!(callees[0]["name"], "Server");
	assert_eq!(callees[0]["edge_type"], "INSTANTIATES");
}

// ── 10. Callees: --edge-types CALLS,INSTANTIATES = union ────────

#[test]
fn callees_union_calls_and_instantiates() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	// main has both CALLS (helper) and INSTANTIATES (Server) callees.
	let output = run_cmd(&["callees", db_str, "r1", "main", "--edge-types", "CALLS,INSTANTIATES"]);
	assert_eq!(output.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&output.stderr));
	let result = parse_json(&output);
	assert_eq!(result["count"], 2, "main has 2 callees (CALLS + INSTANTIATES), got: {}", result);

	let callees = result["results"].as_array().unwrap();
	let edge_types: Vec<&str> = callees
		.iter()
		.map(|c| c["edge_type"].as_str().unwrap())
		.collect();
	assert!(
		edge_types.contains(&"CALLS"),
		"should contain CALLS edge, got: {:?}",
		edge_types
	);
	assert!(
		edge_types.contains(&"INSTANTIATES"),
		"should contain INSTANTIATES edge, got: {:?}",
		edge_types
	);
}
