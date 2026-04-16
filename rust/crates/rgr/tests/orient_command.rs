//! Deterministic tests for the `orient` command (Rust-43B).
//!
//! The tests fall into three groups:
//!
//!   1. Argument-parser tests — cover the CLI-3 and CLI-4
//!      acceptance rules (missing/unknown/repeated budget,
//!      unknown flag, `--focus` propagation as runtime error).
//!   2. Runtime-error tests — missing DB, missing repo,
//!      missing snapshot. All exit 2.
//!   3. One end-to-end smoke test — indexes a tiny TS fixture
//!      via `repo_graph_repo_index::compose::index_path`,
//!      invokes the binary, and asserts the output is valid
//!      `rgr.agent.v1` JSON carrying the always-emitted
//!      informational signals.
//!
//! Rust-43B deliberately does not re-cover per-signal evidence
//! shapes here. The agent crate's own integration tests exercise
//! every signal variant against a fake `AgentStorageRead`. The
//! CLI smoke test only proves the wiring chain
//! `binary → storage open → agent orient → JSON stdout`.

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
// Two-file TS repo with no imports between the files. This gives
// us:
//   - A snapshot with at least MODULE_SUMMARY + SNAPSHOT_INFO
//     always emitting.
//   - No gate requirements, so GATE_NOT_CONFIGURED limit fires.
//   - No cycles, no dead code, no boundary violations → no
//     noise in the ranked signal list.

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

// ── 1. Usage error: missing positional args ─────────────────────

