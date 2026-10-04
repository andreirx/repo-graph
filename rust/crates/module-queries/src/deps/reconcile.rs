//! Dependency reconciliation engine.
//!
//! Joins declared dependencies (from manifests) with observed
//! external references (from imports) to produce a module-level
//! dependency summary.

use std::collections::{HashMap, HashSet};

use super::classify::{classify_observed, ObservedKind};
use super::types::{
    DependencyCategory, DependencyEntry, ManifestContext, ModuleDependencySummary,
    ObservedImportRef,
};

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
    /// Observed external references classified as external — each a raw specifier plus whether it
    /// is an IMPORTS-edge site or a CALL-edge site (DEPS-CLASSIFIER-1 §2.3).
    pub observed_external_imports: Vec<ObservedImportRef>,
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
    /// DEPS-CLASSIFIER-1B §2.2 item 3: normalized package heads this module imports ONLY type-only
    /// (`import type … from "pkg"`). Computed by `compose` from `is_type_only` bindings. A DECLARED
    /// package in this set, with no value import/call observed, reconciles as `TypeOnlyImport`
    /// instead of `DeclaredButUnobserved`. Empty for non-npm ecosystems (only the TS extractor sets
    /// `is_type_only`) and for snapshots with no type-only imports (byte-parity).
    pub type_only_package_heads: HashSet<String>,
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
    // DEPS-GRADLE-CATALOG-1B (RG-REQ-006-L13): does a declared name match observed `package`? For
    // the `java` ecosystem a declared group matches a package on a `.`-segment boundary (equal, or
    // the package starts with the group followed by `.` — the predicate the index-time classifier
    // already applies); every other ecosystem matches exactly.
    let segment_rule = input.ecosystem == "java";
    let declared_matches = |declared: &str, package: &str| -> bool {
        declared == package
            || (segment_rule
                && package.len() > declared.len()
                && package.starts_with(declared)
                && package.as_bytes()[declared.len()] == b'.')
    };

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

    for observed in &input.observed_external_imports {
        let raw = &observed.specifier;
        match classify_observed(raw, &input.ecosystem, &input.runtime_builtins) {
            ObservedKind::Local => {}
            ObservedKind::NonSpecifier => rejected_non_specifier += 1,
            ObservedKind::Builtin { name } => {
                let entry = observed_builtins.entry(name).or_default();
                entry.record(raw, observed.is_import_edge);
            }
            ObservedKind::Package { package } => {
                let entry = observed_packages.entry(package).or_default();
                entry.record(raw, observed.is_import_edge);
            }
        }
    }

    // Emit builtin usages (already proven builtins by the gate — never packages).
    for (name, observed) in &observed_builtins {
        entries.push(DependencyEntry {
            package: name.clone(),
            category: DependencyCategory::RuntimeBuiltin,
            import_count: observed.import_count,
            import_sites: observed.import_sites,
            dependency_class: None,
            confidence: 1.0,
            raw_specifiers: observed.raw_specifiers.clone(),
        });
    }

    // Categorize each observed package (specifier-shaped, non-builtin).
    for (package, observed) in &observed_packages {
        let declared = declared_set.contains(package.as_str())
            || (segment_rule
                && input
                    .declared_dependencies
                    .iter()
                    .any(|d| declared_matches(d, package)));
        if declared {
            entries.push(DependencyEntry {
                package: package.clone(),
                category: DependencyCategory::DeclaredAndUsed,
                import_count: observed.import_count,
                import_sites: observed.import_sites,
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
                import_sites: observed.import_sites,
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
                import_sites: observed.import_sites,
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
                import_sites: observed.import_sites,
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
            // A declared name is unobserved only when it matches no observed package (Java: on a
            // `.`-segment boundary; every other ecosystem: exactly).
            let observed = observed_set.contains(declared.as_str())
                || (segment_rule && observed_set.iter().any(|p| declared_matches(declared, p)));
            if !observed {
                // §2.2 item 3: a declared package with no value use, but imported type-only, is
                // `TypeOnlyImport` — not `DeclaredButUnobserved`. `type_only_package_heads` holds
                // already-normalized heads, so the declared name is compared directly.
                let category = if input.type_only_package_heads.contains(declared.as_str()) {
                    DependencyCategory::TypeOnlyImport
                } else {
                    DependencyCategory::DeclaredButUnobserved
                };
                entries.push(DependencyEntry {
                    package: declared.clone(),
                    category,
                    import_count: 0,
                    import_sites: 0,
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
#[derive(Default)]
struct ObservedPackage {
    import_count: usize,
    /// DEPS-CLASSIFIER-1 §2.3: the IMPORTS-edge subset of `import_count` (the rest are call sites).
    import_sites: usize,
    raw_specifiers: Vec<String>,
}

impl ObservedPackage {
    /// Record one observed reference: bump the total, the import-site subcount when it is an
    /// IMPORTS edge, and remember the distinct raw specifier.
    fn record(&mut self, raw: &str, is_import_edge: bool) {
        self.import_count += 1;
        if is_import_edge {
            self.import_sites += 1;
        }
        if !self.raw_specifiers.iter().any(|s| s == raw) {
            self.raw_specifiers.push(raw.to_string());
        }
    }
}

/// Ordering for dependency categories in output.
fn category_order(cat: DependencyCategory) -> u8 {
    match cat {
        DependencyCategory::DeclaredAndUsed => 0,
        DependencyCategory::TypeOnlyImport => 1,
        DependencyCategory::DeclaredButUnobserved => 2,
        DependencyCategory::ObservedButUndeclared => 3,
        DependencyCategory::FirstPartySelf => 4,
        DependencyCategory::RuntimeBuiltin => 5,
        DependencyCategory::UnknownExternalLike => 6,
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
        // DEPS-CLASSIFIER-1 §2.1: the single shared PEP 503 folding (lowercase + collapse `-_.`
        // runs), the same function the index-time classifier uses for the Python head match — one
        // source of truth, mirroring the cargo arm's delegation below.
        "python" => repo_graph_classification::pep503_normalize(name),
        // IMPORT-RESOLUTION-RUST-1 §2.3: the single shared `_`→`-` canonicalisation.
        "cargo" => repo_graph_classification::canonicalize_cargo_package_name(name),
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build observed refs from bare specifiers, all marked as IMPORTS-edge sites. The import/call
    /// split is exercised by `import_sites_and_call_sites_split`; the category tests below only care
    /// about totals, so `import_count` is unaffected by the flag.
    fn obs(specs: &[&str]) -> Vec<ObservedImportRef> {
        specs
            .iter()
            .map(|s| ObservedImportRef {
                specifier: s.to_string(),
                is_import_edge: true,
            })
            .collect()
    }

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
            observed_external_imports: obs(&["react", "react/jsx-runtime", "lodash/get"]),
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
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
    fn import_sites_and_call_sites_split() {
        // DEPS-CLASSIFIER-1 §2.3: asgiref's shape — declared `["asgiref"]`, imported at IMPORTS-edge
        // sites (`from asgiref.sync import …`) AND used at CALL-edge sites (`sync_to_async(...)`).
        // The DeclaredAndUsed entry must split them: import_sites counts only IMPORTS edges,
        // call_sites = import_count - import_sites. This is the `used (N import sites, M call sites)`
        // basis the row renders instead of the false `no static import: asgiref`.
        let input = ReconcileInput {
            module: ".".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "pyproject.toml".to_string(),
            },
            declared_dependencies: vec!["asgiref".to_string()],
            manifest_scope_available: true,
            observed_external_imports: vec![
                // two IMPORTS-edge sites (asgiref.sync, asgiref.local)
                ObservedImportRef {
                    specifier: "asgiref.sync".to_string(),
                    is_import_edge: true,
                },
                ObservedImportRef {
                    specifier: "asgiref.local".to_string(),
                    is_import_edge: true,
                },
                // three CALL-edge sites (sync_to_async resolved to asgiref.sync by compose)
                ObservedImportRef {
                    specifier: "asgiref.sync".to_string(),
                    is_import_edge: false,
                },
                ObservedImportRef {
                    specifier: "asgiref.sync".to_string(),
                    is_import_edge: false,
                },
                ObservedImportRef {
                    specifier: "asgiref.local".to_string(),
                    is_import_edge: false,
                },
            ],
            runtime_builtins: HashSet::new(),
            ecosystem: "python".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
        };
        let summary = reconcile_module_dependencies(input);
        let asgiref = summary
            .entries
            .iter()
            .find(|e| e.package == "asgiref")
            .unwrap();
        assert_eq!(asgiref.category, DependencyCategory::DeclaredAndUsed);
        assert_eq!(asgiref.import_count, 5, "total = imports + calls");
        assert_eq!(asgiref.import_sites, 2, "only the IMPORTS-edge sites");
        assert_eq!(
            asgiref.import_count - asgiref.import_sites,
            3,
            "call sites = total - import sites"
        );
    }

    #[test]
    fn declared_type_only_import_is_type_only_not_unobserved() {
        // DEPS-CLASSIFIER-1B §2.2 item 3: `@storybook/react` is declared and imported ONLY
        // type-only (no value import/call observed). `type_only_package_heads` carries its
        // normalized head, so it reconciles as TypeOnlyImport, NOT DeclaredButUnobserved. `moment`
        // is declared with no import of any kind → stays DeclaredButUnobserved.
        let mut type_only = HashSet::new();
        type_only.insert("@storybook/react".to_string());
        let input = ReconcileInput {
            module: ".".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "package.json".to_string(),
            },
            declared_dependencies: vec!["@storybook/react".to_string(), "moment".to_string()],
            manifest_scope_available: true,
            observed_external_imports: obs(&[]), // no value use
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: type_only,
        };
        let summary = reconcile_module_dependencies(input);
        assert_eq!(summary.type_only_import_count(), 1);
        assert_eq!(summary.declared_but_unobserved_count(), 1);
        let sb = summary
            .entries
            .iter()
            .find(|e| e.package == "@storybook/react")
            .unwrap();
        assert_eq!(sb.category, DependencyCategory::TypeOnlyImport);
        let moment = summary
            .entries
            .iter()
            .find(|e| e.package == "moment")
            .unwrap();
        assert_eq!(moment.category, DependencyCategory::DeclaredButUnobserved);
    }

    #[test]
    fn value_use_beats_type_only() {
        // A package both value-imported AND in the type-only set is DeclaredAndUsed (value use wins):
        // it takes the observed branch and never consults the type-only set.
        let mut type_only = HashSet::new();
        type_only.insert("react".to_string());
        let input = ReconcileInput {
            module: ".".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "package.json".to_string(),
            },
            declared_dependencies: vec!["react".to_string()],
            manifest_scope_available: true,
            observed_external_imports: obs(&["react"]),
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: type_only,
        };
        let summary = reconcile_module_dependencies(input);
        assert_eq!(summary.declared_and_used_count(), 1);
        assert_eq!(summary.type_only_import_count(), 0);
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
            observed_external_imports: obs(&["react"]),
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&["react", "debug"]), // debug not declared
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&["leftpad"]),
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&["django", "asgiref"]),
            runtime_builtins: HashSet::new(),
            ecosystem: "python".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: own,
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&["widgets::thing"]),
            runtime_builtins: HashSet::new(),
            ecosystem: "cargo".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: own,
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&["django"]),
            runtime_builtins: HashSet::new(),
            ecosystem: "python".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&["fs", "node:path"]),
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&["requests"]),
            runtime_builtins: HashSet::new(),
            ecosystem: "npm".to_string(), // doesn't matter
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&[
                "tokio::spawn",
                "tokio::sync::Mutex",
                "serde::Deserialize",
                "std::collections::HashMap",
            ]),
            runtime_builtins: HashSet::new(),
            ecosystem: "cargo".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
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
            observed_external_imports: obs(&["react", "./utils", "../shared", "/absolute/path"]),
            runtime_builtins: npm_builtins(),
            ecosystem: "npm".to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
        };

        let summary = reconcile_module_dependencies(input);

        // Only react should be counted
        assert_eq!(summary.entries.len(), 1);
        assert_eq!(summary.entries[0].package, "react");
    }

    // ── DEPS-GRADLE-CATALOG-1B: Java declared group ↔ observed package on a `.`-segment boundary ──

    fn reconcile_eco(
        ecosystem: &str,
        declared: &[&str],
        observed: &[&str],
    ) -> ModuleDependencySummary {
        reconcile_module_dependencies(ReconcileInput {
            module: "m".to_string(),
            manifest_context: ManifestContext::Parsed {
                path: "build.gradle".to_string(),
            },
            declared_dependencies: declared.iter().map(|s| s.to_string()).collect(),
            manifest_scope_available: true,
            observed_external_imports: obs(observed),
            runtime_builtins: HashSet::new(),
            ecosystem: ecosystem.to_string(),
            pre_rejected_non_specifier: 0,
            own_manifest_names: HashSet::new(),
            type_only_package_heads: HashSet::new(),
        })
    }

    fn category_of(summary: &ModuleDependencySummary, package: &str) -> Option<DependencyCategory> {
        summary
            .entries
            .iter()
            .find(|e| e.package == package)
            .map(|e| e.category)
    }

    /// Both query-time sites apply the segment rule for Java: the observed package
    /// `com.github.luben.zstd` that the declared group `com.github.luben` matches on a `.` boundary
    /// is `used`, and the group is NOT `no static import found` (it matches an observed package).
    #[test]
    fn java_declared_group_matches_an_observed_package_on_a_segment_boundary() {
        let s = reconcile_eco(
            "java",
            &["com.github.luben", "org.slf4j"],
            &[
                "com.github.luben.zstd.ZstdInputStreamNoFinalizer",
                "org.slf4j.helpers.Util",
            ],
        );
        assert_eq!(
            category_of(&s, "com.github.luben.zstd"),
            Some(DependencyCategory::DeclaredAndUsed)
        );
        assert_eq!(
            category_of(&s, "org.slf4j.helpers"),
            Some(DependencyCategory::DeclaredAndUsed)
        );
        assert_eq!(
            category_of(&s, "com.github.luben"),
            None,
            "the group matches an observed package, so it is not `no static import found`"
        );
        assert_eq!(
            s.by_category(DependencyCategory::DeclaredButUnobserved)
                .len(),
            0
        );
        assert_eq!(
            s.by_category(DependencyCategory::ObservedButUndeclared)
                .len(),
            0
        );
    }

    /// Never mid-segment: `com.foo` does not match `com.foobar.Type`'s package `com.foobar` — the
    /// package stays `undeclared` and the group stays `no static import found`.
    #[test]
    fn java_declared_group_does_not_match_mid_segment() {
        let s = reconcile_eco("java", &["com.foo"], &["com.foobar.Type"]);
        assert_eq!(
            category_of(&s, "com.foobar"),
            Some(DependencyCategory::ObservedButUndeclared)
        );
        assert_eq!(
            category_of(&s, "com.foo"),
            Some(DependencyCategory::DeclaredButUnobserved)
        );
    }

    /// The exact case still matches: declared `org.mockito`, observed package `org.mockito`.
    #[test]
    fn java_exact_package_still_matches() {
        let s = reconcile_eco("java", &["org.mockito"], &["org.mockito.Mockito"]);
        assert_eq!(
            category_of(&s, "org.mockito"),
            Some(DependencyCategory::DeclaredAndUsed)
        );
        assert_eq!(s.entries.len(), 1, "{:?}", s.entries);
    }

    /// The segment rule is Java's only: npm (`lodash` never matches the distinct package
    /// `lodash.get`, at either site), python and cargo keep exact matching.
    #[test]
    fn npm_python_and_cargo_matching_stay_exact() {
        let npm = reconcile_eco("npm", &["lodash"], &["lodash.get"]);
        assert_eq!(
            category_of(&npm, "lodash.get"),
            Some(DependencyCategory::ObservedButUndeclared)
        );
        assert_eq!(
            category_of(&npm, "lodash"),
            Some(DependencyCategory::DeclaredButUnobserved)
        );

        let py = reconcile_eco(
            "python",
            &["google.cloud", "zope"],
            &["google.cloud.storage", "zope.interface"],
        );
        assert_eq!(
            category_of(&py, "google"),
            Some(DependencyCategory::ObservedButUndeclared)
        );
        assert_eq!(
            category_of(&py, "google.cloud"),
            Some(DependencyCategory::DeclaredButUnobserved)
        );
        assert_eq!(
            category_of(&py, "zope"),
            Some(DependencyCategory::DeclaredAndUsed)
        );

        let cargo = reconcile_eco("cargo", &["serde"], &["serde_json::Value"]);
        assert_eq!(
            category_of(&cargo, "serde_json"),
            Some(DependencyCategory::ObservedButUndeclared)
        );
        assert_eq!(
            category_of(&cargo, "serde"),
            Some(DependencyCategory::DeclaredButUnobserved)
        );
    }
}
