//! Modules list command.
//!
//! RS-MG-12b: Module list with rollup statistics.
//! Phase 3.1: Module sanity metrics for trust surface.
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_modules_list` handler
//! - `ModuleListEntry` DTO
//! - `ModuleSanityMetrics` DTO (Phase 3.1)
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - module graph loading (lives in `repo-graph-module-queries`)
//! - rollup computation (belongs in `repo-graph-classification`)

use std::path::Path;
use std::process::ExitCode;

use super::shared::{evaluate_violations_from_facts, load_module_graph_facts};
use crate::cli::{build_envelope, open_storage};

// ── modules list command ─────────────────────────────────────────

/// Sanity metrics for inferred module topology (Phase 3.1).
///
/// These metrics provide a trust surface for heuristic module detection.
/// Agents can use these to assess whether the module topology is reliable
/// enough for downstream analysis.
///
/// Unowned files are classified into three categories:
/// - excluded: in directories excluded by policy (vendor, docs, samples, benchmarks)
/// - suppressed_test: in test-only directories suppressed when real source dirs exist
/// - true_gap: could be owned but aren't (actual heuristic failures)
#[derive(serde::Serialize)]
struct ModuleSanityMetrics {
    /// Percentage of files owned by the largest module.
    /// High values (>80%) suggest coarse granularity.
    largest_module_ownership_pct: f64,
    /// Count of modules with fewer than 3 files.
    /// Many tiny modules may indicate over-splitting.
    tiny_module_count: u64,
    /// True if any module uses the flat-repo root fallback (canonical_root_path = ".").
    root_fallback_used: bool,
    /// Count of modules containing files in multiple programming languages.
    mixed_language_module_count: u64,
    /// True if any module is inferred (heuristic detection, not manifest-declared).
    has_inferred_modules: bool,
    /// Classified unowned file counts.
    unowned_breakdown: UnownedBreakdown,
}

/// Breakdown of unowned files by classification.
#[derive(serde::Serialize)]
struct UnownedBreakdown {
    /// Files in explicitly excluded directories (intentional).
    excluded_count: u64,
    /// Files in suppressed test-only directories (intentional).
    suppressed_test_count: u64,
    /// Files that could be owned but aren't (true heuristic gaps).
    true_gap_count: u64,
    /// True gap as percentage of total indexed files.
    true_gap_pct: f64,
    /// Percentage of unowned files that are classified (should be 100%).
    classified_pct: f64,
}

/// Output DTO for `modules list` command.
///
/// Dedicated CLI output shape — does not expose storage internals
/// like `snapshot_uid`, `repo_uid`, or `metadata_json`.
///
/// RS-MG-12b: Extended with rollup fields for per-module stats.
#[derive(serde::Serialize)]
struct ModuleListEntry {
    // Identity fields
    module_uid: String,
    module_key: String,
    canonical_root_path: String,
    module_kind: String,
    display_name: Option<String>,
    confidence: f64,
    // Rollup fields (RS-MG-12b)
    owned_file_count: u64,
    owned_test_file_count: u64,
    outbound_dependency_count: u64,
    outbound_import_count: u64,
    inbound_dependency_count: u64,
    inbound_import_count: u64,
    /// `None` when policy-derived rollups are unavailable (parse failure).
    /// `Some(0)` means zero violations; `None` means unknown.
    violation_count: Option<u64>,
    dead_symbol_count: u64,
    dead_test_symbol_count: u64,
}

pub(super) fn run_modules_list(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap modules list <db_path> <repo_uid>");
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

    // ── Step 1: Load module graph facts (single load) ─────────────
    let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // ── Step 2: Load dead nodes (SYMBOL kind only) ────────────────
    let dead_nodes = match storage.find_dead_nodes(&snapshot.snapshot_uid, repo_uid, Some("SYMBOL"))
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: failed to load dead nodes: {}", e);
            return ExitCode::from(2);
        }
    };

    // ── Step 3: Evaluate violations (advisory, uses preloaded facts) ─
    let (violations_eval, violations_warning): (
        Option<repo_graph_classification::boundary_evaluator::ModuleBoundaryEvaluation>,
        Option<String>,
    ) = match evaluate_violations_from_facts(&storage, repo_uid, &facts) {
        Ok(r) => (Some(r.evaluation), None),
        Err(msg) => (
            None,
            Some(format!(
                "discovered-module violation rollups unavailable: {}",
                msg
            )),
        ),
    };

    // ── Step 4: Compute rollups ───────────────────────────────────
    use repo_graph_classification::module_rollup::{
        compute_module_rollups, DeadNodeFact, ModuleRollupInput, OwnedFileFact,
    };

    let owned_file_facts: Vec<OwnedFileFact> = facts
        .owned_files()
        .iter()
        .map(|f| OwnedFileFact {
            file_path: f.file_path.clone(),
            module_uid: f.module_candidate_uid.clone(),
            is_test: f.is_test,
        })
        .collect();

    let dead_node_facts: Vec<DeadNodeFact> = dead_nodes
        .into_iter()
        .filter_map(|d| {
            d.file.map(|file_path| DeadNodeFact {
                file_path,
                is_test: d.is_test,
            })
        })
        .collect();

    // When violations are unavailable, pass empty vec — rollups will compute
    // violation_count as 0, but we'll override to None in the output.
    let violations_for_rollup = violations_eval
        .as_ref()
        .map(|e| e.violations.clone())
        .unwrap_or_default();

    let rollup_input = ModuleRollupInput {
        modules: facts.module_refs.clone(),
        owned_files: owned_file_facts,
        edges: facts.edges.clone(),
        violations: violations_for_rollup,
        dead_nodes: dead_node_facts,
    };

    let rollups = match compute_module_rollups(&rollup_input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to compute rollups: {}", e);
            return ExitCode::from(2);
        }
    };

    // ── Step 5: Build rollup lookup by module_uid ─────────────────
    use std::collections::HashMap;
    let rollup_map: HashMap<&str, &repo_graph_classification::module_rollup::ModuleRollup> =
        rollups.iter().map(|r| (r.module_uid.as_str(), r)).collect();

    // ── Step 6: Merge module identity with rollup stats ───────────
    // violation_count is None when violations_eval failed (policy unavailable)
    let violations_available = violations_eval.is_some();

    let results: Vec<ModuleListEntry> = facts
        .modules()
        .iter()
        .map(|m| {
            let rollup = rollup_map.get(m.module_candidate_uid.as_str());
            ModuleListEntry {
                module_uid: m.module_candidate_uid.clone(),
                module_key: m.module_key.clone(),
                canonical_root_path: m.canonical_root_path.clone(),
                module_kind: m.module_kind.clone(),
                display_name: m.display_name.clone(),
                confidence: m.confidence,
                // Rollup fields — default to 0 if rollup missing (shouldn't happen)
                owned_file_count: rollup.map_or(0, |r| r.owned_file_count),
                owned_test_file_count: rollup.map_or(0, |r| r.owned_test_file_count),
                outbound_dependency_count: rollup.map_or(0, |r| r.outbound_dependency_count),
                outbound_import_count: rollup.map_or(0, |r| r.outbound_import_count),
                inbound_dependency_count: rollup.map_or(0, |r| r.inbound_dependency_count),
                inbound_import_count: rollup.map_or(0, |r| r.inbound_import_count),
                // None when policy parsing failed; Some(count) when available
                violation_count: if violations_available {
                    Some(rollup.map_or(0, |r| r.violation_count))
                } else {
                    None
                },
                dead_symbol_count: rollup.map_or(0, |r| r.dead_symbol_count),
                dead_test_symbol_count: rollup.map_or(0, |r| r.dead_test_symbol_count),
            }
        })
        .collect();

    let count = results.len();

    // ── Step 7: Compute sanity metrics (Phase 3.1) ────────────────
    let sanity_metrics = compute_sanity_metrics(
        &results,
        &facts,
        snapshot.files_total as u64,
        &storage,
        &snapshot.snapshot_uid,
        repo_uid,
    );

    // Build extra envelope fields for degradation status
    let mut extra_fields = serde_json::Map::new();
    extra_fields.insert(
        "rollups_degraded".to_string(),
        serde_json::Value::Bool(!violations_available),
    );

    // Add sanity metrics to output
    extra_fields.insert(
        "sanity_metrics".to_string(),
        serde_json::to_value(&sanity_metrics).unwrap(),
    );

    // Build warnings list including degradation notices
    let mut warnings: Vec<String> = violations_warning.into_iter().collect();

    // Add degradation wording for inferred modules
    if sanity_metrics.has_inferred_modules {
        warnings.push(
			"Module topology includes inferred modules (heuristic detection, not manifest-declared). \
			Some directories are intentionally excluded from module ownership. \
			Use `rmap modules unowned` to see classification of files without module assignment.".to_string()
		);
    }

    // Add warning if true gaps exist
    if sanity_metrics.unowned_breakdown.true_gap_count > 0 {
        warnings.push(format!(
			"True heuristic gap: {} files could be owned but aren't. Run `rmap modules unowned` for details.",
			sanity_metrics.unowned_breakdown.true_gap_count
		));
    }

    extra_fields.insert(
        "warnings".to_string(),
        serde_json::to_value(&warnings).unwrap(),
    );

    let output = match build_envelope(
        &storage,
        "modules list",
        repo_uid,
        &snapshot,
        serde_json::to_value(&results).unwrap(),
        count,
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

// ── Sanity metrics computation (Phase 3.1) ────────────────────────

/// Compute sanity metrics for the module topology.
///
/// These metrics help agents assess whether the inferred module topology
/// is reliable for downstream analysis.
fn compute_sanity_metrics(
    results: &[ModuleListEntry],
    facts: &repo_graph_module_queries::ModuleGraphFacts,
    total_files: u64,
    storage: &repo_graph_storage::StorageConnection,
    snapshot_uid: &str,
    repo_uid: &str,
) -> ModuleSanityMetrics {
    use std::collections::{HashMap, HashSet};

    // largest_module_ownership_pct: % of files in largest module (relative to owned files)
    let total_owned: u64 = results.iter().map(|r| r.owned_file_count).sum();
    let max_owned: u64 = results
        .iter()
        .map(|r| r.owned_file_count)
        .max()
        .unwrap_or(0);
    let largest_module_ownership_pct = if total_owned > 0 {
        (max_owned as f64 / total_owned as f64) * 100.0
    } else {
        0.0
    };

    // tiny_module_count: modules with < 3 files
    const TINY_THRESHOLD: u64 = 3;
    let tiny_module_count = results
        .iter()
        .filter(|r| r.owned_file_count < TINY_THRESHOLD)
        .count() as u64;

    // root_fallback_used: any module has canonical_root_path == "."
    let root_fallback_used = results.iter().any(|r| r.canonical_root_path == ".");

    // has_inferred_modules: any module has module_kind == "inferred"
    let has_inferred_modules = results.iter().any(|r| r.module_kind == "inferred");

    // mixed_language_module_count: modules with files in > 1 language
    let mut languages_per_module: HashMap<&str, HashSet<&str>> = HashMap::new();
    for file in facts.owned_files() {
        let lang = infer_language_from_path(&file.file_path);
        languages_per_module
            .entry(file.module_candidate_uid.as_str())
            .or_default()
            .insert(lang);
    }
    let mixed_language_module_count = languages_per_module
        .values()
        .filter(|langs| langs.len() > 1)
        .count() as u64;

    // Compute unowned breakdown by classification
    let unowned_breakdown =
        compute_unowned_breakdown(storage, snapshot_uid, repo_uid, facts, results, total_files);

    ModuleSanityMetrics {
        largest_module_ownership_pct,
        tiny_module_count,
        root_fallback_used,
        mixed_language_module_count,
        has_inferred_modules,
        unowned_breakdown,
    }
}

/// Compute breakdown of unowned files by classification.
fn compute_unowned_breakdown(
    storage: &repo_graph_storage::StorageConnection,
    snapshot_uid: &str,
    repo_uid: &str,
    facts: &repo_graph_module_queries::ModuleGraphFacts,
    results: &[ModuleListEntry],
    total_files: u64,
) -> UnownedBreakdown {
    use std::collections::HashSet;

    // Get all file UIDs in snapshot
    let file_hashes = storage
        .query_file_version_hashes(snapshot_uid)
        .unwrap_or_default();

    // Build set of owned file UIDs
    let owned_uids: HashSet<&str> = facts
        .owned_files()
        .iter()
        .map(|f| f.file_uid.as_str())
        .collect();

    // Build set of module root paths
    let module_roots: HashSet<&str> = results
        .iter()
        .map(|m| m.canonical_root_path.as_str())
        .collect();

    let mut excluded_count: u64 = 0;
    let mut suppressed_test_count: u64 = 0;
    let mut true_gap_count: u64 = 0;

    for file_uid in file_hashes.keys() {
        if owned_uids.contains(file_uid.as_str()) {
            continue;
        }

        // Extract path from file_uid (format: repo_uid:path)
        let path = file_uid
            .strip_prefix(&format!("{}:", repo_uid))
            .unwrap_or(file_uid);

        // Only count source files
        if !is_source_file(path) {
            continue;
        }

        // Classify the unowned file
        let top_level = path.split('/').next().unwrap_or("");

        if is_excluded_directory(top_level) {
            excluded_count += 1;
        } else if is_test_directory(top_level) {
            suppressed_test_count += 1;
        } else if !path.contains('/') {
            // Root-level source file with no module
            true_gap_count += 1;
        } else if module_roots.contains(top_level) {
            // Under a module root but not owned - ownership bug
            true_gap_count += 1;
        } else {
            // Directory not recognized as module
            true_gap_count += 1;
        }
    }

    let _total_unowned = excluded_count + suppressed_test_count + true_gap_count;
    let true_gap_pct = if total_files > 0 {
        (true_gap_count as f64 / total_files as f64) * 100.0
    } else {
        0.0
    };
    // All unowned files are now classified
    let classified_pct = 100.0;

    UnownedBreakdown {
        excluded_count,
        suppressed_test_count,
        true_gap_count,
        true_gap_pct,
        classified_pct,
    }
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

/// Infer programming language from file path extension.
fn infer_language_from_path(path: &str) -> &'static str {
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
