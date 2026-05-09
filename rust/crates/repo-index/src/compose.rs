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

use std::ops::ControlFlow;
use std::path::Path;

use repo_graph_indexer::cargo_manifest::{
    self, CargoModule, CargoModuleCandidateInput, CargoModuleEvidenceInput,
    CargoModuleStorePort, FileOwnershipInput,
};
use repo_graph_indexer::package_json::{self, NpmModule};
use repo_graph_indexer::pyproject::{self, PyprojectModule};
use repo_graph_indexer::settings_gradle::{self, GradleModule};
use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_indexer::orchestrator::{self, FileInput, IndexError};
use repo_graph_indexer::proto_indexer::ProtoFileInput;
use repo_graph_indexer::routing;
use repo_graph_indexer::storage_port::SnapshotLifecyclePort;
use repo_graph_indexer::types::{IndexOptions, IndexPhase, IndexProgressEvent, IndexResult};
use repo_graph_classification::spring_liveness::{classify_spring_liveness, SpringNodeInput};
use repo_graph_classification::types::{PackageDependencySet, TsconfigAliases};
use repo_graph_policy_facts::{
    extractors::behavioral_marker::extract_behavioral_markers,
    extractors::return_fate::extract_return_fates,
    extractors::status_mapping::extract_status_mappings,
    PolicyFactsStorageWrite,
};
use repo_graph_boundary_interaction::table::Language as BiLanguage;
use repo_graph_boundary_interaction_extractor::emit::{
    BoundaryCallsite, BoundaryInteractionEmitter, EmitterContext, MmapFlags, SocketFamily,
    SocketType,
};
use repo_graph_c_extractor::{
    extract_boundary_calls, MmapFlags as RawMmapFlags, RawBoundaryCall,
    SocketFamily as RawSocketFamily, SocketType as RawSocketType,
};
use repo_graph_storage::types::InferenceInput;
use repo_graph_storage::StorageConnection;
use repo_graph_c_extractor::CExtractor;
use repo_graph_cpp_extractor::CppExtractor;
use repo_graph_java_extractor::JavaExtractor;
use repo_graph_python_extractor::PythonExtractor;
use repo_graph_rust_extractor::RustExtractor;
use repo_graph_ts_extractor::{
    extract_amqp_boundary_calls, extract_kafka_boundary_calls, extract_nats_boundary_calls,
    extract_ts_boundary_calls, RawAmqpBoundaryCall, RawKafkaBoundaryCall, RawNatsBoundaryCall,
    RawTsBoundaryCall, TsExtractor,
};

use crate::config::RepoConfigContext;
use crate::impact_propagation::{propagate_impact, ImpactReport};
use crate::refresh_policy::{
    FamilyRefreshResult, RefreshAction, RefreshDiagnostics, COPY_FORWARD_FAMILIES,
};
use crate::scanner::{self, ScannedFile};
use artifact_contracts::{get_contract, ArtifactFamily, RefreshPolicy};

// ── Error type ───────────────────────────────────────────────────

/// Error from the composition layer.
#[derive(Debug)]
pub enum ComposeError {
	Scan(scanner::ScanError),
	Storage(repo_graph_storage::error::StorageError),
	Index(String),
	ExtractorInit(String),
	/// Operation aborted at a progress checkpoint.
	///
	/// This occurs when the progress callback signals stop (e.g., due to
	/// transport failure in daemon mode). The operation terminates early
	/// to avoid completing with a broken control channel.
	Aborted,
}

impl std::fmt::Display for ComposeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Scan(e) => write!(f, "scan: {}", e),
			Self::Storage(e) => write!(f, "storage: {}", e),
			Self::Index(e) => write!(f, "index: {}", e),
			Self::ExtractorInit(e) => write!(f, "extractor init: {}", e),
			Self::Aborted => write!(f, "operation aborted at progress checkpoint"),
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
	/// Path to store in `repos.root_path`. If None, uses `repo_path.to_string_lossy()`.
	///
	/// CLI should compute this as the repo path relative to the DB file location.
	/// This ensures filesystem-backed surfaces resolve correctly regardless of cwd.
	pub storage_root_path: Option<String>,
}

impl Default for ComposeOptions {
	fn default() -> Self {
		Self {
			basis_commit: None,
			edge_batch_size: None,
			c_include_roots: Vec::new(),
			storage_root_path: None,
		}
	}
}

// ── Progress reporting ───────────────────────────────────────────

/// Progress event emitted during index/refresh operations.
#[derive(Debug, Clone)]
pub struct ProgressEvent {
	/// Current phase name (e.g., "scanning", "extracting", "persisting").
	pub phase: String,
	/// Current progress count within the phase.
	pub current: u64,
	/// Total expected count (0 if unknown).
	pub total: u64,
}

impl ProgressEvent {
	/// Create a new progress event.
	pub fn new(phase: impl Into<String>, current: u64, total: u64) -> Self {
		Self {
			phase: phase.into(),
			current,
			total,
		}
	}
}

/// Callback for progress reporting.
///
/// Returns `ControlFlow::Continue(())` to proceed, or `ControlFlow::Break(())`
/// to abort the operation at the current checkpoint.
///
/// This is an **abort checkpoint seam**: the callback can signal stop due to
/// transport failure, cancellation, or any other reason. The orchestration
/// layer will terminate early rather than continue with a broken control channel.
///
/// Uses `FnMut` because callbacks often need to mutate state (e.g., write
/// to an output stream, update a counter, or emit to an event sink).
pub type ProgressCallback<'a> = &'a mut dyn FnMut(&ProgressEvent) -> ControlFlow<()>;

/// Helper to emit progress if callback is provided.
///
/// Returns `Ok(())` if progress was emitted successfully or no callback exists.
/// Returns `Err(ComposeError::Aborted)` if callback signaled stop.
#[inline]
fn emit_progress(progress: &mut Option<ProgressCallback<'_>>, phase: &str, current: u64, total: u64) -> Result<(), ComposeError> {
	if let Some(cb) = progress {
		match cb(&ProgressEvent::new(phase, current, total)) {
			ControlFlow::Continue(()) => Ok(()),
			ControlFlow::Break(()) => Err(ComposeError::Aborted),
		}
	} else {
		Ok(())
	}
}

// ── Shared preparation ───────────────────────────────────────────

/// Result of scanning + config resolution + FileInput assembly.
/// Carries both readable files and read-failed paths so callers
/// can handle the read-failure contract correctly.
/// A config file (e.g., package.json, Cargo.toml) tracked for invalidation.
/// Config files are NOT extracted — they're only tracked for hash comparison
/// during refresh to trigger scope-widening when they change.
#[derive(Debug, Clone)]
pub struct ConfigFileInput {
	/// Repo-relative path.
	pub rel_path: String,
	/// Content hash for change detection.
	pub content_hash: String,
	/// Line count.
	pub line_count: usize,
}

/// Extracted Cargo module from a Cargo.toml manifest.
///
/// Separate from ConfigFileInput per rust-module-parity design:
/// ConfigFileInput is refresh/config substrate, this is module-domain output.
#[derive(Debug, Clone)]
pub struct ExtractedCargoModule {
	/// The extracted module data
	pub module: CargoModule,
	/// Declared pattern that led to this module (for workspace members)
	/// None for root crates, Some("crates/*") for pattern-matched members
	pub declared_pattern: Option<String>,
}

/// Result of Cargo.toml extraction for a repo.
#[derive(Debug, Clone, Default)]
pub struct CargoExtractionResult {
	/// Extracted modules (root crate + resolved workspace members)
	pub modules: Vec<ExtractedCargoModule>,
	/// Workspace patterns that were declared but had no matches
	/// (recorded for evidence, not an error)
	pub unmatched_patterns: Vec<String>,
	/// Whether the repo root has a Cargo.toml
	pub has_root_manifest: bool,
}

/// Extracted npm module with provenance info (rust-module-parity Phase 2).
#[derive(Debug, Clone)]
pub struct ExtractedNpmModule {
	/// The extracted module data
	pub module: NpmModule,
	/// Declared pattern that led to this module (for workspace members)
	/// None for root packages, Some("packages/*") for pattern-matched members
	pub declared_pattern: Option<String>,
}

/// Result of package.json extraction for a repo.
#[derive(Debug, Clone, Default)]
pub struct NpmExtractionResult {
	/// Extracted modules (root package + resolved workspace members)
	pub modules: Vec<ExtractedNpmModule>,
	/// Workspace patterns that were declared but had no matches
	pub unmatched_patterns: Vec<String>,
	/// Whether the repo root has a package.json
	pub has_root_manifest: bool,
	/// Whether workspace patterns came from pnpm-workspace.yaml
	pub is_pnpm_workspace: bool,
}

/// Extracted pyproject module with provenance info (rust-module-parity Phase 2c).
#[derive(Debug, Clone)]
pub struct ExtractedPyprojectModule {
	/// The extracted module data
	pub module: PyprojectModule,
}

/// Result of pyproject.toml extraction for a repo.
#[derive(Debug, Clone, Default)]
pub struct PyprojectExtractionResult {
	/// Extracted modules (single-package for Phase 2c)
	pub modules: Vec<ExtractedPyprojectModule>,
	/// Whether the repo root has a pyproject.toml
	pub has_root_manifest: bool,
}

/// Extracted Gradle module with provenance info (rust-module-parity Phase 2b).
#[derive(Debug, Clone)]
pub struct ExtractedGradleModule {
	/// The extracted module data
	pub module: GradleModule,
}

/// Result of settings.gradle extraction for a repo.
#[derive(Debug, Clone, Default)]
pub struct GradleExtractionResult {
	/// Extracted modules (root + subprojects)
	pub modules: Vec<ExtractedGradleModule>,
	/// Whether the repo root has a settings.gradle
	pub has_root_settings: bool,
}

