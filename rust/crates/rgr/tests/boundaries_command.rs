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

// ── GR-2A: Client stub hint tests ───────────────────────────────────

/// Build a test DB with a GR-2A client stub surface (direction=consumer, basis=stub_creation).
fn build_indexed_db_with_gr2a_client() -> (tempfile::TempDir, PathBuf, String) {
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
            VALUES ('f1', 'test-repo', 'src/HelloWorldClient.java', 'java');

            -- GR-2A client stub surface: direction=consumer, basis=stub_creation
            INSERT INTO boundary_interaction_surfaces (
                surface_uid, snapshot_uid, repo_uid,
                boundary_scope, channel_kind, direction,
                transport_class, provenance, confidence_basis,
                protocol, protocol_family, interaction_pattern,
                endpoint_locality, symbol_stable_key, source_file,
                line_start, line_end, col_start, col_end,
                extractor, basis, confidence, evidence_json
            ) VALUES (
                'surf-greeter-client', 'snap-1', 'test-repo',
                'unknown', 'grpc_channel', 'consumer',
                'schema_rpc', 'inferred', 'stub_creation',
                'grpc', 'rpc', 'unknown',
                'unknown', 'test-repo:src/HelloWorldClient.java#HelloWorldClient.init:SYMBOL:METHOD',
                'src/HelloWorldClient.java',
                20, 20, 5, 5,
                'grpc_client_hint_java', 'stub_creation', 0.85,
                '{"grpc_class":"GreeterGrpc","stub_method":"newBlockingStub","stub_type":"blocking","proto_service_name":"Greeter","mapping_uid":"m-greeter","mapping_confidence":0.85}'
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
                'bc-greeter-client', 'surf-greeter-client', 'ce-greeter-svc',
                'grpc_service', 'generated_code_mapping', 0.85,
                '{"mapping_uid":"m-greeter"}'
            );
            "#,
        )
        .unwrap();

    let surface_uid = "surf-greeter-client".to_string();
    (dir, db_path, surface_uid)
}

#[test]
fn boundaries_list_shows_gr2a_consumer_direction() {
    let (_dir, db_path, _surface_uid) = build_indexed_db_with_gr2a_client();

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

    // GR-2A: direction should be consumer (client side)
    assert_eq!(
        item["direction"], "consumer",
        "GR-2A client stub should have direction=consumer"
    );

    // GR-2A: basis should be stub_creation
    assert_eq!(
        item["basis"], "stub_creation",
        "GR-2A client stub should have basis=stub_creation"
    );

    // Confidence should be 0.85 (hint-grade)
    let confidence = item["confidence"].as_f64().unwrap();
    assert!(
        (confidence - 0.85).abs() < 0.001,
        "GR-2A client stub confidence should be 0.85, got {}",
        confidence
    );

    // Channel kind should be grpc_channel
    assert_eq!(
        item["channelKind"], "grpc_channel",
        "GR-2A client stub should have channelKind=grpc_channel"
    );
}

#[test]
fn boundaries_show_includes_stub_info_for_gr2a() {
    let (_dir, db_path, surface_uid) = build_indexed_db_with_gr2a_client();

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

    // Verify direction=consumer
    assert_eq!(result["direction"], "consumer");

    // evidenceJson should contain stub info
    let evidence_json_str = result["evidenceJson"].as_str()
        .expect("evidenceJson should be a string");
    let evidence: serde_json::Value = serde_json::from_str(evidence_json_str)
        .expect("evidenceJson should be valid JSON");

    // Verify stub-specific fields
    assert_eq!(evidence["grpc_class"], "GreeterGrpc");
    assert_eq!(evidence["stub_method"], "newBlockingStub");
    assert_eq!(evidence["stub_type"], "blocking");
    assert_eq!(evidence["proto_service_name"], "Greeter");
}

#[test]
fn boundaries_show_includes_gr2a_contract_association() {
    let (_dir, db_path, surface_uid) = build_indexed_db_with_gr2a_client();

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

    // Verify contract association is present in show output
    let contracts = result["contracts"].as_array()
        .expect("contracts should be an array in show output");

    assert_eq!(contracts.len(), 1, "should have 1 contract association");

    let contract = &contracts[0];
    assert_eq!(contract["contractElementUid"], "ce-greeter-svc");
    assert_eq!(contract["contractKind"], "grpc_service");
    assert_eq!(contract["contractName"], "helloworld.Greeter");
}

