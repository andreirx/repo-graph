//! CPP-SB-1 integration test: end-to-end C++ state boundary validation.
//!
//! Validates that `rmap index` (via compose::index_path) produces
//! FS_PATH and DB_RESOURCE nodes with correct read/write direction
//! when indexed C++ source contains:
//!   - Stream constructors (ifstream, ofstream, fstream)
//!   - Stream .open() calls (D3 local type map resolution)
//!   - C-style APIs (fopen, open, sqlite3_open) from duplicated cpp bindings
//!
//! This test exercises the FULL production path:
//!   compose::index_path
//!     -> StateBoundaryHook (constructed in compose)
//!     -> orchestrator::index_repo with hook
//!     -> cpp-extractor produces ResolvedCallsite (CPP-SB-1)
//!     -> hook.on_extraction_result -> CppAdapter -> state-extractor emit
//!     -> hook.drain_snapshot_extras -> merged into persistence
//!     -> SQLite DB contains FS_PATH/DB_RESOURCE nodes + READS/WRITES edges
//!
//! Test coverage:
//!   1. ifstream constructor produces FS_PATH with read direction
//!   2. ofstream constructor produces FS_PATH with write direction
//!   3. fstream constructor with mode produces correct direction
//!   4. stream.open() with D3 local type map produces correct direction
//!   5. C-style APIs (fopen, open, sqlite3_open) work in .cpp files
//!   6. Negative limits: parameter receiver, alias, factory, member, dynamic
//!   7. Refresh path: unchanged preservation, mixed, dedup

use std::fs;
use std::path::PathBuf;

use repo_graph_repo_index::compose::{index_path, refresh_path, ComposeOptions};
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

// ══════════════════════════════════════════════════════════════════
//  Constructor path tests
// ══════════════════════════════════════════════════════════════════

