//! Deterministic tests for the `surfaces list` and `surfaces show` commands.
//!
//! Test matrix:
//!   1. surfaces list - usage error (wrong args)
//!   2. surfaces list - DB open failure (missing file)
//!   3. surfaces list - repo not found
//!   4. surfaces list - empty result (valid for repos without surfaces)
//!   5. surfaces list - non-empty result with filtering
//!   6. surfaces list - deterministic ordering
//!   7. surfaces show - usage error
//!   8. surfaces show - surface not found
//!   9. surfaces show - exact surface detail
//!  10. surfaces show - module enrichment via UID lookup
//!  11. repo-ref resolution (by name instead of UID)

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
	assert!(result.files_total >= 1);

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

/// Insert a module candidate for testing.
fn insert_module_candidate(
	db_path: &std::path::Path,
	module_candidate_uid: &str,
	snapshot_uid: &str,
	repo_uid: &str,
	module_key: &str,
	canonical_root_path: &str,
	display_name: Option<&str>,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.execute(
		"INSERT INTO module_candidates
		 (module_candidate_uid, snapshot_uid, repo_uid, module_key,
		  module_kind, canonical_root_path, confidence, display_name, metadata_json)
		 VALUES (?, ?, ?, ?, 'npm', ?, 0.9, ?, NULL)",
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

/// Insert a project surface for testing.
#[allow(clippy::too_many_arguments)]
fn insert_surface(
	db_path: &std::path::Path,
	project_surface_uid: &str,
	snapshot_uid: &str,
	repo_uid: &str,
	module_candidate_uid: &str,
	surface_kind: &str,
	display_name: Option<&str>,
	root_path: &str,
	entrypoint_path: Option<&str>,
	build_system: &str,
	runtime_kind: &str,
	confidence: f64,
	source_type: Option<&str>,
	source_specific_id: Option<&str>,
	stable_surface_key: Option<&str>,
	metadata_json: Option<&str>,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.execute(
		"INSERT INTO project_surfaces
		 (project_surface_uid, snapshot_uid, repo_uid, module_candidate_uid,
		  surface_kind, display_name, root_path, entrypoint_path,
		  build_system, runtime_kind, confidence, metadata_json,
		  source_type, source_specific_id, stable_surface_key)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
		rusqlite::params![
			project_surface_uid,
			snapshot_uid,
			repo_uid,
			module_candidate_uid,
			surface_kind,
			display_name,
			root_path,
			entrypoint_path,
			build_system,
			runtime_kind,
			confidence,
			metadata_json,
			source_type,
			source_specific_id,
			stable_surface_key,
		],
	)
	.expect("insert surface");
}

/// Insert project surface evidence for testing.
fn insert_evidence(
	db_path: &std::path::Path,
	evidence_uid: &str,
	surface_uid: &str,
	snapshot_uid: &str,
	repo_uid: &str,
	source_type: &str,
	source_path: &str,
	evidence_kind: &str,
	confidence: f64,
) {
	let conn = rusqlite::Connection::open(db_path).unwrap();
	conn.execute(
		"INSERT INTO project_surface_evidence
		 (project_surface_evidence_uid, project_surface_uid, snapshot_uid, repo_uid,
		  source_type, source_path, evidence_kind, confidence, payload_json)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)",
		rusqlite::params![
			evidence_uid,
			surface_uid,
			snapshot_uid,
			repo_uid,
			source_type,
			source_path,
			evidence_kind,
			confidence,
		],
	)
	.expect("insert evidence");
}

// ════════════════════════════════════════════════════════════════════
// surfaces list tests
// ════════════════════════════════════════════════════════════════════

#[test]
fn surfaces_list_usage_error() {
	let output = Command::new(binary_path())
		.args(["surfaces", "list"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn surfaces_list_missing_db() {
	let output = Command::new(binary_path())
		.args(["surfaces", "list", "/nonexistent/path.db", "repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	assert!(output.stdout.is_empty(), "stdout must be empty on error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn surfaces_list_repo_not_found() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"list",
			db_path.to_str().unwrap(),
			"nonexistent-repo",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(2));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("repo not found") || stderr.contains("no snapshot"),
		"stderr: {}",
		stderr
	);
}

#[test]
fn surfaces_list_empty_result() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args(["surfaces", "list", db_path.to_str().unwrap(), "test-repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0), "exit 0 for empty result");
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["command"], "surfaces list");
	assert_eq!(json["repo"], "test-repo");
	assert_eq!(json["results"].as_array().unwrap().len(), 0);
	assert_eq!(json["count"], 0);
}

#[test]
fn surfaces_list_with_surfaces() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert module candidate first (FK constraint).
	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		Some("@test/core"),
	);

	// Insert surfaces.
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		Some("Core Library"),
		"packages/core",
		Some("packages/core/src/index.ts"),
		"npm",
		"node",
		0.95,
		Some("package_json"),
		Some("npm:@test/core"),
		Some("test-repo:packages/core:library"),
		Some(r#"{"version":"1.0.0"}"#),
	);
	insert_surface(
		&db_path,
		"surf-002",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"cli",
		Some("CLI Tool"),
		"packages/core",
		Some("packages/core/bin/cli.ts"),
		"npm",
		"node",
		0.85,
		Some("package_json"),
		Some("npm:@test/core:bin"),
		Some("test-repo:packages/core:cli"),
		None,
	);

	let output = Command::new(binary_path())
		.args(["surfaces", "list", db_path.to_str().unwrap(), "test-repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["count"], 2);
	let results = json["results"].as_array().unwrap();
	assert_eq!(results.len(), 2);

	// Check first surface (cli comes before library in ordering: root_path ASC, surface_kind ASC).
	let cli = &results[0];
	assert_eq!(cli["surface_kind"], "cli");
	assert_eq!(cli["display_name"], "CLI Tool");
	assert_eq!(cli["module_display_name"], "@test/core");
	assert_eq!(cli["module_root_path"], "packages/core");
	assert_eq!(cli["source_type"], "package_json");
	assert_eq!(cli["source_specific_id"], "npm:@test/core:bin");
	assert_eq!(cli["stable_surface_key"], "test-repo:packages/core:cli");

	// Check second surface (library).
	let lib = &results[1];
	assert_eq!(lib["surface_kind"], "library");
	assert_eq!(lib["display_name"], "Core Library");
}

#[test]
fn surfaces_list_with_kind_filter() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		Some("@test/core"),
	);

	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		Some("Lib"),
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);
	insert_surface(
		&db_path,
		"surf-002",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"cli",
		Some("CLI"),
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"list",
			db_path.to_str().unwrap(),
			"test-repo",
			"--kind",
			"library",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["count"], 1);
	assert_eq!(json["filter_kind"], "library");
	assert_eq!(json["results"][0]["surface_kind"], "library");
}

#[test]
fn surfaces_list_evidence_count() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);

	// Insert 3 evidence items.
	insert_evidence(
		&db_path,
		"ev-001",
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"package_json",
		"packages/core/package.json",
		"main_field",
		0.9,
	);
	insert_evidence(
		&db_path,
		"ev-002",
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"package_json",
		"packages/core/package.json",
		"exports_field",
		0.85,
	);
	insert_evidence(
		&db_path,
		"ev-003",
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"tsconfig",
		"packages/core/tsconfig.json",
		"lib_output",
		0.8,
	);

	let output = Command::new(binary_path())
		.args(["surfaces", "list", db_path.to_str().unwrap(), "test-repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["results"][0]["evidence_count"], 3);
}

// ════════════════════════════════════════════════════════════════════
// surfaces show tests
// ════════════════════════════════════════════════════════════════════

#[test]
fn surfaces_show_usage_error() {
	let output = Command::new(binary_path())
		.args(["surfaces", "show"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn surfaces_show_surface_not_found() {
	let (_dir, db_path) = build_indexed_db();

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"nonexistent-surface",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

#[test]
fn surfaces_show_by_uid() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		Some("@test/core"),
	);

	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		Some("Core Library"),
		"packages/core",
		Some("packages/core/src/index.ts"),
		"npm",
		"node",
		0.95,
		Some("package_json"),
		Some("npm:@test/core"),
		Some("test-repo:packages/core:library"),
		Some(r#"{"version":"1.0.0"}"#),
	);

	insert_evidence(
		&db_path,
		"ev-001",
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"package_json",
		"packages/core/package.json",
		"main_field",
		0.9,
	);

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"surf-001",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	// Check surface detail.
	let surface = &json["surface"];
	assert_eq!(surface["project_surface_uid"], "surf-001");
	assert_eq!(surface["surface_kind"], "library");
	assert_eq!(surface["display_name"], "Core Library");
	assert_eq!(surface["root_path"], "packages/core");
	assert_eq!(surface["entrypoint_path"], "packages/core/src/index.ts");
	assert_eq!(surface["build_system"], "npm");
	assert_eq!(surface["runtime_kind"], "node");
	assert_eq!(surface["confidence"], 0.95);
	assert_eq!(surface["source_type"], "package_json");
	assert_eq!(surface["source_specific_id"], "npm:@test/core");
	assert_eq!(surface["stable_surface_key"], "test-repo:packages/core:library");
	// metadata_json is parsed (new shape: parsed/raw/parse_error).
	assert_eq!(surface["metadata_json"]["parsed"]["version"], "1.0.0");
	assert!(surface["metadata_json"]["raw"].is_null());
	assert!(surface["metadata_json"]["parse_error"].is_null());

	// Check module.
	let module = &json["module"];
	assert_eq!(module["module_candidate_uid"], "mod-001");
	assert_eq!(module["module_key"], "npm:@test/core");
	assert_eq!(module["display_name"], "@test/core");
	assert_eq!(module["canonical_root_path"], "packages/core");

	// Check evidence.
	let evidence = json["evidence"].as_array().unwrap();
	assert_eq!(evidence.len(), 1);
	assert_eq!(evidence[0]["source_type"], "package_json");
	assert_eq!(evidence[0]["source_path"], "packages/core/package.json");
	assert_eq!(evidence[0]["evidence_kind"], "main_field");
}

#[test]
fn surfaces_show_by_stable_key() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		Some("test-repo:packages/core:library"),
		None,
	);

	// Resolve by stable_surface_key.
	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"test-repo:packages/core:library",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
	assert_eq!(json["surface"]["project_surface_uid"], "surf-001");
}

// ════════════════════════════════════════════════════════════════════
// repo-ref resolution tests
// ════════════════════════════════════════════════════════════════════

#[test]
fn surfaces_list_resolve_by_repo_name() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Update repo name to something different from UID.
	let conn = rusqlite::Connection::open(&db_path).unwrap();
	conn.execute(
		"UPDATE repos SET name = 'My Test Repository' WHERE repo_uid = 'test-repo'",
		[],
	)
	.unwrap();

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);

	// Use repo name instead of UID.
	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"list",
			db_path.to_str().unwrap(),
			"My Test Repository",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
	assert_eq!(json["count"], 1);
	// The repo name in the envelope is the display name.
	assert_eq!(json["repo"], "My Test Repository");
}

