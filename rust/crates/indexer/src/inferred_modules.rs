//! Inferred module detection (rust-module-parity Phase 3).
//!
//! Detects module boundaries from directory structure in manifest-less repos.
//! Uses top-level directory heuristics to infer module boundaries.
//!
//! # Identity Contract
//!
//! Module key format: `inferred:{repo_uid}:{directory_path}`
//!
//! Examples:
//! - `inferred:sqlite:src`
//! - `inferred:sqlite:ext`
//! - `inferred:nginx:src`
//! - `inferred:linux:drivers`
//!
//! Path-anchored identity. Same rule as declared modules.
//!
//! # Evidence Structure
//!
//! Each module candidate has associated evidence:
//! - `source_type` = "directory_heuristic"
//! - `source_path` = directory path
//! - `evidence_kind` = "directory_structure"
//! - `payload_json` contains:
//!   - `heuristic`: "top_level_source_directory"
//!   - `directory_path`: the inferred module root
//!   - `source_file_count`: number of source files in subtree
//!   - `is_fallback_root`: true if this is a flat-repo fallback
//!
//! # Inference Rules (Phase 3 first cut)
//!
//! 1. Infer modules from top-level directories containing source files
//! 2. No root module if meaningful top-level modules exist
//! 3. Fall back to root module only if repo is genuinely flat
//! 4. Do not create modules for pure test-only directories (unless no others)
//!
//! # Confidence
//!
//! Inferred modules use confidence 0.7 (vs 1.0 for declared modules).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Confidence score for inferred modules.
pub const INFERRED_MODULE_CONFIDENCE: f64 = 0.7;

// ── Umbrella splitting thresholds (Phase 3.2) ────────────────────────

/// Directory prefixes that are candidates for umbrella splitting.
/// If a top-level directory matches one of these and has multiple
/// qualifying children, it will be split into child modules.
const UMBRELLA_PREFIXES: &[&str] = &["src", "packages", "services", "apps", "libs", "modules"];

/// Minimum number of qualifying children to trigger umbrella split.
const UMBRELLA_MIN_CHILDREN: usize = 2;

/// Minimum source files per child to qualify for split.
const UMBRELLA_MIN_FILES_PER_CHILD: usize = 5;

/// Maximum direct source files in parent before split is suppressed.
/// If the parent itself has > this many direct source files, treat
/// it as a real module root, not an umbrella.
const UMBRELLA_MAX_PARENT_DIRECT_FILES: usize = 5;

/// Check if a directory name is an umbrella candidate.
fn is_umbrella_candidate(dir_name: &str) -> bool {
    UMBRELLA_PREFIXES.contains(&dir_name.to_lowercase().as_str())
}

// ── Exclusion categories ─────────────────────────────────────────────

/// Reason why a directory was excluded from module inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExclusionReason {
    /// Vendor/dependency directory (vendor/, third_party/, node_modules/)
    VendorDependency,
    /// Build output directory (dist/, build/, out/, target/)
    BuildOutput,
    /// Generated code directory (generated/, gen/)
    GeneratedCode,
    /// Documentation directory (docs/, Documentation/)
    Documentation,
    /// Examples/samples directory (examples/, samples/, demo/)
    ExamplesOrSamples,
    /// Benchmark-only directory
    BenchmarkOnly,
}

impl ExclusionReason {
    /// Human-readable description of the exclusion reason.
    pub fn description(&self) -> &'static str {
        match self {
            Self::VendorDependency => "vendor/dependency directory",
            Self::BuildOutput => "build output directory",
            Self::GeneratedCode => "generated code directory",
            Self::Documentation => "documentation directory",
            Self::ExamplesOrSamples => "examples/samples directory",
            Self::BenchmarkOnly => "benchmark-only directory",
        }
    }
}

/// A directory that was excluded from module inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedDirectory {
    /// Directory path
    pub directory_path: String,
    /// Why it was excluded
    pub reason: ExclusionReason,
    /// Number of source files that were in this directory
    pub source_file_count: usize,
}

// ── Extraction output types ──────────────────────────────────────────

/// A module inferred from directory structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredModule {
    /// Directory path relative to repo root (e.g., `src`, `ext`, `.`)
    pub directory_path: String,
    /// Display name (directory basename, or repo name for root)
    pub display_name: String,
    /// Number of source files in this module's subtree (non-test)
    pub source_file_count: usize,
    /// Number of test files in this module's subtree
    pub test_file_count: usize,
    /// Whether this is a fallback root module (flat repo case)
    pub is_fallback_root: bool,
    /// Build files present in this directory (Phase 3.2 evidence).
    /// e.g., ["CMakeLists.txt", "Makefile"]
    pub build_files_present: Vec<String>,
    /// Dominant language by file count (Phase 3.2 evidence).
    /// None if exact tie between top languages.
    pub dominant_language: Option<String>,
}

/// Result of inferred module detection for a repo.
#[derive(Debug, Clone, Default)]
pub struct InferredModuleResult {
    /// Inferred modules
    pub modules: Vec<InferredModule>,
    /// Directories that were excluded from inference
    pub excluded_directories: Vec<ExcludedDirectory>,
}

/// Evidence payload for inferred modules (Phase 3.2 enriched).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferredEvidencePayload {
    /// Heuristic that matched
    pub heuristic: String,
    /// Directory path
    pub directory_path: String,
    /// Number of non-test source files in subtree
    pub source_file_count: usize,
    /// Number of test files in subtree
    pub test_file_count: usize,
    /// Whether this is a fallback root module
    pub is_fallback_root: bool,
    /// Evidence strength: "basic" or "build_marker_backed" (Phase 3.2).
    /// "build_marker_backed" indicates build file presence, stronger boundary signal.
    pub evidence_strength: String,
    /// Build files present in this directory (Phase 3.2).
    /// e.g., ["CMakeLists.txt", "Makefile"]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_files_present: Vec<String>,
    /// Dominant language by file count (Phase 3.2).
    /// None if exact tie between top languages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dominant_language: Option<String>,
}

// ── Detection functions ──────────────────────────────────────────────

