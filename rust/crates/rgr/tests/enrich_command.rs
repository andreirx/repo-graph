//! Command-level integration tests for `rmap enrich`.
//!
//! These tests exercise the CLI entry point (`run_enrich`) with
//! real storage, verifying:
//! - Exit code semantics for usage errors, missing resources
//! - Argument parsing correctness
//! - Dry-run mode (no persistence)
//!
//! Tests that require actual language servers (rust-analyzer, tsserver,
//! jdtls) are deferred to validation runs against real repos.

use std::process::ExitCode;

use repo_graph_storage::types::{
    CreateSnapshotInput, FileVersion, GraphNode, Repo, SourceLocation, TrackedFile,
    UpdateSnapshotStatusInput,
};
use repo_graph_storage::StorageConnection;
use tempfile::TempDir;

// Re-export run_enrich for testing. It must be pub(crate) or pub.
// We'll need to check/fix visibility.

// ─────────────────────────────────────────────────────────────────────────────
// Test Fixtures
// ─────────────────────────────────────────────────────────────────────────────

fn open_temp_storage() -> (TempDir, StorageConnection, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let storage = StorageConnection::open(&db_path).unwrap();
    (dir, storage, db_path)
}

fn insert_repo(storage: &StorageConnection, uid: &str, name: &str) {
    storage
        .add_repo(&Repo {
            repo_uid: uid.to_string(),
            name: name.to_string(),
            root_path: format!("/tmp/{}", uid),
            default_branch: None,
            created_at: "2026-05-01T00:00:00Z".to_string(),
            metadata_json: None,
        })
        .unwrap();
}

fn create_ready_snapshot(storage: &StorageConnection, repo_uid: &str) -> String {
    let snap = storage
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: repo_uid.to_string(),
            parent_snapshot_uid: None,
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            label: None,
            toolchain_json: None,
        })
        .unwrap();
    storage
        .update_snapshot_status(&UpdateSnapshotStatusInput {
            snapshot_uid: snap.snapshot_uid.clone(),
            status: "ready".to_string(),
            completed_at: Some("2026-05-01T00:01:00Z".to_string()),
        })
        .unwrap();
    snap.snapshot_uid
}

fn insert_file_and_version(
    storage: &mut StorageConnection,
    file_uid: &str,
    repo_uid: &str,
    snapshot_uid: &str,
    path: &str,
    language: &str,
) {
    storage
        .upsert_files(&[TrackedFile {
            file_uid: file_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            path: path.to_string(),
            language: Some(language.to_string()),
            is_test: false,
            is_generated: false,
            is_excluded: false,
        }])
        .unwrap();

    storage
        .upsert_file_versions(&[FileVersion {
            snapshot_uid: snapshot_uid.to_string(),
            file_uid: file_uid.to_string(),
            content_hash: format!("hash-{}", file_uid),
            ast_hash: None,
            extractor: Some("test".to_string()),
            parse_status: "ok".to_string(),
            size_bytes: Some(100),
            line_count: Some(10),
            indexed_at: "2026-05-01T00:02:00Z".to_string(),
        }])
        .unwrap();
}

fn insert_node(
    storage: &mut StorageConnection,
    node_uid: &str,
    snapshot_uid: &str,
    repo_uid: &str,
    file_uid: &str,
    name: &str,
) {
    storage
        .insert_nodes(&[GraphNode {
            node_uid: node_uid.to_string(),
            snapshot_uid: snapshot_uid.to_string(),
            repo_uid: repo_uid.to_string(),
            stable_key: format!("{}::{}", file_uid, name),
            kind: "function".to_string(),
            subtype: None,
            name: name.to_string(),
            qualified_name: None,
            file_uid: Some(file_uid.to_string()),
            parent_node_uid: None,
            location: Some(SourceLocation {
                line_start: 1,
                col_start: 1,
                line_end: 10,
                col_end: 1,
            }),
            signature: None,
            visibility: Some("public".to_string()),
            doc_comment: None,
            metadata_json: None,
        }])
        .unwrap();
}

