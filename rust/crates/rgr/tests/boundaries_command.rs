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

// ══════════════════════════════════════════════════════════════════
// 8. CONTRACT ASSOCIATION VISIBILITY (GR-1A)
// ══════════════════════════════════════════════════════════════════

/// Create a DB with a gRPC boundary surface and contract association.
/// This simulates the GR-1A output: a Java class extending *ImplBase
/// linked to a proto service via boundary_contracts.
fn build_indexed_db_with_contract() -> (tempfile::TempDir, PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // Create DB with migrations by opening via StorageConnection
    {
        let _conn = repo_graph_storage::connection::StorageConnection::open(&db_path).unwrap();
    }

    // Reopen with rusqlite to insert test data directly
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
            r#"
            INSERT INTO repos (repo_uid, name, root_path, created_at)
            VALUES ('test-repo', 'Test', '/tmp/test', datetime('now'));

            INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at)
            VALUES ('snap-1', 'test-repo', 'full', 'ready', datetime('now'));

            -- Create contract schema and element (CS-1 output)
            INSERT INTO contract_schemas (
                schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                package_name, content_hash, extractor, parsed_at
            ) VALUES (
                'cs-greeter', 'snap-1', 'test-repo', 'protobuf', 'api/v1/greeter.proto',
                'api.v1', 'abc123', 'proto-parser:0.1.0', datetime('now')
            );

            INSERT INTO contract_elements (
                element_uid, schema_uid, element_kind, name, full_name
            ) VALUES (
                'ce-greeter-svc', 'cs-greeter', 'service', 'Greeter', 'api.v1.Greeter'
            );

            -- Create boundary surface (GR-1A output)
            INSERT INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                transport_class, provenance, confidence_basis,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality, symbol_stable_key, source_file,
                line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (
                'surf-greeter-impl', 'snap-1', 'test-repo',
                'unknown', 'grpc_channel', 'provider',
                'schema_rpc', 'inferred', 'extends_impl_base',
                'grpc', 'rpc', 'request_response',
                'unknown', 'test-repo:src/GreeterImpl.java#GreeterImpl:SYMBOL:class',
                'src/GreeterImpl.java',
                10, 50, 1, 1,
                'grpc_impl_hint_java', 'inferred', 0.85,
                '{"impl_base_target":"GreeterGrpc.GreeterImplBase"}'
            );

            -- Create contract association (GR-1A output)
            INSERT INTO boundary_contracts (
                association_uid, surface_uid, contract_element_uid,
                contract_kind, association_basis, confidence, evidence_json
            ) VALUES (
                'bc-greeter', 'surf-greeter-impl', 'ce-greeter-svc',
                'grpc_service', 'generated_code_mapping', 0.95,
                '{"mapping_uid":"m-greeter"}'
            );
            "#,
        )
        .unwrap();

    let surface_uid = "surf-greeter-impl".to_string();
    (dir, db_path, surface_uid)
}

#[test]
fn boundaries_list_shows_contract_name_for_grpc_hint() {
    let (_dir, db_path, _surface_uid) = build_indexed_db_with_contract();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, got: {}\nstderr: {}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    assert_eq!(result["count"], 1);
    let item = &result["results"][0];

    // Verify GR-1A fields visible
    assert_eq!(item["channelKind"], "grpc_channel");
    assert_eq!(item["transportClass"], "schema_rpc");
    assert_eq!(item["confidenceBasis"], "extends_impl_base");

    // Verify contract fields visible (GR-1A contract association)
    assert_eq!(
        item["contractName"], "api.v1.Greeter",
        "contract_name should be visible in list output"
    );
    assert_eq!(
        item["contractKind"], "grpc_service",
        "contract_kind should be visible in list output"
    );
}

#[test]
fn boundaries_show_includes_contracts_for_grpc_hint() {
    let (_dir, db_path, surface_uid) = build_indexed_db_with_contract();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "show",
            db_path.to_str().unwrap(),
            "test-repo",
            &surface_uid,
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, got: {}\nstderr: {}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // Verify surface fields (show outputs the detail directly, no envelope)
    assert_eq!(result["channelKind"], "grpc_channel");
    assert_eq!(result["transportClass"], "schema_rpc");

    // Verify contracts array present
    let contracts = result["contracts"].as_array()
        .expect("contracts should be an array");
    assert_eq!(contracts.len(), 1, "should have 1 contract association");

    let contract = &contracts[0];
    assert_eq!(contract["associationUid"], "bc-greeter");
    assert_eq!(contract["contractElementUid"], "ce-greeter-svc");
    assert_eq!(contract["contractKind"], "grpc_service");
    assert_eq!(contract["contractName"], "api.v1.Greeter");
    assert_eq!(contract["associationBasis"], "generated_code_mapping");
    assert!((contract["confidence"].as_f64().unwrap() - 0.95).abs() < 0.001);
}

// ── GR-1B: Registration proof tests ─────────────────────────────────

