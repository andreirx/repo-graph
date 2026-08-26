//! Dependency reconciliation engine.
//!
//! Joins declared dependencies (from manifests) with observed
//! external references (from imports) to produce a module-level
//! dependency summary.

use std::collections::{HashMap, HashSet};

use super::classify::{classify_observed, ObservedKind};
use super::types::{DependencyCategory, DependencyEntry, ManifestContext, ModuleDependencySummary};

/// Input data for dependency reconciliation.
#[derive(Debug, Clone)]
pub struct ReconcileInput {
    /// Module identifier (canonical_root_path).
    pub module: String,
    /// Provenance of the manifest this module's deps were parsed from (§2.2). Set by `compose`
    /// from the persisted parsed-manifest records; never a fabricated fixed-name path.
    pub manifest_context: ManifestContext,
    /// Declared dependency package names from manifest.
    /// Empty if manifest context is unavailable.
    pub declared_dependencies: Vec<String>,
    /// Whether manifest dependency context is available.
    /// `false` for Python/Java until compose.rs attaches their contexts.
    pub manifest_scope_available: bool,
    /// Observed import specifiers classified as external.
    /// Each entry is a raw specifier (e.g., "react/jsx-runtime", "lodash/get").
    pub observed_external_imports: Vec<String>,
    /// Runtime builtin module specifiers (e.g., "fs", "path", "node:fs").
    pub runtime_builtins: HashSet<String>,
    /// Ecosystem for normalization rules: "npm" or "cargo".
    pub ecosystem: String,
    /// Observed references the assembly (`compose`) already dropped as non-import call targets
    /// (bare unbound identifiers, §2.1) — folded into `rejected_non_specifier` for honest totals.
    pub pre_rejected_non_specifier: usize,
}

/// Reconcile declared and observed dependencies for a module.
///
/// Produces a `ModuleDependencySummary` with entries categorized as:
/// - `DeclaredAndUsed` — in manifest AND observed in imports
/// - `DeclaredButUnobserved` — in manifest but no imports found
/// - `ObservedButUndeclared` — imported but not in manifest
/// - `RuntimeBuiltin` — runtime/stdlib module (fs, path, std::*)
/// - `UnknownExternalLike` — couldn't be confidently classified
pub fn reconcile_module_dependencies(input: ReconcileInput) -> ModuleDependencySummary {
    let mut entries: Vec<DependencyEntry> = Vec::new();

    // Build set of declared packages for O(1) lookup.
    let declared_set: HashSet<&str> = input
        .declared_dependencies
        .iter()
        .map(|s| s.as_str())
        .collect();

    // Classify observed references through the specifier-only gate (DEPS-LIST-REWRITE-1
    // §2.1). Only import-specifier-shaped values reach the package namespace; language
    // builtins classify as builtins; call-expression text is dropped and counted.
    let mut observed_packages: HashMap<String, ObservedPackage> = HashMap::new();
    let mut observed_builtins: HashMap<String, ObservedPackage> = HashMap::new();
    let mut rejected_non_specifier: usize = input.pre_rejected_non_specifier;

    for raw in &input.observed_external_imports {
        match classify_observed(raw, &input.ecosystem, &input.runtime_builtins) {
            ObservedKind::Local => {}
            ObservedKind::NonSpecifier => rejected_non_specifier += 1,
            ObservedKind::Builtin { name } => {
                let entry = observed_builtins
                    .entry(name)
                    .or_insert_with(|| ObservedPackage {
                        import_count: 0,
                        raw_specifiers: Vec::new(),
                    });
                entry.import_count += 1;
                if !entry.raw_specifiers.contains(raw) {
                    entry.raw_specifiers.push(raw.clone());
                }
            }
            ObservedKind::Package { package } => {
                let entry = observed_packages
                    .entry(package)
                    .or_insert_with(|| ObservedPackage {
                        import_count: 0,
                        raw_specifiers: Vec::new(),
                    });
                entry.import_count += 1;
                if !entry.raw_specifiers.contains(raw) {
                    entry.raw_specifiers.push(raw.clone());
                }
            }
        }
    }

    // Emit builtin usages (already proven builtins by the gate — never packages).
    for (name, observed) in &observed_builtins {
        entries.push(DependencyEntry {
            package: name.clone(),
            category: DependencyCategory::RuntimeBuiltin,
            import_count: observed.import_count,
            dependency_class: None,
            confidence: 1.0,
            raw_specifiers: observed.raw_specifiers.clone(),
        });
    }

    // Categorize each observed package (specifier-shaped, non-builtin).
    for (package, observed) in &observed_packages {
        if declared_set.contains(package.as_str()) {
            entries.push(DependencyEntry {
                package: package.clone(),
                category: DependencyCategory::DeclaredAndUsed,
                import_count: observed.import_count,
                dependency_class: None, // TODO: extract from manifest
                confidence: 1.0,
                raw_specifiers: observed.raw_specifiers.clone(),
            });
        } else if input.manifest_scope_available {
            // Manifest is available but package not declared.
            entries.push(DependencyEntry {
                package: package.clone(),
                category: DependencyCategory::ObservedButUndeclared,
                import_count: observed.import_count,
                dependency_class: None,
                confidence: 0.8, // Slightly lower confidence for undeclared
                raw_specifiers: observed.raw_specifiers.clone(),
            });
        } else {
            // Manifest not available, can't determine if declared.
            entries.push(DependencyEntry {
                package: package.clone(),
                category: DependencyCategory::UnknownExternalLike,
                import_count: observed.import_count,
                dependency_class: None,
                confidence: 0.5,
                raw_specifiers: observed.raw_specifiers.clone(),
            });
        }
    }

    // Add declared but unobserved packages (if manifest available).
    if input.manifest_scope_available {
        let observed_set: HashSet<&str> = observed_packages.keys().map(|s| s.as_str()).collect();

        for declared in &input.declared_dependencies {
            if !observed_set.contains(declared.as_str()) {
                entries.push(DependencyEntry {
                    package: declared.clone(),
                    category: DependencyCategory::DeclaredButUnobserved,
                    import_count: 0,
                    dependency_class: None, // TODO: extract from manifest
                    confidence: 1.0,
                    raw_specifiers: Vec::new(),
                });
            }
        }
    }

    // Sort entries by category, then by package name for determinism.
    entries.sort_by(|a, b| {
        let cat_ord = category_order(a.category).cmp(&category_order(b.category));
        if cat_ord != std::cmp::Ordering::Equal {
            cat_ord
        } else {
            a.package.cmp(&b.package)
        }
    });

    ModuleDependencySummary {
        module: input.module,
        manifest_context: input.manifest_context,
        manifest_scope_available: input.manifest_scope_available,
        entries,
        rejected_non_specifier,
    }
}