/// Insert an unresolved edge directly via raw SQL.
/// The enrichment query expects edges in the unresolved_edges table.
fn insert_unresolved_edge(
    db_path: &std::path::Path,
    edge_uid: &str,
    snapshot_uid: &str,
    repo_uid: &str,
    source_node_uid: &str,
    category: &str,
) {
    let raw = rusqlite::Connection::open(db_path).unwrap();
    raw.execute(
        "INSERT INTO unresolved_edges \
         (edge_uid, snapshot_uid, repo_uid, source_node_uid, \
          target_key, type, resolution, extractor, \
          category, classification, classifier_version, \
          basis_code, observed_at) \
         VALUES (?, ?, ?, ?, \
          'target::method', 'CALLS', 'unresolved', 'test:1', \
          ?, 'unknown', 1, \
          'no_supporting_signal', '2026-05-01T00:00:00.000Z')",
        rusqlite::params![edge_uid, snapshot_uid, repo_uid, source_node_uid, category],
    )
    .unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Exit Code Tests: Usage Errors (exit 1)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn exit_1_on_missing_args() {
    // run_enrich requires at least <db_path> <repo_uid>
    let result = repo_graph_rgr::commands::run_enrich(&[]);
    assert_eq!(result, ExitCode::from(1));
}

#[test]
fn exit_1_on_missing_repo_uid() {
    let result = repo_graph_rgr::commands::run_enrich(&["test.db".to_string()]);
    assert_eq!(result, ExitCode::from(1));
}

#[test]
fn exit_1_on_unknown_option() {
    let result = repo_graph_rgr::commands::run_enrich(&[
        "test.db".to_string(),
        "repo-1".to_string(),
        "--unknown-flag".to_string(),
    ]);
    assert_eq!(result, ExitCode::from(1));
}

#[test]
fn exit_1_on_language_missing_value() {
    let result = repo_graph_rgr::commands::run_enrich(&[
        "test.db".to_string(),
        "repo-1".to_string(),
        "--language".to_string(),
    ]);
    assert_eq!(result, ExitCode::from(1));
}

#[test]
fn exit_1_on_unknown_language() {
    let result = repo_graph_rgr::commands::run_enrich(&[
        "test.db".to_string(),
        "repo-1".to_string(),
        "--language".to_string(),
        "cobol".to_string(),
    ]);
    assert_eq!(result, ExitCode::from(1));
}

#[test]
fn exit_1_on_java_without_jdtls_path() {
    // Explicitly requesting Java without providing --jdtls-path or JDTLS_PATH
    // should fail with usage error (exit 1)
    // Must have valid repo+snapshot for the jdtls check to be reached
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let _snap_uid = create_ready_snapshot(&storage, "r1");

    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--language".to_string(),
        "java".to_string(),
    ]);
    assert_eq!(result, ExitCode::from(1));
}

// ─────────────────────────────────────────────────────────────────────────────
// Exit Code Tests: Runtime Errors (exit 2)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn exit_2_on_missing_db_file() {
    let result = repo_graph_rgr::commands::run_enrich(&[
        "/nonexistent/path/test.db".to_string(),
        "repo-1".to_string(),
    ]);
    assert_eq!(result, ExitCode::from(2));
}

#[test]
fn exit_2_on_missing_repo() {
    let (_tmp, _storage, db_path) = open_temp_storage();
    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "nonexistent-repo".to_string(),
    ]);
    assert_eq!(result, ExitCode::from(2));
}

#[test]
fn exit_2_on_missing_snapshot() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    // Repo exists but no snapshot
    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
    ]);
    assert_eq!(result, ExitCode::from(2));
}

