//! Deterministic tests for the `gate` command.
//!
//! Test matrix:
//!   1. No requirement declarations => pass, empty obligations, exit 0
//!   2. arch_violations obligation with no matching boundaries => MISSING_EVIDENCE, exit 2
//!   3. arch_violations obligation passing => PASS, exit 0
//!   4. arch_violations obligation failing => FAIL, exit 1
//!   5. Unsupported method => UNSUPPORTED, exit 2
//!   6. Exact JSON contract (toolchain, computed/effective verdict, gate counts)
//!   7. Malformed active requirement => command error, exit 2
//!   8. Multiple obligations with mixed verdicts => fail wins
//!   9. FAIL + active waiver => WAIVED, exit 0 (Rust-25)
//!  10. FAIL + waiver with wrong obligation_id => no suppression, exit 1 (Rust-25)
//!  11. FAIL + expired waiver => no suppression, exit 1 (Rust-25)
//!  12. PASS + active waiver => remains PASS, waiver_basis null (Rust-25)
//!  13. Malformed active waiver => command error, exit 2 (Rust-25)
//!  14. Waiver missing required field => command error, exit 2 (Rust-25)
//!  15. Default mode: UNSUPPORTED without FAIL => incomplete, exit 2 (Rust-26)
//!  16. Strict mode: UNSUPPORTED without FAIL => fail, exit 1 (Rust-26)
//!  17. Advisory mode: UNSUPPORTED without FAIL => pass, exit 0 (Rust-26)
//!  18. Strict mode: WAIVED obligation remains non-failing (Rust-26)
//!  19. --strict + --advisory => usage error, exit 1 (Rust-26)
//!  20. Exact JSON gate.mode field reflects selected mode (Rust-26)
//!
//! Note: storage-read failures during arch_violations evaluation are
//! not testable at the CLI integration level because StorageConnection::open()
//! re-runs migrations that repair any schema damage. The error propagation
//! is guaranteed by Rust's Result + ? type system, and the general
//! command-abort pattern is proven by gate_malformed_requirement_aborts.
//! A targeted unit test in gate.rs covers the specific propagation path.

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"))
}

/// Build a fixture with cross-module imports for gate testing.
/// Same structure as violations tests: src/core, src/adapters, src/util.
fn build_gate_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();
	std::fs::create_dir_all(root.join("src/core")).unwrap();
	std::fs::create_dir_all(root.join("src/util")).unwrap();
	std::fs::create_dir_all(root.join("src/adapters")).unwrap();
	std::fs::write(
		root.join("package.json"),
		r#"{"dependencies":{}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("src/core/service.ts"),
		"import { helper } from \"../util/helper\";\nexport function serve() { helper(); }\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/util/helper.ts"),
		"export function helper() {}\n",
	)
	.unwrap();
	std::fs::write(
		root.join("src/adapters/store.ts"),
		"import { serve } from \"../core/service\";\nexport function store() { serve(); }\n",
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();
	assert_eq!(result.files_total, 3);

	(repo_dir, db_dir, db_path)
}

/// Insert a requirement declaration with verification obligations.
fn insert_requirement(
	db_path: &std::path::Path,
	uid: &str,
	repo_uid: &str,
	req_id: &str,
	version: i64,
	obligations_json: &str,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	let value_json = format!(
		r#"{{"req_id":"{}","version":{},"verification":{}}}"#,
		req_id, version, obligations_json
	);
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES (?, ?, ?, 'requirement', ?, '2024-01-01T00:00:00Z', 1)",
		rusqlite::params![uid, repo_uid, format!("{}:REQ:REQUIREMENT", repo_uid), value_json],
	)
	.unwrap();
}

/// Insert a boundary declaration.
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
	)
	.unwrap();
}

