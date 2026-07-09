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
use std::time::Instant;

/// PERF-INSTRUMENTATION-1: request/phase perf marker.
///
/// Emits to stderr when the compile-time `perf-trace` feature is on (force-on,
/// unchanged legacy behavior) OR the runtime `RMAP_PERF` gate is at level >= 1.
/// When off, the only cost is a single relaxed atomic load (`perf_enabled`);
/// the format arguments are never evaluated.
macro_rules! perf_log {
    ($($arg:tt)*) => {
        if cfg!(feature = "perf-trace") || $crate::perf::perf_enabled() {
            eprintln!($($arg)*);
        }
    };
}

use repo_graph_boundary_interaction::table::Language as BiLanguage;
use repo_graph_boundary_interaction::ChannelKind;
use repo_graph_boundary_interaction_extractor::emit::{
    BoundaryCallsite, BoundaryInteractionEmitter, EmitterContext, MmapFlags, SocketFamily,
    SocketType,
};
use repo_graph_boundary_interaction_extractor::socket_lineage::{FdRegistry, TrackedChannelKind};
use repo_graph_c_extractor::CExtractor;
use repo_graph_c_extractor::{
    extract_boundary_calls, MmapFlags as RawMmapFlags, RawBoundaryCall,
    SocketFamily as RawSocketFamily, SocketType as RawSocketType,
};
use repo_graph_classification::spring_liveness::{classify_spring_liveness, SpringNodeInput};
use repo_graph_classification::types::{PackageDependencySet, TsconfigAliases};
use repo_graph_cpp_extractor::CppExtractor;
use repo_graph_indexer::cargo_manifest::{
    self, CargoModule, CargoModuleCandidateInput, CargoModuleEvidenceInput, CargoModuleStorePort,
    FileOwnershipInput,
};
use repo_graph_indexer::extractor_port::ExtractorPort;
use repo_graph_indexer::inferred_modules::{self, InferredModule};
use repo_graph_indexer::orchestrator::{self, FileInput, IndexError};
use repo_graph_indexer::package_json::{self, NpmModule};
use repo_graph_indexer::proto_indexer::ProtoFileInput;
use repo_graph_indexer::pyproject::{self, PyprojectModule};
use repo_graph_indexer::routing;
use repo_graph_indexer::settings_gradle::{self, GradleModule};
use repo_graph_indexer::storage_port::{SnapshotLifecyclePort, UpdateSnapshotStatusInput};
use repo_graph_indexer::types::{
    IndexOptions, IndexPhase, IndexProgressEvent, IndexResult, SnapshotStatus,
};
use repo_graph_java_extractor::JavaExtractor;
use repo_graph_policy_facts::{
    extractors::behavioral_marker::extract_behavioral_markers,
    extractors::return_fate::extract_return_fates,
    extractors::status_mapping::extract_status_mappings, PolicyFactsStorageWrite,
};
use repo_graph_python_extractor::PythonExtractor;
use repo_graph_rust_extractor::RustExtractor;
use repo_graph_storage::types::InferenceInput;
use repo_graph_storage::StorageConnection;
use repo_graph_ts_extractor::{
    extract_amqp_boundary_calls, extract_kafka_boundary_calls, extract_nats_boundary_calls,
    extract_ts_boundary_calls, RawAmqpBoundaryCall, RawKafkaBoundaryCall, RawNatsBoundaryCall,
    RawTsBoundaryCall, TsExtractor,
};

use crate::config::RepoConfigContext;
use crate::express_detector::detect_express_routes;
use crate::impact_propagation::{propagate_impact, ImpactReport};
use crate::react_detector::{
    components_to_inferences, detect_react_components, detect_react_hooks, hooks_to_inferences,
};
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
#[derive(Default)]
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
fn emit_progress(
    progress: &mut Option<ProgressCallback<'_>>,
    phase: &str,
    current: u64,
    total: u64,
) -> Result<(), ComposeError> {
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

/// Extracted inferred module with provenance info (rust-module-parity Phase 3).
#[derive(Debug, Clone)]
pub struct ExtractedInferredModule {
    /// The inferred module data
    pub module: InferredModule,
}

/// Result of inferred module detection for a repo.
#[derive(Debug, Clone, Default)]
pub struct InferredExtractionResult {
    /// Inferred modules from directory heuristics
    pub modules: Vec<ExtractedInferredModule>,
}

// ── ORIENT-BUG-1: Ecosystem-scoped module coverage ──────────────────────────
//
// Module ecosystems define which file languages they cover.
// A declared module only suppresses inferred detection for files
// in languages that belong to that module's ecosystem.

/// Module ecosystem — determines which file languages a declared module covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleEcosystem {
    /// Cargo/Rust: covers .rs files
    Cargo,
    /// npm/Node: covers .js, .ts, .jsx, .tsx, .mjs, .cjs files
    Npm,
    /// Python: covers .py, .pyi files
    Python,
    /// Gradle/Java: covers .java, .kt, .scala files
    Gradle,
}

impl ModuleEcosystem {
    /// Check if this ecosystem covers a file based on its extension.
    fn covers_extension(&self, ext: &str) -> bool {
        match self {
            ModuleEcosystem::Cargo => ext == "rs",
            ModuleEcosystem::Npm => matches!(ext, "js" | "ts" | "jsx" | "tsx" | "mjs" | "cjs"),
            ModuleEcosystem::Python => matches!(ext, "py" | "pyi"),
            ModuleEcosystem::Gradle => matches!(ext, "java" | "kt" | "scala"),
        }
    }
}

/// A declared module root with its ecosystem.
/// Used for ecosystem-scoped coverage checks (ORIENT-BUG-1).
#[derive(Debug, Clone)]
pub struct DeclaredRoot {
    /// Root path (relative to repo root, "." for repo root)
    pub path: String,
    /// Ecosystem that owns this module
    pub ecosystem: ModuleEcosystem,
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
    /// Inferred module detection results (rust-module-parity Phase 3).
    /// Only populated if no declared modules exist for the repo.
    pub inferred_modules: InferredExtractionResult,
}

/// Scan the repo, resolve config per file, assemble typed FileInput.
///
/// Files are partitioned into:
/// - `file_inputs`: source files for the language extraction pipeline
/// - `contract_file_inputs`: contract files (e.g., .proto) for the contract pipeline
pub fn prepare_repo_inputs(repo_path: &Path) -> Result<PreparedRepoInputs, ComposeError> {
    let scanned = scanner::scan_repo(repo_path).map_err(ComposeError::Scan)?;
    let mut config_ctx = RepoConfigContext::new();

    let mut file_inputs = Vec::new();
    let mut contract_file_inputs = Vec::new();
    let mut config_file_inputs = Vec::new();
    let mut read_failed_paths = Vec::new();
    // Collect Cargo.toml files with content for module extraction.
    // Keyed by rel_path for later workspace member resolution.
    let mut cargo_toml_files: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // Collect package.json files with content for npm module extraction (Phase 2).
    let mut package_json_files: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // Content of pnpm-workspace.yaml if present (Phase 2).
    let mut pnpm_workspace_content: Option<String> = None;
    // Collect pyproject.toml files with content for Python module extraction (Phase 2c).
    let mut pyproject_toml_files: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // Collect settings.gradle files with content for Gradle module extraction (Phase 2b).
    let mut settings_gradle_files: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

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

    // Detect inferred modules (Phase 3, ORIENT-BUG-1 fix) — gap-fill approach.
    // Collect declared module roots and run inferred detection only on uncovered paths.
    let declared_roots = collect_declared_module_roots(
        &cargo_modules,
        &npm_modules,
        &pyproject_modules,
        &gradle_modules,
    );

    let inferred_modules = if declared_roots.is_empty() {
        // No declared modules — run inferred detection on all files.
        detect_inferred_modules_from_inputs(&file_inputs, repo_path)
    } else {
        // Gap-fill: run inferred detection only on files NOT under declared roots.
        detect_inferred_modules_gap_fill(&file_inputs, &declared_roots, repo_path)
    };

    Ok(PreparedRepoInputs {
        file_inputs,
        read_failed_paths,
        contract_file_inputs,
        config_file_inputs,
        cargo_modules,
        npm_modules,
        pyproject_modules,
        gradle_modules,
        inferred_modules,
    })
}

// ── Cargo module extraction (ORIENT-BUG-1: Deep Manifest Discovery) ─────────

/// Extract Cargo modules from ALL Cargo.toml files in the tree.
///
/// Deep discovery approach (ORIENT-BUG-1):
/// 1. Find all workspace roots anywhere in tree (Cargo.toml with [workspace])
/// 2. For each workspace root, expand members relative to that root's directory
/// 3. Find standalone crates (Cargo.toml with [package] not discovered via workspace)
/// 4. Deduplicate by crate_root to avoid double-counting
///
/// This fixes repos where Rust code lives in a subdirectory (e.g., rust/Cargo.toml).
fn extract_cargo_modules(
    repo_path: &Path,
    cargo_toml_files: &std::collections::HashMap<String, String>,
) -> CargoExtractionResult {
    let mut result = CargoExtractionResult::default();
    let mut discovered_crate_roots: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // Track if root Cargo.toml exists (for compatibility)
    result.has_root_manifest = cargo_toml_files.contains_key("Cargo.toml");

    // Phase 1: Find all workspace roots and expand their members
    let mut workspace_roots: Vec<(String, Vec<String>)> = Vec::new(); // (manifest_path, patterns)

    for (manifest_path, content) in cargo_toml_files {
        if let Ok(parsed) = cargo_manifest::parse_cargo_toml(content, manifest_path) {
            if parsed.is_workspace_root {
                workspace_roots.push((manifest_path.clone(), parsed.workspace_members.clone()));
            }
        }
    }

    // Phase 2: For each workspace root, expand members relative to that root's directory
    for (manifest_path, patterns) in &workspace_roots {
        // Get workspace root directory (e.g., "rust" for "rust/Cargo.toml", "" for "Cargo.toml")
        let workspace_dir = manifest_path
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_default();

        // Check if workspace root itself has a [package] section
        if let Some(content) = cargo_toml_files.get(manifest_path) {
            if let Ok(parsed) = cargo_manifest::parse_cargo_toml(content, manifest_path) {
                for module in &parsed.modules {
                    if !discovered_crate_roots.contains(&module.crate_root) {
                        discovered_crate_roots.insert(module.crate_root.clone());
                        result.modules.push(ExtractedCargoModule {
                            module: module.clone(),
                            declared_pattern: None,
                        });
                    }
                }
            }
        }

        // Expand member patterns relative to workspace directory
        for pattern in patterns {
            let expanded = expand_workspace_pattern_relative(
                repo_path,
                &workspace_dir,
                pattern,
                cargo_toml_files,
            );
            if expanded.is_empty() {
                result.unmatched_patterns.push(pattern.clone());
            } else {
                for member_module in expanded {
                    if !discovered_crate_roots.contains(&member_module.crate_root) {
                        discovered_crate_roots.insert(member_module.crate_root.clone());
                        result.modules.push(ExtractedCargoModule {
                            module: member_module,
                            declared_pattern: Some(pattern.clone()),
                        });
                    }
                }
            }
        }
    }

    // Phase 3: Find standalone crates not discovered via workspace
    for (manifest_path, content) in cargo_toml_files {
        if let Ok(parsed) = cargo_manifest::parse_cargo_toml(content, manifest_path) {
            // Only consider packages, not workspace-only manifests
            for module in &parsed.modules {
                if !discovered_crate_roots.contains(&module.crate_root) {
                    discovered_crate_roots.insert(module.crate_root.clone());
                    result.modules.push(ExtractedCargoModule {
                        module: module.clone(),
                        declared_pattern: None,
                    });
                }
            }
        }
    }

    result
}

/// Expand a workspace member pattern relative to a workspace directory.
///
/// For pattern "crates/*" in workspace at "rust/Cargo.toml":
/// - workspace_dir = "rust"
/// - Full pattern becomes "rust/crates/*"
/// - Member crate_root is relative to repo root (e.g., "rust/crates/foo")
fn expand_workspace_pattern_relative(
    repo_path: &Path,
    workspace_dir: &str,
    pattern: &str,
    cargo_toml_files: &std::collections::HashMap<String, String>,
) -> Vec<CargoModule> {
    let mut modules = Vec::new();

    // Build full pattern path relative to repo root
    let full_pattern_base = if workspace_dir.is_empty() {
        pattern.to_string()
    } else {
        format!("{}/{}", workspace_dir, pattern)
    };

    // Check if pattern contains glob characters
    if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
        // Glob expansion
        let full_pattern = repo_path.join(&full_pattern_base).join("Cargo.toml");
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
        let manifest_path = format!("{}/Cargo.toml", full_pattern_base);
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
        let canonical_repo = repo_path
            .canonicalize()
            .unwrap_or_else(|_| repo_path.to_path_buf());
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
        result.modules.push(ExtractedGradleModule {
            module: root_module,
        });
    }

    // Add subprojects
    for subproject in parsed.subprojects {
        result
            .modules
            .push(ExtractedGradleModule { module: subproject });
    }

    result
}

// ── Inferred module detection (rust-module-parity Phase 3, ORIENT-BUG-1) ────

