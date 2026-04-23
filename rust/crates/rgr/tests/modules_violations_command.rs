//! Deterministic tests for the `modules violations` command.
//!
//! RS-MG-11: Discovered-module boundary violation surface for Rust CLI.
//!
//! This tests the end-to-end flow:
//!   1. Index a workspace fixture with cross-package imports
//!   2. Insert discovered module candidates
//!   3. Insert file ownership
//!   4. Declare a boundary via `rmap modules boundary`
//!   5. Run `rmap modules violations`
//!   6. Assert violations are detected
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found (wrong repo_uid)
//!   4. No policy declarations => empty violations
//!   5. Declaration exists, no violating imports => empty violations
//!   6. Exact violation result (end-to-end with fixture)
//!   7. Stale declaration detection
//!   8. Envelope contract

use std::path::PathBuf;
use std::process::Command;

fn binary_path() -> PathBuf {
	let mut path = PathBuf::from(env!("CARGO_BIN_EXE_rmap"));
	if !path.exists() {
		path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("..")
			.join("..")
			.join("target")
			.join("debug")
			.join("rmap");
	}
	path
}

/// Build a workspace fixture with two packages and cross-package imports.
///
/// Layout:
///   packages/app/package.json  — {"name": "@fixture/app", "dependencies": {"@fixture/core": "*"}}
///   packages/app/src/index.ts  — import { coreService } from "@fixture/core/src/service";
///   packages/core/package.json — {"name": "@fixture/core"}
///   packages/core/src/service.ts — export function coreService() {}
///
/// IMPORTS edge (file-level):
///   packages/app/src/index.ts --IMPORTS--> packages/core/src/service.ts
///
/// With boundary "packages/app --forbids--> packages/core":
///   1 violation: app imports from core
fn build_workspace_db() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
	let repo_dir = tempfile::tempdir().unwrap();
	let root = repo_dir.path();

	// Root package.json with workspaces
	std::fs::write(
		root.join("package.json"),
		r#"{"name": "workspace-fixture", "private": true, "workspaces": ["packages/*"]}"#,
	)
	.unwrap();

	// packages/core
	std::fs::create_dir_all(root.join("packages/core/src")).unwrap();
	std::fs::write(
		root.join("packages/core/package.json"),
		r#"{"name": "@fixture/core", "version": "1.0.0"}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("packages/core/src/service.ts"),
		"export function coreService(): string { return \"core\"; }\n",
	)
	.unwrap();

	// packages/app (depends on core)
	// Use relative import to ensure IMPORTS edge is created
	std::fs::create_dir_all(root.join("packages/app/src")).unwrap();
	std::fs::write(
		root.join("packages/app/package.json"),
		r#"{"name": "@fixture/app", "version": "1.0.0", "dependencies": {"@fixture/core": "workspace:*"}}"#,
	)
	.unwrap();
	std::fs::write(
		root.join("packages/app/src/index.ts"),
		"import { coreService } from \"../../core/src/service\";\nexport function app(): string { return `app uses ${coreService()}`; }\n",
	)
	.unwrap();

	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(root, &db_path, "r1", &ComposeOptions::default()).unwrap();
	// 2 TS files indexed (app/src/index.ts, core/src/service.ts)
	assert_eq!(result.files_total, 2);

	(repo_dir, db_dir, db_path)
}

/// Get the snapshot UID for a repo.
fn get_snapshot_uid(db_path: &std::path::Path, repo_uid: &str) -> String {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.query_row(
		"SELECT snapshot_uid FROM snapshots WHERE repo_uid = ? ORDER BY created_at DESC LIMIT 1",
		[repo_uid],
		|row| row.get(0),
	)
	.expect("get snapshot uid")
}

