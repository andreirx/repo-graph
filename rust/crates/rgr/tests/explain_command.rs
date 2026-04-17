//! Deterministic tests for the `explain` command.

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

fn run_cmd(args: &[&str]) -> std::process::Output {
	Command::new(binary_path())
		.args(args)
		.output()
		.expect("failed to spawn rmap")
}

fn build_indexed_repo() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"name":"tiny","dependencies":{}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/a.ts"),
		"export function hello() { return 1; }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/b.ts"),
		"import { hello } from './a';\nexport const x = hello();\n",
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(root, &db_path, "r1", &ComposeOptions::default())
		.unwrap();
	assert_eq!(result.files_total, 2);

	(repo_dir, db_dir, db_path)
}

// ── 1. Usage error: wrong arg count ────────────────────────────

#[test]
fn explain_usage_error_no_args() {
	let output = run_cmd(&["explain"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage"), "stderr: {}", stderr);
}

#[test]
fn explain_usage_error_too_few_args() {
	let output = run_cmd(&["explain", "/some/path.db", "r1"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
}

// ── 2. Runtime errors (exit 2) ──────────────────────────────────

#[test]
fn explain_missing_db_runtime_error() {
	let output = run_cmd(&["explain", "/nonexistent.db", "r1", "src/a.ts"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn explain_missing_repo_runtime_error() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&[
		"explain",
		db.to_str().unwrap(),
		"nonexistent-repo",
		"src/a.ts",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("repo not found") || stderr.contains("nonexistent-repo"),
		"stderr: {}",
		stderr
	);
}

// ── 3. Valid explain with JSON shape assertion ──────────────────

#[test]
fn explain_valid_file_target() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&[
		"explain",
		db.to_str().unwrap(),
		"r1",
		"src/a.ts",
	]);

	assert_eq!(
		output.status.code(),
		Some(0),
		"explain must exit 0 on success. stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(!output.stdout.is_empty(), "must produce JSON output");

	let json: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON");

	// ── Envelope identity ───────────────────────────────────
	assert_eq!(json["schema"], "rgr.agent.v1");
	assert_eq!(json["command"], "explain");

	// ── Focus resolved ──────────────────────────────────────
	assert_eq!(json["focus"]["resolved"], true);

	// ── Signals present ─────────────────────────────────────
	assert!(json["signals"].is_array());
	let signals = json["signals"].as_array().unwrap();
	assert!(!signals.is_empty(), "must have at least one signal");

	// ── Confidence present ──────────────────────────────────
	assert!(json["confidence"].is_string());
}

#[test]
fn explain_valid_path_target() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&[
		"explain",
		db.to_str().unwrap(),
		"r1",
		"src",
	]);

	assert_eq!(
		output.status.code(),
		Some(0),
		"explain must exit 0. stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("valid JSON");
	assert_eq!(json["command"], "explain");
	assert_eq!(json["focus"]["resolved"], true);
}

// ── 4. command = "explain" in envelope ──────────────────────────

#[test]
fn explain_envelope_command_is_explain() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&[
		"explain",
		db.to_str().unwrap(),
		"r1",
		"src/a.ts",
	]);

	let json: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("valid JSON");
	assert_eq!(
		json["command"], "explain",
		"envelope command must be 'explain'"
	);
}