fn run_cmd(args: &[&str]) -> std::process::Output {
	Command::new(binary_path())
		.args(args)
		.output()
		.unwrap()
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
	let stdout = String::from_utf8_lossy(&output.stdout);
	serde_json::from_str(&stdout).unwrap_or_else(|e| {
		panic!("invalid JSON: {}\nstdout: {}", e, stdout)
	})
}

// -- 1. No requirements => pass, exit 0 ------------------------------

#[test]
fn gate_empty_obligations_passes() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "pass");
	assert_eq!(result["gate"]["exit_code"], 0);
	assert_eq!(result["gate"]["counts"]["total"], 0);
	assert_eq!(result["obligations"].as_array().unwrap().len(), 0);
}

// -- 2. arch_violations with no boundaries => MISSING_EVIDENCE --------

#[test]
fn gate_arch_violations_missing_boundaries() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Requirement with arch_violations obligation targeting src/core,
	// but NO boundary declarations exist for src/core.
	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"no violations in core","method":"arch_violations","target":"src/core"}]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(2), "MISSING_EVIDENCE => exit 2");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "incomplete");
	assert_eq!(result["gate"]["counts"]["missing_evidence"], 1);

	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls.len(), 1);
	assert_eq!(obls[0]["computed_verdict"], "MISSING_EVIDENCE");
	assert_eq!(obls[0]["effective_verdict"], "MISSING_EVIDENCE");
}

// -- 3. arch_violations passing => PASS, exit 0 -----------------------

#[test]
fn gate_arch_violations_passes() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Boundary: src/core --forbids--> src/adapters.
	// src/core does NOT import from src/adapters. No violations.
	insert_boundary(&db, "r1", "src/core", "src/adapters");

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"core must not depend on adapters","method":"arch_violations","target":"src/core"}]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(0), "PASS => exit 0");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "pass");
	assert_eq!(result["gate"]["counts"]["pass"], 1);

	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls[0]["computed_verdict"], "PASS");
	assert_eq!(obls[0]["effective_verdict"], "PASS");
	assert_eq!(obls[0]["evidence"]["violation_count"], 0);
}

// -- 4. arch_violations failing => FAIL, exit 1 -----------------------

#[test]
fn gate_arch_violations_fails() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Boundary: src/adapters --forbids--> src/core.
	// store.ts imports from core/service.ts → 1 violation.
	insert_boundary(&db, "r1", "src/adapters", "src/core");

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"adapters must not depend on core","method":"arch_violations","target":"src/adapters"}]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(1), "FAIL => exit 1");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "fail");
	assert_eq!(result["gate"]["counts"]["fail"], 1);

	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls[0]["computed_verdict"], "FAIL");
	assert_eq!(obls[0]["effective_verdict"], "FAIL");
	assert_eq!(obls[0]["evidence"]["violation_count"], 1);
}

// -- 5. Unsupported method => UNSUPPORTED, exit 2 ---------------------

#[test]
fn gate_unsupported_method() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"coverage check","method":"coverage_threshold","target":"src/core","threshold":80,"operator":">="}]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(2), "UNSUPPORTED => exit 2");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "incomplete");
	assert_eq!(result["gate"]["counts"]["unsupported"], 1);

	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls[0]["computed_verdict"], "UNSUPPORTED");
	assert_eq!(obls[0]["method"], "coverage_threshold");
}

// -- 6. Exact JSON contract -------------------------------------------

