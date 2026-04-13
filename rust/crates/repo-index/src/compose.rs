//! Composition entry points — wires scanner + config readers +
//! extractor + storage into the indexer orchestrator.
//!
//! Two entry points:
//!   - `index_into_storage` — accepts existing `&mut StorageConnection`
//!     for deterministic testing
//!   - `index_path` — opens storage from a db_path, full composition

use std::path::Path;

use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_indexer::orchestrator::{self, FileInput};
use repo_graph_indexer::routing;
use repo_graph_indexer::types::{IndexOptions, IndexResult};
use repo_graph_storage::StorageConnection;
use repo_graph_ts_extractor::TsExtractor;

use crate::config::RepoConfigContext;
use crate::scanner::{self, ScannedFile};

/// Error from the composition layer.
#[derive(Debug)]
pub enum ComposeError {
	/// Scanner failed (directory walk error).
	Scan(scanner::ScanError),
	/// Storage open/creation failed.
	Storage(repo_graph_storage::error::StorageError),
	/// Indexer pipeline failed.
	Index(String),
	/// Extractor initialization failed.
	ExtractorInit(String),
}

impl std::fmt::Display for ComposeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Scan(e) => write!(f, "scan: {}", e),
			Self::Storage(e) => write!(f, "storage: {}", e),
			Self::Index(e) => write!(f, "index: {}", e),
			Self::ExtractorInit(e) => write!(f, "extractor init: {}", e),
		}
	}
}

/// Options for the composition layer.
pub struct ComposeOptions {
	/// Forwarded to `IndexOptions.basis_commit`.
	pub basis_commit: Option<String>,
	/// Forwarded to `IndexOptions.edge_batch_size`.
	pub edge_batch_size: Option<usize>,
}

impl Default for ComposeOptions {
	fn default() -> Self {
		Self {
			basis_commit: None,
			edge_batch_size: None,
		}
	}
}

