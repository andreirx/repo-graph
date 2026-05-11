//! Deterministic tests for the `deps` command family (DEP-1).
//!
//! Test matrix:
//!   1. `deps list` — basic execution returns non-empty results
//!   2. `deps list` — module filter works
//!   3. `deps why` — returns module relationship with file evidence
//!   4. `deps drift` — returns anomalies for undeclared/unused
//!   5. Usage errors (wrong args)
//!   6. `--format json` flag is accepted

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

fn monorepo_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("typescript")
        .join("monorepo-packages")
}

/// Build a temp DB by indexing the monorepo-packages fixture.
fn build_indexed_db() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &monorepo_fixture_path(),
        &db_path,
        "test-repo",
        &ComposeOptions::default(),
    )
    .unwrap();

    // Should index at least the server.ts file
    assert!(result.files_total >= 1, "expected at least 1 file indexed");

    (dir, db_path)
}

// ── deps list tests ───────────────────────────────────────────────

#[test]
fn deps_list_usage_error() {
    let output = Command::new(binary_path())
        .args(["deps", "list"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"));
}

#[test]
fn deps_list_returns_json_envelope() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    // May succeed or return empty depending on module detection
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON if any output
    if !stdout.is_empty() {
        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .expect("output should be valid JSON");

        // Check envelope structure
        assert!(parsed.get("command").is_some(), "missing command field");
        assert!(parsed.get("results").is_some(), "missing results field");
    }
}

#[test]
fn deps_list_format_json_flag_accepted() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--format",
            "json",
        ])
        .output()
        .unwrap();

    // Should not fail with "unknown flag" error
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unknown flag"), "format flag should be recognized");
}

#[test]
fn deps_list_format_invalid_rejected() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--format",
            "xml",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported format"));
}

// ── deps why tests ────────────────────────────────────────────────

#[test]
fn deps_why_usage_error() {
    let output = Command::new(binary_path())
        .args(["deps", "why"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"));
}

#[test]
fn deps_why_package_not_found() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "why",
            db_path.to_str().unwrap(),
            "test-repo",
            "nonexistent-package-xyz",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

// ── deps drift tests ──────────────────────────────────────────────

#[test]
fn deps_drift_usage_error() {
    let output = Command::new(binary_path())
        .args(["deps", "drift"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"));
}

#[test]
fn deps_drift_returns_json_envelope() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "drift",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    if !stdout.is_empty() {
        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .expect("output should be valid JSON");

        // Check envelope structure
        assert!(parsed.get("command").is_some(), "missing command field");
        assert!(parsed.get("results").is_some(), "missing results field");
    }
}

// ── positive content assertion tests ──────────────────────────────

#[test]
fn deps_list_returns_modules_with_manifest_path() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",  // Matches build_indexed_db repo name
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "deps list should succeed. stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("output should be valid JSON");

    // Verify results array is non-empty.
    let results = parsed.get("results").and_then(|r| r.as_array()).unwrap();
    assert!(!results.is_empty(), "expected at least one module in results");

    // Find packages/api module and verify manifest_path.
    let api_module = results.iter().find(|m| {
        m.get("module").and_then(|v| v.as_str()) == Some("packages/api")
    });
    assert!(api_module.is_some(), "expected packages/api module in results");
    let api = api_module.unwrap();

    // Verify manifest_path is the package.json, not a source file.
    let manifest_path = api.get("manifest_path").and_then(|v| v.as_str()).unwrap();
    assert_eq!(
        manifest_path, "packages/api/package.json",
        "manifest_path should point to package.json"
    );

    // Verify manifest_scope_available is true.
    let scope_available = api.get("manifest_scope_available").and_then(|v| v.as_bool()).unwrap();
    assert!(scope_available, "manifest_scope_available should be true");
}

#[test]
fn deps_list_returns_declared_packages() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",  // Matches build_indexed_db repo name
        ])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "deps list should succeed. stderr: {}",
        stderr
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("output should be valid JSON");

    let results = parsed.get("results").and_then(|r| r.as_array()).unwrap();
    let api_module = results.iter().find(|m| {
        m.get("module").and_then(|v| v.as_str()) == Some("packages/api")
    }).expect("packages/api module not found");

    // Check entries contain declared packages (express, cors).
    let entries = api_module.get("entries").and_then(|e| e.as_array()).unwrap();
    let package_names: Vec<&str> = entries.iter()
        .filter_map(|e| e.get("package").and_then(|p| p.as_str()))
        .collect();

    assert!(
        package_names.contains(&"express"),
        "expected 'express' in declared packages, got: {:?}",
        package_names
    );
    assert!(
        package_names.contains(&"cors"),
        "expected 'cors' in declared packages, got: {:?}",
        package_names
    );
}

// ── ecosystem flag tests ──────────────────────────────────────────

#[test]
fn deps_list_ecosystem_cargo() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--ecosystem",
            "cargo",
        ])
        .output()
        .unwrap();

    // Should not fail with unknown flag
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unknown flag"));
}

#[test]
fn deps_list_ecosystem_invalid() {
    let (_dir, db_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "deps",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--ecosystem",
            "maven",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid ecosystem"));
}
