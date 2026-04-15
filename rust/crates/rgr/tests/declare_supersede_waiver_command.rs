//! Deterministic tests for `declare supersede waiver` (Rust-40).
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB => exit 2
//!   3. Missing --reason => exit 1
//!   4. Repeated --reason => exit 1
//!   5. Flag-looking value => exit 1
//!   6. Old UID missing => exit 2
//!   7. Old UID inactive => exit 2
//!   8. Old UID wrong kind => exit 2
//!   9. Old waiver malformed value_json => exit 2
//!  10. Success: new UID, old deactivated
//!  11. Gate sees replacement waiver reason
//!  12. Supersede to expired expiry restores gate failure
//!  13. Exact JSON shape

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

/// Set up the full failing-gate fixture:
/// boundary + requirement producing FAIL, then waiver producing WAIVED.
/// Returns (boundary_uid, requirement_uid, waiver_uid).
fn setup_waived_gate(db_str: &str) -> (String, String, String) {
	let b = run_cmd(&[
		"declare", "boundary", db_str, "r1", "src/adapters",
		"--forbids", "src/core",
	]);
	assert_eq!(b.status.code(), Some(0));
	let b_uid = parse_json(&b)["declaration_uid"].as_str().unwrap().to_string();

	let r = run_cmd(&[
		"declare", "requirement", db_str, "r1", "REQ-001",
		"--version", "1",
		"--obligation-id", "obl-1",
		"--method", "arch_violations",
		"--obligation", "adapters must not depend on core",
		"--target", "src/adapters",
	]);
	assert_eq!(r.status.code(), Some(0));
	let r_uid = parse_json(&r)["declaration_uid"].as_str().unwrap().to_string();

	let w = run_cmd(&[
		"declare", "waiver", db_str, "r1", "REQ-001",
		"--requirement-version", "1",
		"--obligation-id", "obl-1",
		"--reason", "old reason",
	]);
	assert_eq!(w.status.code(), Some(0));
	let w_uid = parse_json(&w)["declaration_uid"].as_str().unwrap().to_string();

	(b_uid, r_uid, w_uid)
}

// -- 1. Usage error --------------------------------------------------

#[test]
fn supersede_waiver_usage_error() {
	let output = run_cmd(&["declare", "supersede", "waiver"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
}

// -- 2. Missing DB ---------------------------------------------------

#[test]
fn supersede_waiver_missing_db() {
	let output = run_cmd(&[
		"declare", "supersede", "waiver",
		"/nonexistent/path.db", "some-uid",
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
}

// -- 3. Missing --reason ---------------------------------------------

#[test]
fn supersede_waiver_missing_reason() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (_, _, w_uid) = setup_waived_gate(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &w_uid,
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("--reason"), "stderr: {}", stderr);
}

// -- 4. Repeated --reason --------------------------------------------

#[test]
fn supersede_waiver_repeated_reason() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (_, _, w_uid) = setup_waived_gate(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &w_uid,
		"--reason", "first", "--reason", "second",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("more than once"), "stderr: {}", stderr);
}

// -- 5. Flag-looking value -------------------------------------------

#[test]
fn supersede_waiver_flag_as_value() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (_, _, w_uid) = setup_waived_gate(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &w_uid,
		"--reason", "--expires-at",
	]);
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("requires a"), "stderr: {}", stderr);
}

// -- 6. Old UID missing ----------------------------------------------

#[test]
fn supersede_waiver_old_missing() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, "nonexistent-uid",
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// -- 7. Old UID inactive ---------------------------------------------

#[test]
fn supersede_waiver_old_inactive() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (_, _, w_uid) = setup_waived_gate(db_str);

	run_cmd(&["declare", "deactivate", db_str, &w_uid]);

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &w_uid,
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("inactive"), "stderr: {}", stderr);
}

// -- 8. Old UID wrong kind -------------------------------------------

#[test]
fn supersede_waiver_wrong_kind() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (b_uid, _, _) = setup_waived_gate(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &b_uid,
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("expected 'waiver'"), "stderr: {}", stderr);
}

// -- 9. Old waiver malformed value_json ------------------------------

