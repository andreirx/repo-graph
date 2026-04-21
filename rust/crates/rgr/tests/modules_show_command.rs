//! Deterministic tests for the `modules show` command.
//!
//! RS-MG-13b: Module detail surface for Rust CLI.
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found (wrong repo_uid)
//!   4. Module not found (exit 1, not 2)
//!   5. Resolution by canonical_root_path
//!   6. Resolution by module_key (fallback)
//!   7. Exact field assertions (identity, rollups, neighbors, violations)
//!   8. Degradation on malformed policy
//!   9. Envelope contract

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

/// Insert a raw declaration for testing malformed policy.
fn insert_raw_declaration(
	db_path: &std::path::Path,
	declaration_uid: &str,
	repo_uid: &str,
	kind: &str,
	value_json: &str,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	let now = "2026-01-01T00:00:00Z";
	conn.execute(
		"INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES (?, ?, '', ?, ?, ?, 1)",
		rusqlite::params![declaration_uid, repo_uid, kind, value_json, now],
	)
	.expect("insert declaration");
}

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn modules_show_usage_error() {
	let output = Command::new(binary_path())
		.args(["modules", "show"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn modules_show_usage_error_missing_module() {
	let output = Command::new(binary_path())
		.args(["modules", "show", "/some/path.db", "repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn modules_show_missing_db() {
	let output = Command::new(binary_path())
		.args(["modules", "show", "/nonexistent/path.db", "repo", "module"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn modules_show_repo_not_found() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"modules",
			"show",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
			"some-module",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("no snapshot found"), "stderr: {}", stderr);
}

// ── 4. Module not found (exit 1) ─────────────────────────────────

#[test]
fn modules_show_module_not_found() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert a module so the repo has modules
	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-core",
		"npm:@test/core",
		"packages/core",
		"npm_package",
		Some("@test/core"),
		0.95,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"nonexistent-module",
		])
		.output()
		.unwrap();

	// Exit 1 for resolution failure, not 2
	assert_eq!(
		output.status.code(),
		Some(1),
		"module not found must exit 1, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("module not found"), "stderr: {}", stderr);
}

// ── 5. Resolution by canonical_root_path ─────────────────────────

#[test]
fn modules_show_resolve_by_canonical_path() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-core",
		"npm:@test/core",
		"packages/core",
		"npm_package",
		Some("@test/core"),
		0.95,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"packages/core", // by canonical_root_path
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

	assert_eq!(result["module"]["canonical_root_path"], "packages/core");
	assert_eq!(result["module"]["module_uid"], "mc-core");
}

// ── 6. Resolution by module_key (fallback) ───────────────────────

#[test]
fn modules_show_resolve_by_module_key() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-core",
		"npm:@test/core",
		"packages/core",
		"npm_package",
		Some("@test/core"),
		0.95,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"npm:@test/core", // by module_key
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
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	assert_eq!(result["module"]["module_key"], "npm:@test/core");
	assert_eq!(result["module"]["canonical_root_path"], "packages/core");
}

// ── 7. Exact field assertions ────────────────────────────────────

#[test]
fn modules_show_exact_fields() {
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
		"npm_package",
		Some("@test/core"),
		0.95,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"show",
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
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// Module identity fields
	let m = &result["module"];
	assert_eq!(m["module_uid"], "mc-core");
	assert_eq!(m["module_key"], "npm:@test/core");
	assert_eq!(m["canonical_root_path"], "packages/core");
	assert_eq!(m["module_kind"], "npm_package");
	assert_eq!(m["display_name"], "@test/core");
	assert!((m["confidence"].as_f64().unwrap() - 0.95).abs() < 0.001);

	// Rollups present
	let r = &result["rollups"];
	assert!(r["owned_file_count"].is_number());
	assert!(r["owned_test_file_count"].is_number());
	assert!(r["outbound_dependency_count"].is_number());
	assert!(r["outbound_import_count"].is_number());
	assert!(r["inbound_dependency_count"].is_number());
	assert!(r["inbound_import_count"].is_number());
	assert!(r["violation_count"].is_number()); // 0 when no violations
	assert!(r["dead_symbol_count"].is_number());
	assert!(r["dead_test_symbol_count"].is_number());

	// Neighbor arrays present (may be empty)
	assert!(result["outbound_dependencies"].is_array());
	assert!(result["inbound_dependencies"].is_array());

	// Violations array present (empty when no violations)
	assert!(result["violations"].is_array());

	// Degradation fields
	assert_eq!(result["rollups_degraded"], false);
	assert!(result["warnings"].is_array());
	assert!(result["warnings"].as_array().unwrap().is_empty());
}

// ── 8. Degradation on malformed policy ───────────────────────────

#[test]
fn modules_show_degrades_on_malformed_boundary() {
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
		"npm_package",
		Some("@test/core"),
		0.95,
	);

	// Insert malformed boundary declaration
	insert_raw_declaration(
		&db_path,
		"decl-bad",
		"test-repo",
		"boundary",
		r#"{"source": "invalid-selector-domain:foo", "forbids": "also:invalid"}"#,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"packages/core",
		])
		.output()
		.unwrap();

	// Command still succeeds — degradation is advisory
	assert_eq!(
		output.status.code(),
		Some(0),
		"modules show must succeed even with malformed policy, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// Verify degradation state
	assert_eq!(result["rollups_degraded"], true);
	let warnings = result["warnings"].as_array().expect("warnings must be array");
	assert!(!warnings.is_empty(), "must have warning message");
	assert!(
		warnings[0].as_str().unwrap().contains("unavailable"),
		"warning should mention unavailable: {:?}",
		warnings
	);

	// Module identity still available
	assert_eq!(result["module"]["module_uid"], "mc-core");

	// Non-policy rollups still populated
	assert!(result["rollups"]["owned_file_count"].is_number());
	assert!(result["rollups"]["dead_symbol_count"].is_number());

	// Policy-derived fields are null
	assert!(
		result["rollups"]["violation_count"].is_null(),
		"violation_count must be null when policy unavailable"
	);
	assert!(
		result["violations"].is_null(),
		"violations must be null when policy unavailable"
	);
}

// ── 9. Envelope contract ─────────────────────────────────────────

#[test]
fn modules_show_envelope_contract() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		&snapshot_uid,
		"test-repo",
		"mc-core",
		"npm:@test/core",
		"packages/core",
		"npm_package",
		Some("@test/core"),
		0.95,
	);

	let output = Command::new(binary_path())
		.args([
			"modules",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"packages/core",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));

	let stdout = String::from_utf8_lossy(&output.stdout);
	let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

	// Standard envelope fields
	assert_eq!(result["command"], "modules show");
	assert_eq!(result["repo"], "test-repo");
	assert!(result["snapshot"].is_string());
	assert!(result["snapshot_scope"].is_string());
	assert!(result["stale"].is_boolean());

	// Show-specific fields (no results/count)
	assert!(result.get("results").is_none(), "show should not have results array");
	assert!(result.get("count").is_none(), "show should not have count");

	// Required show fields
	assert!(result["module"].is_object());
	assert!(result["rollups"].is_object());
	assert!(result["outbound_dependencies"].is_array());
	assert!(result["inbound_dependencies"].is_array());
	// violations can be array or null
	assert!(result["violations"].is_array() || result["violations"].is_null());
	assert!(result["rollups_degraded"].is_boolean());
	assert!(result["warnings"].is_array());
}