#[test]
fn gate_exact_json_contract() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	insert_boundary(&db, "r1", "src/core", "src/adapters");
	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"core clean","method":"arch_violations","target":"src/core"}]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);

	// Top-level gate report shape (NOT QueryResult envelope).
	assert_eq!(result["command"], "gate");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	// toolchain: null or object (no toolchain in Rust-indexed DBs).
	assert!(
		result["toolchain"].is_null() || result["toolchain"].is_object(),
		"toolchain must be null or object, got: {}",
		result["toolchain"]
	);

	// Obligations array.
	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls.len(), 1);
	let obl = &obls[0];
	assert_eq!(obl["req_id"], "REQ-001");
	assert_eq!(obl["req_version"], 1);
	assert!(obl["obligation_id"].is_string());
	assert!(obl["obligation"].is_string());
	assert_eq!(obl["method"], "arch_violations");
	assert_eq!(obl["target"], "src/core");
	assert!(obl["threshold"].is_null());
	assert!(obl["operator"].is_null());
	assert_eq!(obl["computed_verdict"], "PASS");
	assert_eq!(obl["effective_verdict"], "PASS");
	assert!(obl["evidence"].is_object());
	assert!(obl["waiver_basis"].is_null());

	// Gate outcome.
	let gate = &result["gate"];
	assert_eq!(gate["outcome"], "pass");
	assert_eq!(gate["exit_code"], 0);
	assert_eq!(gate["mode"], "default");
	assert_eq!(gate["counts"]["total"], 1);
	assert_eq!(gate["counts"]["pass"], 1);
	assert_eq!(gate["counts"]["fail"], 0);
	assert_eq!(gate["counts"]["waived"], 0);
	assert_eq!(gate["counts"]["missing_evidence"], 0);
	assert_eq!(gate["counts"]["unsupported"], 0);
}

// -- 7. Malformed active requirement => command error, exit 2 ---------

#[test]
fn gate_malformed_requirement_aborts() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Insert a requirement with invalid JSON in value_json.
	let conn = rusqlite::Connection::open(&db).unwrap();
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES ('bad-req', 'r1', 'r1:REQ:REQUIREMENT', 'requirement', 'not valid json', '2024-01-01T00:00:00Z', 1)",
		[],
	)
	.unwrap();

	let output = run_cmd(&["gate", db_str, "r1"]);
	// Malformed requirement must abort the command, not silently pass.
	assert_eq!(
		output.status.code(),
		Some(2),
		"malformed requirement must cause exit 2, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stdout.is_empty(), "no JSON on stdout for command error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("malformed"),
		"error should mention malformed requirement, stderr: {}",
		stderr
	);
}

// -- 8. Mixed verdicts: FAIL wins over PASS ---------------------------

#[test]
fn gate_mixed_verdicts_fail_wins() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Boundary for passing obligation: core --forbids--> adapters (no violations).
	insert_boundary(&db, "r1", "src/core", "src/adapters");
	// Boundary for failing obligation: adapters --forbids--> core (1 violation).
	insert_boundary(&db, "r1", "src/adapters", "src/core");

	// One requirement with two obligations: one passes, one fails.
	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[
			{"obligation_id":"obl-pass","obligation":"core clean","method":"arch_violations","target":"src/core"},
			{"obligation_id":"obl-fail","obligation":"adapters clean","method":"arch_violations","target":"src/adapters"}
		]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(1), "FAIL wins over PASS => exit 1");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "fail");
	assert_eq!(result["gate"]["counts"]["total"], 2);
	assert_eq!(result["gate"]["counts"]["pass"], 1);
	assert_eq!(result["gate"]["counts"]["fail"], 1);
}

// ── Rust-25: Waiver tests ──────────────────────────────────────────

/// Insert a waiver declaration for a specific obligation tuple.
fn insert_waiver(
	db_path: &std::path::Path,
	uid: &str,
	repo_uid: &str,
	req_id: &str,
	requirement_version: i64,
	obligation_id: &str,
	reason: &str,
	created_at: &str,
	expires_at: Option<&str>,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	let mut value = serde_json::json!({
		"req_id": req_id,
		"requirement_version": requirement_version,
		"obligation_id": obligation_id,
		"reason": reason,
		"created_at": created_at,
	});
	if let Some(exp) = expires_at {
		value["expires_at"] = serde_json::Value::String(exp.to_string());
	}
	let target_key = format!("{}:waiver:{}#{}", repo_uid, req_id, obligation_id);
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES (?, ?, ?, 'waiver', ?, ?, 1)",
		rusqlite::params![uid, repo_uid, target_key, value.to_string(), created_at],
	)
	.unwrap();
}

