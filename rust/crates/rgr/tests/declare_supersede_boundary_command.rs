//! Deterministic tests for `declare supersede boundary` (Rust-38).
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. Missing DB => exit 2
//!   3. Missing --forbids => exit 1
//!   4. Repeated --forbids => exit 1
//!   5. Flag-looking value => exit 1
//!   6. Old UID missing => exit 2
//!   7. Old UID inactive => exit 2
//!   8. Old UID wrong kind => exit 2
//!   9. Success: new UID, old deactivated
//!  10. Violations sees replacement rule, not old rule
//!  11. Exact JSON shape

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
}

/// Fixture: src/adapters/store.ts imports src/core/service.ts.
/// src/core does NOT import src/adapters.
/// src/adapters does NOT import src/util.
fn build_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src/core")).unwrap();
	std::fs::create_dir_all(root.join("src/adapters")).unwrap();
	std::fs::create_dir_all(root.join("src/util")).unwrap();
	std::fs::write(root.join("package.json"), r#"{"dependencies":{}}"#).unwrap();
	std::fs::write(
		root.join("src/core/service.ts"),
		"export function serve() {}\n",
	).unwrap();
	std::fs::write(
		root.join("src/adapters/store.ts"),
		"import { serve } from \"../core/service\";\nexport function store() { serve(); }\n",
	).unwrap();
	std::fs::write(
		root.join("src/util/helper.ts"),
		"export function helper() {}\n",
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

/// Declare a boundary and return its UID.
fn declare_boundary(db_str: &str, module: &str, forbids: &str) -> String {
	let out = run_cmd(&[
		"declare", "boundary", db_str, "r1", module,
		"--forbids", forbids,
	]);
	assert_eq!(out.status.code(), Some(0));
	parse_json(&out)["declaration_uid"].as_str().unwrap().to_string()
}

/// Declare a requirement and return its UID.
fn declare_requirement(db_str: &str) -> String {
	let out = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "adapters clean",
		"--target", "src/adapters",
	]);
	assert_eq!(out.status.code(), Some(0));
	parse_json(&out)["declaration_uid"].as_str().unwrap().to_string()
}

// -- 1. Usage error --------------------------------------------------

#[test]
fn supersede_boundary_usage_error() {
	// Missing positional args.
	let output = run_cmd(&["declare", "supersede", "boundary"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());

	// Missing supersede kind.
	let output2 = run_cmd(&["declare", "supersede"]);
	assert_eq!(output2.status.code(), Some(1));
}

// -- 2. Missing DB ---------------------------------------------------

#[test]
fn supersede_boundary_missing_db() {
	let output = run_cmd(&[
		"declare", "supersede", "boundary",
		"/nonexistent/path.db", "some-uid",
		"--forbids", "src/core",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
}

// -- 3. Missing --forbids --------------------------------------------

#[test]
fn supersede_boundary_missing_forbids() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let uid = declare_boundary(db_str, "src/adapters", "src/core");

	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, &uid,
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--forbids"), "stderr: {}", stderr);
}

// -- 4. Repeated --forbids -------------------------------------------

#[test]
fn supersede_boundary_repeated_forbids() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let uid = declare_boundary(db_str, "src/adapters", "src/core");

	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, &uid,
		"--forbids", "src/core", "--forbids", "src/util",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("more than once"), "stderr: {}", stderr);
}

// -- 5. Flag-looking value -------------------------------------------

#[test]
fn supersede_boundary_flag_as_value() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let uid = declare_boundary(db_str, "src/adapters", "src/core");

	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, &uid,
		"--forbids", "--reason",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("non-empty"), "stderr: {}", stderr);
}

// -- 6. Old UID missing ----------------------------------------------

#[test]
fn supersede_boundary_old_missing() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, "nonexistent-uid",
		"--forbids", "src/core",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 7. Old UID inactive ---------------------------------------------

#[test]
fn supersede_boundary_old_inactive() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let uid = declare_boundary(db_str, "src/adapters", "src/core");

	// Deactivate it first.
	let deact = run_cmd(&["declare", "deactivate", db_str, &uid]);
	assert_eq!(deact.status.code(), Some(0));

	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, &uid,
		"--forbids", "src/util",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("inactive"), "stderr: {}", stderr);
}

// -- 8. Old UID wrong kind -------------------------------------------

#[test]
fn supersede_boundary_wrong_kind() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	// Declare a requirement, then try to supersede it as a boundary.
	let req_uid = declare_requirement(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, &req_uid,
		"--forbids", "src/core",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("expected 'boundary'"), "stderr: {}", stderr);
}

// -- 9. Success: new UID, old deactivated ----------------------------

#[test]
fn supersede_boundary_success() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let old_uid = declare_boundary(db_str, "src/adapters", "src/core");

	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, &old_uid,
		"--forbids", "src/util", "--reason", "policy update",
	]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	assert_eq!(result["old_declaration_uid"], old_uid);
	assert_ne!(result["new_declaration_uid"], old_uid);
	assert_eq!(result["kind"], "boundary");
	assert_eq!(result["target"], "src/adapters");
	assert_eq!(result["forbids"], "src/util");
	assert_eq!(result["superseded"], true);
}

// -- 10. Violations sees replacement rule ----------------------------

#[test]
fn supersede_boundary_violations_see_replacement() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	// Old boundary: adapters --forbids--> core.
	// store.ts imports core/service.ts → violations > 0.
	let old_uid = declare_boundary(db_str, "src/adapters", "src/core");

	let viol_before = run_cmd(&["violations", db_str, "r1"]);
	let before = parse_json(&viol_before);
	assert!(
		before["count"].as_i64().unwrap() > 0,
		"should have violations before supersede",
	);

	// Supersede to: adapters --forbids--> util.
	// store.ts does NOT import from util → violations = 0.
	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, &old_uid,
		"--forbids", "src/util",
	]);
	assert_eq!(output.status.code(), Some(0));

	let viol_after = run_cmd(&["violations", db_str, "r1"]);
	let after = parse_json(&viol_after);
	assert_eq!(
		after["count"], 0,
		"no violations after supersede to non-imported module",
	);
}

// -- 11. Exact JSON shape --------------------------------------------

#[test]
fn supersede_boundary_json_shape() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let old_uid = declare_boundary(db_str, "src/adapters", "src/core");

	let output = run_cmd(&[
		"declare", "supersede", "boundary", db_str, &old_uid,
		"--forbids", "src/util",
	]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	let obj = result.as_object().unwrap();
	let keys: Vec<&String> = obj.keys().collect();
	assert!(keys.contains(&&"old_declaration_uid".to_string()));
	assert!(keys.contains(&&"new_declaration_uid".to_string()));
	assert!(keys.contains(&&"kind".to_string()));
	assert!(keys.contains(&&"target".to_string()));
	assert!(keys.contains(&&"forbids".to_string()));
	assert!(keys.contains(&&"superseded".to_string()));
	assert_eq!(keys.len(), 6, "exactly 6 keys, got: {:?}", keys);
}