/// Collect all declared module roots with their ecosystems.
///
/// These roots define coverage regions where inferred detection should NOT run.
/// Coverage is ecosystem-scoped: a Cargo root only covers .rs files, npm only covers JS/TS, etc.
/// This prevents a root npm package from suppressing Rust inference in mixed-language repos.
fn collect_declared_module_roots(
    cargo_modules: &CargoExtractionResult,
    npm_modules: &NpmExtractionResult,
    pyproject_modules: &PyprojectExtractionResult,
    gradle_modules: &GradleExtractionResult,
) -> Vec<DeclaredRoot> {
    let mut roots = Vec::new();

    // Cargo modules → cover Rust files
    for m in &cargo_modules.modules {
        let root = &m.module.crate_root;
        let path = if root.is_empty() || root == "." {
            ".".to_string()
        } else {
            root.clone()
        };
        roots.push(DeclaredRoot {
            path,
            ecosystem: ModuleEcosystem::Cargo,
        });
    }

    // npm modules → cover JS/TS files
    for m in &npm_modules.modules {
        let root = &m.module.package_root;
        let path = if root.is_empty() || root == "." {
            ".".to_string()
        } else {
            root.clone()
        };
        roots.push(DeclaredRoot {
            path,
            ecosystem: ModuleEcosystem::Npm,
        });
    }

    // pyproject modules → cover Python files
    for m in &pyproject_modules.modules {
        let root = &m.module.package_root;
        let path = if root.is_empty() || root == "." {
            ".".to_string()
        } else {
            root.clone()
        };
        roots.push(DeclaredRoot {
            path,
            ecosystem: ModuleEcosystem::Python,
        });
    }

    // Gradle modules → cover Java/Kotlin/Scala files
    for m in &gradle_modules.modules {
        let root = &m.module.project_root;
        let path = if root.is_empty() || root == "." {
            ".".to_string()
        } else {
            root.clone()
        };
        roots.push(DeclaredRoot {
            path,
            ecosystem: ModuleEcosystem::Gradle,
        });
    }

    roots
}

/// Check if a file is covered by any declared module root (ecosystem-scoped).
///
/// Coverage rule: a file at `some/path/file.rs` is covered by a Cargo root `some/path`
/// only if:
/// 1. The file is under that root path (or root is ".")
/// 2. The file's extension belongs to that ecosystem (e.g., .rs for Cargo)
///
/// This prevents a root npm package from suppressing Rust inference.
fn is_file_covered_by_roots(file_path: &str, roots: &[DeclaredRoot]) -> bool {
    // Extract file extension
    let ext = file_path.rsplit('.').next().unwrap_or("");

    for root in roots {
        // Check if ecosystem covers this file type
        if !root.ecosystem.covers_extension(ext) {
            continue;
        }

        // Check if file is under this root
        if root.path == "." {
            // Root "." covers everything (for its ecosystem)
            return true;
        }
        if file_path == root.path || file_path.starts_with(&format!("{}/", root.path)) {
            return true;
        }
    }
    false
}

/// Detect inferred modules using gap-fill approach (ORIENT-BUG-1 fix).
///
/// Filters file paths to exclude those covered by declared module roots,
/// using ecosystem-scoped coverage (npm covers JS/TS, Cargo covers Rust, etc.).
/// Then runs inferred detection on the remaining uncovered paths.
fn detect_inferred_modules_gap_fill(
    file_inputs: &[FileInput],
    declared_roots: &[DeclaredRoot],
    repo_path: &Path,
) -> InferredExtractionResult {
    // Filter to uncovered paths only (ecosystem-scoped)
    let uncovered_paths: Vec<String> = file_inputs
        .iter()
        .map(|f| f.rel_path.clone())
        .filter(|path| !is_file_covered_by_roots(path, declared_roots))
        .collect();

    // If all paths are covered, no inferred modules
    if uncovered_paths.is_empty() {
        return InferredExtractionResult::default();
    }

    // Derive repo display name from path
    let repo_display_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("root");

    // Detect inferred modules on uncovered paths
    let detected = inferred_modules::detect_inferred_modules(&uncovered_paths, repo_display_name);

    // Convert to extraction result
    let modules = detected
        .modules
        .into_iter()
        .map(|module| ExtractedInferredModule { module })
        .collect();

    InferredExtractionResult { modules }
}

/// Detect inferred modules from file inputs (original behavior).
///
/// Uses top-level directory heuristics to infer module boundaries
/// in repos without manifest files.
fn detect_inferred_modules_from_inputs(
    file_inputs: &[FileInput],
    repo_path: &Path,
) -> InferredExtractionResult {
    // Extract file paths from inputs.
    let file_paths: Vec<String> = file_inputs.iter().map(|f| f.rel_path.clone()).collect();

    // Derive repo display name from path.
    let repo_display_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("root");

    // Detect inferred modules using the indexer heuristics.
    let detected = inferred_modules::detect_inferred_modules(&file_paths, repo_display_name);

    // Convert to extraction result.
    let modules = detected
        .modules
        .into_iter()
        .map(|module| ExtractedInferredModule { module })
        .collect();

    InferredExtractionResult { modules }
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
    if let Some(json_str) =
        TrustStorageRead::get_snapshot_extraction_diagnostics(storage, snapshot_uid)
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
    storage
        .upsert_files(&tracked)
        .map_err(ComposeError::Storage)?;

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
            ])
            .with_extractor(extractor);
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
                    .replace_inferences_by_kind(
                        snapshot_uid,
                        &["spring_container_managed"],
                        &inferences,
                    )
                    .map_err(ComposeError::Storage)?;
            }
        }
    }

    Ok(())
}

// ── Post-index policy-facts extraction ───────────────────────────

// ── PERSIST-RECURSION-1: re-parse postpass depth guard + failure isolation ──
//
// The per-file depth guard (`MAX_POSTPASS_TREE_DEPTH` / `tree_exceeds_depth`) lives
// in `crate::walk` so the compose-level postpasses (below) and the in-crate
// detectors (`express_detector`, `react_detector`) apply the SAME bound.
use crate::walk::{tree_exceeds_depth, MAX_POSTPASS_TREE_DEPTH};

/// Read-modify-write the snapshot's `extraction_diagnostics_json` blob — the
/// existing honest-degradation channel (the same free-form JSON the orchestrator
/// builds at finalize and `persist_read_failures` extends with `files_read_failed`).
/// Merging a key needs NO schema change: the blob is free-form and the typed
/// `ExtractionDiagnostics` reader ignores unknown keys.
///
/// **FALLIBLE (PERSIST-RECURSION-1 review-3):** this blob IS the honest-degradation
/// signal. If we cannot read/parse/write it, a skipped-facts degradation would be
/// silently lost — a READY snapshot claiming completeness it does not have. So every
/// failure PROPAGATES; the caller decides what a persist failure means (for the
/// postpass path it demotes the snapshot out of READY — see `isolate_postpass`). A
/// prior version swallowed all three failures (`.ok().flatten()` / `if let Ok` /
/// `let _ =`), which review-3 flagged as a false-completeness hole.
///
/// The blob normally exists (the orchestrator writes it at finalize before the
/// postpasses run), but if the column is still NULL we START FROM AN EMPTY OBJECT and
/// write it, rather than dropping the diagnostic — the snapshot row exists, so the
/// `UPDATE` lands. On the NON-pathological path this fn is never called (both callers
/// gate on `count > 0` or the postpass-error arm), so making it fallible cannot change
/// byte-for-byte output on existing fixtures.
fn merge_extraction_diagnostics(
    storage: &mut StorageConnection,
    snapshot_uid: &str,
    mutate: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> Result<(), ComposeError> {
    use repo_graph_trust::TrustStorageRead;
    let existing = TrustStorageRead::get_snapshot_extraction_diagnostics(storage, snapshot_uid)
        .map_err(ComposeError::Storage)?;
    let mut value = match existing {
        Some(json_str) => serde_json::from_str::<serde_json::Value>(&json_str).map_err(|e| {
            ComposeError::Index(format!(
                "extraction diagnostics blob is not valid JSON: {}",
                e
            ))
        })?,
        None => serde_json::Value::Object(serde_json::Map::new()),
    };
    let obj = value.as_object_mut().ok_or_else(|| {
        ComposeError::Index("extraction diagnostics blob is not a JSON object".to_string())
    })?;
    mutate(obj);
    let serialized = serde_json::to_string(&value)
        .map_err(|e| ComposeError::Index(format!("serialize extraction diagnostics: {}", e)))?;
    SnapshotLifecyclePort::update_snapshot_extraction_diagnostics(
        storage,
        snapshot_uid,
        &serialized,
    )
    .map_err(ComposeError::Storage)
}

/// Record that `count` files were skipped by a postpass because their AST
/// exceeded `MAX_POSTPASS_TREE_DEPTH` (PERSIST-RECURSION-1 honest degradation).
/// The count accumulates into `key` in the extraction-diagnostics blob.
///
/// FALLIBLE (review-3): a skip that cannot be recorded must NOT be silently dropped
/// — that would present a READY snapshot as complete when it skipped pathological
/// files. The persist failure propagates; the caller (each postpass, via `?`) turns
/// it into the postpass outcome, which `isolate_postpass` then treats as an
/// infrastructure failure. `count == 0` stays a no-op (no key written), preserving
/// byte-equality on non-pathological input.
fn record_files_skipped_deep_nesting(
    storage: &mut StorageConnection,
    snapshot_uid: &str,
    key: &str,
    count: u64,
) -> Result<(), ComposeError> {
    if count == 0 {
        return Ok(());
    }
    merge_extraction_diagnostics(storage, snapshot_uid, |obj| {
        let current = obj.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
        obj.insert(key.to_string(), serde_json::json!(current + count));
    })
}

/// Run a fallible re-parse postpass so an ordinary postpass failure NEVER aborts the
/// index (PERSIST-RECURSION-1 item 3). `Aborted` (transport / cancellation) still
/// propagates; any other error runs the caller's compensating `cleanup` and is
/// recorded as an extraction diagnostic, and the index keeps its snapshot MINUS that
/// postpass's facts. The stack overflow the slice fixes bypassed this entirely by
/// killing the process — item 1 (iterative walks) is the real fix; this is the
/// failure-isolation contract made explicit.
///
/// ## Why `cleanup` (review-2 item 2 — atomicity)
///
/// A postpass that writes several tables does so through several storage methods,
/// and each opens its OWN transaction (`connection().transaction()` = `BEGIN`).
/// SQLite forbids a nested `BEGIN`, so the postpass's writes CANNOT be wrapped in
/// one outer transaction. That means a postpass can fail after some of its writers
/// already committed — a partial subset (e.g. `persist_policy_facts` after
/// `status_mappings` committed but `behavioral_markers` failed; `persist_express_
/// surfaces` after surfaces committed but evidence failed). `outcome` alone can't
/// tell us that happened. So each call site passes a `cleanup` that DELETES all of
/// that postpass's facts for the snapshot; we run it on the isolatable-error path,
/// leaving the snapshot with NONE of the postpass's facts (the contract) instead of
/// a half-written mix.
///
/// ## When the isolation mechanism ITSELF fails (review-3 item 1)
///
/// `cleanup` (drop partial facts) and `merge_extraction_diagnostics` (record the
/// degradation) ARE the mechanism that keeps a failed postpass honest. If either
/// FAILS, completing would leave a READY snapshot that is silently wrong — partial
/// facts survive, or the missing-facts degradation is unrecorded (a false Layer-0
/// completeness claim). Both are now fallible and both propagate.
///
/// But propagating alone is not enough here: **the snapshot is already `Ready`.**
/// `index_repo` finalizes it to READY (`orchestrator.rs`) BEFORE these postpasses
/// run, the daemon's error arm does not touch snapshot status, and the served
/// snapshot is `get_latest_snapshot` = the latest `status = 'ready'` row. So a mere
/// `Err` return would still SERVE the dishonest snapshot. On this infrastructure-
/// failure path we therefore DEMOTE the snapshot out of READY (to `Failed`, the
/// orchestrator's own fatal-error state — same `update_snapshot_status` idiom it uses
/// on its fatal paths) so `get_latest_snapshot` excludes it; on refresh, serving
/// falls back to the last-good parent (untouched during refresh). The demotion is
/// best-effort (if it too fails the DB is unwritable); the original infra error still
/// propagates. This path triggers ONLY on a compounded infra failure, never the
/// normal postpass-failure path.
fn isolate_postpass(
    storage: &mut StorageConnection,
    snapshot_uid: &str,
    postpass: &str,
    error_key: &str,
    outcome: Result<usize, ComposeError>,
    cleanup: impl FnOnce(&mut StorageConnection) -> Result<(), ComposeError>,
) -> Result<(), ComposeError> {
    match outcome {
        Ok(_) => Ok(()),
        Err(ComposeError::Aborted) => Err(ComposeError::Aborted),
        Err(e) => {
            perf_log!(
                "[PERSIST-RECURSION-1] {} postpass failed; index continues without its facts: {}",
                postpass,
                e
            );
            // The isolation mechanism: drop any partial facts, THEN record the
            // failure as a diagnostic. Either failing is an infrastructure failure.
            let mut isolation = cleanup(storage);
            if isolation.is_ok() {
                let message = e.to_string();
                isolation = merge_extraction_diagnostics(storage, snapshot_uid, |obj| {
                    obj.insert(error_key.to_string(), serde_json::json!(message));
                });
            }
            match isolation {
                Ok(()) => Ok(()),
                Err(infra) => {
                    // Demote out of READY so the silently-dishonest snapshot is not
                    // served (see the doc comment). Best-effort, mirroring the
                    // orchestrator's own fatal-error demotion idiom.
                    let _ = SnapshotLifecyclePort::update_snapshot_status(
                        storage,
                        &UpdateSnapshotStatusInput {
                            snapshot_uid: snapshot_uid.to_string(),
                            status: SnapshotStatus::Failed,
                            completed_at: None,
                        },
                    );
                    Err(infra)
                }
            }
        }
    }
}

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
    // PERSIST-RECURSION-1: count files whose facts are skipped for pathological depth.
    let mut skipped_deep: u64 = 0;

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

        // PERSIST-RECURSION-1: skip pathologically deep files entirely for this
        // postpass (honest degradation), never let it walk the file at all.
        if tree_exceeds_depth(&tree.root_node(), MAX_POSTPASS_TREE_DEPTH) {
            skipped_deep += 1;
            perf_log!(
                "[PERSIST-RECURSION-1] policy-facts: skipping deeply nested file {} (AST depth > {})",
                file.rel_path,
                MAX_POSTPASS_TREE_DEPTH
            );
            continue;
        }

        // PF-1: Extract STATUS_MAPPING facts.
        let mappings =
            extract_status_mappings(&tree, file.content.as_bytes(), &file.rel_path, repo_uid);
        all_mappings.extend(mappings);

        // PF-2: Extract BEHAVIORAL_MARKER facts.
        let markers =
            extract_behavioral_markers(&tree, file.content.as_bytes(), &file.rel_path, repo_uid);
        all_markers.extend(markers);

        // PF-3: Extract RETURN_FATE facts.
        let fates = extract_return_fates(&tree, file.content.as_bytes(), &file.rel_path, repo_uid);
        all_fates.extend(fates);
    }

    record_files_skipped_deep_nesting(
        storage,
        snapshot_uid,
        "policy_facts_files_skipped_deep_nesting",
        skipped_deep,
    )?;

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