/// Insert a module candidate for testing.
fn insert_module_candidate(
	db_path: &std::path::Path,
	snapshot_uid: &str,
	repo_uid: &str,
	module_candidate_uid: &str,
	module_key: &str,
	canonical_root_path: &str,
	module_kind: &str,
	display_name: Option<&str>,
	confidence: f64,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.execute(
		"INSERT INTO module_candidates
		 (module_candidate_uid, snapshot_uid, repo_uid, module_key,
		  module_kind, canonical_root_path, confidence, display_name, metadata_json)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)",
		rusqlite::params![
			module_candidate_uid,
			snapshot_uid,
			repo_uid,
			module_key,
			module_kind,
			canonical_root_path,
			confidence,
			display_name,
		],
	)
	.expect("insert module candidate");
}

/// Insert file ownership for all files under a module's canonical root path.
fn insert_file_ownership_for_module(
	db_path: &std::path::Path,
	snapshot_uid: &str,
	repo_uid: &str,
	module_candidate_uid: &str,
	canonical_root_path: &str,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	// Find all files under the module's canonical root path
	let mut stmt = conn
		.prepare(
			"SELECT file_uid FROM files
			 WHERE repo_uid = ? AND path LIKE ?",
		)
		.unwrap();
	let pattern = format!("{}%", canonical_root_path);
	let file_uids: Vec<String> = stmt
		.query_map([repo_uid, &pattern], |row| row.get(0))
		.unwrap()
		.map(|r| r.unwrap())
		.collect();

	// Insert ownership for each file
	for file_uid in file_uids {
		conn.execute(
			"INSERT INTO module_file_ownership
			 (snapshot_uid, repo_uid, file_uid, module_candidate_uid,
			  assignment_kind, confidence, basis_json)
			 VALUES (?, ?, ?, ?, 'manifest', 1.0, NULL)",
			rusqlite::params![snapshot_uid, repo_uid, file_uid, module_candidate_uid],
		)
		.expect("insert file ownership");
	}
}

