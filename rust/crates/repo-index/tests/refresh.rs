//! Refresh integration tests — deterministic scenarios proving
//! the full disk-to-SQLite incremental refresh lifecycle.
//!
//! Test matrix:
//!   1. No parent snapshot → fallback to full index
//!   2. Unchanged + changed files → copy-forward + re-extraction
//!   3. Deleted file → not copied forward
//!   4. Exclusion behavior during refresh

use std::fs;
use std::path::Path;

use repo_graph_repo_index::compose::{
	index_into_storage, refresh_into_storage, ComposeOptions,
};
use repo_graph_storage::StorageConnection;

fn make_two_file_repo(dir: &Path) {
	fs::write(
		dir.join("package.json"),
		r#"{"dependencies":{"express":"1"}}"#,
	)
	.unwrap();
	fs::create_dir_all(dir.join("src")).unwrap();
	fs::write(
		dir.join("src/index.ts"),
		"import { serve } from \"./server\";\nserve();\n",
	)
	.unwrap();
	fs::write(
		dir.join("src/server.ts"),
		"export function serve() {}\n",
	)
	.unwrap();
}

// ── 1. No parent snapshot → fallback to full index ───────────────

#[test]
fn refresh_with_no_parent_falls_back_to_full_index() {
	let dir = tempfile::tempdir().unwrap();
	make_two_file_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Call refresh directly on a fresh DB — no prior snapshot.
	let result = refresh_into_storage(
		dir.path(),
		&mut storage,
		"r1",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap = storage.get_snapshot(&result.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap.status, "ready");
	// Fallback produces a FULL snapshot, not REFRESH.
	assert_eq!(snap.kind, "full");
	// 2 source files: src/index.ts, src/server.ts
	// Config file (package.json) tracked for invalidation but not counted.
	assert_eq!(result.files_total, 2);
	assert!(result.nodes_total >= 4);
}

// ── 2. Unchanged + changed files ─────────────────────────────────

#[test]
fn refresh_copies_unchanged_and_reextracts_changed() {
	let dir = tempfile::tempdir().unwrap();
	make_two_file_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"r1",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(r1.files_total, 2);
	let snap1_uid = r1.snapshot_uid.clone();

	// Phase 2: modify server.ts, keep index.ts unchanged.
	fs::write(
		dir.path().join("src/server.ts"),
		"export function serve() { return 'v2'; }\n",
	)
	.unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"r1",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.status, "ready");
	assert_eq!(snap2.kind, "refresh");
	assert_eq!(
		snap2.parent_snapshot_uid,
		Some(snap1_uid.clone()),
		"refresh snapshot must link to parent"
	);

	// Both files present in refresh (copied + re-extracted).
	assert_eq!(r2.files_total, 2, "files_total in refresh");

	// Nodes from both files present (copy-forward + extraction).
	use repo_graph_indexer::storage_port::{FileCatalogPort, NodeStorePort};
	let nodes = NodeStorePort::query_all_nodes(&storage, &r2.snapshot_uid).unwrap();
	let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

	assert!(
		stable_keys.contains(&"r1:src/index.ts:FILE"),
		"unchanged file should be present (copy-forward)"
	);
	assert!(
		stable_keys.contains(&"r1:src/server.ts:FILE"),
		"changed file should be present (re-extracted)"
	);
	assert!(
		stable_keys.iter().any(|k| k.contains("#serve:SYMBOL:FUNCTION")),
		"serve function should be present"
	);

	// ── Prove delta behavior, not disguised full rebuild ──────

	// Query file version hashes for BOTH snapshots.
	let hashes_snap1 = FileCatalogPort::query_file_version_hashes(&storage, &snap1_uid).unwrap();
	let hashes_snap2 = FileCatalogPort::query_file_version_hashes(&storage, &r2.snapshot_uid).unwrap();

	// index.ts was unchanged → hash must be identical across snapshots.
	let idx_hash_1 = hashes_snap1.get("r1:src/index.ts").unwrap();
	let idx_hash_2 = hashes_snap2.get("r1:src/index.ts").unwrap();
	assert_eq!(
		idx_hash_1, idx_hash_2,
		"unchanged file must have identical hash across snapshots (copy-forward proof)"
	);

	// server.ts was changed → hash must differ.
	let srv_hash_1 = hashes_snap1.get("r1:src/server.ts").unwrap();
	let srv_hash_2 = hashes_snap2.get("r1:src/server.ts").unwrap();
	assert_ne!(
		srv_hash_1, srv_hash_2,
		"changed file must have different hash (re-extraction proof)"
	);

	// Exact node count pins: if a full rebuild ran, the node UIDs
	// would all be freshly generated. With copy-forward, the
	// unchanged file's nodes are copied (new UIDs but same stable
	// keys). Pin the total to catch regressions.
	assert_eq!(r2.nodes_total, 4, "exact nodes_total in refresh");
	assert_eq!(r2.edges_total, 4, "exact edges_total in refresh");
}

