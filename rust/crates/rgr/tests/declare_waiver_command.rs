//! Deterministic tests for `declare waiver` (Rust-35).
//!
//! Test matrix:
//!   1. Missing required flags => usage error
//!   2. Invalid --requirement-version => usage error
//!   3. Repeated flag => usage error
//!   4. Flag-looking value => usage error
//!   5. Missing DB => storage error, exit 2
//!   6. Insert success
//!   7. Idempotent repeated insert
//!   8. Reason does not affect identity
//!   9. Optional fields do not affect identity
//!  10. Inserted waiver suppresses gate failure
//!  11. Expired waiver does not suppress gate
//!  12. Exact JSON shape
//!  13. Empty --reason => usage error

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Build a fixture with a cross-module import violation.
/// src/adapters/store.ts imports from src/core/service.ts.
fn build_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src/core")).unwrap();
	std::fs::create_dir_all(root.join("src/adapters")).unwrap();
	std::fs::write(root.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
	std::fs::write(
		root.join("src/core/service.ts"),
		"export function serve() {}\n",
	).unwrap();
	std::fs::write(
		root.join("src/adapters/store.ts"),
		"import { serve } from \"../core/service\";\nexport function store() { serve(); }\n",
	).unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();

	(repo_dir, db_dir, db_path)
}

fn run_cmd(args: &[&str]) -> std::process::Output {
	Command::new(binary_path()).args(args).output().unwrap()
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
	let stdout = String::from_utf8_lossy(&output.stdout);
	serde_json::from_str(&stdout).unwrap_or_else(|e| {
		panic!("invalid JSON: {}\nstdout: {}", e, stdout)
	})
}

/// Set up the full gate fixture: boundary + requirement producing FAIL.
/// adapters --forbids--> core, with arch_violations obligation targeting adapters.
fn setup_failing_gate(db_path: &std::path::Path) {
	let db_str = db_path.to_str().unwrap();

	// Declare boundary: adapters --forbids--> core.
	let out = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/adapters",
		"--forbids", "src/core",
	]);
	assert_eq!(out.status.code(), Some(0));

	// Declare requirement: arch_violations on adapters.
	let out = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "adapters must not depend on core",
		"--target", "src/adapters",
	]);
	assert_eq!(out.status.code(), Some(0));
}

fn base_waiver_args<'a>(db_str: &'a str) -> Vec<&'a str> {
	vec![
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "known dependency tracked for removal",
	]
}

// -- 1. Missing required flags ---------------------------------------

#[test]
fn declare_waiver_missing_requirement_version() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--obligation-id", "obl-1",
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--requirement-version"), "stderr: {}", stderr);
}

#[test]
fn declare_waiver_missing_obligation_id() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--obligation-id"), "stderr: {}", stderr);
}

#[test]
fn declare_waiver_missing_reason() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--reason"), "stderr: {}", stderr);
}

// -- 2. Invalid --requirement-version --------------------------------

#[test]
fn declare_waiver_invalid_version() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "abc",
		"--obligation-id", "obl-1",
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("integer"), "stderr: {}", stderr);
}

// -- 3. Repeated flag ------------------------------------------------

#[test]
fn declare_waiver_repeated_flag() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "first", "--reason", "second",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("more than once"), "stderr: {}", stderr);
}

// -- 4. Flag-looking value -------------------------------------------

#[test]
fn declare_waiver_flag_as_value() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "--obligation-id",
		"--obligation-id", "obl-1",
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("requires a"), "stderr: {}", stderr);
}

// -- 5. Missing DB ---------------------------------------------------

#[test]
fn declare_waiver_missing_db() {
	let output = run_cmd(&[
		"declare", "waiver", "/nonexistent/path.db", "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(2), "missing DB => exit 2");
	assert!(output.stdout.is_empty());
}

// -- 6. Insert success -----------------------------------------------

#[test]
fn declare_waiver_success() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&base_waiver_args(db_str));
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	assert!(result["declaration_uid"].is_string());
	assert_eq!(result["kind"], "waiver");
	assert_eq!(result["req_id"], "REQ-001");
	assert_eq!(result["requirement_version"], 1);
	assert_eq!(result["obligation_id"], "obl-1");
	assert_eq!(result["inserted"], true);
}

// -- 7. Idempotent repeated insert -----------------------------------

#[test]
fn declare_waiver_idempotent() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let first = run_cmd(&base_waiver_args(db_str));
	assert_eq!(first.status.code(), Some(0));
	let first_result = parse_json(&first);
	assert_eq!(first_result["inserted"], true);

	let second = run_cmd(&base_waiver_args(db_str));
	assert_eq!(second.status.code(), Some(0));
	let second_result = parse_json(&second);
	assert_eq!(second_result["inserted"], false);
	assert_eq!(
		first_result["declaration_uid"],
		second_result["declaration_uid"],
	);
}

