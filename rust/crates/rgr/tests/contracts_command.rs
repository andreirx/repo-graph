//! Deterministic tests for the `contracts` command family.
//!
//! CS-1: Contract schema discovery read-side.
//! CS-2A: Java generated code mappings (usages command).
//!
//! Test matrix:
//!   1. Usage errors (wrong args, missing subcommand)
//!   2. DB open failure (missing file)
//!   3. Repo not found (wrong repo_uid)
//!   4. List success / empty results
//!   5. List with filters (--kind)
//!   6. Show success / not found
//!   7. Elements success / empty / with filters
//!   8. Usages success / empty / with filters (--element, --min-confidence)

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

/// Create a minimal repo with a .proto file.
fn create_test_repo_with_proto(dir: &std::path::Path) {
    let proto = dir.join("api.proto");
    let mut f = File::create(&proto).unwrap();
    writeln!(f, r#"syntax = "proto3";"#).unwrap();
    writeln!(f, r#"package api.v1;"#).unwrap();
    writeln!(f).unwrap();
    writeln!(f, r#"message User {{"#).unwrap();
    writeln!(f, r#"  string id = 1;"#).unwrap();
    writeln!(f, r#"  string name = 2;"#).unwrap();
    writeln!(f, r#"}}"#).unwrap();
    writeln!(f).unwrap();
    writeln!(f, r#"service UserService {{"#).unwrap();
    writeln!(f, r#"  rpc GetUser(User) returns (User);"#).unwrap();
    writeln!(f, r#"}}"#).unwrap();
}

/// Build a temp DB by indexing a minimal test repo with proto.
fn build_indexed_db_with_proto() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_test_repo_with_proto(&repo_path);

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

/// Build a DB with no contract schemas (just a plain TS file).
fn build_indexed_db_no_contracts() -> (tempfile::TempDir, PathBuf, PathBuf) {
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

/// Create a test repo with proto + Java generated code.
fn create_test_repo_with_proto_and_java(dir: &std::path::Path) {
    // Proto file with java_package option
    let proto = dir.join("api.proto");
    let mut f = File::create(&proto).unwrap();
    writeln!(f, r#"syntax = "proto3";"#).unwrap();
    writeln!(f, r#"package api.v1;"#).unwrap();
    writeln!(f, r#"option java_package = "com.example.api";"#).unwrap();
    writeln!(f, r#"option java_outer_classname = "ApiProtos";"#).unwrap();
    writeln!(f).unwrap();
    writeln!(f, r#"message User {{"#).unwrap();
    writeln!(f, r#"  string id = 1;"#).unwrap();
    writeln!(f, r#"}}"#).unwrap();
    writeln!(f).unwrap();
    writeln!(f, r#"service UserService {{"#).unwrap();
    writeln!(f, r#"  rpc GetUser(User) returns (User);"#).unwrap();
    writeln!(f, r#"}}"#).unwrap();

    // Simulate generated Java file (outer class pattern)
    let java_dir = dir.join("com/example/api");
    fs::create_dir_all(&java_dir).unwrap();

    let java_protos = java_dir.join("ApiProtos.java");
    let mut f = File::create(&java_protos).unwrap();
    writeln!(f, "package com.example.api;").unwrap();
    writeln!(f, "public final class ApiProtos {{").unwrap();
    writeln!(
        f,
        "  public static final class User extends com.google.protobuf.GeneratedMessageV3 {{}}"
    )
    .unwrap();
    writeln!(f, "}}").unwrap();

    // Simulate gRPC generated file
    let grpc_java = java_dir.join("UserServiceGrpc.java");
    let mut f = File::create(&grpc_java).unwrap();
    writeln!(f, "package com.example.api;").unwrap();
    writeln!(f, "public final class UserServiceGrpc {{").unwrap();
    writeln!(f, "  public static abstract class UserServiceImplBase {{}}").unwrap();
    writeln!(
        f,
        "  public static final class UserServiceBlockingStub {{}}"
    )
    .unwrap();
    writeln!(f, "}}").unwrap();
}

/// Build a temp DB with proto + Java generated code.
fn build_indexed_db_with_proto_and_java() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_test_repo_with_proto_and_java(&repo_path);

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

// ======================================================================
// 1. USAGE ERRORS
// ======================================================================

#[test]
fn contracts_usage_error_no_subcommand() {
    let output = Command::new(binary_path())
        .args(["contracts"])
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
fn contracts_usage_error_unknown_subcommand() {
    let output = Command::new(binary_path())
        .args(["contracts", "unknown"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown contracts subcommand"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn contracts_list_usage_error_missing_args() {
    let output = Command::new(binary_path())
        .args(["contracts", "list"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn contracts_show_usage_error_missing_file_path() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args(["contracts", "show", db_path.to_str().unwrap(), "test-repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn contracts_elements_usage_error_missing_args() {
    let output = Command::new(binary_path())
        .args(["contracts", "elements"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

#[test]
fn contracts_usages_usage_error_missing_args() {
    let output = Command::new(binary_path())
        .args(["contracts", "usages"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "stderr: {}", stderr);
}

// ======================================================================
// 2. DB OPEN FAILURE
// ======================================================================

#[test]
fn contracts_list_missing_db_exit_2() {
    let output = Command::new(binary_path())
        .args(["contracts", "list", "/nonexistent/path.db", "repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "stdout must be empty on error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn contracts_show_missing_db_exit_2() {
    let output = Command::new(binary_path())
        .args([
            "contracts",
            "show",
            "/nonexistent/path.db",
            "repo",
            "api.proto",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn contracts_elements_missing_db_exit_2() {
    let output = Command::new(binary_path())
        .args(["contracts", "elements", "/nonexistent/path.db", "repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

#[test]
fn contracts_usages_missing_db_exit_2() {
    let output = Command::new(binary_path())
        .args(["contracts", "usages", "/nonexistent/path.db", "repo"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "stderr: {}", stderr);
}

// ======================================================================
// 3. REPO NOT FOUND
// ======================================================================

#[test]
fn contracts_list_repo_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
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
fn contracts_show_repo_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "show",
            db_path.to_str().unwrap(),
            "nonexistent-repo",
            "api.proto",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

#[test]
fn contracts_elements_repo_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "elements",
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
fn contracts_usages_repo_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
            db_path.to_str().unwrap(),
            "nonexistent-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

// ======================================================================
// 4. LIST SUCCESS / EMPTY
// ======================================================================

#[test]
fn contracts_list_empty_results() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args(["contracts", "list", db_path.to_str().unwrap(), "test-repo"])
        .output()
        .unwrap();

    // Exit 0 for success (empty is still valid)
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // JSON output with empty results
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["command"], "contracts list");
    assert_eq!(result["count"], 0);
    assert!(result["results"].as_array().unwrap().is_empty());
}

#[test]
fn contracts_list_envelope_contract() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_proto();

    let output = Command::new(binary_path())
        .args(["contracts", "list", db_path.to_str().unwrap(), "test-repo"])
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

    // Envelope fields
    assert_eq!(result["command"], "contracts list");
    assert!(result["repo"].is_string());
    assert!(result["snapshot"].is_string());
    assert!(result["count"].is_number());
    assert!(result["results"].is_array());

    // Should have at least one schema
    assert!(result["count"].as_u64().unwrap() >= 1);

    // Check schema entry shape
    let schemas = result["results"].as_array().unwrap();
    if !schemas.is_empty() {
        let schema = &schemas[0];
        assert!(schema["schema_uid"].is_string());
        assert!(schema["file_path"].is_string());
        assert!(schema["schema_kind"].is_string());
        assert_eq!(schema["schema_kind"], "protobuf");
    }
}

// ======================================================================
// 5. LIST WITH FILTERS
// ======================================================================

#[test]
fn contracts_list_unknown_option_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
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
fn contracts_list_filter_kind_reflected_in_envelope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
            "--kind",
            "protobuf",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["filter_kind"], "protobuf");
}

// ======================================================================
// 6. SHOW SUCCESS / NOT FOUND
// ======================================================================

#[test]
fn contracts_show_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "show",
            db_path.to_str().unwrap(),
            "test-repo",
            "nonexistent.proto",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

#[test]
fn contracts_show_success_envelope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_proto();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "show",
            db_path.to_str().unwrap(),
            "test-repo",
            "api.proto",
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

    // Envelope fields
    assert_eq!(result["command"], "contracts show");
    assert!(result["repo"].is_string());
    assert!(result["snapshot"].is_string());

    // Detail shape
    let detail = &result["results"];
    assert!(detail["schema_uid"].is_string());
    assert_eq!(detail["file_path"], "api.proto");
    assert_eq!(detail["schema_kind"], "protobuf");
    assert!(detail["elements"].is_array());

    // Should have elements (message, fields, service, method)
    let elements = detail["elements"].as_array().unwrap();
    assert!(!elements.is_empty(), "expected elements in proto schema");

    // Check element shape
    let elem = &elements[0];
    assert!(elem["element_uid"].is_string());
    assert!(elem["element_kind"].is_string());
    assert!(elem["name"].is_string());
    assert!(elem["full_name"].is_string());
}

// ======================================================================
// 7. ELEMENTS SUCCESS / EMPTY / WITH FILTERS
// ======================================================================

#[test]
fn contracts_elements_empty_results() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "elements",
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["command"], "contracts elements");
    assert_eq!(result["count"], 0);
    assert!(result["results"].as_array().unwrap().is_empty());
}

#[test]
fn contracts_elements_envelope_contract() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_proto();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "elements",
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // Envelope fields
    assert_eq!(result["command"], "contracts elements");
    assert!(result["repo"].is_string());
    assert!(result["snapshot"].is_string());
    assert!(result["count"].is_number());
    assert!(result["results"].is_array());

    // Should have elements
    assert!(result["count"].as_u64().unwrap() >= 1);

    // Check element entry shape
    let elements = result["results"].as_array().unwrap();
    if !elements.is_empty() {
        let elem = &elements[0];
        assert!(elem["element_uid"].is_string());
        assert!(elem["schema_uid"].is_string());
        assert!(elem["file_path"].is_string());
        assert!(elem["element_kind"].is_string());
        assert!(elem["name"].is_string());
        assert!(elem["full_name"].is_string());
    }
}

#[test]
fn contracts_elements_filter_kind_reflected() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_proto();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "elements",
            db_path.to_str().unwrap(),
            "test-repo",
            "--kind",
            "message",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["filter_kind"], "message");

    // All results should be messages
    let elements = result["results"].as_array().unwrap();
    for elem in elements {
        assert_eq!(elem["element_kind"], "message");
    }
}

#[test]
fn contracts_elements_filter_file_reflected() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_proto();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "elements",
            db_path.to_str().unwrap(),
            "test-repo",
            "--file",
            "api.proto",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["filter_file"], "api.proto");
}

#[test]
fn contracts_elements_unknown_option_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "elements",
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
fn contracts_elements_file_not_found_exit_2() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "elements",
            db_path.to_str().unwrap(),
            "test-repo",
            "--file",
            "nonexistent.proto",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {}", stderr);
}

// ======================================================================
// 8. USAGES SUCCESS / EMPTY / WITH FILTERS (CS-2A)
// ======================================================================

#[test]
fn contracts_usages_empty_results() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["command"], "contracts usages");
    assert_eq!(result["count"], 0);
    assert!(result["results"].as_array().unwrap().is_empty());
}

#[test]
fn contracts_usages_envelope_contract() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_proto_and_java();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // Envelope fields
    assert_eq!(result["command"], "contracts usages");
    assert!(result["repo"].is_string());
    assert!(result["snapshot"].is_string());
    assert!(result["count"].is_number());
    assert!(result["results"].is_array());

    // This test now proves actual mappings were produced.
    // The fixture has:
    //   - api.proto with java_package="com.example.api", java_outer_classname="ApiProtos"
    //   - ApiProtos.java with com.example.api.ApiProtos.User class
    //   - UserServiceGrpc.java with service stubs
    //
    // The mapper should produce mappings for BOTH protobuf message classes
    // AND gRPC service stubs. We verify both to ensure the full CS-2A
    // pipeline works end-to-end.
    let count = result["count"].as_u64().unwrap();
    assert!(
        count >= 1,
        "expected at least 1 mapping from proto+java fixture, got {} mappings. stderr: {}",
        count,
        String::from_utf8_lossy(&output.stderr)
    );

    let results = result["results"].as_array().unwrap();

    // Verify mapping entry shape for all results
    for mapping in results {
        assert!(
            mapping["mapping_uid"].is_string(),
            "mapping_uid must be string"
        );
        assert!(
            mapping["schema_element_uid"].is_string(),
            "schema_element_uid must be string"
        );
        assert!(
            mapping["generated_symbol_key"].is_string(),
            "generated_symbol_key must be string"
        );
        assert!(mapping["language"].is_string(), "language must be string");
        assert_eq!(mapping["language"], "java", "language must be 'java'");
        assert!(
            mapping["generated_file"].is_string(),
            "generated_file must be string"
        );
        assert!(
            mapping["mapping_basis"].is_string(),
            "mapping_basis must be string"
        );
        assert!(
            mapping["confidence"].is_number(),
            "confidence must be number"
        );

        // Confidence should be above minimum floor (0.50)
        let confidence = mapping["confidence"].as_f64().unwrap();
        assert!(
            confidence >= 0.50,
            "confidence {} below minimum floor 0.50",
            confidence
        );
    }

    // Verify gRPC service mapping exists (not just protobuf message mappings).
    // This proves the gRPC half of CS-2A works end-to-end.
    let has_grpc_mapping = results.iter().any(|m| {
        let file = m["generated_file"].as_str().unwrap_or("");
        file.ends_with("UserServiceGrpc.java")
    });
    assert!(
        has_grpc_mapping,
        "expected at least one gRPC service mapping (UserServiceGrpc.java), but none found. \
         Mappings: {:?}",
        results
            .iter()
            .map(|m| m["generated_file"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn contracts_usages_unknown_option_exit_1() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
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
fn contracts_usages_element_filter_reflected_in_envelope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
            db_path.to_str().unwrap(),
            "test-repo",
            "--element",
            "elem-123",
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

    assert_eq!(result["filter_element"], "elem-123");
}

#[test]
fn contracts_usages_min_confidence_filter_reflected_in_envelope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
            db_path.to_str().unwrap(),
            "test-repo",
            "--min-confidence",
            "0.75",
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

    assert_eq!(result["filter_min_confidence"], 0.75);
}

#[test]
fn contracts_usages_element_requires_value() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
            db_path.to_str().unwrap(),
            "test-repo",
            "--element",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a value"), "stderr: {}", stderr);
}

#[test]
fn contracts_usages_min_confidence_requires_value() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
            db_path.to_str().unwrap(),
            "test-repo",
            "--min-confidence",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a value"), "stderr: {}", stderr);
}

#[test]
fn contracts_usages_min_confidence_invalid_value() {
    let (_dir, db_path, _repo_path) = build_indexed_db_no_contracts();

    let output = Command::new(binary_path())
        .args([
            "contracts",
            "usages",
            db_path.to_str().unwrap(),
            "test-repo",
            "--min-confidence",
            "1.5",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("between 0.0 and 1.0"), "stderr: {}", stderr);
}
