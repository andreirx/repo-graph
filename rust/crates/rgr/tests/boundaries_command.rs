//! Deterministic tests for the `boundaries` command family.
//!
//! BI-1A: Boundary interaction discovery read-side.
//!
//! Test matrix:
//!   1. Usage errors (wrong args, missing subcommand)
//!   2. DB open failure (missing file)
//!   3. Repo not found (wrong repo_uid)
//!   4. List success / empty results
//!   5. List with filters (--symbol, --kind, --file, etc.)
//!   6. Show success / not found
//!   7. Summary success / empty

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

/// Create a minimal repo with a C file containing boundary interaction code.
fn create_test_repo_with_socket(dir: &std::path::Path) {
    let src = dir.join("server.c");
    let mut f = File::create(&src).unwrap();
    // This triggers UnixSocket detection via bind() + AF_UNIX
    writeln!(f, "#include <sys/socket.h>").unwrap();
    writeln!(f, "#include <sys/un.h>").unwrap();
    writeln!(f, "void start_server() {{").unwrap();
    writeln!(f, "    int fd = socket(AF_UNIX, SOCK_STREAM, 0);").unwrap();
    writeln!(f, "    struct sockaddr_un addr;").unwrap();
    writeln!(f, "    addr.sun_family = AF_UNIX;").unwrap();
    writeln!(f, "    bind(fd, (struct sockaddr*)&addr, sizeof(addr));").unwrap();
    writeln!(f, "}}").unwrap();
}

/// Build a temp DB by indexing a minimal test repo.
fn build_indexed_db() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_test_repo_with_socket(&repo_path);

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

/// Build a DB with no boundary interactions (just a plain TS file).
fn build_indexed_db_no_boundaries() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

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

// ══════════════════════════════════════════════════════════════════
// 1. USAGE ERRORS
// ══════════════════════════════════════════════════════════════════

#[test]
fn boundaries_usage_error_no_subcommand() {
    let output = Command::new(binary_path())
        .args(["boundaries"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout must be empty on usage error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn boundaries_usage_error_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["boundaries", "unknown"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown boundaries subcommand"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn boundaries_list_usage_error_missing_args() {
    let output = Command::new(binary_path())
        .args(["boundaries", "list"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn boundaries_show_usage_error_missing_surface_uid() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "show",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn boundaries_summary_usage_error_missing_repo() {
    let output = Command::new(binary_path())
        .args(["boundaries", "summary", "/tmp/test.db"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// 2. DB OPEN FAILURE
// ══════════════════════════════════════════════════════════════════

#[test]
fn boundaries_list_missing_db_exit_2() {
    let output = Command::new(binary_path())
        .args(["boundaries", "list", "/nonexistent/path.db", "repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout must be empty on error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn boundaries_show_missing_db_exit_2() {
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "show",
            "/nonexistent/path.db",
            "repo",
            "surface-uid",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn boundaries_summary_missing_db_exit_2() {
    let output = Command::new(binary_path())
        .args(["boundaries", "summary", "/nonexistent/path.db", "repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// 3. REPO NOT FOUND
// ══════════════════════════════════════════════════════════════════

#[test]
fn boundaries_list_repo_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "nonexistent-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

#[test]
fn boundaries_show_repo_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "show",
            db_path.to_str().unwrap(),
            "nonexistent-repo",
            "surface-uid",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

#[test]
fn boundaries_summary_repo_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "summary",
            db_path.to_str().unwrap(),
            "nonexistent-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// 4. LIST SUCCESS / EMPTY
// ══════════════════════════════════════════════════════════════════

#[test]
fn boundaries_list_empty_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    // Exit code 1 for "no results found"
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // JSON output with empty results
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["command"], "boundaries list");
    assert_eq!(result["count"], 0);
    assert!(result["results"].as_array().unwrap().is_empty());
}

#[test]
fn boundaries_list_envelope_contract() {
    let (_dir, db_path, _repo_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    // May be exit 0 (found results) or 1 (no results)
    assert!(
        output.status.code() == Some(0) || output.status.code() == Some(1),
        "unexpected exit code: {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // Envelope fields
    assert_eq!(result["command"], "boundaries list");
    assert!(result["repo"].is_string());
    assert!(result["snapshot"].is_string());
    assert!(result["count"].is_number());
    assert!(result["results"].is_array());
}

// ══════════════════════════════════════════════════════════════════
// 5. LIST WITH FILTERS
// ══════════════════════════════════════════════════════════════════

#[test]
fn boundaries_list_unknown_option_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--unknown-flag",
            "value",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown option"), "stderr: {}", stderr);
}

#[test]
fn boundaries_list_invalid_kind_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--kind",
            "invalid_kind_value",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown channel kind"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn boundaries_list_invalid_scope_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--scope",
            "invalid_scope",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown boundary scope"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn boundaries_list_filter_kind_reflected_in_envelope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--kind",
            "unix_socket",
        ])
        .output()
        .unwrap();

    // Exit 1 for empty, but should still output JSON
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["filter_kind"], "unix_socket");
}

#[test]
fn boundaries_list_filter_symbol_reflected_in_envelope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--symbol",
            "some_function",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["filter_symbol"], "some_function");
}

#[test]
fn boundaries_list_filter_file_reflected_in_envelope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--file",
            "src/server.c",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["filter_file"], "src/server.c");
}

// ══════════════════════════════════════════════════════════════════
// 6. SHOW SUCCESS / NOT FOUND
// ══════════════════════════════════════════════════════════════════

#[test]
fn boundaries_show_not_found_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "show",
            db_path.to_str().unwrap(),
            "test-repo",
            "nonexistent-surface-uid",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

// ══════════════════════════════════════════════════════════════════
// 7. SUMMARY SUCCESS / EMPTY
// ══════════════════════════════════════════════════════════════════

#[test]
fn boundaries_summary_empty_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_boundaries();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "summary",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    // Exit code 1 for "no surfaces"
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["command"], "boundaries summary");
    // Summary uses camelCase serialization
    assert_eq!(result["summary"]["totalSurfaces"], 0);
}

#[test]
fn boundaries_summary_envelope_contract() {
    let (_dir, db_path, _repo_path) = build_indexed_db();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "summary",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    // May be exit 0 or 1 depending on whether surfaces exist
    assert!(
        output.status.code() == Some(0) || output.status.code() == Some(1),
        "unexpected exit code: {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // Envelope fields
    assert_eq!(result["command"], "boundaries summary");
    assert!(result["repo"].is_string());
    assert!(result["snapshot"].is_string());
    assert!(result["summary"].is_object());
    // Summary uses camelCase serialization
    assert!(result["summary"]["totalSurfaces"].is_number());
    assert!(result["summary"]["totalChannels"].is_number());
    assert!(result["summary"]["byChannelKind"].is_array());
    assert!(result["summary"]["byBoundaryScope"].is_array());
    assert!(result["summary"]["byDirection"].is_array());
    assert!(result["summary"]["byProtocolFamily"].is_array());
}