// -- 8. Reason does not affect identity ------------------------------

#[test]
fn declare_waiver_reason_does_not_affect_identity() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let first = run_cmd(&base_waiver_args(db_str));
	let first_result = parse_json(&first);
	assert_eq!(first_result["inserted"], true);

	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "completely different reason text",
	]);
	let second_result = parse_json(&output);
	assert_eq!(second_result["inserted"], false, "reason change must not create new declaration");
	assert_eq!(
		first_result["declaration_uid"],
		second_result["declaration_uid"],
	);
}

// -- 9. Optional fields do not affect identity -----------------------

#[test]
fn declare_waiver_optional_fields_do_not_affect_identity() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let first = run_cmd(&base_waiver_args(db_str));
	let first_result = parse_json(&first);
	assert_eq!(first_result["inserted"], true);

	// Same identity tuple but with all optional fields added.
	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "same reason",
		"--expires-at", "2030-01-01T00:00:00Z",
		"--created-by", "team-lead",
		"--rationale-category", "tech_debt",
		"--policy-basis", "gate-v2",
	]);
	let second_result = parse_json(&output);
	assert_eq!(second_result["inserted"], false, "optional fields must not affect identity");
	assert_eq!(
		first_result["declaration_uid"],
		second_result["declaration_uid"],
	);
}

// -- 10. Inserted waiver suppresses gate failure ---------------------

#[test]
fn declare_waiver_suppresses_gate() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	setup_failing_gate(&db);

	// Without waiver: gate should FAIL.
	let gate_before = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_before.status.code(), Some(1), "gate should FAIL without waiver");
	let before = parse_json(&gate_before);
	assert_eq!(before["obligations"][0]["computed_verdict"], "FAIL");
	assert_eq!(before["obligations"][0]["effective_verdict"], "FAIL");

	// Declare waiver.
	let waiver_out = run_cmd(&base_waiver_args(db_str));
	assert_eq!(waiver_out.status.code(), Some(0));

	// With waiver: gate should pass (WAIVED).
	let gate_after = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_after.status.code(), Some(0), "gate should pass with waiver");

	let after = parse_json(&gate_after);
	assert_eq!(after["obligations"][0]["computed_verdict"], "FAIL");
	assert_eq!(after["obligations"][0]["effective_verdict"], "WAIVED");

	let basis = &after["obligations"][0]["waiver_basis"];
	assert!(!basis.is_null(), "waiver_basis must be populated");
	assert_eq!(basis["reason"], "known dependency tracked for removal");

	assert_eq!(after["gate"]["counts"]["waived"], 1);
	assert_eq!(after["gate"]["counts"]["fail"], 0);
}

// -- 11. Expired waiver does not suppress gate -----------------------

#[test]
fn declare_waiver_expired_does_not_suppress() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	setup_failing_gate(&db);

	// Declare waiver with expiry in the past.
	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "temporary exception",
		"--expires-at", "2023-01-01T00:00:00Z",
	]);
	assert_eq!(output.status.code(), Some(0));

	// Gate should still FAIL — expired waiver is ignored.
	let gate_out = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_out.status.code(), Some(1), "expired waiver must not suppress");

	let result = parse_json(&gate_out);
	assert_eq!(result["obligations"][0]["effective_verdict"], "FAIL");
	assert!(result["obligations"][0]["waiver_basis"].is_null());
}

// -- 12. Exact JSON shape --------------------------------------------

#[test]
fn declare_waiver_json_shape() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&base_waiver_args(db_str));
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	let obj = result.as_object().unwrap();
	let keys: Vec<&String> = obj.keys().collect();
	assert!(keys.contains(&&"declaration_uid".to_string()));
	assert!(keys.contains(&&"kind".to_string()));
	assert!(keys.contains(&&"req_id".to_string()));
	assert!(keys.contains(&&"requirement_version".to_string()));
	assert!(keys.contains(&&"obligation_id".to_string()));
	assert!(keys.contains(&&"inserted".to_string()));
	assert_eq!(keys.len(), 6, "exactly 6 keys, got: {:?}", keys);

	assert_eq!(result["kind"], "waiver");
}

// -- 13. Empty --reason => usage error -------------------------------

#[test]
fn declare_waiver_empty_reason() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "",
	]);
	assert_eq!(output.status.code(), Some(1), "empty --reason => usage error");
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("non-empty"), "stderr: {}", stderr);
}