/// Intermediate struct for counting observed imports.
struct ObservedPackage {
    import_count: usize,
    raw_specifiers: Vec<String>,
}

/// Ordering for dependency categories in output.
fn category_order(cat: DependencyCategory) -> u8 {
    match cat {
        DependencyCategory::DeclaredAndUsed => 0,
        DependencyCategory::DeclaredButUnobserved => 1,
        DependencyCategory::ObservedButUndeclared => 2,
        DependencyCategory::RuntimeBuiltin => 3,
        DependencyCategory::UnknownExternalLike => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npm_builtins() -> HashSet<String> {
        [
            "fs",
            "path",
            "node:fs",
            "node:path",
            "http",
            "https",
            "url",
            "util",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    #[test]
    fn declared_and_used_match() {
        let input = ReconcileInput {
            module: "frontend".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "manifest".to_string(),
            },
            declared_dependencies: vec!["react".to_string(), "lodash".to_string()],
            manifest_scope_available: true,
            observed_external_imports: vec![
                "react".to_string(),
                "react/jsx-runtime".to_string(),
                "lodash/get".to_string(),
            ],
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
        };

        let summary = reconcile_module_dependencies(input);

        assert!(summary.manifest_scope_available);
        assert_eq!(summary.declared_and_used_count(), 2);

        let react = summary
            .entries
            .iter()
            .find(|e| e.package == "react")
            .unwrap();
        assert_eq!(react.category, DependencyCategory::DeclaredAndUsed);
        assert_eq!(react.import_count, 2); // react + react/jsx-runtime

        let lodash = summary
            .entries
            .iter()
            .find(|e| e.package == "lodash")
            .unwrap();
        assert_eq!(lodash.category, DependencyCategory::DeclaredAndUsed);
        assert_eq!(lodash.import_count, 1);
    }

    #[test]
    fn declared_but_unobserved() {
        let input = ReconcileInput {
            module: "frontend".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "manifest".to_string(),
            },
            declared_dependencies: vec!["react".to_string(), "moment".to_string()],
            manifest_scope_available: true,
            observed_external_imports: vec!["react".to_string()],
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
        };

        let summary = reconcile_module_dependencies(input);

        assert_eq!(summary.declared_but_unobserved_count(), 1);

        let moment = summary
            .entries
            .iter()
            .find(|e| e.package == "moment")
            .unwrap();
        assert_eq!(moment.category, DependencyCategory::DeclaredButUnobserved);
        assert_eq!(moment.import_count, 0);
    }

    #[test]
    fn observed_but_undeclared() {
        let input = ReconcileInput {
            module: "frontend".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "manifest".to_string(),
            },
            declared_dependencies: vec!["react".to_string()],
            manifest_scope_available: true,
            observed_external_imports: vec![
                "react".to_string(),
                "debug".to_string(), // Not declared
            ],
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
        };

        let summary = reconcile_module_dependencies(input);

        assert_eq!(summary.observed_but_undeclared_count(), 1);

        let debug = summary
            .entries
            .iter()
            .find(|e| e.package == "debug")
            .unwrap();
        assert_eq!(debug.category, DependencyCategory::ObservedButUndeclared);
        assert_eq!(debug.import_count, 1);
    }

    #[test]
    fn zero_declared_but_scoped_imports_are_observed_but_undeclared() {
        // Ruling-3 item 3 end state: a PARSED zero-dependency manifest sets scope=true with an
        // EMPTY declared set. Its observed imports must reconcile as `observed_but_undeclared`
        // (declared-context known, nothing declared) — NOT `unknown_external_like` (which is the
        // no-manifest-scope degrade). This is the behavioural pair to `scope_available`'s truth.
        let input = ReconcileInput {
            module: "pkg".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "pkg/package.json".to_string(),
            },
            declared_dependencies: vec![], // parsed manifest, zero declared deps
            manifest_scope_available: true,
            observed_external_imports: vec!["leftpad".to_string()],
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
        };

        let summary = reconcile_module_dependencies(input);
        assert_eq!(summary.observed_but_undeclared_count(), 1);
        assert_eq!(summary.unknown_external_like_count(), 0);
        let leftpad = &summary.entries[0];
        assert_eq!(leftpad.package, "leftpad");
        assert_eq!(leftpad.category, DependencyCategory::ObservedButUndeclared);
    }

    #[test]
    fn runtime_builtins_detected() {
        let input = ReconcileInput {
            module: "backend".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "manifest".to_string(),
            },
            declared_dependencies: vec![],
            manifest_scope_available: true,
            observed_external_imports: vec!["fs".to_string(), "node:path".to_string()],
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
        };

        let summary = reconcile_module_dependencies(input);

        assert_eq!(summary.runtime_builtins_count(), 2);

        let fs = summary.entries.iter().find(|e| e.package == "fs").unwrap();
        assert_eq!(fs.category, DependencyCategory::RuntimeBuiltin);

        let path = summary
            .entries
            .iter()
            .find(|e| e.package == "node:path")
            .unwrap();
        assert_eq!(path.category, DependencyCategory::RuntimeBuiltin);
    }

    #[test]
    fn manifest_unavailable_degrades_to_unknown() {
        let input = ReconcileInput {
            module: "python-module".to_string(),
            manifest_context: ManifestContext::Absent,
            declared_dependencies: vec![],
            manifest_scope_available: false, // Python, no manifest context
            observed_external_imports: vec!["requests".to_string()],
            runtime_builtins: HashSet::new(),
            ecosystem: "npm".to_string(), // doesn't matter
            pre_rejected_non_specifier: 0,
        };

        let summary = reconcile_module_dependencies(input);

        assert!(!summary.manifest_scope_available);
        assert_eq!(summary.entries.len(), 1);

        let requests = &summary.entries[0];
        assert_eq!(requests.category, DependencyCategory::UnknownExternalLike);
        assert_eq!(requests.confidence, 0.5);
    }

    #[test]
    fn cargo_normalization_works() {
        let input = ReconcileInput {
            module: "rust-crate".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "manifest".to_string(),
            },
            declared_dependencies: vec!["tokio".to_string(), "serde".to_string()],
            manifest_scope_available: true,
            observed_external_imports: vec![
                "tokio::spawn".to_string(),
                "tokio::sync::Mutex".to_string(),
                "serde::Deserialize".to_string(),
                "std::collections::HashMap".to_string(),
            ],
            runtime_builtins: HashSet::new(),
            ecosystem: "cargo".to_string(),
            pre_rejected_non_specifier: 0,
        };

        let summary = reconcile_module_dependencies(input);

        // tokio and serde are declared and used
        assert_eq!(summary.declared_and_used_count(), 2);

        let tokio = summary
            .entries
            .iter()
            .find(|e| e.package == "tokio")
            .unwrap();
        assert_eq!(tokio.category, DependencyCategory::DeclaredAndUsed);
        assert_eq!(tokio.import_count, 2); // spawn + sync::Mutex

        // std is a runtime builtin
        let std = summary.entries.iter().find(|e| e.package == "std").unwrap();
        assert_eq!(std.category, DependencyCategory::RuntimeBuiltin);
    }

    #[test]
    fn local_imports_ignored() {
        let input = ReconcileInput {
            module: "frontend".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "manifest".to_string(),
            },
            declared_dependencies: vec!["react".to_string()],
            manifest_scope_available: true,
            observed_external_imports: vec![
                "react".to_string(),
                "./utils".to_string(),        // local
                "../shared".to_string(),      // local
                "/absolute/path".to_string(), // local
            ],
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
        };

        let summary = reconcile_module_dependencies(input);

        // Only react should be counted
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].package, "react");
    }
}