/// Detect inferred modules from a list of file paths.
///
/// # Arguments
/// - `file_paths`: list of file paths relative to repo root
/// - `repo_display_name`: display name for fallback root module
///
/// # Returns
/// - `InferredModuleResult` with detected modules
///
/// # Algorithm
///
/// 1. Partition files by top-level directory
/// 2. Count source files per directory (exclude tests)
/// 3. Identify directories with source files
/// 4. Filter out test-only directories
/// 5. If no source directories, check for test directories
/// 6. If no top-level directories with sources, fall back to root
pub fn detect_inferred_modules(
    file_paths: &[String],
    repo_display_name: &str,
) -> InferredModuleResult {
    let mut result = InferredModuleResult::default();

    // Partition files by top-level directory.
    let mut dir_stats: HashMap<String, DirectoryStats> = HashMap::new();
    let mut root_stats = DirectoryStats::default();

    // Track second-level stats for umbrella candidates (Phase 3.2).
    // Maps "top_level/second_level" -> stats for that subtree.
    let mut umbrella_children: HashMap<String, DirectoryStats> = HashMap::new();
    // Track direct files in umbrella (not in any child).
    let mut umbrella_direct_counts: HashMap<String, usize> = HashMap::new();

    for path in file_paths {
        // Extract filename for build file detection.
        let filename = path.rsplit('/').next().unwrap_or(path);
        let parts: Vec<&str> = path.split('/').collect();

        // Determine top-level directory.
        let top_level = get_top_level_directory(path);

        // Check for build file (Phase 3.2).
        // Build files are tracked for ALL directories, not just source directories.
        if is_build_file(filename) {
            match &top_level {
                Some(dir) => {
                    // Only track if file is directly in top-level dir (not nested).
                    if parts.len() == 2 {
                        let stats = dir_stats.entry(dir.clone()).or_default();
                        if !stats.build_files.contains(&filename.to_string()) {
                            stats.build_files.push(filename.to_string());
                        }
                    }
                    // Track build files at second level for umbrella children.
                    if parts.len() == 3 && is_umbrella_candidate(dir) {
                        let child_path = format!("{}/{}", parts[0], parts[1]);
                        let child_stats = umbrella_children.entry(child_path).or_default();
                        if !child_stats.build_files.contains(&filename.to_string()) {
                            child_stats.build_files.push(filename.to_string());
                        }
                    }
                }
                None => {
                    // Build file at repo root.
                    if !root_stats.build_files.contains(&filename.to_string()) {
                        root_stats.build_files.push(filename.to_string());
                    }
                }
            }
        }

        // Skip non-source files for module detection.
        if !is_source_file(path) {
            continue;
        }

        let is_test = is_test_path(path);

        // Extract language from extension (Phase 3.2).
        let ext = path.rsplit('.').next().unwrap_or("");
        let language = extension_to_language(ext);

        match &top_level {
            Some(dir) => {
                let stats = dir_stats.entry(dir.clone()).or_default();
                if is_test {
                    stats.test_file_count += 1;
                } else {
                    stats.source_file_count += 1;
                    // Track language counts for non-test files only (Phase 3.2).
                    // Test files should not skew implementation topology signal.
                    if let Some(lang) = language {
                        *stats.language_counts.entry(lang.to_string()).or_insert(0) += 1;
                    }
                }

                // Track umbrella children stats (Phase 3.2).
                if is_umbrella_candidate(dir) {
                    if parts.len() == 2 {
                        // Direct file in umbrella (e.g., src/main.c)
                        if !is_test {
                            *umbrella_direct_counts.entry(dir.clone()).or_insert(0) += 1;
                        }
                    } else if parts.len() >= 3 {
                        // File in child directory (e.g., src/core/nginx.c)
                        let child_path = format!("{}/{}", parts[0], parts[1]);
                        let child_stats = umbrella_children.entry(child_path).or_default();
                        if is_test {
                            child_stats.test_file_count += 1;
                        } else {
                            child_stats.source_file_count += 1;
                            // Track language counts for non-test files only.
                            if let Some(lang) = language {
                                *child_stats
                                    .language_counts
                                    .entry(lang.to_string())
                                    .or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
            None => {
                // File is at repo root.
                if is_test {
                    root_stats.test_file_count += 1;
                } else {
                    root_stats.source_file_count += 1;
                    // Track language counts for non-test files only (Phase 3.2).
                    if let Some(lang) = language {
                        *root_stats
                            .language_counts
                            .entry(lang.to_string())
                            .or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // Partition directories into included and excluded.
    let mut source_dirs: Vec<String> = Vec::new();
    let mut test_only_dirs: Vec<String> = Vec::new();

    for (dir, stats) in &dir_stats {
        // Check for exclusion.
        if let Some(reason) = should_exclude_directory(dir) {
            let total = stats.source_file_count + stats.test_file_count;
            if total > 0 {
                result.excluded_directories.push(ExcludedDirectory {
                    directory_path: dir.clone(),
                    reason,
                    source_file_count: total,
                });
            }
            continue;
        }

        // Categorize by whether it has non-test source files.
        if stats.source_file_count > 0 {
            source_dirs.push(dir.clone());
        } else if stats.test_file_count > 0 {
            test_only_dirs.push(dir.clone());
        }
    }

    // If we have source directories, create modules for them.
    // Check for umbrella splitting (Phase 3.2).
    if !source_dirs.is_empty() {
        for dir in source_dirs {
            // Check if this is an umbrella candidate that should be split.
            if is_umbrella_candidate(&dir) {
                let direct_count = *umbrella_direct_counts.get(&dir).unwrap_or(&0);

                // Collect qualifying children (those with >= min files).
                let prefix = format!("{}/", dir);
                let mut qualifying_children: Vec<(String, &DirectoryStats)> = umbrella_children
                    .iter()
                    .filter(|(path, stats)| {
                        path.starts_with(&prefix)
                            && stats.source_file_count >= UMBRELLA_MIN_FILES_PER_CHILD
                    })
                    .map(|(path, stats)| (path.clone(), stats))
                    .collect();

                // Sort for deterministic ordering.
                qualifying_children.sort_by(|a, b| a.0.cmp(&b.0));

                // Check split conditions:
                // 1. Parent has <= ceiling direct files
                // 2. At least min qualifying children
                let should_split = direct_count <= UMBRELLA_MAX_PARENT_DIRECT_FILES
                    && qualifying_children.len() >= UMBRELLA_MIN_CHILDREN;

                if should_split {
                    // Create child modules instead of parent.
                    for (child_path, child_stats) in qualifying_children {
                        let dominant_language =
                            compute_dominant_language(&child_stats.language_counts);
                        let mut build_files = child_stats.build_files.clone();
                        build_files.sort();

                        // Display name is the child directory name (e.g., "core" from "src/core").
                        let display_name = child_path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&child_path)
                            .to_string();

                        result.modules.push(InferredModule {
                            directory_path: child_path,
                            display_name,
                            source_file_count: child_stats.source_file_count,
                            test_file_count: child_stats.test_file_count,
                            is_fallback_root: false,
                            build_files_present: build_files,
                            dominant_language,
                        });
                    }
                    continue; // Skip creating parent module
                }
            }

            // Normal case: create module for the top-level directory.
            let stats = dir_stats.get(&dir).unwrap();
            let dominant_language = compute_dominant_language(&stats.language_counts);
            let mut build_files = stats.build_files.clone();
            build_files.sort(); // deterministic order

            result.modules.push(InferredModule {
                directory_path: dir.clone(),
                display_name: dir.clone(),
                source_file_count: stats.source_file_count,
                test_file_count: stats.test_file_count,
                is_fallback_root: false,
                build_files_present: build_files,
                dominant_language,
            });
        }
        return result;
    }

    // No source directories. Check for test-only directories as fallback.
    if !test_only_dirs.is_empty() {
        for dir in test_only_dirs {
            let stats = dir_stats.get(&dir).unwrap();
            let dominant_language = compute_dominant_language(&stats.language_counts);
            let mut build_files = stats.build_files.clone();
            build_files.sort();

            result.modules.push(InferredModule {
                directory_path: dir.clone(),
                display_name: dir.clone(),
                source_file_count: 0,
                test_file_count: stats.test_file_count,
                is_fallback_root: false,
                build_files_present: build_files,
                dominant_language,
            });
        }
        return result;
    }

    // No top-level directories with source files.
    // Fall back to root module if there are source files at root.
    if root_stats.source_file_count > 0 || root_stats.test_file_count > 0 {
        let dominant_language = compute_dominant_language(&root_stats.language_counts);
        let mut build_files = root_stats.build_files.clone();
        build_files.sort();

        result.modules.push(InferredModule {
            directory_path: ".".to_string(),
            display_name: repo_display_name.to_string(),
            source_file_count: root_stats.source_file_count,
            test_file_count: root_stats.test_file_count,
            is_fallback_root: true,
            build_files_present: build_files,
            dominant_language,
        });
    }

    result
}

#[derive(Default)]
struct DirectoryStats {
    source_file_count: usize,
    test_file_count: usize,
    /// Build files found directly in this directory (Phase 3.2).
    build_files: Vec<String>,
    /// Language counts by language name (Phase 3.2).
    language_counts: HashMap<String, usize>,
}

// ── Build file detection (Phase 3.2) ─────────────────────────────────

/// Build files that indicate module boundaries.
/// Conservative initial set - presence-only detection.
const BUILD_FILE_MARKERS: &[&str] = &[
    "CMakeLists.txt",
    "Makefile",
    "GNUmakefile",
    "makefile",
    "meson.build",
    "BUILD",
    "BUILD.bazel",
    "Kbuild",
];

/// Check if a filename is a build file marker.
fn is_build_file(filename: &str) -> bool {
    BUILD_FILE_MARKERS.contains(&filename)
}

// ── Language detection (Phase 3.2) ───────────────────────────────────

/// Map file extension to language name.
/// Returns None for non-source extensions.
fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "c" | "h" => Some("c"),
        "cpp" | "hpp" | "cc" | "hh" | "cxx" | "hxx" => Some("cpp"),
        "java" => Some("java"),
        "kt" => Some("kotlin"),
        "scala" => Some("scala"),
        "py" => Some("python"),
        "rs" => Some("rust"),
        "go" => Some("go"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "tsx" => Some("typescript"),
        "rb" => Some("ruby"),
        "swift" => Some("swift"),
        "m" => Some("objective-c"),
        "mm" => Some("objective-cpp"),
        _ => None,
    }
}

/// Compute dominant language from counts.
/// Returns None on exact tie between top languages.
fn compute_dominant_language(counts: &HashMap<String, usize>) -> Option<String> {
    if counts.is_empty() {
        return None;
    }

    // Find max count
    let max_count = counts.values().max().copied().unwrap_or(0);
    if max_count == 0 {
        return None;
    }

    // Collect all languages with max count
    let top_languages: Vec<_> = counts
        .iter()
        .filter(|(_, &count)| count == max_count)
        .map(|(lang, _)| lang.clone())
        .collect();

    // Exact tie => None (Q3: Option C)
    if top_languages.len() > 1 {
        return None;
    }

    top_languages.into_iter().next()
}

/// Get the top-level directory from a file path.
///
/// `src/foo/bar.c` → Some("src")
/// `foo.c` → None (file at root)
fn get_top_level_directory(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 1 {
        Some(parts[0].to_string())
    } else {
        None
    }
}

/// Check if a file path is a source file (not config, docs, etc.).
fn is_source_file(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "c" | "h"
            | "cpp"
            | "hpp"
            | "cc"
            | "hh"
            | "cxx"
            | "hxx"
            | "java"
            | "kt"
            | "scala"
            | "py"
            | "rs"
            | "go"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "mjs"
            | "cjs"
            | "rb"
            | "swift"
            | "m"
            | "mm" // Objective-C
    )
}

/// Check if a file path is in a test directory or is a test file.
fn is_test_path(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    // Test directory patterns.
    if path_lower.contains("/test/")
        || path_lower.contains("/tests/")
        || path_lower.contains("/testing/")
        || path_lower.contains("/__tests__/")
        || path_lower.contains("/spec/")
        || path_lower.contains("/specs/")
        || path_lower.starts_with("test/")
        || path_lower.starts_with("tests/")
    {
        return true;
    }

    // Test file patterns.
    let filename = path.rsplit('/').next().unwrap_or(path);
    let filename_lower = filename.to_lowercase();

    filename_lower.starts_with("test_")
        || filename_lower.ends_with("_test.c")
        || filename_lower.ends_with("_test.cpp")
        || filename_lower.ends_with("_test.cc")
        || filename_lower.ends_with("_test.java")
        || filename_lower.ends_with("_test.py")
        || filename_lower.ends_with("_test.rs")
        || filename_lower.ends_with("_test.go")
        || filename_lower.ends_with(".test.js")
        || filename_lower.ends_with(".test.ts")
        || filename_lower.ends_with(".spec.js")
        || filename_lower.ends_with(".spec.ts")
}

/// Check if a top-level directory should be excluded from module inference.
///
/// Returns `Some(ExclusionReason)` if the directory should be excluded,
/// `None` if it should be considered for module inference.
fn should_exclude_directory(dir_name: &str) -> Option<ExclusionReason> {
    let dir_lower = dir_name.to_lowercase();

    // Vendor/dependency directories
    if matches!(
        dir_lower.as_str(),
        "vendor"
            | "vendors"
            | "third_party"
            | "third-party"
            | "thirdparty"
            | "node_modules"
            | "bower_components"
            | "jspm_packages"
            | "external"
            | "externals"
            | "deps"
            | "dependencies"
    ) {
        return Some(ExclusionReason::VendorDependency);
    }

    // Build output directories
    if matches!(
        dir_lower.as_str(),
        "dist" | "build" | "builds"
            | "out" | "output"
            | "target"  // Rust/Maven
            | "bin"     // Often build output
            | "obj"     // .NET
            | "_build"  // Elixir/Erlang
            | "cmake-build-debug" | "cmake-build-release"
    ) {
        return Some(ExclusionReason::BuildOutput);
    }

    // Generated code directories
    if matches!(
        dir_lower.as_str(),
        "generated" | "gen" | "codegen" | "auto" | "autogen" | "__generated__"
    ) {
        return Some(ExclusionReason::GeneratedCode);
    }

    // Documentation directories
    if matches!(
        dir_lower.as_str(),
        "docs" | "doc" | "documentation" | "man" | "manpages" | "javadoc" | "apidoc" | "apidocs"
    ) {
        return Some(ExclusionReason::Documentation);
    }

    // Examples/samples directories
    if matches!(
        dir_lower.as_str(),
        "examples" | "example" | "samples" | "sample" | "demo" | "demos" | "tutorials" | "tutorial"
    ) {
        return Some(ExclusionReason::ExamplesOrSamples);
    }

    // Benchmark-only directories
    if matches!(
        dir_lower.as_str(),
        "benchmark" | "benchmarks" | "bench" | "benches" | "perf" | "performance"
    ) {
        return Some(ExclusionReason::BenchmarkOnly);
    }

    None
}

// ── Identity generation ──────────────────────────────────────────────

/// Generate a deterministic module_candidate_uid for inferred modules.
///
/// Identity is derived from: repo_uid + directory_path + "inferred"
pub fn generate_module_uid(repo_uid: &str, directory_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"inferred_module:");
    hasher.update(repo_uid.as_bytes());
    hasher.update(b":");
    hasher.update(directory_path.as_bytes());
    hasher.update(b":inferred");
    let hash = hasher.finalize();
    format!(
        "inferred-mod-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate a deterministic evidence_uid.
pub fn generate_evidence_uid(module_uid: &str, directory_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"inferred_evidence:");
    hasher.update(module_uid.as_bytes());
    hasher.update(b":");
    hasher.update(directory_path.as_bytes());
    let hash = hasher.finalize();
    format!(
        "inferred-ev-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate the canonical module_key.
///
/// Format: `inferred:{repo_uid}:{directory_path}`
pub fn generate_module_key(repo_uid: &str, directory_path: &str) -> String {
    format!("inferred:{}:{}", repo_uid, directory_path)
}

// ── Storage input conversion ─────────────────────────────────────────

use crate::cargo_manifest::{CargoModuleCandidateInput, CargoModuleEvidenceInput};

/// Convert an InferredModule to storage inputs.
///
/// Generates deterministic UIDs and evidence payload.
/// Returns the same input types as declared modules (they're generic).
pub fn to_storage_inputs(
    module: &InferredModule,
    repo_uid: &str,
    snapshot_uid: &str,
) -> (CargoModuleCandidateInput, CargoModuleEvidenceInput) {
    let module_uid = generate_module_uid(repo_uid, &module.directory_path);
    let module_key = generate_module_key(repo_uid, &module.directory_path);
    let evidence_uid = generate_evidence_uid(&module_uid, &module.directory_path);

    // Determine evidence strength (Phase 3.2).
    // "build_marker_backed" if any build file present, otherwise "basic".
    let evidence_strength = if module.build_files_present.is_empty() {
        "basic".to_string()
    } else {
        "build_marker_backed".to_string()
    };

    let payload = InferredEvidencePayload {
        heuristic: "top_level_source_directory".to_string(),
        directory_path: module.directory_path.clone(),
        source_file_count: module.source_file_count,
        test_file_count: module.test_file_count,
        is_fallback_root: module.is_fallback_root,
        evidence_strength,
        build_files_present: module.build_files_present.clone(),
        dominant_language: module.dominant_language.clone(),
    };

    let candidate = CargoModuleCandidateInput {
        module_candidate_uid: module_uid.clone(),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        module_key,
        module_kind: "inferred".to_string(),
        canonical_root_path: module.directory_path.clone(),
        confidence: INFERRED_MODULE_CONFIDENCE,
        display_name: module.display_name.clone(),
        metadata_json: None,
    };

    let evidence = CargoModuleEvidenceInput {
        evidence_uid,
        module_candidate_uid: module_uid,
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        source_type: "directory_heuristic".to_string(),
        source_path: module.directory_path.clone(),
        evidence_kind: "directory_structure".to_string(),
        confidence: INFERRED_MODULE_CONFIDENCE,
        payload_json: serde_json::to_string(&payload).unwrap_or_default(),
    };

    (candidate, evidence)
}

// ── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn detect_sqlite_structure() {
        let files = vec![
            "src/main.c".to_string(),
            "src/sqlite3.c".to_string(),
            "src/sqlite3.h".to_string(),
            "ext/fts5/fts5.c".to_string(),
            "ext/rtree/rtree.c".to_string(),
            "tool/lemon.c".to_string(),
            "test/test1.c".to_string(),
            "test/test2.c".to_string(),
        ];

        let result = detect_inferred_modules(&files, "sqlite");

        // Should detect src, ext, tool (not test because it's test-only)
        assert_eq!(result.modules.len(), 3);

        let dirs: HashSet<_> = result
            .modules
            .iter()
            .map(|m| m.directory_path.as_str())
            .collect();
        assert!(dirs.contains("src"));
        assert!(dirs.contains("ext"));
        assert!(dirs.contains("tool"));
        assert!(!dirs.contains("test")); // test-only directory excluded
    }

    #[test]
    fn detect_nginx_structure() {
        let files = vec![
            "src/core/nginx.c".to_string(),
            "src/core/ngx_config.h".to_string(),
            "src/http/ngx_http.c".to_string(),
            "src/event/ngx_event.c".to_string(),
            "conf/nginx.conf".to_string(), // Not a source file
        ];

        let result = detect_inferred_modules(&files, "nginx");

        // Should detect just src (top-level)
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "src");
        assert_eq!(result.modules[0].source_file_count, 4);
    }

    #[test]
    fn detect_flat_repo() {
        let files = vec![
            "main.c".to_string(),
            "util.c".to_string(),
            "util.h".to_string(),
        ];

        let result = detect_inferred_modules(&files, "myapp");

        // Should fall back to root module
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, ".");
        assert_eq!(result.modules[0].display_name, "myapp");
        assert!(result.modules[0].is_fallback_root);
    }

    #[test]
    fn detect_test_only_fallback() {
        let files = vec!["test/test1.c".to_string(), "test/test2.c".to_string()];

        let result = detect_inferred_modules(&files, "testonly");

        // No source directories, should fall back to test directory
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "test");
    }

    #[test]
    fn detect_empty_repo() {
        let files: Vec<String> = vec![];

        let result = detect_inferred_modules(&files, "empty");

        // No modules for empty repo
        assert!(result.modules.is_empty());
    }

    #[test]
    fn detect_mixed_source_and_test() {
        let files = vec![
            "src/main.c".to_string(),
            "src/test/unit_test.c".to_string(), // Test within src
            "tests/integration.c".to_string(),  // Top-level test dir
        ];

        let result = detect_inferred_modules(&files, "mixed");

        // Should detect src (has non-test sources), not tests
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "src");
    }

    #[test]
    fn top_level_directory_extraction() {
        assert_eq!(
            get_top_level_directory("src/foo/bar.c"),
            Some("src".to_string())
        );
        assert_eq!(get_top_level_directory("foo.c"), None);
        assert_eq!(get_top_level_directory("a/b.c"), Some("a".to_string()));
    }

    #[test]
    fn is_source_file_detection() {
        assert!(is_source_file("main.c"));
        assert!(is_source_file("foo.cpp"));
        assert!(is_source_file("bar.java"));
        assert!(is_source_file("baz.py"));
        assert!(is_source_file("qux.rs"));
        assert!(is_source_file("app.ts"));

        assert!(!is_source_file("Makefile"));
        assert!(!is_source_file("README.md"));
        assert!(!is_source_file("config.json"));
        assert!(!is_source_file("foo.txt"));
    }

    #[test]
    fn is_test_path_detection() {
        assert!(is_test_path("test/foo.c"));
        assert!(is_test_path("tests/bar.c"));
        assert!(is_test_path("src/test/unit.c"));
        assert!(is_test_path("test_main.c"));
        assert!(is_test_path("foo_test.c"));
        assert!(is_test_path("bar.test.js"));
        assert!(is_test_path("baz.spec.ts"));

        assert!(!is_test_path("src/main.c"));
        assert!(!is_test_path("lib/util.c"));
        assert!(!is_test_path("testing.c")); // File named testing.c is not a test
    }

    #[test]
    fn module_key_format() {
        let key = generate_module_key("sqlite", "src");
        assert_eq!(key, "inferred:sqlite:src");

        let key = generate_module_key("myapp", ".");
        assert_eq!(key, "inferred:myapp:.");
    }

    #[test]
    fn uid_determinism() {
        let uid1 = generate_module_uid("repo", "src");
        let uid2 = generate_module_uid("repo", "src");
        assert_eq!(uid1, uid2);

        let uid3 = generate_module_uid("repo", "lib");
        assert_ne!(uid1, uid3);
    }

    #[test]
    fn uid_prefix_distinguishes_from_others() {
        let inferred_uid = generate_module_uid("repo", "src");
        let cargo_uid = crate::cargo_manifest::generate_module_uid("repo", "src");
        let gradle_uid = crate::settings_gradle::generate_module_uid("repo", "src");

        assert!(inferred_uid.starts_with("inferred-mod-"));
        assert!(cargo_uid.starts_with("cargo-mod-"));
        assert!(gradle_uid.starts_with("gradle-mod-"));

        assert_ne!(inferred_uid, cargo_uid);
        assert_ne!(inferred_uid, gradle_uid);
    }

    #[test]
    fn storage_input_conversion() {
        let module = InferredModule {
            directory_path: "src".to_string(),
            display_name: "src".to_string(),
            source_file_count: 42,
            test_file_count: 8,
            is_fallback_root: false,
            build_files_present: vec!["Makefile".to_string()],
            dominant_language: Some("c".to_string()),
        };

        let (candidate, evidence) = to_storage_inputs(&module, "sqlite", "snap-1");

        assert_eq!(candidate.module_kind, "inferred");
        assert_eq!(candidate.canonical_root_path, "src");
        assert_eq!(candidate.display_name, "src");
        assert!((candidate.confidence - 0.7).abs() < f64::EPSILON);
        assert_eq!(candidate.module_key, "inferred:sqlite:src");

        assert_eq!(evidence.source_type, "directory_heuristic");
        assert_eq!(evidence.source_path, "src");
        assert_eq!(evidence.evidence_kind, "directory_structure");

        // Verify payload JSON
        let payload: InferredEvidencePayload =
            serde_json::from_str(&evidence.payload_json).unwrap();
        assert_eq!(payload.heuristic, "top_level_source_directory");
        assert_eq!(payload.source_file_count, 42);
        assert!(!payload.is_fallback_root);
        // Phase 3.2 fields
        assert_eq!(payload.evidence_strength, "build_marker_backed");
        assert_eq!(payload.build_files_present, vec!["Makefile"]);
        assert_eq!(payload.dominant_language, Some("c".to_string()));
    }

    #[test]
    fn confidence_is_lower_than_declared() {
        const { assert!(INFERRED_MODULE_CONFIDENCE < 1.0) };
        assert!((INFERRED_MODULE_CONFIDENCE - 0.7).abs() < f64::EPSILON);
    }

    // ── Exclusion tests (Phase 3.1) ──────────────────────────────────

    #[test]
    fn exclude_vendor_directories() {
        let files = vec![
            "src/main.c".to_string(),
            "vendor/lib/util.c".to_string(),
            "third_party/json/json.c".to_string(),
            "node_modules/pkg/index.js".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        // Should only have src, vendor/third_party/node_modules excluded
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "src");

        // Check exclusions
        assert_eq!(result.excluded_directories.len(), 3);
        let excluded_paths: HashSet<_> = result
            .excluded_directories
            .iter()
            .map(|e| e.directory_path.as_str())
            .collect();
        assert!(excluded_paths.contains("vendor"));
        assert!(excluded_paths.contains("third_party"));
        assert!(excluded_paths.contains("node_modules"));

        // Verify exclusion reasons
        for excl in &result.excluded_directories {
            assert_eq!(excl.reason, ExclusionReason::VendorDependency);
        }
    }

    #[test]
    fn exclude_build_output_directories() {
        let files = vec![
            "src/main.c".to_string(),
            "build/output.c".to_string(),
            "dist/bundle.js".to_string(),
            "target/debug/app.rs".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "src");

        assert_eq!(result.excluded_directories.len(), 3);
        for excl in &result.excluded_directories {
            assert_eq!(excl.reason, ExclusionReason::BuildOutput);
        }
    }

    #[test]
    fn exclude_documentation_directories() {
        let files = vec![
            "src/main.c".to_string(),
            "docs/example.c".to_string(),
            "Documentation/sample.c".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.excluded_directories.len(), 2);
        for excl in &result.excluded_directories {
            assert_eq!(excl.reason, ExclusionReason::Documentation);
        }
    }

    #[test]
    fn exclude_examples_directories() {
        let files = vec![
            "src/main.c".to_string(),
            "examples/demo.c".to_string(),
            "samples/sample.c".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.excluded_directories.len(), 2);
        for excl in &result.excluded_directories {
            assert_eq!(excl.reason, ExclusionReason::ExamplesOrSamples);
        }
    }

    #[test]
    fn exclude_benchmark_directories() {
        let files = vec![
            "src/main.c".to_string(),
            "benchmark/perf.c".to_string(),
            "benches/bench.rs".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.excluded_directories.len(), 2);
        for excl in &result.excluded_directories {
            assert_eq!(excl.reason, ExclusionReason::BenchmarkOnly);
        }
    }

    #[test]
    fn exclusion_check_function() {
        // Vendor
        assert_eq!(
            should_exclude_directory("vendor"),
            Some(ExclusionReason::VendorDependency)
        );
        assert_eq!(
            should_exclude_directory("third_party"),
            Some(ExclusionReason::VendorDependency)
        );
        assert_eq!(
            should_exclude_directory("node_modules"),
            Some(ExclusionReason::VendorDependency)
        );

        // Build
        assert_eq!(
            should_exclude_directory("build"),
            Some(ExclusionReason::BuildOutput)
        );
        assert_eq!(
            should_exclude_directory("dist"),
            Some(ExclusionReason::BuildOutput)
        );
        assert_eq!(
            should_exclude_directory("target"),
            Some(ExclusionReason::BuildOutput)
        );

        // Generated
        assert_eq!(
            should_exclude_directory("generated"),
            Some(ExclusionReason::GeneratedCode)
        );
        assert_eq!(
            should_exclude_directory("gen"),
            Some(ExclusionReason::GeneratedCode)
        );

        // Docs
        assert_eq!(
            should_exclude_directory("docs"),
            Some(ExclusionReason::Documentation)
        );
        assert_eq!(
            should_exclude_directory("Documentation"),
            Some(ExclusionReason::Documentation)
        );

        // Examples
        assert_eq!(
            should_exclude_directory("examples"),
            Some(ExclusionReason::ExamplesOrSamples)
        );
        assert_eq!(
            should_exclude_directory("samples"),
            Some(ExclusionReason::ExamplesOrSamples)
        );

        // Benchmarks
        assert_eq!(
            should_exclude_directory("benchmark"),
            Some(ExclusionReason::BenchmarkOnly)
        );
        assert_eq!(
            should_exclude_directory("benches"),
            Some(ExclusionReason::BenchmarkOnly)
        );

        // Not excluded
        assert_eq!(should_exclude_directory("src"), None);
        assert_eq!(should_exclude_directory("lib"), None);
        assert_eq!(should_exclude_directory("core"), None);
        assert_eq!(should_exclude_directory("internal"), None);
    }

    #[test]
    fn test_file_count_tracked_separately() {
        let files = vec![
            "src/main.c".to_string(),
            "src/util.c".to_string(),
            "src/test/unit_test.c".to_string(),
            "src/test/integration_test.c".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        let module = &result.modules[0];
        assert_eq!(module.source_file_count, 2); // main.c, util.c
        assert_eq!(module.test_file_count, 2); // unit_test.c, integration_test.c
    }

    // ── Build file detection tests (Phase 3.2) ───────────────────────────

    #[test]
    fn detect_build_files_in_directory() {
        let files = vec![
            "src/main.c".to_string(),
            "src/util.c".to_string(),
            "src/Makefile".to_string(),
            "lib/core.c".to_string(),
            "lib/CMakeLists.txt".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 2);

        let src_module = result
            .modules
            .iter()
            .find(|m| m.directory_path == "src")
            .unwrap();
        assert_eq!(src_module.build_files_present, vec!["Makefile"]);

        let lib_module = result
            .modules
            .iter()
            .find(|m| m.directory_path == "lib")
            .unwrap();
        assert_eq!(lib_module.build_files_present, vec!["CMakeLists.txt"]);
    }

    #[test]
    fn build_files_only_at_direct_level() {
        // Build files nested in subdirectories should NOT be counted
        let files = vec![
            "src/main.c".to_string(),
            "src/sub/Makefile".to_string(), // nested, should NOT count
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        let module = &result.modules[0];
        assert!(module.build_files_present.is_empty());
    }

    #[test]
    fn build_files_at_root() {
        let files = vec![
            "main.c".to_string(),
            "Makefile".to_string(),
            "CMakeLists.txt".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        let module = &result.modules[0];
        assert!(module.is_fallback_root);
        assert_eq!(
            module.build_files_present,
            vec!["CMakeLists.txt", "Makefile"]
        );
    }

    #[test]
    fn evidence_strength_basic_without_build_files() {
        let module = InferredModule {
            directory_path: "src".to_string(),
            display_name: "src".to_string(),
            source_file_count: 10,
            test_file_count: 0,
            is_fallback_root: false,
            build_files_present: vec![],
            dominant_language: None,
        };

        let (_, evidence) = to_storage_inputs(&module, "test", "snap-1");
        let payload: InferredEvidencePayload =
            serde_json::from_str(&evidence.payload_json).unwrap();

        assert_eq!(payload.evidence_strength, "basic");
    }

    #[test]
    fn evidence_strength_build_marker_backed() {
        let module = InferredModule {
            directory_path: "src".to_string(),
            display_name: "src".to_string(),
            source_file_count: 10,
            test_file_count: 0,
            is_fallback_root: false,
            build_files_present: vec!["Makefile".to_string()],
            dominant_language: None,
        };

        let (_, evidence) = to_storage_inputs(&module, "test", "snap-1");
        let payload: InferredEvidencePayload =
            serde_json::from_str(&evidence.payload_json).unwrap();

        assert_eq!(payload.evidence_strength, "build_marker_backed");
    }

    #[test]
    fn is_build_file_detection() {
        // Should detect
        assert!(is_build_file("CMakeLists.txt"));
        assert!(is_build_file("Makefile"));
        assert!(is_build_file("GNUmakefile"));
        assert!(is_build_file("makefile"));
        assert!(is_build_file("meson.build"));
        assert!(is_build_file("BUILD"));
        assert!(is_build_file("BUILD.bazel"));
        assert!(is_build_file("Kbuild"));

        // Should NOT detect
        assert!(!is_build_file("main.c"));
        assert!(!is_build_file("Makefile.am")); // autotools config, not direct makefile
        assert!(!is_build_file("configure.ac")); // excluded from initial set
        assert!(!is_build_file("SConstruct")); // excluded from initial set
    }

    // ── Dominant language tests (Phase 3.2) ──────────────────────────────

    #[test]
    fn dominant_language_plurality() {
        let files = vec![
            "src/main.c".to_string(),
            "src/util.c".to_string(),
            "src/helper.c".to_string(),
            "src/config.py".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        let module = &result.modules[0];
        assert_eq!(module.dominant_language, Some("c".to_string()));
    }

    #[test]
    fn dominant_language_tie_returns_none() {
        let files = vec![
            "src/main.c".to_string(),
            "src/util.c".to_string(),
            "src/app.py".to_string(),
            "src/helper.py".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        let module = &result.modules[0];
        // Exact tie (2 C, 2 Python) => None
        assert_eq!(module.dominant_language, None);
    }

    #[test]
    fn dominant_language_excludes_test_files() {
        // Implementation files: 3 C
        // Test files: 5 Python (should be ignored)
        let files = vec![
            "src/main.c".to_string(),
            "src/util.c".to_string(),
            "src/helper.c".to_string(),
            "src/test/test1.py".to_string(),
            "src/test/test2.py".to_string(),
            "src/test/test3.py".to_string(),
            "src/test/test4.py".to_string(),
            "src/test/test5.py".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        let module = &result.modules[0];
        // Dominant language should be C (from implementation files).
        // Test files (Python) should NOT be counted.
        assert_eq!(module.dominant_language, Some("c".to_string()));
    }

    #[test]
    fn dominant_language_multiple_extensions_same_language() {
        let files = vec![
            "src/main.cpp".to_string(),
            "src/util.cc".to_string(),
            "src/helper.cxx".to_string(),
            "src/header.hpp".to_string(),
        ];

        let result = detect_inferred_modules(&files, "test");

        assert_eq!(result.modules.len(), 1);
        let module = &result.modules[0];
        assert_eq!(module.dominant_language, Some("cpp".to_string()));
    }

    #[test]
    fn extension_to_language_mapping() {
        // C
        assert_eq!(extension_to_language("c"), Some("c"));
        assert_eq!(extension_to_language("h"), Some("c"));

        // C++
        assert_eq!(extension_to_language("cpp"), Some("cpp"));
        assert_eq!(extension_to_language("hpp"), Some("cpp"));
        assert_eq!(extension_to_language("cc"), Some("cpp"));
        assert_eq!(extension_to_language("cxx"), Some("cpp"));

        // Python
        assert_eq!(extension_to_language("py"), Some("python"));

        // Rust
        assert_eq!(extension_to_language("rs"), Some("rust"));

        // JavaScript/TypeScript
        assert_eq!(extension_to_language("js"), Some("javascript"));
        assert_eq!(extension_to_language("jsx"), Some("javascript"));
        assert_eq!(extension_to_language("ts"), Some("typescript"));
        assert_eq!(extension_to_language("tsx"), Some("typescript"));

        // Java/Kotlin
        assert_eq!(extension_to_language("java"), Some("java"));
        assert_eq!(extension_to_language("kt"), Some("kotlin"));

        // Non-source
        assert_eq!(extension_to_language("txt"), None);
        assert_eq!(extension_to_language("md"), None);
        assert_eq!(extension_to_language("json"), None);
    }

    #[test]
    fn compute_dominant_empty_counts() {
        let counts: HashMap<String, usize> = HashMap::new();
        assert_eq!(compute_dominant_language(&counts), None);
    }

    #[test]
    fn compute_dominant_single_language() {
        let mut counts = HashMap::new();
        counts.insert("rust".to_string(), 10);
        assert_eq!(compute_dominant_language(&counts), Some("rust".to_string()));
    }

    #[test]
    fn compute_dominant_clear_winner() {
        let mut counts = HashMap::new();
        counts.insert("c".to_string(), 100);
        counts.insert("python".to_string(), 5);
        assert_eq!(compute_dominant_language(&counts), Some("c".to_string()));
    }

    #[test]
    fn compute_dominant_exact_tie() {
        let mut counts = HashMap::new();
        counts.insert("c".to_string(), 50);
        counts.insert("python".to_string(), 50);
        assert_eq!(compute_dominant_language(&counts), None);
    }

    #[test]
    fn compute_dominant_three_way_tie() {
        let mut counts = HashMap::new();
        counts.insert("c".to_string(), 10);
        counts.insert("python".to_string(), 10);
        counts.insert("rust".to_string(), 10);
        assert_eq!(compute_dominant_language(&counts), None);
    }

    // ── Umbrella splitting tests (Phase 3.2) ─────────────────────────────

    #[test]
    fn umbrella_split_when_thresholds_met() {
        // Create nginx-like structure with enough files per child.
        // Thresholds: 2+ children, 5+ files per child, 5 or fewer direct in parent.
        let mut files = vec![];

        // src/core: 6 files
        for i in 0..6 {
            files.push(format!("src/core/file{}.c", i));
        }
        // src/http: 5 files
        for i in 0..5 {
            files.push(format!("src/http/file{}.c", i));
        }
        // src/event: 5 files
        for i in 0..5 {
            files.push(format!("src/event/file{}.c", i));
        }

        let result = detect_inferred_modules(&files, "nginx");

        // Should be split into 3 child modules (core, http, event).
        assert_eq!(
            result.modules.len(),
            3,
            "Expected 3 child modules, got {:?}",
            result
                .modules
                .iter()
                .map(|m| &m.directory_path)
                .collect::<Vec<_>>()
        );

        let paths: HashSet<_> = result
            .modules
            .iter()
            .map(|m| m.directory_path.as_str())
            .collect();
        assert!(paths.contains("src/core"));
        assert!(paths.contains("src/http"));
        assert!(paths.contains("src/event"));
        assert!(!paths.contains("src")); // Parent should not exist
    }

    #[test]
    fn umbrella_no_split_below_child_threshold() {
        // Children have < 5 files each (below threshold).
        let files = vec![
            "src/core/nginx.c".to_string(),
            "src/core/config.c".to_string(),
            "src/http/http.c".to_string(),
            "src/http/request.c".to_string(),
        ];

        let result = detect_inferred_modules(&files, "nginx");

        // Should NOT split (each child has only 2 files, threshold is 5).
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "src");
    }

    #[test]
    fn umbrella_no_split_single_child() {
        // Only one qualifying child (need at least 2).
        let mut files = vec![];
        for i in 0..10 {
            files.push(format!("src/core/file{}.c", i));
        }
        // Add a non-qualifying child (only 2 files).
        files.push("src/http/http.c".to_string());
        files.push("src/http/request.c".to_string());

        let result = detect_inferred_modules(&files, "nginx");

        // Should NOT split (only 1 qualifying child, threshold is 2).
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "src");
    }

    #[test]
    fn umbrella_no_split_too_many_direct_files() {
        // Parent has > 5 direct source files.
        let mut files = vec![];

        // Direct files in src/ (6 files, above ceiling of 5).
        for i in 0..6 {
            files.push(format!("src/main{}.c", i));
        }

        // Children that would otherwise qualify.
        for i in 0..5 {
            files.push(format!("src/core/file{}.c", i));
        }
        for i in 0..5 {
            files.push(format!("src/http/file{}.c", i));
        }

        let result = detect_inferred_modules(&files, "test");

        // Should NOT split (parent has 6 direct files, ceiling is 5).
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "src");
    }

    #[test]
    fn umbrella_split_with_direct_files_at_ceiling() {
        // Parent has exactly 5 direct files (at ceiling, should still split).
        let mut files = vec![];

        // Direct files in src/ (5 files, at ceiling).
        for i in 0..5 {
            files.push(format!("src/main{}.c", i));
        }

        // Children that qualify.
        for i in 0..5 {
            files.push(format!("src/core/file{}.c", i));
        }
        for i in 0..5 {
            files.push(format!("src/http/file{}.c", i));
        }

        let result = detect_inferred_modules(&files, "test");

        // Should split (parent has 5 direct files, ceiling is 5, so <= ceiling).
        assert_eq!(result.modules.len(), 2, "Expected 2 child modules");

        let paths: HashSet<_> = result
            .modules
            .iter()
            .map(|m| m.directory_path.as_str())
            .collect();
        assert!(paths.contains("src/core"));
        assert!(paths.contains("src/http"));
    }

    #[test]
    fn umbrella_non_umbrella_prefix_not_split() {
        // "lib" is not in the umbrella prefix list.
        let mut files = vec![];
        for i in 0..5 {
            files.push(format!("lib/core/file{}.c", i));
        }
        for i in 0..5 {
            files.push(format!("lib/util/file{}.c", i));
        }

        let result = detect_inferred_modules(&files, "test");

        // Should NOT split (lib is not an umbrella prefix).
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "lib");
    }

    #[test]
    fn umbrella_split_deterministic_order() {
        // Verify split modules are in deterministic order.
        let mut files = vec![];
        // Add in reverse alphabetical order.
        for i in 0..5 {
            files.push(format!("src/zebra/file{}.c", i));
        }
        for i in 0..5 {
            files.push(format!("src/alpha/file{}.c", i));
        }
        for i in 0..5 {
            files.push(format!("src/beta/file{}.c", i));
        }

        let result = detect_inferred_modules(&files, "test");

        // Should be in alphabetical order.
        assert_eq!(result.modules.len(), 3);
        assert_eq!(result.modules[0].directory_path, "src/alpha");
        assert_eq!(result.modules[1].directory_path, "src/beta");
        assert_eq!(result.modules[2].directory_path, "src/zebra");
    }

    #[test]
    fn umbrella_split_display_name_is_child_name() {
        let mut files = vec![];
        for i in 0..5 {
            files.push(format!("src/core/file{}.c", i));
        }
        for i in 0..5 {
            files.push(format!("src/http/file{}.c", i));
        }

        let result = detect_inferred_modules(&files, "nginx");

        // Display names should be the child directory names, not full paths.
        let core = result
            .modules
            .iter()
            .find(|m| m.directory_path == "src/core")
            .unwrap();
        assert_eq!(core.display_name, "core");

        let http = result
            .modules
            .iter()
            .find(|m| m.directory_path == "src/http")
            .unwrap();
        assert_eq!(http.display_name, "http");
    }

    #[test]
    fn umbrella_split_preserves_file_counts() {
        let mut files = vec![];
        // src/core: 6 source + 2 test.
        for i in 0..6 {
            files.push(format!("src/core/file{}.c", i));
        }
        files.push("src/core/test/test1.c".to_string());
        files.push("src/core/test/test2.c".to_string());

        // src/http: 5 source.
        for i in 0..5 {
            files.push(format!("src/http/file{}.c", i));
        }

        let result = detect_inferred_modules(&files, "test");

        let core = result
            .modules
            .iter()
            .find(|m| m.directory_path == "src/core")
            .unwrap();
        assert_eq!(core.source_file_count, 6);
        assert_eq!(core.test_file_count, 2);

        let http = result
            .modules
            .iter()
            .find(|m| m.directory_path == "src/http")
            .unwrap();
        assert_eq!(http.source_file_count, 5);
        assert_eq!(http.test_file_count, 0);
    }

    #[test]
    fn is_umbrella_candidate_detection() {
        // Should match (lowercase comparison).
        assert!(is_umbrella_candidate("src"));
        assert!(is_umbrella_candidate("Src"));
        assert!(is_umbrella_candidate("SRC"));
        assert!(is_umbrella_candidate("packages"));
        assert!(is_umbrella_candidate("services"));
        assert!(is_umbrella_candidate("apps"));
        assert!(is_umbrella_candidate("libs"));
        assert!(is_umbrella_candidate("modules"));

        // Should NOT match.
        assert!(!is_umbrella_candidate("lib")); // excluded from initial list
        assert!(!is_umbrella_candidate("components")); // excluded from initial list
        assert!(!is_umbrella_candidate("core"));
        assert!(!is_umbrella_candidate("utils"));
    }
}