// ══════════════════════════════════════════════════════════════════
// 9. BI-1D: PROCESS SIGNAL FILTER TESTS
// ══════════════════════════════════════════════════════════════════

/// Create a minimal repo with C files containing signal sending and handling code.
fn create_test_repo_with_signals(dir: &std::path::Path) {
    // Signal sender: kill() and raise()
    let sender = dir.join("signal_sender.c");
    let mut f = File::create(&sender).unwrap();
    writeln!(f, "#include <signal.h>").unwrap();
    writeln!(f, "#include <sys/types.h>").unwrap();
    writeln!(f, "void send_shutdown(pid_t child) {{").unwrap();
    writeln!(f, "    kill(child, SIGTERM);").unwrap();
    writeln!(f, "}}").unwrap();
    writeln!(f, "void self_signal(void) {{").unwrap();
    writeln!(f, "    raise(SIGUSR1);").unwrap();
    writeln!(f, "}}").unwrap();

    // Signal handler: signal() and sigaction()
    let handler = dir.join("signal_handler.c");
    let mut f = File::create(&handler).unwrap();
    writeln!(f, "#include <signal.h>").unwrap();
    writeln!(f, "void term_handler(int sig) {{ }}").unwrap();
    writeln!(f, "void int_handler(int sig) {{ }}").unwrap();
    writeln!(f, "void setup(void) {{").unwrap();
    writeln!(f, "    signal(SIGTERM, term_handler);").unwrap();
    writeln!(f, "    struct sigaction act;").unwrap();
    writeln!(f, "    sigaction(SIGINT, &act, NULL);").unwrap();
    writeln!(f, "}}").unwrap();
}

/// Build a temp DB by indexing a repo with signal-related C code.
fn build_indexed_db_with_signals() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_test_repo_with_signals(&repo_path);

    let db_path = dir.path().join("signal_test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &repo_path,
        &db_path,
        "signal-test-repo",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert!(result.files_total >= 2, "expected 2 C fixture files");

    (dir, db_path, repo_path)
}

#[test]
fn boundaries_list_filter_kind_process_signal_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_signals();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "signal-test-repo",
            "--kind",
            "process_signal",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected in envelope
    assert_eq!(result["filter_kind"], "process_signal");

    // Should find 4 signal surfaces (2 sender + 2 handler)
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(
        count, 4,
        "expected 4 process_signal surfaces (kill, raise, signal, sigaction)"
    );

    // All results should be process_signal
    for item in result["results"].as_array().unwrap() {
        assert_eq!(
            item["channelKind"], "process_signal",
            "all filtered results should be process_signal"
        );
    }
}

#[test]
fn boundaries_list_filter_kind_signal_alias_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_signals();

    // "signal" is the alias for "process_signal"
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "signal-test-repo",
            "--kind",
            "signal",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected (alias maps to process_signal)
    assert_eq!(result["filter_kind"], "process_signal");

    // Same count as process_signal
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 4, "signal alias should yield same results as process_signal");
}

#[test]
fn boundaries_list_filter_family_signal_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_signals();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "signal-test-repo",
            "--family",
            "signal",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected in envelope
    assert_eq!(result["filter_family"], "signal");

    // Should find 4 signal surfaces
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 4, "expected 4 surfaces in signal family");

    // All results should have protocolFamily=signal
    for item in result["results"].as_array().unwrap() {
        assert_eq!(
            item["protocolFamily"], "signal",
            "all filtered results should have protocolFamily=signal"
        );
    }
}