// -- 9. FAIL + active waiver => WAIVED, exit 0 -----------------------

#[test]
fn gate_waiver_suppresses_fail() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Boundary: adapters --forbids--> core (1 violation).
	insert_boundary(&db, "r1", "src/adapters", "src/core");

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"adapters must not depend on core","method":"arch_violations","target":"src/adapters"}]"#,
	);

	// Active waiver matching the exact tuple (no expiry = perpetual).
	insert_waiver(
		&db, "waiver-1", "r1", "REQ-001", 1, "obl-1",
		"known dependency, tracked for removal",
		"2024-01-01T00:00:00Z",
		None,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(0), "WAIVED => exit 0");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "pass");
	assert_eq!(result["gate"]["counts"]["waived"], 1);
	assert_eq!(result["gate"]["counts"]["fail"], 0);
	assert_eq!(result["gate"]["counts"]["pass"], 0);

	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls.len(), 1);
	assert_eq!(obls[0]["computed_verdict"], "FAIL");
	assert_eq!(obls[0]["effective_verdict"], "WAIVED");

	// Waiver basis populated with audit trail.
	let basis = &obls[0]["waiver_basis"];
	assert!(!basis.is_null(), "waiver_basis must be non-null for WAIVED");
	assert_eq!(basis["waiver_uid"], "waiver-1");
	assert_eq!(basis["reason"], "known dependency, tracked for removal");
	assert!(basis["expires_at"].is_null(), "perpetual waiver has no expiry");
}

// -- 10. FAIL + waiver with wrong obligation_id => no suppression ----

#[test]
fn gate_waiver_wrong_obligation_id_no_suppression() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	insert_boundary(&db, "r1", "src/adapters", "src/core");

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"adapters must not depend on core","method":"arch_violations","target":"src/adapters"}]"#,
	);

	// Waiver for a DIFFERENT obligation_id.
	insert_waiver(
		&db, "waiver-wrong", "r1", "REQ-001", 1, "obl-OTHER",
		"wrong obligation",
		"2024-01-01T00:00:00Z",
		None,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(1), "wrong obl_id => no suppression, exit 1");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "fail");
	assert_eq!(result["gate"]["counts"]["fail"], 1);
	assert_eq!(result["gate"]["counts"]["waived"], 0);

	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls[0]["computed_verdict"], "FAIL");
	assert_eq!(obls[0]["effective_verdict"], "FAIL");
	assert!(obls[0]["waiver_basis"].is_null());
}

// -- 11. FAIL + expired waiver => no suppression ---------------------

#[test]
fn gate_expired_waiver_no_suppression() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	insert_boundary(&db, "r1", "src/adapters", "src/core");

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"adapters must not depend on core","method":"arch_violations","target":"src/adapters"}]"#,
	);

	// Waiver that expired in the past.
	insert_waiver(
		&db, "waiver-expired", "r1", "REQ-001", 1, "obl-1",
		"temporary exception",
		"2023-01-01T00:00:00Z",
		Some("2023-06-01T00:00:00Z"), // Expired mid-2023
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(1), "expired waiver => no suppression, exit 1");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "fail");
	assert_eq!(result["gate"]["counts"]["fail"], 1);
	assert_eq!(result["gate"]["counts"]["waived"], 0);

	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls[0]["effective_verdict"], "FAIL");
	assert!(obls[0]["waiver_basis"].is_null());
}

// -- 12. PASS + active waiver => remains PASS, waiver_basis null -----