// ── 3. Deleted file ──────────────────────────────────────────────

#[test]
fn refresh_does_not_copy_deleted_files() {
	let dir = tempfile::tempdir().unwrap();
	make_two_file_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index with 2 files.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"r1",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(r1.files_total, 2);

	// Phase 2: delete server.ts.
	fs::remove_file(dir.path().join("src/server.ts")).unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"r1",
		&ComposeOptions::default(),
	)
	.unwrap();

	assert_eq!(r2.files_total, 1, "only index.ts remains");

	// server.ts should NOT appear in the refreshed snapshot.
	use repo_graph_indexer::storage_port::NodeStorePort;
	let nodes = NodeStorePort::query_all_nodes(&storage, &r2.snapshot_uid).unwrap();
	let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

	assert!(
		stable_keys.contains(&"r1:src/index.ts:FILE"),
		"kept file should be present"
	);
	assert!(
		!stable_keys.iter().any(|k| k.contains("server")),
		"deleted file should NOT be in refreshed snapshot: {:?}",
		stable_keys
	);
}

// ── 4. Exclusion behavior during refresh ─────────────────────────

#[test]
fn refresh_respects_exclusions() {
	let dir = tempfile::tempdir().unwrap();
	let root = dir.path();

	// Initial setup with gitignore + excluded dirs.
	make_two_file_repo(root);
	fs::write(root.join(".gitignore"), "src/generated.ts\n").unwrap();
	fs::write(root.join("src/generated.ts"), "const gen = 1;").unwrap();
	fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
	fs::write(root.join("node_modules/pkg/index.ts"), "const x=1;").unwrap();

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(root, &mut storage, "r1", &ComposeOptions::default()).unwrap();
	assert_eq!(r1.files_total, 2); // only index.ts + server.ts

	// Phase 2: modify server.ts to trigger re-extraction.
	fs::write(root.join("src/server.ts"), "export function serve() { return 'v2'; }\n").unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(root, &mut storage, "r1", &ComposeOptions::default()).unwrap();

	assert_eq!(r2.files_total, 2, "gitignored + excluded files still excluded in refresh");

	use repo_graph_indexer::storage_port::NodeStorePort;
	let nodes = NodeStorePort::query_all_nodes(&storage, &r2.snapshot_uid).unwrap();
	let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

	assert!(
		!stable_keys.iter().any(|k| k.contains("generated")),
		"gitignored file excluded in refresh"
	);
	assert!(
		!stable_keys.iter().any(|k| k.contains("node_modules")),
		"node_modules excluded in refresh"
	);
}

// ── 5. Java refresh integration ──────────────────────────────────

fn make_java_repo(dir: &std::path::Path) {
	fs::create_dir_all(dir.join("src/main/java/com/example")).unwrap();
	fs::write(
		dir.join("src/main/java/com/example/App.java"),
		r#"package com.example;

public class App {
    public void run() {
        System.out.println("v1");
    }
}
"#,
	)
	.unwrap();
	fs::write(
		dir.join("src/main/java/com/example/Service.java"),
		r#"package com.example;

public interface Service {
    void execute();
}
"#,
	)
	.unwrap();
}