#[test]
fn surfaces_list_resolve_by_root_path() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Get the root_path from the repo.
	let conn = rusqlite::Connection::open(&db_path).unwrap();
	let root_path: String = conn
		.query_row("SELECT root_path FROM repos WHERE repo_uid = 'test-repo'", [], |r| r.get(0))
		.unwrap();

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);

	// Use root_path instead of UID.
	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"list",
			db_path.to_str().unwrap(),
			&root_path,
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
	assert_eq!(json["count"], 1);
}

// ════════════════════════════════════════════════════════════════════
// Filter tests (--runtime, --source, --module)
// ════════════════════════════════════════════════════════════════════

#[test]
fn surfaces_list_with_runtime_filter() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	// Insert surfaces with different runtimes.
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);
	insert_surface(
		&db_path,
		"surf-002",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/browser",
		None,
		"npm",
		"browser",
		0.9,
		None,
		None,
		None,
		None,
	);

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"list",
			db_path.to_str().unwrap(),
			"test-repo",
			"--runtime",
			"browser",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["count"], 1);
	assert_eq!(json["filter_runtime"], "browser");
	assert_eq!(json["results"][0]["runtime_kind"], "browser");
}

#[test]
fn surfaces_list_with_source_filter() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	// Insert surfaces with different source_types.
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		Some("package_json"),
		None,
		None,
		None,
	);
	insert_surface(
		&db_path,
		"surf-002",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/other",
		None,
		"cargo",
		"native",
		0.9,
		Some("cargo_toml"),
		None,
		None,
		None,
	);

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"list",
			db_path.to_str().unwrap(),
			"test-repo",
			"--source",
			"cargo_toml",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["count"], 1);
	assert_eq!(json["filter_source"], "cargo_toml");
	assert_eq!(json["results"][0]["source_type"], "cargo_toml");
}