#[test]
fn boundaries_list_signal_provider_consumer_directions() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_signals();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "signal-test-repo",
            "--kind",
            "process_signal",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let surfaces = result["results"].as_array().unwrap();

    // Count provider vs consumer
    let providers: Vec<_> = surfaces
        .iter()
        .filter(|s| s["direction"].as_str() == Some("provider"))
        .collect();
    let consumers: Vec<_> = surfaces
        .iter()
        .filter(|s| s["direction"].as_str() == Some("consumer"))
        .collect();

    // 2 providers: kill(), raise()
    assert_eq!(
        providers.len(),
        2,
        "expected 2 provider surfaces (kill, raise)"
    );

    // 2 consumers: signal(), sigaction()
    assert_eq!(
        consumers.len(),
        2,
        "expected 2 consumer surfaces (signal, sigaction)"
    );

    // Provider sources should be signal_sender.c
    for p in &providers {
        let src = p["sourceFile"].as_str().unwrap_or("");
        assert!(
            src.contains("signal_sender.c"),
            "provider should be from signal_sender.c, got: {}",
            src
        );
    }

    // Consumer sources should be signal_handler.c
    for c in &consumers {
        let src = c["sourceFile"].as_str().unwrap_or("");
        assert!(
            src.contains("signal_handler.c"),
            "consumer should be from signal_handler.c, got: {}",
            src
        );
    }
}

#[test]
fn boundaries_list_signal_consumer_has_unknown_scope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_signals();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "signal-test-repo",
            "--kind",
            "process_signal",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let surfaces = result["results"].as_array().unwrap();

    // Consumer surfaces (signal, sigaction) should have boundaryScope=unknown
    // because they can receive signals from both kill (inter_process) and raise (intra_process)
    let consumers: Vec<_> = surfaces
        .iter()
        .filter(|s| s["direction"].as_str() == Some("consumer"))
        .collect();

    for c in &consumers {
        assert_eq!(
            c["boundaryScope"], "unknown",
            "consumer signal surfaces should have boundaryScope=unknown (P1 fix), got: {:?}",
            c
        );
    }
}

// ══════════════════════════════════════════════════════════════════
// GR-2A FIXTURE VALIDATION: Real indexed fixture run
// ══════════════════════════════════════════════════════════════════