#[test]
fn gate_pass_with_waiver_stays_pass() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Boundary: core --forbids--> adapters. core does NOT import from
	// adapters, so the obligation PASSES on merit.
	insert_boundary(&db, "r1", "src/core", "src/adapters");

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"core must not depend on adapters","method":"arch_violations","target":"src/core"}]"#,
	);

	// Active waiver for this obligation — should NOT transform PASS to WAIVED.
	insert_waiver(
		&db, "waiver-unnecessary", "r1", "REQ-001", 1, "obl-1",
		"precautionary waiver",
		"2024-01-01T00:00:00Z",
		None,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(0), "PASS stays PASS => exit 0");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "pass");
	assert_eq!(result["gate"]["counts"]["pass"], 1);
	assert_eq!(result["gate"]["counts"]["waived"], 0, "PASS must not count as waived");

	let obls = result["obligations"].as_array().unwrap();
	assert_eq!(obls[0]["computed_verdict"], "PASS");
	assert_eq!(obls[0]["effective_verdict"], "PASS");
	assert!(
		obls[0]["waiver_basis"].is_null(),
		"waiver_basis must be null when computed_verdict is PASS"
	);
}

// -- 13. Malformed active waiver => command error, exit 2 ------------

#[test]
fn gate_malformed_waiver_aborts() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Set up a failing obligation so the gate attempts waiver lookup.
	insert_boundary(&db, "r1", "src/adapters", "src/core");
	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"adapters clean","method":"arch_violations","target":"src/adapters"}]"#,
	);

	// Insert a waiver with invalid JSON.
	let conn = rusqlite::Connection::open(&db).unwrap();
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES ('bad-waiver', 'r1', 'r1:waiver:REQ-001#obl-1', 'waiver', 'not valid json', '2024-01-01T00:00:00Z', 1)",
		[],
	)
	.unwrap();

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(2),
		"malformed waiver must cause exit 2, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stdout.is_empty(), "no JSON on stdout for command error");
	// SQLite's json_extract fails on invalid JSON before our parsing
	// layer sees the row. The error propagates through the Sqlite
	// variant as "malformed JSON", which is then wrapped by the gate
	// evaluator as "failed to read waivers: ...". Either way: the
	// malformed waiver cannot silently disappear.
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("failed to read waivers"),
		"error should propagate from waiver lookup, stderr: {}",
		stderr
	);
}

// -- 14. Waiver missing required field => command error, exit 2 ------

#[test]
fn gate_waiver_missing_reason_aborts() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	insert_boundary(&db, "r1", "src/adapters", "src/core");
	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"adapters clean","method":"arch_violations","target":"src/adapters"}]"#,
	);

	// Insert a waiver with valid JSON but missing required "reason" field.
	let conn = rusqlite::Connection::open(&db).unwrap();
	let incomplete_value = serde_json::json!({
		"req_id": "REQ-001",
		"requirement_version": 1,
		"obligation_id": "obl-1",
		"created_at": "2024-01-01T00:00:00Z"
		// "reason" deliberately omitted
	});
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES ('incomplete-waiver', 'r1', 'r1:waiver:REQ-001#obl-1', 'waiver', ?, '2024-01-01T00:00:00Z', 1)",
		rusqlite::params![incomplete_value.to_string()],
	)
	.unwrap();

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(2),
		"missing required field must cause exit 2, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stdout.is_empty(), "no JSON on stdout for command error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("malformed waiver"),
		"error should mention malformed waiver, stderr: {}",
		stderr
	);
	assert!(
		stderr.contains("reason"),
		"error should identify the missing field, stderr: {}",
		stderr
	);
}

// ── Rust-26: Gate mode tests ───────────────────────────────────────

// -- 15. Default mode: UNSUPPORTED without FAIL => incomplete, exit 2 --
// (This is the same as test 5, but explicitly confirms default mode label.)

#[test]
fn gate_default_mode_unsupported_is_incomplete() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"coverage check","method":"coverage_threshold","target":"src/core","threshold":80,"operator":">="}]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(2), "default: UNSUPPORTED => exit 2");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "incomplete");
	assert_eq!(result["gate"]["mode"], "default");
	assert_eq!(result["gate"]["counts"]["unsupported"], 1);
}

