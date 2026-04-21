//! Deterministic tests for the `modules files` command.
//!
//! RS-MG-11: File ownership surface for Rust CLI.
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found (wrong repo_uid)
//!   4. Module not found (wrong module argument)
//!   5. Empty result (module with no owned files)
//!   6. Non-empty result with exact field assertions
//!   7. Module resolution by canonical_root_path
//!   8. Module resolution by module_key
//!   9. Deterministic ordering by path
//!  10. Envelope contract with module identity

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

fn fixture_path() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("..")
		.join("..")
		.join("..")
		.join("test")
		.join("fixtures")
		.join("typescript")
		.join("classifier-repo")
}

/// Build a temp DB by indexing the classifier-repo fixture.
fn build_indexed_db() -> (tempfile::TempDir, PathBuf) {
	let dir = tempfile::tempdir().unwrap();
	let db_path = dir.path().join("test.db");

	use repo_graph_repo_index::compose::{index_path, ComposeOptions};
	let result = index_path(
		&fixture_path(),
		&db_path,
		"test-repo",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(result.files_total, 1);

	(dir, db_path)
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

/// Insert a module candidate directly for testing.
fn insert_module_candidate(
	db_path: &std::path::Path,
	snapshot_uid: &str,
	repo_uid: &str,
	module_candidate_uid: &str,
	module_key: &str,
	canonical_root_path: &str,
	display_name: Option<&str>,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.execute(
		"INSERT INTO module_candidates
		 (module_candidate_uid, snapshot_uid, repo_uid, module_key,
		  module_kind, canonical_root_path, confidence, display_name, metadata_json)
		 VALUES (?, ?, ?, ?, 'npm_package', ?, 1.0, ?, NULL)",
		rusqlite::params![
			module_candidate_uid,
			snapshot_uid,
			repo_uid,
			module_key,
			canonical_root_path,
			display_name,
		],
	)
	.expect("insert module candidate");
}

/// Insert a file into the files table.
fn insert_file(
	db_path: &std::path::Path,
	repo_uid: &str,
	file_uid: &str,
	path: &str,
	language: Option<&str>,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.execute(
		"INSERT INTO files (file_uid, repo_uid, path, language, is_test, is_generated, is_excluded)
		 VALUES (?, ?, ?, ?, 0, 0, 0)",
		rusqlite::params![file_uid, repo_uid, path, language],
	)
	.expect("insert file");
}

/// Insert a file ownership assignment.
fn insert_file_ownership(
	db_path: &std::path::Path,
	snapshot_uid: &str,
	repo_uid: &str,
	file_uid: &str,
	module_candidate_uid: &str,
	assignment_kind: &str,
	confidence: f64,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.execute(
		"INSERT INTO module_file_ownership
		 (snapshot_uid, repo_uid, file_uid, module_candidate_uid,
		  assignment_kind, confidence, basis_json)
		 VALUES (?, ?, ?, ?, ?, ?, NULL)",
		rusqlite::params![
			snapshot_uid,
			repo_uid,
			file_uid,
			module_candidate_uid,
			assignment_kind,
			confidence,
		],
	)
	.expect("insert file ownership");
}

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn modules_files_usage_error() {
	let output = Command::new(binary_path())
		.args(["modules", "files"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn modules_files_usage_error_missing_module() {
	let output = Command::new(binary_path())
		.args(["modules", "files", "/some/path.db", "repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn modules_files_missing_db() {
	let output = Command::new(binary_path())
		.args(["modules", "files", "/nonexistent/path.db", "repo", "some-module"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("does not exist"),
		"stderr: {}",
		stderr
	);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn modules_files_repo_not_found() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"modules",
			"files",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
			"some-module",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("no snapshot found"),
		"stderr: {}",
		stderr
	);
}

// ── 4. Module not found ──────────────────────────────────────────

#[test]
fn modules_files_module_not_found() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"modules",
			"files",
			db_path.to_str().unwrap(),
			"test-repo",
			"nonexistent-module",
		])
		.output()
		.unwrap();

	// Module not found is exit 1 (usage-class error), not exit 2 (runtime)
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("module not found"),
		"stderr: {}",
		stderr
	);
	assert!(
		stderr.contains("hint:"),
		"stderr should contain hint: {}",
		stderr
	);
}

// ── 5. Empty result ──────────────────────────────────────────────

#[test]
fn modules_files_empty_result() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert a module with no files
	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-empty",
		"npm:@test/empty",
		"packages/empty",
		None,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"files",
			db_path.to_str().unwrap(),
			"test-repo",
			"packages/empty",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"empty result is success, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

	assert_eq!(result["command"], "modules files");
	assert_eq!(result["count"], 0);
	assert!(result["results"].as_array().unwrap().is_empty());
}

// ── 6. Non-empty result with exact field assertions ──────────────

#[test]
fn modules_files_exact_fields() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert module
	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-core",
		"npm:@test/core",
		"packages/core",
		Some("@test/core"),
	);

	// Insert file
	insert_file(
		&db_path,
		"test-repo",
		"file-1",
		"packages/core/index.ts",
		Some("typescript"),
	);

	// Create ownership
	insert_file_ownership(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"file-1",
		"mc-core",
		"manifest",
		0.95,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"files",
			db_path.to_str().unwrap(),
			"test-repo",
			"packages/core",
		])
		.output()
		.unwrap();

	assert_eq!(
		output.status.code(),
		Some(0),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

	assert_eq!(result["count"], 1);

	let files = result["results"].as_array().unwrap();
	assert_eq!(files.len(), 1);

	let f = &files[0];
	assert_eq!(f["file_uid"], "file-1");
	assert_eq!(f["path"], "packages/core/index.ts");
	assert_eq!(f["language"], "typescript");
	assert_eq!(f["assignment_kind"], "manifest");
	assert!((f["confidence"].as_f64().unwrap() - 0.95).abs() < 0.001);
}

