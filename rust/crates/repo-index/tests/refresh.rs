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
