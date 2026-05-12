//! Dependency reconciliation composition layer.
//!
//! Orchestrates data loading from storage and passes it to the
//! reconciliation engine. This is the entry point for DEP-1
//! dependency analysis.

use std::collections::{HashMap, HashSet};

use repo_graph_storage::types::ModuleCandidate;
use repo_graph_storage::StorageConnection;

use super::reconcile::{reconcile_module_dependencies, ReconcileInput};
use super::resolve::{build_identifier_resolution_map, resolve_import_specifier};
use super::types::ModuleDependencySummary;
use crate::ModuleQueryError;

/// Input for composing dependency summaries across modules.
#[derive(Debug, Clone)]
pub struct ComposeDependenciesInput<'a> {
    /// Snapshot UID to analyze.
    pub snapshot_uid: &'a str,
    /// Runtime builtin module names for the ecosystem.
    /// For npm: ["fs", "path", "http", "https", "url", "util", ...]
    /// For Cargo: ["std", "core", "alloc"]
    pub runtime_builtins: HashSet<String>,
    /// Ecosystem identifier: "npm" or "cargo".
    pub ecosystem: String,
}

/// Result of composing dependency summaries.
#[derive(Debug, Clone)]
pub struct ComposeDependenciesResult {
    /// Per-module dependency summaries.
    pub summaries: Vec<ModuleDependencySummary>,
    /// Total external imports observed across all modules.
    pub total_external_imports: usize,
    /// Modules that had no manifest context available.
    pub modules_without_manifest_context: Vec<String>,
}

/// Compose dependency summaries for all modules in a snapshot.
///
/// Orchestration steps:
/// 1. Load module_candidates for UID → canonical_root_path mapping
/// 2. Load file ownership (file → module mapping)
/// 3. Load external imports from unresolved_edges
/// 4. Load package dependencies from file_signals
/// 5. Group by module (using canonical_root_path as identity)
/// 6. Reconcile each module's declared vs observed dependencies
///
/// # Arguments
///
/// * `storage` - Storage connection for data access.
/// * `input` - Composition parameters.
///
/// # Returns
///
/// `ComposeDependenciesResult` with per-module summaries and diagnostics.
pub fn compose_dependency_summaries(
    storage: &StorageConnection,
    input: &ComposeDependenciesInput,
) -> Result<ComposeDependenciesResult, ModuleQueryError> {
    // 1. Load module_candidates for UID → canonical_root_path + module_kind mapping.
    let modules = storage.get_module_candidates_for_snapshot(input.snapshot_uid)?;
    let uid_to_module: HashMap<&str, &ModuleCandidate> = modules
        .iter()
        .map(|m| (m.module_candidate_uid.as_str(), m))
        .collect();

    // 2. Load file ownership to map files to module UIDs.
    let ownership = storage.get_file_ownership_for_snapshot(input.snapshot_uid)?;
    let file_to_module_uid: HashMap<&str, &str> = ownership
        .iter()
        .map(|o| (o.file_uid.as_str(), o.module_candidate_uid.as_str()))
        .collect();

    // 3. Load external imports.
    let external_imports = storage.get_external_imports_for_snapshot(input.snapshot_uid)?;
    let total_external_imports = external_imports.len();

    // 3.5. Load import bindings for identifier → specifier resolution.
    // This resolves callee identifiers (e.g., "useState") to import specifiers (e.g., "react").
    let import_bindings = storage.get_external_import_bindings_for_snapshot(input.snapshot_uid)?;
    let identifier_to_specifier = build_identifier_resolution_map(&import_bindings);

    // 4. Load package dependencies from file_signals.
    let package_deps = storage.get_package_dependencies_for_snapshot(input.snapshot_uid)?;

    // 5. Group data by module canonical_root_path.
    // Key: canonical_root_path (user-facing identity)
    let mut module_imports: HashMap<String, Vec<String>> = HashMap::new();
    let mut module_declared: HashMap<String, HashSet<String>> = HashMap::new();
    // Tracks whether module has manifest context (deps were loaded).
    let mut module_has_manifest: HashMap<String, bool> = HashMap::new();
    // module_key contains ecosystem prefix (e.g., "npm:repo:path" or "cargo:repo:path").
    let mut module_keys: HashMap<String, String> = HashMap::new();

    // Group external imports by module canonical path.
    // Resolve identifiers to specifiers using import bindings.
    for import in &external_imports {
        if let Some(&module_uid) = file_to_module_uid.get(import.source_file_uid.as_str()) {
            if let Some(&module) = uid_to_module.get(module_uid) {
                let canonical_path = &module.canonical_root_path;

                // Resolve the specifier:
                // 1. Try to resolve via import binding (e.g., "useState" → "react")
                // 2. For member access like "React.createElement", try the receiver ("React")
                // 3. Fall back to the raw target_key if no binding found
                let resolved_specifier = resolve_import_specifier(
                    &import.specifier,
                    import.source_file_uid.as_str(),
                    &identifier_to_specifier,
                );

                module_imports
                    .entry(canonical_path.clone())
                    .or_default()
                    .push(resolved_specifier);
                // Track module_key for ecosystem filtering (contains "npm:" or "cargo:" prefix).
                module_keys.insert(canonical_path.clone(), module.module_key.clone());
            }
        }
    }

    // Group declared dependencies by module canonical path.
    for dep in &package_deps {
        if let Some(&module_uid) = file_to_module_uid.get(dep.file_uid.as_str()) {
            if let Some(&module) = uid_to_module.get(module_uid) {
                let canonical_path = &module.canonical_root_path;
                let declared = module_declared.entry(canonical_path.clone()).or_default();
                for name in &dep.package_names {
                    declared.insert(name.clone());
                }
                // Mark that this module has manifest context.
                // Note: dep.file_path is the source file; manifest path is derived below.
                module_has_manifest.insert(canonical_path.clone(), true);
                module_keys.insert(canonical_path.clone(), module.module_key.clone());
            }
        }
    }

    // Collect all modules that have either imports or declared deps.
    let all_module_paths: HashSet<&str> = module_imports
        .keys()
        .map(|s| s.as_str())
        .chain(module_declared.keys().map(|s| s.as_str()))
        .collect();

    // 6. Reconcile each module.
    let mut summaries = Vec::new();
    let mut modules_without_manifest_context = Vec::new();

    for canonical_path in all_module_paths {
        // Filter by ecosystem using module_key prefix.
        // Module keys are formatted as "npm:repo:path" or "cargo:repo:path".
        if let Some(module_key) = module_keys.get(canonical_path) {
            let ecosystem_match = match input.ecosystem.as_str() {
                "cargo" => module_key.starts_with("cargo:"),
                "npm" => module_key.starts_with("npm:"),
                _ => true, // Unknown ecosystem — include all
            };
            if !ecosystem_match {
                continue;
            }
        }

        let imports = module_imports
            .get(canonical_path)
            .cloned()
            .unwrap_or_default();
        let declared: Vec<String> = module_declared
            .get(canonical_path)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        let has_manifest = module_has_manifest
            .get(canonical_path)
            .copied()
            .unwrap_or(false);
        // Derive manifest path from canonical_root_path + ecosystem convention.
        // npm: package.json, cargo: Cargo.toml
        let manifest_path = if has_manifest {
            let manifest_file = match input.ecosystem.as_str() {
                "cargo" => "Cargo.toml",
                _ => "package.json",
            };
            if canonical_path == "." || canonical_path.is_empty() {
                Some(manifest_file.to_string())
            } else {
                Some(format!("{}/{}", canonical_path, manifest_file))
            }
        } else {
            None
        };
        let manifest_scope_available = has_manifest;

        if !manifest_scope_available {
            modules_without_manifest_context.push(canonical_path.to_string());
        }

        let reconcile_input = ReconcileInput {
            module: canonical_path.to_string(),
            manifest_path,
            declared_dependencies: declared,
            manifest_scope_available,
            observed_external_imports: imports,
            runtime_builtins: input.runtime_builtins.clone(),
            ecosystem: input.ecosystem.clone(),
        };

        let summary = reconcile_module_dependencies(reconcile_input);
        summaries.push(summary);
    }

    // Sort summaries by module canonical path for deterministic output.
    summaries.sort_by(|a, b| a.module.cmp(&b.module));

    Ok(ComposeDependenciesResult {
        summaries,
        total_external_imports,
        modules_without_manifest_context,
    })
}

