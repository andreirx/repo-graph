//! Composition entry points — wires scanner + config readers +
//! extractor + storage into the indexer orchestrator.
//!
//! Two write-side entry points:
//!   - `index_into_storage` — full index from disk
//!   - `refresh_into_storage` — incremental refresh from disk
//!
//! Plus `index_path` / `refresh_path` variants that open storage.
//!
//! Both share `prepare_repo_inputs` for scanning, config resolution,
//! and FileInput assembly.

use std::path::Path;

use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_indexer::orchestrator::{self, FileInput};
use repo_graph_indexer::routing;
use repo_graph_indexer::storage_port::SnapshotLifecyclePort;
use repo_graph_indexer::types::{IndexOptions, IndexResult};
use repo_graph_storage::StorageConnection;
use repo_graph_ts_extractor::TsExtractor;

use crate::config::RepoConfigContext;
use crate::scanner::{self, ScannedFile};

// ── Error type ───────────────────────────────────────────────────

/// Error from the composition layer.
#[derive(Debug)]
pub enum ComposeError {
	Scan(scanner::ScanError),
	Storage(repo_graph_storage::error::StorageError),
	Index(String),
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
	pub basis_commit: Option<String>,
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

// ── Shared preparation ───────────────────────────────────────────

/// Result of scanning + config resolution + FileInput assembly.
/// Carries both readable files and read-failed paths so callers
/// can handle the read-failure contract correctly.
pub struct PreparedRepoInputs {
	/// Readable files with config attached, ready for the orchestrator.
	pub file_inputs: Vec<FileInput>,
	/// Paths that were discovered but could not be read.
	pub read_failed_paths: Vec<String>,
}

/// Scan the repo, resolve config per file, assemble typed FileInput.
pub fn prepare_repo_inputs(
	repo_path: &Path,
) -> Result<PreparedRepoInputs, ComposeError> {
	let scanned = scanner::scan_repo(repo_path).map_err(ComposeError::Scan)?;
	let mut config_ctx = RepoConfigContext::new();

	let mut file_inputs = Vec::new();
	let mut read_failed_paths = Vec::new();

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

	Ok(PreparedRepoInputs {
		file_inputs,
		read_failed_paths,
	})
}

// ── Post-index read-failure repair ───────────────────────────────

/// Persist read-failed file records and repair snapshot counts/diagnostics.
/// Called after index_repo or refresh_repo returns.
fn persist_read_failures(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	read_failed_paths: &[String],
	result: &mut IndexResult,
) -> Result<(), ComposeError> {
	if read_failed_paths.is_empty() {
		return Ok(());
	}

	// Tracked file records.
	let failed_tracked: Vec<repo_graph_storage::types::TrackedFile> = read_failed_paths
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
	storage
		.upsert_files(&failed_tracked)
		.map_err(ComposeError::Storage)?;

	// File version records with parse_status = "failed".
	let failed_versions: Vec<repo_graph_storage::types::FileVersion> = read_failed_paths
		.iter()
		.map(|path| repo_graph_storage::types::FileVersion {
			snapshot_uid: snapshot_uid.into(),
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

	// Re-update snapshot counts.
	SnapshotLifecyclePort::update_snapshot_counts(storage, snapshot_uid)
		.map_err(ComposeError::Storage)?;

	// Read-modify-write extraction diagnostics.
	let read_failed_count = read_failed_paths.len() as u64;
	use repo_graph_trust::TrustStorageRead;
	if let Some(json_str) = TrustStorageRead::get_snapshot_extraction_diagnostics(
		storage,
		snapshot_uid,
	)
	.ok()
	.flatten()
	{
		if let Ok(mut diag) = serde_json::from_str::<serde_json::Value>(&json_str) {
			let current = diag
				.get("files_read_failed")
				.and_then(|v| v.as_u64())
				.unwrap_or(0);
			diag["files_read_failed"] = serde_json::json!(current + read_failed_count);
			SnapshotLifecyclePort::update_snapshot_extraction_diagnostics(
				storage,
				snapshot_uid,
				&serde_json::to_string(&diag).unwrap_or_default(),
			)
			.map_err(ComposeError::Storage)?;
		}
	}

	result.files_total += read_failed_count;
	Ok(())
}

// ── Full index ───────────────────────────────────────────────────

/// Index a repo from disk into an existing StorageConnection.
pub fn index_into_storage(
	repo_path: &Path,
	storage: &mut StorageConnection,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	let prepared = prepare_repo_inputs(repo_path)?;

	let mut extractor = TsExtractor::new();
	extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(e.to_string()))?;

	ensure_repo(storage, repo_uid, repo_path)?;

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
		&prepared.file_inputs,
		&mut idx_options,
	)
	.map_err(|e| ComposeError::Index(format!("{}", e)))?;

	persist_read_failures(
		storage,
		repo_uid,
		&result.snapshot_uid.clone(),
		&prepared.read_failed_paths,
		&mut result,
	)?;

	Ok(result)
}

/// Index a repo from disk, opening storage at db_path.
pub fn index_path(
	repo_path: &Path,
	db_path: &Path,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	let mut storage = open_or_create_storage(db_path)?;
	index_into_storage(repo_path, &mut storage, repo_uid, options)
}

// ── Refresh ──────────────────────────────────────────────────────

/// Refresh (incremental re-index) a repo from disk into an existing
/// StorageConnection.
///
/// If no prior READY snapshot exists, falls back to a full index
/// (matching the accepted policy behavior from Rust-5).
pub fn refresh_into_storage(
	repo_path: &Path,
	storage: &mut StorageConnection,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	let prepared = prepare_repo_inputs(repo_path)?;

	let mut extractor = TsExtractor::new();
	extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(e.to_string()))?;

