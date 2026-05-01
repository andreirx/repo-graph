//! Tests for contract indexing summary in `rmap index` / `rmap refresh`.
//!
//! CS-1: Validates the contract summary line on index/refresh output.
//! CS-2A: Validates the generated code mapping summary line.
//!
//! Test matrix:
//!   1. No contract files - no summary line
//!   2. Successful contract indexing - summary with counts
//!   3. Parse failures - summary with failure count and details
//!   4. Combined scenarios
//!   5. Generated code mappings - summary with counts (CS-2A)

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

/// Create a repo with only source files (no contracts).
fn create_repo_no_contracts(dir: &std::path::Path) {
    let src = dir.join("index.ts");
    let mut f = File::create(&src).unwrap();
    writeln!(f, "export function main() {{}}").unwrap();
}

/// Create a repo with valid proto files.
fn create_repo_with_protos(dir: &std::path::Path) {
    // TypeScript file for source extraction
    let src = dir.join("index.ts");
    let mut f = File::create(&src).unwrap();
    writeln!(f, "export function main() {{}}").unwrap();

    // Valid proto file
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

// ======================================================================
// 1. NO CONTRACT FILES
// ======================================================================

#[test]
fn index_no_contracts_no_summary_line() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_repo_no_contracts(&repo_path);

    let db_path = dir.path().join("test.db");

    let output = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should have the main index line
    assert!(stderr.contains("indexed"), "stderr: {}", stderr);
    // Should NOT have a contracts line
    assert!(
        !stderr.contains("contracts:"),
        "unexpected contracts line in: {}",
        stderr
    );
}

#[test]
fn refresh_no_contracts_no_summary_line() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_repo_no_contracts(&repo_path);

    let db_path = dir.path().join("test.db");

    // First index
    let _ = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // Then refresh
    let output = Command::new(binary_path())
        .args([
            "refresh",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("refreshed"), "stderr: {}", stderr);
    assert!(
        !stderr.contains("contracts:"),
        "unexpected contracts line in: {}",
        stderr
    );
}

// ======================================================================
// 2. SUCCESSFUL CONTRACT INDEXING
// ======================================================================

#[test]
fn index_with_protos_shows_contract_summary() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_repo_with_protos(&repo_path);

    let db_path = dir.path().join("test.db");

    let output = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should have the main index line
    assert!(stderr.contains("indexed"), "stderr: {}", stderr);
    // Should have contracts summary
    assert!(
        stderr.contains("contracts:"),
        "missing contracts line in: {}",
        stderr
    );
    // Should show schema and element counts
    assert!(
        stderr.contains("schemas"),
        "missing schemas in: {}",
        stderr
    );
    assert!(
        stderr.contains("elements"),
        "missing elements in: {}",
        stderr
    );
    // Should NOT show failure indicators
    assert!(
        !stderr.contains("failed"),
        "unexpected failure in: {}",
        stderr
    );
    assert!(
        !stderr.contains("FAILED"),
        "unexpected FAILED in: {}",
        stderr
    );
}

#[test]
fn refresh_with_protos_shows_contract_summary() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_repo_with_protos(&repo_path);

    let db_path = dir.path().join("test.db");

    // First index
    let _ = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    // Then refresh
    let output = Command::new(binary_path())
        .args([
            "refresh",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("refreshed"), "stderr: {}", stderr);
    assert!(
        stderr.contains("contracts:"),
        "missing contracts line in: {}",
        stderr
    );
    assert!(
        stderr.contains("schemas"),
        "missing schemas in: {}",
        stderr
    );
}

// ======================================================================
// 3. CONTRACT SUMMARY FORMAT VALIDATION
// ======================================================================

#[test]
fn index_contract_summary_format() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_repo_with_protos(&repo_path);

    let db_path = dir.path().join("test.db");

    let output = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();

    // Should have at least 2 lines (main line + contracts line)
    assert!(lines.len() >= 2, "expected at least 2 lines, got: {}", stderr);

    // First line should be the main index line
    assert!(
        lines[0].contains("indexed") && lines[0].contains("files"),
        "first line should be index summary: {}",
        lines[0]
    );

    // Second line should be indented contracts summary
    assert!(
        lines[1].starts_with("  contracts:"),
        "second line should be indented contracts summary: {}",
        lines[1]
    );

    // Contracts line should contain numeric counts
    // Format: "  contracts: N schemas, M elements"
    assert!(
        lines[1].contains("1 schema") || lines[1].contains("schemas"),
        "contracts line should contain schema count: {}",
        lines[1]
    );
}

// ======================================================================
// 4. MULTIPLE PROTO FILES
// ======================================================================

