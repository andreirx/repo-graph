//! Deterministic tests for the `violations` command.
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. No declarations => empty result
//!   5. Declaration exists but no violating imports => empty result
//!   6. Exact violation result with raw-SQL boundary declarations
//!   7. Duplicate boundary declarations produce deduplicated violations
//!   8. Envelope contract

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rgr-rust"))
}

/// Build a fixture with cross-module imports for violation testing.
///
/// Layout:
///   src/core/service.ts   — import { helper } from "../util/helper";
///                            export function serve() { helper(); }
///   src/util/helper.ts    — export function helper() {}
///   src/adapters/store.ts — import { serve } from "../core/service";
///                            export function store() { serve(); }
///
/// IMPORTS edges (FILE-level):
///   src/core/service.ts    --IMPORTS--> src/util/helper.ts
///   src/adapters/store.ts  --IMPORTS--> src/core/service.ts
///
/// With boundary "src/core --forbids--> src/adapters":
///   No violations (core does not import from adapters)
///
/// With boundary "src/adapters --forbids--> src/core":
///   1 violation: store.ts imports from core/service.ts
fn build_violations_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
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
	let result = index_path(
		root,
		&db_path,
		"r1",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(result.files_total, 3);

	(repo_dir, db_dir, db_path)
}

/// Insert a raw boundary declaration via SQL.
fn insert_boundary_declaration(
	db_path: &std::path::Path,
	repo_uid: &str,
	module_path: &str,
	forbids: &str,
	reason: Option<&str>,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	let uid = format!("decl-{}-{}", module_path.replace('/', "-"), forbids.replace('/', "-"));
	let stable_key = format!("{}:{}:MODULE", repo_uid, module_path);
	let value_json = if let Some(r) = reason {
		format!(r#"{{"forbids":"{}","reason":"{}"}}"#, forbids, r)
	} else {
		format!(r#"{{"forbids":"{}"}}"#, forbids)
	};
	conn.execute(
		"INSERT INTO declarations (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
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

// -- 1. Usage error ---------------------------------------------------

#[test]
fn violations_usage_error() {
	let output = run_cmd(&["violations"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// -- 2. Missing DB ----------------------------------------------------

#[test]
fn violations_missing_db() {
	let output = run_cmd(&["violations", "/nonexistent.db", "r1"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
}

// -- 3. Repo not found ------------------------------------------------

#[test]
fn violations_repo_not_found() {
	let (_r, _d, db) = build_violations_db();
	let output = run_cmd(&["violations", db.to_str().unwrap(), "nonexistent"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("no snapshot"), "stderr: {}", stderr);
}

// -- 4. No declarations => empty result -------------------------------

#[test]
fn violations_empty_when_no_declarations() {
	let (_r, _d, db) = build_violations_db();
	let db_str = db.to_str().unwrap();

	// No boundary declarations exist.
	let output = run_cmd(&["violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());

	let result = parse_json(&output);
	assert_eq!(result["count"], 0);
}

// -- 5. Declaration exists but no violating imports -------------------

#[test]
fn violations_empty_when_no_violating_imports() {
	let (_r, _d, db) = build_violations_db();
	let db_str = db.to_str().unwrap();

	// src/core --forbids--> src/adapters.
	// core does NOT import from adapters, so 0 violations.
	insert_boundary_declaration(&db, "r1", "src/core", "src/adapters", None);

	let output = run_cmd(&["violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());

	let result = parse_json(&output);
	assert_eq!(result["count"], 0, "core does not import adapters");
}

// -- 6. Exact violation result ----------------------------------------

#[test]
fn violations_exact_results() {
	let (_r, _d, db) = build_violations_db();
	let db_str = db.to_str().unwrap();

	// src/adapters --forbids--> src/core.
	// store.ts in adapters imports from core/service.ts → 1 violation.
	insert_boundary_declaration(
		&db,
		"r1",
		"src/adapters",
		"src/core",
		Some("adapters must not depend on core"),
	);

	let output = run_cmd(&["violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());

	let result = parse_json(&output);
	assert_eq!(result["count"], 1, "expected 1 violation, got: {}", result);

	let violations = result["results"].as_array().unwrap();
	assert_eq!(violations.len(), 1);

	let v = &violations[0];
	assert_eq!(v["boundary_module"], "src/adapters");
	assert_eq!(v["forbidden_module"], "src/core");
	assert_eq!(v["reason"], "adapters must not depend on core");
	assert!(
		v["source_file"].as_str().unwrap().contains("store.ts"),
		"source should be store.ts, got: {}",
		v["source_file"]
	);
	assert!(
		v["target_file"].as_str().unwrap().contains("service.ts"),
		"target should be service.ts, got: {}",
		v["target_file"]
	);
}

// -- 7. Duplicate declarations produce deduplicated violations --------

#[test]
fn violations_dedup_duplicate_declarations() {
	let (_r, _d, db) = build_violations_db();
	let db_str = db.to_str().unwrap();

	// Insert the SAME boundary rule twice (different declaration UIDs).
	insert_boundary_declaration(
		&db,
		"r1",
		"src/adapters",
		"src/core",
		Some("first declaration"),
	);
	// Second declaration with same (module, forbids) but different UID.
	let conn = rusqlite::Connection::open(&db).unwrap();
	conn.execute(
		"INSERT INTO declarations (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES ('decl-dup-2', 'r1', 'r1:src/adapters:MODULE', 'boundary', '{\"forbids\":\"src/core\",\"reason\":\"second declaration\"}', '2024-01-02T00:00:00Z', 1)",
		[],
	)
	.unwrap();

	let output = run_cmd(&["violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);
	// Should still produce exactly 1 violation (store.ts → service.ts),
	// not 2 (one per declaration).
	assert_eq!(
		result["count"], 1,
		"duplicate declarations must be deduplicated, got: {}",
		result
	);
}

// -- 8. Envelope contract ---------------------------------------------

#[test]
fn violations_envelope_contract() {
	let (_r, _d, db) = build_violations_db();
	let db_str = db.to_str().unwrap();

	insert_boundary_declaration(&db, "r1", "src/adapters", "src/core", None);

	let output = run_cmd(&["violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);

	// Full TS-compatible QueryResult envelope.
	assert_eq!(result["command"], "arch violations");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(
		result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental"
	);
	assert!(
		result["basis_commit"].is_null() || result["basis_commit"].is_string()
	);
	assert!(result["stale"].is_boolean());
	assert!(result["results"].is_array());
	assert!(result["count"].is_number());
}