#[test]
fn supersede_waiver_malformed_old_value() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();

	let conn = rusqlite::Connection::open(&db).unwrap();
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES ('bad-waiver', 'r1', 'r1:waiver:REQ-BAD#obl-1', 'waiver', 'not valid json', '2024-01-01T00:00:00Z', 1)",
		[],
	).unwrap();

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, "bad-waiver",
		"--reason", "test",
	]);
	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("malformed"), "stderr: {}", stderr);
}

// -- 10. Success: new UID, old deactivated ---------------------------

#[test]
fn supersede_waiver_success() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (_, _, old_uid) = setup_waived_gate(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &old_uid,
		"--reason", "updated reason",
		"--rationale-category", "tech_debt",
	]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	assert_eq!(result["old_declaration_uid"], old_uid);
	assert_ne!(result["new_declaration_uid"].as_str().unwrap(), old_uid.as_str());
	assert_eq!(result["kind"], "waiver");
	assert_eq!(result["req_id"], "REQ-001");
	assert_eq!(result["requirement_version"], 1);
	assert_eq!(result["obligation_id"], "obl-1");
	assert_eq!(result["superseded"], true);
}

// -- 11. Gate sees replacement waiver reason -------------------------

#[test]
fn supersede_waiver_gate_sees_new_reason() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (_, _, old_uid) = setup_waived_gate(db_str);

	// Before supersede: gate shows old reason.
	let gate_before = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_before.status.code(), Some(0));
	let before = parse_json(&gate_before);
	assert_eq!(before["obligations"][0]["effective_verdict"], "WAIVED");
	assert_eq!(before["obligations"][0]["waiver_basis"]["reason"], "old reason");

	// Supersede with new reason.
	let sup = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &old_uid,
		"--reason", "new reason after review",
	]);
	assert_eq!(sup.status.code(), Some(0));

	// After supersede: gate shows new reason.
	let gate_after = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_after.status.code(), Some(0));
	let after = parse_json(&gate_after);
	assert_eq!(after["obligations"][0]["effective_verdict"], "WAIVED");
	assert_eq!(after["obligations"][0]["waiver_basis"]["reason"], "new reason after review");
}

// -- 12. Supersede to expired expiry restores gate failure -----------

#[test]
fn supersede_waiver_expired_restores_gate_failure() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (_, _, old_uid) = setup_waived_gate(db_str);

	// Before: gate passes (WAIVED).
	let gate_before = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_before.status.code(), Some(0));

	// Supersede waiver with expired expiry.
	let sup = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &old_uid,
		"--reason", "expired exception",
		"--expires-at", "2023-01-01T00:00:00Z",
	]);
	assert_eq!(sup.status.code(), Some(0));

	// After: gate fails (expired waiver ignored).
	let gate_after = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(gate_after.status.code(), Some(1), "expired waiver must not suppress gate");
	let after = parse_json(&gate_after);
	assert_eq!(after["obligations"][0]["effective_verdict"], "FAIL");
	assert!(after["obligations"][0]["waiver_basis"].is_null());
}

// -- 13. Exact JSON shape --------------------------------------------

#[test]
fn supersede_waiver_json_shape() {
	let (_r, _d, db) = build_db();
	let db_str = db.to_str().unwrap();
	let (_, _, old_uid) = setup_waived_gate(db_str);

	let output = run_cmd(&[
		"declare", "supersede", "waiver", db_str, &old_uid,
		"--reason", "updated",
	]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	let obj = result.as_object().unwrap();
	let keys: Vec<&String> = obj.keys().collect();
	assert!(keys.contains(&&"old_declaration_uid".to_string()));
	assert!(keys.contains(&&"new_declaration_uid".to_string()));
	assert!(keys.contains(&&"kind".to_string()));
	assert!(keys.contains(&&"req_id".to_string()));
	assert!(keys.contains(&&"requirement_version".to_string()));
	assert!(keys.contains(&&"obligation_id".to_string()));
	assert!(keys.contains(&&"superseded".to_string()));
	assert_eq!(keys.len(), 7, "exactly 7 keys, got: {:?}", keys);
}
