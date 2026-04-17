//! Deterministic tests for the `check` command (Rust-44).
//!
//! The tests cover:
//!
//!   1. Usage errors — missing args, wrong arg count.
//!   2. Runtime errors — missing DB, missing repo.
//!   3. Envelope shape — schema, command, focus, signals.
//!   4. Exit code mapping — verdict to exit code.
//!   5. Signal isolation — no orient-only signals leak.
//!   6. Condition structure — verdict evidence carries conditions.
//!
//! The CLI smoke tests use a real indexed fixture via
//! `repo_graph_repo_index::compose::index_path`. The fixture is
//! intentionally minimal (2-file TS repo, no requirements, no
//! enrichment) to keep the tests fast and deterministic.

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

// ── Fixture: indexed TS repo ─────────────────────────────────────
//
// Two-file TS repo with no imports between the files. On a freshly
// indexed tiny repo the check will produce CHECK_FAIL because:
//   - dead_code_reliability = LOW (no entrypoint declarations)
//   - enrichment_state = NotRun (Rust indexer has no enrichment)
//   - gate: not configured (no requirements)
//
// Exit code: 1 (fail).

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
		"export const a = 1;\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/b.ts"),
		"export const b = 2;\n",
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

// ── 1. Usage error: missing args ────────────────────────────────

