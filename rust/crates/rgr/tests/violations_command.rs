//! Deterministic tests for the `violations` command.
//!
//! Output shape (post RS-MG integration):
//!   results: {
//!     declared_boundary_violations: [...],
//!     discovered_module_violations: [...]
//!   }
//!   stale_declarations: [...]
//!   count: total (declared + discovered)
//!   declared_boundary_count: N
//!   discovered_module_count: N
//!   stale_count: N
//!
//! Test matrix:
//!   1. Usage error
//!   2. Missing DB / open failure
//!   3. Repo not found / no READY snapshot
//!   4. No declarations => empty results in both sections
//!   5. Declaration exists but no violating imports => empty results
//!   6. Exact violation result with raw-SQL boundary declarations
//!   7. Duplicate boundary declarations produce deduplicated violations
//!   8. Envelope contract (updated for new shape)

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	PathBuf::from(env!("CARGO_BIN_EXE_rmap"))
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

	// Total count is declared + discovered
	assert_eq!(result["count"], 1, "expected 1 total violation, got: {}", result);
	assert_eq!(result["declared_boundary_count"], 1);
	assert_eq!(result["discovered_module_count"], 0);

	// Access declared violations via the new structure
	let declared = result["results"]["declared_boundary_violations"].as_array().unwrap();
	assert_eq!(declared.len(), 1);

	let v = &declared[0];
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

	// Discovered section should be empty
	let discovered = result["results"]["discovered_module_violations"].as_array().unwrap();
	assert!(discovered.is_empty());
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
	// not 2 (one per declaration). Check declared section.
	assert_eq!(
		result["declared_boundary_count"], 1,
		"duplicate declarations must be deduplicated, got: {}",
		result
	);
	assert_eq!(result["count"], 1, "total count should be 1");
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

	// Updated output shape: results is an object with two sections
	assert!(result["results"].is_object(), "results must be object");
	assert!(
		result["results"]["declared_boundary_violations"].is_array(),
		"declared_boundary_violations must be array"
	);
	assert!(
		result["results"]["discovered_module_violations"].is_array(),
		"discovered_module_violations must be array"
	);

	// Count fields
	assert!(result["count"].is_number());
	assert!(result["declared_boundary_count"].is_number());
	assert!(result["discovered_module_count"].is_number());
	assert!(result["stale_count"].is_number());
	assert!(result["stale_declarations"].is_array());
}

// -- 9. Discovered-module violations integration ----------------------
//
// These tests verify the structural integration of discovered-module
// policy into the unified violations command. Full end-to-end testing
// of discovered-module violation detection is covered by:
// - modules violations command tests (in the modules test suite)
// - gate module_violations method tests
//
// Here we verify:
// 1. The discovered section exists and is empty when no modules exist
// 2. Both policy substrates can coexist

#[test]
fn violations_discovered_section_empty_when_no_modules() {
	let (_r, _d, db) = build_violations_db();
	let db_str = db.to_str().unwrap();

	// No module candidates exist, so discovered_module_violations should be empty
	let output = run_cmd(&["violations", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);

	assert_eq!(result["discovered_module_count"], 0);
	let discovered = result["results"]["discovered_module_violations"].as_array().unwrap();
	assert!(discovered.is_empty());
}

// Note: Stale declaration detection is tested by the `modules violations`
// command tests. The unified violations command uses the same evaluation
// helper, so stale behavior is covered there.

#[test]
fn violations_both_sections_independent() {
	let (_r, _d, db) = build_violations_db();
	let db_str = db.to_str().unwrap();

	// Set up declared boundary (legacy style) - this will produce a violation
	insert_boundary_declaration(&db, "r1", "src/adapters", "src/core", None);

	// Run violations command
	let output = run_cmd(&["violations", db_str, "r1"]);
	assert_eq!(output.status.code(), Some(0));

	let result = parse_json(&output);

	// Should have declared violation but no discovered (no modules set up)
	assert_eq!(result["declared_boundary_count"], 1);
	assert_eq!(result["discovered_module_count"], 0);
	assert_eq!(result["count"], 1, "total is declared only");

	// Both sections should exist
	assert!(result["results"]["declared_boundary_violations"].is_array());
	assert!(result["results"]["discovered_module_violations"].is_array());
}