pub struct PreparedRepoInputs {
	/// Readable source files with config attached, ready for the orchestrator.
	pub file_inputs: Vec<FileInput>,
	/// Paths that were discovered but could not be read.
	pub read_failed_paths: Vec<String>,
	/// Contract files (e.g., .proto) for the contract indexing subpipeline.
	pub contract_file_inputs: Vec<ProtoFileInput>,
	/// Config files tracked for invalidation widening (not extracted).
	pub config_file_inputs: Vec<ConfigFileInput>,
	/// Cargo.toml extraction results (rust-module-parity Phase 1).
	/// Separate from config_file_inputs: this is module-domain output.
	pub cargo_modules: CargoExtractionResult,
	/// package.json extraction results (rust-module-parity Phase 2).
	pub npm_modules: NpmExtractionResult,
	/// pyproject.toml extraction results (rust-module-parity Phase 2c).
	pub pyproject_modules: PyprojectExtractionResult,
	/// settings.gradle extraction results (rust-module-parity Phase 2b).
	pub gradle_modules: GradleExtractionResult,
}

/// Scan the repo, resolve config per file, assemble typed FileInput.
///
/// Files are partitioned into:
/// - `file_inputs`: source files for the language extraction pipeline
/// - `contract_file_inputs`: contract files (e.g., .proto) for the contract pipeline
pub fn prepare_repo_inputs(
	repo_path: &Path,
) -> Result<PreparedRepoInputs, ComposeError> {
	let scanned = scanner::scan_repo(repo_path).map_err(ComposeError::Scan)?;
	let mut config_ctx = RepoConfigContext::new();

	let mut file_inputs = Vec::new();
	let mut contract_file_inputs = Vec::new();
	let mut config_file_inputs = Vec::new();
	let mut read_failed_paths = Vec::new();
	// Collect Cargo.toml files with content for module extraction.
	// Keyed by rel_path for later workspace member resolution.
	let mut cargo_toml_files: std::collections::HashMap<String, String> = std::collections::HashMap::new();
	// Collect package.json files with content for npm module extraction (Phase 2).
	let mut package_json_files: std::collections::HashMap<String, String> = std::collections::HashMap::new();
	// Content of pnpm-workspace.yaml if present (Phase 2).
	let mut pnpm_workspace_content: Option<String> = None;
	// Collect pyproject.toml files with content for Python module extraction (Phase 2c).
	let mut pyproject_toml_files: std::collections::HashMap<String, String> = std::collections::HashMap::new();
	// Collect settings.gradle files with content for Gradle module extraction (Phase 2b).
	let mut settings_gradle_files: std::collections::HashMap<String, String> = std::collections::HashMap::new();

	for file in &scanned {
		match file {
			ScannedFile::Ok(ok) => {
				// Check for contract file extension first (e.g., .proto).
				// Contract files go to the contract pipeline, not language extraction.
				let ext = routing::get_extension(&ok.rel_path);
				if routing::is_contract_extension(ext) {
					contract_file_inputs.push(ProtoFileInput {
						rel_path: ok.rel_path.clone(),
						content: ok.content.clone(),
						content_hash: ok.content_hash.clone(),
					});
					continue;
				}

				// Config files are tracked for invalidation widening but NOT extracted.
				// They don't produce FILE nodes or symbols — only file_versions for
				// hash comparison during refresh.
				if routing::is_config_file(&ok.rel_path) {
					// Collect Cargo.toml content for module extraction (separate from config tracking).
					if ok.rel_path.ends_with("Cargo.toml") {
						cargo_toml_files.insert(ok.rel_path.clone(), ok.content.clone());
					}
					// Collect package.json content for npm module extraction (Phase 2).
					if ok.rel_path.ends_with("package.json") {
						package_json_files.insert(ok.rel_path.clone(), ok.content.clone());
					}
					// Collect pnpm-workspace.yaml for workspace pattern extraction (Phase 2).
					if ok.rel_path == "pnpm-workspace.yaml" {
						pnpm_workspace_content = Some(ok.content.clone());
					}
					// Collect pyproject.toml content for Python module extraction (Phase 2c).
					if ok.rel_path.ends_with("pyproject.toml") {
						pyproject_toml_files.insert(ok.rel_path.clone(), ok.content.clone());
					}
					// Collect settings.gradle content for Gradle module extraction (Phase 2b).
					if ok.rel_path == "settings.gradle" || ok.rel_path == "settings.gradle.kts" {
						settings_gradle_files.insert(ok.rel_path.clone(), ok.content.clone());
					}
					config_file_inputs.push(ConfigFileInput {
						rel_path: ok.rel_path.clone(),
						content_hash: ok.content_hash.clone(),
						line_count: ok.line_count,
					});
					continue;
				}

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

	// Extract Cargo modules from collected Cargo.toml files.
	let cargo_modules = extract_cargo_modules(repo_path, &cargo_toml_files);

	// Extract npm modules from collected package.json files (Phase 2).
	let npm_modules = extract_npm_modules(
		repo_path,
		&package_json_files,
		pnpm_workspace_content.as_deref(),
	);

	// Extract pyproject modules from collected pyproject.toml files (Phase 2c).
	let pyproject_modules = extract_pyproject_modules(&pyproject_toml_files);

	// Extract Gradle modules from collected settings.gradle files (Phase 2b).
	let gradle_modules = extract_gradle_modules(&settings_gradle_files);

	Ok(PreparedRepoInputs {
		file_inputs,
		read_failed_paths,
		contract_file_inputs,
		config_file_inputs,
		cargo_modules,
		npm_modules,
		pyproject_modules,
		gradle_modules,
	})
}

// ── Cargo module extraction ──────────────────────────────────────────

/// Extract Cargo modules from Cargo.toml files.
///
/// Parses the root Cargo.toml, expands workspace member patterns using
/// glob, and collects all resolved crates with their evidence.
fn extract_cargo_modules(
	repo_path: &Path,
	cargo_toml_files: &std::collections::HashMap<String, String>,
) -> CargoExtractionResult {
	let mut result = CargoExtractionResult::default();

	// Check for root Cargo.toml
	let root_manifest_path = "Cargo.toml";
	let root_content = match cargo_toml_files.get(root_manifest_path) {
		Some(content) => content,
		None => return result, // No Cargo.toml at root, not a Rust project
	};

	result.has_root_manifest = true;

	// Parse root manifest
	let root_parsed = match cargo_manifest::parse_cargo_toml(root_content, root_manifest_path) {
		Ok(parsed) => parsed,
		Err(_) => return result, // Parse error, skip silently (matches Cargo behavior)
	};

	// Add root crate if it has a [package] section
	for module in &root_parsed.modules {
		result.modules.push(ExtractedCargoModule {
			module: module.clone(),
			declared_pattern: None,
		});
	}

	// If workspace root, expand member patterns
	if root_parsed.is_workspace_root {
		for pattern in &root_parsed.workspace_members {
			let expanded = expand_workspace_pattern(repo_path, pattern, cargo_toml_files);
			if expanded.is_empty() {
				result.unmatched_patterns.push(pattern.clone());
			} else {
				for member_module in expanded {
					result.modules.push(ExtractedCargoModule {
						module: member_module,
						declared_pattern: Some(pattern.clone()),
					});
				}
			}
		}
	}

	result
}

/// Expand a workspace member pattern and return resolved modules.
///
/// Pattern examples:
/// - "crates/foo" → direct path
/// - "crates/*" → glob all directories under crates/
/// - "packages/*" → glob all directories under packages/
///
/// For each match, checks if a Cargo.toml exists and parses it.
fn expand_workspace_pattern(
	repo_path: &Path,
	pattern: &str,
	cargo_toml_files: &std::collections::HashMap<String, String>,
) -> Vec<CargoModule> {
	let mut modules = Vec::new();

	// Check if pattern contains glob characters
	if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
		// Glob expansion
		let full_pattern = repo_path.join(pattern).join("Cargo.toml");
		let pattern_str = full_pattern.to_string_lossy();

		if let Ok(paths) = glob::glob(&pattern_str) {
			for entry in paths.flatten() {
				// Convert back to repo-relative path
				if let Ok(rel) = entry.strip_prefix(repo_path) {
					let rel_path = rel.to_string_lossy().replace('\\', "/");
					if let Some(content) = cargo_toml_files.get(&rel_path) {
						if let Ok(parsed) = cargo_manifest::parse_cargo_toml(content, &rel_path) {
							for mut module in parsed.modules {
								module.is_workspace_member = true;
								modules.push(module);
							}
						}
					}
				}
			}
		}
	} else {
		// Direct path (no glob)
		let manifest_path = format!("{}/Cargo.toml", pattern);
		if let Some(content) = cargo_toml_files.get(&manifest_path) {
			if let Ok(parsed) = cargo_manifest::parse_cargo_toml(content, &manifest_path) {
				for mut module in parsed.modules {
					module.is_workspace_member = true;
					modules.push(module);
				}
			}
		}
	}

	modules
}

// ── npm module extraction (rust-module-parity Phase 2) ───────────────

/// Extract npm modules from package.json files.
///
/// Parses the root package.json (if present), consumes pnpm-workspace.yaml
/// (if present), expands workspace member patterns using glob, and collects
/// all resolved packages with their evidence.
///
/// Supports three configurations:
/// 1. Root package.json with workspaces array (npm/yarn workspaces)
/// 2. Root package.json + pnpm-workspace.yaml (pnpm with root package)
/// 3. pnpm-workspace.yaml only (pnpm virtual workspace, no root package)
fn extract_npm_modules(
	repo_path: &Path,
	package_json_files: &std::collections::HashMap<String, String>,
	pnpm_workspace_content: Option<&str>,
) -> NpmExtractionResult {
	let mut result = NpmExtractionResult::default();

	// Parse root package.json if it exists.
	let root_manifest_path = "package.json";
	let root_parsed = package_json_files
		.get(root_manifest_path)
		.and_then(|content| package_json::parse_package_json(content, root_manifest_path).ok());

	if root_parsed.is_some() {
		result.has_root_manifest = true;
	}

	// Parse pnpm-workspace.yaml if it exists.
	let pnpm_patterns = pnpm_workspace_content
		.and_then(|content| package_json::parse_pnpm_workspace(content).ok())
		.map(|parsed| {
			result.is_pnpm_workspace = true;
			parsed.workspace_patterns
		});

	// Early return if neither root package.json nor pnpm-workspace.yaml exists.
	if root_parsed.is_none() && pnpm_patterns.is_none() {
		return result;
	}

	// Add root package if it has a "name" field.
	if let Some(ref parsed) = root_parsed {
		if let Some(module) = &parsed.module {
			result.modules.push(ExtractedNpmModule {
				module: module.clone(),
				declared_pattern: None,
			});
		}
	}

	// Determine workspace patterns.
	// Priority: pnpm-workspace.yaml > package.json workspaces
	let workspace_patterns = pnpm_patterns
		.or_else(|| root_parsed.as_ref().map(|p| p.workspace_patterns.clone()))
		.unwrap_or_default();

	// Expand workspace patterns
	for pattern in &workspace_patterns {
		let expanded = expand_npm_workspace_pattern(repo_path, pattern, package_json_files);
		if expanded.is_empty() {
			result.unmatched_patterns.push(pattern.clone());
		} else {
			for member_module in expanded {
				result.modules.push(ExtractedNpmModule {
					module: member_module,
					declared_pattern: Some(pattern.clone()),
				});
			}
		}
	}

	result
}

/// Expand an npm workspace member pattern and return resolved modules.
///
/// Pattern examples:
/// - "packages/core" → direct path
/// - "packages/*" → glob all directories under packages/
/// - "apps/**" → recursive glob
/// - "!**/test/**" → negative pattern (skip matching directories)
///
/// For each match, checks if a package.json exists and parses it.
fn expand_npm_workspace_pattern(
	repo_path: &Path,
	pattern: &str,
	package_json_files: &std::collections::HashMap<String, String>,
) -> Vec<NpmModule> {
	let mut modules = Vec::new();

	// Skip negative patterns (pnpm/yarn feature)
	if pattern.starts_with('!') {
		return modules;
	}

	// Check if pattern contains glob characters
	if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
		// Glob expansion
		// Canonicalize repo_path to ensure consistent path comparisons.
		let canonical_repo = repo_path.canonicalize().unwrap_or_else(|_| repo_path.to_path_buf());
		let full_pattern = canonical_repo.join(pattern).join("package.json");
		let pattern_str = full_pattern.to_string_lossy();

		if let Ok(paths) = glob::glob(&pattern_str) {
			for entry in paths.flatten() {
				// Canonicalize the entry path for consistent strip_prefix.
				let canonical_entry = entry.canonicalize().unwrap_or(entry);
				// Convert back to repo-relative path
				if let Ok(rel) = canonical_entry.strip_prefix(&canonical_repo) {
					let rel_path = rel.to_string_lossy().replace('\\', "/");
					if let Some(content) = package_json_files.get(&rel_path) {
						if let Ok(parsed) = package_json::parse_package_json(content, &rel_path) {
							if let Some(mut module) = parsed.module {
								module.is_workspace_member = true;
								modules.push(module);
							}
						}
					}
				}
			}
		}
	} else {
		// Direct path (no glob)
		let manifest_path = format!("{}/package.json", pattern);
		if let Some(content) = package_json_files.get(&manifest_path) {
			if let Ok(parsed) = package_json::parse_package_json(content, &manifest_path) {
				if let Some(mut module) = parsed.module {
					module.is_workspace_member = true;
					modules.push(module);
				}
			}
		}
	}

	modules
}

// ── pyproject module extraction (rust-module-parity Phase 2c) ────────

/// Extract Python modules from pyproject.toml files.
///
/// Phase 2c: single-package only. No workspace/monorepo support yet.
fn extract_pyproject_modules(
	pyproject_toml_files: &std::collections::HashMap<String, String>,
) -> PyprojectExtractionResult {
	let mut result = PyprojectExtractionResult::default();

	// Check for root pyproject.toml
	let root_manifest_path = "pyproject.toml";
	let root_content = match pyproject_toml_files.get(root_manifest_path) {
		Some(content) => content,
		None => return result, // No pyproject.toml at root
	};

	result.has_root_manifest = true;

	// Parse root manifest
	let root_parsed = match pyproject::parse_pyproject_toml(root_content, root_manifest_path) {
		Ok(parsed) => parsed,
		Err(_) => return result, // Parse error, skip silently
	};

	// Add root package if it has a [project].name
	if let Some(module) = root_parsed.module {
		result.modules.push(ExtractedPyprojectModule { module });
	}

	result
}

// ── Gradle module extraction (rust-module-parity Phase 2b) ────────

/// Extract Gradle modules from settings.gradle files.
///
/// Parses settings.gradle (Groovy DSL) or settings.gradle.kts (Kotlin DSL)
/// for `include` statements and project renames. Extracts root project
/// and all declared subprojects.
fn extract_gradle_modules(
	settings_gradle_files: &std::collections::HashMap<String, String>,
) -> GradleExtractionResult {
	let mut result = GradleExtractionResult::default();

	// Check for root settings.gradle (prefer Groovy over Kotlin)
	let settings_path = if settings_gradle_files.contains_key("settings.gradle") {
		"settings.gradle"
	} else if settings_gradle_files.contains_key("settings.gradle.kts") {
		"settings.gradle.kts"
	} else {
		return result; // No settings.gradle at root
	};

	let settings_content = match settings_gradle_files.get(settings_path) {
		Some(content) => content,
		None => return result,
	};

	result.has_root_settings = true;

	// Parse settings file
	let parsed = settings_gradle::parse_settings_gradle(settings_content, settings_path);

	// Add root project
	if let Some(root_module) = parsed.root_project {
		result.modules.push(ExtractedGradleModule { module: root_module });
	}

	// Add subprojects
	for subproject in parsed.subprojects {
		result.modules.push(ExtractedGradleModule { module: subproject });
	}

	result
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
	let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
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
			indexed_at: now.clone(),
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

// ── Config file version persistence ──────────────────────────────

/// Persist config file versions for refresh tracking.
///
/// Config files are NOT extracted (no FILE nodes, no symbols), but their
/// file_versions are tracked so refresh can detect changes and trigger
/// scope-widening invalidation.
fn persist_config_file_versions(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	config_files: &[ConfigFileInput],
) -> Result<(), ComposeError> {
	if config_files.is_empty() {
		return Ok(());
	}

	// TrackedFile records — config files are tracked but have no language.
	let tracked: Vec<repo_graph_storage::types::TrackedFile> = config_files
		.iter()
		.map(|f| repo_graph_storage::types::TrackedFile {
			file_uid: format!("{}:{}", repo_uid, f.rel_path),
			repo_uid: repo_uid.into(),
			path: f.rel_path.clone(),
			language: None, // Config files have no language
			is_test: false,
			is_generated: false,
			is_excluded: false,
		})
		.collect();
	storage.upsert_files(&tracked).map_err(ComposeError::Storage)?;

	// FileVersion records — parse_status = "config" indicates tracked-only.
	let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
	let versions: Vec<repo_graph_storage::types::FileVersion> = config_files
		.iter()
		.map(|f| repo_graph_storage::types::FileVersion {
			snapshot_uid: snapshot_uid.into(),
			file_uid: format!("{}:{}", repo_uid, f.rel_path),
			content_hash: f.content_hash.clone(),
			ast_hash: None,
			extractor: Some("config:invalidation_tracking".into()),
			parse_status: "config".into(), // Not parsed, just tracked
			size_bytes: None,
			line_count: Some(f.line_count as i64),
			indexed_at: now.clone(),
		})
		.collect();
	storage
		.upsert_file_versions(&versions)
		.map_err(ComposeError::Storage)?;

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
///
/// # Arguments
/// * `changed_file_paths` - If Some, only process nodes from these files (refresh mode).
///   If None, process all nodes and replace all Spring inferences (full index mode).
///
/// In refresh mode, inferences for unchanged files are preserved via copy-forward.
/// This respects the ACR-4 impact propagation model: copy-forwarded inferences
/// can be marked impacted if their provenance references changed L0 keys.
fn persist_spring_liveness_inferences(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	changed_file_paths: Option<&[&str]>,
) -> Result<(), ComposeError> {
	// Query all nodes for the snapshot
	let nodes = storage
		.query_all_nodes(snapshot_uid)
		.map_err(ComposeError::Storage)?;

	// Filter nodes based on changed files (if in refresh mode)
	let nodes_to_process: Vec<_> = match changed_file_paths {
		Some(changed_paths) => {
			// Refresh mode: only process nodes from changed files
			nodes
				.into_iter()
				.filter(|n| {
					changed_paths.iter().any(|path| {
						// Match SYMBOL nodes by "repo:path#" prefix
						let symbol_prefix = format!("{}:{}#", repo_uid, path);
						n.stable_key.starts_with(&symbol_prefix)
					})
				})
				.collect()
		}
		None => {
			// Full index mode: process all nodes
			nodes
		}
	};

	// Project to SpringNodeInput — only SYMBOL nodes with metadata
	let inputs: Vec<SpringNodeInput> = nodes_to_process
		.iter()
		.filter(|n| n.kind == "SYMBOL" && n.metadata_json.is_some())
		.map(|n| SpringNodeInput {
			stable_key: n.stable_key.clone(),
			kind: n.kind.clone(),
			subtype: n.subtype.clone(),
			metadata_json: n.metadata_json.clone(),
		})
		.collect();

	if inputs.is_empty() && changed_file_paths.is_some() {
		// Refresh mode with no Spring symbols in changed files — nothing to do.
		// Copy-forwarded inferences from unchanged files are preserved.
		return Ok(());
	}

	// Run classifier
	let classified = classify_spring_liveness(&inputs);

	// Convert to InferenceInput with provenance (ACR-4)
	// Use real ISO timestamp and version consistent with Rust indexer
	let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
	let extractor = "indexer:1.0.0"; // Match INDEXER_VERSION in orchestrator.rs

	let inferences: Vec<InferenceInput> = classified
		.iter()
		.enumerate()
		.map(|(i, inf)| {
			// Construct canonical provenance for impact propagation.
			// The inference depends on the Node it classifies (the target).
			// When that node changes, this inference should be marked impacted.
			let provenance = artifact_contracts::Provenance::from_layer0_items(vec![
				artifact_contracts::ProvenanceAnchor::new("Nodes", &inf.target_stable_key),
			]).with_extractor(extractor);
			let provenance_json = serde_json::to_string(&provenance).ok();

			InferenceInput {
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
				provenance_json,
			}
		})
		.collect();

	match changed_file_paths {
		Some(changed_paths) => {
			// Refresh mode: delete only inferences for changed files, then insert new ones.
			// This preserves copy-forwarded inferences for unchanged files.
			if !changed_paths.is_empty() {
				// Delete Spring inferences whose target_stable_key matches changed files
				storage
					.delete_inferences_by_kind_and_files(
						snapshot_uid,
						repo_uid,
						"spring_container_managed",
						changed_paths,
					)
					.map_err(ComposeError::Storage)?;
			}
			// Insert new inferences for changed files
			if !inferences.is_empty() {
				storage
					.insert_inferences(&inferences)
					.map_err(ComposeError::Storage)?;
			}
		}
		None => {
			// Full index mode: replace all Spring inferences
			if inferences.is_empty() {
				// No Spring symbols — delete any existing Spring inferences
				storage
					.delete_inferences_by_kind(snapshot_uid, &["spring_container_managed"])
					.map_err(ComposeError::Storage)?;
			} else {
				storage
					.replace_inferences_by_kind(snapshot_uid, &["spring_container_managed"], &inferences)
					.map_err(ComposeError::Storage)?;
			}
		}
	}

	Ok(())
}

// ── Post-index policy-facts extraction ───────────────────────────

/// Extract and persist policy facts from C files.
///
/// TEMPORARY postpass: Re-parses C files after extraction to
/// extract policy facts. This duplicates the tree-sitter parsing
/// work already done by the C extractor.
///
/// **TECH DEBT:** This re-parse approach is explicitly temporary.
/// The target architecture is extraction-time integration where
/// the C extractor carries policy-fact output directly. See
/// `docs/TECH-DEBT.md` entry "PF-1 temporary re-parse postpass".
///
/// Returns the total number of policy facts persisted
/// (STATUS_MAPPING + BEHAVIORAL_MARKER + RETURN_FATE).
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
	let mut all_fates = Vec::new();

	for file in file_inputs {
		// Policy-facts scope: C files only (.c and .h).
		// C++ (.cpp, .hpp, .cc, .cxx) is explicitly out of scope.
		// See docs/slices/pf-1-status-mapping.md "What PF-1 Does NOT Include".
		// See docs/slices/pf-2-behavioral-marker.md "Non-Goals".
		// See docs/slices/pf-3-return-fate.md "Non-Goals".
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

		// PF-3: Extract RETURN_FATE facts.
		let fates = extract_return_fates(
			&tree,
			file.content.as_bytes(),
			&file.rel_path,
			repo_uid,
		);
		all_fates.extend(fates);
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

	// Persist RETURN_FATE facts.
	if !all_fates.is_empty() {
		let count = storage
			.insert_return_fates(snapshot_uid, &all_fates)
			.map_err(|e| ComposeError::Index(format!("policy-facts storage: {}", e)))?;
		total_count += count;
	}

	Ok(total_count)
}

/// BI-1A: Extract and persist boundary interaction facts from C files.
///
/// TEMPORARY postpass: Re-parses C files after extraction to detect
/// IPC boundary calls. This duplicates the tree-sitter parsing work
/// already done by the C extractor.
///
/// **TECH DEBT:** Same architecture as PF-1 postpass. Target is
/// extraction-time integration. See `docs/TECH-DEBT.md` entry
/// "Boundary Interaction Extraction — Slice 1A".
///
/// Returns the number of surfaces persisted.
fn persist_boundary_interactions(
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
		.map_err(|e| ComposeError::ExtractorInit(format!("boundary-interaction C parser: {}", e)))?;

	// Create emitter context.
	let context = EmitterContext {
		snapshot_uid: snapshot_uid.to_string(),
		repo_uid: repo_uid.to_string(),
		extractor: "c-ipc:0.1.0".to_string(),
	};
	let mut emitter = BoundaryInteractionEmitter::new(context);

	for file in file_inputs {
		// Boundary interaction scope: C files only (.c and .h).
		let is_c_file = file.rel_path.ends_with(".c") || file.rel_path.ends_with(".h");
		if !is_c_file {
			continue;
		}

		// Parse the file.
		let tree = match parser.parse(&file.content, None) {
			Some(t) => t,
			None => continue, // Parse failed, skip.
		};

		// Extract raw boundary calls.
		let raw_calls = extract_boundary_calls(
			&tree.root_node(),
			file.content.as_bytes(),
			&file.rel_path,
		);

		// Convert to BoundaryCallsite and emit.
		for raw in raw_calls {
			let callsite = convert_raw_to_callsite(&raw, &file.rel_path, repo_uid);
			// try_emit returns:
			//   Ok(Some(_)) - surface emitted
			//   Ok(None) - no binding matched OR guard predicate rejected
			//   Err(_) - emission logic error (bug in code)
			if let Err(e) = emitter.try_emit(&callsite) {
				return Err(ComposeError::Index(format!(
					"boundary-interaction emitter failed at {}:{}: {}",
					file.rel_path, raw.location.line_start, e
				)));
			}
		}
	}

	// Collect emitted facts.
	let surfaces: Vec<_> = emitter.surfaces().cloned().collect();
	let channels: Vec<_> = emitter.channels().cloned().collect();

	if surfaces.is_empty() {
		return Ok(0);
	}

	// Persist to storage using combined function that handles UID mapping.
	// The emitter uses deterministic UIDs for deduplication, but storage
	// needs fresh UUIDs to allow same logical surface in multiple snapshots.
	let (surface_count, _channel_count) = storage
		.insert_boundary_surfaces_and_channels(&surfaces, &channels)
		.map_err(|e| ComposeError::Index(format!("boundary-interaction storage: {}", e)))?;

	Ok(surface_count)
}

/// Convert a raw boundary call to a BoundaryCallsite for the emitter.
fn convert_raw_to_callsite(raw: &RawBoundaryCall, file_path: &str, repo_uid: &str) -> BoundaryCallsite {
	// Build enclosing symbol stable key.
	let enclosing_symbol_key = if raw.enclosing_function.is_empty() {
		format!("{}:{}:FILE", repo_uid, file_path)
	} else {
		format!(
			"{}:{}#{}:SYMBOL:FUNCTION",
			repo_uid, file_path, raw.enclosing_function
		)
	};

	BoundaryCallsite {
		language: BiLanguage::C,
		function_name: raw.function_name.clone(),
		location: raw.location.clone(),
		source_file: file_path.to_string(),
		enclosing_symbol_key,
		extracted_argument: raw.extracted_argument.clone(),
		argument_index: raw.argument_index,
		raw_argument_text: None,
		// No api_family filter — C IPC calls don't conflict with message broker patterns
		api_family: None,
		socket_family: raw.socket_family.map(convert_socket_family),
		socket_type: raw.socket_type.map(convert_socket_type),
		mmap_flags: raw.mmap_flags.map(convert_mmap_flags),
		mknod_mode: raw.mknod_mode,
	}
}

fn convert_socket_family(raw: RawSocketFamily) -> SocketFamily {
	match raw {
		RawSocketFamily::Unix => SocketFamily::Unix,
		RawSocketFamily::Inet => SocketFamily::Inet,
		RawSocketFamily::Inet6 => SocketFamily::Inet6,
		RawSocketFamily::Can => SocketFamily::Can,
	}
}

fn convert_socket_type(raw: RawSocketType) -> SocketType {
	match raw {
		RawSocketType::Stream => SocketType::Stream,
		RawSocketType::Datagram => SocketType::Datagram,
		RawSocketType::Raw => SocketType::Raw,
		RawSocketType::SeqPacket => SocketType::SeqPacket,
	}
}

fn convert_mmap_flags(raw: RawMmapFlags) -> MmapFlags {
	match raw {
		RawMmapFlags::Shared => MmapFlags::Shared,
		RawMmapFlags::Private => MmapFlags::Private,
	}
}

/// BI-1C: Extract and persist boundary interaction facts from TS/JS files.
///
/// Detects SharedArrayBuffer, Worker, postMessage, and Atomics patterns.
///
/// TEMPORARY postpass: Re-parses TS/JS files after extraction to detect
/// worker boundary calls. Target is extraction-time integration.
///
/// Returns the number of surfaces persisted.
fn persist_ts_boundary_interactions(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	file_inputs: &[FileInput],
) -> Result<usize, ComposeError> {
	// Initialize tree-sitter parser for TS/JS.
	let mut parser = tree_sitter::Parser::new();
	let ts_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
	let tsx_language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();

	// Create emitter context.
	let context = EmitterContext {
		snapshot_uid: snapshot_uid.to_string(),
		repo_uid: repo_uid.to_string(),
		extractor: "ts-worker:0.1.0".to_string(),
	};
	let mut emitter = BoundaryInteractionEmitter::new(context);

	for file in file_inputs {
		// Boundary interaction scope: TS/JS files only.
		let is_ts = file.rel_path.ends_with(".ts")
			|| file.rel_path.ends_with(".tsx")
			|| file.rel_path.ends_with(".js")
			|| file.rel_path.ends_with(".jsx");
		if !is_ts {
			continue;
		}

		// Select grammar based on extension.
		let language = if file.rel_path.ends_with(".tsx") || file.rel_path.ends_with(".jsx") {
			&tsx_language
		} else {
			&ts_language
		};

		parser
			.set_language(language)
			.map_err(|e| ComposeError::ExtractorInit(format!("boundary-interaction TS parser: {}", e)))?;

		// Parse the file.
		let tree = match parser.parse(&file.content, None) {
			Some(t) => t,
			None => continue, // Parse failed, skip.
		};

		// Extract SharedArrayBuffer/Atomics boundary calls (BI-1C).
		let raw_calls = extract_ts_boundary_calls(
			&tree.root_node(),
			file.content.as_bytes(),
			&file.rel_path,
		);

		// Convert to BoundaryCallsite and emit.
		for raw in raw_calls {
			let callsite = convert_ts_raw_to_callsite(&raw, &file.rel_path, repo_uid);
			// try_emit returns:
			//   Ok(Some(_)) - surface emitted
			//   Ok(None) - no binding matched
			//   Err(_) - emission logic error
			if let Err(e) = emitter.try_emit(&callsite) {
				return Err(ComposeError::Index(format!(
					"boundary-interaction emitter failed at {}:{}: {}",
					file.rel_path, raw.location.line_start, e
				)));
			}
		}

		// Extract AMQP/RabbitMQ boundary calls (MB-1A).
		let amqp_calls = extract_amqp_boundary_calls(
			&tree.root_node(),
			file.content.as_bytes(),
			&file.rel_path,
		);

		for raw in amqp_calls {
			let callsite = convert_amqp_raw_to_callsite(&raw, &file.rel_path, repo_uid);
			if let Err(e) = emitter.try_emit(&callsite) {
				return Err(ComposeError::Index(format!(
					"boundary-interaction emitter failed at {}:{}: {}",
					file.rel_path, raw.location.line_start, e
				)));
			}
		}

		// Extract Kafka boundary calls (MB-2A).
		let kafka_calls = extract_kafka_boundary_calls(
			&tree.root_node(),
			file.content.as_bytes(),
			&file.rel_path,
		);

		for raw in kafka_calls {
			let callsite = convert_kafka_raw_to_callsite(&raw, &file.rel_path, repo_uid);
			if let Err(e) = emitter.try_emit(&callsite) {
				return Err(ComposeError::Index(format!(
					"boundary-interaction emitter failed at {}:{}: {}",
					file.rel_path, raw.location.line_start, e
				)));
			}
		}

		// Extract NATS boundary calls (MB-3A).
		let nats_calls = extract_nats_boundary_calls(
			&tree.root_node(),
			file.content.as_bytes(),
			&file.rel_path,
		);

		for raw in nats_calls {
			let callsite = convert_nats_raw_to_callsite(&raw, &file.rel_path, repo_uid);
			if let Err(e) = emitter.try_emit(&callsite) {
				return Err(ComposeError::Index(format!(
					"boundary-interaction emitter failed at {}:{}: {}",
					file.rel_path, raw.location.line_start, e
				)));
			}
		}
	}

	// Collect emitted facts.
	let surfaces: Vec<_> = emitter.surfaces().cloned().collect();
	let channels: Vec<_> = emitter.channels().cloned().collect();

	if surfaces.is_empty() {
		return Ok(0);
	}

	// Persist to storage using combined function that handles UID mapping.
	let (surface_count, _channel_count) = storage
		.insert_boundary_surfaces_and_channels(&surfaces, &channels)
		.map_err(|e| ComposeError::Index(format!("boundary-interaction storage: {}", e)))?;

	Ok(surface_count)
}

/// Convert a raw TS boundary call to a BoundaryCallsite for the emitter.
fn convert_ts_raw_to_callsite(raw: &RawTsBoundaryCall, file_path: &str, repo_uid: &str) -> BoundaryCallsite {
	// Build enclosing symbol stable key.
	let enclosing_symbol_key = if raw.enclosing_function.is_empty() {
		format!("{}:{}:FILE", repo_uid, file_path)
	} else {
		format!(
			"{}:{}#{}:SYMBOL:FUNCTION",
			repo_uid, file_path, raw.enclosing_function
		)
	};

	BoundaryCallsite {
		language: BiLanguage::TypeScript,
		function_name: raw.function_name.clone(),
		location: raw.location.clone(),
		source_file: file_path.to_string(),
		enclosing_symbol_key,
		extracted_argument: raw.extracted_argument.clone(),
		argument_index: None,
		raw_argument_text: None,
		// No api_family filter — TS SAB/Worker detection uses unique function names
		api_family: None,
		// TS SAB/Worker detection doesn't use socket/mmap semantics
		socket_family: None,
		socket_type: None,
		mmap_flags: None,
		mknod_mode: None,
	}
}

/// Convert a raw AMQP boundary call to a BoundaryCallsite for the emitter.
fn convert_amqp_raw_to_callsite(raw: &RawAmqpBoundaryCall, file_path: &str, repo_uid: &str) -> BoundaryCallsite {
	// Build enclosing symbol stable key.
	let enclosing_symbol_key = if raw.enclosing_function.is_empty() {
		format!("{}:{}:FILE", repo_uid, file_path)
	} else {
		format!(
			"{}:{}#{}:SYMBOL:FUNCTION",
			repo_uid, file_path, raw.enclosing_function
		)
	};

	// Build extracted argument from queue/exchange/routing-key.
	// Priority: queue_name > exchange_name (for the main channel identity).
	let extracted_argument = raw.queue_name.clone().or_else(|| raw.exchange_name.clone());

	BoundaryCallsite {
		language: BiLanguage::TypeScript,
		function_name: raw.function_name.clone(),
		location: raw.location.clone(),
		source_file: file_path.to_string(),
		enclosing_symbol_key,
		extracted_argument,
		argument_index: Some(0), // First argument is typically queue/exchange
		raw_argument_text: None,
		// Filter to AMQP bindings only — prevents matching Kafka/NATS functions
		api_family: Some("amqplib".to_string()),
		// AMQP doesn't use socket/mmap semantics
		socket_family: None,
		socket_type: None,
		mmap_flags: None,
		mknod_mode: None,
	}
}

/// Convert a raw Kafka boundary call to a BoundaryCallsite for the emitter.
fn convert_kafka_raw_to_callsite(raw: &RawKafkaBoundaryCall, file_path: &str, repo_uid: &str) -> BoundaryCallsite {
	// Build enclosing symbol stable key.
	let enclosing_symbol_key = if raw.enclosing_function.is_empty() {
		format!("{}:{}:FILE", repo_uid, file_path)
	} else {
		format!(
			"{}:{}#{}:SYMBOL:FUNCTION",
			repo_uid, file_path, raw.enclosing_function
		)
	};

	// Build extracted argument from topic or topics[0].
	// Priority: topic > topics[0] (for the main channel identity).
	let extracted_argument = raw.topic.clone().or_else(|| {
		raw.topics.as_ref().and_then(|t| t.first().cloned())
	});

	BoundaryCallsite {
		language: BiLanguage::TypeScript,
		function_name: raw.function_name.clone(),
		location: raw.location.clone(),
		source_file: file_path.to_string(),
		enclosing_symbol_key,
		extracted_argument,
		argument_index: Some(0), // First argument is typically the options object with topic
		raw_argument_text: None,
		// Filter to Kafka bindings only — prevents matching AMQP/NATS functions
		api_family: Some("kafkajs".to_string()),
		// Kafka doesn't use socket/mmap semantics
		socket_family: None,
		socket_type: None,
		mmap_flags: None,
		mknod_mode: None,
	}
}

/// Convert a raw NATS boundary call to a BoundaryCallsite for the emitter.
fn convert_nats_raw_to_callsite(raw: &RawNatsBoundaryCall, file_path: &str, repo_uid: &str) -> BoundaryCallsite {
	// Build enclosing symbol stable key.
	let enclosing_symbol_key = if raw.enclosing_function.is_empty() {
		format!("{}:{}:FILE", repo_uid, file_path)
	} else {
		format!(
			"{}:{}#{}:SYMBOL:FUNCTION",
			repo_uid, file_path, raw.enclosing_function
		)
	};

	BoundaryCallsite {
		language: BiLanguage::TypeScript,
		function_name: raw.function_name.clone(),
		location: raw.location.clone(),
		source_file: file_path.to_string(),
		enclosing_symbol_key,
		extracted_argument: raw.subject.clone(),
		argument_index: Some(0), // First argument is subject
		raw_argument_text: None,
		// Filter to NATS bindings only — prevents matching AMQP/Kafka "publish"
		api_family: Some("nats".to_string()),
		// NATS doesn't use socket/mmap semantics
		socket_family: None,
		socket_type: None,
		mmap_flags: None,
		mknod_mode: None,
	}
}

// ── Cargo module persistence (rust-module-parity Phase 1) ────────

/// Persist Cargo.toml-derived module candidates, evidence, and file ownership.
///
/// Phase 1.5: persists module rows and ownership assignments.
/// - Root crate/package
/// - Resolved workspace members with real Cargo.toml
/// - File ownership via longest-prefix-match
fn persist_cargo_modules(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	cargo_extraction: &CargoExtractionResult,
	file_inputs: &[FileInput],
) -> Result<usize, ComposeError> {
	if cargo_extraction.modules.is_empty() {
		return Ok(0);
	}

	// Convert extracted modules to storage input DTOs.
	let mut candidates: Vec<CargoModuleCandidateInput> = Vec::new();
	let mut evidence: Vec<CargoModuleEvidenceInput> = Vec::new();

	for extracted in &cargo_extraction.modules {
		let (candidate, ev) = cargo_manifest::to_storage_inputs(
			&extracted.module,
			repo_uid,
			snapshot_uid,
		);
		candidates.push(candidate);
		evidence.push(ev);
	}

	// Persist using the storage port.
	let candidate_count = storage
		.insert_cargo_module_candidates(&candidates)
		.map_err(ComposeError::Storage)?;
	let _evidence_count = storage
		.insert_cargo_module_evidence(&evidence)
		.map_err(ComposeError::Storage)?;

	// Phase 1.5: Compute and persist file ownership.
	let ownership = compute_cargo_file_ownership(
		repo_uid,
		snapshot_uid,
		&candidates,
		file_inputs,
	);

	if !ownership.is_empty() {
		storage
			.insert_file_ownership(&ownership)
			.map_err(ComposeError::Storage)?;
	}

	Ok(candidate_count)
}

/// Compute file ownership assignments using longest-prefix-match.
///
/// Each file is assigned to the module candidate whose `canonical_root_path`
/// is the longest prefix of the file's relative path. Files that don't match
/// any module prefix are not assigned (no ownership row).
///
/// Algorithm:
/// 1. Sort modules by canonical_root_path length descending (longest first)
/// 2. For each file, find the first module whose path is a prefix
/// 3. Create ownership record with assignment_kind = "manifest_prefix"
fn compute_cargo_file_ownership(
	repo_uid: &str,
	snapshot_uid: &str,
	candidates: &[CargoModuleCandidateInput],
	file_inputs: &[FileInput],
) -> Vec<FileOwnershipInput> {
	if candidates.is_empty() || file_inputs.is_empty() {
		return Vec::new();
	}

	// Build sorted list of (canonical_root_path, module_candidate_uid) by path length descending.
	// This ensures longest-prefix-match when iterating.
	let mut sorted_modules: Vec<(&str, &str)> = candidates
		.iter()
		.map(|c| (c.canonical_root_path.as_str(), c.module_candidate_uid.as_str()))
		.collect();
	sorted_modules.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

	let mut ownership = Vec::new();

	for file in file_inputs {
		// Find longest matching module prefix.
		let matched = sorted_modules.iter().find(|(root_path, _)| {
			if *root_path == "." {
				// Root crate matches all files
				true
			} else {
				// Check if file path starts with module root path.
				// Must match as a directory boundary: "crates/foo" matches "crates/foo/src/lib.rs"
				// but not "crates/foobar/src/lib.rs".
				file.rel_path == *root_path
					|| file.rel_path.starts_with(&format!("{}/", root_path))
			}
		});

		if let Some((_, module_uid)) = matched {
			ownership.push(FileOwnershipInput {
				snapshot_uid: snapshot_uid.to_string(),
				repo_uid: repo_uid.to_string(),
				file_uid: format!("{}:{}", repo_uid, file.rel_path),
				module_candidate_uid: module_uid.to_string(),
				assignment_kind: "manifest_prefix".to_string(),
				confidence: 1.0,
				basis_json: None,
			});
		}
	}

	ownership
}

// ── npm module persistence (rust-module-parity Phase 2) ──────────

/// Persist package.json-derived module candidates, evidence, and file ownership.
///
/// Phase 2: persists module rows and ownership assignments for npm packages.
/// - Root package
/// - Resolved workspace members with real package.json
/// - File ownership via longest-prefix-match
fn persist_npm_modules(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	npm_extraction: &NpmExtractionResult,
	file_inputs: &[FileInput],
) -> Result<usize, ComposeError> {
	if npm_extraction.modules.is_empty() {
		return Ok(0);
	}

	// Convert extracted modules to storage input DTOs.
	// Reuses the same input types as Cargo (they're generic).
	let mut candidates: Vec<CargoModuleCandidateInput> = Vec::new();
	let mut evidence: Vec<CargoModuleEvidenceInput> = Vec::new();

	for extracted in &npm_extraction.modules {
		let (candidate, ev) = package_json::to_storage_inputs(
			&extracted.module,
			repo_uid,
			snapshot_uid,
		);
		candidates.push(candidate);
		evidence.push(ev);
	}

	// Persist using the storage port (same methods as Cargo).
	let candidate_count = storage
		.insert_cargo_module_candidates(&candidates)
		.map_err(ComposeError::Storage)?;
	let _evidence_count = storage
		.insert_cargo_module_evidence(&evidence)
		.map_err(ComposeError::Storage)?;

	// Compute and persist file ownership using same algorithm as Cargo.
	// Only for JS/TS files — other languages should not be assigned to npm modules.
	let js_ts_files: Vec<_> = file_inputs
		.iter()
		.filter(|f| {
			let lang = routing::detect_language(&f.rel_path);
			matches!(lang, Some("typescript" | "tsx" | "javascript" | "jsx"))
		})
		.cloned()
		.collect();

	let ownership = compute_cargo_file_ownership(
		repo_uid,
		snapshot_uid,
		&candidates,
		&js_ts_files,
	);

	if !ownership.is_empty() {
		storage
			.insert_file_ownership(&ownership)
			.map_err(ComposeError::Storage)?;
	}

	Ok(candidate_count)
}

// ── pyproject module persistence (rust-module-parity Phase 2c) ───

/// Persist pyproject.toml-derived module candidates, evidence, and file ownership.
///
/// Phase 2c: persists module rows and ownership assignments for Python packages.
/// - Root package only (single-package, no workspace support yet)
/// - File ownership via longest-prefix-match (.py files only)
fn persist_pyproject_modules(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	pyproject_extraction: &PyprojectExtractionResult,
	file_inputs: &[FileInput],
) -> Result<usize, ComposeError> {
	if pyproject_extraction.modules.is_empty() {
		return Ok(0);
	}

	// Convert extracted modules to storage input DTOs.
	let mut candidates: Vec<CargoModuleCandidateInput> = Vec::new();
	let mut evidence: Vec<CargoModuleEvidenceInput> = Vec::new();

	for extracted in &pyproject_extraction.modules {
		let (candidate, ev) = pyproject::to_storage_inputs(
			&extracted.module,
			repo_uid,
			snapshot_uid,
		);
		candidates.push(candidate);
		evidence.push(ev);
	}

	// Persist using the storage port (same methods as Cargo/npm).
	let candidate_count = storage
		.insert_cargo_module_candidates(&candidates)
		.map_err(ComposeError::Storage)?;
	let _evidence_count = storage
		.insert_cargo_module_evidence(&evidence)
		.map_err(ComposeError::Storage)?;

	// Compute and persist file ownership.
	// Only for Python files — other languages should not be assigned to pyproject modules.
	let python_files: Vec<_> = file_inputs
		.iter()
		.filter(|f| {
			let lang = routing::detect_language(&f.rel_path);
			matches!(lang, Some("python"))
		})
		.cloned()
		.collect();

	let ownership = compute_cargo_file_ownership(
		repo_uid,
		snapshot_uid,
		&candidates,
		&python_files,
	);

	if !ownership.is_empty() {
		storage
			.insert_file_ownership(&ownership)
			.map_err(ComposeError::Storage)?;
	}

	Ok(candidate_count)
}

// ── Gradle module persistence (rust-module-parity Phase 2b) ───

/// Persist settings.gradle-derived module candidates, evidence, and file ownership.
///
/// Phase 2b: persists module rows and ownership assignments for Gradle projects.
/// - Root project and subprojects from settings.gradle
/// - File ownership via longest-prefix-match (.java, .kt, .scala files only)
fn persist_gradle_modules(
	storage: &mut StorageConnection,
	repo_uid: &str,
	snapshot_uid: &str,
	gradle_extraction: &GradleExtractionResult,
	file_inputs: &[FileInput],
) -> Result<usize, ComposeError> {
	if gradle_extraction.modules.is_empty() {
		return Ok(0);
	}

	// Convert extracted modules to storage input DTOs.
	let mut candidates: Vec<CargoModuleCandidateInput> = Vec::new();
	let mut evidence: Vec<CargoModuleEvidenceInput> = Vec::new();

	for extracted in &gradle_extraction.modules {
		let (candidate, ev) = settings_gradle::to_storage_inputs(
			&extracted.module,
			repo_uid,
			snapshot_uid,
		);
		candidates.push(candidate);
		evidence.push(ev);
	}

	// Persist using the storage port (same methods as Cargo/npm/pyproject).
	let candidate_count = storage
		.insert_cargo_module_candidates(&candidates)
		.map_err(ComposeError::Storage)?;
	let _evidence_count = storage
		.insert_cargo_module_evidence(&evidence)
		.map_err(ComposeError::Storage)?;

	// Compute and persist file ownership.
	// Only for JVM files — .java, .kt, .scala
	let jvm_files: Vec<_> = file_inputs
		.iter()
		.filter(|f| {
			let lang = routing::detect_language(&f.rel_path);
			matches!(lang, Some("java" | "kotlin" | "scala"))
		})
		.cloned()
		.collect();

	let ownership = compute_cargo_file_ownership(
		repo_uid,
		snapshot_uid,
		&candidates,
		&jvm_files,
	);

	if !ownership.is_empty() {
		storage
			.insert_file_ownership(&ownership)
			.map_err(ComposeError::Storage)?;
	}

	Ok(candidate_count)
}

// ── Full index ───────────────────────────────────────────────────

/// Index a repo from disk into an existing StorageConnection.
///
/// If `progress` is provided, it will be called with phase-level progress events:
/// - "scanning" (1 step)
/// - "extracting" (1 step)
/// - "persisting" (4 steps)
pub fn index_into_storage(
	repo_path: &Path,
	storage: &mut StorageConnection,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	index_into_storage_with_progress(repo_path, storage, repo_uid, options, None)
}

/// Index a repo from disk into an existing StorageConnection with progress reporting.
pub fn index_into_storage_with_progress(
	repo_path: &Path,
	storage: &mut StorageConnection,
	repo_uid: &str,
	options: &ComposeOptions,
	mut progress: Option<ProgressCallback<'_>>,
) -> Result<IndexResult, ComposeError> {
	emit_progress(&mut progress, "scanning", 0, 1)?;
	let prepared = prepare_repo_inputs(repo_path)?;
	emit_progress(&mut progress, "scanning", 1, 1)?;

	let mut ts_extractor = TsExtractor::new();
	ts_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("ts: {}", e)))?;

	let mut c_extractor = CExtractor::new();
	c_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("c: {}", e)))?;

	let mut cpp_extractor = CppExtractor::new();
	cpp_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("cpp: {}", e)))?;

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

	// Checkpoint BEFORE repo mutation — abort here if transport failed
	emit_progress(&mut progress, "initializing", 0, 1)?;
	ensure_repo(storage, repo_uid, repo_path, options)?;

	let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ts_extractor, &mut c_extractor, &mut cpp_extractor, &mut java_extractor, &mut python_extractor, &mut rust_extractor];

	// Bridge the compose progress callback to the indexer callback.
	// The indexer emits per-file extracting progress with abort checkpoints.
	let mut indexer_progress_callback = |event: &IndexProgressEvent| -> ControlFlow<()> {
		if let Some(ref mut cb) = progress {
			let phase = match event.phase {
				IndexPhase::Scanning => "scanning",
				IndexPhase::Extracting => "extracting",
				IndexPhase::Resolving => "resolving",
				IndexPhase::Persisting => "persisting",
			};
			cb(&ProgressEvent::new(phase, event.current, event.total))
		} else {
			ControlFlow::Continue(())
		}
	};

	let mut idx_options = IndexOptions {
		basis_commit: options.basis_commit.clone(),
		edge_batch_size: options.edge_batch_size,
		c_include_roots: options.c_include_roots.clone(),
		on_progress: Some(&mut indexer_progress_callback),
		..IndexOptions::default()
	};

	// State-boundary hook: wired at the composition root (SB-4-pre).
	// Constructs the hook; on invalid repo_uid it degrades
	// gracefully (diagnostic, no emission, no abort).
	let mut sb_hook = crate::state_boundary_hook::StateBoundaryHook::new(repo_uid);

	// The indexer now emits per-file progress with abort checkpoints.
	// IndexError::Aborted maps to ComposeError::Aborted for transport failure.
	let mut result = match orchestrator::index_repo(
		storage,
		&mut extractors,
		repo_uid,
		&prepared.file_inputs,
		&prepared.contract_file_inputs,
		&mut idx_options,
		Some(&mut sb_hook),
	) {
		Ok(r) => r,
		Err(IndexError::Aborted) => return Err(ComposeError::Aborted),
		Err(e) => return Err(ComposeError::Index(format!("{}", e))),
	};

	// Persisting phase: checkpoint BEFORE each mutation (7 mutations total)
	// Semantics: current=N means "about to do mutation N"

	emit_progress(&mut progress, "persisting", 0, 8)?;  // about to persist read failures
	persist_read_failures(
		storage,
		repo_uid,
		&result.snapshot_uid.clone(),
		&prepared.read_failed_paths,
		&mut result,
	)?;

	emit_progress(&mut progress, "persisting", 1, 8)?;  // about to persist config file versions
	// Persist config file versions for refresh invalidation tracking.
	// Config files are NOT extracted — only tracked for hash comparison.
	persist_config_file_versions(
		storage,
		repo_uid,
		&result.snapshot_uid,
		&prepared.config_file_inputs,
	)?;

	emit_progress(&mut progress, "persisting", 2, 8)?;  // about to persist metrics
	// RS-MS-3c-prereq: Persist metrics (complexity, params, nesting).
	persist_metrics(storage, repo_uid, &result.snapshot_uid, &result.metrics)?;

	emit_progress(&mut progress, "persisting", 3, 8)?;  // about to persist spring liveness
	// Persist Spring framework-liveness inferences for dead-code suppression.
	// Full index mode: process all nodes, replace all Spring inferences.
	persist_spring_liveness_inferences(storage, repo_uid, &result.snapshot_uid, None)?;

	emit_progress(&mut progress, "persisting", 4, 8)?;  // about to persist policy facts
	// PF-1: Extract and persist STATUS_MAPPING policy facts from C files.
	// TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
	persist_policy_facts(storage, repo_uid, &result.snapshot_uid, &prepared.file_inputs)?;

	emit_progress(&mut progress, "persisting", 5, 8)?;  // about to persist C boundary interactions
	// BI-1A: Extract and persist boundary interaction facts from C files.
	// TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
	persist_boundary_interactions(storage, repo_uid, &result.snapshot_uid, &prepared.file_inputs)?;

	emit_progress(&mut progress, "persisting", 6, 8)?;  // about to persist TS boundary interactions
	// BI-1C: Extract and persist boundary interaction facts from TS/JS files.
	// SharedArrayBuffer, Worker, postMessage, Atomics patterns.
	persist_ts_boundary_interactions(storage, repo_uid, &result.snapshot_uid, &prepared.file_inputs)?;

	emit_progress(&mut progress, "persisting", 7, 9)?;  // about to persist Cargo modules
	// rust-module-parity Phase 1.5: Persist Cargo.toml-derived module candidates and file ownership.
	persist_cargo_modules(storage, repo_uid, &result.snapshot_uid, &prepared.cargo_modules, &prepared.file_inputs)?;

	emit_progress(&mut progress, "persisting", 8, 10)?;  // about to persist npm modules
	// rust-module-parity Phase 2: Persist package.json-derived module candidates and file ownership.
	persist_npm_modules(storage, repo_uid, &result.snapshot_uid, &prepared.npm_modules, &prepared.file_inputs)?;

	emit_progress(&mut progress, "persisting", 9, 11)?;  // about to persist pyproject modules
	// rust-module-parity Phase 2c: Persist pyproject.toml-derived module candidates and file ownership.
	persist_pyproject_modules(storage, repo_uid, &result.snapshot_uid, &prepared.pyproject_modules, &prepared.file_inputs)?;

	emit_progress(&mut progress, "persisting", 10, 11)?;  // about to persist Gradle modules
	// rust-module-parity Phase 2b: Persist settings.gradle-derived module candidates and file ownership.
	persist_gradle_modules(storage, repo_uid, &result.snapshot_uid, &prepared.gradle_modules, &prepared.file_inputs)?;

	Ok(result)
}

