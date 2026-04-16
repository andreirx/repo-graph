//! Contract tests for the TS-compatible QueryResult JSON envelope.
//!
//! These tests pin the envelope shape across all five read-side
//! commands (callers, callees, dead, cycles, stats) to prevent
//! silent drift from the established TS `formatQueryResult` contract.
//!
//! Each test verifies:
//!   - All 8 envelope fields are present and typed correctly
//!   - command discriminator matches TS naming ("graph <cmd>")
//!   - stdout/stderr discipline (JSON only on stdout, empty stderr)
//!   - exit code 0 on success
//!
//! Added in Rust-16 (consolidation slice).

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a fixture with enough structure for all five commands.
///
/// Layout:
///   src/a/index.ts — import { foo } from "../b/index"; export function caller() { foo(); }
///   src/b/index.ts — import { bar } from "../a/index"; export function foo() {}
///                     export function unused() {}
///   src/a/types.ts — export interface Config {}
///
/// This gives:
///   - callers: foo has caller as a caller
///   - callees: caller has foo as a callee
///   - dead: unused has no incoming edges
///   - cycles: src/a <-> src/b mutual imports
///   - stats: two modules with nonzero fan-in/fan-out
fn build_fixture_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
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
		"import { foo } from \"../b/index\";\nexport function caller() { foo(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/a/types.ts"),
		"export interface Config {}\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/b/index.ts"),
		"import { bar } from \"../a/index\";\nexport function foo() {}\nexport function unused() {}\nexport const bar = 1;\n",
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();

	(repo_dir, db_dir, db_path)
}

/// Assert the 8 standard QueryResult envelope fields.
fn assert_envelope(result: &serde_json::Value, expected_command: &str) {
	assert_eq!(
		result["command"].as_str().unwrap(),
		expected_command,
		"command discriminator mismatch"
	);
	assert!(
		result["repo"].is_string(),
		"repo must be string, got: {}",
		result["repo"]
	);
	assert!(
		result["snapshot"].is_string(),
		"snapshot must be string, got: {}",
		result["snapshot"]
	);
	let scope = result["snapshot_scope"].as_str().unwrap();
	assert!(
		scope == "full" || scope == "incremental",
		"snapshot_scope must be full or incremental, got: {}",
		scope
	);
	assert!(
		result["basis_commit"].is_null() || result["basis_commit"].is_string(),
		"basis_commit must be string or null, got: {}",
		result["basis_commit"]
	);
	assert!(
		result["results"].is_array(),
		"results must be array"
	);
	assert!(
		result["count"].is_number(),
		"count must be number"
	);
	assert!(
		result["stale"].is_boolean(),
		"stale must be boolean, got: {}",
		result["stale"]
	);
}

/// Run a command, assert exit 0, empty stderr, parse JSON stdout.
fn run_success(args: &[&str]) -> serde_json::Value {
	let output = Command::new(binary_path())
		.args(args)
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"command {:?} failed, stderr: {}",
		args,
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(
		output.stderr.is_empty(),
		"stderr must be empty on success, got: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	serde_json::from_str(&stdout).unwrap_or_else(|e| {
		panic!("invalid JSON for {:?}: {}\nstdout: {}", args, e, stdout)
	})
}

// ── callers envelope ────────────────────────────────────────────

#[test]
fn callers_envelope_contract() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let result = run_success(&["callers", db_str, "r1", "foo"]);
	assert_envelope(&result, "graph callers");

	// Command-specific field: target must be present.
	assert!(result["target"].is_object(), "callers must include target");
	assert_eq!(result["target"]["kind"], "SYMBOL");
}

// ── callees envelope ────────────────────────────────────────────

#[test]
fn callees_envelope_contract() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let result = run_success(&["callees", db_str, "r1", "caller"]);
	assert_envelope(&result, "graph callees");

	// Command-specific field: target must be present.
	assert!(result["target"].is_object(), "callees must include target");
	assert_eq!(result["target"]["kind"], "SYMBOL");
}

// ── dead envelope ───────────────────────────────────────────────

#[test]
fn dead_envelope_contract() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let result = run_success(&["dead", db_str, "r1", "SYMBOL"]);
	assert_envelope(&result, "graph dead");

	// Command-specific field: kind_filter must be present.
	assert_eq!(result["kind_filter"], "SYMBOL");
}

// ── cycles envelope ─────────────────────────────────────────────

#[test]
fn cycles_envelope_contract() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let result = run_success(&["cycles", db_str, "r1"]);
	assert_envelope(&result, "graph cycles");

	// Should have at least one cycle (src/a <-> src/b).
	assert!(
		result["count"].as_u64().unwrap() >= 1,
		"expected at least 1 cycle"
	);
}

// ── stats envelope ──────────────────────────────────────────────

#[test]
fn stats_envelope_contract() {
	let (_r, _d, db) = build_fixture_db();
	let db_str = db.to_str().unwrap();

	let result = run_success(&["stats", db_str, "r1"]);
	assert_envelope(&result, "graph stats");

	// Each result must have the module metrics fields.
	let results = result["results"].as_array().unwrap();
	assert!(!results.is_empty(), "stats should return at least one module");
	let first = &results[0];
	assert!(first["module"].is_string());
	assert!(first["fan_in"].is_number());
	assert!(first["fan_out"].is_number());
	assert!(first["instability"].is_number());
	assert!(first["abstractness"].is_number());
	assert!(first["distance_from_main_sequence"].is_number());
	assert!(first["file_count"].is_number());
	assert!(first["symbol_count"].is_number());
}