// -- 16. Strict mode: UNSUPPORTED without FAIL => fail, exit 1 -------

#[test]
fn gate_strict_mode_unsupported_is_fail() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"coverage check","method":"coverage_threshold","target":"src/core","threshold":80,"operator":">="}]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1", "--strict"]);
	assert_eq!(output.status.code(), Some(1), "strict: UNSUPPORTED => exit 1");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "fail");
	assert_eq!(result["gate"]["mode"], "strict");
	assert_eq!(result["gate"]["counts"]["unsupported"], 1);
	assert_eq!(result["gate"]["counts"]["fail"], 0);
}

// -- 17. Advisory mode: UNSUPPORTED without FAIL => pass, exit 0 -----

#[test]
fn gate_advisory_mode_unsupported_is_pass() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"coverage check","method":"coverage_threshold","target":"src/core","threshold":80,"operator":">="}]"#,
	);

	let output = run_cmd(&["gate", db_str, "r1", "--advisory"]);
	assert_eq!(output.status.code(), Some(0), "advisory: UNSUPPORTED => exit 0");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "pass");
	assert_eq!(result["gate"]["mode"], "advisory");
	assert_eq!(result["gate"]["counts"]["unsupported"], 1);
}

// -- 18. Strict mode: WAIVED obligation remains non-failing ----------

#[test]
fn gate_strict_mode_waived_is_non_failing() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// Set up a failing obligation with a waiver.
	insert_boundary(&db, "r1", "src/adapters", "src/core");
	insert_requirement(
		&db, "req-1", "r1", "REQ-001", 1,
		r#"[{"obligation_id":"obl-1","obligation":"adapters clean","method":"arch_violations","target":"src/adapters"}]"#,
	);
	insert_waiver(
		&db, "waiver-1", "r1", "REQ-001", 1, "obl-1",
		"known dependency",
		"2024-01-01T00:00:00Z",
		None,
	);

	let output = run_cmd(&["gate", db_str, "r1", "--strict"]);
	assert_eq!(output.status.code(), Some(0), "strict: WAIVED => exit 0");

	let result = parse_json(&output);
	assert_eq!(result["gate"]["outcome"], "pass");
	assert_eq!(result["gate"]["mode"], "strict");
	assert_eq!(result["gate"]["counts"]["waived"], 1);
	assert_eq!(result["gate"]["counts"]["fail"], 0);
}

// -- 19. --strict + --advisory => usage error, exit 1 ----------------

#[test]
fn gate_strict_and_advisory_mutually_exclusive() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["gate", db_str, "r1", "--strict", "--advisory"]);
	assert_eq!(output.status.code(), Some(1), "mutually exclusive => exit 1");
	assert!(output.stdout.is_empty(), "no JSON on stdout for usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("mutually exclusive"),
		"error should mention mutual exclusion, stderr: {}",
		stderr
	);
}

// -- 20. Exact JSON: gate.mode reflects selected mode ----------------

#[test]
fn gate_mode_field_reflects_selection() {
	let (_r, _d, db) = build_gate_db();
	let db_str = db.to_str().unwrap();

	// No requirements => vacuous pass. Test mode field in each mode.
	let output_default = run_cmd(&["gate", db_str, "r1"]);
	let result_default = parse_json(&output_default);
	assert_eq!(result_default["gate"]["mode"], "default");

	let output_strict = run_cmd(&["gate", db_str, "r1", "--strict"]);
	let result_strict = parse_json(&output_strict);
	assert_eq!(result_strict["gate"]["mode"], "strict");

	let output_advisory = run_cmd(&["gate", db_str, "r1", "--advisory"]);
	let result_advisory = parse_json(&output_advisory);
	assert_eq!(result_advisory["gate"]["mode"], "advisory");
}