	ensure_repo(storage, repo_uid, repo_path)?;

	let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut extractor];
	let mut idx_options = IndexOptions {
		basis_commit: options.basis_commit.clone(),
		edge_batch_size: options.edge_batch_size,
		..IndexOptions::default()
	};

	let mut result = orchestrator::refresh_repo(
		storage,
		&mut extractors,
		repo_uid,
		&prepared.file_inputs,
		&mut idx_options,
	)
	.map_err(|e| ComposeError::Index(format!("{}", e)))?;

	persist_read_failures(
		storage,
		repo_uid,
		&result.snapshot_uid.clone(),
		&prepared.read_failed_paths,
		&mut result,
	)?;

	Ok(result)
}

/// Refresh a repo from disk, opening storage at db_path.
pub fn refresh_path(
	repo_path: &Path,
	db_path: &Path,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	let mut storage = open_or_create_storage(db_path)?;
	refresh_into_storage(repo_path, &mut storage, repo_uid, options)
}

// ── Helpers ──────────────────────────────────────────────────────

fn open_or_create_storage(db_path: &Path) -> Result<StorageConnection, ComposeError> {
	if db_path.to_string_lossy() == ":memory:" {
		StorageConnection::open_in_memory().map_err(ComposeError::Storage)
	} else {
		StorageConnection::open(db_path).map_err(ComposeError::Storage)
	}
}

fn ensure_repo(
	storage: &StorageConnection,
	repo_uid: &str,
	repo_path: &Path,
) -> Result<(), ComposeError> {
	use repo_graph_storage::types::{Repo, RepoRef};

	let existing = storage
		.get_repo(&RepoRef::Uid(repo_uid.into()))
		.map_err(ComposeError::Storage)?;
	if existing.is_some() {
		return Ok(());
	}

	storage
		.add_repo(&Repo {
			repo_uid: repo_uid.into(),
			name: repo_uid.into(),
			root_path: repo_path.to_string_lossy().into(),
			default_branch: None,
			created_at: "2025-01-01T00:00:00.000Z".into(),
			metadata_json: None,
		})
		.map_err(ComposeError::Storage)?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;

	fn make_fixture_repo() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::write(
			root.join("package.json"),
			r#"{"dependencies":{"express":"^4.18.0"}}"#,
		)
		.unwrap();
		fs::write(root.join(".gitignore"), "src/generated.ts\n").unwrap();
		fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
		fs::write(root.join("node_modules/pkg/index.ts"), "const x=1;").unwrap();
		fs::create_dir_all(root.join("src")).unwrap();
		fs::write(
			root.join("src/index.ts"),
			"import { serve } from \"./server\";\nserve();\n",
		)
		.unwrap();
		fs::write(root.join("src/server.ts"), "export function serve() {}\n").unwrap();
		fs::write(root.join("src/generated.ts"), "const gen = 1;").unwrap();
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

		let snap = storage.get_snapshot(&result.snapshot_uid).unwrap().unwrap();
		assert_eq!(snap.status, "ready");
		assert_eq!(result.files_total, 2, "files_total");
		assert_eq!(result.nodes_total, 4, "nodes_total");
		assert_eq!(result.edges_total, 4, "edges_total");
		assert_eq!(result.edges_unresolved, 0, "edges_unresolved");

		use repo_graph_indexer::storage_port::NodeStorePort;
		let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
		let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

		assert!(stable_keys.contains(&"r1:src/index.ts:FILE"));
		assert!(stable_keys.contains(&"r1:src/server.ts:FILE"));
		assert!(stable_keys.iter().any(|k| k.contains("#serve:SYMBOL:FUNCTION")));
		assert!(stable_keys.iter().any(|k| k.contains("src:MODULE")));
		assert!(!stable_keys.iter().any(|k| k.contains("generated")));
		assert!(!stable_keys.iter().any(|k| k.contains("node_modules")));
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

		assert_eq!(result.files_total, 2);
		assert_eq!(result.nodes_total, 4);
		assert_eq!(result.edges_total, 4);
		assert_eq!(result.edges_unresolved, 0);
	}
}
