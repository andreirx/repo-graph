//! C-SB-1 integration test: end-to-end C state boundary validation.
//!
//! Validates that `rmap index` (via compose::index_path) produces
//! FS_PATH and DB_RESOURCE nodes with correct read/write direction
//! when indexed C source contains fopen(), open(), and sqlite3_open()
//! calls with literal string arguments.
//!
//! This test exercises the FULL production path:
//!   compose::index_path
//!     -> StateBoundaryHook (constructed in compose)
//!     -> orchestrator::index_repo with hook
//!     -> c-extractor produces ResolvedCallsite (C-SB-1)
//!     -> hook.on_extraction_result -> CAdapter -> state-extractor emit
//!     -> hook.drain_snapshot_extras -> merged into persistence
//!     -> SQLite DB contains FS_PATH/DB_RESOURCE nodes + READS/WRITES edges
//!
//! Test coverage:
//!   1. fopen with mode string produces FS_PATH with correct direction
//!   2. open with O_RDONLY/O_WRONLY/O_RDWR produces correct direction
//!   3. sqlite3_open produces DB_RESOURCE with read_write
//!   4. Dynamic path arguments produce no state-boundary facts
//!   5. Non-state-boundary calls (printf) produce no state-boundary facts

use std::fs;
use std::path::PathBuf;

use repo_graph_repo_index::compose::{index_path, ComposeOptions};
use repo_graph_storage::StorageConnection;

fn temp_repo(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    for (path, content) in files {
        let full = repo.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
    }
    (dir, repo)
}

// ── fopen: mode parsing produces correct direction ────────────────

