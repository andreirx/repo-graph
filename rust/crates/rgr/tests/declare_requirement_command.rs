//! Deterministic tests for `declare requirement` (Rust-34).
//!
//! Test matrix:
//!   1. Missing required flags => usage error
//!   2. Invalid --version (non-integer) => usage error
//!   3. Invalid --threshold (non-number) => usage error
//!   4. Invalid --operator => usage error
//!   5. Repeated flag => usage error
//!   6. Flag-looking value => usage error
//!   7. Insert success
//!   8. Idempotent repeated insert
//!   9. Obligation text does not affect identity
//!  10. Inserted requirement visible to gate
//!  11. Exact JSON shape
//!  12. Missing DB => storage error, exit 2

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"))
}

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

/// Helper to insert a boundary declaration (needed for gate arch_violations tests).
fn insert_boundary(db_path: &std::path::Path, repo_uid: &str, module: &str, forbids: &str) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	let uid = format!("bd-{}-{}", module.replace('/', "-"), forbids.replace('/', "-"));
	let stable_key = format!("{}:{}:MODULE", repo_uid, module);
	let value_json = format!(r#"{{"forbids":"{}"}}"#, forbids);
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES (?, ?, ?, 'boundary', ?, '2024-01-01T00:00:00Z', 1)",
		rusqlite::params![uid, repo_uid, stable_key, value_json],
	).unwrap();
}

// Base args for a valid declare requirement command.
fn base_args<'a>(db_str: &'a str) -> Vec<&'a str> {
	vec![
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "core must not depend on adapters",
		"--target", "src/core",
	]
}

// -- 1. Missing required flags => usage error ------------------------

#[test]
fn declare_requirement_missing_version() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--version"), "stderr: {}", stderr);
}

#[test]
fn declare_requirement_missing_obligation_id() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--obligation-id"), "stderr: {}", stderr);
}

#[test]
fn declare_requirement_missing_method() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--method"), "stderr: {}", stderr);
}

#[test]
fn declare_requirement_missing_obligation() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--obligation"), "stderr: {}", stderr);
}

// -- 2. Invalid --version --------------------------------------------

#[test]
fn declare_requirement_invalid_version() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "abc",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("integer"), "stderr: {}", stderr);
}

// -- 3. Invalid --threshold ------------------------------------------

#[test]
fn declare_requirement_invalid_threshold() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let mut args = base_args(db_str);
	args.extend_from_slice(&["--threshold", "not-a-number"]);
	let output = run_cmd(&args);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("number"), "stderr: {}", stderr);
}

// -- 4. Invalid --operator -------------------------------------------

#[test]
fn declare_requirement_invalid_operator() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let mut args = base_args(db_str);
	args.extend_from_slice(&["--operator", "!="]);
	let output = run_cmd(&args);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("!="), "stderr: {}", stderr);
}

// -- 5. Repeated flag ------------------------------------------------

#[test]
fn declare_requirement_repeated_flag() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1", "--version", "2",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("more than once"), "stderr: {}", stderr);
}

// -- 6. Flag-looking value -------------------------------------------

#[test]
fn declare_requirement_flag_as_value() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "--method",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("requires a value"), "stderr: {}", stderr);
}

// -- 7. Insert success -----------------------------------------------

#[test]
fn declare_requirement_success() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&base_args(db_str));
	assert_eq!(output.status.code(), Some(0), "success => exit 0");

	let result = parse_json(&output);
	assert!(result["declaration_uid"].is_string());
	assert_eq!(result["kind"], "requirement");
	assert_eq!(result["req_id"], "REQ-001");
	assert_eq!(result["version"], 1);
	assert_eq!(result["inserted"], true);
}

// -- 8. Idempotent repeated insert -----------------------------------

#[test]
fn declare_requirement_idempotent() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let first = run_cmd(&base_args(db_str));
	assert_eq!(first.status.code(), Some(0));
	let first_result = parse_json(&first);
	assert_eq!(first_result["inserted"], true);

	let second = run_cmd(&base_args(db_str));
	assert_eq!(second.status.code(), Some(0));
	let second_result = parse_json(&second);
	assert_eq!(second_result["inserted"], false);
	assert_eq!(
		first_result["declaration_uid"],
		second_result["declaration_uid"],
	);
}

// -- 9. Obligation text does not affect identity ---------------------

#[test]
fn declare_requirement_obligation_text_does_not_affect_identity() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let first = run_cmd(&base_args(db_str));
	assert_eq!(first.status.code(), Some(0));
	let first_result = parse_json(&first);
	assert_eq!(first_result["inserted"], true);

	// Same (repo, req_id, version) but different obligation text.
	let output = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "completely different wording",
		"--target", "src/core",
	]);
	assert_eq!(output.status.code(), Some(0));
	let second_result = parse_json(&output);
	assert_eq!(second_result["inserted"], false, "obligation text change must not create new declaration");
	assert_eq!(
		first_result["declaration_uid"],
		second_result["declaration_uid"],
	);
}

// -- 10. Inserted requirement visible to gate ------------------------

#[test]
fn declare_requirement_visible_to_gate() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	// Declare a boundary first (needed for arch_violations to have data).
	insert_boundary(&db, "r1", "src/core", "src/adapters");

	// Declare requirement with arch_violations obligation targeting src/core.
	let declare_out = run_cmd(&base_args(db_str));
	assert_eq!(declare_out.status.code(), Some(0));

	// Run gate — should pick up the requirement.
	let gate_out = run_cmd(&["gate", db_str, "r1"]);
	// src/core does NOT import from src/adapters, so violation_count=0 → PASS.
	assert_eq!(gate_out.status.code(), Some(0), "gate should pass");

	let gate_result = parse_json(&gate_out);
	let obls = gate_result["obligations"].as_array().unwrap();
	assert_eq!(obls.len(), 1);
	assert_eq!(obls[0]["req_id"], "REQ-001");
	assert_eq!(obls[0]["method"], "arch_violations");
	assert_eq!(obls[0]["computed_verdict"], "PASS");
}

// -- 11. Exact JSON shape --------------------------------------------

#[test]
fn declare_requirement_json_shape() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&base_args(db_str));
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	let obj = result.as_object().unwrap();
	let keys: Vec<&String> = obj.keys().collect();
	assert!(keys.contains(&&"declaration_uid".to_string()));
	assert!(keys.contains(&&"kind".to_string()));
	assert!(keys.contains(&&"req_id".to_string()));
	assert!(keys.contains(&&"version".to_string()));
	assert!(keys.contains(&&"inserted".to_string()));
	assert_eq!(keys.len(), 5, "exactly 5 keys, got: {:?}", keys);

	assert_eq!(result["kind"], "requirement");
	assert!(result["declaration_uid"].as_str().unwrap().len() > 0);
	assert!(result["version"].is_number());
}

// -- 12. Missing DB => storage error, exit 2 -------------------------

#[test]
fn declare_requirement_missing_db() {
	let output = run_cmd(&[
		"declare", "requirement", "/nonexistent/path.db", "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "test",
	]);
	assert_eq!(output.status.code(), Some(2), "missing DB => exit 2");
	assert!(output.stdout.is_empty(), "no JSON on stdout for storage error");
}
