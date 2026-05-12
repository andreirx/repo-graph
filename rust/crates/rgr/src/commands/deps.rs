//! Package dependency reconciliation commands (DEP-1).
//!
//! Reconciles declared dependencies (from manifests) with observed
//! external references (from imports) to produce module-level
//! dependency summaries.
//!
//! # Commands
//!
//! - `rmap deps list <db_path> <repo_uid> [module]` — list dependencies for all modules or a specific module
//! - `rmap deps why <db_path> <repo_uid> <package>` — explain why a package is used
//! - `rmap deps drift <db_path> <repo_uid>` — show dependency drift anomalies
//!
//! # Boundary rules
//!
//! This module owns:
//! - `run_deps` handler and subcommand dispatch
//! - CLI rendering of dependency summaries
//!
//! This module does **not** own:
//! - Reconciliation logic (lives in `repo-graph-module-queries::deps`)
//! - Storage queries (lives in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use repo_graph_module_queries::{
    build_identifier_resolution_map, cargo_runtime_builtins, compose_dependency_summaries,
    npm_runtime_builtins, resolve_import_specifier, ComposeDependenciesInput, DependencyCategory,
};

use crate::cli::{build_envelope, open_storage};

/// Entry point for `rmap deps` command family.
pub fn run_deps(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: rmap deps <subcommand> <args...>");
        eprintln!("subcommands: list, why, drift");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_deps_list(&args[1..]),
        "why" => run_deps_why(&args[1..]),
        "drift" => run_deps_drift(&args[1..]),
        other => {
            eprintln!("unknown deps subcommand: {}", other);
            eprintln!("subcommands: list, why, drift");
            ExitCode::from(1)
        }
    }
}

// ── deps list command ─────────────────────────────────────────────