/// Index the real grpc-java-minimal fixture and validate the full GR-2A chain:
/// 1. Java extractor emits CALLS edge for GreeterGrpc.newBlockingStub
/// 2. CS-2A maps GreeterBlockingStub to proto service
/// 3. GR-2A joins to produce consumer hint
/// 4. CLI surfaces show direction=consumer, basis=stub_creation
///
/// This is the real fixture validation - not synthetic DB seeding.
#[test]
fn gr2a_fixture_validated_full_indexed_run() {
    // Locate the grpc-java-minimal fixture
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest
        .join("..")
        .join("..")
        .join("..")
        .join("test")
        .join("fixtures")
        .join("grpc-java-minimal");

    assert!(
        fixture_path.join("src/main/java/io/grpc/examples/helloworld/HelloWorldClient.java").exists(),
        "HelloWorldClient.java fixture not found at {:?}",
        fixture_path
    );

    // Create temp DB and index the fixture
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("grpc-fixture.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &fixture_path,
        &db_path,
        "grpc-java-minimal",
        &ComposeOptions::default(),
    )
    .expect("fixture indexing should succeed");

    // ── Layer 0: Extraction proof ────────────────────────────────────
    // Verify Java files were indexed (5 Java files in fixture)
    assert!(
        result.files_total >= 5,
        "expected at least 5 Java files, got {}",
        result.files_total
    );

    // ── GR-2A: Boundary surfaces via CLI ─────────────────────────────
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "grpc-java-minimal",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "boundaries list should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {}\nstdout: {}", e, stdout));

    // boundaries list returns "results" array
    let surfaces = result["results"].as_array()
        .expect("results should be an array");

    // Find the GR-2A consumer surface (from HelloWorldClient)
    let client_surface = surfaces.iter().find(|s| {
        s["direction"].as_str() == Some("consumer")
            && s["basis"].as_str() == Some("stub_creation")
    });

    assert!(
        client_surface.is_some(),
        "expected to find GR-2A consumer surface with basis=stub_creation in boundaries list.\n\
         All surfaces: {:?}",
        surfaces
    );

    let client = client_surface.unwrap();

    // Verify GR-2A surface semantics
    assert_eq!(client["direction"], "consumer", "GR-2A should emit direction=consumer");
    assert_eq!(client["channelKind"], "grpc_channel", "should be grpc_channel");
    assert_eq!(client["transportClass"], "schema_rpc", "should be schema_rpc");
    assert_eq!(client["basis"], "stub_creation", "basis should be stub_creation");
    assert_eq!(client["protocol"], "grpc", "protocol should be grpc");

    // Verify confidence is hint-grade
    let confidence = client["confidence"].as_f64().unwrap_or(0.0);
    assert!(
        (confidence - 0.85).abs() < 0.01,
        "confidence should be 0.85 (hint-grade), got {}",
        confidence
    );

    // Verify source file points to HelloWorldClient
    let source_file = client["sourceFile"].as_str().unwrap_or("");
    assert!(
        source_file.contains("HelloWorldClient.java"),
        "source file should be HelloWorldClient.java, got: {}",
        source_file
    );

    // ── GR-2A: Contract association proof ────────────────────────────
    // Get surface_uid and verify contract link via show command
    let surface_uid = client["surfaceUid"].as_str()
        .expect("surface should have surfaceUid");

    let show_output = Command::new(binary_path())
        .args([
            "boundaries",
            "show",
            db_path.to_str().unwrap(),
            "grpc-java-minimal",
            surface_uid,
        ])
        .output()
        .unwrap();

    assert_eq!(
        show_output.status.code(),
        Some(0),
        "boundaries show should succeed"
    );

    let show_stdout = String::from_utf8_lossy(&show_output.stdout);
    let show_result: serde_json::Value = serde_json::from_str(&show_stdout)
        .unwrap_or_else(|e| panic!("show output not valid JSON: {}\nstdout: {}", e, show_stdout));

    // Verify evidence contains stub info
    let evidence_json_str = show_result["evidenceJson"].as_str()
        .expect("show should include evidenceJson");
    let evidence: serde_json::Value = serde_json::from_str(evidence_json_str)
        .expect("evidenceJson should be valid JSON");

    assert_eq!(evidence["grpc_class"], "GreeterGrpc", "grpc_class in evidence");
    assert_eq!(evidence["stub_method"], "newBlockingStub", "stub_method in evidence");
    assert_eq!(evidence["stub_type"], "blocking", "stub_type in evidence");

    // Verify contract association to proto service
    let contracts = show_result["contracts"].as_array();
    assert!(
        contracts.is_some() && !contracts.unwrap().is_empty(),
        "GR-2A surface should have contract association to proto service.\n\
         show_result: {:?}",
        show_result
    );

    let contract = &contracts.unwrap()[0];
    assert_eq!(contract["contractKind"], "grpc_service", "contract kind should be grpc_service");

    // Contract name should reference Greeter service
    let contract_name = contract["contractName"].as_str().unwrap_or("");
    assert!(
        contract_name.contains("Greeter"),
        "contract should link to Greeter service, got: {}",
        contract_name
    );

    println!("GR-2A fixture validation PASSED:");
    println!("  - HelloWorldClient.java indexed");
    println!("  - CALLS edge to GreeterGrpc.newBlockingStub detected");
    println!("  - CS-2A mapping to proto service present");
    println!("  - GR-2A consumer surface emitted (confidence={:.2})", confidence);
    println!("  - Contract association to Greeter service verified");
}

// ══════════════════════════════════════════════════════════════════
// 10. BI-1C: SHAREDARRAYBUFFER/WORKER FILTER TESTS
// ══════════════════════════════════════════════════════════════════

/// Create a minimal repo with TS files containing SharedArrayBuffer/Worker code.
fn create_test_repo_with_sab(dir: &std::path::Path) {
    // Main thread: creates SAB and uses Atomics
    // Note: Worker and postMessage do NOT emit SAB surfaces (Option A decision)
    let main_ts = dir.join("main.ts");
    let mut f = File::create(&main_ts).unwrap();
    writeln!(f, "const sab = new SharedArrayBuffer(1024);").unwrap();
    writeln!(f, "const view = new Int32Array(sab);").unwrap();
    writeln!(f, "Atomics.store(view, 0, 0);").unwrap();
    writeln!(f, "Atomics.notify(view, 0, 1);").unwrap();

    // Worker: consumes SAB with Atomics
    let worker_ts = dir.join("worker.ts");
    let mut f = File::create(&worker_ts).unwrap();
    writeln!(f, "const view = new Int32Array(self.buffer);").unwrap();
    writeln!(f, "Atomics.wait(view, 0, 0);").unwrap();
    writeln!(f, "const val = Atomics.load(view, 0);").unwrap();
    writeln!(f, "Atomics.store(view, 1, val * 2);").unwrap();
}