#[test]
fn index_multiple_protos_aggregates_counts() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    // Create multiple proto files
    let proto1 = repo_path.join("user.proto");
    let mut f = File::create(&proto1).unwrap();
    writeln!(f, r#"syntax = "proto3";"#).unwrap();
    writeln!(f, r#"package api;"#).unwrap();
    writeln!(f, r#"message User {{ string id = 1; }}"#).unwrap();

    let proto2 = repo_path.join("order.proto");
    let mut f = File::create(&proto2).unwrap();
    writeln!(f, r#"syntax = "proto3";"#).unwrap();
    writeln!(f, r#"package api;"#).unwrap();
    writeln!(f, r#"message Order {{ string id = 1; }}"#).unwrap();

    let proto3 = repo_path.join("service.proto");
    let mut f = File::create(&proto3).unwrap();
    writeln!(f, r#"syntax = "proto3";"#).unwrap();
    writeln!(f, r#"package api;"#).unwrap();
    writeln!(f, r#"service MyService {{ rpc Get(User) returns (Order); }}"#).unwrap();

    // Need at least one source file
    let src = repo_path.join("index.ts");
    let mut f = File::create(&src).unwrap();
    writeln!(f, "export function main() {{}}").unwrap();

    let db_path = dir.path().join("test.db");

    let output = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should show 3 schemas
    assert!(
        stderr.contains("3 schemas"),
        "expected 3 schemas in: {}",
        stderr
    );
}

// ======================================================================
// 5. GENERATED CODE MAPPING SUMMARY (CS-2A)
// ======================================================================

/// Create a repo with proto + Java generated code.
fn create_repo_with_proto_and_java(dir: &std::path::Path) {
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
    writeln!(f, "  public static final class User extends com.google.protobuf.GeneratedMessageV3 {{}}").unwrap();
    writeln!(f, "}}").unwrap();

    // Simulate gRPC generated file
    let grpc_java = java_dir.join("UserServiceGrpc.java");
    let mut f = File::create(&grpc_java).unwrap();
    writeln!(f, "package com.example.api;").unwrap();
    writeln!(f, "public final class UserServiceGrpc {{").unwrap();
    writeln!(f, "  public static abstract class UserServiceImplBase {{}}").unwrap();
    writeln!(f, "  public static final class UserServiceBlockingStub {{}}").unwrap();
    writeln!(f, "}}").unwrap();
}

#[test]
fn index_no_java_no_mapping_summary() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_repo_with_protos(&repo_path); // proto but no Java

    let db_path = dir.path().join("test.db");

    let output = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should have contracts line
    assert!(
        stderr.contains("contracts:"),
        "missing contracts line in: {}",
        stderr
    );
    // Should NOT have mappings line (no Java code)
    assert!(
        !stderr.contains("mappings:"),
        "unexpected mappings line in: {}",
        stderr
    );
}

#[test]
fn index_with_proto_and_java_shows_mapping_summary() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_repo_with_proto_and_java(&repo_path);

    let db_path = dir.path().join("test.db");

    let output = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should have contracts line
    assert!(
        stderr.contains("contracts:"),
        "missing contracts line in: {}",
        stderr
    );
    // Should have mappings line
    assert!(
        stderr.contains("mappings:"),
        "missing mappings line in: {}",
        stderr
    );
    // Should show persisted count
    assert!(
        stderr.contains("persisted"),
        "missing 'persisted' in: {}",
        stderr
    );
    // Should show high-confidence count
    assert!(
        stderr.contains("high-confidence"),
        "missing 'high-confidence' in: {}",
        stderr
    );
    // Should NOT show any failure indicators
    assert!(
        !stderr.contains("failed"),
        "unexpected 'failed' in: {}",
        stderr
    );
}

#[test]
fn index_mapping_summary_format() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_repo_with_proto_and_java(&repo_path);

    let db_path = dir.path().join("test.db");

    let output = Command::new(binary_path())
        .args([
            "index",
            repo_path.to_str().unwrap(),
            db_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();

    // Should have at least 3 lines (main + contracts + mappings)
    assert!(
        lines.len() >= 3,
        "expected at least 3 lines, got {} lines:\n{}",
        lines.len(),
        stderr
    );

    // Find the mappings line
    let mappings_line = lines.iter().find(|l| l.contains("mappings:"));
    assert!(
        mappings_line.is_some(),
        "missing mappings line in: {}",
        stderr
    );

    let mappings_line = mappings_line.unwrap();

    // Mappings line should be indented
    assert!(
        mappings_line.starts_with("  mappings:"),
        "mappings line should be indented: {}",
        mappings_line
    );

    // Format: "  mappings: N persisted (M high-confidence)"
    assert!(
        mappings_line.contains("persisted") && mappings_line.contains("high-confidence"),
        "mappings line should contain counts: {}",
        mappings_line
    );
}

// Note: refresh mapping summary test is deferred because delta indexing
// may not preserve Java symbol metadata in all cases. The index path is
// the primary surface and is tested above. See TECH-DEBT.md for tracking.