#[test]
fn surfaces_list_with_module_filter() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	// Insert two modules.
	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);
	insert_module_candidate(
		&db_path,
		"mod-002",
		&snapshot_uid,
		"test-repo",
		"npm:@test/utils",
		"packages/utils",
		None,
	);

	// Insert surfaces for different modules.
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);
	insert_surface(
		&db_path,
		"surf-002",
		&snapshot_uid,
		"test-repo",
		"mod-002",
		"library",
		None,
		"packages/utils",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"list",
			db_path.to_str().unwrap(),
			"test-repo",
			"--module",
			"mod-002",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["count"], 1);
	assert_eq!(json["filter_module"], "mod-002");
	assert_eq!(json["results"][0]["module_candidate_uid"], "mod-002");
}

// ════════════════════════════════════════════════════════════════════
// Legacy NULL identity fields
// ════════════════════════════════════════════════════════════════════

#[test]
fn surfaces_list_legacy_null_identity_fields() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	// Insert surface with NULL identity fields (legacy row).
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None, // source_type = NULL
		None, // source_specific_id = NULL
		None, // stable_surface_key = NULL
		None,
	);

	let output = Command::new(binary_path())
		.args(["surfaces", "list", db_path.to_str().unwrap(), "test-repo"])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	assert_eq!(json["count"], 1);
	let surface = &json["results"][0];
	assert!(surface["source_type"].is_null());
	assert!(surface["source_specific_id"].is_null());
	assert!(surface["stable_surface_key"].is_null());
}