/// Index a repo from disk, opening storage at db_path.
pub fn index_path(
	repo_path: &Path,
	db_path: &Path,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	index_path_with_progress(repo_path, db_path, repo_uid, options, None)
}

/// Index a repo from disk with progress reporting.
pub fn index_path_with_progress(
	repo_path: &Path,
	db_path: &Path,
	repo_uid: &str,
	options: &ComposeOptions,
	progress: Option<ProgressCallback<'_>>,
) -> Result<IndexResult, ComposeError> {
	let mut storage = open_or_create_storage(db_path)?;
	index_into_storage_with_progress(repo_path, &mut storage, repo_uid, options, progress)
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
	refresh_into_storage_with_progress(repo_path, storage, repo_uid, options, None)
}

/// Refresh with progress reporting.
pub fn refresh_into_storage_with_progress(
	repo_path: &Path,
	storage: &mut StorageConnection,
	repo_uid: &str,
	options: &ComposeOptions,
	mut progress: Option<ProgressCallback<'_>>,
) -> Result<IndexResult, ComposeError> {
	emit_progress(&mut progress, "scanning", 0, 1)?;
	let prepared = prepare_repo_inputs(repo_path)?;
	emit_progress(&mut progress, "scanning", 1, 1)?;

	let mut ts_extractor = TsExtractor::new();
	ts_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("ts: {}", e)))?;

	let mut c_extractor = CExtractor::new();
	c_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("c: {}", e)))?;

	let mut cpp_extractor = CppExtractor::new();
	cpp_extractor
		.initialize()
		.map_err(|e| ComposeError::ExtractorInit(format!("cpp: {}", e)))?;

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

	// Checkpoint BEFORE repo mutation — abort here if transport failed
	emit_progress(&mut progress, "initializing", 0, 1)?;
	ensure_repo(storage, repo_uid, repo_path, options)?;

	let mut extractors: Vec<&mut dyn ExtractorPort> = vec![&mut ts_extractor, &mut c_extractor, &mut cpp_extractor, &mut java_extractor, &mut python_extractor, &mut rust_extractor];

	// Bridge the compose progress callback to the indexer callback.
	// The indexer emits per-file extracting progress with abort checkpoints.
	let mut indexer_progress_callback = |event: &IndexProgressEvent| -> ControlFlow<()> {
		if let Some(ref mut cb) = progress {
			let phase = match event.phase {
				IndexPhase::Scanning => "scanning",
				IndexPhase::Extracting => "extracting",
				IndexPhase::Resolving => "resolving",
				IndexPhase::Persisting => "persisting",
			};
			cb(&ProgressEvent::new(phase, event.current, event.total))
		} else {
			ControlFlow::Continue(())
		}
	};

	let mut idx_options = IndexOptions {
		basis_commit: options.basis_commit.clone(),
		edge_batch_size: options.edge_batch_size,
		c_include_roots: options.c_include_roots.clone(),
		on_progress: Some(&mut indexer_progress_callback),
		..IndexOptions::default()
	};

	// State-boundary hook (symmetric with index path — SB-4-pre.8).
	let mut sb_hook = crate::state_boundary_hook::StateBoundaryHook::new(repo_uid);

	// Convert config files to state for invalidation planning.
	let config_states: Vec<orchestrator::ConfigFileState> = prepared
		.config_file_inputs
		.iter()
		.map(|cf| orchestrator::ConfigFileState {
			rel_path: cf.rel_path.clone(),
			content_hash: cf.content_hash.clone(),
		})
		.collect();

	// The indexer now emits per-file progress with abort checkpoints.
	// IndexError::Aborted maps to ComposeError::Aborted for transport failure.
	let mut result = match orchestrator::refresh_repo(
		storage,
		&mut extractors,
		repo_uid,
		&prepared.file_inputs,
		&prepared.contract_file_inputs,
		&config_states,
		&mut idx_options,
		Some(&mut sb_hook),
	) {
		Ok(r) => r,
		Err(IndexError::Aborted) => return Err(ComposeError::Aborted),
		Err(e) => return Err(ComposeError::Index(format!("{}", e))),
	};

	// ══════════════════════════════════════════════════════════════════════════
	// Contract-Driven Refresh Dispatch (ACR-2)
	// ══════════════════════════════════════════════════════════════════════════
	//
	// Dispatch copy-forward for unchanged files based on artifact contracts.
	// Each family in COPY_FORWARD_FAMILIES is handled according to its
	// RefreshPolicy. Families with ReextractChangedInputs or MarkImpactedDeferRecompute
	// copy forward rows for unchanged files.
	//
	// This replaces the ad-hoc copy_forward_derived_artifacts() call with
	// explicit, contract-driven dispatch.

	let unchanged_file_set: std::collections::HashSet<&str> =
		result.unchanged_files.as_ref()
			.map(|files| files.iter().map(|s| s.as_str()).collect())
			.unwrap_or_default();

	if let (Some(parent_uid), Some(unchanged_files)) = (
		&result.parent_snapshot_uid,
		&result.unchanged_files,
	) {
		let mut diagnostics = RefreshDiagnostics::new();

		// Per-family counters for backward-compatible ArtifactCopyForward
		let mut measurements_copied: u64 = 0;
		let mut inferences_copied: u64 = 0;
		let mut boundary_surfaces_copied: u64 = 0;
		let mut boundary_channels_copied: u64 = 0;
		// ContractSchemas is re-indexed, not copy-forwarded in active path
		let contract_schemas_copied: u64 = 0;
		let contract_elements_copied: u64 = 0;

		// ── Contract-driven dispatch for COPY_FORWARD_FAMILIES ──
		for family in COPY_FORWARD_FAMILIES {
			let contract = get_contract(*family);

			let (action, rows) = match contract.refresh_policy {
				RefreshPolicy::ReextractChangedInputs | RefreshPolicy::MarkImpactedDeferRecompute => {
					// Unchanged branch: copy forward from parent snapshot
					match family {
						ArtifactFamily::Measurements => {
							let n = storage.copy_forward_measurements(
								parent_uid,
								&result.snapshot_uid,
								repo_uid,
								unchanged_files,
							).map_err(ComposeError::Storage)?;
							measurements_copied = n;
							(RefreshAction::CopiedForward, Some(n as usize))
						}
						ArtifactFamily::Inferences => {
							let n = storage.copy_forward_inferences(
								parent_uid,
								&result.snapshot_uid,
								repo_uid,
								unchanged_files,
							).map_err(ComposeError::Storage)?;
							inferences_copied = n;
							(RefreshAction::CopiedForward, Some(n as usize))
						}
						ArtifactFamily::BoundaryInteractionSurfaces => {
							let (surfaces, channels) = storage.copy_forward_boundary_surfaces(
								parent_uid,
								&result.snapshot_uid,
								unchanged_files,
							).map_err(ComposeError::Storage)?;
							boundary_surfaces_copied = surfaces;
							boundary_channels_copied = channels;
							(RefreshAction::CopiedForward, Some((surfaces + channels) as usize))
						}
						_ => {
							// Family in COPY_FORWARD_FAMILIES but no storage method yet
							(RefreshAction::NotImplemented, None)
						}
					}
				}
				RefreshPolicy::RecomputeFromCurrentSnapshot => {
					// Should not be in COPY_FORWARD_FAMILIES
					(RefreshAction::Skipped, None)
				}
				RefreshPolicy::SnapshotIndependent => {
					(RefreshAction::Skipped, None)
				}
				_ => {
					(RefreshAction::NotImplemented, None)
				}
			};

			diagnostics.record(FamilyRefreshResult {
				family: *family,
				policy: contract.refresh_policy,
				action,
				rows_affected: rows,
			});
		}

		// Diagnostics are captured but not emitted here.
		// The structured data is available in `diagnostics` for future
		// exposure through result objects or explicit diagnostic APIs.
		let _ = &diagnostics; // suppress unused warning until wired to result

		// Surface copy-forward counts in IndexResult for backward compatibility.
		// This struct is part of the public API; keep populating it until
		// consumers migrate to contract-driven diagnostics.
		result.artifact_copy_forward = Some(repo_graph_indexer::types::ArtifactCopyForward {
			measurements_copied,
			inferences_copied,
			boundary_surfaces_copied,
			boundary_channels_copied,
			contract_schemas_copied,
			contract_elements_copied,
		});

		// ══════════════════════════════════════════════════════════════════════════
		// Impact Propagation (ACR-4)
		// ══════════════════════════════════════════════════════════════════════════
		//
		// Mark derived artifacts as impacted when their Layer 0 dependencies changed.
		// This implements the MarkImpactedOnRelevantLayer0Change policy.
		//
		// Collect stable keys of nodes from changed files. A file is "changed" if
		// it was re-extracted (not in unchanged_files). We use the stable_key format
		// `{repo_uid}:{path}#{symbol}:SYMBOL:{type}` to match nodes to changed files.
		let changed_file_paths: Vec<&str> = prepared.file_inputs
			.iter()
			.map(|f| f.rel_path.as_str())
			.filter(|path| !unchanged_file_set.contains(*path))
			.collect();

		let changed_stable_keys: Vec<String> = if changed_file_paths.is_empty() {
			Vec::new()
		} else {
			// Query all nodes for the snapshot and filter to those from changed files.
			// Node stable_keys embed the file path with specific delimiters:
			// - SYMBOL nodes: "repo:path#symbol:SYMBOL:type" (# after path)
			// - FILE nodes: "repo:path:FILE" (exact match)
			//
			// We must check for the delimiter to avoid false-matching path prefixes
			// (e.g., "src/A.java" should not match "src/A.javax/Foo.java").
			let all_nodes = storage
				.query_all_nodes(&result.snapshot_uid)
				.map_err(ComposeError::Storage)?;

			all_nodes
				.into_iter()
				.filter(|node| {
					changed_file_paths.iter().any(|path| {
						// SYMBOL nodes have # after path: "repo:path#symbol:SYMBOL:type"
						let symbol_prefix = format!("{}:{}#", repo_uid, path);
						// FILE nodes have exact format: "repo:path:FILE"
						let file_key = format!("{}:{}:FILE", repo_uid, path);
						node.stable_key.starts_with(&symbol_prefix) || node.stable_key == file_key
					})
				})
				.map(|node| node.stable_key)
				.collect()
		};

		// Propagate impact to derived artifacts whose provenance references changed stable keys
		let _impact_report: ImpactReport = propagate_impact(
			storage,
			&result.snapshot_uid,
			&changed_stable_keys,
		).map_err(ComposeError::Storage)?;

		// Impact report available for future diagnostics integration.
		// Fields: total_impacted(), get(family) per artifact family.
		// TODO: Include impact_report in result diagnostics (ACR-4 follow-on)
	}

	// Filter file inputs to only changed files for postpass extraction.
	// Unchanged files already have their artifacts from copy-forward.
	let changed_files_owned: Vec<FileInput> = prepared.file_inputs
		.iter()
		.filter(|f| !unchanged_file_set.contains(f.rel_path.as_str()))
		.cloned()
		.collect();

	// Persisting phase: checkpoint BEFORE each mutation (7 mutations total)
	// Semantics: current=N means "about to do mutation N"

	emit_progress(&mut progress, "persisting", 0, 8)?;  // about to persist read failures
	persist_read_failures(
		storage,
		repo_uid,
		&result.snapshot_uid.clone(),
		&prepared.read_failed_paths,
		&mut result,
	)?;

	emit_progress(&mut progress, "persisting", 1, 8)?;  // about to persist config file versions
	// Persist config file versions for refresh invalidation tracking.
	// Config files are NOT extracted — only tracked for hash comparison.
	persist_config_file_versions(
		storage,
		repo_uid,
		&result.snapshot_uid,
		&prepared.config_file_inputs,
	)?;

	emit_progress(&mut progress, "persisting", 2, 8)?;  // about to persist metrics
	// RS-MS-3c-prereq: Persist metrics (complexity, params, nesting).
	// Only for changed files; unchanged file metrics already copied forward.
	persist_metrics(storage, repo_uid, &result.snapshot_uid, &result.metrics)?;

	emit_progress(&mut progress, "persisting", 3, 8)?;  // about to persist spring liveness
	// Persist Spring framework-liveness inferences for dead-code suppression.
	// Refresh mode: only process nodes from changed files, preserve copy-forwarded inferences.
	let changed_paths_for_spring: Vec<&str> = changed_files_owned
		.iter()
		.map(|f| f.rel_path.as_str())
		.collect();
	persist_spring_liveness_inferences(
		storage,
		repo_uid,
		&result.snapshot_uid,
		Some(&changed_paths_for_spring),
	)?;

	emit_progress(&mut progress, "persisting", 4, 8)?;  // about to persist policy facts
	// PF-1: Extract and persist STATUS_MAPPING policy facts from C files.
	// TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
	// Only extract from changed files; unchanged files copied forward.
	persist_policy_facts(storage, repo_uid, &result.snapshot_uid, &changed_files_owned)?;

	emit_progress(&mut progress, "persisting", 5, 8)?;  // about to persist C boundary interactions
	// BI-1A: Extract and persist boundary interaction facts from C files.
	// TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
	// Only extract from changed files; unchanged files copied forward.
	persist_boundary_interactions(storage, repo_uid, &result.snapshot_uid, &changed_files_owned)?;

	emit_progress(&mut progress, "persisting", 6, 8)?;  // about to persist TS boundary interactions
	// BI-1C: Extract and persist boundary interaction facts from TS/JS files.
	// SharedArrayBuffer, Worker, postMessage, Atomics patterns.
	// Only extract from changed files; unchanged files copied forward.
	persist_ts_boundary_interactions(storage, repo_uid, &result.snapshot_uid, &changed_files_owned)?;

	emit_progress(&mut progress, "persisting", 7, 8)?;  // about to persist Cargo modules
	// rust-module-parity Phase 1.5: Persist Cargo.toml-derived module candidates and file ownership.
	// Always recompute from current prepared inputs (Cargo.toml content).
	// Cargo.toml changes trigger config-file invalidation, so recompute is correct.
	// Ownership is recomputed for all files to maintain consistency.
	persist_cargo_modules(storage, repo_uid, &result.snapshot_uid, &prepared.cargo_modules, &prepared.file_inputs)?;

	// rust-module-parity Phase 2: Persist package.json-derived module candidates and file ownership.
	// Same recompute semantics as Cargo.
	persist_npm_modules(storage, repo_uid, &result.snapshot_uid, &prepared.npm_modules, &prepared.file_inputs)?;

	// rust-module-parity Phase 2c: Persist pyproject.toml-derived module candidates and file ownership.
	// Same recompute semantics as Cargo and npm.
	persist_pyproject_modules(storage, repo_uid, &result.snapshot_uid, &prepared.pyproject_modules, &prepared.file_inputs)?;

	// rust-module-parity Phase 2b: Persist settings.gradle-derived module candidates and file ownership.
	// Same recompute semantics as Cargo, npm, and pyproject.
	persist_gradle_modules(storage, repo_uid, &result.snapshot_uid, &prepared.gradle_modules, &prepared.file_inputs)?;

	Ok(result)
}