fn run_cmd(args: &[&str]) -> std::process::Output {
	Command::new(binary_path()).args(args).output().unwrap()
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
	let stdout = String::from_utf8_lossy(&output.stdout);
	serde_json::from_str(&stdout)
		.unwrap_or_else(|e| panic!("invalid JSON: {}\nstdout: {}", e, stdout))
}

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn modules_violations_usage_error() {
	let output = run_cmd(&["modules", "violations"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn modules_violations_usage_error_missing_repo() {
	let output = run_cmd(&["modules", "violations", "/some/path.db"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn modules_violations_missing_db() {
	let output = run_cmd(&["modules", "violations", "/nonexistent.db", "r1"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn modules_violations_repo_not_found() {
	let (_r, _d, db) = build_workspace_db();
	let output = run_cmd(&["modules", "violations", db.to_str().unwrap(), "nonexistent"]);
	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("no snapshot"), "stderr: {}", stderr);
}

// ── 4. No declarations => empty ──────────────────────────────────

#[test]
fn modules_violations_empty_when_no_declarations() {
	let (_r, _d, db) = build_workspace_db();
	let snapshot_uid = get_snapshot_uid(&db, "r1");

	// Insert module candidates (but no boundary declarations)
	insert_module_candidate(
		&db,
		&snapshot_uid,
		"r1",
		"mc-app",
		"npm:@fixture/app",
		"packages/app",
		"npm_package",
		Some("@fixture/app"),
		0.95,
	);
	insert_module_candidate(
		&db,
		&snapshot_uid,
		"r1",
		"mc-core",
		"npm:@fixture/core",
		"packages/core",
		"npm_package",
		Some("@fixture/core"),
		0.95,
	);

	// Insert file ownership
	insert_file_ownership_for_module(&db, &snapshot_uid, "r1", "mc-app", "packages/app/");
	insert_file_ownership_for_module(&db, &snapshot_uid, "r1", "mc-core", "packages/core/");

	let db_str = db.to_str().unwrap();
	let output = run_cmd(&["modules", "violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);
	assert_eq!(result["count"], 0);
	let violations = result["results"]["violations"].as_array().unwrap();
	assert!(violations.is_empty());
}

// ── 5. Declaration exists, no violating imports ──────────────────

#[test]
fn modules_violations_empty_when_no_violating_imports() {
	let (_r, _d, db) = build_workspace_db();
	let snapshot_uid = get_snapshot_uid(&db, "r1");

	// Insert module candidates
	insert_module_candidate(
		&db,
		&snapshot_uid,
		"r1",
		"mc-app",
		"npm:@fixture/app",
		"packages/app",
		"npm_package",
		Some("@fixture/app"),
		0.95,
	);
	insert_module_candidate(
		&db,
		&snapshot_uid,
		"r1",
		"mc-core",
		"npm:@fixture/core",
		"packages/core",
		"npm_package",
		Some("@fixture/core"),
		0.95,
	);

	// Insert file ownership
	insert_file_ownership_for_module(&db, &snapshot_uid, "r1", "mc-app", "packages/app/");
	insert_file_ownership_for_module(&db, &snapshot_uid, "r1", "mc-core", "packages/core/");

	let db_str = db.to_str().unwrap();

	// Declare boundary: core --forbids--> app (core does NOT import from app)
	let boundary_output = run_cmd(&[
		"modules",
		"boundary",
		db_str,
		"r1",
		"packages/core",
		"--forbids",
		"packages/app",
	]);
	assert_eq!(
		boundary_output.status.code(),
		Some(0),
		"boundary declaration failed: {}",
		String::from_utf8_lossy(&boundary_output.stderr)
	);

	let output = run_cmd(&["modules", "violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);
	assert_eq!(result["count"], 0, "core does not import from app");
}

// ── 6. Exact violation result (end-to-end) ───────────────────────

#[test]
fn modules_violations_exact_results() {
	let (_r, _d, db) = build_workspace_db();
	let snapshot_uid = get_snapshot_uid(&db, "r1");

	// Insert module candidates
	insert_module_candidate(
		&db,
		&snapshot_uid,
		"r1",
		"mc-app",
		"npm:@fixture/app",
		"packages/app",
		"npm_package",
		Some("@fixture/app"),
		0.95,
	);
	insert_module_candidate(
		&db,
		&snapshot_uid,
		"r1",
		"mc-core",
		"npm:@fixture/core",
		"packages/core",
		"npm_package",
		Some("@fixture/core"),
		0.95,
	);

	// Insert file ownership
	insert_file_ownership_for_module(&db, &snapshot_uid, "r1", "mc-app", "packages/app/");
	insert_file_ownership_for_module(&db, &snapshot_uid, "r1", "mc-core", "packages/core/");

	let db_str = db.to_str().unwrap();

	// Declare boundary: app --forbids--> core (app DOES import from core)
	let boundary_output = run_cmd(&[
		"modules",
		"boundary",
		db_str,
		"r1",
		"packages/app",
		"--forbids",
		"packages/core",
		"--reason",
		"app must not depend on core",
	]);
	assert_eq!(
		boundary_output.status.code(),
		Some(0),
		"boundary declaration failed: {}",
		String::from_utf8_lossy(&boundary_output.stderr)
	);

	let output = run_cmd(&["modules", "violations", db_str, "r1"]);
	// Exit code is 1 when violations exist (not 0)
	assert_eq!(
		output.status.code(),
		Some(1),
		"expected exit code 1 when violations exist, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);

	// Should have exactly 1 violation
	assert_eq!(result["count"], 1, "expected 1 violation, got: {}", result);

	let violations = result["results"]["violations"].as_array().unwrap();
	assert_eq!(violations.len(), 1);

	let v = &violations[0];
	assert_eq!(v["source"], "packages/app");
	assert_eq!(v["target"], "packages/core");
	assert_eq!(v["reason"], "app must not depend on core");
	assert!(
		v["import_count"].as_u64().unwrap() >= 1,
		"import_count should be >= 1, got: {}",
		v["import_count"]
	);
	assert!(
		v["source_file_count"].as_u64().unwrap() >= 1,
		"source_file_count should be >= 1, got: {}",
		v["source_file_count"]
	);

	// Stale declarations should be empty
	let stale = result["results"]["stale_declarations"].as_array().unwrap();
	assert!(stale.is_empty());
	assert_eq!(result["stale_count"], 0);
}

// ── 7. Stale declaration detection ───────────────────────────────

/// Insert a discovered-module boundary declaration via SQL (bypassing CLI validation).
fn insert_boundary_declaration(
	db_path: &std::path::Path,
	repo_uid: &str,
	declaration_uid: &str,
	source_path: &str,
	target_path: &str,
	reason: Option<&str>,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	// Use the exact format that the CLI uses (camelCase keys, selectorDomain, nested objects)
	let value_json = match reason {
		Some(r) => format!(
			r#"{{"selectorDomain":"discovered_module","source":{{"canonicalRootPath":"{}"}},"forbids":{{"canonicalRootPath":"{}"}},"reason":"{}"}}"#,
			source_path, target_path, r
		),
		None => format!(
			r#"{{"selectorDomain":"discovered_module","source":{{"canonicalRootPath":"{}"}},"forbids":{{"canonicalRootPath":"{}"}}}}"#,
			source_path, target_path
		),
	};
	let target_stable_key = format!("{}:{}:MODULE", repo_uid, source_path);
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES (?, ?, ?, 'boundary', ?, '2024-01-01T00:00:00Z', 1)",
		rusqlite::params![declaration_uid, repo_uid, target_stable_key, value_json],
	)
	.expect("insert boundary declaration");
}

#[test]
fn modules_violations_stale_declaration() {
	let (_r, _d, db) = build_workspace_db();
	let snapshot_uid = get_snapshot_uid(&db, "r1");

	// Insert module candidates (only app, NOT core)
	insert_module_candidate(
		&db,
		&snapshot_uid,
		"r1",
		"mc-app",
		"npm:@fixture/app",
		"packages/app",
		"npm_package",
		Some("@fixture/app"),
		0.95,
	);
	// Note: NOT inserting core module candidate

	// Insert file ownership
	insert_file_ownership_for_module(&db, &snapshot_uid, "r1", "mc-app", "packages/app/");

	// Insert boundary via SQL (bypasses CLI validation that target exists)
	// app --forbids--> packages/nonexistent
	insert_boundary_declaration(
		&db,
		"r1",
		"decl-stale-test",
		"packages/app",
		"packages/nonexistent",
		None,
	);

	let db_str = db.to_str().unwrap();
	let output = run_cmd(&["modules", "violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);

	// No violations (target doesn't exist as module)
	assert_eq!(result["count"], 0);

	// But should have 1 stale declaration
	assert_eq!(result["stale_count"], 1, "expected 1 stale declaration, got: {}", result);
	let stale = result["results"]["stale_declarations"].as_array().unwrap();
	assert_eq!(stale.len(), 1);

	let s = &stale[0];
	assert_eq!(s["stale_side"], "target");
	let missing = s["missing_paths"].as_array().unwrap();
	assert!(
		missing.iter().any(|p| p.as_str().unwrap().contains("nonexistent")),
		"missing_paths should contain nonexistent, got: {:?}",
		missing
	);
}

// ── 8. Envelope contract ─────────────────────────────────────────

#[test]
fn modules_violations_envelope_contract() {
	let (_r, _d, db) = build_workspace_db();
	let db_str = db.to_str().unwrap();

	let output = run_cmd(&["modules", "violations", db_str, "r1"]);
	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let result = parse_json(&output);

	// Full QueryResult envelope
	assert_eq!(result["command"], "modules violations");
	assert!(result["repo"].is_string());
	assert!(result["snapshot"].is_string());
	assert!(
		result["snapshot_scope"] == "full" || result["snapshot_scope"] == "incremental"
	);
	assert!(result["basis_commit"].is_null() || result["basis_commit"].is_string());
	assert!(result["stale"].is_boolean());

	// Results structure
	assert!(result["results"].is_object());
	assert!(result["results"]["violations"].is_array());
	assert!(result["results"]["stale_declarations"].is_array());

	// Count fields
	assert!(result["count"].is_number());
	assert!(result["stale_count"].is_number());
}