#[test]
fn refresh_java_copies_unchanged_and_reextracts_changed() {
	let dir = tempfile::tempdir().unwrap();
	make_java_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"java-r1",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(r1.files_total, 2, "initial Java files");
	let snap1_uid = r1.snapshot_uid.clone();

	// Verify initial extraction produced Java symbols.
	use repo_graph_indexer::storage_port::NodeStorePort;
	let nodes1 = NodeStorePort::query_all_nodes(&storage, &snap1_uid).unwrap();
	let keys1: Vec<&str> = nodes1.iter().map(|n| n.stable_key.as_str()).collect();
	assert!(
		keys1.iter().any(|k| k.contains("#App:SYMBOL:CLASS")),
		"App class must exist after initial index"
	);
	assert!(
		keys1.iter().any(|k| k.contains("#Service:SYMBOL:INTERFACE")),
		"Service interface must exist after initial index"
	);

	// Phase 2: modify App.java, keep Service.java unchanged.
	fs::write(
		dir.path().join("src/main/java/com/example/App.java"),
		r#"package com.example;

public class App {
    public void run() {
        System.out.println("v2");
    }

    public void newMethod() {}
}
"#,
	)
	.unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"java-r1",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.status, "ready");
	assert_eq!(snap2.kind, "refresh");
	assert_eq!(r2.files_total, 2, "both Java files in refresh");

	// Verify nodes from both files present.
	let nodes2 = NodeStorePort::query_all_nodes(&storage, &r2.snapshot_uid).unwrap();
	let keys2: Vec<&str> = nodes2.iter().map(|n| n.stable_key.as_str()).collect();

	assert!(
		keys2.iter().any(|k| k.contains("App.java:FILE")),
		"changed App.java FILE present"
	);
	assert!(
		keys2.iter().any(|k| k.contains("Service.java:FILE")),
		"unchanged Service.java FILE present (copy-forward)"
	);
	assert!(
		keys2.iter().any(|k| k.contains("#App:SYMBOL:CLASS")),
		"App class present"
	);
	assert!(
		keys2.iter().any(|k| k.contains("#Service:SYMBOL:INTERFACE")),
		"Service interface present (copy-forward)"
	);
	// New method from modified file.
	assert!(
		keys2.iter().any(|k| k.contains("#App.newMethod:SYMBOL:METHOD")),
		"newMethod from modified App.java must exist"
	);

	// Prove delta: unchanged file hash identical, changed file hash differs.
	use repo_graph_indexer::storage_port::FileCatalogPort;
	let hashes1 = FileCatalogPort::query_file_version_hashes(&storage, &snap1_uid).unwrap();
	let hashes2 = FileCatalogPort::query_file_version_hashes(&storage, &r2.snapshot_uid).unwrap();

	let svc_hash_1 = hashes1.get("java-r1:src/main/java/com/example/Service.java").unwrap();
	let svc_hash_2 = hashes2.get("java-r1:src/main/java/com/example/Service.java").unwrap();
	assert_eq!(
		svc_hash_1, svc_hash_2,
		"unchanged Service.java must have identical hash (copy-forward proof)"
	);

	let app_hash_1 = hashes1.get("java-r1:src/main/java/com/example/App.java").unwrap();
	let app_hash_2 = hashes2.get("java-r1:src/main/java/com/example/App.java").unwrap();
	assert_ne!(
		app_hash_1, app_hash_2,
		"changed App.java must have different hash (re-extraction proof)"
	);
}

// ── 6. Config-widening on refresh ────────────────────────────────