/// Index a repo from disk into an existing `StorageConnection`.
///
/// This is the testable entry point: the caller controls storage
/// lifetime and can inspect the SQLite state after indexing.
///
/// Steps:
///   1. Scan `repo_path` for source files
///   2. Resolve config (package.json, tsconfig.json) per file
///   3. Assemble typed `FileInput` structs
///   4. Initialize `TsExtractor`
///   5. Register repo if needed
///   6. Call `index_repo` from the indexer orchestrator
pub fn index_into_storage(
	repo_path: &Path,
	storage: &mut StorageConnection,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	// 1. Scan.
	let scanned = scanner::scan_repo(repo_path).map_err(ComposeError::Scan)?;

	// 2. Resolve config.
	let mut config_ctx = RepoConfigContext::new();

	// 3. Separate readable files from read-failed files.
	let mut file_inputs: Vec<FileInput> = Vec::new();
	let mut read_failed_paths: Vec<String> = Vec::new();

	for file in &scanned {
		match file {
			ScannedFile::Ok(ok) => {
				let pkg_deps = config_ctx.resolve_package_deps(&ok.rel_path, repo_path);
				let tsconfig = config_ctx.resolve_tsconfig_aliases(&ok.rel_path, repo_path);

				file_inputs.push(FileInput {
					rel_path: ok.rel_path.clone(),
					content: ok.content.clone(),
					content_hash: ok.content_hash.clone(),
					size_bytes: ok.size_bytes,
					line_count: ok.line_count,
					package_dependencies: if pkg_deps.names.is_empty() {
						None
					} else {
						Some(pkg_deps)
					},
					tsconfig_aliases: if tsconfig.entries.is_empty() {
						None
					} else {
						Some(tsconfig)
					},
				});
			}
			ScannedFile::ReadFailed { rel_path } => {
				read_failed_paths.push(rel_path.clone());
			}
		}
	}

	// 4. Initialize extractor.
	let mut extractor = TsExtractor::new();
	extractor.initialize().map_err(|e| ComposeError::ExtractorInit(e.to_string()))?;

	// 5. Register repo if needed.
	ensure_repo(storage, repo_uid, repo_path)?;

	// 6. Persist read-failed files directly as tracked files with
	//    ParseStatus::Failed, bypassing the orchestrator. The
	//    orchestrator only receives readable files.
	if !read_failed_paths.is_empty() {
		let failed_tracked: Vec<repo_graph_storage::types::TrackedFile> =
			read_failed_paths
				.iter()
				.map(|path| repo_graph_storage::types::TrackedFile {
					file_uid: format!("{}:{}", repo_uid, path),
					repo_uid: repo_uid.into(),
					path: path.clone(),
					language: routing::detect_language(path).map(|s| s.to_string()),
					is_test: routing::is_test_file(path),
					is_generated: false,
					is_excluded: false,
				})
				.collect();
		storage.upsert_files(&failed_tracked).map_err(ComposeError::Storage)?;
	}

	// 7. Run indexer on readable files only.
	let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut extractor];
	let mut idx_options = IndexOptions {
		basis_commit: options.basis_commit.clone(),
		edge_batch_size: options.edge_batch_size,
		..IndexOptions::default()
	};

	let mut result = orchestrator::index_repo(
		storage,
		&mut extractors,
		repo_uid,
		&file_inputs,
		&mut idx_options,
	)
	.map_err(|e| ComposeError::Index(format!("{}", e)))?;

	// 8. Persist read-failed file-version records AFTER indexing.
	//    The snapshot now exists with the returned snapshot_uid.
	//    After persisting, re-update snapshot counts and diagnostics
	//    so the database state includes the failed files.
	if !read_failed_paths.is_empty() {
		let failed_versions: Vec<repo_graph_storage::types::FileVersion> =
			read_failed_paths
				.iter()
				.map(|path| repo_graph_storage::types::FileVersion {
					snapshot_uid: result.snapshot_uid.clone(),
					file_uid: format!("{}:{}", repo_uid, path),
					content_hash: String::new(),
					ast_hash: None,
					extractor: Some("skipped:read_failed".into()),
					parse_status: "failed".into(),
					size_bytes: None,
					line_count: None,
					indexed_at: "2025-01-01T00:00:00.000Z".into(),
				})
				.collect();
		storage
			.upsert_file_versions(&failed_versions)
			.map_err(ComposeError::Storage)?;

		// Re-update snapshot counts so the persisted files_total
		// includes the failed file_versions rows.
		use repo_graph_indexer::storage_port::SnapshotLifecyclePort;
		SnapshotLifecyclePort::update_snapshot_counts(storage, &result.snapshot_uid)
			.map_err(ComposeError::Storage)?;

		// Read-modify-write extraction diagnostics to increment
		// files_read_failed without erasing orchestrator-produced
		// counters (e.g. files_skipped_oversized).
		let read_failed_count = read_failed_paths.len() as u64;
		use repo_graph_trust::TrustStorageRead;
		let existing_json = TrustStorageRead::get_snapshot_extraction_diagnostics(
			storage,
			&result.snapshot_uid,
		)
		.ok()
		.flatten();

		if let Some(json_str) = existing_json {
			if let Ok(mut diag) = serde_json::from_str::<serde_json::Value>(&json_str) {
				let current = diag
					.get("files_read_failed")
					.and_then(|v| v.as_u64())
					.unwrap_or(0);
				diag["files_read_failed"] = serde_json::json!(current + read_failed_count);
				SnapshotLifecyclePort::update_snapshot_extraction_diagnostics(
					storage,
					&result.snapshot_uid,
					&serde_json::to_string(&diag).unwrap_or_default(),
				)
				.map_err(ComposeError::Storage)?;
			}
		}

		// Adjust returned DTO.
		result.files_total += read_failed_count;
	}

	Ok(result)
}

/// Index a repo from disk, opening storage at `db_path`.
///
/// This is the full-composition entry point for production use.
pub fn index_path(
	repo_path: &Path,
	db_path: &Path,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	let mut storage = if db_path.to_string_lossy() == ":memory:" {
		StorageConnection::open_in_memory().map_err(ComposeError::Storage)?
	} else {
		StorageConnection::open(db_path).map_err(ComposeError::Storage)?
	};

	index_into_storage(repo_path, &mut storage, repo_uid, options)
}

/// Ensure the repo is registered in storage.
fn ensure_repo(
	storage: &StorageConnection,
	repo_uid: &str,
	repo_path: &Path,
) -> Result<(), ComposeError> {
	use repo_graph_storage::types::{Repo, RepoRef};

	// Check if already registered.
	let existing = storage
		.get_repo(&RepoRef::Uid(repo_uid.into()))
		.map_err(ComposeError::Storage)?;
	if existing.is_some() {
		return Ok(());
	}

	let now = chrono_like_now();
	storage
		.add_repo(&Repo {
			repo_uid: repo_uid.into(),
			name: repo_uid.into(),
			root_path: repo_path.to_string_lossy().into(),
			default_branch: None,
			created_at: now,
			metadata_json: None,
		})
		.map_err(ComposeError::Storage)?;

	Ok(())
}