fn run_deps_list(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [module] [--ecosystem npm|cargo]
    let (positional, ecosystem) = match parse_deps_list_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap deps list <db_path> <repo_uid> [module] [--ecosystem npm|cargo] [--format json]");
            return ExitCode::from(1);
        }
    };

    if positional.len() < 2 || positional.len() > 3 {
        eprintln!("usage: rmap deps list <db_path> <repo_uid> [module] [--ecosystem npm|cargo] [--format json]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&positional[0]);
    let repo_uid = &positional[1];
    let module_filter: Option<&str> = positional.get(2).map(|s| s.as_str());

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

    // Select runtime builtins based on ecosystem
    let runtime_builtins = match ecosystem.as_str() {
        "cargo" => cargo_runtime_builtins(),
        _ => npm_runtime_builtins(),
    };

    let input = ComposeDependenciesInput {
        snapshot_uid: &snapshot.snapshot_uid,
        runtime_builtins,
        ecosystem: ecosystem.clone(),
    };

    let result = match compose_dependency_summaries(&storage, &input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Filter to specific module if requested
    let summaries: Vec<_> = if let Some(filter) = module_filter {
        result
            .summaries
            .into_iter()
            .filter(|s| {
                s.module == filter
                    || s.module.ends_with(&format!("/{}", filter))
                    || s.module.starts_with(&format!("{}/", filter))
            })
            .collect()
    } else {
        result.summaries
    };

    if let Some(filter) = module_filter.as_ref() {
        if summaries.is_empty() {
            eprintln!("error: no dependencies found for module '{}'", filter);
            eprintln!("hint: use canonical path (e.g., 'packages/app') or check rmap modules list");
            return ExitCode::from(1);
        }
    }

    // Build JSON output
    let results: Vec<serde_json::Value> = summaries
        .iter()
        .map(|s| {
            let entries: Vec<serde_json::Value> = s
                .entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "package": e.package,
                        "category": format_category(e.category),
                        "import_count": e.import_count,
                        "confidence": e.confidence,
                    })
                })
                .collect();

            serde_json::json!({
                "module": s.module,
                "manifest_path": s.manifest_path,
                "manifest_scope_available": s.manifest_scope_available,
                "declared_and_used": s.declared_and_used_count(),
                "declared_but_unobserved": s.declared_but_unobserved_count(),
                "observed_but_undeclared": s.observed_but_undeclared_count(),
                "runtime_builtins": s.runtime_builtins_count(),
                "entries": entries,
            })
        })
        .collect();

    let count = results.len();

    // Build extra fields for envelope
    let mut extra = serde_json::Map::new();
    if let Some(m) = module_filter {
        extra.insert(
            "module_filter".to_string(),
            serde_json::Value::String(m.to_string()),
        );
    }
    extra.insert(
        "ecosystem".to_string(),
        serde_json::Value::String(ecosystem),
    );
    extra.insert(
        "total_external_imports".to_string(),
        serde_json::Value::Number(result.total_external_imports.into()),
    );
    extra.insert(
        "modules_without_manifest_context".to_string(),
        serde_json::Value::Number(result.modules_without_manifest_context.len().into()),
    );

    let output = match build_envelope(
        &storage,
        "deps list",
        repo_uid,
        &snapshot,
        serde_json::to_value(&results).unwrap(),
        count,
        extra,
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

// ── deps why command ──────────────────────────────────────────────

fn run_deps_why(args: &[String]) -> ExitCode {
    use repo_graph_module_queries::{normalize_cargo_specifier, normalize_npm_specifier};
    use std::collections::HashMap;

    // Parse args: <db_path> <repo_uid> <package> [--ecosystem npm|cargo]
    let (positional, ecosystem) = match parse_deps_why_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap deps why <db_path> <repo_uid> <package> [--ecosystem npm|cargo] [--format json]");
            return ExitCode::from(1);
        }
    };

    if positional.len() != 3 {
        eprintln!("usage: rmap deps why <db_path> <repo_uid> <package> [--ecosystem npm|cargo] [--format json]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&positional[0]);
    let repo_uid = &positional[1];
    let package_query = &positional[2];

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

    // 1. Load module_candidates for file → module mapping
    let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };
    let uid_to_canonical: HashMap<&str, &str> = modules
        .iter()
        .map(|m| {
            (
                m.module_candidate_uid.as_str(),
                m.canonical_root_path.as_str(),
            )
        })
        .collect();

    // 2. Load file ownership
    let ownership = match storage.get_file_ownership_for_snapshot(&snapshot.snapshot_uid) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };
    let file_to_module: HashMap<&str, &str> = ownership
        .iter()
        .filter_map(|o| {
            uid_to_canonical
                .get(o.module_candidate_uid.as_str())
                .map(|&path| (o.file_uid.as_str(), path))
        })
        .collect();

    // 3. Load external imports with file locations
    let imports_with_locations =
        match storage.get_external_imports_with_locations(&snapshot.snapshot_uid) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::from(2);
            }
        };

    // 3.5. Load import bindings for identifier → specifier resolution
    let import_bindings =
        match storage.get_external_import_bindings_for_snapshot(&snapshot.snapshot_uid) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("error: {}", e);
                return ExitCode::from(2);
            }
        };
    let identifier_to_specifier = build_identifier_resolution_map(&import_bindings);

    // 4. Also get reconciliation summaries to check if package is declared
    let runtime_builtins = match ecosystem.as_str() {
        "cargo" => cargo_runtime_builtins(),
        _ => npm_runtime_builtins(),
    };
    let compose_input = ComposeDependenciesInput {
        snapshot_uid: &snapshot.snapshot_uid,
        runtime_builtins,
        ecosystem: ecosystem.clone(),
    };
    let reconciled = match compose_dependency_summaries(&storage, &compose_input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build lookup: module → (is_declared, category)
    let mut module_decl_info: HashMap<&str, (bool, &str)> = HashMap::new();
    for summary in &reconciled.summaries {
        for entry in &summary.entries {
            if entry.package == *package_query {
                let declared = matches!(
                    entry.category,
                    DependencyCategory::DeclaredAndUsed | DependencyCategory::DeclaredButUnobserved
                );
                module_decl_info
                    .insert(&summary.module, (declared, format_category(entry.category)));
            }
        }
    }

    // 5. Filter imports to queried package, group by module
    let normalizer: fn(&str) -> String = match ecosystem.as_str() {
        "cargo" => normalize_cargo_specifier,
        _ => normalize_npm_specifier,
    };

    // Group imports by module
    let mut module_samples: HashMap<String, Vec<serde_json::Value>> = HashMap::new();

    for import in &imports_with_locations {
        // Resolve the callee identifier to import specifier first
        let resolved = resolve_import_specifier(
            &import.specifier,
            &import.file_uid,
            &identifier_to_specifier,
        );
        let normalized = normalizer(&resolved);
        if normalized != *package_query {
            continue;
        }

        // Find module for this file
        if let Some(&module_path) = file_to_module.get(import.file_uid.as_str()) {
            let sample = serde_json::json!({
                "file_path": import.file_path,
                "specifier": import.specifier,  // Original specifier for debugging
                "resolved_to": resolved,         // Resolved package name
                "line": import.line_start,
                "column": import.col_start,
            });
            module_samples
                .entry(module_path.to_string())
                .or_default()
                .push(sample);
        }
    }

    if module_samples.is_empty() {
        eprintln!("error: package '{}' not found in any module", package_query);
        eprintln!("hint: check package name or try rmap deps list to see all packages");
        return ExitCode::from(1);
    }

    // 6. Build output with module summary + sample imports
    let mut usages: Vec<serde_json::Value> = Vec::new();

    for (module_path, samples) in &module_samples {
        let (declared, category) = module_decl_info
            .get(module_path.as_str())
            .copied()
            .unwrap_or((false, "unknown"));

        // Limit samples to 5 per module
        let limited_samples: Vec<_> = samples.iter().take(5).cloned().collect();

        usages.push(serde_json::json!({
            "module": module_path,
            "import_count": samples.len(),
            "declared": declared,
            "category": category,
            "sample_imports": limited_samples,
        }));
    }

    // Sort by module path
    usages.sort_by(|a, b| {
        a.get("module")
            .and_then(|v| v.as_str())
            .cmp(&b.get("module").and_then(|v| v.as_str()))
    });

    let count = usages.len();

    // Build extra fields for envelope
    let mut extra = serde_json::Map::new();
    extra.insert(
        "package".to_string(),
        serde_json::Value::String(package_query.clone()),
    );
    extra.insert(
        "ecosystem".to_string(),
        serde_json::Value::String(ecosystem),
    );

    let output = match build_envelope(
        &storage,
        "deps why",
        repo_uid,
        &snapshot,
        serde_json::to_value(&usages).unwrap(),
        count,
        extra,
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

// ── deps drift command ────────────────────────────────────────────

fn run_deps_drift(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [--ecosystem npm|cargo]
    let (positional, ecosystem) = match parse_deps_drift_args(args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            eprintln!("usage: rmap deps drift <db_path> <repo_uid> [--ecosystem npm|cargo] [--format json]");
            return ExitCode::from(1);
        }
    };

    if positional.len() != 2 {
        eprintln!(
            "usage: rmap deps drift <db_path> <repo_uid> [--ecosystem npm|cargo] [--format json]"
        );
        return ExitCode::from(1);
    }

    let db_path = Path::new(&positional[0]);
    let repo_uid = &positional[1];

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

    // Select runtime builtins based on ecosystem
    let runtime_builtins = match ecosystem.as_str() {
        "cargo" => cargo_runtime_builtins(),
        _ => npm_runtime_builtins(),
    };

    let input = ComposeDependenciesInput {
        snapshot_uid: &snapshot.snapshot_uid,
        runtime_builtins,
        ecosystem: ecosystem.clone(),
    };

    let result = match compose_dependency_summaries(&storage, &input) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Collect drift anomalies across all modules
    let mut drift_entries: Vec<serde_json::Value> = Vec::new();

    for summary in &result.summaries {
        // DeclaredButUnobserved -> unused
        for entry in summary.by_category(DependencyCategory::DeclaredButUnobserved) {
            drift_entries.push(serde_json::json!({
                "module": summary.module,
                "package": entry.package,
                "kind": "unused_declared",
                "hint": "Package is declared in manifest but no imports found. Consider removing.",
            }));
        }

        // ObservedButUndeclared -> missing
        for entry in summary.by_category(DependencyCategory::ObservedButUndeclared) {
            drift_entries.push(serde_json::json!({
                "module": summary.module,
                "package": entry.package,
                "kind": "undeclared_usage",
                "import_count": entry.import_count,
                "hint": "Package is imported but not declared in manifest. Add to dependencies.",
            }));
        }

        // UnknownExternalLike -> unclear
        for entry in summary.by_category(DependencyCategory::UnknownExternalLike) {
            drift_entries.push(serde_json::json!({
                "module": summary.module,
                "package": entry.package,
                "kind": "unknown_external",
                "import_count": entry.import_count,
                "hint": "External-looking import but manifest context unavailable. Verify dependency.",
            }));
        }
    }

    let count = drift_entries.len();

    // Build extra fields for envelope
    let mut extra = serde_json::Map::new();
    extra.insert(
        "ecosystem".to_string(),
        serde_json::Value::String(ecosystem),
    );
    extra.insert(
        "modules_analyzed".to_string(),
        serde_json::Value::Number(result.summaries.len().into()),
    );
    extra.insert(
        "modules_without_manifest_context".to_string(),
        serde_json::Value::Number(result.modules_without_manifest_context.len().into()),
    );

    let output = match build_envelope(
        &storage,
        "deps drift",
        repo_uid,
        &snapshot,
        serde_json::to_value(&drift_entries).unwrap(),
        count,
        extra,
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
            // Exit with 1 if there are drift anomalies (matches gate behavior)
            if count > 0 {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── Argument parsing helpers ──────────────────────────────────────

fn parse_deps_list_args(args: &[String]) -> Result<(Vec<String>, String), String> {
    let mut positional = Vec::new();
    let mut ecosystem = "npm".to_string();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--ecosystem" => {
                if i + 1 >= args.len() {
                    return Err("--ecosystem requires a value (npm or cargo)".to_string());
                }
                ecosystem = args[i + 1].clone();
                if ecosystem != "npm" && ecosystem != "cargo" {
                    return Err(format!(
                        "invalid ecosystem: {} (expected npm or cargo)",
                        ecosystem
                    ));
                }
                i += 2;
            }
            "--format" => {
                if i + 1 >= args.len() {
                    return Err("--format requires a value (json)".to_string());
                }
                let format = &args[i + 1];
                if format != "json" {
                    return Err(format!(
                        "unsupported format: {} (only json is supported)",
                        format
                    ));
                }
                // JSON is the default and only format, so this is a no-op
                i += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {}", other));
            }
            _ => {
                positional.push(args[i].clone());
                i += 1;
            }
        }
    }

    Ok((positional, ecosystem))
}

fn parse_deps_drift_args(args: &[String]) -> Result<(Vec<String>, String), String> {
    // Same as list but without module filter
    parse_deps_list_args(args)
}

fn parse_deps_why_args(args: &[String]) -> Result<(Vec<String>, String), String> {
    // Same as list — parses <db_path> <repo_uid> <package> [--ecosystem]
    parse_deps_list_args(args)
}

fn format_category(cat: DependencyCategory) -> &'static str {
    match cat {
        DependencyCategory::DeclaredAndUsed => "declared_and_used",
        DependencyCategory::DeclaredButUnobserved => "declared_but_unobserved",
        DependencyCategory::ObservedButUndeclared => "observed_but_undeclared",
        DependencyCategory::RuntimeBuiltin => "runtime_builtin",
        DependencyCategory::UnknownExternalLike => "unknown_external_like",
    }
}