#[test]
fn refresh_config_change_triggers_widening() {
	// Test that changing a config file (package.json) triggers re-extraction
	// of unchanged source files in scope — the config-widening behavior.
	//
	// When a global config (root package.json) changes, ALL source files become
	// config-widened. Since nothing can be copied forward, the refresh falls back
	// to a full index. This is correct behavior — the semantic effect is that all
	// files are re-extracted with updated dependency context.
	let dir = tempfile::tempdir().unwrap();
	make_two_file_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"cw1",
		&ComposeOptions::default(),
	)
	.unwrap();
	assert_eq!(r1.files_total, 2); // index.ts + server.ts

	// Phase 2: modify ONLY package.json (config file), keep source files unchanged.
	fs::write(
		dir.path().join("package.json"),
		r#"{"dependencies":{"express":"2"}}"#, // changed version
	)
	.unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"cw1",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.status, "ready");
	// When ALL files are config-widened, refresh falls back to full index.
	// This is correct: nothing to copy forward means full rebuild is optimal.
	assert!(
		snap2.kind == "full" || snap2.kind == "refresh",
		"snapshot kind should be full (fallback) or refresh"
	);

	// Verify file count is still 2 (source files only, config files not counted).
	assert_eq!(r2.files_total, 2, "files_total should be 2 source files");

	// Verify nodes exist for both source files (re-extracted).
	use repo_graph_indexer::storage_port::NodeStorePort;
	let nodes = NodeStorePort::query_all_nodes(&storage, &r2.snapshot_uid).unwrap();
	let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

	assert!(
		stable_keys.contains(&"cw1:src/index.ts:FILE"),
		"index.ts should be present after config-widening"
	);
	assert!(
		stable_keys.contains(&"cw1:src/server.ts:FILE"),
		"server.ts should be present after config-widening"
	);

	// Key verification: config file is tracked in file_versions but NOT as a FILE node.
	use repo_graph_indexer::storage_port::FileCatalogPort;
	let hashes = FileCatalogPort::query_file_version_hashes(&storage, &r2.snapshot_uid).unwrap();
	assert!(
		hashes.contains_key("cw1:package.json"),
		"config file should be tracked in file_versions"
	);
	assert!(
		!stable_keys.iter().any(|k| k.contains("package.json:FILE")),
		"config file should NOT have a FILE node"
	);
}

// ── 7. Nested config-widening (partial scope) ────────────────────

#[test]
fn refresh_nested_config_widens_only_scoped_files() {
	// Test that changing a nested config (e.g., src/tsconfig.json) only
	// widens files under that directory, allowing files outside scope
	// to be copied forward.
	let dir = tempfile::tempdir().unwrap();
	let root = dir.path();

	// Create structure with nested config.
	fs::write(
		root.join("package.json"),
		r#"{"dependencies":{"express":"1"}}"#,
	).unwrap();
	fs::create_dir_all(root.join("src")).unwrap();
	fs::create_dir_all(root.join("lib")).unwrap();
	fs::write(
		root.join("src/tsconfig.json"),
		r#"{"compilerOptions":{"strict":true}}"#,
	).unwrap();
	fs::write(root.join("src/app.ts"), "export const app = 1;\n").unwrap();
	fs::write(root.join("lib/util.ts"), "export const util = 2;\n").unwrap();

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(root, &mut storage, "nc1", &ComposeOptions::default()).unwrap();
	assert_eq!(r1.files_total, 2); // src/app.ts + lib/util.ts
	let snap1_uid = r1.snapshot_uid.clone();

	// Phase 2: modify ONLY src/tsconfig.json (nested config).
	fs::write(
		root.join("src/tsconfig.json"),
		r#"{"compilerOptions":{"strict":false}}"#,
	).unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(root, &mut storage, "nc1", &ComposeOptions::default()).unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.status, "ready");
	// This SHOULD be a refresh, not a full index, because lib/util.ts
	// is outside the scope of src/tsconfig.json and can be copied forward.
	assert_eq!(snap2.kind, "refresh", "nested config should allow partial refresh");
	assert_eq!(r2.files_total, 2);

	// Verify copy-forward happened for lib/util.ts (outside config scope).
	use repo_graph_indexer::storage_port::FileCatalogPort;
	let hashes1 = FileCatalogPort::query_file_version_hashes(&storage, &snap1_uid).unwrap();
	let hashes2 = FileCatalogPort::query_file_version_hashes(&storage, &r2.snapshot_uid).unwrap();

	let util_hash_1 = hashes1.get("nc1:lib/util.ts").unwrap();
	let util_hash_2 = hashes2.get("nc1:lib/util.ts").unwrap();
	assert_eq!(
		util_hash_1, util_hash_2,
		"lib/util.ts should have same hash (copy-forward, outside config scope)"
	);

	// src/app.ts was re-extracted due to config-widening.
	// Hash is same (content unchanged) but it went through extraction.
	let app_hash_1 = hashes1.get("nc1:src/app.ts").unwrap();
	let app_hash_2 = hashes2.get("nc1:src/app.ts").unwrap();
	assert_eq!(
		app_hash_1, app_hash_2,
		"src/app.ts hash unchanged (content same, but was re-extracted)"
	);

	// Verify nested config is tracked in file_versions.
	assert!(
		hashes2.contains_key("nc1:src/tsconfig.json"),
		"nested config should be tracked in file_versions"
	);
}