/// Build a temp DB by indexing a repo with SharedArrayBuffer code.
fn build_indexed_db_with_sab() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_test_repo_with_sab(&repo_path);

    let db_path = dir.path().join("sab_test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &repo_path,
        &db_path,
        "sab-test-repo",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert!(result.files_total >= 2, "expected 2 TS fixture files");

    (dir, db_path, repo_path)
}

#[test]
fn boundaries_list_filter_kind_shared_array_buffer_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_sab();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "sab-test-repo",
            "--kind",
            "shared_array_buffer",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected in envelope
    assert_eq!(result["filter_kind"], "shared_array_buffer");

    // Should find SharedArrayBuffer surfaces
    let count = result["count"].as_u64().unwrap_or(0);
    assert!(
        count >= 6,
        "expected at least 6 shared_array_buffer surfaces; got {}",
        count
    );

    // All results should be shared_array_buffer
    for item in result["results"].as_array().unwrap() {
        assert_eq!(
            item["channelKind"], "shared_array_buffer",
            "all filtered results should be shared_array_buffer"
        );
    }
}

#[test]
fn boundaries_list_filter_kind_sab_alias_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_sab();

    // "sab" is the alias for "shared_array_buffer"
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "sab-test-repo",
            "--kind",
            "sab",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected (alias maps to shared_array_buffer)
    assert_eq!(result["filter_kind"], "shared_array_buffer");

    // Same count as shared_array_buffer
    let count = result["count"].as_u64().unwrap_or(0);
    assert!(count >= 6, "sab alias should yield SharedArrayBuffer results");
}

#[test]
fn boundaries_list_filter_family_shared_memory_includes_sab() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_sab();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "sab-test-repo",
            "--family",
            "shared_memory",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected in envelope
    assert_eq!(result["filter_family"], "shared_memory");

    // Should find surfaces (SAB maps to shared_memory family)
    let count = result["count"].as_u64().unwrap_or(0);
    assert!(count >= 6, "expected SharedArrayBuffer surfaces in shared_memory family");

    // All results should have protocolFamily=shared_memory
    for item in result["results"].as_array().unwrap() {
        assert_eq!(
            item["protocolFamily"], "shared_memory",
            "all filtered results should have protocolFamily=shared_memory"
        );
    }
}

#[test]
fn boundaries_list_sab_has_intra_process_scope() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_sab();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "sab-test-repo",
            "--kind",
            "shared_array_buffer",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let surfaces = result["results"].as_array().unwrap();

    // All SharedArrayBuffer surfaces should have boundaryScope=intra_process
    // (same OS process, different V8 isolates)
    for s in surfaces {
        assert_eq!(
            s["boundaryScope"], "intra_process",
            "SharedArrayBuffer surfaces should have intra_process scope, got: {:?}",
            s
        );
    }
}

// ══════════════════════════════════════════════════════════════════
// 11. MB-1A: AMQP / RABBITMQ FILTER TESTS
// ══════════════════════════════════════════════════════════════════

/// Create a minimal repo with TS files containing amqplib code.
fn create_test_repo_with_amqp(dir: &std::path::Path) {
    // Producer: assertQueue + sendToQueue
    let producer_ts = dir.join("producer.ts");
    let mut f = File::create(&producer_ts).unwrap();
    writeln!(f, "import amqp from 'amqplib';").unwrap();
    writeln!(f, "async function main() {{").unwrap();
    writeln!(f, "    const conn = await amqp.connect('amqp://localhost');").unwrap();
    writeln!(f, "    const channel = await conn.createChannel();").unwrap();
    writeln!(f, "    await channel.assertQueue('hello', {{ durable: true }});").unwrap();
    writeln!(f, "    channel.sendToQueue('hello', Buffer.from('msg'));").unwrap();
    writeln!(f, "}}").unwrap();

    // Consumer: assertQueue + consume
    let consumer_ts = dir.join("consumer.ts");
    let mut f = File::create(&consumer_ts).unwrap();
    writeln!(f, "import amqp from 'amqplib';").unwrap();
    writeln!(f, "async function main() {{").unwrap();
    writeln!(f, "    const conn = await amqp.connect('amqp://localhost');").unwrap();
    writeln!(f, "    const channel = await conn.createChannel();").unwrap();
    writeln!(f, "    await channel.assertQueue('hello', {{ durable: true }});").unwrap();
    writeln!(f, "    channel.consume('hello', (msg) => console.log(msg));").unwrap();
    writeln!(f, "}}").unwrap();
}

