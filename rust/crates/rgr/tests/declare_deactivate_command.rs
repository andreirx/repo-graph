//! Deterministic tests for `declare deactivate` (Rust-36).
//!
//! Test matrix:
//!   1. Usage error (wrong arg count)
//!   2. Missing DB => storage error, exit 2
//!   3. Deactivate existing declaration => deactivated: true
//!   4. Nonexistent UID => deactivated: false (idempotent)
//!   5. Deactivated boundary no longer affects violations
//!   6. Deactivated requirement no longer affects gate
//!   7. Deactivated waiver no longer suppresses gate
//!   8. Exact JSON shape

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

/// Declare a boundary and return its UID.
fn declare_boundary(db_str: &str) -> String {
	let out = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/adapters",
		"--forbids", "src/core",
	]);
	assert_eq!(out.status.code(), Some(0));
	let result = parse_json(&out);
	result["declaration_uid"].as_str().unwrap().to_string()
}

/// Declare a requirement and return its UID.
fn declare_requirement(db_str: &str) -> String {
	let out = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "adapters must not depend on core",
		"--target", "src/adapters",
	]);
	assert_eq!(out.status.code(), Some(0));
	let result = parse_json(&out);
	result["declaration_uid"].as_str().unwrap().to_string()
}

/// Declare a waiver and return its UID.
fn declare_waiver(db_str: &str) -> String {
	let out = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "known dependency",
	]);
	assert_eq!(out.status.code(), Some(0));
	let result = parse_json(&out);
	result["declaration_uid"].as_str().unwrap().to_string()
}

// -- 1. Usage error --------------------------------------------------

#[test]
fn declare_deactivate_usage_error() {
	let output = run_cmd(&["declare", "deactivate"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());

	// Too many args.
	let output2 = run_cmd(&["declare", "deactivate", "a", "b", "c"]);
	assert_eq!(output2.status.code(), Some(1));
}

// -- 2. Missing DB ---------------------------------------------------

#[test]
fn declare_deactivate_missing_db() {
	let output = run_cmd(&["declare", "deactivate", "/nonexistent/path.db", "some-uid"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
}

// -- 3. Deactivate existing declaration ------------------------------

#[test]
fn declare_deactivate_existing() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let uid = declare_boundary(db_str);

	let output = run_cmd(&["declare", "deactivate", db_str, &uid]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	assert_eq!(result["declaration_uid"], uid);
	assert_eq!(result["deactivated"], true);
}

// -- 4. Nonexistent UID => deactivated: false ------------------------

#[test]
fn declare_deactivate_nonexistent() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["declare", "deactivate", db_str, "nonexistent-uid-12345"]);
	assert_eq!(output.status.code(), Some(0), "nonexistent => exit 0 (idempotent)");

	let result = parse_json(&output);
	assert_eq!(result["deactivated"], false);
}

// -- 5. Deactivated boundary no longer affects violations ------------

#[test]
fn declare_deactivate_boundary_removes_from_violations() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let uid = declare_boundary(db_str);

	// Before deactivation: violations should exist.
	let viol_before = run_cmd(&["violations", db_str, "r1"]);
	let before = parse_json(&viol_before);
	assert!(before["count"].as_i64().unwrap() > 0, "should have violations before deactivation");

	// Deactivate.
	let deact = run_cmd(&["declare", "deactivate", db_str, &uid]);
	assert_eq!(deact.status.code(), Some(0));

	// After deactivation: no violations (no active boundary declarations).
	let viol_after = run_cmd(&["violations", db_str, "r1"]);
	let after = parse_json(&viol_after);
	assert_eq!(after["count"], 0, "no violations after boundary deactivated");
}

// -- 6. Deactivated requirement no longer affects gate ----------------

#[test]
fn declare_deactivate_requirement_removes_from_gate() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	declare_boundary(db_str);
	let req_uid = declare_requirement(db_str);

	// Before: gate should FAIL (adapters imports core).
	let gate_before = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_before.status.code(), Some(1), "gate FAIL before deactivation");

	// Deactivate requirement.
	let deact = run_cmd(&["declare", "deactivate", db_str, &req_uid]);
	assert_eq!(deact.status.code(), Some(0));

	// After: gate should pass (no active requirements).
	let gate_after = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_after.status.code(), Some(0), "gate pass after requirement deactivated");
	let after = parse_json(&gate_after);
	assert_eq!(after["obligations"].as_array().unwrap().len(), 0);
}

// -- 7. Deactivated waiver no longer suppresses gate -----------------

#[test]
fn declare_deactivate_waiver_restores_gate_failure() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	declare_boundary(db_str);
	declare_requirement(db_str);
	let waiver_uid = declare_waiver(db_str);

	// With waiver: gate should pass (WAIVED).
	let gate_waived = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_waived.status.code(), Some(0), "gate pass with waiver");
	let waived = parse_json(&gate_waived);
	assert_eq!(waived["obligations"][0]["effective_verdict"], "WAIVED");

	// Deactivate waiver.
	let deact = run_cmd(&["declare", "deactivate", db_str, &waiver_uid]);
	assert_eq!(deact.status.code(), Some(0));
	assert_eq!(parse_json(&deact)["deactivated"], true);

	// Without waiver: gate should FAIL again.
	let gate_after = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_after.status.code(), Some(1), "gate FAIL after waiver deactivated");
	let after = parse_json(&gate_after);
	assert_eq!(after["obligations"][0]["effective_verdict"], "FAIL");
	assert!(after["obligations"][0]["waiver_basis"].is_null());
}

// -- 8. Exact JSON shape ---------------------------------------------

#[test]
fn declare_deactivate_json_shape() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let uid = declare_boundary(db_str);

	let output = run_cmd(&["declare", "deactivate", db_str, &uid]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	let obj = result.as_object().unwrap();
	let keys: Vec<&String> = obj.keys().collect();
	assert!(keys.contains(&&"declaration_uid".to_string()));
	assert!(keys.contains(&&"deactivated".to_string()));
	assert_eq!(keys.len(), 2, "exactly 2 keys, got: {:?}", keys);

	assert_eq!(result["declaration_uid"], uid);
	assert_eq!(result["deactivated"], true);
}