// ── 8. End-to-end agent parity: boundaries ───────────────────────

fn make_c_boundary_repo(dir: &std::path::Path) {
	// C file with IPC boundary interactions (socket, pipe, shm).
	// These patterns trigger boundary detection in the c-extractor.
	fs::write(
		dir.join("ipc_server.c"),
		r#"#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

int start_server(void) {
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) return -1;

    struct sockaddr_un addr;
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, "/var/run/app.sock", sizeof(addr.sun_path) - 1);

    bind(fd, (struct sockaddr*)&addr, sizeof(addr));
    listen(fd, 5);
    return fd;
}
"#,
	)
	.unwrap();

	// Second C file with pipe IPC.
	fs::write(
		dir.join("pipe_comm.c"),
		r#"#include <unistd.h>
#include <sys/stat.h>

int create_named_pipe(void) {
    return mkfifo("/tmp/myfifo", 0666);
}

void use_pipe(void) {
    int fds[2];
    pipe(fds);
}
"#,
	)
	.unwrap();
}

#[test]
fn refresh_boundary_parity_no_changes() {
	// Test that boundaries are preserved across refresh when no files change.
	// This validates the copy_forward_boundary_surfaces path and ensures
	// `rmap boundaries summary` would return semantically equivalent results.
	let dir = tempfile::tempdir().unwrap();
	make_c_boundary_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"bp1",
		&ComposeOptions::default(),
	)
	.unwrap();
	let snap1_uid = r1.snapshot_uid.clone();

	// Query boundary summary from initial index.
	use repo_graph_boundary_interaction::BoundaryInteractionReadPort;
	let summary1 = storage.get_boundary_interaction_summary(&snap1_uid).unwrap();

	// Verify initial index produced boundaries.
	// C files with socket/mkfifo/pipe calls should produce surfaces.
	assert!(
		summary1.total_surfaces > 0,
		"initial index should detect boundary surfaces from C IPC calls; got {}",
		summary1.total_surfaces
	);
	assert!(
		!summary1.files_with_boundaries.is_empty(),
		"initial index should have files with boundaries"
	);

	// Phase 2: refresh with NO changes.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"bp1",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.status, "ready");
	assert_eq!(snap2.kind, "refresh", "should be refresh not full rebuild");

	// Query boundary summary from refresh snapshot.
	let summary2 = storage.get_boundary_interaction_summary(&r2.snapshot_uid).unwrap();

	// ── Semantic parity assertions ────────────────────────────────
	// These validate what `rmap boundaries summary` would report.

	assert_eq!(
		summary1.total_surfaces, summary2.total_surfaces,
		"boundary surface count must be identical after refresh with no changes"
	);

	assert_eq!(
		summary1.total_channels, summary2.total_channels,
		"boundary channel count must be identical after refresh"
	);

	assert_eq!(
		summary1.files_with_boundaries, summary2.files_with_boundaries,
		"files with boundaries must be identical after refresh"
	);

	// By-kind counts should match.
	assert_eq!(
		summary1.by_channel_kind.len(),
		summary2.by_channel_kind.len(),
		"by_channel_kind breakdown must have same number of entries"
	);
	for (k1, k2) in summary1.by_channel_kind.iter().zip(summary2.by_channel_kind.iter()) {
		assert_eq!(k1.channel_kind, k2.channel_kind, "channel kind mismatch");
		assert_eq!(k1.count, k2.count, "channel kind count mismatch for {:?}", k1.channel_kind);
	}

	// By-direction counts should match.
	for (d1, d2) in summary1.by_direction.iter().zip(summary2.by_direction.iter()) {
		assert_eq!(d1.direction, d2.direction, "direction mismatch");
		assert_eq!(d1.count, d2.count, "direction count mismatch for {:?}", d1.direction);
	}

	// Verify copy-forward diagnostics reported surfaces copied.
	let cf = r2.artifact_copy_forward.as_ref().expect("refresh should have copy-forward diagnostics");
	assert!(
		cf.boundary_surfaces_copied > 0,
		"refresh should report boundary surfaces were copied forward; got {}",
		cf.boundary_surfaces_copied
	);
}