#[test]
fn fopen_read_mode_produces_fs_path_with_read_direction() {
    let source = r#"
#include <stdio.h>

void read_config(void) {
    FILE* f = fopen("/etc/config.txt", "r");
    if (f) fclose(f);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/reader.c", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "c-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    // Verify FS_PATH node exists.
    let fs_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

    assert_eq!(fs_nodes.len(), 1, "expected one FS_PATH node");
    assert!(
        fs_nodes[0].stable_key.contains("/etc/config.txt"),
        "stable_key should contain path"
    );

    // Verify READS edge exists via find_direct_callees.
    let func_stable_key = "c-test:src/reader.c#read_config:SYMBOL:FUNCTION";
    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert_eq!(
        reads_callees.len(),
        1,
        "fopen 'r' should produce exactly one READS edge"
    );
    assert_eq!(
        reads_callees[0].stable_key,
        "c-test:fs:/etc/config.txt:FS_PATH"
    );

    // No WRITES edge for read-only.
    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert!(
        writes_callees.is_empty(),
        "fopen 'r' should not produce WRITES edge"
    );
}

#[test]
fn fopen_write_mode_produces_fs_path_with_write_direction() {
    let source = r#"
#include <stdio.h>

void write_log(void) {
    FILE* f = fopen("/var/log/app.log", "w");
    if (f) fclose(f);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/writer.c", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "c-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    // Verify WRITES edge exists.
    let func_stable_key = "c-test:src/writer.c#write_log:SYMBOL:FUNCTION";
    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert_eq!(
        writes_callees.len(),
        1,
        "fopen 'w' should produce exactly one WRITES edge"
    );

    // No READS edge for write-only.
    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert!(
        reads_callees.is_empty(),
        "fopen 'w' should not produce READS edge"
    );
}

#[test]
fn fopen_read_write_mode_produces_both_edges() {
    let source = r#"
#include <stdio.h>

void update_data(void) {
    FILE* f = fopen("/data/state.dat", "r+");
    if (f) fclose(f);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/updater.c", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "c-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "c-test:src/updater.c#update_data:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");

    assert_eq!(
        reads_callees.len(),
        1,
        "fopen 'r+' should produce READS edge"
    );
    assert_eq!(
        writes_callees.len(),
        1,
        "fopen 'r+' should produce WRITES edge"
    );
}

// ── open: flag parsing produces correct direction ─────────────────

#[test]
fn open_rdonly_produces_read_direction() {
    let source = r#"
#include <fcntl.h>

void read_device(void) {
    int fd = open("/dev/input0", O_RDONLY);
    if (fd >= 0) close(fd);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/device.c", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "c-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "c-test:src/device.c#read_device:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert_eq!(
        reads_callees.len(),
        1,
        "open O_RDONLY should produce READS edge"
    );

    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert!(
        writes_callees.is_empty(),
        "open O_RDONLY should not produce WRITES edge"
    );
}

// ── sqlite3_open: produces DB_RESOURCE with read_write ────────────

#[test]
fn sqlite3_open_produces_db_resource_node() {
    let source = r#"
#include <sqlite3.h>

int db_open(void) {
    sqlite3* db;
    int rc = sqlite3_open("app.db", &db);
    if (rc == 0) sqlite3_close(db);
    return rc;
}
"#;
    let (_dir, repo) = temp_repo(&[("src/database.c", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "c-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let db_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "DB_RESOURCE").collect();

    assert_eq!(
        db_nodes.len(),
        1,
        "sqlite3_open should produce DB_RESOURCE node"
    );
    assert!(
        db_nodes[0].stable_key.contains("app.db"),
        "stable_key should contain db path"
    );
    assert_eq!(db_nodes[0].subtype.as_deref(), Some("CONNECTION"));

    // read_write produces both READS and WRITES edges.
    let func_stable_key = "c-test:src/database.c#db_open:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");

    assert_eq!(
        reads_callees.len(),
        1,
        "sqlite3_open should produce READS edge"
    );
    assert_eq!(
        writes_callees.len(),
        1,
        "sqlite3_open should produce WRITES edge"
    );
}

// ── Negative cases ────────────────────────────────────────────────

#[test]
fn fopen_dynamic_path_produces_no_state_boundary() {
    let source = r#"
#include <stdio.h>

void read_file(char* path) {
    FILE* f = fopen(path, "r");
    if (f) fclose(f);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/dynamic.c", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "c-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let fs_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

    assert!(
        fs_nodes.is_empty(),
        "dynamic path should not produce FS_PATH node"
    );
}

#[test]
fn printf_produces_no_state_boundary() {
    let source = r#"
#include <stdio.h>

void log_message(void) {
    printf("Hello, world!\n");
}
"#;
    let (_dir, repo) = temp_repo(&[("src/logger.c", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "c-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let state_nodes: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind == "FS_PATH" || n.kind == "DB_RESOURCE")
        .collect();

    assert!(
        state_nodes.is_empty(),
        "printf should not produce state-boundary nodes"
    );
}

// ══════════════════════════════════════════════════════════════
//  Refresh-path tests (C-SB-1)
// ══════════════════════════════════════════════════════════════

use repo_graph_repo_index::compose::refresh_path;

#[test]
fn c_refresh_preserves_unchanged_file_state_boundary_facts() {
    let source = r#"
#include <stdio.h>

void read_config(void) {
    FILE* f = fopen("/etc/unchanged.conf", "r");
    if (f) fclose(f);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/config.c", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    // Full index.
    let idx =
        index_path(&repo, &db_path, "c-test", &ComposeOptions::default()).expect("full index");

    // Verify state-boundary facts exist after full index.
    let storage = StorageConnection::open(&db_path).unwrap();
    let idx_nodes = storage.query_all_nodes(&idx.snapshot_uid).unwrap();
    let idx_fs: Vec<_> = idx_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();
    assert_eq!(idx_fs.len(), 1, "full index must produce FS_PATH node");
    drop(storage);

    // Refresh (no file changes → all copied forward).
    let ref_result =
        refresh_path(&repo, &db_path, "c-test", &ComposeOptions::default()).expect("refresh");

    let storage = StorageConnection::open(&db_path).unwrap();
    let ref_nodes = storage.query_all_nodes(&ref_result.snapshot_uid).unwrap();
    let ref_fs: Vec<_> = ref_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();
    assert_eq!(
        ref_fs.len(),
        1,
        "refresh must preserve FS_PATH node via copy-forward"
    );
    assert_eq!(ref_fs[0].name, "/etc/unchanged.conf");

    // Verify the READS edge also survived refresh.
    let func_key = "c-test:src/config.c#read_config:SYMBOL:FUNCTION";
    let callees = storage
        .find_direct_callees(&ref_result.snapshot_uid, func_key, &["READS"])
        .unwrap();
    assert_eq!(
        callees.len(),
        1,
        "refresh must preserve READS edge via copy-forward"
    );
}

#[test]
fn c_refresh_mixed_unchanged_and_changed_files() {
    // Initial: two files, each touching a different FS resource.
    let src_a = r#"
#include <stdio.h>

void read_a(void) {
    FILE* f = fopen("/etc/a.conf", "r");
    if (f) fclose(f);
}
"#;
    let src_b = r#"
#include <stdio.h>

void write_b(void) {
    FILE* f = fopen("/var/log/b.log", "w");
    if (f) fclose(f);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/a.c", src_a), ("src/b.c", src_b)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    // Full index.
    let _idx =
        index_path(&repo, &db_path, "c-test", &ComposeOptions::default()).expect("full index");

    // Modify file b (change the path).
    let src_b2 = r#"
#include <stdio.h>

void write_b(void) {
    FILE* f = fopen("/var/log/b2.log", "w");
    if (f) fclose(f);
}
"#;
    fs::write(repo.join("src/b.c"), src_b2).unwrap();

    // Refresh: a.c is unchanged → copy-forward; b.c changed.
    let ref_result =
        refresh_path(&repo, &db_path, "c-test", &ComposeOptions::default()).expect("refresh");

    let storage = StorageConnection::open(&db_path).unwrap();
    let ref_nodes = storage.query_all_nodes(&ref_result.snapshot_uid).unwrap();
    let ref_fs: Vec<_> = ref_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

    let names: Vec<&str> = ref_fs.iter().map(|n| n.name.as_str()).collect();
    assert!(
        names.contains(&"/etc/a.conf"),
        "unchanged file's resource must survive refresh, got: {:?}",
        names
    );
    assert!(
        names.contains(&"/var/log/b2.log"),
        "changed file's new resource must appear, got: {:?}",
        names
    );
    // Stale orphan: /var/log/b.log persists (documented residual debt).
    assert!(
        names.contains(&"/var/log/b.log"),
        "stale orphan resource should persist until full reindex, got: {:?}",
        names
    );
}

#[test]
fn c_refresh_deduplicates_shared_resource_across_changed_and_unchanged() {
    // Both files read the SAME resource. After refresh with one
    // file changed (but still reading the same path), the
    // resource node must appear exactly once.
    let src_a = r#"
#include <stdio.h>

void read_a(void) {
    FILE* f = fopen("/etc/shared.conf", "r");
    if (f) fclose(f);
}
"#;
    let src_b = r#"
#include <stdio.h>

void read_b(void) {
    FILE* f = fopen("/etc/shared.conf", "r");
    if (f) fclose(f);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/a.c", src_a), ("src/b.c", src_b)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let _idx =
        index_path(&repo, &db_path, "c-test", &ComposeOptions::default()).expect("full index");

    // Modify b.c trivially (add a comment so the hash changes).
    let src_b2 = r#"
// changed
#include <stdio.h>

void read_b(void) {
    FILE* f = fopen("/etc/shared.conf", "r");
    if (f) fclose(f);
}
"#;
    fs::write(repo.join("src/b.c"), src_b2).unwrap();

    let ref_result =
        refresh_path(&repo, &db_path, "c-test", &ComposeOptions::default()).expect("refresh");

    let storage = StorageConnection::open(&db_path).unwrap();
    let ref_nodes = storage.query_all_nodes(&ref_result.snapshot_uid).unwrap();
    let shared_nodes: Vec<_> = ref_nodes
        .iter()
        .filter(|n| n.kind == "FS_PATH" && n.name == "/etc/shared.conf")
        .collect();
    assert_eq!(
        shared_nodes.len(),
        1,
        "shared resource must appear exactly once after refresh (dedup), got: {:?}",
        shared_nodes
            .iter()
            .map(|n| &n.stable_key)
            .collect::<Vec<_>>()
    );
}
