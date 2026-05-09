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
}

/// Result of inferred module detection for a repo.
#[derive(Debug, Clone, Default)]
pub struct InferredModuleResult {
    /// Inferred modules
    pub modules: Vec<InferredModule>,
    /// Directories that were excluded from inference
    pub excluded_directories: Vec<ExcludedDirectory>,
}

/// Evidence payload for inferred modules (Phase 3.1 enriched).
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
    let mut root_source_count = 0usize;
    let mut root_test_count = 0usize;

    for path in file_paths {
        // Skip non-source files.
        if !is_source_file(path) {
            continue;
        }

        let is_test = is_test_path(path);

        // Determine top-level directory.
        let top_level = get_top_level_directory(path);

        match top_level {
            Some(dir) => {
                let stats = dir_stats.entry(dir).or_default();
                if is_test {
                    stats.test_file_count += 1;
                } else {
                    stats.source_file_count += 1;
                }
            }
            None => {
                // File is at repo root.
                if is_test {
                    root_test_count += 1;
                } else {
                    root_source_count += 1;
                }
            }
        }
    }

    // Partition directories into included and excluded.
    let mut source_dirs: Vec<(String, usize, usize)> = Vec::new(); // (dir, source_count, test_count)
    let mut test_only_dirs: Vec<(String, usize)> = Vec::new();     // (dir, test_count)

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
            source_dirs.push((dir.clone(), stats.source_file_count, stats.test_file_count));
        } else if stats.test_file_count > 0 {
            test_only_dirs.push((dir.clone(), stats.test_file_count));
        }
    }

    // If we have source directories, create modules for them.
    if !source_dirs.is_empty() {
        for (dir, source_count, test_count) in source_dirs {
            result.modules.push(InferredModule {
                directory_path: dir.clone(),
                display_name: dir.clone(),
                source_file_count: source_count,
                test_file_count: test_count,
                is_fallback_root: false,
            });
        }
        return result;
    }

    // No source directories. Check for test-only directories as fallback.
    if !test_only_dirs.is_empty() {
        for (dir, test_count) in test_only_dirs {
            result.modules.push(InferredModule {
                directory_path: dir.clone(),
                display_name: dir.clone(),
                source_file_count: 0,
                test_file_count: test_count,
                is_fallback_root: false,
            });
        }
        return result;
    }

    // No top-level directories with source files.
    // Fall back to root module if there are source files at root.
    if root_source_count > 0 || root_test_count > 0 {
        result.modules.push(InferredModule {
            directory_path: ".".to_string(),
            display_name: repo_display_name.to_string(),
            source_file_count: root_source_count,
            test_file_count: root_test_count,
            is_fallback_root: true,
        });
    }

    result
}

#[derive(Default)]
struct DirectoryStats {
    source_file_count: usize,
    test_file_count: usize,
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
        "c" | "h" | "cpp" | "hpp" | "cc" | "hh" | "cxx" | "hxx"
            | "java" | "kt" | "scala"
            | "py"
            | "rs"
            | "go"
            | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs"
            | "rb"
            | "swift"
            | "m" | "mm"  // Objective-C
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
        "vendor" | "vendors"
            | "third_party" | "third-party" | "thirdparty"
            | "node_modules"
            | "bower_components"
            | "jspm_packages"
            | "external" | "externals"
            | "deps" | "dependencies"
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
        "generated" | "gen" | "codegen"
            | "auto" | "autogen"
            | "__generated__"
    ) {
        return Some(ExclusionReason::GeneratedCode);
    }

    // Documentation directories
    if matches!(
        dir_lower.as_str(),
        "docs" | "doc" | "documentation"
            | "man" | "manpages"
            | "javadoc" | "apidoc" | "apidocs"
    ) {
        return Some(ExclusionReason::Documentation);
    }

    // Examples/samples directories
    if matches!(
        dir_lower.as_str(),
        "examples" | "example"
            | "samples" | "sample"
            | "demo" | "demos"
            | "tutorials" | "tutorial"
    ) {
        return Some(ExclusionReason::ExamplesOrSamples);
    }

    // Benchmark-only directories
    if matches!(
        dir_lower.as_str(),
        "benchmark" | "benchmarks" | "bench" | "benches"
            | "perf" | "performance"
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

    let payload = InferredEvidencePayload {
        heuristic: "top_level_source_directory".to_string(),
        directory_path: module.directory_path.clone(),
        source_file_count: module.source_file_count,
        test_file_count: module.test_file_count,
        is_fallback_root: module.is_fallback_root,
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

        let dirs: HashSet<_> = result.modules.iter().map(|m| m.directory_path.as_str()).collect();
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
        let files = vec![
            "test/test1.c".to_string(),
            "test/test2.c".to_string(),
        ];

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
            "src/test/unit_test.c".to_string(),  // Test within src
            "tests/integration.c".to_string(),   // Top-level test dir
        ];

        let result = detect_inferred_modules(&files, "mixed");

        // Should detect src (has non-test sources), not tests
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].directory_path, "src");
    }

    #[test]
    fn top_level_directory_extraction() {
        assert_eq!(get_top_level_directory("src/foo/bar.c"), Some("src".to_string()));
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
    }

    #[test]
    fn confidence_is_lower_than_declared() {
        assert!(INFERRED_MODULE_CONFIDENCE < 1.0);
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
        let excluded_paths: HashSet<_> = result.excluded_directories.iter()
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
        assert_eq!(should_exclude_directory("vendor"), Some(ExclusionReason::VendorDependency));
        assert_eq!(should_exclude_directory("third_party"), Some(ExclusionReason::VendorDependency));
        assert_eq!(should_exclude_directory("node_modules"), Some(ExclusionReason::VendorDependency));

        // Build
        assert_eq!(should_exclude_directory("build"), Some(ExclusionReason::BuildOutput));
        assert_eq!(should_exclude_directory("dist"), Some(ExclusionReason::BuildOutput));
        assert_eq!(should_exclude_directory("target"), Some(ExclusionReason::BuildOutput));

        // Generated
        assert_eq!(should_exclude_directory("generated"), Some(ExclusionReason::GeneratedCode));
        assert_eq!(should_exclude_directory("gen"), Some(ExclusionReason::GeneratedCode));

        // Docs
        assert_eq!(should_exclude_directory("docs"), Some(ExclusionReason::Documentation));
        assert_eq!(should_exclude_directory("Documentation"), Some(ExclusionReason::Documentation));

        // Examples
        assert_eq!(should_exclude_directory("examples"), Some(ExclusionReason::ExamplesOrSamples));
        assert_eq!(should_exclude_directory("samples"), Some(ExclusionReason::ExamplesOrSamples));

        // Benchmarks
        assert_eq!(should_exclude_directory("benchmark"), Some(ExclusionReason::BenchmarkOnly));
        assert_eq!(should_exclude_directory("benches"), Some(ExclusionReason::BenchmarkOnly));

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
        assert_eq!(module.source_file_count, 2);  // main.c, util.c
        assert_eq!(module.test_file_count, 2);    // unit_test.c, integration_test.c
    }
}
