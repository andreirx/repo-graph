//! Deterministic tests for `declare supersede requirement` (Rust-39).
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. Missing DB => exit 2
//!   3. Missing required flags => exit 1
//!   4. Invalid threshold => exit 1
//!   5. Invalid operator => exit 1
//!   6. Flag-looking value => exit 1
//!   7. Old UID missing => exit 2
//!   8. Old UID inactive => exit 2
//!   9. Old UID wrong kind => exit 2
//!  10. Old requirement malformed value_json => exit 2
//!  11. Success: new UID, old deactivated
//!  12. Gate sees replacement obligation, not old
//!  13. Exact JSON shape

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"))
}

/// Fixture: src/adapters/store.ts imports src/core/service.ts.
/// Boundary: adapters --forbids--> core.
/// So arch_violations targeting src/adapters will FAIL.
/// arch_violations targeting src/core will PASS (core doesn't import adapters).
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

/// Declare a requirement targeting src/core (PASS — core doesn't import adapters).
fn declare_requirement_core(db_str: &str) -> String {
	let out = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "core must not depend on adapters",
		"--target", "src/core",
	]);
	assert_eq!(out.status.code(), Some(0));
	parse_json(&out)["declaration_uid"].as_str().unwrap().to_string()
}

fn declare_boundary_decl(db_str: &str) -> String {
	let out = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/adapters",
		"--forbids", "src/core",
	]);
	assert_eq!(out.status.code(), Some(0));
	parse_json(&out)["declaration_uid"].as_str().unwrap().to_string()
}

// -- 1. Usage error --------------------------------------------------

#[test]
fn supersede_requirement_usage_error() {
	let output = run_cmd(&["declare", "supersede", "requirement"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
}

// -- 2. Missing DB ---------------------------------------------------

#[test]
fn supersede_requirement_missing_db() {
	let output = run_cmd(&[
		"declare", "supersede", "requirement",
		"/nonexistent/path.db", "some-uid",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
}

// -- 3. Missing required flags ---------------------------------------

#[test]
fn supersede_requirement_missing_obligation_id() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let req_uid = declare_requirement_core(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &req_uid,
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--obligation-id"), "stderr: {}", stderr);
}

#[test]
fn supersede_requirement_missing_method() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let req_uid = declare_requirement_core(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &req_uid,
		"--obligation-id", "obl-1",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--method"), "stderr: {}", stderr);
}

#[test]
fn supersede_requirement_missing_obligation() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let req_uid = declare_requirement_core(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &req_uid,
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--obligation"), "stderr: {}", stderr);
}

// -- 4. Invalid threshold --------------------------------------------

#[test]
fn supersede_requirement_invalid_threshold() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let req_uid = declare_requirement_core(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &req_uid,
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
		"--threshold", "abc",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("number"), "stderr: {}", stderr);
}

// -- 5. Invalid operator ---------------------------------------------

#[test]
fn supersede_requirement_invalid_operator() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let req_uid = declare_requirement_core(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &req_uid,
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
		"--operator", "!=",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("!="), "stderr: {}", stderr);
}

// -- 6. Flag-looking value -------------------------------------------

#[test]
fn supersede_requirement_flag_as_value() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let req_uid = declare_requirement_core(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &req_uid,
		"--obligation-id", "--method",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("requires a"), "stderr: {}", stderr);
}

// -- 7. Old UID missing ----------------------------------------------

#[test]
fn supersede_requirement_old_missing() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, "nonexistent-uid",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 8. Old UID inactive ---------------------------------------------

#[test]
fn supersede_requirement_old_inactive() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let req_uid = declare_requirement_core(db_str);

	run_cmd(&["declare", "deactivate", db_str, &req_uid]);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &req_uid,
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("inactive"), "stderr: {}", stderr);
}

// -- 9. Old UID wrong kind -------------------------------------------

#[test]
fn supersede_requirement_wrong_kind() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let bd_uid = declare_boundary_decl(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &bd_uid,
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("expected 'requirement'"), "stderr: {}", stderr);
}

// -- 10. Old requirement malformed value_json => exit 2 --------------

#[test]
fn supersede_requirement_malformed_old_value() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	// Insert a requirement with bad value_json directly.
	let conn = rusqlite::Connection::open(&db).unwrap();
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES ('bad-req', 'r1', 'r1:requirement:REQ-BAD:1', 'requirement', 'not valid json', '2024-01-01T00:00:00Z', 1)",
		[],
	).unwrap();

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, "bad-req",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("malformed"), "stderr: {}", stderr);
}

// -- 11. Success: new UID, old deactivated ---------------------------

#[test]
fn supersede_requirement_success() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let old_uid = declare_requirement_core(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &old_uid,
		"--obligation-id", "obl-2",
		"--method", "arch_violations",
		"--obligation", "updated obligation text",
		"--target", "src/adapters",
	]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	assert_eq!(result["old_declaration_uid"], old_uid);
	assert_ne!(result["new_declaration_uid"].as_str().unwrap(), old_uid);
	assert_eq!(result["kind"], "requirement");
	assert_eq!(result["req_id"], "REQ-001");
	assert_eq!(result["version"], 1);
	assert_eq!(result["superseded"], true);
}

// -- 12. Gate sees replacement obligation ----------------------------

#[test]
fn supersede_requirement_gate_sees_replacement() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	// Two boundaries:
	//   core --forbids--> adapters (core doesn't import adapters → 0 violations)
	//   adapters --forbids--> core (store.ts imports core → 1+ violations)
	run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/core",
		"--forbids", "src/adapters",
	]);
	run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/adapters",
		"--forbids", "src/core",
	]);

	// Old requirement targets src/core (PASS — core doesn't import adapters).
	let old_uid = declare_requirement_core(db_str);

	let gate_before = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_before.status.code(), Some(0), "gate PASS before supersede");
	let before = parse_json(&gate_before);
	assert_eq!(before["obligations"][0]["computed_verdict"], "PASS");

	// Supersede requirement to target src/adapters (FAIL — store.ts imports core).
	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &old_uid,
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "adapters must not depend on core",
		"--target", "src/adapters",
	]);
	assert_eq!(output.status.code(), Some(0));

	let gate_after = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_after.status.code(), Some(1), "gate FAIL after supersede");
	let after = parse_json(&gate_after);
	assert_eq!(after["obligations"][0]["computed_verdict"], "FAIL");
	assert_eq!(after["obligations"][0]["target"], "src/adapters");
}

// -- 13. Exact JSON shape --------------------------------------------

#[test]
fn supersede_requirement_json_shape() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let old_uid = declare_requirement_core(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "requirement", db_str, &old_uid,
		"--obligation-id", "obl-2",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	let obj = result.as_object().unwrap();
	let keys: Vec<&String> = obj.keys().collect();
	assert!(keys.contains(&&"old_declaration_uid".to_string()));
	assert!(keys.contains(&&"new_declaration_uid".to_string()));
	assert!(keys.contains(&&"kind".to_string()));
	assert!(keys.contains(&&"req_id".to_string()));
	assert!(keys.contains(&&"version".to_string()));
	assert!(keys.contains(&&"superseded".to_string()));
	assert_eq!(keys.len(), 6, "exactly 6 keys, got: {:?}", keys);
}
