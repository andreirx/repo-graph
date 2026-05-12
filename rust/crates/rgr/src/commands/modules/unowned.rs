//! Unowned files analysis command (Phase 3.1B).
//!
//! Lists source files that are not assigned to any module, grouped by reason.
//! This is a diagnostic command for understanding ownership gaps before
//! deprecating MODULE-node fallback.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage};

// ── Output DTOs ──────────────────────────────────────────────────────

/// An unowned file with classification reason.
#[derive(serde::Serialize)]
struct UnownedFile {
    file_path: String,
    language: String,
    reason: String,
}

/// Summary of unowned files by reason.
#[derive(serde::Serialize)]
struct UnownedSummary {
    total_indexed_files: u64,
    total_owned_files: u64,
    total_unowned_files: u64,
    unowned_pct: f64,
    by_reason: HashMap<String, u64>,
}

// ── Command handler ──────────────────────────────────────────────────

pub(super) fn run_modules_unowned(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap modules unowned <db_path> <repo_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Get all indexed files via file_version_hashes (snapshot-scoped)
    let file_version_hashes = match storage.query_file_version_hashes(&snapshot.snapshot_uid) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: failed to load files: {}", e);
            return ExitCode::from(2);
        }
    };

    // Extract file paths from file_uids (format: repo_uid:path)
    let all_file_paths: Vec<(String, String)> = file_version_hashes
        .keys()
        .map(|file_uid| {
            let path = file_uid
                .strip_prefix(&format!("{}:", repo_uid))
                .unwrap_or(file_uid)
                .to_string();
            (file_uid.clone(), path)
        })
        .collect();

    // Get owned files
    let ownership = match storage.get_file_ownership_for_snapshot(&snapshot.snapshot_uid) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: failed to load ownership: {}", e);
            return ExitCode::from(2);
        }
    };

    // Get module candidates for context
    let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: failed to load modules: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build set of owned file UIDs
    let owned_uids: HashSet<&str> = ownership.iter().map(|o| o.file_uid.as_str()).collect();

    // Build set of module root paths for classification
    let module_roots: HashSet<&str> = modules
        .iter()
        .map(|m| m.canonical_root_path.as_str())
        .collect();

    // Find unowned files and classify reasons
    let mut unowned_files: Vec<UnownedFile> = Vec::new();
    let mut by_reason: HashMap<String, u64> = HashMap::new();

    for (file_uid, path) in &all_file_paths {
        if owned_uids.contains(file_uid.as_str()) {
            continue;
        }

        // Only count source files as "eligible" unowned
        if !is_source_file(path) {
            continue;
        }

        let reason = classify_unowned_reason(path, &module_roots);
        let language = infer_language(path);

        *by_reason.entry(reason.clone()).or_insert(0) += 1;

        unowned_files.push(UnownedFile {
            file_path: path.clone(),
            language: language.to_string(),
            reason,
        });
    }

    // Sort by reason then path
    unowned_files.sort_by(|a, b| {
        a.reason
            .cmp(&b.reason)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    // Compute summary
    let total_indexed = all_file_paths.len() as u64;
    let total_owned = ownership.len() as u64;
    let total_unowned = unowned_files.len() as u64;
    let unowned_pct = if total_indexed > 0 {
        (total_unowned as f64 / total_indexed as f64) * 100.0
    } else {
        0.0
    };

    let summary = UnownedSummary {
        total_indexed_files: total_indexed,
        total_owned_files: total_owned,
        total_unowned_files: total_unowned,
        unowned_pct,
        by_reason,
    };

    // Build output
    let mut extra_fields = serde_json::Map::new();
    extra_fields.insert(
        "summary".to_string(),
        serde_json::to_value(&summary).unwrap(),
    );

    let output = match build_envelope(
        &storage,
        "modules unowned",
        repo_uid,
        &snapshot,
        serde_json::to_value(&unowned_files).unwrap(),
        unowned_files.len(),
        extra_fields,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── Classification helpers ───────────────────────────────────────────

/// Classify why a file is unowned.
fn classify_unowned_reason(path: &str, module_roots: &HashSet<&str>) -> String {
    // Check if file is at repo root (no directory)
    if !path.contains('/') {
        return "root_source_no_module".to_string();
    }

    // Get top-level directory
    let top_level = path.split('/').next().unwrap_or("");

    // Check if in an excluded directory
    if is_excluded_directory(top_level) {
        return format!("excluded_directory:{}", top_level);
    }

    // Check if parent would be a module root
    if module_roots.contains(top_level) {
        // File is under a detected module but not owned - ownership bug
        return "ownership_computation_gap".to_string();
    }

    // Check if it's a test directory that was suppressed
    if is_test_directory(top_level) {
        return "suppressed_test_directory".to_string();
    }

    // Generic heuristic gap
    format!("heuristic_gap:{}", top_level)
}

/// Check if a directory name is in the exclusion list.
fn is_excluded_directory(dir_name: &str) -> bool {
    let dir_lower = dir_name.to_lowercase();
    matches!(
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
            | "dist"
            | "build"
            | "builds"
            | "out"
            | "output"
            | "target"
            | "bin"
            | "obj"
            | "_build"
            | "generated"
            | "gen"
            | "codegen"
            | "auto"
            | "autogen"
            | "__generated__"
            | "docs"
            | "doc"
            | "documentation"
            | "man"
            | "manpages"
            | "javadoc"
            | "apidoc"
            | "apidocs"
            | "examples"
            | "example"
            | "samples"
            | "sample"
            | "demo"
            | "demos"
            | "tutorials"
            | "tutorial"
            | "benchmark"
            | "benchmarks"
            | "bench"
            | "benches"
            | "perf"
            | "performance"
    )
}

/// Check if a directory is a test directory.
fn is_test_directory(dir_name: &str) -> bool {
    let dir_lower = dir_name.to_lowercase();
    matches!(
        dir_lower.as_str(),
        "test" | "tests" | "testing" | "__tests__" | "spec" | "specs"
    )
}

/// Check if a file is a source file.
fn is_source_file(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(
        ext.to_lowercase().as_str(),
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
            | "mm"
    )
}

/// Infer language from file extension.
fn infer_language(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext.to_lowercase().as_str() {
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "py" | "pyi" => "python",
        "rs" => "rust",
        "go" => "go",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "rb" => "ruby",
        "swift" => "swift",
        "m" | "mm" => "objc",
        _ => "other",
    }
}
