//! Deterministic tests for the `modules list` command.
//!
//! RS-MG-9: Module catalog surface for Rust CLI.
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found (wrong repo_uid)
//!   4. Empty result (valid for repos without discovered modules)
//!   5. Non-empty result with exact field assertions
//!   6. Deterministic ordering by canonical_root_path

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
///
/// The Rust indexer populates `module_candidates` via inferred module
/// detection (top-level directory heuristic). After Phase 4 (2026-05-10),
/// there is no fallback to MODULE nodes - `module_candidates` is the
/// sole source of module topology.
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

/// Insert a module candidate directly for testing.
#[allow(clippy::too_many_arguments)]
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

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn modules_list_usage_error() {
    let output = Command::new(binary_path())
        .args(["modules", "list"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn modules_list_usage_error_missing_repo() {
    let output = Command::new(binary_path())
        .args(["modules", "list", "/some/path.db"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "stdout must be empty on usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn modules_list_missing_db() {
    let output = Command::new(binary_path())
        .args(["modules", "list", "/nonexistent/path.db", "repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout must be empty on error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn modules_list_repo_not_found() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "modules",
            "list",
            db_path.to_str().unwrap(),
            "nonexistent-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout must be empty on error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no snapshot found"), "stderr: {}", stderr);
}

// ── 4. Declared module from package.json ─────────────────────────
//
// The fixture has a package.json which creates a declared module at root.
// After Phase 4 (2026-05-10), there is no fallback to MODULE nodes -
// `module_candidates` is the sole source of module topology.

#[test]
fn modules_list_declared_from_package_json() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args(["modules", "list", db_path.to_str().unwrap(), "test-repo"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "declared modules result is success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        output.stderr.is_empty(),
        "stderr must be empty on success, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    assert_eq!(result["command"], "modules list");
    assert_eq!(result["repo"], "test-repo");
    // package.json at root creates a declared module at "."
    // The module comes from module_candidates, NOT from MODULE node fallback.
    assert_eq!(result["count"], 1);
    let modules = result["results"].as_array().expect("results is array");
    assert_eq!(modules.len(), 1);
    // The module is at root (declared from package.json)
    assert_eq!(modules[0]["canonical_root_path"], ".");
    assert_eq!(modules[0]["module_kind"], "declared");
}

// ── 5. Non-empty result with exact field assertions ──────────────

#[test]
fn modules_list_exact_fields() {
    let (_dir, db_path) = build_indexed_db();
    let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

    // Insert a module candidate (in addition to auto-detected root module)
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
        .args(["modules", "list", db_path.to_str().unwrap(), "test-repo"])
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

    assert_eq!(result["command"], "modules list");
    // 2 modules: auto-detected root module from package.json + inserted mc-core
    assert_eq!(result["count"], 2);

    let modules = result["results"].as_array().unwrap();
    assert_eq!(modules.len(), 2);

    // Find the inserted module (sorted by canonical_root_path, "." comes before "packages/core")
    let m = modules
        .iter()
        .find(|m| m["canonical_root_path"] == "packages/core")
        .expect("inserted module not found");
    // Verify identity fields in snake_case
    assert_eq!(m["module_uid"], "mc-core");
    assert_eq!(m["module_key"], "npm:@test/core");
    assert_eq!(m["canonical_root_path"], "packages/core");
    assert_eq!(m["module_kind"], "npm_package");
    assert_eq!(m["display_name"], "@test/core");
    assert!((m["confidence"].as_f64().unwrap() - 0.95).abs() < 0.001);

    // Verify rollup fields are present (RS-MG-12b)
    // Module has no owned files, no deps, no violations, no dead symbols
    assert_eq!(m["owned_file_count"], 0);
    assert_eq!(m["owned_test_file_count"], 0);
    assert_eq!(m["outbound_dependency_count"], 0);
    assert_eq!(m["outbound_import_count"], 0);
    assert_eq!(m["inbound_dependency_count"], 0);
    assert_eq!(m["inbound_import_count"], 0);
    assert_eq!(m["violation_count"], 0);
    assert_eq!(m["dead_symbol_count"], 0);
    assert_eq!(m["dead_test_symbol_count"], 0);

    // Verify internal fields are NOT exposed
    assert!(
        m.get("snapshot_uid").is_none(),
        "snapshot_uid must not be in output"
    );
    assert!(
        m.get("repo_uid").is_none(),
        "repo_uid must not be in output"
    );
    assert!(
        m.get("metadata_json").is_none(),
        "metadata_json must not be in output"
    );
}

// ── 6. Deterministic ordering ────────────────────────────────────

#[test]
fn modules_list_sorted_by_canonical_path() {
    let (_dir, db_path) = build_indexed_db();
    let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

    // Insert in non-alphabetical order
    insert_module_candidate(
        &db_path,
        &snapshot_uid,
        "test-repo",
        "mc-zebra",
        "npm:@test/zebra",
        "packages/zebra",
        "npm_package",
        Some("@test/zebra"),
        1.0,
    );
    insert_module_candidate(
        &db_path,
        &snapshot_uid,
        "test-repo",
        "mc-alpha",
        "npm:@test/alpha",
        "packages/alpha",
        "npm_package",
        Some("@test/alpha"),
        1.0,
    );
    insert_module_candidate(
        &db_path,
        &snapshot_uid,
        "test-repo",
        "mc-beta",
        "npm:@test/beta",
        "packages/beta",
        "npm_package",
        Some("@test/beta"),
        1.0,
    );

    let output = Command::new(binary_path())
        .args(["modules", "list", db_path.to_str().unwrap(), "test-repo"])
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

    let modules = result["results"].as_array().unwrap();
    // 4 modules: auto-detected root "." + 3 inserted
    assert_eq!(modules.len(), 4);

    // Sorted by canonical_root_path ascending
    // "." comes before "packages/*"
    assert_eq!(modules[0]["canonical_root_path"], ".");
    assert_eq!(modules[1]["canonical_root_path"], "packages/alpha");
    assert_eq!(modules[2]["canonical_root_path"], "packages/beta");
    assert_eq!(modules[3]["canonical_root_path"], "packages/zebra");
}

// ── 7. Envelope contract ─────────────────────────────────────────

#[test]
fn modules_list_envelope_contract() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args(["modules", "list", db_path.to_str().unwrap(), "test-repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Standard envelope fields
    assert_eq!(result["command"], "modules list");
    assert_eq!(result["repo"], "test-repo");
    assert!(result["snapshot"].is_string());
    assert!(result["snapshot_scope"].is_string());
    assert!(result["count"].is_number());
    assert!(result["stale"].is_boolean());
    assert!(result["results"].is_array());

    // Degradation envelope fields (always present)
    assert_eq!(
        result["rollups_degraded"], false,
        "rollups_degraded must be false when no policy errors"
    );
    assert!(result["warnings"].is_array(), "warnings must be an array");
    assert!(
        result["warnings"].as_array().unwrap().is_empty(),
        "warnings must be empty when no errors"
    );
}

// ── 8. Rollup fields with actual data ────────────────────────────

/// Insert a file into the files table.
fn insert_file(
    db_path: &std::path::Path,
    repo_uid: &str,
    file_uid: &str,
    path: &str,
    is_test: bool,
) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute(
        "INSERT INTO files (file_uid, repo_uid, path, language, is_test, is_generated, is_excluded)
		 VALUES (?, ?, ?, 'typescript', ?, 0, 0)",
        rusqlite::params![file_uid, repo_uid, path, if is_test { 1 } else { 0 }],
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
) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute(
        "INSERT INTO module_file_ownership
		 (snapshot_uid, repo_uid, file_uid, module_candidate_uid,
		  assignment_kind, confidence, basis_json)
		 VALUES (?, ?, ?, ?, 'manifest', 1.0, NULL)",
        rusqlite::params![snapshot_uid, repo_uid, file_uid, module_candidate_uid],
    )
    .expect("insert file ownership");
}

#[test]
fn modules_list_rollup_with_owned_files() {
    let (_dir, db_path) = build_indexed_db();
    let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

    // Insert module
    insert_module_candidate(
        &db_path,
        &snapshot_uid,
        "test-repo",
        "mc-app",
        "npm:@test/app",
        "packages/app",
        "npm_package",
        Some("@test/app"),
        1.0,
    );

    // Insert files (2 non-test, 1 test)
    insert_file(&db_path, "test-repo", "f1", "packages/app/index.ts", false);
    insert_file(
        &db_path,
        "test-repo",
        "f2",
        "packages/app/service.ts",
        false,
    );
    insert_file(
        &db_path,
        "test-repo",
        "f3",
        "packages/app/index.test.ts",
        true,
    );

    // Assign files to module
    insert_file_ownership(&db_path, &snapshot_uid, "test-repo", "f1", "mc-app");
    insert_file_ownership(&db_path, &snapshot_uid, "test-repo", "f2", "mc-app");
    insert_file_ownership(&db_path, &snapshot_uid, "test-repo", "f3", "mc-app");

    let output = Command::new(binary_path())
        .args(["modules", "list", db_path.to_str().unwrap(), "test-repo"])
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

    let modules = result["results"].as_array().unwrap();
    // 2 modules: auto-detected root "." + inserted mc-app
    assert_eq!(modules.len(), 2);

    // Find the inserted module
    let m = modules
        .iter()
        .find(|m| m["module_uid"] == "mc-app")
        .expect("inserted module not found");
    assert_eq!(m["owned_file_count"], 2, "2 non-test files");
    assert_eq!(m["owned_test_file_count"], 1, "1 test file");
}

// ── 9. Degraded mode on malformed policy ─────────────────────────

/// Insert a raw declaration for testing malformed policy.
fn insert_raw_declaration(
    db_path: &std::path::Path,
    declaration_uid: &str,
    repo_uid: &str,
    kind: &str,
    value_json: &str,
) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    // Fixed timestamp is fine for tests — just needs to be valid ISO 8601
    let now = "2026-01-01T00:00:00Z";
    conn.execute(
        "INSERT INTO declarations
		 (declaration_uid, repo_uid, target_stable_key, kind, value_json, created_at, is_active)
		 VALUES (?, ?, '', ?, ?, ?, 1)",
        rusqlite::params![declaration_uid, repo_uid, kind, value_json, now],
    )
    .expect("insert declaration");
}

#[test]
fn modules_list_degrades_on_malformed_boundary() {
    let (_dir, db_path) = build_indexed_db();
    let snapshot_uid = get_snapshot_uid(&db_path, "test-repo");

    // Insert a module to have something in the catalog
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

    // Insert a malformed boundary declaration (invalid JSON structure)
    insert_raw_declaration(
        &db_path,
        "decl-bad",
        "test-repo",
        "boundary",
        r#"{"source": "invalid-selector-domain:foo", "forbids": "also:invalid"}"#,
    );

    let output = Command::new(binary_path())
        .args(["modules", "list", db_path.to_str().unwrap(), "test-repo"])
        .output()
        .unwrap();

    // Catalog still succeeds — orientation surface degrades gracefully
    assert_eq!(
        output.status.code(),
        Some(0),
        "modules list must succeed even with malformed policy, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // Verify degradation envelope
    assert_eq!(
        result["rollups_degraded"], true,
        "must be degraded on parse error"
    );
    let warnings = result["warnings"]
        .as_array()
        .expect("warnings must be array");
    assert!(!warnings.is_empty(), "must have warning message");
    assert!(
        warnings[0].as_str().unwrap().contains("unavailable"),
        "warning should mention unavailable: {:?}",
        warnings
    );

    // Catalog still returned
    let modules = result["results"].as_array().unwrap();
    // 2 modules: auto-detected root "." + inserted mc-core
    assert_eq!(modules.len(), 2);
    let mc_core = modules
        .iter()
        .find(|m| m["module_uid"] == "mc-core")
        .expect("inserted module not found");
    assert_eq!(mc_core["canonical_root_path"], "packages/core");

    // Non-policy rollups still populated
    assert_eq!(mc_core["owned_file_count"], 0);
    assert_eq!(mc_core["dead_symbol_count"], 0);

    // Policy-derived rollup is null (unknown, not zero)
    assert!(
        modules[0]["violation_count"].is_null(),
        "violation_count must be null when policy unavailable, got: {:?}",
        modules[0]["violation_count"]
    );
}

// ── 10. Phase 4: No fallback to legacy MODULE nodes ──────────────
//
// After Phase 4 (2026-05-10), empty module_candidates returns empty
// results even when legacy MODULE nodes exist in the nodes table.
// This is the critical no-fallback behavior test.

/// Build a DB with only legacy MODULE nodes (no module_candidates).
/// This simulates a pre-Phase-4 indexed snapshot or manual node insertion.
fn build_db_with_legacy_module_nodes_only() -> (tempfile::TempDir, PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // Create DB with all migrations via StorageConnection, then close it
    {
        let _storage = repo_graph_storage::StorageConnection::open(&db_path).unwrap();
        // StorageConnection runs all migrations on open
    }

    // Reopen with raw rusqlite to insert test data manually
    let conn = rusqlite::Connection::open(&db_path).unwrap();

    // Insert repo
    conn.execute(
        "INSERT INTO repos (repo_uid, name, root_path, created_at) \
		 VALUES ('legacy-repo', 'legacy-test', '/tmp/legacy', '2026-05-10T00:00:00Z')",
        [],
    )
    .unwrap();

    // Insert snapshot
    let snapshot_uid = "legacy-repo/2026-05-10T00:00:00Z/test1234";
    conn.execute(
        "INSERT INTO snapshots \
		 (snapshot_uid, repo_uid, kind, status, files_total, nodes_total, edges_total, created_at) \
		 VALUES (?, 'legacy-repo', 'full', 'ready', 2, 3, 0, '2026-05-10T00:00:00Z')",
        [snapshot_uid],
    )
    .unwrap();

    // Insert legacy MODULE nodes (pre-Phase-4 Rust indexer style)
    // Using minimal columns required by schema: node_uid, snapshot_uid, repo_uid, stable_key, kind, name
    conn.execute(
		"INSERT INTO nodes \
		 (node_uid, snapshot_uid, repo_uid, stable_key, kind, name, qualified_name, visibility) \
		 VALUES ('mod-src', ?, 'legacy-repo', 'legacy-repo:src:MODULE', 'MODULE', 'src', 'src', 'public')",
		[snapshot_uid],
	).unwrap();
    conn.execute(
		"INSERT INTO nodes \
		 (node_uid, snapshot_uid, repo_uid, stable_key, kind, name, qualified_name, visibility) \
		 VALUES ('mod-lib', ?, 'legacy-repo', 'legacy-repo:lib:MODULE', 'MODULE', 'lib', 'lib', 'public')",
		[snapshot_uid],
	).unwrap();

    // Verify: module_candidates is empty, but MODULE nodes exist
    let mc_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM module_candidates WHERE snapshot_uid = ?",
            [snapshot_uid],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(mc_count, 0, "module_candidates must be empty for this test");

    let module_node_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM nodes WHERE snapshot_uid = ? AND kind = 'MODULE'",
            [snapshot_uid],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(module_node_count, 2, "should have 2 legacy MODULE nodes");

    drop(conn);
    (dir, db_path, snapshot_uid.to_string())
}

#[test]
fn modules_list_no_fallback_to_legacy_module_nodes() {
    let (_dir, db_path, _snapshot_uid) = build_db_with_legacy_module_nodes_only();

    let output = Command::new(binary_path())
        .args(["modules", "list", db_path.to_str().unwrap(), "legacy-repo"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "command should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // Phase 4 critical assertion: count must be 0, NOT 2
    // Legacy MODULE nodes are NOT used as fallback
    assert_eq!(
        result["count"], 0,
        "Phase 4: module_candidates is empty, so count must be 0. \
		 Legacy MODULE nodes must NOT be used as fallback. Got: {}",
        result["count"]
    );

    let modules = result["results"].as_array().expect("results is array");
    assert!(
        modules.is_empty(),
        "Phase 4: results array must be empty when module_candidates is empty. \
		 Legacy MODULE nodes (2 present) must NOT be returned. Got: {:?}",
        modules
    );
}