#[test]
fn orient_missing_args_usage_error() {
	let output = run_cmd(&["orient"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage: rmap orient"), "stderr: {}", stderr);
}

#[test]
fn orient_one_positional_arg_usage_error() {
	let output = run_cmd(&["orient", "/some/path.db"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
}

// ── 2. Usage error: flag validation ─────────────────────────────

#[test]
fn orient_unknown_flag_usage_error() {
	let output = run_cmd(&["orient", "/p.db", "r1", "--bogus"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("unknown flag"), "stderr: {}", stderr);
}

#[test]
fn orient_budget_missing_value_usage_error() {
	let output = run_cmd(&["orient", "/p.db", "r1", "--budget"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--budget requires a value"), "stderr: {}", stderr);
}

#[test]
fn orient_budget_unknown_value_usage_error() {
	let output = run_cmd(&["orient", "/p.db", "r1", "--budget", "enormous"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("invalid --budget value"),
		"stderr: {}",
		stderr
	);
	assert!(
		stderr.contains("small|medium|large"),
		"stderr must list accepted values: {}",
		stderr
	);
}

#[test]
fn orient_budget_repeated_usage_error() {
	let (_r, _d, db) = build_indexed_repo();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"orient", db_str, "r1", "--budget", "small", "--budget", "medium",
	]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("--budget specified more than once"),
		"stderr: {}",
		stderr
	);
}

#[test]
fn orient_budget_case_sensitive_usage_error() {
	let (_r, _d, db) = build_indexed_repo();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&["orient", db_str, "r1", "--budget", "Small"]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("invalid --budget value"), "stderr: {}", stderr);
}

#[test]
fn orient_focus_repeated_usage_error() {
	let (_r, _d, db) = build_indexed_repo();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"orient", db_str, "r1", "--focus", "a", "--focus", "b",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("--focus specified more than once"),
		"stderr: {}",
		stderr
	);
}

#[test]
fn orient_focus_flag_as_value_usage_error() {
	// Regression for the P3 review: the parser used to consume
	// the next token after --focus without checking whether it
	// was itself a flag, which silently accepted "--bogus" as
	// a focus string and exited through the runtime
	// FocusNotImplementedYet path (exit 2). A usage error must
	// exit 1 and the diagnostic must name the offending token.
	let (_r, _d, db) = build_indexed_repo();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"orient", db_str, "r1", "--focus", "--bogus",
	]);
	assert_eq!(
		output.status.code(),
		Some(1),
		"flag-as-value must be a usage error (exit 1), not a runtime error (exit 2). stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("--focus requires a value") && stderr.contains("--bogus"),
		"stderr must name the offending flag token: {}",
		stderr
	);
}

#[test]
fn orient_budget_flag_as_value_usage_error() {
	// Same flag-as-value guard applies to --budget. Without
	// the check, `--budget --focus` would be diagnosed as
	// "invalid --budget value: --focus" via the enum match,
	// which is correct in outcome (exit 1) but the diagnostic
	// does not name the situation as "flag consumed as value".
	// The guard makes both errors uniform.
	let (_r, _d, db) = build_indexed_repo();
	let db_str = db.to_str().unwrap();
	let output = run_cmd(&[
		"orient", db_str, "r1", "--budget", "--focus", "x",
	]);
	assert_eq!(
		output.status.code(),
		Some(1),
		"flag-as-value must be a usage error. stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("--budget requires a value") && stderr.contains("--focus"),
		"stderr must name the offending flag token: {}",
		stderr
	);
}

// ── 3. Runtime errors (exit 2) ──────────────────────────────────

#[test]
fn orient_missing_db_runtime_error() {
	let output = run_cmd(&["orient", "/nonexistent.db", "r1"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn orient_missing_repo_runtime_error() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["orient", db.to_str().unwrap(), "nonexistent-repo"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("repo not found") || stderr.contains("nonexistent-repo"),
		"stderr: {}",
		stderr
	);
}

#[test]
fn orient_missing_snapshot_runtime_error() {
	// Acceptance lock item 9: a repo row with no READY snapshot
	// must exit 2 with the NoSnapshot error. Build an empty DB
	// (just the schema) and insert a repo row directly through
	// the storage library. No snapshot is created.
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("no_snapshot.db");

	use repo_graph_storage::types::Repo;
	use repo_graph_storage::StorageConnection;
	let storage = StorageConnection::open(&db_path).unwrap();
	storage
		.add_repo(&Repo {
			repo_uid: "r1".into(),
			name: "no-snapshot-repo".into(),
			root_path: "/tmp/no-snapshot-repo".into(),
			default_branch: None,
			created_at: "2026-04-15T00:00:00Z".into(),
			metadata_json: None,
		})
		.unwrap();
	drop(storage);

	let output = run_cmd(&["orient", db_path.to_str().unwrap(), "r1"]);
	assert_eq!(
		output.status.code(),
		Some(2),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("no READY snapshot") || stderr.contains("index the repo first"),
		"stderr: {}",
		stderr
	);
}

// ── 4. Focus-not-implemented (exit 2) ───────────────────────────

#[test]
fn orient_focus_flag_exits_with_runtime_error() {
	// CLI-4 acceptance: `--focus <value>` is syntactically
	// valid but the runtime surface is not yet implemented.
	// Exit 2 is correct (not 1: not a usage error).
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&[
		"orient", db.to_str().unwrap(), "r1", "--focus", "src/core",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("focus") && stderr.contains("not supported"),
		"stderr should describe FocusNotImplementedYet: {}",
		stderr
	);
}

// ── 5. Default budget success path ──────────────────────────────

#[test]
fn orient_default_budget_small_succeeds() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["orient", db.to_str().unwrap(), "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(!output.stdout.is_empty());
	let json: serde_json::Value = serde_json::from_slice(&output.stdout)
		.expect("stdout must be valid JSON");

	// Envelope identity.
	assert_eq!(json["schema"], "rgr.agent.v1");
	assert_eq!(json["command"], "orient");
	assert_eq!(json["repo"], "r1");
	// Focus resolves to repo level.
	assert_eq!(json["focus"]["resolved"], true);
	assert_eq!(json["focus"]["resolved_kind"], "repo");
	// Confidence present.
	assert!(json["confidence"].is_string());
	// Signals and limits are arrays.
	assert!(json["signals"].is_array());
	assert!(json["limits"].is_array());
}

// ── 6. --budget medium and large ────────────────────────────────

#[test]
fn orient_medium_budget_succeeds() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&[
		"orient",
		db.to_str().unwrap(),
		"r1",
		"--budget",
		"medium",
	]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
}

#[test]
fn orient_large_budget_succeeds() {
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&[
		"orient",
		db.to_str().unwrap(),
		"r1",
		"--budget",
		"large",
	]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
}

// ── 7. End-to-end smoke: required signals + limit ───────────────

#[test]
fn orient_smoke_emits_informational_signals_and_gate_limit() {
	// Acceptance lock item 10: the smoke test pins the minimum
	// expected signal set for an unconfigured tiny repo. The
	// fixture has no requirements, so gate emits the
	// GATE_NOT_CONFIGURED limit (not a gate signal) and the
	// informational signals MODULE_SUMMARY and SNAPSHOT_INFO
	// both always fire.
	let (_r, _d, db) = build_indexed_repo();
	let output = run_cmd(&["orient", db.to_str().unwrap(), "r1", "--budget", "large"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let json: serde_json::Value = serde_json::from_slice(&output.stdout)
		.expect("stdout must be valid JSON");

	let signals = json["signals"].as_array().expect("signals is array");
	let signal_codes: Vec<String> = signals
		.iter()
		.map(|s| s["code"].as_str().unwrap().to_string())
		.collect();

	assert!(
		signal_codes.iter().any(|c| c == "MODULE_SUMMARY"),
		"signals must include MODULE_SUMMARY: {:?}",
		signal_codes
	);
	assert!(
		signal_codes.iter().any(|c| c == "SNAPSHOT_INFO"),
		"signals must include SNAPSHOT_INFO: {:?}",
		signal_codes
	);

	let limits = json["limits"].as_array().expect("limits is array");
	let limit_codes: Vec<String> = limits
		.iter()
		.map(|l| l["code"].as_str().unwrap().to_string())
		.collect();

	// Acceptance lock item 10: either GATE_NOT_CONFIGURED
	// limit OR one of the gate signals. This fixture has no
	// requirements, so the limit path is expected.
	let has_gate_limit = limit_codes.iter().any(|c| c == "GATE_NOT_CONFIGURED");
	let has_gate_signal = signal_codes
		.iter()
		.any(|c| matches!(c.as_str(), "GATE_PASS" | "GATE_FAIL" | "GATE_INCOMPLETE"));
	assert!(
		has_gate_limit || has_gate_signal,
		"orient must report gate state (limit or signal). \
		 signals: {:?}, limits: {:?}",
		signal_codes,
		limit_codes
	);

	// And for this specific fixture: gate must be the limit
	// path because no requirements are seeded.
	assert!(
		has_gate_limit && !has_gate_signal,
		"tiny fixture has no requirements → GATE_NOT_CONFIGURED limit, no gate signal"
	);
}