#[test]
fn check_usage_error_no_args() {
	let output = run_cmd(&["check"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage"), "stderr: {}", stderr);
}

#[test]
fn check_usage_error_one_arg() {
	let output = run_cmd(&["check", "/some/path.db"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
}

#[test]
fn check_usage_error_too_many_args() {
	let output = run_cmd(&["check", "/some/path.db", "r1", "extra"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
}

// ── 2. Runtime errors (exit 2) ──────────────────────────────────

#[test]
fn check_missing_db_runtime_error() {
	let output = run_cmd(&["check", "/nonexistent.db", "r1"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn check_missing_repo_runtime_error() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["check", db.to_str().unwrap(), "nonexistent-repo"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("repo not found") || stderr.contains("nonexistent-repo"),
		"stderr: {}",
		stderr
	);
}

// ── 3. Envelope shape ───────────────────────────────────────────

#[test]
fn check_envelope_shape() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["check", db.to_str().unwrap(), "r1"]);

	// Output must be valid JSON on stdout.
	assert!(!output.stdout.is_empty(), "must produce JSON output");
	let json: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON");

	// Envelope identity.
	assert_eq!(json["schema"], "rgr.agent.v1");
	assert_eq!(json["command"], "check");

	// Focus resolves to repo level.
	assert_eq!(json["focus"]["resolved"], true);
	assert_eq!(json["focus"]["resolved_kind"], "repo");

	// Confidence present.
	assert!(json["confidence"].is_string());

	// Signals is an array.
	assert!(json["signals"].is_array());
}

// ── 4. Exit code reflects verdict ───────────────────────────────

#[test]
fn check_exit_code_reflects_verdict() {
	// On the tiny repo, check produces CHECK_FAIL (dead_code_reliability
	// LOW + enrichment NotRun). Exit code must be 1.
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["check", db.to_str().unwrap(), "r1"]);

	let json: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON");
	let signals = json["signals"].as_array().expect("signals is array");
	let signal_codes: Vec<String> = signals
		.iter()
		.map(|s| s["code"].as_str().unwrap().to_string())
		.collect();

	// Must have exactly one check verdict signal.
	let check_verdicts: Vec<&String> = signal_codes
		.iter()
		.filter(|c| matches!(c.as_str(), "CHECK_PASS" | "CHECK_FAIL" | "CHECK_INCOMPLETE"))
		.collect();
	assert_eq!(
		check_verdicts.len(),
		1,
		"expected exactly one check verdict signal, got: {:?}",
		check_verdicts
	);

	// For this fixture, the verdict should be CHECK_FAIL.
	assert_eq!(
		check_verdicts[0], "CHECK_FAIL",
		"tiny fixture should produce CHECK_FAIL, got: {}",
		check_verdicts[0]
	);

	// Exit code should be 1 (fail).
	assert_eq!(
		output.status.code(),
		Some(1),
		"CHECK_FAIL must exit 1"
	);
}

// ── 5. No orient signals leak ───────────────────────────────────

#[test]
fn check_no_orient_signals_leak() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["check", db.to_str().unwrap(), "r1"]);
	let json: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON");
	let signals = json["signals"].as_array().expect("signals is array");
	let signal_codes: Vec<String> = signals
		.iter()
		.map(|s| s["code"].as_str().unwrap().to_string())
		.collect();

	// Orient-specific signals that must NOT appear in check output.
	let orient_only_codes = [
		"IMPORT_CYCLES",
		"DEAD_CODE",
		"BOUNDARY_VIOLATIONS",
		"CALLERS_SUMMARY",
		"CALLEES_SUMMARY",
		"MODULE_SUMMARY",
		"HIGH_COMPLEXITY",
		"HIGH_FAN_OUT",
		"HIGH_INSTABILITY",
		"TRUST_LOW_RESOLUTION",
		"TRUST_NO_ENRICHMENT",
		"TRUST_STALE_SNAPSHOT",
		"GATE_PASS",
		"GATE_FAIL",
		"GATE_INCOMPLETE",
	];

	for code in &orient_only_codes {
		assert!(
			!signal_codes.iter().any(|c| c == *code),
			"orient-only signal {} must not appear in check output. All signals: {:?}",
			code,
			signal_codes
		);
	}

	// Only allowed signal codes in check: CHECK_PASS, CHECK_FAIL,
	// CHECK_INCOMPLETE, SNAPSHOT_INFO.
	let allowed = ["CHECK_PASS", "CHECK_FAIL", "CHECK_INCOMPLETE", "SNAPSHOT_INFO"];
	for code in &signal_codes {
		assert!(
			allowed.contains(&code.as_str()),
			"unexpected signal code in check output: {}. Allowed: {:?}",
			code,
			allowed
		);
	}
}

// ── 6. Snapshot info present ────────────────────────────────────

#[test]
fn check_has_snapshot_info() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["check", db.to_str().unwrap(), "r1"]);
	let json: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON");
	let signals = json["signals"].as_array().expect("signals is array");
	let signal_codes: Vec<String> = signals
		.iter()
		.map(|s| s["code"].as_str().unwrap().to_string())
		.collect();

	assert!(
		signal_codes.iter().any(|c| c == "SNAPSHOT_INFO"),
		"check must include SNAPSHOT_INFO signal. All signals: {:?}",
		signal_codes
	);
}

// ── 7. Verdict evidence has conditions ──────────────────────────

#[test]
fn check_envelope_has_conditions() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["check", db.to_str().unwrap(), "r1"]);
	let json: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON");
	let signals = json["signals"].as_array().expect("signals is array");

	// Find the verdict signal.
	let verdict_signal = signals
		.iter()
		.find(|s| {
			let code = s["code"].as_str().unwrap_or("");
			matches!(code, "CHECK_PASS" | "CHECK_FAIL" | "CHECK_INCOMPLETE")
		})
		.expect("must have a verdict signal");

	let evidence = &verdict_signal["evidence"];

	// For CHECK_FAIL, evidence has `fail_conditions` and `passing` arrays.
	// For CHECK_PASS, evidence has `conditions` array.
	// For CHECK_INCOMPLETE, evidence has `incomplete_conditions`,
	// `fail_conditions`, and `passing`.
	//
	// All condition entries must have `code`, `status`, and `summary`.
	let all_conditions: Vec<&serde_json::Value> = {
		let mut v = Vec::new();
		if let Some(arr) = evidence.get("conditions").and_then(|a| a.as_array()) {
			v.extend(arr.iter());
		}
		if let Some(arr) = evidence.get("fail_conditions").and_then(|a| a.as_array()) {
			v.extend(arr.iter());
		}
		if let Some(arr) = evidence.get("passing").and_then(|a| a.as_array()) {
			v.extend(arr.iter());
		}
		if let Some(arr) = evidence.get("incomplete_conditions").and_then(|a| a.as_array()) {
			v.extend(arr.iter());
		}
		v
	};

	assert!(
		!all_conditions.is_empty(),
		"verdict evidence must contain at least one condition"
	);

	for condition in &all_conditions {
		assert!(
			condition.get("code").is_some() && condition["code"].is_string(),
			"condition must have a string 'code' field: {:?}",
			condition
		);
		assert!(
			condition.get("status").is_some() && condition["status"].is_string(),
			"condition must have a string 'status' field: {:?}",
			condition
		);
		assert!(
			condition.get("summary").is_some() && condition["summary"].is_string(),
			"condition must have a string 'summary' field: {:?}",
			condition
		);
	}
}