#[test]
fn surfaces_show_legacy_null_identity_fields() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	// Insert surface with NULL identity fields (legacy row).
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"surf-001",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	let surface = &json["surface"];
	assert!(surface["source_type"].is_null());
	assert!(surface["source_specific_id"].is_null());
	assert!(surface["stable_surface_key"].is_null());
}

// ════════════════════════════════════════════════════════════════════
// Invalid metadata handling (raw preservation)
// ════════════════════════════════════════════════════════════════════

#[test]
fn surfaces_show_invalid_metadata_preserves_raw() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	// Insert surface with invalid JSON in metadata_json.
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		Some("{this is not valid json}"), // Invalid JSON
	);

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"surf-001",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	let metadata = &json["surface"]["metadata_json"];
	// parsed should be null when invalid.
	assert!(metadata["parsed"].is_null());
	// raw should contain the original invalid string.
	assert_eq!(metadata["raw"], "{this is not valid json}");
	// parse_error should contain the error message.
	assert!(metadata["parse_error"].is_string());
	assert!(
		metadata["parse_error"].as_str().unwrap().contains("key must be a string"),
		"parse_error: {}",
		metadata["parse_error"]
	);
}

#[test]
fn surfaces_show_absent_metadata() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	// Insert surface with NULL metadata_json.
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None, // NULL metadata
	);

	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"surf-001",
		])
		.output()
		.unwrap();

	assert_eq!(output.status.code(), Some(0));
	let stdout = String::from_utf8_lossy(&output.stdout);
	let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

	let metadata = &json["surface"]["metadata_json"];
	// All fields should be null when metadata is absent.
	assert!(metadata["parsed"].is_null());
	assert!(metadata["raw"].is_null());
	assert!(metadata["parse_error"].is_null());
}

// ════════════════════════════════════════════════════════════════════
// Ambiguity handling
// ════════════════════════════════════════════════════════════════════

#[test]
fn surfaces_show_ambiguous_uid_prefix() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	// Insert two surfaces with UIDs that share a common prefix.
	insert_surface(
		&db_path,
		"ps-ambig-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);
	insert_surface(
		&db_path,
		"ps-ambig-002",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"cli",
		None,
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);

	// Query with ambiguous prefix that matches both.
	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"ps-ambig",
		])
		.output()
		.unwrap();

	// Should fail with exit code 1 (user error, not runtime error).
	assert_eq!(output.status.code(), Some(1));
	assert!(output.stdout.is_empty(), "stdout must be empty on ambiguity error");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("ambiguous"),
		"stderr should contain 'ambiguous': {}",
		stderr
	);
	assert!(
		stderr.contains("2 matches"),
		"stderr should contain '2 matches': {}",
		stderr
	);
}

#[test]
fn surfaces_show_ambiguous_display_name() {
	let (_dir, db_path) = build_indexed_db();
	let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

	insert_module_candidate(
		&db_path,
		"mod-001",
		&snapshot_uid,
		"test-repo",
		"npm:@test/core",
		"packages/core",
		None,
	);

	// Insert two surfaces with the same display_name.
	insert_surface(
		&db_path,
		"surf-001",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"library",
		Some("My Surface"), // same display_name
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);
	insert_surface(
		&db_path,
		"surf-002",
		&snapshot_uid,
		"test-repo",
		"mod-001",
		"cli",
		Some("My Surface"), // same display_name
		"packages/core",
		None,
		"npm",
		"node",
		0.9,
		None,
		None,
		None,
		None,
	);

	// Query by display_name which matches both.
	let output = Command::new(binary_path())
		.args([
			"surfaces",
			"show",
			db_path.to_str().unwrap(),
			"test-repo",
			"My Surface",
		])
		.output()
		.unwrap();

	// Should fail with exit code 1.
	assert_eq!(output.status.code(), Some(1));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("ambiguous"),
		"stderr should contain 'ambiguous': {}",
		stderr
	);
	assert!(
		stderr.contains("display_name"),
		"stderr should indicate resolution method: {}",
		stderr
	);
}