/// Build a test DB with a GR-1B boosted surface (confidence 0.90, registration_sites in evidence).
fn build_indexed_db_with_gr1b_boost() -> (tempfile::TempDir, PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // Create DB with migrations via StorageConnection
    {
        let _conn = repo_graph_storage::connection::StorageConnection::open(&db_path).unwrap();
    }

    // Reopen with rusqlite to insert test data
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
            r#"
            INSERT INTO repos (repo_uid, name, root_path, created_at)
            VALUES ('test-repo', 'test-repo', '/test', datetime('now'));

            INSERT INTO snapshots (snapshot_uid, repo_uid, kind, status, created_at)
            VALUES ('snap-1', 'test-repo', 'full', 'ready', datetime('now'));

            INSERT INTO files (file_uid, repo_uid, path, language)
            VALUES ('f1', 'test-repo', 'src/HelloWorldServer.java', 'java');

            -- GR-1B boosted surface: confidence 0.90, basis extends_impl_base_registered
            INSERT INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                transport_class, provenance, confidence_basis,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality, symbol_stable_key, source_file,
                line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (
                'surf-greeter-boosted', 'snap-1', 'test-repo',
                'unknown', 'grpc_channel', 'provider',
                'schema_rpc', 'inferred', 'extends_impl_base',
                'grpc', 'rpc', 'unknown',
                'unknown', 'test-repo:src/HelloWorldServer.java#HelloWorldServer.GreeterImpl:SYMBOL:CLASS',
                'src/HelloWorldServer.java',
                34, 34, 29, 29,
                'grpc_impl_hint_java', 'extends_impl_base_registered', 0.90,
                '{"impl_base_target":"GreeterGrpc.GreeterImplBase","registration_sites":[{"file":"src/HelloWorldServer.java","line":18,"method":"start","pattern":"addService(new GreeterImpl())"}]}'
            );

            -- Contract schema and element for the service
            INSERT INTO contract_schemas (
                schema_uid, snapshot_uid, repo_uid, schema_kind, file_path,
                package_name, content_hash, extractor, parsed_at
            ) VALUES (
                'cs-greeter', 'snap-1', 'test-repo', 'protobuf', 'helloworld.proto',
                'helloworld', 'hash123', 'proto-parser:0.1.0', datetime('now')
            );

            INSERT INTO contract_elements (
                element_uid, schema_uid, element_kind, name, full_name
            ) VALUES (
                'ce-greeter-svc', 'cs-greeter', 'service', 'Greeter', 'helloworld.Greeter'
            );

            -- Contract association
            INSERT INTO boundary_contracts (
                association_uid, surface_uid, contract_element_uid,
                contract_kind, association_basis, confidence, evidence_json
            ) VALUES (
                'bc-greeter-boost', 'surf-greeter-boosted', 'ce-greeter-svc',
                'grpc_service', 'generated_code_mapping', 0.90,
                '{"mapping_uid":"m-greeter"}'
            );
            "#,
        )
        .unwrap();

    let surface_uid = "surf-greeter-boosted".to_string();
    (dir, db_path, surface_uid)
}

#[test]
fn boundaries_list_shows_gr1b_boosted_confidence() {
    let (_dir, db_path, _surface_uid) = build_indexed_db_with_gr1b_boost();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "test-repo",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, got: {}\nstderr: {}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    assert_eq!(result["count"], 1);
    let item = &result["results"][0];

    // GR-1B boosted confidence should be 0.90
    let confidence = item["confidence"].as_f64().unwrap();
    assert!(
        (confidence - 0.90).abs() < 0.001,
        "GR-1B boosted confidence should be 0.90, got {}",
        confidence
    );

    // basis should map to extends_impl_base (even though stored as extends_impl_base_registered)
    assert_eq!(item["basis"], "extends_impl_base");
}

#[test]
fn boundaries_show_includes_registration_sites_for_gr1b() {
    let (_dir, db_path, surface_uid) = build_indexed_db_with_gr1b_boost();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "show",
            db_path.to_str().unwrap(),
            "test-repo",
            &surface_uid,
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, got: {}\nstderr: {}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // GR-1B boosted confidence
    let confidence = result["confidence"].as_f64().unwrap();
    assert!(
        (confidence - 0.90).abs() < 0.001,
        "GR-1B boosted confidence should be 0.90, got {}",
        confidence
    );

    // evidenceJson should contain registration_sites
    let evidence_json_str = result["evidenceJson"].as_str()
        .expect("evidenceJson should be a string");
    let evidence: serde_json::Value = serde_json::from_str(evidence_json_str)
        .expect("evidenceJson should be valid JSON");

    let registration_sites = evidence["registration_sites"].as_array()
        .expect("registration_sites should be an array");
    assert_eq!(registration_sites.len(), 1, "should have 1 registration site");

    let site = &registration_sites[0];
    assert_eq!(site["file"], "src/HelloWorldServer.java");
    assert_eq!(site["line"], 18);
    assert_eq!(site["method"], "start");
    assert_eq!(site["pattern"], "addService(new GreeterImpl())");
}