#[test]
fn ifstream_constructor_produces_fs_path_with_read_direction() {
    let source = r#"
#include <fstream>

void read_config() {
    std::ifstream file("/etc/config.ini");
    // read from file
}
"#;
    let (_dir, repo) = temp_repo(&[("src/reader.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let fs_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

    assert_eq!(fs_nodes.len(), 1, "expected one FS_PATH node");
    assert!(
        fs_nodes[0].stable_key.contains("/etc/config.ini"),
        "stable_key should contain path"
    );

    // Verify READS edge exists.
    let func_stable_key = "cpp-test:src/reader.cpp#read_config:SYMBOL:FUNCTION";
    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert_eq!(
        reads_callees.len(),
        1,
        "ifstream should produce exactly one READS edge"
    );

    // No WRITES edge for ifstream.
    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert!(
        writes_callees.is_empty(),
        "ifstream should not produce WRITES edge"
    );
}

#[test]
fn ofstream_constructor_produces_fs_path_with_write_direction() {
    let source = r#"
#include <fstream>

void write_log() {
    std::ofstream file("/var/log/app.log");
    // write to file
}
"#;
    let (_dir, repo) = temp_repo(&[("src/writer.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    // Verify WRITES edge exists.
    let func_stable_key = "cpp-test:src/writer.cpp#write_log:SYMBOL:FUNCTION";
    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert_eq!(
        writes_callees.len(),
        1,
        "ofstream should produce exactly one WRITES edge"
    );

    // No READS edge for ofstream.
    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert!(
        reads_callees.is_empty(),
        "ofstream should not produce READS edge"
    );
}

#[test]
fn fstream_constructor_with_read_mode_produces_read_direction() {
    let source = r#"
#include <fstream>

void read_data() {
    std::fstream file("/data/input.bin", std::ios::in);
    // read from file
}
"#;
    let (_dir, repo) = temp_repo(&[("src/data.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "cpp-test:src/data.cpp#read_data:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert_eq!(
        reads_callees.len(),
        1,
        "fstream std::ios::in should produce READS edge"
    );

    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert!(
        writes_callees.is_empty(),
        "fstream std::ios::in should not produce WRITES edge"
    );
}

#[test]
fn fstream_constructor_with_write_mode_produces_write_direction() {
    let source = r#"
#include <fstream>

void write_data() {
    std::fstream file("/data/output.bin", std::ios::out);
    // write to file
}
"#;
    let (_dir, repo) = temp_repo(&[("src/data.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "cpp-test:src/data.cpp#write_data:SYMBOL:FUNCTION";

    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert_eq!(
        writes_callees.len(),
        1,
        "fstream std::ios::out should produce WRITES edge"
    );

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert!(
        reads_callees.is_empty(),
        "fstream std::ios::out should not produce READS edge"
    );
}

#[test]
fn fstream_constructor_with_inout_mode_produces_both_edges() {
    let source = r#"
#include <fstream>

void update_data() {
    std::fstream file("/data/state.bin", std::ios::in | std::ios::out);
    // read and write
}
"#;
    let (_dir, repo) = temp_repo(&[("src/update.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "cpp-test:src/update.cpp#update_data:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");

    assert_eq!(
        reads_callees.len(),
        1,
        "fstream in|out should produce READS edge"
    );
    assert_eq!(
        writes_callees.len(),
        1,
        "fstream in|out should produce WRITES edge"
    );
}

// ══════════════════════════════════════════════════════════════════
//  .open() path tests (D3 local type map)
// ══════════════════════════════════════════════════════════════════

#[test]
fn ifstream_open_via_d3_local_type_map() {
    let source = r#"
#include <fstream>

void read_late() {
    std::ifstream file;
    file.open("/etc/late.conf");
    // read from file
}
"#;
    let (_dir, repo) = temp_repo(&[("src/late.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "cpp-test:src/late.cpp#read_late:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert_eq!(
        reads_callees.len(),
        1,
        "ifstream.open() via D3 should produce READS edge"
    );

    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert!(
        writes_callees.is_empty(),
        "ifstream.open() should not produce WRITES edge"
    );
}

#[test]
fn ofstream_open_via_d3_local_type_map() {
    let source = r#"
#include <fstream>

void write_late() {
    std::ofstream file;
    file.open("/var/log/late.log");
    // write to file
}
"#;
    let (_dir, repo) = temp_repo(&[("src/late.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "cpp-test:src/late.cpp#write_late:SYMBOL:FUNCTION";

    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");
    assert_eq!(
        writes_callees.len(),
        1,
        "ofstream.open() via D3 should produce WRITES edge"
    );

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert!(
        reads_callees.is_empty(),
        "ofstream.open() should not produce READS edge"
    );
}

#[test]
fn fstream_open_with_mode_via_d3_local_type_map() {
    let source = r#"
#include <fstream>

void update_late() {
    std::fstream file;
    file.open("/data/late.bin", std::ios::in | std::ios::out);
    // read and write
}
"#;
    let (_dir, repo) = temp_repo(&[("src/late.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "cpp-test:src/late.cpp#update_late:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    let writes_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["WRITES"])
        .expect("callee query must succeed");

    assert_eq!(
        reads_callees.len(),
        1,
        "fstream.open() with in|out should produce READS edge"
    );
    assert_eq!(
        writes_callees.len(),
        1,
        "fstream.open() with in|out should produce WRITES edge"
    );
}

// ══════════════════════════════════════════════════════════════════
//  C-style APIs in C++ files (duplicated cpp bindings)
// ══════════════════════════════════════════════════════════════════

#[test]
fn fopen_in_cpp_file_produces_fs_path() {
    let source = r#"
#include <cstdio>

void read_c_style() {
    FILE* f = fopen("/etc/c_style.conf", "r");
    if (f) fclose(f);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/cstyle.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "cpp-test:src/cstyle.cpp#read_c_style:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert_eq!(
        reads_callees.len(),
        1,
        "fopen in .cpp file should produce READS edge"
    );
}

#[test]
fn open_in_cpp_file_produces_fs_path() {
    let source = r#"
#include <fcntl.h>

void read_fd() {
    int fd = open("/dev/input", O_RDONLY);
    if (fd >= 0) close(fd);
}
"#;
    let (_dir, repo) = temp_repo(&[("src/fd.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();

    let func_stable_key = "cpp-test:src/fd.cpp#read_fd:SYMBOL:FUNCTION";

    let reads_callees = storage
        .find_direct_callees(&result.snapshot_uid, func_stable_key, &["READS"])
        .expect("callee query must succeed");
    assert_eq!(
        reads_callees.len(),
        1,
        "open() in .cpp file should produce READS edge"
    );
}

#[test]
fn sqlite3_open_in_cpp_file_produces_db_resource() {
    let source = r#"
#include <sqlite3.h>

int db_init() {
    sqlite3* db;
    int rc = sqlite3_open("app.db", &db);
    if (rc == 0) sqlite3_close(db);
    return rc;
}
"#;
    let (_dir, repo) = temp_repo(&[("src/db.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
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

    // read_write produces both edges.
    let func_stable_key = "cpp-test:src/db.cpp#db_init:SYMBOL:FUNCTION";

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

// ══════════════════════════════════════════════════════════════════
//  Negative limit tests (D3 explicit limits)
// ══════════════════════════════════════════════════════════════════

#[test]
fn parameter_receiver_produces_no_state_boundary() {
    // D3 limit: parameter receivers are excluded.
    let source = r#"
#include <fstream>

void process(std::ifstream& stream) {
    stream.open("/etc/param.conf");
    // parameter receiver - should NOT emit
}
"#;
    let (_dir, repo) = temp_repo(&[("src/param.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let fs_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

    assert!(
        fs_nodes.is_empty(),
        "parameter receiver should not produce FS_PATH (D3 limit)"
    );
}

#[test]
fn factory_return_produces_no_state_boundary() {
    // D3 limit: factory return receivers are excluded.
    let source = r#"
#include <fstream>

std::ifstream getStream();

void use_factory() {
    getStream().open("/etc/factory.conf");
    // factory return - should NOT emit
}
"#;
    let (_dir, repo) = temp_repo(&[("src/factory.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let fs_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

    assert!(
        fs_nodes.is_empty(),
        "factory return receiver should not produce FS_PATH (D3 limit)"
    );
}

#[test]
fn member_field_receiver_produces_no_state_boundary() {
    // D3 limit: member field receivers are excluded.
    let source = r#"
#include <fstream>

class FileHandler {
    std::ifstream stream;

    void open_file() {
        stream.open("/etc/member.conf");
        // member field - should NOT emit
    }
};
"#;
    let (_dir, repo) = temp_repo(&[("src/member.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let fs_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

    assert!(
        fs_nodes.is_empty(),
        "member field receiver should not produce FS_PATH (D3 limit)"
    );
}

#[test]
fn dynamic_path_produces_no_state_boundary() {
    // Dynamic paths are excluded (not deterministic).
    let source = r#"
#include <fstream>
#include <string>

void read_dynamic(const std::string& path) {
    std::ifstream file(path);
    // dynamic path - should NOT emit
}
"#;
    let (_dir, repo) = temp_repo(&[("src/dynamic.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let fs_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

    assert!(
        fs_nodes.is_empty(),
        "dynamic path should not produce FS_PATH"
    );
}

#[test]
fn sqlite3_memory_produces_no_state_boundary() {
    // :memory: databases are in-memory only, not persistent state.
    let source = r#"
#include <sqlite3.h>

int db_memory() {
    sqlite3* db;
    int rc = sqlite3_open(":memory:", &db);
    if (rc == 0) sqlite3_close(db);
    return rc;
}
"#;
    let (_dir, repo) = temp_repo(&[("src/memory.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let result = index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default())
        .expect("indexing must succeed");

    let storage = StorageConnection::open(&db_path).unwrap();
    let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

    let db_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "DB_RESOURCE").collect();

    assert!(
        db_nodes.is_empty(),
        ":memory: should not produce DB_RESOURCE"
    );
}

// ══════════════════════════════════════════════════════════════════
//  Refresh-path tests (CPP-SB-1)
// ══════════════════════════════════════════════════════════════════

#[test]
fn cpp_refresh_preserves_unchanged_file_state_boundary_facts() {
    let source = r#"
#include <fstream>

void read_config() {
    std::ifstream file("/etc/unchanged.conf");
    // read from file
}
"#;
    let (_dir, repo) = temp_repo(&[("src/config.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    // Full index.
    let idx =
        index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default()).expect("full index");

    // Verify state-boundary facts exist after full index.
    let storage = StorageConnection::open(&db_path).unwrap();
    let idx_nodes = storage.query_all_nodes(&idx.snapshot_uid).unwrap();
    let idx_fs: Vec<_> = idx_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();
    assert_eq!(idx_fs.len(), 1, "full index must produce FS_PATH node");
    drop(storage);

    // Refresh (no file changes -> all copied forward).
    let ref_result =
        refresh_path(&repo, &db_path, "cpp-test", &ComposeOptions::default()).expect("refresh");

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
    let func_key = "cpp-test:src/config.cpp#read_config:SYMBOL:FUNCTION";
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
fn cpp_refresh_mixed_unchanged_and_changed_files() {
    // Initial: two files, each touching a different FS resource.
    let src_a = r#"
#include <fstream>

void read_a() {
    std::ifstream file("/etc/a.conf");
}
"#;
    let src_b = r#"
#include <fstream>

void write_b() {
    std::ofstream file("/var/log/b.log");
}
"#;
    let (_dir, repo) = temp_repo(&[("src/a.cpp", src_a), ("src/b.cpp", src_b)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    // Full index.
    let _idx =
        index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default()).expect("full index");

    // Modify file b (change the path).
    let src_b2 = r#"
#include <fstream>

void write_b() {
    std::ofstream file("/var/log/b2.log");
}
"#;
    fs::write(repo.join("src/b.cpp"), src_b2).unwrap();

    // Refresh: a.cpp is unchanged -> copy-forward; b.cpp changed.
    let ref_result =
        refresh_path(&repo, &db_path, "cpp-test", &ComposeOptions::default()).expect("refresh");

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
fn cpp_refresh_deduplicates_shared_resource_across_changed_and_unchanged() {
    // Both files read the SAME resource. After refresh with one
    // file changed (but still reading the same path), the
    // resource node must appear exactly once.
    let src_a = r#"
#include <fstream>

void read_a() {
    std::ifstream file("/etc/shared.conf");
}
"#;
    let src_b = r#"
#include <fstream>

void read_b() {
    std::ifstream file("/etc/shared.conf");
}
"#;
    let (_dir, repo) = temp_repo(&[("src/a.cpp", src_a), ("src/b.cpp", src_b)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    let _idx =
        index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default()).expect("full index");

    // Modify b.cpp trivially (add a comment so the hash changes).
    let src_b2 = r#"
// changed
#include <fstream>

void read_b() {
    std::ifstream file("/etc/shared.conf");
}
"#;
    fs::write(repo.join("src/b.cpp"), src_b2).unwrap();

    let ref_result =
        refresh_path(&repo, &db_path, "cpp-test", &ComposeOptions::default()).expect("refresh");

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

#[test]
fn cpp_refresh_preserves_d3_open_facts() {
    // Validates that .open() facts via D3 local type map survive refresh.
    let source = r#"
#include <fstream>

void read_late() {
    std::ifstream file;
    file.open("/etc/d3_refresh.conf");
}
"#;
    let (_dir, repo) = temp_repo(&[("src/d3.cpp", source)]);
    let db_dir = tempfile::tempdir().unwrap();
    let db_path = db_dir.path().join("test.db");

    // Full index.
    let idx =
        index_path(&repo, &db_path, "cpp-test", &ComposeOptions::default()).expect("full index");

    let storage = StorageConnection::open(&db_path).unwrap();
    let idx_nodes = storage.query_all_nodes(&idx.snapshot_uid).unwrap();
    let idx_fs: Vec<_> = idx_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();
    assert_eq!(
        idx_fs.len(),
        1,
        "D3 .open() must produce FS_PATH in full index"
    );
    drop(storage);

    // Refresh (no changes).
    let ref_result =
        refresh_path(&repo, &db_path, "cpp-test", &ComposeOptions::default()).expect("refresh");

    let storage = StorageConnection::open(&db_path).unwrap();
    let ref_nodes = storage.query_all_nodes(&ref_result.snapshot_uid).unwrap();
    let ref_fs: Vec<_> = ref_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();
    assert_eq!(ref_fs.len(), 1, "D3 .open() facts must survive refresh");
    assert_eq!(ref_fs[0].name, "/etc/d3_refresh.conf");
}