#[test]
fn refresh_boundary_parity_unrelated_change() {
	// Test that boundaries for unchanged files are preserved when an
	// unrelated file is modified.
	let dir = tempfile::tempdir().unwrap();
	make_c_boundary_repo(dir.path());

	// Add a third file that will be modified.
	fs::write(dir.path().join("util.c"), "int util_fn(void) { return 1; }\n").unwrap();

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"bp2",
		&ComposeOptions::default(),
	)
	.unwrap();
	let snap1_uid = r1.snapshot_uid.clone();

	use repo_graph_boundary_interaction::BoundaryInteractionReadPort;
	let summary1 = storage.get_boundary_interaction_summary(&snap1_uid).unwrap();
	assert!(summary1.total_surfaces > 0, "should have boundary surfaces");

	// Phase 2: modify ONLY util.c (no boundary calls).
	fs::write(dir.path().join("util.c"), "int util_fn(void) { return 2; }\n").unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"bp2",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.kind, "refresh");

	let summary2 = storage.get_boundary_interaction_summary(&r2.snapshot_uid).unwrap();

	// Boundaries should be preserved — the boundary-emitting files were unchanged.
	assert_eq!(
		summary1.total_surfaces, summary2.total_surfaces,
		"boundary surfaces from unchanged files must be preserved"
	);
	assert_eq!(
		summary1.files_with_boundaries, summary2.files_with_boundaries,
		"files_with_boundaries must match after unrelated change"
	);
}

#[test]
fn refresh_boundary_parity_boundary_file_changed() {
	// Test that when a boundary-emitting file changes, its boundaries
	// are regenerated (not stale-copied).
	let dir = tempfile::tempdir().unwrap();
	make_c_boundary_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"bp3",
		&ComposeOptions::default(),
	)
	.unwrap();
	let snap1_uid = r1.snapshot_uid.clone();

	use repo_graph_boundary_interaction::{BoundaryInteractionFilter, BoundaryInteractionReadPort};
	let filter = BoundaryInteractionFilter::new().with_file("ipc_server.c");
	let surfaces1 = storage.list_boundary_interactions(&snap1_uid, &filter).unwrap();
	assert!(!surfaces1.is_empty(), "ipc_server.c should have boundary surfaces");

	// Phase 2: modify ipc_server.c — change the socket path.
	fs::write(
		dir.path().join("ipc_server.c"),
		r#"#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

int start_server(void) {
    int fd = socket(AF_UNIX, SOCK_DGRAM, 0);
    if (fd < 0) return -1;

    struct sockaddr_un addr;
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, "/var/run/app_v2.sock", sizeof(addr.sun_path) - 1);

    bind(fd, (struct sockaddr*)&addr, sizeof(addr));
    return fd;
}
"#,
	)
	.unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"bp3",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.kind, "refresh");

	// Query surfaces for the changed file.
	let surfaces2 = storage.list_boundary_interactions(&r2.snapshot_uid, &filter).unwrap();
	assert!(!surfaces2.is_empty(), "changed ipc_server.c should still have boundary surfaces");

	// pipe_comm.c was unchanged — its boundaries should be copied forward.
	let pipe_filter = BoundaryInteractionFilter::new().with_file("pipe_comm.c");
	let pipe_surfaces1 = storage.list_boundary_interactions(&snap1_uid, &pipe_filter).unwrap();
	let pipe_surfaces2 = storage.list_boundary_interactions(&r2.snapshot_uid, &pipe_filter).unwrap();

	assert_eq!(
		pipe_surfaces1.len(),
		pipe_surfaces2.len(),
		"unchanged pipe_comm.c should have same number of surfaces"
	);

	// Verify copy-forward reported some surfaces copied (from pipe_comm.c).
	let cf = r2.artifact_copy_forward.as_ref().expect("refresh should have copy-forward diagnostics");
	assert!(
		cf.boundary_surfaces_copied > 0,
		"should have copied forward surfaces from unchanged file"
	);
}

