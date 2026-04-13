//! Deterministic tests for the `trust` command.
//!
//! Uses the Rust composition layer to build a real indexed DB from
//! a fixture repo, then invokes the trust command binary against it.
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found (wrong repo_uid)
//!   4. Success with exact field assertions

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	let mut path = PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"));
	// Fallback for older cargo versions.
	if !path.exists() {
		path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("..")
			.join("..")
			.join("target")
			.join("debug")
			.join("rgr-rust");
	}
	path
}

fn fixture_path() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("..")
		.join("..")
		.join("..")
		.join("test")
		.join("fixtures")
		.join("typescript")
		.join("classifier-repo")
}

/// Build a temp DB by indexing the classifier-repo fixture.
fn build_indexed_db() -> (tempfile::TempDir, PathBuf) {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(
		&fixture_path(),
		&db_path,
		"classifier-repo",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(result.files_total, 1);

	(dir, db_path)
}

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn trust_usage_error_exit_1() {
	let output = Command::new(binary_path())
		.args(["trust"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn trust_missing_db_exit_2() {
	let output = Command::new(binary_path())
		.args(["trust", "/nonexistent/path.db", "repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("does not exist"),
		"stderr: {}",
		stderr
	);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn trust_repo_not_found_exit_2() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"trust",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("no snapshot found"),
		"stderr: {}",
		stderr
	);
}

// ── 4. Success with exact field assertions ───────────────────────

#[test]
fn trust_success_produces_valid_report() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"trust",
			db_path.to_str().unwrap(),
			"classifier-repo",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	// Success contract: JSON only on stdout, nothing on stderr.
	assert!(
		output.stderr.is_empty(),
		"stderr must be empty on success, got: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let report: serde_json::Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

	// ── Exact field assertions ───────────────────────────────

	// Snapshot UID starts with repo name.
	assert!(
		report["snapshot_uid"]
			.as_str()
			.unwrap()
			.starts_with("classifier-repo/"),
		"snapshot_uid: {:?}",
		report["snapshot_uid"]
	);

	// Diagnostics available (the fixture has unresolved edges).
	assert_eq!(report["diagnostics_available"], true);
	assert_eq!(report["diagnostics_version"], 1);

	// Summary counts match the fixture.
	let summary = &report["summary"];
	assert_eq!(summary["unresolved_total"], 5);
	assert_eq!(summary["resolved_calls"], 0);
	assert_eq!(summary["unresolved_calls"], 4);

	// Call resolution rate: 0/(0+3) = 0.0 (3 internal-like calls).
	assert_eq!(summary["call_resolution_rate"], 0.0);

	// Reliability: call graph should be LOW (rate = 0.0 < 0.50).
	assert_eq!(
		summary["reliability"]["call_graph"]["level"], "LOW",
		"call graph reliability should be LOW for 0% resolution"
	);

	// Dead code reliability should be LOW (missing entrypoints + low call graph).
	assert_eq!(
		summary["reliability"]["dead_code"]["level"], "LOW",
		"dead code reliability"
	);

	// Caveats should include at least the permanent cycle caveat
	// and the call-graph caveat.
	let caveats = report["caveats"].as_array().unwrap();
	assert!(
		caveats.len() >= 2,
		"expected >= 2 caveats, got {}",
		caveats.len()
	);
	assert!(
		caveats.iter().any(|c| c.as_str().unwrap().contains("Call-graph reliability")),
		"missing call-graph caveat"
	);
	assert!(
		caveats.iter().any(|c| c.as_str().unwrap().contains("Cycle payloads")),
		"missing permanent cycle caveat"
	);

	// Categories breakdown should have calls_function_ambiguous_or_missing.
	let categories = report["categories"].as_array().unwrap();
	let calls_cat = categories.iter().find(|c| {
		c["category"].as_str() == Some("calls_function_ambiguous_or_missing")
	});
	assert!(calls_cat.is_some(), "missing calls_function category");
	assert_eq!(calls_cat.unwrap()["unresolved"], 4);
}