/// Extractor tags stamped on every boundary-interaction surface, and the SINGLE
/// source of truth for them. Each is written by its postpass's `EmitterContext`
/// AND matched by that postpass's failure-isolation cleanup
/// (`delete_boundary_facts_by_extractor`). The writer and the deleter MUST agree:
/// a drifted literal would make the cleanup match the wrong rows (over- or
/// under-delete the postpass's own facts), so they share one constant each.
const C_BOUNDARY_EXTRACTOR: &str = "c-ipc:0.1.0";
const TS_BOUNDARY_EXTRACTOR: &str = "ts-worker:0.1.0";

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
/// BI-1B Phase 2: FD role tracking for TCP/UDP sockets.
/// Groups calls by enclosing function, tracks socket lineages, and refines
/// direction based on accumulated bind/listen/connect evidence.
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
    parser.set_language(&c_language).map_err(|e| {
        ComposeError::ExtractorInit(format!("boundary-interaction C parser: {}", e))
    })?;

    // Create emitter context.
    let context = EmitterContext {
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        extractor: C_BOUNDARY_EXTRACTOR.to_string(),
    };
    let mut emitter = BoundaryInteractionEmitter::new(context);
    // PERSIST-RECURSION-1: count files whose facts are skipped for pathological depth.
    let mut skipped_deep: u64 = 0;

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

        // PERSIST-RECURSION-1: skip pathologically deep files (honest degradation).
        if tree_exceeds_depth(&tree.root_node(), MAX_POSTPASS_TREE_DEPTH) {
            skipped_deep += 1;
            perf_log!(
                "[PERSIST-RECURSION-1] boundary-interaction: skipping deeply nested file {} (AST depth > {})",
                file.rel_path,
                MAX_POSTPASS_TREE_DEPTH
            );
            continue;
        }

        // Extract raw boundary calls.
        let raw_calls =
            extract_boundary_calls(&tree.root_node(), file.content.as_bytes(), &file.rel_path);

        // BI-1B Phase 2: Group calls by enclosing function for FD tracking.
        // Within each function, we track socket lineages and accumulate
        // bind/listen/connect evidence for role detection.
        let mut calls_by_function: std::collections::HashMap<String, Vec<_>> =
            std::collections::HashMap::new();
        for raw in raw_calls {
            calls_by_function
                .entry(raw.enclosing_function.clone())
                .or_default()
                .push(raw);
        }

        // Process each function's calls with FD registry for role tracking.
        for (_function_name, calls) in calls_by_function {
            let mut fd_registry = FdRegistry::new();

            for raw in calls {
                let callsite = convert_raw_to_callsite(&raw, &file.rel_path, repo_uid);

                // Process based on function type for FD tracking.
                match raw.function_name.as_str() {
                    "socket" => {
                        // Emit the socket surface.
                        match emitter.try_emit(&callsite) {
                            Ok(Some(facts)) => {
                                // BI-1B: If TCP/UDP with assigned identifier, register for tracking.
                                if let Some(id) = &raw.assigned_identifier {
                                    let kind = match facts.surface.channel_kind {
                                        ChannelKind::TcpSocket => {
                                            Some(TrackedChannelKind::TcpSocket)
                                        }
                                        ChannelKind::UdpSocket => {
                                            Some(TrackedChannelKind::UdpSocket)
                                        }
                                        _ => None,
                                    };
                                    if let Some(k) = kind {
                                        fd_registry.register_socket(
                                            id,
                                            k,
                                            &facts.surface.surface_uid,
                                        );
                                    }
                                }
                            }
                            Ok(None) => {} // No binding matched, fine.
                            Err(e) => {
                                return Err(ComposeError::Index(format!(
                                    "boundary-interaction emitter failed at {}:{}: {}",
                                    file.rel_path, raw.location.line_start, e
                                )));
                            }
                        }
                    }

                    "bind" => {
                        // BI-1B: Record bind evidence if fd is tracked.
                        if let Some(fd_arg) = &raw.fd_argument {
                            if fd_registry.is_tracked(fd_arg) {
                                fd_registry.record_bind(fd_arg);
                                // Don't emit - evidence only. Guard would reject anyway
                                // (no socket_type on bind callsite).
                                continue;
                            }
                        }
                        // Not tracked or no fd_arg - emit normally (Unix socket case).
                        if let Err(e) = emitter.try_emit(&callsite) {
                            return Err(ComposeError::Index(format!(
                                "boundary-interaction emitter failed at {}:{}: {}",
                                file.rel_path, raw.location.line_start, e
                            )));
                        }
                    }

                    "listen" => {
                        // BI-1B: Record listen evidence if fd is tracked.
                        if let Some(fd_arg) = &raw.fd_argument {
                            if fd_registry.is_tracked(fd_arg) {
                                fd_registry.record_listen(fd_arg);
                                continue; // Evidence only.
                            }
                        }
                        // Not tracked - emit normally.
                        if let Err(e) = emitter.try_emit(&callsite) {
                            return Err(ComposeError::Index(format!(
                                "boundary-interaction emitter failed at {}:{}: {}",
                                file.rel_path, raw.location.line_start, e
                            )));
                        }
                    }

                    "connect" => {
                        // BI-1B: Record connect evidence if fd is tracked.
                        if let Some(fd_arg) = &raw.fd_argument {
                            if fd_registry.is_tracked(fd_arg) {
                                fd_registry.record_connect(fd_arg);
                                continue; // Evidence only.
                            }
                        }
                        // Not tracked - emit normally (Unix socket case).
                        if let Err(e) = emitter.try_emit(&callsite) {
                            return Err(ComposeError::Index(format!(
                                "boundary-interaction emitter failed at {}:{}: {}",
                                file.rel_path, raw.location.line_start, e
                            )));
                        }
                    }

                    "accept" => {
                        // BI-1B: Record accept evidence if fd is tracked.
                        if let Some(fd_arg) = &raw.fd_argument {
                            if fd_registry.is_tracked(fd_arg) {
                                fd_registry.record_accept(fd_arg);
                                continue; // Evidence only.
                            }
                        }
                        // Not tracked - emit normally.
                        if let Err(e) = emitter.try_emit(&callsite) {
                            return Err(ComposeError::Index(format!(
                                "boundary-interaction emitter failed at {}:{}: {}",
                                file.rel_path, raw.location.line_start, e
                            )));
                        }
                    }

                    _ => {
                        // All other boundary calls: emit normally.
                        if let Err(e) = emitter.try_emit(&callsite) {
                            return Err(ComposeError::Index(format!(
                                "boundary-interaction emitter failed at {}:{}: {}",
                                file.rel_path, raw.location.line_start, e
                            )));
                        }
                    }
                }
            }

            // BI-1B: Function boundary - resolve directions and update surfaces.
            for (surface_uid, direction) in fd_registry.drain_for_refinement() {
                // Silently ignore update failures - the surface might have been
                // deduplicated or the direction is already correct. This is a
                // best-effort refinement, not a hard requirement.
                let _ = emitter.update_surface_direction(&surface_uid, direction);
            }
        }
    }

    record_files_skipped_deep_nesting(
        storage,
        snapshot_uid,
        "boundary_facts_files_skipped_deep_nesting",
        skipped_deep,
    )?;

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
fn convert_raw_to_callsite(
    raw: &RawBoundaryCall,
    file_path: &str,
    repo_uid: &str,
) -> BoundaryCallsite {
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
        location: raw.location,
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
        extractor: TS_BOUNDARY_EXTRACTOR.to_string(),
    };
    let mut emitter = BoundaryInteractionEmitter::new(context);
    // PERSIST-RECURSION-1: count files whose facts are skipped for pathological depth.
    let mut skipped_deep: u64 = 0;

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

        parser.set_language(language).map_err(|e| {
            ComposeError::ExtractorInit(format!("boundary-interaction TS parser: {}", e))
        })?;

        // Parse the file.
        let tree = match parser.parse(&file.content, None) {
            Some(t) => t,
            None => continue, // Parse failed, skip.
        };

        // PERSIST-RECURSION-1: skip pathologically deep files (honest
        // degradation). Guards ALL of this postpass's walks — SAB/Atomics,
        // AMQP, Kafka, NATS — before any of them descends the file.
        if tree_exceeds_depth(&tree.root_node(), MAX_POSTPASS_TREE_DEPTH) {
            skipped_deep += 1;
            perf_log!(
                "[PERSIST-RECURSION-1] ts-boundary-interaction: skipping deeply nested file {} (AST depth > {})",
                file.rel_path,
                MAX_POSTPASS_TREE_DEPTH
            );
            continue;
        }

        // Extract SharedArrayBuffer/Atomics boundary calls (BI-1C).
        let raw_calls =
            extract_ts_boundary_calls(&tree.root_node(), file.content.as_bytes(), &file.rel_path);

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
        let amqp_calls =
            extract_amqp_boundary_calls(&tree.root_node(), file.content.as_bytes(), &file.rel_path);

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
        let nats_calls =
            extract_nats_boundary_calls(&tree.root_node(), file.content.as_bytes(), &file.rel_path);

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

    record_files_skipped_deep_nesting(
        storage,
        snapshot_uid,
        "ts_boundary_facts_files_skipped_deep_nesting",
        skipped_deep,
    )?;

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
fn convert_ts_raw_to_callsite(
    raw: &RawTsBoundaryCall,
    file_path: &str,
    repo_uid: &str,
) -> BoundaryCallsite {
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
        location: raw.location,
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
fn convert_amqp_raw_to_callsite(
    raw: &RawAmqpBoundaryCall,
    file_path: &str,
    repo_uid: &str,
) -> BoundaryCallsite {
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
        location: raw.location,
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
fn convert_kafka_raw_to_callsite(
    raw: &RawKafkaBoundaryCall,
    file_path: &str,
    repo_uid: &str,
) -> BoundaryCallsite {
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
    let extracted_argument = raw
        .topic
        .clone()
        .or_else(|| raw.topics.as_ref().and_then(|t| t.first().cloned()));

    BoundaryCallsite {
        language: BiLanguage::TypeScript,
        function_name: raw.function_name.clone(),
        location: raw.location,
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
fn convert_nats_raw_to_callsite(
    raw: &RawNatsBoundaryCall,
    file_path: &str,
    repo_uid: &str,
) -> BoundaryCallsite {
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
        location: raw.location,
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

// ── Express route surface persistence (FD-1A) ─────────────────────

/// FD-1A: Detect Express routes in TS/JS files and persist as http_provider surfaces.
///
/// Re-parses TS/JS files with tree-sitter to detect route registrations
/// (app.get, router.post, etc.) and persists them to project_surfaces.
///
/// **Requires:** npm modules must be persisted first (FK constraint).
///
/// Returns the number of surfaces persisted.
fn persist_express_surfaces(
    storage: &mut StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
    file_inputs: &[FileInput],
    npm_extraction: &NpmExtractionResult,
) -> Result<usize, ComposeError> {
    // Early exit if no npm modules to resolve against.
    if npm_extraction.modules.is_empty() {
        return Ok(0);
    }

    // Detect Express routes. PERSIST-RECURSION-1: `detect_express_routes` applies
    // the per-file depth guard internally (it owns the parse) and reports how many
    // files it skipped for pathological nesting; record that honestly (before any
    // early return) so the skip surfaces to the reader even if no route resolved.
    let crate::express_detector::DetectedRoutes {
        routes,
        files_skipped_deep_nesting,
    } = detect_express_routes(file_inputs);
    record_files_skipped_deep_nesting(
        storage,
        snapshot_uid,
        "express_surface_facts_files_skipped_deep_nesting",
        files_skipped_deep_nesting,
    )?;
    if routes.is_empty() {
        return Ok(0);
    }

    // Build module resolver: file_path → module_candidate_uid
    // Sorted by path length descending for longest-prefix match.
    // Using owned Strings to avoid lifetime issues with the closure.
    let mut sorted_modules: Vec<(String, String)> = npm_extraction
        .modules
        .iter()
        .map(|m| {
            let uid = package_json::generate_module_uid(repo_uid, &m.module.package_root);
            (m.module.package_root.clone(), uid)
        })
        .collect();
    sorted_modules.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    // Resolver closure: finds longest-prefix-matching module.
    // Uses directory-boundary-safe matching (same logic as compute_cargo_file_ownership).
    let resolve_module = move |file_path: &str| -> Option<String> {
        for (root, uid) in &sorted_modules {
            // Root package "." matches everything.
            if root == "." {
                return Some(uid.clone());
            }
            // Directory-boundary-safe prefix match:
            // "packages/app" must NOT match "packages/app2/file.ts"
            // Only matches "packages/app/file.ts" or "packages/app" exactly.
            if file_path == root || file_path.starts_with(&format!("{}/", root)) {
                return Some(uid.clone());
            }
        }
        None
    };

    // Convert routes to surfaces, keeping track of which routes converted.
    // routes_to_surfaces uses filter_map, so we need to track indices.
    let mut surfaces = Vec::new();
    let mut converted_routes = Vec::new();
    let mut seen_stable_keys = std::collections::HashSet::new();
    for route in &routes {
        if let Some(surface) = crate::express_detector::route_to_surface_with_resolver(
            route,
            snapshot_uid,
            repo_uid,
            &resolve_module,
        ) {
            // Deduplicate by stable_surface_key to avoid unique constraint violation.
            // This can happen when the same route is defined in multiple AST patterns
            // that the detector recognizes.
            if seen_stable_keys.insert(surface.stable_surface_key.clone()) {
                surfaces.push(surface);
                converted_routes.push(route);
            }
        }
    }

    if surfaces.is_empty() {
        return Ok(0);
    }

    let count = surfaces.len();

    // Insert surfaces and get generated UIDs.
    let surface_uids = storage
        .insert_project_surfaces_batch(&surfaces)
        .map_err(|e| ComposeError::Index(format!("express-surfaces storage: {}", e)))?;

    // Build evidence records for each surface.
    // converted_routes and surface_uids are aligned (same order).
    let evidence: Vec<repo_graph_storage::types::CreateProjectSurfaceEvidenceInput> = surface_uids
        .iter()
        .zip(converted_routes.iter())
        .map(
            |(uid, route)| repo_graph_storage::types::CreateProjectSurfaceEvidenceInput {
                project_surface_uid: uid.clone(),
                snapshot_uid: snapshot_uid.to_string(),
                repo_uid: repo_uid.to_string(),
                source_type: "code_detection".to_string(),
                source_path: route.file_path.clone(),
                evidence_kind: "route_registration".to_string(),
                confidence: route.confidence,
                payload_json: Some(
                    serde_json::json!({
                        "method": route.http_method,
                        "path": route.path,
                        "receiver": route.receiver,
                        "lineStart": route.line_start,
                    })
                    .to_string(),
                ),
            },
        )
        .collect();

    // Insert evidence records.
    if !evidence.is_empty() {
        storage
            .insert_project_surface_evidence_batch(&evidence)
            .map_err(|e| ComposeError::Index(format!("express-evidence storage: {}", e)))?;
    }

    Ok(count)
}

// ── React inference persistence (FD-1B) ───────────────────────────

/// FD-1B: Detect React components and hooks in TSX/JSX files and persist as inferences.
///
/// Re-parses TSX/JSX files with tree-sitter to detect:
/// - React component definitions (PascalCase functions returning JSX)
/// - React hook usage (useState, useEffect, custom hooks)
///
/// Persists to `inferences` table with kinds:
/// - `react_component` — component definition evidence
/// - `react_hook_usage` — hook call evidence
///
/// Returns the total number of inferences persisted.
fn persist_react_inferences(
    storage: &mut StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
    file_inputs: &[FileInput],
) -> Result<usize, ComposeError> {
    // Detect components and hooks. PERSIST-RECURSION-1: each detector applies the
    // per-file depth guard internally (they own the parse) and returns the PATHS of
    // the files it skipped. React runs TWO passes over the same files (components +
    // hooks, different gates), so a single ultra-deep .tsx can be skipped by both.
    // We UNION the two path lists and record the count of DISTINCT files skipped — a
    // file skipped by both passes is one skipped file, not two (review-2 item 1;
    // the reader frame says "React inferences skipped for N files"). Recorded before
    // any early return so the skip surfaces even if no inference resolved.
    let crate::react_detector::DetectedComponents {
        components,
        files_skipped_deep_nesting: components_skipped_deep,
    } = detect_react_components(file_inputs);
    let crate::react_detector::DetectedHooks {
        hooks,
        files_skipped_deep_nesting: hooks_skipped_deep,
    } = detect_react_hooks(file_inputs);
    let distinct_skipped_deep: std::collections::BTreeSet<String> = components_skipped_deep
        .into_iter()
        .chain(hooks_skipped_deep)
        .collect();
    record_files_skipped_deep_nesting(
        storage,
        snapshot_uid,
        "react_inference_facts_files_skipped_deep_nesting",
        distinct_skipped_deep.len() as u64,
    )?;

    if components.is_empty() && hooks.is_empty() {
        return Ok(0);
    }

    // Convert to inference inputs.
    let component_inferences = components_to_inferences(&components, snapshot_uid, repo_uid);
    let hook_inferences = hooks_to_inferences(&hooks, snapshot_uid, repo_uid);

    let total_count = component_inferences.len() + hook_inferences.len();

    // Combine all inferences.
    let mut all_inferences = component_inferences;
    all_inferences.extend(hook_inferences);

    // Replace existing React inferences for this snapshot (idempotent re-index).
    storage
        .replace_inferences_by_kind(
            snapshot_uid,
            &["react_component", "react_hook_usage"],
            &all_inferences,
        )
        .map_err(|e| ComposeError::Index(format!("react-inferences storage: {}", e)))?;

    Ok(total_count)
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
        let (candidate, ev) =
            cargo_manifest::to_storage_inputs(&extracted.module, repo_uid, snapshot_uid);
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
    // ORIENT-BUG-1: Only Rust files should be owned by Cargo modules.
    let rust_files: Vec<_> = file_inputs
        .iter()
        .filter(|f| {
            let lang = routing::detect_language(&f.rel_path);
            matches!(lang, Some("rust"))
        })
        .cloned()
        .collect();

    let ownership = compute_cargo_file_ownership(repo_uid, snapshot_uid, &candidates, &rust_files);

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
        .map(|c| {
            (
                c.canonical_root_path.as_str(),
                c.module_candidate_uid.as_str(),
            )
        })
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
                file.rel_path == *root_path || file.rel_path.starts_with(&format!("{}/", root_path))
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
        let (candidate, ev) =
            package_json::to_storage_inputs(&extracted.module, repo_uid, snapshot_uid);
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

    let ownership = compute_cargo_file_ownership(repo_uid, snapshot_uid, &candidates, &js_ts_files);

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
        let (candidate, ev) =
            pyproject::to_storage_inputs(&extracted.module, repo_uid, snapshot_uid);
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

    let ownership =
        compute_cargo_file_ownership(repo_uid, snapshot_uid, &candidates, &python_files);

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
        let (candidate, ev) =
            settings_gradle::to_storage_inputs(&extracted.module, repo_uid, snapshot_uid);
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

    let ownership = compute_cargo_file_ownership(repo_uid, snapshot_uid, &candidates, &jvm_files);

    if !ownership.is_empty() {
        storage
            .insert_file_ownership(&ownership)
            .map_err(ComposeError::Storage)?;
    }

    Ok(candidate_count)
}

// ── Inferred module persistence (rust-module-parity Phase 3) ───

/// Persist inferred module candidates, evidence, and file ownership.
///
/// Phase 3: persists module rows and ownership assignments for inferred modules.
/// - Top-level directories containing source files
/// - File ownership via longest-prefix-match (uncovered files only)
///
/// ORIENT-BUG-1: Only compute ownership for files NOT covered by declared modules.
/// This prevents duplicate ownership between declared and inferred modules.
fn persist_inferred_modules(
    storage: &mut StorageConnection,
    repo_uid: &str,
    snapshot_uid: &str,
    inferred_extraction: &InferredExtractionResult,
    file_inputs: &[FileInput],
    declared_roots: &[DeclaredRoot],
) -> Result<usize, ComposeError> {
    if inferred_extraction.modules.is_empty() {
        return Ok(0);
    }

    // Convert extracted modules to storage input DTOs.
    let mut candidates: Vec<CargoModuleCandidateInput> = Vec::new();
    let mut evidence: Vec<CargoModuleEvidenceInput> = Vec::new();

    for extracted in &inferred_extraction.modules {
        let (candidate, ev) =
            inferred_modules::to_storage_inputs(&extracted.module, repo_uid, snapshot_uid);
        candidates.push(candidate);
        evidence.push(ev);
    }

    // Persist using the storage port (same methods as declared modules).
    let candidate_count = storage
        .insert_cargo_module_candidates(&candidates)
        .map_err(ComposeError::Storage)?;
    let _evidence_count = storage
        .insert_cargo_module_evidence(&evidence)
        .map_err(ComposeError::Storage)?;

    // Compute and persist file ownership.
    // ORIENT-BUG-1: Only compute ownership for files NOT covered by declared modules.
    // This prevents a .rs file under a Cargo module from also being owned by an inferred module.
    let uncovered_files: Vec<FileInput> = file_inputs
        .iter()
        .filter(|f| !is_file_covered_by_roots(&f.rel_path, declared_roots))
        .cloned()
        .collect();

    let ownership =
        compute_cargo_file_ownership(repo_uid, snapshot_uid, &candidates, &uncovered_files);

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
    // PERF-INSTRUMENTATION-1: capture the runtime gate once. Every marker below
    // self-gates; when RMAP_PERF is off the only cost is the atomic load already
    // paid here plus the per-phase `Instant`s (negligible).
    let perf_on = crate::perf::perf_enabled();
    let perf_files = crate::perf::perf_file_enabled();
    let index_start = Instant::now();

    perf_log!("[PERF] index {}: > discover", repo_uid);
    emit_progress(&mut progress, "scanning", 0, 1)?;
    let discover_start = Instant::now();
    let prepared = prepare_repo_inputs(repo_path)?;
    let discover_ms = discover_start.elapsed().as_millis();
    let discover_files = prepared.file_inputs.len();
    emit_progress(&mut progress, "scanning", 1, 1)?;

    perf_log!("[PERF] index {}: > init", repo_uid);
    let init_start = Instant::now();
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
    let init_ms = init_start.elapsed().as_millis();

    // Checkpoint BEFORE repo mutation — abort here if transport failed
    emit_progress(&mut progress, "initializing", 0, 1)?;
    ensure_repo(storage, repo_uid, repo_path, options)?;

    let mut extractors: Vec<&mut dyn ExtractorPort> = vec![
        &mut ts_extractor,
        &mut c_extractor,
        &mut cpp_extractor,
        &mut java_extractor,
        &mut python_extractor,
        &mut rust_extractor,
    ];

    // State-boundary hook: wired at the composition root (SB-4-pre).
    // Constructs the hook; on invalid repo_uid it degrades
    // gracefully (diagnostic, no emission, no abort).
    let mut sb_hook = crate::state_boundary_hook::StateBoundaryHook::new(repo_uid);

    // PERF-INSTRUMENTATION-1: emit phase-ENTRY + (level-2) per-file markers from
    // the orchestrator's progress stream. The phase DURATIONS are NOT derived
    // here — they are measured at the real storage-write boundaries inside the
    // orchestrator and returned on `result.phase_timings` (the only layer that
    // can see those boundaries). Scoped so the marker borrow is released before
    // `result` is read for the summary.
    let mut markers =
        crate::perf::IndexProgressMarkers::new(repo_uid, discover_files as u64, perf_files);

    // Bridge the compose progress callback to the indexer callback.
    // The indexer emits per-file extracting progress with abort checkpoints.
    // IndexError::Aborted maps to ComposeError::Aborted for transport failure.
    let mut result = {
        let mut indexer_progress_callback = |event: &IndexProgressEvent| -> ControlFlow<()> {
            if perf_on {
                markers.observe(event);
            }
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

        match orchestrator::index_repo(
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
        }
    };

    // Persisting phase: checkpoint BEFORE each mutation (7 mutations total)
    // Semantics: current=N means "about to do mutation N"

    perf_log!("[PERF] index {}: > postpass", repo_uid);
    let postpass_start = Instant::now();
    emit_progress(&mut progress, "persisting", 0, 8)?; // about to persist read failures
    persist_read_failures(
        storage,
        repo_uid,
        &result.snapshot_uid.clone(),
        &prepared.read_failed_paths,
        &mut result,
    )?;

    emit_progress(&mut progress, "persisting", 1, 8)?; // about to persist config file versions
                                                       // Persist config file versions for refresh invalidation tracking.
                                                       // Config files are NOT extracted — only tracked for hash comparison.
    persist_config_file_versions(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.config_file_inputs,
    )?;

    emit_progress(&mut progress, "persisting", 2, 8)?; // about to persist metrics
                                                       // RS-MS-3c-prereq: Persist metrics (complexity, params, nesting).
    persist_metrics(storage, repo_uid, &result.snapshot_uid, &result.metrics)?;

    emit_progress(&mut progress, "persisting", 3, 8)?; // about to persist spring liveness
                                                       // Persist Spring framework-liveness inferences for dead-code suppression.
                                                       // Full index mode: process all nodes, replace all Spring inferences.
    persist_spring_liveness_inferences(storage, repo_uid, &result.snapshot_uid, None)?;

    emit_progress(&mut progress, "persisting", 4, 8)?; // about to persist policy facts
                                                       // PF-1: Extract and persist STATUS_MAPPING policy facts from C files.
                                                       // TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
    let policy_facts_outcome = persist_policy_facts(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.file_inputs,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "policy-facts",
        "policy_facts_postpass_error",
        policy_facts_outcome,
        // Compensating cleanup: policy facts span three tables via three
        // separately-committing writers — clear all three for the snapshot.
        |s| {
            s.delete_policy_facts(&result.snapshot_uid)
                .map(|_| ())
                .map_err(ComposeError::Storage)
        },
    )?;

    emit_progress(&mut progress, "persisting", 5, 8)?; // about to persist C boundary interactions
                                                       // BI-1A: Extract and persist boundary interaction facts from C files.
                                                       // TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
    let boundary_outcome = persist_boundary_interactions(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.file_inputs,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "boundary-interaction",
        "boundary_facts_postpass_error",
        boundary_outcome,
        // Compensating cleanup: the boundary writer inserts surfaces then channels
        // as separate autocommitting statements, so a failure can leave a partial
        // subset. Scope the delete to THIS postpass's own extractor — C and TS
        // boundary facts share this table, so a snapshot-wide delete would erase the
        // sibling postpass's already-committed facts.
        |s| {
            s.delete_boundary_facts_by_extractor(&result.snapshot_uid, C_BOUNDARY_EXTRACTOR)
                .map(|_| ())
                .map_err(ComposeError::Storage)
        },
    )?;

    emit_progress(&mut progress, "persisting", 6, 8)?; // about to persist TS boundary interactions
                                                       // BI-1C: Extract and persist boundary interaction facts from TS/JS files.
                                                       // SharedArrayBuffer, Worker, postMessage, Atomics patterns.
    let ts_boundary_outcome = persist_ts_boundary_interactions(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.file_inputs,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "ts-boundary-interaction",
        "ts_boundary_facts_postpass_error",
        ts_boundary_outcome,
        // Compensating cleanup: same table as the C postpass — scope to THIS
        // postpass's extractor so a TS failure never deletes the C boundary facts.
        |s| {
            s.delete_boundary_facts_by_extractor(&result.snapshot_uid, TS_BOUNDARY_EXTRACTOR)
                .map(|_| ())
                .map_err(ComposeError::Storage)
        },
    )?;

    emit_progress(&mut progress, "persisting", 7, 9)?; // about to persist Cargo modules
                                                       // rust-module-parity Phase 1.5: Persist Cargo.toml-derived module candidates and file ownership.
    persist_cargo_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.cargo_modules,
        &prepared.file_inputs,
    )?;

    emit_progress(&mut progress, "persisting", 8, 10)?; // about to persist npm modules
                                                        // rust-module-parity Phase 2: Persist package.json-derived module candidates and file ownership.
    persist_npm_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.npm_modules,
        &prepared.file_inputs,
    )?;

    // FD-1A: Extract and persist Express route surfaces from TS/JS files.
    // Must come after npm modules are persisted (FK constraint on module_candidate_uid).
    // PERSIST-RECURSION-1 item 3: a re-parse postpass failure never aborts the index.
    let express_outcome = persist_express_surfaces(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.file_inputs,
        &prepared.npm_modules,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "express-surfaces",
        "express_surface_facts_postpass_error",
        express_outcome,
        // Compensating cleanup: surfaces are inserted then evidence keyed by them —
        // clear both (evidence deleted explicitly, then surfaces).
        |s| {
            s.delete_project_surface_facts(&result.snapshot_uid)
                .map(|_| ())
                .map_err(ComposeError::Storage)
        },
    )?;

    // FD-1B: Extract and persist React component/hook inferences from TSX/JSX files.
    let react_outcome = persist_react_inferences(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.file_inputs,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "react-inferences",
        "react_inference_facts_postpass_error",
        react_outcome,
        // Compensating cleanup: React writes inferences of two kinds — clear both.
        |s| {
            s.delete_inferences_by_kind(
                &result.snapshot_uid,
                &["react_component", "react_hook_usage"],
            )
            .map(|_| ())
            .map_err(ComposeError::Storage)
        },
    )?;

    emit_progress(&mut progress, "persisting", 9, 11)?; // about to persist pyproject modules
                                                        // rust-module-parity Phase 2c: Persist pyproject.toml-derived module candidates and file ownership.
    persist_pyproject_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.pyproject_modules,
        &prepared.file_inputs,
    )?;

    emit_progress(&mut progress, "persisting", 10, 12)?; // about to persist Gradle modules
                                                         // rust-module-parity Phase 2b: Persist settings.gradle-derived module candidates and file ownership.
    persist_gradle_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.gradle_modules,
        &prepared.file_inputs,
    )?;

    emit_progress(&mut progress, "persisting", 11, 12)?; // about to persist inferred modules
                                                         // rust-module-parity Phase 3: Persist inferred module candidates and file ownership.
                                                         // ORIENT-BUG-1: Compute declared roots to prevent duplicate ownership.
    let declared_roots = collect_declared_module_roots(
        &prepared.cargo_modules,
        &prepared.npm_modules,
        &prepared.pyproject_modules,
        &prepared.gradle_modules,
    );
    persist_inferred_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.inferred_modules,
        &prepared.file_inputs,
        &declared_roots,
    )?;

    // PERF-INSTRUMENTATION-1: one-line per-phase summary for the whole index.
    // `extract`/`resolve`/`store`/`finalize` are the orchestrator's REAL
    // measurements (`store` = the actual storage write calls); `discover`/`init`/
    // `postpass` are these compose-level windows; counts come from the result.
    // Formatting lives in `crate::perf` (out of this 4000-line file).
    let postpass_ms = postpass_start.elapsed().as_millis();
    let total_ms = index_start.elapsed().as_millis();
    perf_log!(
        "{}",
        crate::perf::format_index_summary(
            repo_uid,
            discover_ms,
            discover_files,
            init_ms,
            &result.phase_timings,
            postpass_ms,
            total_ms,
            &crate::perf::IndexCounts {
                nodes: result.nodes_total,
                edges: result.edges_total,
                unresolved: result.edges_unresolved,
            },
        )
    );

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
// RMAPD-PERF-2: Allow unused timing variables when perf-trace feature is disabled.
#[allow(unused_variables)]
pub fn refresh_into_storage_with_progress(
    repo_path: &Path,
    storage: &mut StorageConnection,
    repo_uid: &str,
    options: &ComposeOptions,
    mut progress: Option<ProgressCallback<'_>>,
) -> Result<IndexResult, ComposeError> {
    // RMAPD-PERF-2: Timing instrumentation for refresh regression diagnosis
    let refresh_start = Instant::now();

    emit_progress(&mut progress, "scanning", 0, 1)?;
    let scan_start = Instant::now();
    let prepared = prepare_repo_inputs(repo_path)?;
    let scan_ms = scan_start.elapsed().as_millis();
    perf_log!(
        "[PERF] refresh: scan={}ms files={}",
        scan_ms,
        prepared.file_inputs.len()
    );
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

    let mut extractors: Vec<&mut dyn ExtractorPort> = vec![
        &mut ts_extractor,
        &mut c_extractor,
        &mut cpp_extractor,
        &mut java_extractor,
        &mut python_extractor,
        &mut rust_extractor,
    ];

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
    // RMAPD-PERF-2: Time the core refresh
    let refresh_core_start = Instant::now();
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
    let refresh_core_ms = refresh_core_start.elapsed().as_millis();
    perf_log!("[PERF] refresh: core_refresh={}ms", refresh_core_ms);

    // RMAPD-PERF-2: Time copy-forward phase
    let copy_forward_start = Instant::now();

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

    let unchanged_file_set: std::collections::HashSet<&str> = result
        .unchanged_files
        .as_ref()
        .map(|files| files.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();
    perf_log!(
        "[PERF] refresh: unchanged_set built, count={}",
        unchanged_file_set.len()
    );

    if let (Some(parent_uid), Some(unchanged_files)) =
        (&result.parent_snapshot_uid, &result.unchanged_files)
    {
        perf_log!(
            "[PERF] refresh: entering copy-forward, unchanged_files={}",
            unchanged_files.len()
        );
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
        let copy_loop_start = Instant::now();
        for family in COPY_FORWARD_FAMILIES {
            let contract = get_contract(*family);

            let (action, rows) = match contract.refresh_policy {
                RefreshPolicy::ReextractChangedInputs
                | RefreshPolicy::MarkImpactedDeferRecompute => {
                    // Unchanged branch: copy forward from parent snapshot
                    match family {
                        ArtifactFamily::Measurements => {
                            let t = Instant::now();
                            let n = storage
                                .copy_forward_measurements(
                                    parent_uid,
                                    &result.snapshot_uid,
                                    repo_uid,
                                    unchanged_files,
                                )
                                .map_err(ComposeError::Storage)?;
                            perf_log!(
                                "[PERF] refresh: copy_measurements={}ms copied={}",
                                t.elapsed().as_millis(),
                                n
                            );
                            measurements_copied = n;
                            (RefreshAction::CopiedForward, Some(n as usize))
                        }
                        ArtifactFamily::Inferences => {
                            let t = Instant::now();
                            let n = storage
                                .copy_forward_inferences(
                                    parent_uid,
                                    &result.snapshot_uid,
                                    repo_uid,
                                    unchanged_files,
                                )
                                .map_err(ComposeError::Storage)?;
                            perf_log!(
                                "[PERF] refresh: copy_inferences={}ms copied={}",
                                t.elapsed().as_millis(),
                                n
                            );
                            inferences_copied = n;
                            (RefreshAction::CopiedForward, Some(n as usize))
                        }
                        ArtifactFamily::BoundaryInteractionSurfaces => {
                            let t = Instant::now();
                            let (surfaces, channels) = storage
                                .copy_forward_boundary_surfaces(
                                    parent_uid,
                                    &result.snapshot_uid,
                                    unchanged_files,
                                )
                                .map_err(ComposeError::Storage)?;
                            perf_log!(
                                "[PERF] refresh: copy_boundaries={}ms surfaces={} channels={}",
                                t.elapsed().as_millis(),
                                surfaces,
                                channels
                            );
                            boundary_surfaces_copied = surfaces;
                            boundary_channels_copied = channels;
                            (
                                RefreshAction::CopiedForward,
                                Some((surfaces + channels) as usize),
                            )
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
                RefreshPolicy::SnapshotIndependent => (RefreshAction::Skipped, None),
                _ => (RefreshAction::NotImplemented, None),
            };

            diagnostics.record(FamilyRefreshResult {
                family: *family,
                policy: contract.refresh_policy,
                action,
                rows_affected: rows,
            });
        }
        let copy_loop_ms = copy_loop_start.elapsed().as_millis();
        perf_log!("[PERF] refresh: copy_loop={}ms", copy_loop_ms);

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
        let impact_start = Instant::now();
        let changed_file_paths: Vec<&str> = prepared
            .file_inputs
            .iter()
            .map(|f| f.rel_path.as_str())
            .filter(|path| !unchanged_file_set.contains(*path))
            .collect();
        perf_log!(
            "[PERF] refresh: changed_file_paths={}",
            changed_file_paths.len()
        );

        let changed_stable_keys: Vec<String> = if changed_file_paths.is_empty() {
            perf_log!("[PERF] refresh: skipping query_all_nodes (no changed files)");
            Vec::new()
        } else {
            // Query all nodes for the snapshot and filter to those from changed files.
            // Node stable_keys embed the file path with specific delimiters:
            // - SYMBOL nodes: "repo:path#symbol:SYMBOL:type" (# after path)
            // - FILE nodes: "repo:path:FILE" (exact match)
            //
            // We must check for the delimiter to avoid false-matching path prefixes
            // (e.g., "src/A.java" should not match "src/A.javax/Foo.java").
            perf_log!("[PERF] refresh: querying all nodes...");
            let query_start = Instant::now();
            let all_nodes = storage
                .query_all_nodes(&result.snapshot_uid)
                .map_err(ComposeError::Storage)?;
            let query_ms = query_start.elapsed().as_millis();
            perf_log!(
                "[PERF] refresh: query_all_nodes={}ms nodes={}",
                query_ms,
                all_nodes.len()
            );

            let filter_start = Instant::now();
            let result: Vec<String> = all_nodes
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
                .collect();
            let filter_ms = filter_start.elapsed().as_millis();
            perf_log!(
                "[PERF] refresh: filter_nodes={}ms matched={}",
                filter_ms,
                result.len()
            );
            result
        };

        // Propagate impact to derived artifacts whose provenance references changed stable keys
        perf_log!("[PERF] refresh: propagating impact...");
        let prop_start = Instant::now();
        let _impact_report: ImpactReport =
            propagate_impact(storage, &result.snapshot_uid, &changed_stable_keys)
                .map_err(ComposeError::Storage)?;
        let prop_ms = prop_start.elapsed().as_millis();
        let impact_ms = impact_start.elapsed().as_millis();
        perf_log!(
            "[PERF] refresh: propagate_impact={}ms total_impact={}ms",
            prop_ms,
            impact_ms
        );

        // Impact report available for future diagnostics integration.
        // Fields: total_impacted(), get(family) per artifact family.
        // TODO: Include impact_report in result diagnostics (ACR-4 follow-on)
    }
    let copy_forward_ms = copy_forward_start.elapsed().as_millis();
    perf_log!("[PERF] refresh: copy_forward_impact={}ms", copy_forward_ms);

    // RMAPD-PERF-2: Time postpass and module persistence phase
    let postpass_start = Instant::now();

    // Filter file inputs to only changed files for postpass extraction.
    // Unchanged files already have their artifacts from copy-forward.
    let changed_files_owned: Vec<FileInput> = prepared
        .file_inputs
        .iter()
        .filter(|f| !unchanged_file_set.contains(f.rel_path.as_str()))
        .cloned()
        .collect();

    // Persisting phase: checkpoint BEFORE each mutation (7 mutations total)
    // Semantics: current=N means "about to do mutation N"

    emit_progress(&mut progress, "persisting", 0, 8)?; // about to persist read failures
    persist_read_failures(
        storage,
        repo_uid,
        &result.snapshot_uid.clone(),
        &prepared.read_failed_paths,
        &mut result,
    )?;

    emit_progress(&mut progress, "persisting", 1, 8)?; // about to persist config file versions
                                                       // Persist config file versions for refresh invalidation tracking.
                                                       // Config files are NOT extracted — only tracked for hash comparison.
    persist_config_file_versions(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.config_file_inputs,
    )?;

    emit_progress(&mut progress, "persisting", 2, 8)?; // about to persist metrics
                                                       // RS-MS-3c-prereq: Persist metrics (complexity, params, nesting).
                                                       // Only for changed files; unchanged file metrics already copied forward.
    persist_metrics(storage, repo_uid, &result.snapshot_uid, &result.metrics)?;

    emit_progress(&mut progress, "persisting", 3, 8)?; // about to persist spring liveness
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

    emit_progress(&mut progress, "persisting", 4, 8)?; // about to persist policy facts
                                                       // PF-1: Extract and persist STATUS_MAPPING policy facts from C files.
                                                       // TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
                                                       // Only extract from changed files; unchanged files copied forward.
    let policy_facts_outcome = persist_policy_facts(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &changed_files_owned,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "policy-facts",
        "policy_facts_postpass_error",
        policy_facts_outcome,
        // Compensating cleanup: policy facts span three tables via three
        // separately-committing writers — clear all three for the snapshot.
        |s| {
            s.delete_policy_facts(&result.snapshot_uid)
                .map(|_| ())
                .map_err(ComposeError::Storage)
        },
    )?;

    emit_progress(&mut progress, "persisting", 5, 8)?; // about to persist C boundary interactions
                                                       // BI-1A: Extract and persist boundary interaction facts from C files.
                                                       // TEMPORARY re-parse postpass; see docs/TECH-DEBT.md.
                                                       // Only extract from changed files; unchanged files copied forward.
    let boundary_outcome = persist_boundary_interactions(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &changed_files_owned,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "boundary-interaction",
        "boundary_facts_postpass_error",
        boundary_outcome,
        // Compensating cleanup: the boundary writer inserts surfaces then channels
        // as separate autocommitting statements, so a failure can leave a partial
        // subset. Scope the delete to THIS postpass's own extractor — C and TS
        // boundary facts share this table, so a snapshot-wide delete would erase the
        // sibling postpass's already-committed facts.
        |s| {
            s.delete_boundary_facts_by_extractor(&result.snapshot_uid, C_BOUNDARY_EXTRACTOR)
                .map(|_| ())
                .map_err(ComposeError::Storage)
        },
    )?;

    emit_progress(&mut progress, "persisting", 6, 8)?; // about to persist TS boundary interactions
                                                       // BI-1C: Extract and persist boundary interaction facts from TS/JS files.
                                                       // SharedArrayBuffer, Worker, postMessage, Atomics patterns.
                                                       // Only extract from changed files; unchanged files copied forward.
    let ts_boundary_outcome = persist_ts_boundary_interactions(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &changed_files_owned,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "ts-boundary-interaction",
        "ts_boundary_facts_postpass_error",
        ts_boundary_outcome,
        // Compensating cleanup: same table as the C postpass — scope to THIS
        // postpass's extractor so a TS failure never deletes the C boundary facts.
        |s| {
            s.delete_boundary_facts_by_extractor(&result.snapshot_uid, TS_BOUNDARY_EXTRACTOR)
                .map(|_| ())
                .map_err(ComposeError::Storage)
        },
    )?;

    let early_postpass_ms = postpass_start.elapsed().as_millis();
    perf_log!("[PERF] refresh: early_postpass={}ms", early_postpass_ms);

    // RMAPD-PERF-2: Time module persistence phase
    let module_persist_start = Instant::now();

    emit_progress(&mut progress, "persisting", 7, 8)?; // about to persist Cargo modules
                                                       // rust-module-parity Phase 1.5: Persist Cargo.toml-derived module candidates and file ownership.
                                                       // Always recompute from current prepared inputs (Cargo.toml content).
                                                       // Cargo.toml changes trigger config-file invalidation, so recompute is correct.
                                                       // Ownership is recomputed for all files to maintain consistency.
    persist_cargo_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.cargo_modules,
        &prepared.file_inputs,
    )?;

    // rust-module-parity Phase 2: Persist package.json-derived module candidates and file ownership.
    // Same recompute semantics as Cargo.
    persist_npm_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.npm_modules,
        &prepared.file_inputs,
    )?;

    // FD-1A: Extract and persist Express route surfaces from TS/JS files.
    // Only extract from changed files; unchanged files copied forward.
    // Must come after npm modules are persisted (FK constraint on module_candidate_uid).
    // PERSIST-RECURSION-1 item 3: a re-parse postpass failure never aborts the index.
    let express_outcome = persist_express_surfaces(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &changed_files_owned,
        &prepared.npm_modules,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "express-surfaces",
        "express_surface_facts_postpass_error",
        express_outcome,
        // Compensating cleanup: surfaces are inserted then evidence keyed by them —
        // clear both (evidence deleted explicitly, then surfaces).
        |s| {
            s.delete_project_surface_facts(&result.snapshot_uid)
                .map(|_| ())
                .map_err(ComposeError::Storage)
        },
    )?;

    // FD-1B: Extract and persist React component/hook inferences from TSX/JSX files.
    // Only extract from changed files; unchanged files copied forward via inference copy-forward.
    let react_outcome = persist_react_inferences(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &changed_files_owned,
    );
    isolate_postpass(
        storage,
        &result.snapshot_uid,
        "react-inferences",
        "react_inference_facts_postpass_error",
        react_outcome,
        // Compensating cleanup: React writes inferences of two kinds — clear both.
        |s| {
            s.delete_inferences_by_kind(
                &result.snapshot_uid,
                &["react_component", "react_hook_usage"],
            )
            .map(|_| ())
            .map_err(ComposeError::Storage)
        },
    )?;

    // rust-module-parity Phase 2c: Persist pyproject.toml-derived module candidates and file ownership.
    // Same recompute semantics as Cargo and npm.
    persist_pyproject_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.pyproject_modules,
        &prepared.file_inputs,
    )?;

    // rust-module-parity Phase 2b: Persist settings.gradle-derived module candidates and file ownership.
    // Same recompute semantics as Cargo, npm, and pyproject.
    persist_gradle_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.gradle_modules,
        &prepared.file_inputs,
    )?;

    // rust-module-parity Phase 3: Persist inferred module candidates and file ownership.
    // Same recompute semantics as declared modules.
    // ORIENT-BUG-1: Compute declared roots to prevent duplicate ownership.
    let declared_roots = collect_declared_module_roots(
        &prepared.cargo_modules,
        &prepared.npm_modules,
        &prepared.pyproject_modules,
        &prepared.gradle_modules,
    );
    persist_inferred_modules(
        storage,
        repo_uid,
        &result.snapshot_uid,
        &prepared.inferred_modules,
        &prepared.file_inputs,
        &declared_roots,
    )?;

    // PERF-INSTRUMENTATION-1: runtime-gated (was `#[cfg(feature = "perf-trace")]`)
    // so a refresh under RMAP_PERF emits its summary consistently with the rest of
    // the refresh markers and the index path. `perf_log!` self-gates; the two
    // durations are cheap to compute unconditionally.
    {
        let module_persist_ms = module_persist_start.elapsed().as_millis();
        let total_ms = refresh_start.elapsed().as_millis();
        perf_log!(
            "[PERF] refresh: module_persist={}ms total={}ms",
            module_persist_ms,
            total_ms
        );
    }

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

    /// PERSIST-RECURSION-1: the per-file depth guard detects pathologically deep
    /// trees iteratively — it never recurses, so the check itself cannot overflow
    /// — and leaves normal-depth trees untouched.
    #[test]
    fn tree_exceeds_depth_flags_only_pathological_nesting() {
        fn parse_c(source: &str) -> tree_sitter::Tree {
            let mut parser = tree_sitter::Parser::new();
            let lang: tree_sitter::Language = tree_sitter_c::LANGUAGE.into();
            parser.set_language(&lang).unwrap();
            parser.parse(source, None).unwrap()
        }

        // An ordinary shallow file is well under the guard.
        let shallow = parse_c("int main() { return 0; }");
        assert!(!tree_exceeds_depth(
            &shallow.root_node(),
            MAX_POSTPASS_TREE_DEPTH
        ));

        // A file nested far past the guard is flagged — and the check completes
        // (it is iterative, so it does not overflow while measuring the depth).
        let mut deep = String::from("void deep() {\n");
        for _ in 0..(MAX_POSTPASS_TREE_DEPTH + 5_000) {
            deep.push('{');
        }
        for _ in 0..(MAX_POSTPASS_TREE_DEPTH + 5_000) {
            deep.push('}');
        }
        deep.push_str("\n}\n");
        let deep_tree = parse_c(&deep);
        assert!(tree_exceeds_depth(
            &deep_tree.root_node(),
            MAX_POSTPASS_TREE_DEPTH
        ));

        // The bound is honored, not hard-coded: a tiny limit is exceeded even by
        // the shallow tree.
        assert!(tree_exceeds_depth(&shallow.root_node(), 1));
    }

    /// PERSIST-RECURSION-1 item 3: a fallible postpass failure must NEVER abort
    /// the index. `isolate_postpass` converts a non-`Aborted` error into `Ok`
    /// after recording it as an extraction diagnostic, so the index completes
    /// without that postpass's facts; `Aborted` (transport / cancellation) still
    /// propagates so a real cancel is honored, not swallowed.
    #[test]
    fn isolate_postpass_records_failure_but_never_aborts_index() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();

        // A non-Aborted postpass error is ISOLATED: Ok returned (index continues),
        // the compensating cleanup RUNS (so any partial facts are removed), and the
        // failure is recorded in the extraction-diagnostics blob under the postpass's
        // error key.
        let cleaned = std::cell::Cell::new(false);
        let outcome = isolate_postpass(
            &mut storage,
            &snap_uid,
            "test-postpass",
            "test_postpass_error",
            Err(ComposeError::Index("simulated postpass failure".into())),
            |_s| {
                cleaned.set(true);
                Ok(())
            },
        );
        assert!(
            outcome.is_ok(),
            "a non-Aborted postpass failure must be isolated, not propagated"
        );
        assert!(
            cleaned.get(),
            "cleanup must run on an isolatable failure so no partial facts survive"
        );

        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid)
            .unwrap()
            .expect("diagnostics blob is present after a normal index");
        let value: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            value.get("test_postpass_error").and_then(|v| v.as_str()),
            Some("index: simulated postpass failure"),
            "the isolated failure is recorded as an extraction diagnostic"
        );

        // Aborted STILL propagates — isolation must not swallow cancellation — and
        // cleanup does NOT run (the whole index is tearing down; DAEMON-CRASH-
        // RECOVERY-1 reconciles the orphaned snapshot).
        let cleaned = std::cell::Cell::new(false);
        let aborted = isolate_postpass(
            &mut storage,
            &snap_uid,
            "test-postpass",
            "test_postpass_error",
            Err(ComposeError::Aborted),
            |_s| {
                cleaned.set(true);
                Ok(())
            },
        );
        assert!(
            matches!(aborted, Err(ComposeError::Aborted)),
            "Aborted (transport/cancel) must propagate, not be isolated"
        );
        assert!(!cleaned.get(), "Aborted must not trigger fact cleanup");

        // A successful postpass is a plain pass-through — its facts are KEPT, so
        // cleanup does NOT run.
        let cleaned = std::cell::Cell::new(false);
        assert!(isolate_postpass(
            &mut storage,
            &snap_uid,
            "test-postpass",
            "test_postpass_error",
            Ok(5),
            |_s| {
                cleaned.set(true);
                Ok(())
            },
        )
        .is_ok());
        assert!(
            !cleaned.get(),
            "a successful postpass keeps its facts — no cleanup"
        );
    }

    /// PERSIST-RECURSION-1 review-2 item 2 (failure-path proof): when a multi-write
    /// postpass fails AFTER committing some of its facts, `isolate_postpass`'s
    /// compensating cleanup removes those partial facts so the snapshot completes
    /// WITHOUT them — never a half-written subset. We plant a committed policy fact
    /// (standing in for the partial write), fail the postpass, and prove the fact is
    /// gone and the failure is honestly recorded.
    #[test]
    fn isolate_postpass_cleanup_removes_partial_facts() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();

        // Plant a committed policy fact — stands in for a partial write left by a
        // policy-facts postpass that then failed on a later table.
        storage
            .execute_raw(&format!(
                "INSERT INTO status_mappings
                 (uid, snapshot_uid, symbol_key, function_name, file_path,
                  line_start, line_end, source_type, target_type, mappings_json)
                 VALUES ('u1', '{snap_uid}', 'k', 'f', 'a.c', 1, 2, 'in', 'out', '[]')"
            ))
            .unwrap();
        let count = |s: &StorageConnection| -> i64 {
            s.query_scalar(&format!(
                "SELECT COUNT(*) FROM status_mappings WHERE snapshot_uid = '{snap_uid}'"
            ))
            .unwrap()
        };
        assert_eq!(count(&storage), 1, "partial fact present before isolation");

        // The postpass fails; isolate_postpass runs the REAL policy-facts cleanup.
        let outcome = isolate_postpass(
            &mut storage,
            &snap_uid,
            "policy-facts",
            "policy_facts_postpass_error",
            Err(ComposeError::Index("boom".into())),
            |s| {
                s.delete_policy_facts(&snap_uid)
                    .map(|_| ())
                    .map_err(ComposeError::Storage)
            },
        );
        assert!(outcome.is_ok(), "the index continues past the failure");

        // The partial fact is GONE (no half-written subset), and the failure is
        // recorded so the degradation is honest, not silent.
        assert_eq!(
            count(&storage),
            0,
            "cleanup removed the partial policy fact"
        );
        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid)
            .unwrap()
            .expect("diagnostics blob present");
        let value: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            value
                .get("policy_facts_postpass_error")
                .and_then(|v| v.as_str()),
            Some("index: boom"),
            "the isolated failure is recorded as an extraction diagnostic"
        );
    }

    /// PERSIST-RECURSION-1 review-2 item 2 (boundary sibling-preservation): the C
    /// (`c-ipc`) and TS (`ts-worker`) boundary postpasses write the SAME table under
    /// one snapshot, tagged by `extractor`. When the TS postpass fails, its
    /// compensating cleanup must remove ONLY the TS facts and LEAVE the already-
    /// committed C facts — a snapshot-wide delete would collaterally erase correct C
    /// facts and misreport them as measured-absent. Exercises the REAL TS boundary
    /// cleanup closure and the extractor consts the production call sites wire in.
    #[test]
    fn isolate_postpass_boundary_cleanup_preserves_sibling_extractor_facts() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();
        // The surfaces' repo_uid FK (NOT NULL → repos) needs the real repo uid.
        let repo_uid: String = storage
            .query_scalar(&format!(
                "SELECT repo_uid FROM snapshots WHERE snapshot_uid = '{snap_uid}'"
            ))
            .unwrap();

        // Plant one committed C boundary surface (the successful C postpass) and one
        // TS boundary surface (a fact the TS postpass committed before it then failed).
        let plant = |s: &StorageConnection, uid: &str, extractor: &str, file: &str| {
            s.execute_raw(&format!(
                "INSERT INTO boundary_interaction_surfaces
                 (surface_uid, snapshot_uid, repo_uid, boundary_scope, channel_kind,
                  direction, protocol, protocol_family, interaction_pattern,
                  endpoint_locality, symbol_stable_key, source_file, line_start,
                  line_end, col_start, col_end, extractor, basis, confidence, evidence_json)
                 VALUES ('{uid}', '{snap_uid}', '{repo_uid}', 'inter_process', 'unix_socket',
                  'provider', 'unix', 'socket', 'stream', 'same_host_named', 'k', '{file}',
                  1, 2, 3, 4, '{extractor}', 'api_call', 1.0, '{{}}')"
            ))
            .unwrap();
        };
        plant(&storage, "c-surf", C_BOUNDARY_EXTRACTOR, "src/server.c");
        plant(&storage, "ts-surf", TS_BOUNDARY_EXTRACTOR, "src/worker.ts");

        let count_ext = |s: &StorageConnection, ext: &str| -> i64 {
            s.query_scalar(&format!(
                "SELECT COUNT(*) FROM boundary_interaction_surfaces
                 WHERE snapshot_uid = '{snap_uid}' AND extractor = '{ext}'"
            ))
            .unwrap()
        };
        assert_eq!(count_ext(&storage, C_BOUNDARY_EXTRACTOR), 1);
        assert_eq!(count_ext(&storage, TS_BOUNDARY_EXTRACTOR), 1);

        // The TS boundary postpass fails; isolate_postpass runs the REAL TS cleanup.
        let outcome = isolate_postpass(
            &mut storage,
            &snap_uid,
            "ts-boundary-interaction",
            "ts_boundary_facts_postpass_error",
            Err(ComposeError::Index("boom".into())),
            |s| {
                s.delete_boundary_facts_by_extractor(&snap_uid, TS_BOUNDARY_EXTRACTOR)
                    .map(|_| ())
                    .map_err(ComposeError::Storage)
            },
        );
        assert!(outcome.is_ok(), "the index continues past the failure");

        // TS facts gone; the sibling C facts SURVIVE (not collaterally deleted).
        assert_eq!(
            count_ext(&storage, TS_BOUNDARY_EXTRACTOR),
            0,
            "the failed postpass's own facts are removed"
        );
        assert_eq!(
            count_ext(&storage, C_BOUNDARY_EXTRACTOR),
            1,
            "the sibling C boundary facts are preserved"
        );

        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid)
            .unwrap()
            .expect("diagnostics blob present");
        let value: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            value
                .get("ts_boundary_facts_postpass_error")
                .and_then(|v| v.as_str()),
            Some("index: boom"),
            "the isolated failure is recorded"
        );
    }

    /// PERSIST-RECURSION-1 review-2 item 1 (dedup proof): a single pathologically
    /// deep React file is skipped by BOTH the component and hook passes, but the
    /// recorded degradation count is the number of DISTINCT files skipped — ONE, not
    /// two. Guards against reporting one skipped file as two.
    #[test]
    fn persist_react_inferences_dedups_deep_file_across_passes() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();

        // One deep .tsx that BOTH React passes parse (components gate: .tsx + react
        // import; hooks gate: wide ext + react import) and then skip at the depth
        // guard — so without dedup the recorded count would be 2.
        let mut deep =
            String::from("import React, { useState } from 'react';\nfunction Deep() {\n");
        for _ in 0..(MAX_POSTPASS_TREE_DEPTH + 2_000) {
            deep.push('{');
        }
        deep.push_str(" useState(0); ");
        for _ in 0..(MAX_POSTPASS_TREE_DEPTH + 2_000) {
            deep.push('}');
        }
        deep.push_str("\n  return <div/>;\n}\n");
        let file_inputs = vec![FileInput {
            rel_path: "deep.tsx".to_string(),
            content: deep,
            content_hash: String::new(),
            size_bytes: 0,
            line_count: 0,
            package_dependencies: None,
            tsconfig_aliases: None,
        }];

        let persisted =
            persist_react_inferences(&mut storage, "r1", &snap_uid, &file_inputs).unwrap();
        assert_eq!(
            persisted, 0,
            "the deep file yields no inferences (all skipped)"
        );

        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid)
            .unwrap()
            .expect("diagnostics blob present");
        let value: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            value
                .get("react_inference_facts_files_skipped_deep_nesting")
                .and_then(|v| v.as_u64()),
            Some(1),
            "one deep file skipped by both passes counts ONCE, not twice"
        );
    }

    /// PERSIST-RECURSION-1 item 2 (honest degradation write path): the per-file
    /// skip counter accumulates into the extraction-diagnostics blob so a reader
    /// can see that pathological files were skipped rather than silently lost.
    #[test]
    fn record_files_skipped_deep_nesting_accumulates_into_diagnostics() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();

        // Zero is a no-op — no key is written when nothing was skipped.
        record_files_skipped_deep_nesting(&mut storage, &snap_uid, "boundary_deep_skips", 0)
            .unwrap();
        let diag0 = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid)
            .unwrap()
            .unwrap();
        let v0: serde_json::Value = serde_json::from_str(&diag0).unwrap();
        assert!(
            v0.get("boundary_deep_skips").is_none(),
            "0 must not write a key"
        );

        // Successive skips accumulate (multiple postpasses / refresh add up).
        record_files_skipped_deep_nesting(&mut storage, &snap_uid, "boundary_deep_skips", 3)
            .unwrap();
        record_files_skipped_deep_nesting(&mut storage, &snap_uid, "boundary_deep_skips", 2)
            .unwrap();
        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid)
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            v.get("boundary_deep_skips").and_then(|x| x.as_u64()),
            Some(5),
            "skip counts accumulate honestly across calls"
        );
    }

    /// PERSIST-RECURSION-1 review-3 item 1 (failure-path): if the compensating
    /// cleanup ITSELF fails, the snapshot would retain a partial subset of the failed
    /// postpass's facts — a silently-dishonest READY snapshot. `isolate_postpass` must
    /// NOT complete: it DEMOTES the snapshot out of READY (so `get_latest_snapshot`
    /// stops serving it) and PROPAGATES the infrastructure error. The prior version
    /// swallowed the cleanup error (`let _ = ...`) and returned `Ok`.
    #[test]
    fn isolate_postpass_demotes_and_propagates_when_cleanup_fails() {
        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();

        let status = |s: &StorageConnection| -> String {
            s.query_scalar(&format!(
                "SELECT status FROM snapshots WHERE snapshot_uid = '{snap_uid}'"
            ))
            .unwrap()
        };
        assert_eq!(
            status(&storage),
            "ready",
            "the snapshot is already READY before the postpass runs (orchestrator finalize)"
        );

        // The postpass failed AND its cleanup cannot remove the partial facts — a
        // storage I/O failure, modeled as a cleanup closure that returns an error.
        let ran = std::cell::Cell::new(false);
        let outcome = isolate_postpass(
            &mut storage,
            &snap_uid,
            "policy-facts",
            "policy_facts_postpass_error",
            Err(ComposeError::Index("postpass boom".into())),
            |_s| {
                ran.set(true);
                Err(ComposeError::Index(
                    "cleanup could not remove partial facts".into(),
                ))
            },
        );
        assert!(ran.get(), "cleanup was attempted");
        assert!(
            outcome.is_err(),
            "a cleanup failure must PROPAGATE — the index must not silently complete"
        );
        assert_eq!(
            status(&storage),
            "failed",
            "the dishonest snapshot is DEMOTED out of READY so it is not served"
        );
    }

    /// PERSIST-RECURSION-1 review-3 item 1 (failure-path): if RECORDING the
    /// degradation diagnostic fails, the READY snapshot would hide that this
    /// postpass's facts are absent (a false completeness claim). `isolate_postpass`
    /// must DEMOTE and PROPAGATE. We inject the failure by corrupting the diagnostics
    /// blob so the read-modify-write cannot parse it — a stand-in for a storage I/O
    /// failure on the diagnostics write (which cannot be provoked on an in-memory DB).
    #[test]
    fn isolate_postpass_demotes_and_propagates_when_diagnostic_persist_fails() {
        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();

        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = 'not-valid-json{{' \
                 WHERE snapshot_uid = '{snap_uid}'"
            ))
            .unwrap();

        let outcome = isolate_postpass(
            &mut storage,
            &snap_uid,
            "policy-facts",
            "policy_facts_postpass_error",
            Err(ComposeError::Index("postpass boom".into())),
            // Cleanup succeeds; the failure is in RECORDING the degradation.
            |_s| Ok(()),
        );
        assert!(
            outcome.is_err(),
            "a diagnostic-persist failure must PROPAGATE — the degradation must not be silently lost"
        );
        let status: String = storage
            .query_scalar(&format!(
                "SELECT status FROM snapshots WHERE snapshot_uid = '{snap_uid}'"
            ))
            .unwrap();
        assert_eq!(
            status, "failed",
            "the snapshot whose degradation could not be recorded is demoted out of READY"
        );
    }

    /// PERSIST-RECURSION-1 review-3 item 1: a deep-file skip that cannot be persisted
    /// must PROPAGATE, never be silently dropped — a dropped skip is a READY snapshot
    /// claiming a completeness it does not have. Corrupting the blob makes the
    /// read-modify-write fail.
    #[test]
    fn record_files_skipped_deep_nesting_propagates_persist_failure() {
        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();

        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = 'not-json' \
                 WHERE snapshot_uid = '{snap_uid}'"
            ))
            .unwrap();

        let outcome =
            record_files_skipped_deep_nesting(&mut storage, &snap_uid, "boundary_deep_skips", 1);
        assert!(
            outcome.is_err(),
            "a skip that cannot be persisted must propagate, not be silently lost"
        );
    }

    /// PERSIST-RECURSION-1 review-3 item 1: even if the diagnostics column is still
    /// NULL, a deep-file skip is RECORDED (not silently dropped) —
    /// `merge_extraction_diagnostics` starts from an empty object and writes it (the
    /// snapshot row exists, so the UPDATE lands). Closes the `None`-branch honesty gap.
    #[test]
    fn record_files_skipped_deep_nesting_creates_blob_when_column_null() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap_uid = result.snapshot_uid.clone();

        storage
            .execute_raw(&format!(
                "UPDATE snapshots SET extraction_diagnostics_json = NULL \
                 WHERE snapshot_uid = '{snap_uid}'"
            ))
            .unwrap();

        record_files_skipped_deep_nesting(&mut storage, &snap_uid, "boundary_deep_skips", 2)
            .unwrap();

        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap_uid)
            .unwrap()
            .expect("a skip must create the blob even when the column was NULL");
        let v: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            v.get("boundary_deep_skips").and_then(|x| x.as_u64()),
            Some(2),
            "the skip is recorded honestly rather than dropped"
        );
    }

    /// PERSIST-RECURSION-1 regression (the F13 killer BI-1A + its PF-1 sibling):
    /// a pathologically deep C file fed to the C re-parse postpasses COMPLETES
    /// (no stack overflow — reaching the asserts is the proof) and is honestly
    /// SKIPPED, with the skip recorded in the snapshot's extraction-diagnostics
    /// blob (the exact key the reader surface renders). Never a process death,
    /// never silent loss.
    ///
    /// Driven straight into the postpasses (not through `index_into_storage`) on
    /// purpose: the MAIN C extractor still descends the AST recursively (TECH-DEBT
    /// F14, out of this slice's scope), so a deep file routed through "extracting"
    /// would overflow THERE before reaching the guarded postpass. This isolates the
    /// postpass contract this slice fixes.
    #[test]
    fn deeply_nested_c_file_is_skipped_by_postpasses_and_recorded() {
        use repo_graph_trust::TrustStorageRead;

        let fixture = make_fixture_repo();
        let mut storage = StorageConnection::open_in_memory().unwrap();
        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "r1",
            &ComposeOptions::default(),
        )
        .unwrap();
        let snap = result.snapshot_uid.clone();

        // A C file nested far past the guard (~2×15k braces).
        let mut deep = String::from("void deep() {\n");
        for _ in 0..(MAX_POSTPASS_TREE_DEPTH + 5_000) {
            deep.push('{');
        }
        for _ in 0..(MAX_POSTPASS_TREE_DEPTH + 5_000) {
            deep.push('}');
        }
        deep.push_str("\n}\n");
        let deep_file = FileInput {
            rel_path: "deep.c".to_string(),
            content: deep,
            content_hash: String::new(),
            size_bytes: 0,
            line_count: 0,
            package_dependencies: None,
            tsconfig_aliases: None,
        };

        // Both C re-parse postpasses complete WITHOUT overflow and skip the file
        // rather than descend it.
        persist_boundary_interactions(&mut storage, "r1", &snap, std::slice::from_ref(&deep_file))
            .unwrap();
        persist_policy_facts(&mut storage, "r1", &snap, std::slice::from_ref(&deep_file)).unwrap();

        // The skips are recorded honestly — this is the blob the reader surface reads.
        let diag = TrustStorageRead::get_snapshot_extraction_diagnostics(&storage, &snap)
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&diag).unwrap();
        assert_eq!(
            v.get("boundary_facts_files_skipped_deep_nesting")
                .and_then(|x| x.as_u64()),
            Some(1),
            "BI-1A (the F13 killer) skip recorded"
        );
        assert_eq!(
            v.get("policy_facts_files_skipped_deep_nesting")
                .and_then(|x| x.as_u64()),
            Some(1),
            "PF-1 sibling skip recorded"
        );
    }

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
        assert!(stable_keys
            .iter()
            .any(|k| k.contains("#serve:SYMBOL:FUNCTION")));
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
            stable_keys
                .iter()
                .any(|k| k.contains("#Service:SYMBOL:INTERFACE")),
            "expected Service INTERFACE symbol; got keys: {:?}",
            stable_keys
        );
        assert!(
            stable_keys
                .iter()
                .any(|k| k.contains("#App.run:SYMBOL:METHOD")),
            "expected App.run METHOD symbol; got keys: {:?}",
            stable_keys
        );
        assert!(
            stable_keys
                .iter()
                .any(|k| k.contains("#App.main:SYMBOL:METHOD")),
            "expected App.main METHOD symbol; got keys: {:?}",
            stable_keys
        );

        // Constructor
        assert!(
            stable_keys
                .iter()
                .any(|k| k.contains("#App:SYMBOL:CONSTRUCTOR")),
            "expected App CONSTRUCTOR symbol; got keys: {:?}",
            stable_keys
        );

        // Field (uses PROPERTY subtype, consistent with TS extractor)
        assert!(
            stable_keys
                .iter()
                .any(|k| k.contains("#App.name:SYMBOL:PROPERTY")),
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
            targets
                .iter()
                .any(|t| t.contains("UserService:SYMBOL:CLASS")),
            "expected UserService inference; targets: {:?}",
            targets
        );
        assert!(
            targets
                .iter()
                .any(|t| t.contains("ApiController:SYMBOL:CLASS")),
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

    // ── Inferred Module Refresh Parity Tests (Phase 3.1 gate) ─────────

    /// Creates a manifest-less C repo with multiple top-level source directories.
    /// This will trigger inferred module detection (no Cargo.toml, package.json, etc.)
    fn make_inferred_c_fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // src/ directory
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.c"),
            "#include \"util.h\"\nint main() { return 0; }",
        )
        .unwrap();
        fs::write(root.join("src/util.c"), "void helper() {}").unwrap();
        fs::write(root.join("src/util.h"), "void helper();").unwrap();

        // lib/ directory
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(root.join("lib/core.c"), "int compute() { return 42; }").unwrap();
        fs::write(root.join("lib/core.h"), "int compute();").unwrap();

        // test/ directory (should be suppressed when real source dirs exist)
        fs::create_dir_all(root.join("test")).unwrap();
        fs::write(root.join("test/test_main.c"), "void test() {}").unwrap();

        dir
    }

    #[test]
    fn inferred_modules_detected_on_first_index() {
        let fixture = make_inferred_c_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();

        let result = index_into_storage(
            fixture.path(),
            &mut storage,
            "inferred-test",
            &ComposeOptions::default(),
        )
        .unwrap();

        let modules = storage
            .get_module_candidates_for_snapshot(&result.snapshot_uid)
            .unwrap();

        // Should detect src and lib as inferred modules (test suppressed)
        assert_eq!(
            modules.len(),
            2,
            "expected 2 inferred modules; got {}",
            modules.len()
        );

        let module_keys: Vec<&str> = modules.iter().map(|m| m.module_key.as_str()).collect();
        assert!(
            module_keys.iter().any(|k| k.contains(":src")),
            "expected src module; got {:?}",
            module_keys
        );
        assert!(
            module_keys.iter().any(|k| k.contains(":lib")),
            "expected lib module; got {:?}",
            module_keys
        );

        // All should be inferred kind with confidence < 1.0
        for module in &modules {
            assert_eq!(module.module_kind, "inferred", "expected inferred kind");
            assert!(
                module.confidence < 1.0,
                "inferred modules should have confidence < 1.0"
            );
        }
    }

    #[test]
    fn inferred_modules_stable_across_refresh() {
        let fixture = make_inferred_c_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();

        // First index
        let result1 = index_into_storage(
            fixture.path(),
            &mut storage,
            "inferred-test",
            &ComposeOptions::default(),
        )
        .unwrap();

        let modules1 = storage
            .get_module_candidates_for_snapshot(&result1.snapshot_uid)
            .unwrap();
        let ownership1 = storage
            .get_file_ownership_for_snapshot(&result1.snapshot_uid)
            .unwrap();

        // Refresh
        let result2 = refresh_into_storage(
            fixture.path(),
            &mut storage,
            "inferred-test",
            &ComposeOptions::default(),
        )
        .unwrap();

        let modules2 = storage
            .get_module_candidates_for_snapshot(&result2.snapshot_uid)
            .unwrap();
        let ownership2 = storage
            .get_file_ownership_for_snapshot(&result2.snapshot_uid)
            .unwrap();

        // Module count must match
        assert_eq!(
            modules1.len(),
            modules2.len(),
            "module count must be stable across refresh"
        );

        // Module UIDs must match (deterministic identity)
        let uids1: std::collections::HashSet<&str> = modules1
            .iter()
            .map(|m| m.module_candidate_uid.as_str())
            .collect();
        let uids2: std::collections::HashSet<&str> = modules2
            .iter()
            .map(|m| m.module_candidate_uid.as_str())
            .collect();
        assert_eq!(uids1, uids2, "module UIDs must be stable across refresh");

        // Module keys must match
        let keys1: std::collections::HashSet<&str> =
            modules1.iter().map(|m| m.module_key.as_str()).collect();
        let keys2: std::collections::HashSet<&str> =
            modules2.iter().map(|m| m.module_key.as_str()).collect();
        assert_eq!(keys1, keys2, "module keys must be stable across refresh");

        // Ownership count must match
        assert_eq!(
            ownership1.len(),
            ownership2.len(),
            "ownership count must be stable across refresh"
        );

        // Ownership assignments must match (file_uid -> module_candidate_uid)
        let owner_map1: std::collections::HashMap<&str, &str> = ownership1
            .iter()
            .map(|o| (o.file_uid.as_str(), o.module_candidate_uid.as_str()))
            .collect();
        let owner_map2: std::collections::HashMap<&str, &str> = ownership2
            .iter()
            .map(|o| (o.file_uid.as_str(), o.module_candidate_uid.as_str()))
            .collect();
        assert_eq!(
            owner_map1, owner_map2,
            "file ownership must be stable across refresh"
        );
    }

    #[test]
    fn inferred_modules_update_on_directory_addition() {
        let fixture = make_inferred_c_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();

        // First index
        let result1 = index_into_storage(
            fixture.path(),
            &mut storage,
            "inferred-test",
            &ComposeOptions::default(),
        )
        .unwrap();

        let modules1 = storage
            .get_module_candidates_for_snapshot(&result1.snapshot_uid)
            .unwrap();
        assert_eq!(modules1.len(), 2, "initial: expected 2 modules");

        // Add a new source directory
        fs::create_dir_all(fixture.path().join("util")).unwrap();
        fs::write(
            fixture.path().join("util/parse.c"),
            "int parse() { return 1; }",
        )
        .unwrap();

        // Refresh
        let result2 = refresh_into_storage(
            fixture.path(),
            &mut storage,
            "inferred-test",
            &ComposeOptions::default(),
        )
        .unwrap();

        let modules2 = storage
            .get_module_candidates_for_snapshot(&result2.snapshot_uid)
            .unwrap();

        // Should now have 3 modules
        assert_eq!(modules2.len(), 3, "after adding util/: expected 3 modules");

        let keys: Vec<&str> = modules2.iter().map(|m| m.module_key.as_str()).collect();
        assert!(
            keys.iter().any(|k| k.contains(":util")),
            "expected new util module; got {:?}",
            keys
        );

        // Original modules should still have same UIDs
        let src_uid1 = modules1
            .iter()
            .find(|m| m.module_key.contains(":src"))
            .map(|m| &m.module_candidate_uid);
        let src_uid2 = modules2
            .iter()
            .find(|m| m.module_key.contains(":src"))
            .map(|m| &m.module_candidate_uid);
        assert_eq!(src_uid1, src_uid2, "src module UID must be stable");
    }

    #[test]
    fn inferred_modules_disappear_when_directory_emptied() {
        let fixture = make_inferred_c_fixture();
        let mut storage = StorageConnection::open_in_memory().unwrap();

        // First index
        let result1 = index_into_storage(
            fixture.path(),
            &mut storage,
            "inferred-test",
            &ComposeOptions::default(),
        )
        .unwrap();

        let modules1 = storage
            .get_module_candidates_for_snapshot(&result1.snapshot_uid)
            .unwrap();
        assert_eq!(modules1.len(), 2, "initial: expected 2 modules (src, lib)");

        // Remove all source files from lib/
        fs::remove_file(fixture.path().join("lib/core.c")).unwrap();
        fs::remove_file(fixture.path().join("lib/core.h")).unwrap();

        // Refresh
        let result2 = refresh_into_storage(
            fixture.path(),
            &mut storage,
            "inferred-test",
            &ComposeOptions::default(),
        )
        .unwrap();

        let modules2 = storage
            .get_module_candidates_for_snapshot(&result2.snapshot_uid)
            .unwrap();

        // Should now have only 1 module (lib disappears)
        assert_eq!(modules2.len(), 1, "after removing lib/*: expected 1 module");
        assert!(
            modules2[0].module_key.contains(":src"),
            "remaining module should be src"
        );
    }
}