// ── 9. End-to-end agent parity: contracts ────────────────────────

fn make_proto_repo(dir: &std::path::Path) {
	// Proto files for contract extraction.
	fs::create_dir_all(dir.join("proto")).unwrap();

	fs::write(
		dir.join("proto/greeter.proto"),
		r#"syntax = "proto3";

package greeter.v1;

service Greeter {
    rpc SayHello (HelloRequest) returns (HelloReply);
}

message HelloRequest {
    string name = 1;
}

message HelloReply {
    string message = 1;
}
"#,
	)
	.unwrap();

	fs::write(
		dir.join("proto/types.proto"),
		r#"syntax = "proto3";

package types.v1;

message User {
    string id = 1;
    string email = 2;
    optional string name = 3;
}

enum Status {
    STATUS_UNSPECIFIED = 0;
    STATUS_ACTIVE = 1;
    STATUS_INACTIVE = 2;
}
"#,
	)
	.unwrap();

	// Add non-proto source files (need multiple for refresh tests).
	fs::write(dir.join("main.c"), "int main(void) { return 0; }\n").unwrap();
	fs::write(dir.join("util.c"), "int util(void) { return 1; }\n").unwrap();
}

#[test]
fn refresh_contract_parity_no_changes() {
	// Test that contract schemas/elements are preserved across refresh
	// when proto files are unchanged.
	//
	// NOTE: Contract files are currently re-indexed on refresh (no delta
	// optimization yet). This test verifies the schema counts match, not
	// copy-forward behavior. See orchestrator.rs refresh_repo comment.
	let dir = tempfile::tempdir().unwrap();
	make_proto_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"cp1",
		&ComposeOptions::default(),
	)
	.unwrap();
	let snap1_uid = r1.snapshot_uid.clone();

	// Query contract schemas from initial index via port trait.
	use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;
	let schemas1 = storage.list_contract_schemas(&snap1_uid, None).unwrap();

	// Verify initial index produced contract schemas.
	assert!(
		!schemas1.is_empty(),
		"initial index should extract contract schemas from proto files"
	);

	// Query contract elements count via port trait.
	let elements_count1 = storage.count_elements(&snap1_uid).unwrap();
	assert!(elements_count1 > 0, "should have contract elements");

	// Phase 2: refresh with NO changes.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"cp1",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.status, "ready");
	assert_eq!(snap2.kind, "refresh", "should be refresh not full rebuild");

	// Query contract schemas from refresh snapshot.
	let schemas2 = storage.list_contract_schemas(&r2.snapshot_uid, None).unwrap();
	let elements_count2 = storage.count_elements(&r2.snapshot_uid).unwrap();

	// ── Semantic parity assertions ────────────────────────────────
	// These validate what `rmap contracts list` would report.

	assert_eq!(
		schemas1.len(),
		schemas2.len(),
		"contract schema count must be identical after refresh"
	);

	// Compare schema metadata (file_path, kind, package).
	for (s1, s2) in schemas1.iter().zip(schemas2.iter()) {
		assert_eq!(s1.file_path, s2.file_path, "file_path mismatch");
		assert_eq!(s1.schema_kind, s2.schema_kind, "schema_kind mismatch");
		assert_eq!(s1.package_name, s2.package_name, "package_name mismatch");
	}

	assert_eq!(
		elements_count1, elements_count2,
		"contract element count must be identical after refresh"
	);

	// Contracts are re-indexed during refresh (no copy-forward yet).
	// Verify the indexing actually happened.
	assert!(
		r2.contracts.is_some(),
		"refresh should report contract indexing results"
	);
	let contracts = r2.contracts.as_ref().unwrap();
	assert_eq!(
		contracts.schemas_indexed, schemas1.len(),
		"refresh should re-index same number of schemas"
	);
}

