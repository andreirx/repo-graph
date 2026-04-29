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
use repo_graph_classification::spring_liveness::{classify_spring_liveness, SpringNodeInput};
use repo_graph_classification::types::{PackageDependencySet, TsconfigAliases};
use repo_graph_policy_facts::{
    extractors::behavioral_marker::extract_behavioral_markers,
    extractors::status_mapping::extract_status_mappings,
    PolicyFactsStorageWrite,
};
use repo_graph_storage::types::InferenceInput;
use repo_graph_storage::StorageConnection;
use repo_graph_c_extractor::CExtractor;
use repo_graph_java_extractor::JavaExtractor;
use repo_graph_python_extractor::PythonExtractor;
use repo_graph_rust_extractor::RustExtractor;
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
	/// C/C++ include roots (configured via `--include-root`).
	/// Searched in order before conventional roots.
	pub c_include_roots: Vec<String>,
}

impl Default for ComposeOptions {
	fn default() -> Self {
		Self {
			basis_commit: None,
			edge_batch_size: None,
			c_include_roots: Vec::new(),
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
				// Language-aware dependency resolution — explicit per language.
				// Only the owning manifest type is resolved per language.
				// No language-specific fallback: Java/C/C++ files receive empty
				// signals until dedicated manifest readers exist for those languages.
				// This prevents mixed-repo contamination where a nearby package.json
				// would wrongly appear as dependency context for a Java or C file.
				let language = routing::detect_language(&ok.rel_path);
				let empty_deps = PackageDependencySet { names: vec![] };
				let empty_tsconfig = TsconfigAliases { entries: vec![] };
				let (pkg_deps, tsconfig) = match language {
					Some("rust") => {
						// Rust: Cargo.toml. tsconfig not applicable.
						let cargo_deps = config_ctx.resolve_cargo_deps(&ok.rel_path, repo_path);
						(cargo_deps, empty_tsconfig)
					}
					Some("typescript" | "tsx" | "javascript" | "jsx") => {
						// JS/TS: package.json + tsconfig.json.
						let pkg = config_ctx.resolve_package_deps(&ok.rel_path, repo_path);
						let ts = config_ctx.resolve_tsconfig_aliases(&ok.rel_path, repo_path);
						(pkg, ts)
					}
					_ => {
						// Java, C, C++, unknown: no manifest reader implemented yet.
						// Return empty rather than inheriting a nearby package.json.
						(empty_deps, empty_tsconfig)
					}
				};

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

// ── Post-index metrics persistence ───────────────────────────────

/// Persist metrics (complexity, parameter_count, nesting) from extraction.
///
/// RS-MS-3c-prereq: Called after index_repo or refresh_repo returns.
/// Converts ExtractedMetrics to MeasurementInput and batch-inserts.
fn persist_metrics(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	metrics: &std::collections::BTreeMap<String, repo_graph_indexer::types::ExtractedMetrics>,
) -> Result<(), ComposeError> {
	if metrics.is_empty() {
		return Ok(());
	}

	let now = "2025-01-01T00:00:00.000Z"; // Placeholder timestamp
	let source = "indexer:0.1.0";

	let mut measurements: Vec<repo_graph_storage::types::MeasurementInput> = Vec::new();

	for (stable_key, m) in metrics {
		// cyclomatic_complexity
		measurements.push(repo_graph_storage::types::MeasurementInput {
			measurement_uid: format!("{}-cc-{}", snapshot_uid, stable_key),
			snapshot_uid: snapshot_uid.into(),
			repo_uid: repo_uid.into(),
			target_stable_key: stable_key.clone(),
			kind: "cyclomatic_complexity".into(),
			value_json: format!(r#"{{"value":{}}}"#, m.cyclomatic_complexity),
			source: source.into(),
			created_at: now.into(),
		});

		// parameter_count
		measurements.push(repo_graph_storage::types::MeasurementInput {
			measurement_uid: format!("{}-pc-{}", snapshot_uid, stable_key),
			snapshot_uid: snapshot_uid.into(),
			repo_uid: repo_uid.into(),
			target_stable_key: stable_key.clone(),
			kind: "parameter_count".into(),
			value_json: format!(r#"{{"value":{}}}"#, m.parameter_count),
			source: source.into(),
			created_at: now.into(),
		});

		// max_nesting_depth
		measurements.push(repo_graph_storage::types::MeasurementInput {
			measurement_uid: format!("{}-mnd-{}", snapshot_uid, stable_key),
			snapshot_uid: snapshot_uid.into(),
			repo_uid: repo_uid.into(),
			target_stable_key: stable_key.clone(),
			kind: "max_nesting_depth".into(),
			value_json: format!(r#"{{"value":{}}}"#, m.max_nesting_depth),
			source: source.into(),
			created_at: now.into(),
		});

		// function_length (Phase A) — only persist if computed
		if let Some(fl) = m.function_length {
			measurements.push(repo_graph_storage::types::MeasurementInput {
				measurement_uid: format!("{}-fl-{}", snapshot_uid, stable_key),
				snapshot_uid: snapshot_uid.into(),
				repo_uid: repo_uid.into(),
				target_stable_key: stable_key.clone(),
				kind: "function_length".into(),
				value_json: format!(r#"{{"value":{}}}"#, fl),
				source: source.into(),
				created_at: now.into(),
			});
		}

		// cognitive_complexity (Phase A) — only persist if computed
		if let Some(cog) = m.cognitive_complexity {
			measurements.push(repo_graph_storage::types::MeasurementInput {
				measurement_uid: format!("{}-cog-{}", snapshot_uid, stable_key),
				snapshot_uid: snapshot_uid.into(),
				repo_uid: repo_uid.into(),
				target_stable_key: stable_key.clone(),
				kind: "cognitive_complexity".into(),
				value_json: format!(r#"{{"value":{}}}"#, cog),
				source: source.into(),
				created_at: now.into(),
			});
		}
	}

	storage
		.insert_measurements(&measurements)
		.map_err(ComposeError::Storage)?;

	Ok(())
}

// ── Post-index Spring liveness inference ─────────────────────────

/// Persist Spring framework-liveness inferences from extraction.
///
/// Queries all nodes from the snapshot, projects Java SYMBOL nodes
/// with metadata_json to SpringNodeInput, runs the Spring liveness
/// classifier, and persists the resulting inferences.
///
/// This enables dead-code suppression for Spring container-managed
/// symbols (@Service, @Component, @Repository, @Controller,
/// @RestController, @Configuration classes; @Bean methods).
fn persist_spring_liveness_inferences(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
) -> Result<(), ComposeError> {
	// Query all nodes for the snapshot
	let nodes = storage
		.query_all_nodes(snapshot_uid)
		.map_err(ComposeError::Storage)?;

	// Project to SpringNodeInput — only SYMBOL nodes with metadata
	let inputs: Vec<SpringNodeInput> = nodes
		.iter()
		.filter(|n| n.kind == "SYMBOL" && n.metadata_json.is_some())
		.map(|n| SpringNodeInput {
			stable_key: n.stable_key.clone(),
			kind: n.kind.clone(),
			subtype: n.subtype.clone(),
			metadata_json: n.metadata_json.clone(),
		})
		.collect();

	if inputs.is_empty() {
		return Ok(());
	}

	// Run classifier
	let classified = classify_spring_liveness(&inputs);

	if classified.is_empty() {
		return Ok(());
	}

	// Convert to InferenceInput
	// Use real ISO timestamp and version consistent with Rust indexer
	let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
	let extractor = "indexer:1.0.0"; // Match INDEXER_VERSION in orchestrator.rs

	let inferences: Vec<InferenceInput> = classified
		.iter()
		.enumerate()
		.map(|(i, inf)| InferenceInput {
			inference_uid: format!("{}-spring-{}", snapshot_uid, i),
			snapshot_uid: snapshot_uid.to_string(),
			repo_uid: repo_uid.to_string(),
			target_stable_key: inf.target_stable_key.clone(),
			kind: inf.kind.clone(),
			value_json: inf.value_json.clone(),
			confidence: inf.confidence,
			basis_json: inf.basis_json.clone(),
			extractor: extractor.to_string(),
			created_at: now.clone(),
		})
		.collect();

	// Atomic replace: delete + insert in a single transaction.
	// If insert fails, the old inference set survives (transaction rollback).
	storage
		.replace_inferences_by_kind(snapshot_uid, &["spring_container_managed"], &inferences)
		.map_err(ComposeError::Storage)?;

	Ok(())
}

// ── Post-index policy-facts extraction ───────────────────────────

/// Extract and persist STATUS_MAPPING policy facts from C files.
///
/// PF-1 TEMPORARY postpass: Re-parses C files after extraction to
/// extract STATUS_MAPPING facts. This duplicates the tree-sitter
/// parsing work already done by the C extractor.
///
/// **TECH DEBT:** This re-parse approach is explicitly temporary.
/// The target architecture is extraction-time integration where
/// the C extractor carries policy-fact output directly. See
/// `docs/TECH-DEBT.md` entry "PF-1 temporary re-parse postpass".
///
/// Returns the total number of policy facts persisted
/// (STATUS_MAPPING + BEHAVIORAL_MARKER).
fn persist_policy_facts(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	file_inputs: &[FileInput],
) -> Result<usize, ComposeError> {
	// Initialize tree-sitter parser for C.
	let mut parser = tree_sitter::Parser::new();
	let c_language: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
	parser
		.set_language(&c_language)
		.map_err(|e| ComposeError::ExtractorInit(format!("policy-facts C parser: {}", e)))?;

	let mut all_mappings = Vec::new();
	let mut all_markers = Vec::new();

	for file in file_inputs {
		// Policy-facts scope: C files only (.c and .h).
		// C++ (.cpp, .hpp, .cc, .cxx) is explicitly out of scope.
		// See docs/slices/pf-1-status-mapping.md "What PF-1 Does NOT Include".
		// See docs/slices/pf-2-behavioral-marker.md "Non-Goals".
		let is_c_file = file.rel_path.ends_with(".c") || file.rel_path.ends_with(".h");

		if !is_c_file {
			continue;
		}

		// Parse the file.
		let tree = match parser.parse(&file.content, None) {
			Some(t) => t,
			None => continue, // Parse failed, skip.
		};

		// PF-1: Extract STATUS_MAPPING facts.
		let mappings = extract_status_mappings(
			&tree,
			file.content.as_bytes(),
			&file.rel_path,
			repo_uid,
		);
		all_mappings.extend(mappings);

		// PF-2: Extract BEHAVIORAL_MARKER facts.
		let markers = extract_behavioral_markers(
			&tree,
			file.content.as_bytes(),
			&file.rel_path,
			repo_uid,
		);
		all_markers.extend(markers);
	}

	let mut total_count = 0;

	// Persist STATUS_MAPPING facts.
	if !all_mappings.is_empty() {
		let count = storage
			.insert_status_mappings(snapshot_uid, &all_mappings)
			.map_err(|e| ComposeError::Index(format!("policy-facts storage: {}", e)))?;
		total_count += count;
	}

	// Persist BEHAVIORAL_MARKER facts.
	if !all_markers.is_empty() {
		let count = storage
			.insert_behavioral_markers(snapshot_uid, &all_markers)
			.map_err(|e| ComposeError::Index(format!("policy-facts storage: {}", e)))?;
		total_count += count;
	}

	Ok(total_count)
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

	let mut ts_extractor = TsExtractor::new();
	ts_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("ts: {}", e)))?;

	let mut c_extractor = CExtractor::new();
	c_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("c: {}", e)))?;

	let mut java_extractor = JavaExtractor::new();
	java_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("java: {}", e)))?;

	let mut python_extractor = PythonExtractor::new();
	python_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("python: {}", e)))?;