/// Build a temp DB by indexing a repo with AMQP code.
fn build_indexed_db_with_amqp() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_test_repo_with_amqp(&repo_path);

    let db_path = dir.path().join("amqp_test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &repo_path,
        &db_path,
        "amqp-test-repo",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert!(result.files_total >= 2, "expected 2 TS fixture files");

    (dir, db_path, repo_path)
}

#[test]
fn boundaries_list_filter_kind_amqp_queue_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_amqp();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "amqp-test-repo",
            "--kind",
            "amqp_queue",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected in envelope
    assert_eq!(result["filter_kind"], "amqp_queue");

    // Should find AMQP surfaces:
    // producer.ts: assertQueue + sendToQueue = 2
    // consumer.ts: assertQueue + consume = 2
    // Total: 4 surfaces
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(
        count, 4,
        "expected 4 amqp_queue surfaces (2 per file); got {}",
        count
    );

    // All results should be amqp_queue
    for item in result["results"].as_array().unwrap() {
        assert_eq!(
            item["channelKind"], "amqp_queue",
            "all filtered results should be amqp_queue"
        );
    }
}

#[test]
fn boundaries_list_filter_kind_amqp_alias_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_amqp();

    // "amqp" is the alias for "amqp_queue"
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "amqp-test-repo",
            "--kind",
            "amqp",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected (alias maps to amqp_queue)
    assert_eq!(result["filter_kind"], "amqp_queue");

    // Same count as amqp_queue
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 4, "amqp alias should yield same results as amqp_queue");
}

#[test]
fn boundaries_list_filter_kind_rabbitmq_alias_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_amqp();

    // "rabbitmq" is the alias for "amqp_queue"
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "amqp-test-repo",
            "--kind",
            "rabbitmq",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected (alias maps to amqp_queue)
    assert_eq!(result["filter_kind"], "amqp_queue");

    // Same count as amqp_queue
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 4, "rabbitmq alias should yield same results as amqp_queue");
}

#[test]
fn boundaries_list_filter_family_message_broker_includes_amqp() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_amqp();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "amqp-test-repo",
            "--family",
            "message_broker",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected in envelope
    assert_eq!(result["filter_family"], "message_broker");

    // Should find AMQP surfaces (amqp_queue maps to message_broker family)
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 4, "expected 4 surfaces in message_broker family");

    // All results should have protocolFamily=message_broker
    for item in result["results"].as_array().unwrap() {
        assert_eq!(
            item["protocolFamily"], "message_broker",
            "all filtered results should have protocolFamily=message_broker"
        );
    }
}

#[test]
fn boundaries_list_amqp_provider_consumer_directions() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_amqp();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "amqp-test-repo",
            "--kind",
            "amqp_queue",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let surfaces = result["results"].as_array().unwrap();

    // Count by direction
    let providers: Vec<_> = surfaces
        .iter()
        .filter(|s| s["direction"].as_str() == Some("provider"))
        .collect();
    let consumers: Vec<_> = surfaces
        .iter()
        .filter(|s| s["direction"].as_str() == Some("consumer"))
        .collect();
    let bidirectional: Vec<_> = surfaces
        .iter()
        .filter(|s| s["direction"].as_str() == Some("bidirectional"))
        .collect();

    // Expected:
    // - 1 provider: sendToQueue (producer.ts)
    // - 1 consumer: consume (consumer.ts)
    // - 2 bidirectional: assertQueue (producer.ts + consumer.ts)
    assert_eq!(
        providers.len(),
        1,
        "expected 1 provider surface (sendToQueue)"
    );
    assert_eq!(
        consumers.len(),
        1,
        "expected 1 consumer surface (consume)"
    );
    assert_eq!(
        bidirectional.len(),
        2,
        "expected 2 bidirectional surfaces (assertQueue)"
    );
}