#[test]
fn exit_2_on_explicit_missing_snapshot() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let _snap_uid = create_ready_snapshot(&storage, "r1");

    // Provide explicit --snapshot that doesn't exist
    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--snapshot".to_string(),
        "nonexistent-snapshot".to_string(),
    ]);
    assert_eq!(result, ExitCode::from(2));
}

#[test]
fn exit_2_on_snapshot_not_ready() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");

    // Create snapshot but don't mark it ready
    let snap = storage
        .create_snapshot(&CreateSnapshotInput {
            repo_uid: "r1".to_string(),
            parent_snapshot_uid: None,
            kind: "full".to_string(),
            basis_ref: None,
            basis_commit: None,
            label: None,
            toolchain_json: None,
        })
        .unwrap();

    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--snapshot".to_string(),
        snap.snapshot_uid,
    ]);
    assert_eq!(result, ExitCode::from(2));
}

// ─────────────────────────────────────────────────────────────────────────────
// Success Path Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn success_with_no_eligible_edges() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let _snap_uid = create_ready_snapshot(&storage, "r1");

    // Repo and ready snapshot exist, but no unresolved edges
    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
    ]);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn dry_run_does_not_persist() {
    let (_tmp, mut storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let snap_uid = create_ready_snapshot(&storage, "r1");

    // Create a file and node so the edge can derive language
    insert_file_and_version(&mut storage, "f1", "r1", &snap_uid, "src/test.ts", "typescript");
    insert_node(&mut storage, "n1", &snap_uid, "r1", "f1", "testFunc");

    // Insert an unresolved edge with NULL metadata
    insert_unresolved_edge(
        &db_path,
        "e1",
        &snap_uid,
        "r1",
        "n1",
        "calls_obj_method_needs_type_info",
    );

    // Verify metadata is NULL before run
    {
        let raw = rusqlite::Connection::open(&db_path).unwrap();
        let metadata: Option<String> = raw
            .query_row(
                "SELECT metadata_json FROM unresolved_edges WHERE edge_uid = ?",
                ["e1"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            metadata.is_none(),
            "metadata_json should be NULL before dry-run"
        );
    }

    // Run with --dry-run
    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--dry-run".to_string(),
    ]);

    // Command should succeed (resolver may fail but that's a valid result)
    assert_eq!(
        result,
        ExitCode::SUCCESS,
        "dry-run with eligible edges should succeed"
    );

    // Verify metadata is STILL NULL after dry-run (no persistence)
    {
        let raw = rusqlite::Connection::open(&db_path).unwrap();
        let metadata: Option<String> = raw
            .query_row(
                "SELECT metadata_json FROM unresolved_edges WHERE edge_uid = ?",
                ["e1"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            metadata.is_none(),
            "dry-run must not persist enrichment metadata"
        );
    }
}

#[test]
fn limit_flag_accepted() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let _snap_uid = create_ready_snapshot(&storage, "r1");

    // Test that --limit is accepted without error
    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--limit".to_string(),
        "10".to_string(),
    ]);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn force_flag_accepted() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let _snap_uid = create_ready_snapshot(&storage, "r1");

    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--force".to_string(),
    ]);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn promote_flag_accepted() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let _snap_uid = create_ready_snapshot(&storage, "r1");

    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--promote".to_string(),
    ]);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn language_filter_typescript_accepted() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let _snap_uid = create_ready_snapshot(&storage, "r1");

    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--language".to_string(),
        "typescript".to_string(),
    ]);
    assert_eq!(result, ExitCode::SUCCESS);
}

#[test]
fn language_filter_rust_accepted() {
    let (_tmp, storage, db_path) = open_temp_storage();
    insert_repo(&storage, "r1", "test-repo");
    let _snap_uid = create_ready_snapshot(&storage, "r1");

    let result = repo_graph_rgr::commands::run_enrich(&[
        db_path.to_string_lossy().to_string(),
        "r1".to_string(),
        "--language".to_string(),
        "rust".to_string(),
    ]);
    assert_eq!(result, ExitCode::SUCCESS);
}