/// Simple ISO timestamp for repo creation (no chrono dep).
fn chrono_like_now() -> String {
	// The storage crate generates proper timestamps via SQLite's
	// strftime. For the repo record we just need any valid string.
	"2025-01-01T00:00:00.000Z".to_string()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;

	fn make_fixture_repo() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		// package.json with a dependency.
		fs::write(
			root.join("package.json"),
			r#"{"dependencies":{"express":"^4.18.0"}}"#,
		)
		.unwrap();

		// .gitignore — exclude generated file.
		fs::write(root.join(".gitignore"), "src/generated.ts\n").unwrap();

		// node_modules — should be excluded.
		fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
		fs::write(root.join("node_modules/pkg/index.ts"), "const x=1;").unwrap();

		// Source files.
		fs::create_dir_all(root.join("src")).unwrap();

		// src/index.ts — imports server, calls serve.
		fs::write(
			root.join("src/index.ts"),
			"import { serve } from \"./server\";\nserve();\n",
		)
		.unwrap();

		// src/server.ts — exported function.
		fs::write(
			root.join("src/server.ts"),
			"export function serve() {}\n",
		)
		.unwrap();

		// src/generated.ts — should be gitignored.
		fs::write(root.join("src/generated.ts"), "const gen = 1;").unwrap();

		// README.md — not a source extension, should be excluded.
		fs::write(root.join("README.md"), "# Test").unwrap();

		dir
	}

	#[test]
	fn index_into_storage_exact_assertions() {
		let fixture = make_fixture_repo();
		let mut storage = StorageConnection::open_in_memory().unwrap();

		let result = index_into_storage(
			fixture.path(),
			&mut storage,
			"r1",
			&ComposeOptions::default(),
		)
		.unwrap();

		// Snapshot should be READY.
		let snap = storage
			.get_snapshot(&result.snapshot_uid)
			.unwrap()
			.unwrap();
		assert_eq!(snap.status, "ready");

		// ── Exact deterministic counts ───────────────────────
		// files: index.ts + server.ts = 2
		// Excluded: node_modules/pkg/index.ts, src/generated.ts (gitignore), README.md
		assert_eq!(result.files_total, 2, "files_total");

		// nodes: FILE(index.ts) + FILE(server.ts) + FUNCTION(serve) + MODULE(src) = 4
		assert_eq!(result.nodes_total, 4, "nodes_total");

		// edges: IMPORTS(index→server) + CALLS(serve from index.ts) +
		//        OWNS(src→index.ts) + OWNS(src→server.ts) = 4
		assert_eq!(result.edges_total, 4, "edges_total");

		// No unresolved edges (serve() resolves to the exported function).
		assert_eq!(result.edges_unresolved, 0, "edges_unresolved");

		// Check specific stable keys exist via query_all_nodes.
		use repo_graph_indexer::storage_port::NodeStorePort;
		let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();

		let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

		// FILE nodes.
		assert!(
			stable_keys.contains(&"r1:src/index.ts:FILE"),
			"missing FILE node for index.ts, keys: {:?}",
			stable_keys
		);
		assert!(
			stable_keys.contains(&"r1:src/server.ts:FILE"),
			"missing FILE node for server.ts, keys: {:?}",
			stable_keys
		);

		// FUNCTION node for serve.
		assert!(
			stable_keys.iter().any(|k| k.contains("#serve:SYMBOL:FUNCTION")),
			"missing FUNCTION node for serve, keys: {:?}",
			stable_keys
		);

		// MODULE node for src.
		assert!(
			stable_keys.iter().any(|k| k.contains("src:MODULE")),
			"missing MODULE node for src, keys: {:?}",
			stable_keys
		);

		// Excluded files should NOT appear.
		assert!(
			!stable_keys.iter().any(|k| k.contains("generated")),
			"gitignored file should not appear in nodes"
		);
		assert!(
			!stable_keys.iter().any(|k| k.contains("node_modules")),
			"node_modules file should not appear in nodes"
		);
	}

	#[test]
	fn index_path_with_memory_db() {
		let fixture = make_fixture_repo();

		let result = index_path(
			fixture.path(),
			Path::new(":memory:"),
			"r1",
			&ComposeOptions::default(),
		)
		.unwrap();

		// Same exact counts as index_into_storage — both paths
		// produce identical results for the same fixture.
		assert_eq!(result.files_total, 2);
		assert_eq!(result.nodes_total, 4);
		assert_eq!(result.edges_total, 4);
		assert_eq!(result.edges_unresolved, 0);
	}
}
