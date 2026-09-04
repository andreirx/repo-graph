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
    /// DEPS-SELF-1 (FINAL-POLISH-1 §2.2): the package names THIS repo's parsed manifests declare as
    /// their OWN (`module_candidates.module_kind='declared'`, `display_name` — the same fact
    /// TRUST-FIRSTPARTY-1 uses, NEVER a directory name). An observed specifier equal to one of these
    /// (under ecosystem-aware normalization) is a first-party self-reference, not an undeclared
    /// external. Empty (no parsed manifests) → nothing reclassified (byte-identical pre-slice output).
    pub own_manifest_names: HashSet<String>,
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

    // DEPS-SELF-1: the repo's own manifest names, normalized once per the ecosystem's package-name
    // semantics, for the self-reference check below. Normalization is a real domain rule (Python
    // PEP 503 case/`-_.` folding; Cargo `_`↔`-`) — the same class of equivalence
    // TRUST-FIRSTPARTY-1 applies to Cargo — NEVER a fuzzy prefix match.
    let own_names_normalized: HashSet<String> = input
        .own_manifest_names
        .iter()
        .map(|n| normalize_self_name(n, &input.ecosystem))
        .collect();
    let is_self = |package: &str| -> bool {
        own_names_normalized.contains(&normalize_self_name(package, &input.ecosystem))
    };

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
        } else if is_self(package) {
            // DEPS-SELF-1 (§2.2): a self-import — the specifier is THIS repo's own manifest name.
            // Checked AFTER `declared_set` so a genuinely-declared dependency of the same name keeps
            // its `DeclaredAndUsed` classification; only the otherwise-undeclared self-import lands
            // here (django importing `django`), so it never renders as a third-party external.
            entries.push(DependencyEntry {
                package: package.clone(),
                category: DependencyCategory::FirstPartySelf,
                import_count: observed.import_count,
                dependency_class: None,
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
        // HONESTY-GATE-1 §2.2: reconcile does not see provenance; `compose` attributes the
        // contributing manifests and overwrites this after the summary is built (empty = single
        // cited manifest, byte-parity for the leaf case reconcile alone produces).
        declared_manifest_paths: Vec::new(),
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
        DependencyCategory::FirstPartySelf => 3,
        DependencyCategory::RuntimeBuiltin => 4,
        DependencyCategory::UnknownExternalLike => 5,
    }
}

/// DEPS-SELF-1 (§2.2): normalize a package name for the self-reference equality check, per the
/// ecosystem's package-name semantics. This is a domain equivalence, not a heuristic:
/// - **Python** — PEP 503: names are case-insensitive and runs of `-`, `_`, `.` are equivalent, so
///   the distribution name `Django` and the import package `django` are the SAME package.
/// - **Cargo** — `_` and `-` are equivalent (the same rule TRUST-FIRSTPARTY-1 applies).
/// - **npm / java / other** — names are literal; compared exactly (normalizing them would risk
///   matching two genuinely-distinct packages).
fn normalize_self_name(name: &str, ecosystem: &str) -> String {
    match ecosystem {
        "python" => {
            // PEP 503: lowercase, then collapse any run of `-`, `_`, `.` into a single `-`.
            let lowered = name.to_ascii_lowercase();
            let mut out = String::with_capacity(lowered.len());
            let mut prev_sep = false;
            for ch in lowered.chars() {
                if matches!(ch, '-' | '_' | '.') {
                    if !prev_sep {
                        out.push('-');
                        prev_sep = true;
                    }
                } else {
                    out.push(ch);
                    prev_sep = false;
                }
            }
            out
        }
        "cargo" => name.replace('_', "-"),
        _ => name.to_string(),
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
            own_manifest_names: HashSet::new(),
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
            own_manifest_names: HashSet::new(),
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
            own_manifest_names: HashSet::new(),
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
            own_manifest_names: HashSet::new(),
        };

        let summary = reconcile_module_dependencies(input);
        assert_eq!(summary.observed_but_undeclared_count(), 1);
        assert_eq!(summary.unknown_external_like_count(), 0);
        let leftpad = &summary.entries[0];
        assert_eq!(leftpad.package, "leftpad");
        assert_eq!(leftpad.category, DependencyCategory::ObservedButUndeclared);
    }

    #[test]
    fn self_import_is_first_party_not_undeclared() {
        // DEPS-SELF-1 (§2.2): django's shape — the `django` package imports `django.*`. The
        // manifest declares its OWN name `Django` (PyPI distribution spelling); the import
        // specifier is `django`. PEP 503 normalization makes them equal, so it classifies as a
        // first-party self-reference, NEVER `observed_but_undeclared`.
        let mut own = HashSet::new();
        own.insert("Django".to_string()); // the manifest's own declared name (capitalized)
        let input = ReconcileInput {
            module: ".".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "pyproject.toml".to_string(),
            },
            declared_dependencies: vec!["asgiref".to_string()],
            manifest_scope_available: true,
            observed_external_imports: vec!["django".to_string(), "asgiref".to_string()],
            runtime_builtins: HashSet::new(),
            ecosystem: "python".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: own,
        };
        let summary = reconcile_module_dependencies(input);
        // `django` is self, not undeclared.
        assert_eq!(summary.first_party_self_count(), 1);
        assert_eq!(summary.observed_but_undeclared_count(), 0);
        let django = summary
            .entries
            .iter()
            .find(|e| e.package == "django")
            .unwrap();
        assert_eq!(django.category, DependencyCategory::FirstPartySelf);
        // A declared dependency is unaffected.
        assert_eq!(summary.declared_and_used_count(), 1);
    }

    #[test]
    fn declared_dependency_named_like_self_stays_declared() {
        // Ordering rule: `declared_set` wins over the self-check. A workspace sibling that IS a
        // declared dependency keeps `DeclaredAndUsed` even though its name is also a repo-own
        // manifest name — only the otherwise-undeclared self-import is reclassified.
        let mut own = HashSet::new();
        own.insert("widgets".to_string());
        let input = ReconcileInput {
            module: "app".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "app/Cargo.toml".to_string(),
            },
            declared_dependencies: vec!["widgets".to_string()],
            manifest_scope_available: true,
            observed_external_imports: vec!["widgets::thing".to_string()],
            runtime_builtins: HashSet::new(),
            ecosystem: "cargo".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: own,
        };
        let summary = reconcile_module_dependencies(input);
        assert_eq!(summary.declared_and_used_count(), 1);
        assert_eq!(summary.first_party_self_count(), 0);
    }

    #[test]
    fn directory_named_like_package_is_not_self() {
        // DEPS-SELF-1 (§45) NEGATIVE witness + byte-parity: self classification keys on the stored
        // manifest-name FACT (`own_manifest_names`), NEVER on the module/directory name. Here the
        // module PATH is literally "django" and the imported specifier is `django`, but the
        // own-name set is EMPTY (no parsed manifest name) — so `django` must stay
        // `observed_but_undeclared`, proving a directory coincidentally named like a package does
        // NOT trigger self classification. Empty own-name set also preserves the pre-slice
        // observed_but_undeclared behaviour exactly (byte-parity).
        let input = ReconcileInput {
            module: "django".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "pyproject.toml".to_string(),
            },
            declared_dependencies: vec![],
            manifest_scope_available: true,
            observed_external_imports: vec!["django".to_string()],
            runtime_builtins: HashSet::new(),
            ecosystem: "python".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
        };
        let summary = reconcile_module_dependencies(input);
        assert_eq!(summary.first_party_self_count(), 0);
        assert_eq!(summary.observed_but_undeclared_count(), 1);
        let django = summary
            .entries
            .iter()
            .find(|e| e.package == "django")
            .unwrap();
        assert_eq!(django.category, DependencyCategory::ObservedButUndeclared);
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
            own_manifest_names: HashSet::new(),
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
            own_manifest_names: HashSet::new(),
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
            own_manifest_names: HashSet::new(),
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
            own_manifest_names: HashSet::new(),
        };

        let summary = reconcile_module_dependencies(input);

        // Only react should be counted
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].package, "react");
    }
}
