//! Deterministic tests for the `docs` command.
//!
//! Tests semantic fact extraction from documentation files.
//!
//! Test matrix:
//!   1. Usage error (wrong args)
//!   2. DB open failure (missing file)
//!   3. Repo not found (wrong repo_uid)
//!   4. Success with basic extraction
//!   5. Extraction with explicit markers

use std::fs::{self, File};
use std::io::Write;
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

/// Create a minimal repo with documentation files.
fn create_test_repo(dir: &std::path::Path) {
    // Create README with a marker
    let readme = dir.join("README.md");
    let mut f = File::create(&readme).unwrap();
    writeln!(f, "# Test Repo").unwrap();
    writeln!(f).unwrap();
    writeln!(f, "<!-- rg:replaces old-module -->").unwrap();
    writeln!(f).unwrap();
    writeln!(f, "This is a test repository.").unwrap();
}

/// Build a temp DB by indexing a minimal test repo.
fn build_indexed_db_with_docs() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_test_repo(&repo_path);

    // Also create a minimal source file so indexing works
    let src = repo_path.join("index.ts");
    let mut f = File::create(&src).unwrap();
    writeln!(f, "export function main() {{}}").unwrap();

    let db_path = dir.path().join("test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &repo_path,
        &db_path,
        "test-repo",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert!(result.files_total >= 1);

    (dir, db_path, repo_path)
}

// ── 1. Usage error ───────────────────────────────────────────────

#[test]
fn docs_usage_error_exit_1() {
    let output = Command::new(binary_path())
        .args(["docs"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ── 2. DB open failure ───────────────────────────────────────────

#[test]
fn docs_missing_db_exit_2() {
    let output = Command::new(binary_path())
        .args(["docs", "/nonexistent/path.db", "repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout must be empty on error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ── 3. Repo not found ────────────────────────────────────────────

#[test]
fn docs_repo_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_docs();

    let output = Command::new(binary_path())
        .args([
            "docs",
            db_path.to_str().unwrap(),
            "nonexistent-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout must be empty on error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found"),
        "stderr: {}",
        stderr
    );
}

// ── 4. Success with basic extraction ─────────────────────────────

#[test]
fn docs_success_extracts_markers() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_docs();

    let output = Command::new(binary_path())
        .args([
            "docs",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Success: JSON on stdout, nothing on stderr
    assert!(
        output.stderr.is_empty(),
        "stderr must be empty on success, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // ── Field assertions ─────────────────────────────────────
    assert_eq!(result["command"], "docs");
    assert_eq!(result["repo"], "test-repo");

    // Should have scanned at least the README
    assert!(
        result["files_scanned"].as_u64().unwrap() >= 1,
        "files_scanned: {:?}",
        result["files_scanned"]
    );

    // Should have extracted at least one fact (the rg:replaces marker)
    assert!(
        result["facts_extracted"].as_u64().unwrap() >= 1,
        "facts_extracted: {:?}",
        result["facts_extracted"]
    );

    // Counts by kind should include replacement_for
    let counts = &result["counts_by_kind"];
    assert!(
        counts["replacement_for"].as_u64().unwrap_or(0) >= 1,
        "counts_by_kind: {:?}",
        counts
    );
}

// ── 5. Facts persist and can be re-queried ───────────────────────

#[test]
fn docs_facts_persist_in_storage() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_docs();

    // Run docs command
    let output = Command::new(binary_path())
        .args([
            "docs",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    // Open storage and verify facts were persisted
    let storage = repo_graph_storage::StorageConnection::open(&db_path).unwrap();
    let facts = storage.get_semantic_facts_for_repo("test-repo").unwrap();

    assert!(
        !facts.is_empty(),
        "facts should be persisted in storage"
    );

    // Verify the replacement_for fact exists
    let replacement_facts: Vec<_> = facts
        .iter()
        .filter(|f| f.fact_kind == "replacement_for")
        .collect();
    assert!(
        !replacement_facts.is_empty(),
        "replacement_for fact should be persisted"
    );

    // Verify the fact has correct subject (inferred from README.md path)
    let fact = &replacement_facts[0];
    assert_eq!(fact.object_ref, Some("old-module".to_string()));
    assert_eq!(fact.extraction_method, "explicit_marker");
}