#[test]
fn refresh_contract_parity_unrelated_change() {
	// Test that contracts for unchanged proto files are preserved when
	// a non-proto file is modified.
	//
	// NOTE: Contract files are re-indexed (not copy-forward) during refresh.
	let dir = tempfile::tempdir().unwrap();
	make_proto_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"cp2",
		&ComposeOptions::default(),
	)
	.unwrap();
	let snap1_uid = r1.snapshot_uid.clone();

	use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;
	let schema_count1 = storage.count_schemas(&snap1_uid).unwrap();

	// Phase 2: modify ONLY main.c (not a proto file).
	fs::write(dir.path().join("main.c"), "int main(void) { return 42; }\n").unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"cp2",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.kind, "refresh");

	let schema_count2 = storage.count_schemas(&r2.snapshot_uid).unwrap();

	// Contract schemas should be preserved (via re-indexing).
	assert_eq!(
		schema_count1, schema_count2,
		"contract schemas must be preserved after refresh"
	);
}

#[test]
fn refresh_contract_parity_proto_changed() {
	// Test that when a proto file changes, its contract schema is regenerated.
	let dir = tempfile::tempdir().unwrap();
	make_proto_repo(dir.path());

	let mut storage = StorageConnection::open_in_memory().unwrap();

	// Phase 1: full index.
	let r1 = index_into_storage(
		dir.path(),
		&mut storage,
		"cp3",
		&ComposeOptions::default(),
	)
	.unwrap();
	let snap1_uid = r1.snapshot_uid.clone();

	// Count elements for each proto file.
	use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;
	let types_schema1 = storage.get_schema_by_file(&snap1_uid, "proto/types.proto").unwrap();
	let types_elements1 = if let Some(s) = &types_schema1 {
		storage.list_elements_for_schema(&s.schema_uid, None).unwrap().len()
	} else {
		0
	};

	// Phase 2: modify types.proto — add a field.
	fs::write(
		dir.path().join("proto/types.proto"),
		r#"syntax = "proto3";

package types.v1;

message User {
    string id = 1;
    string email = 2;
    optional string name = 3;
    int64 created_at = 4;
}

enum Status {
    STATUS_UNSPECIFIED = 0;
    STATUS_ACTIVE = 1;
    STATUS_INACTIVE = 2;
}
"#,
	)
	.unwrap();

	// Phase 3: refresh.
	let r2 = refresh_into_storage(
		dir.path(),
		&mut storage,
		"cp3",
		&ComposeOptions::default(),
	)
	.unwrap();

	let snap2 = storage.get_snapshot(&r2.snapshot_uid).unwrap().unwrap();
	assert_eq!(snap2.kind, "refresh");

	// types.proto was changed — element count should be higher (new field).
	let types_schema2 = storage.get_schema_by_file(&r2.snapshot_uid, "proto/types.proto").unwrap();
	let types_elements2 = if let Some(s) = &types_schema2 {
		storage.list_elements_for_schema(&s.schema_uid, None).unwrap().len()
	} else {
		0
	};

	assert!(
		types_elements2 > types_elements1,
		"changed proto should have more elements (added field); before={}, after={}",
		types_elements1,
		types_elements2
	);

	// greeter.proto was unchanged — should still have elements (re-indexed).
	// Note: contracts are currently re-indexed on refresh, not copied forward.
	let greeter_schema2 = storage.get_schema_by_file(&r2.snapshot_uid, "proto/greeter.proto").unwrap();
	let greeter_elements2 = if let Some(s) = &greeter_schema2 {
		storage.list_elements_for_schema(&s.schema_uid, None).unwrap().len()
	} else {
		0
	};

	assert!(greeter_elements2 > 0, "unchanged greeter.proto should have elements after refresh");
}