/// Refresh a repo from disk, opening storage at db_path.
pub fn refresh_path(
	repo_path: &Path,
	db_path: &Path,
	repo_uid: &str,
	options: &ComposeOptions,
) -> Result<IndexResult, ComposeError> {
	refresh_path_with_progress(repo_path, db_path, repo_uid, options, None)
}

/// Refresh a repo from disk with progress reporting.
pub fn refresh_path_with_progress(
	repo_path: &Path,
	db_path: &Path,
	repo_uid: &str,
	options: &ComposeOptions,
	progress: Option<ProgressCallback<'_>>,
) -> Result<IndexResult, ComposeError> {
	let mut storage = open_or_create_storage(db_path)?;
	refresh_into_storage_with_progress(repo_path, &mut storage, repo_uid, options, progress)
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
	options: &ComposeOptions,
) -> Result<(), ComposeError> {
	use repo_graph_storage::types::{Repo, RepoRef};

	// Use storage_root_path if provided (CLI computes DB-relative path),
	// otherwise fall back to the literal repo_path string.
	let root_path = options
		.storage_root_path
		.clone()
		.unwrap_or_else(|| repo_path.to_string_lossy().into());

	let existing = storage
		.get_repo(&RepoRef::Uid(repo_uid.into()))
		.map_err(ComposeError::Storage)?;

	if let Some(repo) = existing {
		// Repo exists. Check if root_path needs migration to DB-relative form.
		// This handles backward compatibility: old DBs stored caller-relative
		// paths; refresh updates them to the new DB-relative contract.
		if repo.root_path != root_path {
			storage
				.update_repo_root_path(repo_uid, &root_path)
				.map_err(ComposeError::Storage)?;
		}
		return Ok(());
	}

	// Create new repo with DB-relative root_path
	storage
		.add_repo(&Repo {
			repo_uid: repo_uid.into(),
			name: repo_uid.into(),
			root_path,
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
