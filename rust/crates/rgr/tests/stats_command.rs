//! Deterministic tests for the `stats` command.
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. Exact metrics on a known module graph
//!   5. Empty-module behavior (no exported symbols)
//!   6. Results sorted by module path

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a temp DB with a known module graph for metrics verification.
///
/// Layout:
///   src/core/service.ts  — import { helper } from "../util/helper";
///                          export function serve() { helper(); }
///   src/core/types.ts    — export interface Config {}
///                          export type Status = "ok" | "err";
///   src/util/helper.ts   — export function helper() { return 1; }
///   src/empty/stub.ts    — const x = 1; (no exports, no types)
///
/// Module map:
///   src/core  → IMPORTS → src/util   (service.ts imports helper.ts)
///   src/util  → (no outgoing IMPORTS)
///   src/empty → (no outgoing IMPORTS, no incoming)
///
/// Expected metrics:
///   src/core:
///     fan_in=0 (nobody imports core)
///     fan_out=1 (imports util)
///     instability=1.0  (1 / (0+1))
///     file_count=2
///     symbol_count = exported symbols in core's files
///     abstractness: types.ts has INTERFACE + TYPE_ALIAS = 2 abstract,
///                   2 total type-like. service.ts has FUNCTION (not type).
///                   abstract_count=2, type_count=2 → abstractness=1.0
///     distance=|1.0 + 1.0 - 1| = 1.0
///
///   src/util:
///     fan_in=1 (core imports util)
///     fan_out=0
///     instability=0.0  (0 / (1+0))
///     file_count=1
///     abstractness=0.0 (no types, only function)
///     distance=|0.0 + 0.0 - 1| = 1.0
///
///   src/empty:
///     fan_in=0, fan_out=0
///     instability=0.0
///     file_count=1
///     symbol_count=0 (no exports)
///     abstractness=0.0
///     distance=1.0
fn build_stats_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src/core")).unwrap();
	std::fs::create_dir_all(root.join("src/util")).unwrap();
	std::fs::create_dir_all(root.join("src/empty")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/core/service.ts"),
		"import { helper } from \"../util/helper\";\nexport function serve() { helper(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/core/types.ts"),
		"export interface Config {}\nexport type Status = \"ok\" | \"err\";\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/util/helper.ts"),
		"export function helper() { return 1; }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/empty/stub.ts"),
		"const x = 1;\n",
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
	assert_eq!(result.files_total, 4);

	(repo_dir, db_dir, db_path)
}

// -- 1. Usage error ---------------------------------------------------

#[test]
fn stats_usage_error() {
	let output = Command::new(binary_path())
		.args(["stats"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn stats_missing_db() {
	let output = Command::new(binary_path())
		.args(["stats", "/nonexistent.db", "r1"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn stats_repo_not_found() {
	let (_repo_dir, _db_dir, db_path) = build_stats_db();

	let output = Command::new(binary_path())
		.args([
			"stats",
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

// -- 4. Exact metrics on known module graph ---------------------------

#[test]
fn stats_exact_metrics() {
	let (_repo_dir, _db_dir, db_path) = build_stats_db();

	let output = Command::new(binary_path())
		.args([
			"stats",
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

	// Verify TS-compatible QueryResult envelope fields.
	assert_eq!(result["command"], "graph stats");
	assert!(
		result["repo"].is_string(),
		"repo field must be present"
	);
	assert!(
		result["snapshot"].is_string(),
		"snapshot field must be present"
	);
	assert!(
		result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental",
		"snapshot_scope must be full or incremental, got: {}",
		result["snapshot_scope"]
	);
	// basis_commit may be null (no git basis in temp fixture).
	assert!(
		result["basis_commit"].is_null() || result["basis_commit"].is_string(),
		"basis_commit must be string or null"
	);
	assert!(
		result["stale"].is_boolean(),
		"stale field must be boolean"
	);

	let results = result["results"].as_array().unwrap();

	// Find modules by path.
	let find = |path: &str| -> &serde_json::Value {
		results
			.iter()
			.find(|r| r["module"].as_str().unwrap() == path)
			.unwrap_or_else(|| panic!("module '{}' not found in results: {}", path, stdout))
	};

	// src/core: fan_in=0, fan_out=1, instability=1.0
	let core = find("src/core");
	assert_eq!(core["fan_in"], 0, "core fan_in");
	assert_eq!(core["fan_out"], 1, "core fan_out");
	assert_eq!(core["instability"], 1.0, "core instability");
	assert_eq!(core["file_count"], 2, "core file_count");

	// src/util: fan_in=1, fan_out=0, instability=0.0
	let util = find("src/util");
	assert_eq!(util["fan_in"], 1, "util fan_in");
	assert_eq!(util["fan_out"], 0, "util fan_out");
	assert_eq!(util["instability"], 0.0, "util instability");
	assert_eq!(util["file_count"], 1, "util file_count");
}

// -- 5. Empty module behavior -----------------------------------------

#[test]
fn stats_empty_module_has_zero_symbols() {
	let (_repo_dir, _db_dir, db_path) = build_stats_db();

	let output = Command::new(binary_path())
		.args([
			"stats",
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

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
	let results = result["results"].as_array().unwrap();

	let empty = results
		.iter()
		.find(|r| r["module"].as_str().unwrap() == "src/empty");

	// src/empty has stub.ts with no exports → should still appear
	// (file_count > 0) but with symbol_count = 0.
	let empty = empty.unwrap_or_else(|| {
		panic!(
			"src/empty module should be in results (has 1 file), modules: {:?}",
			results.iter().map(|r| r["module"].as_str().unwrap()).collect::<Vec<_>>()
		)
	});

	assert_eq!(empty["symbol_count"], 0, "empty module symbol_count");
	assert_eq!(empty["fan_in"], 0);
	assert_eq!(empty["fan_out"], 0);
	assert_eq!(empty["instability"], 0.0);
	assert_eq!(empty["abstractness"], 0.0);
	assert_eq!(empty["distance_from_main_sequence"], 1.0);
}

// -- 6. Results sorted by module path ---------------------------------

#[test]
fn stats_results_sorted_by_module_path() {
	let (_repo_dir, _db_dir, db_path) = build_stats_db();

	let output = Command::new(binary_path())
		.args([
			"stats",
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

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
	let results = result["results"].as_array().unwrap();

	let paths: Vec<&str> = results
		.iter()
		.map(|r| r["module"].as_str().unwrap())
		.collect();

	let mut sorted_paths = paths.clone();
	sorted_paths.sort();
	assert_eq!(
		paths, sorted_paths,
		"results must be sorted by module path"
	);
}