// ── 7. Module resolution by canonical_root_path ──────────────────

#[test]
fn modules_files_resolve_by_canonical_path() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-app",
		"npm:@test/app",
		"packages/app",
		None,
	);

	insert_file(
		&db_path,
		"test-repo",
		"file-app",
		"packages/app/main.ts",
		None,
	);

	insert_file_ownership(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"file-app",
		"mc-app",
		"manifest",
		1.0,
	);

	// Resolve by canonical_root_path
	let output = Command::new(binary_path())
		.args([
			"modules",
			"files",
			db_path.to_str().unwrap(),
			"test-repo",
			"packages/app",  // canonical_root_path
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	assert_eq!(result["count"], 1);
	assert_eq!(result["results"][0]["path"], "packages/app/main.ts");
}

// ── 8. Module resolution by module_key ───────────────────────────

#[test]
fn modules_files_resolve_by_module_key() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-lib",
		"npm:@test/lib",
		"packages/lib",
		None,
	);

	insert_file(
		&db_path,
		"test-repo",
		"file-lib",
		"packages/lib/util.ts",
		None,
	);

	insert_file_ownership(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"file-lib",
		"mc-lib",
		"manifest",
		1.0,
	);

	// Resolve by module_key
	let output = Command::new(binary_path())
		.args([
			"modules",
			"files",
			db_path.to_str().unwrap(),
			"test-repo",
			"npm:@test/lib",  // module_key
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	assert_eq!(result["count"], 1);
	assert_eq!(result["results"][0]["path"], "packages/lib/util.ts");
}

// ── 9. Deterministic ordering by path ────────────────────────────

#[test]
fn modules_files_sorted_by_path() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-pkg",
		"npm:@test/pkg",
		"packages/pkg",
		None,
	);

	// Insert files in non-alphabetical order
	insert_file(&db_path, "test-repo", "f3", "packages/pkg/zebra.ts", None);
	insert_file(&db_path, "test-repo", "f1", "packages/pkg/alpha.ts", None);
	insert_file(&db_path, "test-repo", "f2", "packages/pkg/beta.ts", None);

	insert_file_ownership(&db_path, &snapshot_uid, "test-repo", "f3", "mc-pkg", "manifest", 1.0);
	insert_file_ownership(&db_path, &snapshot_uid, "test-repo", "f1", "mc-pkg", "manifest", 1.0);
	insert_file_ownership(&db_path, &snapshot_uid, "test-repo", "f2", "mc-pkg", "manifest", 1.0);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"files",
			db_path.to_str().unwrap(),
			"test-repo",
			"packages/pkg",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	let files = result["results"].as_array().unwrap();
	assert_eq!(files.len(), 3);

	// Sorted by path ascending
	assert_eq!(files[0]["path"], "packages/pkg/alpha.ts");
	assert_eq!(files[1]["path"], "packages/pkg/beta.ts");
	assert_eq!(files[2]["path"], "packages/pkg/zebra.ts");
}

// ── 10. Envelope contract with module identity ───────────────────

#[test]
fn modules_files_envelope_contract() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-test",
		"npm:@test/test",
		"packages/test",
		None,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"files",
			db_path.to_str().unwrap(),
			"test-repo",
			"packages/test",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// Standard envelope fields
	assert_eq!(result["command"], "modules files");
	assert_eq!(result["repo"], "test-repo");
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"].is_string());
	assert!(result["count"].is_number());
	assert!(result["stale"].is_boolean());
	assert!(result["results"].is_array());

	// Module identity in envelope
	let module = &result["module"];
	assert!(module.is_object(), "module identity must be in envelope");
	assert_eq!(module["module_uid"], "mc-test");
	assert_eq!(module["module_key"], "npm:@test/test");
	assert_eq!(module["canonical_root_path"], "packages/test");
}
