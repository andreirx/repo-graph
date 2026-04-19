//! SB-4-pre integration test: end-to-end pipeline verification.
//!
//! Validates that `rmap index` (via compose::index_path) produces
//! state-boundary nodes and edges in the SQLite database when the
//! indexed source contains FS stdlib calls with resolvable
//! argument-0 payloads.
//!
//! This test exercises the FULL production path:
//!   compose::index_path
//!     → StateBoundaryHook (constructed in compose)
//!     → orchestrator::index_repo with hook
//!     → ts-extractor produces ResolvedCallsite
//!     → hook.on_extraction_result → state-extractor emit
//!     → hook.drain_snapshot_extras → merged into persistence
//!     → SQLite DB contains FS_PATH nodes + READS/WRITES edges

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

// ── Happy path: named import + literal string arg ──────────────

#[test]
fn index_produces_fs_path_node_and_reads_edge() {
	let source = r#"
import { readFile } from "fs";
export function load() {
  readFile("/etc/app.yaml", () => {});
}
"#;
	let (_dir, repo) = temp_repo(&[("src/load.ts", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(
		&repo,
		&db_path,
		"myservice",
		&ComposeOptions::default(),
	)
	.expect("indexing must succeed");

	assert!(result.nodes_total > 0, "should have at least file + symbol nodes");

	// Open the DB and verify state-boundary facts are present.
	let storage = StorageConnection::open(&db_path).unwrap();
	let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();

	let fs_path_nodes: Vec<_> = nodes
		.iter()
		.filter(|n| n.kind == "FS_PATH")
		.collect();
	assert_eq!(
		fs_path_nodes.len(),
		1,
		"expected exactly one FS_PATH node for /etc/app.yaml, got: {:?}",
		fs_path_nodes.iter().map(|n| &n.stable_key).collect::<Vec<_>>()
	);
	assert_eq!(fs_path_nodes[0].name, "/etc/app.yaml");
	assert_eq!(
		fs_path_nodes[0].stable_key,
		"myservice:fs:/etc/app.yaml:FS_PATH"
	);
	assert_eq!(fs_path_nodes[0].subtype.as_deref(), Some("FILE_PATH"));

	// The READS edge should be resolved (Phase 3 resolves
	// target_key = resource stable key → target_node_uid via
	// the stable-key resolver path added in SB-4-pre).
	// Use `find_direct_callees` to query the resolved `edges`
	// table with edge_type = READS.
	let load_stable_key = "myservice:src/load.ts#load:SYMBOL:FUNCTION";
	let callees = storage
		.find_direct_callees(&result.snapshot_uid, load_stable_key, &["READS"])
		.expect("callee query must succeed");
	assert_eq!(
		callees.len(),
		1,
		"expected one READS callee from `load`, got: {:?}",
		callees.iter().map(|c| &c.stable_key).collect::<Vec<_>>()
	);
	assert_eq!(callees[0].stable_key, "myservice:fs:/etc/app.yaml:FS_PATH");
	assert_eq!(callees[0].kind, "FS_PATH");
}

// ── Negative: non-FS import produces no state-boundary facts ───

#[test]
fn non_fs_import_produces_no_state_boundary_nodes() {
	let source = r#"
import express from "express";
const app = express();
app.listen(3000);
"#;
	let (_dir, repo) = temp_repo(&[("src/app.ts", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(
		&repo,
		&db_path,
		"myservice",
		&ComposeOptions::default(),
	)
	.unwrap();

	let storage = StorageConnection::open(&db_path).unwrap();
	let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();
	let resource_nodes: Vec<_> = nodes
		.iter()
		.filter(|n| {
			n.kind == "FS_PATH"
				|| n.kind == "DB_RESOURCE"
				|| n.kind == "BLOB"
				|| (n.kind == "STATE" && n.subtype.as_deref() == Some("CACHE"))
		})
		.collect();
	assert!(
		resource_nodes.is_empty(),
		"non-FS imports must produce no state-boundary resource nodes, got: {:?}",
		resource_nodes.iter().map(|n| &n.stable_key).collect::<Vec<_>>()
	);
}

// ── node:fs/promises matches (4A specifier parity) ─────────────

#[test]
fn node_fs_promises_import_produces_state_boundary_edge() {
	let source = r#"
import { writeFile } from "node:fs/promises";
export async function save() {
  await writeFile("/var/log/out.txt", "data");
}
"#;
	let (_dir, repo) = temp_repo(&[("src/save.ts", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let result = index_path(
		&repo,
		&db_path,
		"myservice",
		&ComposeOptions::default(),
	)
	.unwrap();

	let storage = StorageConnection::open(&db_path).unwrap();
	let nodes = storage.query_all_nodes(&result.snapshot_uid).unwrap();
	let fs_nodes: Vec<_> = nodes.iter().filter(|n| n.kind == "FS_PATH").collect();
	assert_eq!(fs_nodes.len(), 1);
	assert_eq!(fs_nodes[0].name, "/var/log/out.txt");
}

// ══════════════════════════════════════════════════════════════
//  Refresh-path tests (SB-4-pre Fix B)
// ══════════════════════════════════════════════════════════════

#[test]
fn refresh_preserves_unchanged_file_state_boundary_facts() {
	// Full index creates state-boundary facts. Refresh with no
	// changes must preserve them via copy-forward.
	let source = r#"
import { readFile } from "fs";
export function load() {
  readFile("/etc/unchanged.yaml", () => {});
}
"#;
	let (_dir, repo) = temp_repo(&[("src/load.ts", source)]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	// Full index.
	let idx = index_path(&repo, &db_path, "myservice", &ComposeOptions::default())
		.expect("full index");

	// Verify state-boundary facts exist after full index.
	let storage = StorageConnection::open(&db_path).unwrap();
	let idx_nodes = storage.query_all_nodes(&idx.snapshot_uid).unwrap();
	let idx_fs: Vec<_> = idx_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();
	assert_eq!(idx_fs.len(), 1, "full index must produce FS_PATH node");
	drop(storage);

	// Refresh (no file changes → all copied forward).
	let ref_result = refresh_path(&repo, &db_path, "myservice", &ComposeOptions::default())
		.expect("refresh");

	let storage = StorageConnection::open(&db_path).unwrap();
	let ref_nodes = storage.query_all_nodes(&ref_result.snapshot_uid).unwrap();
	let ref_fs: Vec<_> = ref_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();
	assert_eq!(
		ref_fs.len(),
		1,
		"refresh must preserve FS_PATH node via copy-forward"
	);
	assert_eq!(ref_fs[0].name, "/etc/unchanged.yaml");
	assert_eq!(
		ref_fs[0].stable_key,
		"myservice:fs:/etc/unchanged.yaml:FS_PATH"
	);

	// Verify the READS edge also survived refresh.
	let load_key = "myservice:src/load.ts#load:SYMBOL:FUNCTION";
	let callees = storage
		.find_direct_callees(&ref_result.snapshot_uid, load_key, &["READS"])
		.unwrap();
	assert_eq!(
		callees.len(),
		1,
		"refresh must preserve READS edge via copy-forward"
	);
	assert_eq!(callees[0].stable_key, "myservice:fs:/etc/unchanged.yaml:FS_PATH");
}

#[test]
fn refresh_mixed_unchanged_and_changed_files() {
	// Initial: two files, each touching a different FS resource.
	let src_a = r#"
import { readFile } from "fs";
export function loadA() {
  readFile("/etc/a", () => {});
}
"#;
	let src_b = r#"
import { writeFile } from "fs";
export function saveB() {
  writeFile("/etc/b", "data", () => {});
}
"#;
	let (_dir, repo) = temp_repo(&[
		("src/a.ts", src_a),
		("src/b.ts", src_b),
	]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	// Full index.
	let _idx = index_path(&repo, &db_path, "myservice", &ComposeOptions::default())
		.expect("full index");

	// Modify file b (change the path).
	let src_b2 = r#"
import { writeFile } from "fs";
export function saveB() {
  writeFile("/etc/b2", "data", () => {});
}
"#;
	fs::write(repo.join("src/b.ts"), src_b2).unwrap();

	// Refresh: a.ts is unchanged → copy-forward; b.ts changed.
	let ref_result = refresh_path(&repo, &db_path, "myservice", &ComposeOptions::default())
		.expect("refresh");

	let storage = StorageConnection::open(&db_path).unwrap();
	let ref_nodes = storage.query_all_nodes(&ref_result.snapshot_uid).unwrap();
	let ref_fs: Vec<_> = ref_nodes.iter().filter(|n| n.kind == "FS_PATH").collect();

	// /etc/a should survive from copy-forward.
	// /etc/b2 should appear from changed-file extraction.
	// /etc/b (old) MAY persist as a stale orphan — this is the
	// documented residual debt from SB-4-pre Fix B. Resource
	// nodes are snapshot-scoped deduped artifacts; the copy-
	// forward preserves all null-file nodes from the parent
	// snapshot because it cannot determine which resource is
	// still referenced. Stale orphans are cleaned on full reindex.
	let names: Vec<&str> = ref_fs.iter().map(|n| n.name.as_str()).collect();
	assert!(
		names.contains(&"/etc/a"),
		"unchanged file's resource must survive refresh, got: {:?}", names
	);
	assert!(
		names.contains(&"/etc/b2"),
		"changed file's new resource must appear, got: {:?}", names
	);
	// Stale orphan: /etc/b persists. This is accepted residual
	// debt, not a correctness defect. See TECH-DEBT.md.
	// (We assert it IS present to pin the behavior and catch
	// unintentional fix/regression.)
	assert!(
		names.contains(&"/etc/b"),
		"stale orphan resource /etc/b should persist until full reindex, got: {:?}", names
	);
}

#[test]
fn refresh_deduplicates_shared_resource_across_changed_and_unchanged() {
	// Both files read the SAME resource. After refresh with one
	// file changed (but still reading the same path), the
	// resource node must appear exactly once.
	let src_a = r#"
import { readFile } from "fs";
export function loadA() {
  readFile("/etc/shared", () => {});
}
"#;
	let src_b = r#"
import { readFile } from "fs";
export function loadB() {
  readFile("/etc/shared", () => {});
}
"#;
	let (_dir, repo) = temp_repo(&[
		("src/a.ts", src_a),
		("src/b.ts", src_b),
	]);
	let db_dir = tempfile::tempdir().unwrap();
	let db_path = db_dir.path().join("test.db");

	let _idx = index_path(&repo, &db_path, "myservice", &ComposeOptions::default())
		.expect("full index");

	// Modify b.ts trivially (add a comment so the hash changes).
	let src_b2 = r#"
// changed
import { readFile } from "fs";
export function loadB() {
  readFile("/etc/shared", () => {});
}
"#;
	fs::write(repo.join("src/b.ts"), src_b2).unwrap();

	let ref_result = refresh_path(&repo, &db_path, "myservice", &ComposeOptions::default())
		.expect("refresh");

	let storage = StorageConnection::open(&db_path).unwrap();
	let ref_nodes = storage.query_all_nodes(&ref_result.snapshot_uid).unwrap();
	let shared_nodes: Vec<_> = ref_nodes
		.iter()
		.filter(|n| n.kind == "FS_PATH" && n.name == "/etc/shared")
		.collect();
	assert_eq!(
		shared_nodes.len(),
		1,
		"shared resource must appear exactly once after refresh (dedup), got: {:?}",
		shared_nodes.iter().map(|n| &n.stable_key).collect::<Vec<_>>()
	);
}