	let mut rust_extractor = RustExtractor::new();
	rust_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("rust: {}", e)))?;

	ensure_repo(storage, repo_uid, repo_path)?;

	let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ts_extractor, &mut c_extractor, &mut java_extractor, &mut python_extractor, &mut rust_extractor];
	let mut idx_options = IndexOptions {
		basis_commit: options.basis_commit.clone(),
		edge_batch_size: options.edge_batch_size,
		c_include_roots: options.c_include_roots.clone(),
		..IndexOptions::default()
	};

	// State-boundary hook: wired at the composition root (SB-4-pre).
	// Constructs the hook; on invalid repo_uid it degrades
	// gracefully (diagnostic, no emission, no abort).
	let mut sb_hook = crate::state_boundary_hook::StateBoundaryHook::new(repo_uid);

	let mut result = orchestrator::index_repo(
		storage,
		&mut extractors,
		repo_uid,
		&prepared.file_inputs,
		&mut idx_options,
		Some(&mut sb_hook),
	)
	.map_err(|e| ComposeError::Index(format!("{}", e)))?;

	persist_read_failures(
		storage,
		repo_uid,
		&result.snapshot_uid.clone(),
		&prepared.read_failed_paths,
		&mut result,
	)?;

	// RS-MS-3c-prereq: Persist metrics (complexity, params, nesting).
	persist_metrics(storage, repo_uid, &result.snapshot_uid, &result.metrics)?;

	// Persist Spring framework-liveness inferences for dead-code suppression.
	persist_spring_liveness_inferences(storage, repo_uid, &result.snapshot_uid)?;

	// PF-1: Extract and persist STATUS_MAPPING policy facts from C files.
	// TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
	persist_policy_facts(storage, repo_uid, &result.snapshot_uid, &prepared.file_inputs)?;

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

	let mut ts_extractor = TsExtractor::new();
	ts_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("ts: {}", e)))?;

	let mut c_extractor = CExtractor::new();
	c_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("c: {}", e)))?;

	let mut java_extractor = JavaExtractor::new();
	java_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("java: {}", e)))?;

	let mut python_extractor = PythonExtractor::new();
	python_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("python: {}", e)))?;

	let mut rust_extractor = RustExtractor::new();
	rust_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("rust: {}", e)))?;

	ensure_repo(storage, repo_uid, repo_path)?;

	let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ts_extractor, &mut c_extractor, &mut java_extractor, &mut python_extractor, &mut rust_extractor];
	let mut idx_options = IndexOptions {
		basis_commit: options.basis_commit.clone(),
		edge_batch_size: options.edge_batch_size,
		c_include_roots: options.c_include_roots.clone(),
		..IndexOptions::default()
	};

	// State-boundary hook (symmetric with index path — SB-4-pre.8).
	let mut sb_hook = crate::state_boundary_hook::StateBoundaryHook::new(repo_uid);

	let mut result = orchestrator::refresh_repo(
		storage,
		&mut extractors,
		repo_uid,
		&prepared.file_inputs,
		&mut idx_options,
		Some(&mut sb_hook),
	)
	.map_err(|e| ComposeError::Index(format!("{}", e)))?;

	persist_read_failures(
		storage,
		repo_uid,
		&result.snapshot_uid.clone(),
		&prepared.read_failed_paths,
		&mut result,
	)?;

	// RS-MS-3c-prereq: Persist metrics (complexity, params, nesting).
	persist_metrics(storage, repo_uid, &result.snapshot_uid, &result.metrics)?;

	// Persist Spring framework-liveness inferences for dead-code suppression.
	persist_spring_liveness_inferences(storage, repo_uid, &result.snapshot_uid)?;

	// PF-1: Extract and persist STATUS_MAPPING policy facts from C files.
	// TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
	persist_policy_facts(storage, repo_uid, &result.snapshot_uid, &prepared.file_inputs)?;

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

	#[test]
	fn index_into_storage_persists_metrics() {
		let fixture = make_fixture_repo();
		let mut storage = StorageConnection::open_in_memory().unwrap();

		let result = index_into_storage(
			fixture.path(),
			&mut storage,
			"r1",
			&ComposeOptions::default(),
		)
		.unwrap();

		// The fixture has `serve` function in server.ts which should have metrics.
		// Verify metrics are in the result.
		assert!(
			!result.metrics.is_empty(),
			"expected metrics for functions in fixture; got empty metrics map"
		);

		// Verify metrics are persisted to storage.
		let cc_rows = storage
			.query_measurements_by_kind(&result.snapshot_uid, "cyclomatic_complexity")
			.unwrap();
		assert!(
			!cc_rows.is_empty(),
			"expected cyclomatic_complexity measurements persisted; got none"
		);

		// All three metric kinds should be persisted.
		let pc_rows = storage
			.query_measurements_by_kind(&result.snapshot_uid, "parameter_count")
			.unwrap();
		let mnd_rows = storage
			.query_measurements_by_kind(&result.snapshot_uid, "max_nesting_depth")
			.unwrap();
		assert_eq!(
			cc_rows.len(),
			pc_rows.len(),
			"cyclomatic_complexity and parameter_count counts must match"
		);
		assert_eq!(
			cc_rows.len(),
			mnd_rows.len(),
			"cyclomatic_complexity and max_nesting_depth counts must match"
		);
	}

	// ── Java extractor integration ───────────────────────────────

	fn make_java_fixture_repo() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::create_dir_all(root.join("src/main/java/com/example")).unwrap();
		fs::write(
			root.join("src/main/java/com/example/App.java"),
			r#"package com.example;

import java.util.List;

public class App {
    private String name;

    public App(String name) {
        this.name = name;
    }

    public void run() {
        System.out.println("Hello " + name);
    }

    public static void main(String[] args) {
        App app = new App("World");
        app.run();
    }
}
"#,
		)
		.unwrap();
		fs::write(
			root.join("src/main/java/com/example/Service.java"),
			r#"package com.example;

public interface Service {
    void execute();
}
"#,
		)
		.unwrap();

		dir
	}

	#[test]
	fn index_java_extracts_file_and_symbol_nodes() {
		let fixture = make_java_fixture_repo();
		let mut storage = StorageConnection::open_in_memory().unwrap();

		let result = index_into_storage(
			fixture.path(),
			&mut storage,
			"java-test",
			&ComposeOptions::default(),
		)
		.unwrap();

		// Should have indexed 2 Java files
		assert_eq!(result.files_total, 2, "files_total");

		use repo_graph_indexer::storage_port::NodeStorePort;
		let nodes = NodeStorePort::query_all_nodes(&storage, &result.snapshot_uid).unwrap();
		let stable_keys: Vec<&str> = nodes.iter().map(|n| n.stable_key.as_str()).collect();

		// FILE nodes for both Java files
		assert!(
			stable_keys.iter().any(|k| k.contains("App.java:FILE")),
			"expected App.java FILE node; got keys: {:?}",
			stable_keys
		);
		assert!(
			stable_keys.iter().any(|k| k.contains("Service.java:FILE")),
			"expected Service.java FILE node; got keys: {:?}",
			stable_keys
		);

		// SYMBOL nodes: class App, interface Service, methods
		assert!(
			stable_keys.iter().any(|k| k.contains("#App:SYMBOL:CLASS")),
			"expected App CLASS symbol; got keys: {:?}",
			stable_keys
		);
		assert!(
			stable_keys.iter().any(|k| k.contains("#Service:SYMBOL:INTERFACE")),
			"expected Service INTERFACE symbol; got keys: {:?}",
			stable_keys
		);
		assert!(
			stable_keys.iter().any(|k| k.contains("#App.run:SYMBOL:METHOD")),
			"expected App.run METHOD symbol; got keys: {:?}",
			stable_keys
		);
		assert!(
			stable_keys.iter().any(|k| k.contains("#App.main:SYMBOL:METHOD")),
			"expected App.main METHOD symbol; got keys: {:?}",
			stable_keys
		);

		// Constructor
		assert!(
			stable_keys.iter().any(|k| k.contains("#App:SYMBOL:CONSTRUCTOR")),
			"expected App CONSTRUCTOR symbol; got keys: {:?}",
			stable_keys
		);

		// Field (uses PROPERTY subtype, consistent with TS extractor)
		assert!(
			stable_keys.iter().any(|k| k.contains("#App.name:SYMBOL:PROPERTY")),
			"expected App.name PROPERTY symbol; got keys: {:?}",
			stable_keys
		);
	}

	#[test]
	fn index_java_persists_metrics() {
		let fixture = make_java_fixture_repo();
		let mut storage = StorageConnection::open_in_memory().unwrap();

		let result = index_into_storage(
			fixture.path(),
			&mut storage,
			"java-test",
			&ComposeOptions::default(),
		)
		.unwrap();

		// Java methods should have metrics
		assert!(
			!result.metrics.is_empty(),
			"expected metrics for Java methods; got empty metrics map"
		);

		// Verify metrics persisted
		let cc_rows = storage
			.query_measurements_by_kind(&result.snapshot_uid, "cyclomatic_complexity")
			.unwrap();
		assert!(
			!cc_rows.is_empty(),
			"expected cyclomatic_complexity measurements for Java methods; got none"
		);
	}

	// ── Spring liveness inference integration ────────────────────

	fn make_spring_fixture_repo() -> tempfile::TempDir {
		let dir = tempfile::tempdir().unwrap();
		let root = dir.path();

		fs::create_dir_all(root.join("src/main/java/com/example")).unwrap();

		// @Service class — should be inferred as spring_container_managed
		fs::write(
			root.join("src/main/java/com/example/UserService.java"),
			r#"package com.example;

import org.springframework.stereotype.Service;

@Service
public class UserService {
    public void process() {
        System.out.println("Processing...");
    }
}
"#,
		)
		.unwrap();

		// @RestController — should be inferred as spring_container_managed
		fs::write(
			root.join("src/main/java/com/example/ApiController.java"),
			r#"package com.example;

import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.bind.annotation.GetMapping;

@RestController
public class ApiController {
    @GetMapping("/health")
    public String health() {
        return "ok";
    }
}
"#,
		)
		.unwrap();

		// Plain class (no Spring annotation) — should NOT be inferred
		fs::write(
			root.join("src/main/java/com/example/PlainHelper.java"),
			r#"package com.example;

public class PlainHelper {
    public static void help() {
        System.out.println("Helping...");
    }
}
"#,
		)
		.unwrap();

		// @Configuration with @Bean method — both should be inferred
		fs::write(
			root.join("src/main/java/com/example/AppConfig.java"),
			r#"package com.example;

import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Bean;

@Configuration
public class AppConfig {
    @Bean
    public String appName() {
        return "MyApp";
    }
}
"#,
		)
		.unwrap();

		dir
	}

	#[test]
	fn index_spring_produces_container_managed_inferences() {
		let fixture = make_spring_fixture_repo();
		let mut storage = StorageConnection::open_in_memory().unwrap();

		let result = index_into_storage(
			fixture.path(),
			&mut storage,
			"spring-test",
			&ComposeOptions::default(),
		)
		.unwrap();

		// Should have indexed 4 Java files
		assert_eq!(result.files_total, 4, "files_total");

		// Query Spring inferences
		let inferences = storage
			.query_inferences_by_kind(&result.snapshot_uid, "spring_container_managed")
			.unwrap();

		// Should have inferences for:
		// - UserService (@Service)
		// - ApiController (@RestController)
		// - AppConfig (@Configuration)
		// - AppConfig.appName (@Bean)
		assert_eq!(
			inferences.len(),
			4,
			"expected 4 spring_container_managed inferences; got {}",
			inferences.len()
		);

		let targets: Vec<&str> = inferences
			.iter()
			.map(|i| i.target_stable_key.as_str())
			.collect();

		assert!(
			targets.iter().any(|t| t.contains("UserService:SYMBOL:CLASS")),
			"expected UserService inference; targets: {:?}",
			targets
		);
		assert!(
			targets.iter().any(|t| t.contains("ApiController:SYMBOL:CLASS")),
			"expected ApiController inference; targets: {:?}",
			targets
		);
		assert!(
			targets.iter().any(|t| t.contains("AppConfig:SYMBOL:CLASS")),
			"expected AppConfig inference; targets: {:?}",
			targets
		);
		assert!(
			targets.iter().any(|t| t.contains("appName:SYMBOL:METHOD")),
			"expected appName @Bean inference; targets: {:?}",
			targets
		);

		// PlainHelper should NOT have an inference
		assert!(
			!targets.iter().any(|t| t.contains("PlainHelper")),
			"PlainHelper should not have spring inference; targets: {:?}",
			targets
		);
	}

	#[test]
	fn index_spring_inferences_idempotent_on_reindex() {
		let fixture = make_spring_fixture_repo();
		let mut storage = StorageConnection::open_in_memory().unwrap();

		// First index
		let result1 = index_into_storage(
			fixture.path(),
			&mut storage,
			"spring-test",
			&ComposeOptions::default(),
		)
		.unwrap();

		let inferences1 = storage
			.query_inferences_by_kind(&result1.snapshot_uid, "spring_container_managed")
			.unwrap();

		// Second index (creates new snapshot)
		let result2 = index_into_storage(
			fixture.path(),
			&mut storage,
			"spring-test",
			&ComposeOptions::default(),
		)
		.unwrap();

		let inferences2 = storage
			.query_inferences_by_kind(&result2.snapshot_uid, "spring_container_managed")
			.unwrap();

		// Both should have same count
		assert_eq!(
			inferences1.len(),
			inferences2.len(),
			"inference counts must match across re-index"
		);
	}
}