/// Default npm runtime builtins.
///
/// Node.js core modules that don't require package.json declaration.
/// Includes both bare and `node:` prefixed variants.
pub fn npm_runtime_builtins() -> HashSet<String> {
    [
        // File system
        "fs",
        "path",
        "os",
        // Network
        "http",
        "https",
        "http2",
        "net",
        "dns",
        "tls",
        // Streams
        "stream",
        "zlib",
        // Process
        "process",
        "child_process",
        "cluster",
        "worker_threads",
        // Utilities
        "util",
        "url",
        "querystring",
        "string_decoder",
        // Crypto
        "crypto",
        // Events
        "events",
        // Buffer
        "buffer",
        // Timers
        "timers",
        // Console
        "console",
        // Assert
        "assert",
        // Node prefixed variants
        "node:fs",
        "node:path",
        "node:os",
        "node:http",
        "node:https",
        "node:http2",
        "node:net",
        "node:dns",
        "node:tls",
        "node:stream",
        "node:zlib",
        "node:process",
        "node:child_process",
        "node:cluster",
        "node:worker_threads",
        "node:util",
        "node:url",
        "node:querystring",
        "node:string_decoder",
        "node:crypto",
        "node:events",
        "node:buffer",
        "node:timers",
        "node:console",
        "node:assert",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Default Cargo runtime builtins.
///
/// Rust standard library crates that don't require Cargo.toml declaration.
pub fn cargo_runtime_builtins() -> HashSet<String> {
    ["std", "core", "alloc"]
        .into_iter()
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_builtins_contains_common_modules() {
        let builtins = npm_runtime_builtins();
        assert!(builtins.contains("fs"));
        assert!(builtins.contains("path"));
        assert!(builtins.contains("node:fs"));
        assert!(builtins.contains("http"));
    }

    #[test]
    fn cargo_builtins_contains_std() {
        let builtins = cargo_runtime_builtins();
        assert!(builtins.contains("std"));
        assert!(builtins.contains("core"));
        assert!(builtins.contains("alloc"));
    }
}
