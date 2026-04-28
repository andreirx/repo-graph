//! Deterministic tests for the `dead` command.
//!
//! STATUS: COMMAND DISABLED as of 2026-04-27.
//!
//! The CLI surface is disabled due to 85-95% false positive rates on real
//! codebases. Historical contract tests are preserved (ignored) to aid
//! reintroduction. They verify the correct envelope shape, kind filtering,
//! per-result trust, and other contract properties that must hold when
//! the surface is restored.
//!
//! To re-enable: remove `#[ignore]` from each test after the command is
//! reintroduced (see `run_dead` in main.rs for criteria).
//!
//! Test matrix (when enabled):
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. Invalid kind filter (typo → exit 1)
//!   5. Empty result (all symbols are referenced)
//!   6. Exact dead symbols on a known fixture
//!   7. Kind filter narrows results to SYMBOL only

use std::path::PathBuf;
use std::process::Command;

// ═══════════════════════════════════════════════════════════════════════
// ACTIVE TEST: Verifies the command is properly disabled.
// This test is NOT ignored — it pins the intentional disabled behavior.
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn dead_command_is_disabled() {
	// The dead command should exit 2 regardless of arguments.
	let output = Command::new(binary_path())
		.args(["dead", "/any.db", "any"])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(2),
		"dead command should exit 2 (disabled), got: {:?}",
		output.status.code()
	);

	// Stdout should be empty (no JSON output).
	assert!(
		output.stdout.is_empty(),
		"dead command should produce no stdout"
	);

	// Stderr should contain the disabled message.
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("`rmap dead` is disabled"),
		"expected disabled message in stderr, got: {}",
		stderr
	);
	assert!(
		stderr.contains("false-positive"),
		"expected false-positive explanation in stderr, got: {}",
		stderr
	);
	assert!(
		stderr.contains("callers"),
		"expected alternative commands in stderr, got: {}",
		stderr
	);
}

// ═══════════════════════════════════════════════════════════════════════
// IGNORED TESTS: Historical contract tests for reintroduction.
// ═══════════════════════════════════════════════════════════════════════

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
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
#[ignore = "dead command disabled - see module doc"]
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
#[ignore = "dead command disabled - see module doc"]
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
#[ignore = "dead command disabled - see module doc"]
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
#[ignore = "dead command disabled - see module doc"]
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
#[ignore = "dead command disabled - see module doc"]
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
#[ignore = "dead command disabled - see module doc"]
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

	// TS-compatible QueryResult envelope.
	assert_eq!(result["command"], "graph dead");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental");
	assert!(result["basis_commit"].is_null() || result["basis_commit"].is_string());
	assert!(result["stale"].is_boolean());

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
#[ignore = "dead command disabled - see module doc"]
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

// -- 7. Per-result trust.dead_confidence ---------------------------------

#[test]
#[ignore = "dead command disabled - see module doc"]
fn dead_results_include_per_result_trust() {
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

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
	let results = result["results"].as_array().unwrap();

	// Every dead result must have a trust section with dead_confidence.
	for r in results {
		assert!(
			r["trust"].is_object(),
			"every dead result must have trust section, got: {}",
			r
		);
		let trust = &r["trust"];
		assert!(
			trust["dead_confidence"].is_string(),
			"trust must have dead_confidence field, got: {}",
			trust
		);
		let confidence = trust["dead_confidence"].as_str().unwrap();
		assert!(
			confidence == "HIGH" || confidence == "MEDIUM" || confidence == "LOW",
			"dead_confidence must be HIGH/MEDIUM/LOW, got: {}",
			confidence
		);
	}
}

#[test]
#[ignore = "dead command disabled - see module doc"]
fn dead_output_includes_top_level_trust_summary() {
	let (_repo_dir, _db_dir, db_path) = build_indexed_db();

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

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// Top-level trust summary should be present (always for dead).
	assert!(
		result["trust"].is_object(),
		"dead output must have top-level trust summary, got keys: {:?}",
		result.as_object().map(|o| o.keys().collect::<Vec<_>>())
	);

	let trust = &result["trust"];
	assert_eq!(
		trust["summary_scope"], "repo_snapshot",
		"top-level trust must have summary_scope = repo_snapshot"
	);
	assert_eq!(
		trust["graph_basis"], "CALLS+IMPORTS",
		"dead uses CALLS+IMPORTS graph basis"
	);
	assert!(
		trust["reliability"].is_object(),
		"trust must have reliability axes"
	);
}

#[test]
#[ignore = "dead command disabled - see module doc"]
fn dead_confidence_reasons_are_stable_vocabulary() {
	// Build a fixture that will likely have unresolved pressure.
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

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
	let results = result["results"].as_array().unwrap();

	// Valid reason vocabulary.
	let valid_reasons = [
		"framework_opaque",
		"registry_pattern_suspicion",
		"missing_entrypoint_declarations",
		"unresolved_call_pressure",
		"unresolved_import_pressure",
		"trust_unavailable",
	];

	for r in results {
		let trust = &r["trust"];
		// reasons may be absent (skip_serializing_if empty) or an array.
		if let Some(reasons) = trust.get("reasons") {
			if let Some(arr) = reasons.as_array() {
				for reason in arr {
					let reason_str = reason.as_str().unwrap();
					assert!(
						valid_reasons.contains(&reason_str),
						"unknown reason vocabulary: {}, expected one of: {:?}",
						reason_str,
						valid_reasons
					);
				}
			}
		}
	}
}