#[test]
fn boundaries_list_amqp_no_detection_without_import() {
    // Create a repo with AMQP-like method names but NO amqplib import
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    // File with publish/consume/assertQueue but NO amqplib import
    let fake_ts = repo_path.join("fake_broker.ts");
    let mut f = File::create(&fake_ts).unwrap();
    writeln!(f, "// No amqplib import - these should NOT be detected").unwrap();
    writeln!(f, "const bus = {{").unwrap();
    writeln!(f, "    publish: (x: string) => console.log(x),").unwrap();
    writeln!(f, "    consume: (x: string) => console.log(x),").unwrap();
    writeln!(f, "    assertQueue: (x: string) => console.log(x),").unwrap();
    writeln!(f, "}};").unwrap();
    writeln!(f, "bus.publish('hello');").unwrap();
    writeln!(f, "bus.consume('world');").unwrap();
    writeln!(f, "bus.assertQueue('queue');").unwrap();

    let db_path = dir.path().join("fake_amqp.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &repo_path,
        &db_path,
        "fake-amqp-repo",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert!(result.files_total >= 1, "expected 1 TS fixture file");

    // Query for AMQP surfaces - should find NONE
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "fake-amqp-repo",
            "--kind",
            "amqp_queue",
        ])
        .output()
        .unwrap();

    // Exit code 1 for "no results found"
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 (no results) for fake AMQP without amqplib import, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Count should be 0 - the import guard prevents false positives
    assert_eq!(
        result["count"], 0,
        "P1 regression: generic .publish/.consume/.assertQueue without amqplib import \
         should NOT emit AMQP surfaces. Got count: {}",
        result["count"]
    );
}

// ══════════════════════════════════════════════════════════════════
// 12. MB-2A: KAFKA FILTER TESTS
// ══════════════════════════════════════════════════════════════════

/// Create a minimal repo with TS files containing kafkajs code.
fn create_test_repo_with_kafka(dir: &std::path::Path) {
    // Producer: send
    let producer_ts = dir.join("producer.ts");
    let mut f = File::create(&producer_ts).unwrap();
    writeln!(f, "import {{ Kafka }} from 'kafkajs';").unwrap();
    writeln!(f, "async function main() {{").unwrap();
    writeln!(f, "    const kafka = new Kafka({{ brokers: ['localhost:9092'] }});").unwrap();
    writeln!(f, "    const producer = kafka.producer();").unwrap();
    writeln!(f, "    await producer.send({{ topic: 'orders', messages: [] }});").unwrap();
    writeln!(f, "}}").unwrap();

    // Consumer: subscribe only (run is NOT detected — no topic evidence)
    let consumer_ts = dir.join("consumer.ts");
    let mut f = File::create(&consumer_ts).unwrap();
    writeln!(f, "import {{ Kafka }} from 'kafkajs';").unwrap();
    writeln!(f, "async function main() {{").unwrap();
    writeln!(f, "    const kafka = new Kafka({{ brokers: ['localhost:9092'] }});").unwrap();
    writeln!(f, "    const consumer = kafka.consumer({{ groupId: 'billing' }});").unwrap();
    writeln!(f, "    await consumer.subscribe({{ topic: 'orders' }});").unwrap();
    writeln!(f, "    await consumer.run({{ eachMessage: async () => {{}} }});").unwrap();
    writeln!(f, "}}").unwrap();
}

/// Build a temp DB by indexing a repo with Kafka code.
fn build_indexed_db_with_kafka() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();
    create_test_repo_with_kafka(&repo_path);

    let db_path = dir.path().join("kafka_test.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &repo_path,
        &db_path,
        "kafka-test-repo",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert!(result.files_total >= 2, "expected 2 TS fixture files");

    (dir, db_path, repo_path)
}

#[test]
fn boundaries_list_filter_kind_kafka_topic_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_kafka();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "kafka-test-repo",
            "--kind",
            "kafka_topic",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected in envelope
    assert_eq!(result["filter_kind"], "kafka_topic");

    // Should find Kafka surfaces:
    // producer.ts: send = 1 (with topic evidence)
    // consumer.ts: subscribe = 1 (with topic evidence)
    // consumer.ts: run = 0 (NO topic evidence — intentionally excluded)
    // Total: 2 surfaces
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(
        count, 2,
        "expected 2 kafka_topic surfaces (1 send, 1 subscribe — run excluded); got {}",
        count
    );

    // All results should be kafka_topic
    for item in result["results"].as_array().unwrap() {
        assert_eq!(
            item["channelKind"], "kafka_topic",
            "all filtered results should be kafka_topic"
        );
    }
}

#[test]
fn boundaries_list_filter_kind_kafka_alias_works() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_kafka();

    // "kafka" is the alias for "kafka_topic"
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "kafka-test-repo",
            "--kind",
            "kafka",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Filter reflected (alias maps to kafka_topic)
    assert_eq!(result["filter_kind"], "kafka_topic");

    // Same count as kafka_topic
    let count = result["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 2, "kafka alias should yield same results as kafka_topic");
}

#[test]
fn boundaries_list_kafka_provider_consumer_directions() {
    let (_dir, db_path, _repo_path) = build_indexed_db_with_kafka();

    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "kafka-test-repo",
            "--kind",
            "kafka_topic",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let surfaces = result["results"].as_array().unwrap();

    // Count by direction
    let providers: Vec<_> = surfaces
        .iter()
        .filter(|s| s["direction"].as_str() == Some("provider"))
        .collect();
    let consumers: Vec<_> = surfaces
        .iter()
        .filter(|s| s["direction"].as_str() == Some("consumer"))
        .collect();

    // Expected:
    // - 1 provider: send (producer.ts)
    // - 1 consumer: subscribe (consumer.ts)
    // Note: run() is NOT detected — no topic evidence
    assert_eq!(
        providers.len(),
        1,
        "expected 1 provider surface (send)"
    );
    assert_eq!(
        consumers.len(),
        1,
        "expected 1 consumer surface (subscribe only — run excluded)"
    );
}

#[test]
fn boundaries_list_kafka_no_detection_without_import() {
    // Create a repo with Kafka-like method names but NO kafkajs import
    let dir = tempfile::tempdir().unwrap();
    let repo_path = dir.path().join("repo");
    fs::create_dir_all(&repo_path).unwrap();

    // File with send/subscribe/run but NO kafkajs import
    let fake_ts = repo_path.join("fake_kafka.ts");
    let mut f = File::create(&fake_ts).unwrap();
    writeln!(f, "// No kafkajs import - these should NOT be detected").unwrap();
    writeln!(f, "const producer = {{").unwrap();
    writeln!(f, "    send: (x: any) => console.log(x),").unwrap();
    writeln!(f, "}};").unwrap();
    writeln!(f, "const consumer = {{").unwrap();
    writeln!(f, "    subscribe: (x: any) => console.log(x),").unwrap();
    writeln!(f, "    run: (x: any) => console.log(x),").unwrap();
    writeln!(f, "}};").unwrap();
    writeln!(f, "producer.send({{ topic: 'test' }});").unwrap();
    writeln!(f, "consumer.subscribe({{ topic: 'test' }});").unwrap();
    writeln!(f, "consumer.run({{ eachMessage: () => {{}} }});").unwrap();

    let db_path = dir.path().join("fake_kafka.db");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let result = index_path(
        &repo_path,
        &db_path,
        "fake-kafka-repo",
        &ComposeOptions::default(),
    )
    .unwrap();
    assert!(result.files_total >= 1, "expected 1 TS fixture file");

    // Query for Kafka surfaces - should find NONE
    let output = Command::new(binary_path())
        .args([
            "boundaries",
            "list",
            db_path.to_str().unwrap(),
            "fake-kafka-repo",
            "--kind",
            "kafka_topic",
        ])
        .output()
        .unwrap();

    // Exit code 1 for "no results found"
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 (no results) for fake Kafka without kafkajs import, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {}\nstdout: {}", e, stdout));

    // Count should be 0 - the import guard prevents false positives
    assert_eq!(
        result["count"], 0,
        "P1 regression: generic .send/.subscribe/.run without kafkajs import \
         should NOT emit Kafka surfaces. Got count: {}",
        result["count"]
    );
}
