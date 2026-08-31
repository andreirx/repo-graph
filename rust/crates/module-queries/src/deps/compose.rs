//! Dependency reconciliation composition layer.
//!
//! Orchestrates data loading from storage and passes it to the
//! reconciliation engine. This is the entry point for DEP-1
//! dependency analysis.

use std::collections::{HashMap, HashSet};

use repo_graph_storage::types::ModuleCandidate;
use repo_graph_storage::StorageConnection;

use super::classify::{classify_observed, ObservedKind};
use super::reconcile::{reconcile_module_dependencies, ReconcileInput};
use super::resolve::{
    build_file_specifier_sets, build_identifier_resolution_map, is_bound, resolve_import_specifier,
};
use super::types::{ManifestContext, ModuleDependencySummary, ProvenanceRead};
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
    /// Outcome of reading persisted parsed-manifest provenance for this snapshot (§2.2; operator
    /// rulings 2026-08-26). The quad-state carries the SPECIFIC cause when no exact file can be
    /// pinned (predates tracking vs. read failure vs. corruption) instead of collapsing them to a
    /// single label. `Tracked` (even empty) = provenance was tracked; each module attaches its exact
    /// manifest by longest-ancestor-`dir` match, never a filesystem rescan.
    pub manifest_provenance: ProvenanceRead,
}

/// Result of composing dependency summaries.
#[derive(Debug, Clone)]
pub struct ComposeDependenciesResult {
    /// Per-module dependency summaries.
    pub summaries: Vec<ModuleDependencySummary>,
    /// Total external imports observed across all modules.
    pub total_external_imports: usize,
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
    // Per-file set of import-declaration SPECIFIERS (the exact `import X.Y.Z` / `use a::b` strings,
    // §2.1 review-3 item 1). This is the deterministic import-declaration fact that lets a dotted
    // value like `org.springframework.boot.SpringApplication` (a real Java import) be admitted while
    // `file.toString` (a method chain — no such import) is rejected.
    let file_specifiers = build_file_specifier_sets(&import_bindings);

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
    // Per-module count of observed references dropped as non-import call targets (§2.1) — bare
    // unbound identifiers like `chr`/`class`/`aiter`. Folded into the module's rejected count so
    // the unattributed headline stays honest (they are NOT unattributed imports).
    let mut module_rejected: HashMap<String, usize> = HashMap::new();

    // Group external imports by module canonical path.
    // Resolve identifiers to specifiers using import bindings.
    for import in &external_imports {
        if let Some(&module_uid) = file_to_module_uid.get(import.source_file_uid.as_str()) {
            if let Some(&module) = uid_to_module.get(module_uid) {
                let canonical_path = &module.canonical_root_path;
                // Track module_key for ecosystem filtering (contains "npm:"/"cargo:"/… prefix).
                module_keys.insert(canonical_path.clone(), module.module_key.clone());

                // Resolve the specifier:
                // 1. Try to resolve via import binding (e.g., "useState" → "react")
                // 2. For member access like "React.createElement", try the receiver ("React")
                // 3. Fall back to the raw target_key if no binding found
                let file_uid = import.source_file_uid.as_str();
                let bound = is_bound(&import.specifier, file_uid, &identifier_to_specifier);
                let resolved_specifier =
                    resolve_import_specifier(&import.specifier, file_uid, &identifier_to_specifier);

                // §2.1 (review-3 item 1): admit to the observed bucket on IMPORT-DECLARATION
                // EVIDENCE, never on bare dotted shape. `specifier_backed` = the resolved value is
                // literally one of this file's import-declaration specifiers.
                let specifier_backed = file_specifiers
                    .get(file_uid)
                    .is_some_and(|s| s.contains(resolved_specifier.as_str()));
                match admit_observed(
                    &resolved_specifier,
                    &input.ecosystem,
                    &input.runtime_builtins,
                    bound,
                    specifier_backed,
                ) {
                    Admission::Import => module_imports
                        .entry(canonical_path.clone())
                        .or_default()
                        .push(resolved_specifier),
                    Admission::Rejected => {
                        *module_rejected.entry(canonical_path.clone()).or_default() += 1
                    }
                    Admission::Skip => {}
                }
            }
        }
    }

    // Group declared dependencies by module canonical path.
    for dep in &package_deps {
        // DEPS-LIST-REWRITE-1 (django npm-junk fix): declared deps are attached AT INDEX TIME to the
        // SOURCE file that owns them, by that file's own language resolver (a `.py` file carries
        // `pyproject.toml` deps; a `.js` tooling file carries `package.json` deps). Under a
        // dominant-language ecosystem view, keep ONLY deps whose source-file language belongs to that
        // ecosystem — so django's Python view never renders biome/grunt/puppeteer from a stray
        // `package.json`. For `none-detected` (no manifest reader), no source ecosystem matches, so
        // declared deps are simply not attributed (the imports still count toward the headline). This
        // is a pure query-time filter over already-stored data — no extractor-emitted data changes.
        if path_ecosystem(&dep.file_path) != Some(input.ecosystem.as_str()) {
            continue;
        }
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

    // Collect all modules to reconcile. Review-5 item 2: this MUST include modules that carry ONLY
    // rejected references (call-expression fragments, no admitted import, no declared dep). Omitting
    // them dropped their `rejected_non_specifier` counts from every summary — so `total_rejected`
    // undercounted and `compute_unattributed` then re-labelled those non-import fragments as
    // "unattributed external imports" (a false headline). Folding `module_rejected`'s keys in gives
    // each such module a summary whose `rejected_non_specifier` is counted, keeping the headline math
    // honest. The ecosystem filter below still applies to these keys, exactly as it does to imports.
    let all_module_paths: HashSet<&str> =
        reconcilable_module_paths(&module_imports, &module_declared, &module_rejected);

    // Ecosystem declared-module key prefix (DEPS-ATTRIB-2): npm→`npm:`, cargo→`cargo:`,
    // python→`pyproject:`, java→`gradle:`; `None` for `none-detected`/unknown (every module included).
    let ecosystem_prefix: Option<&str> = match input.ecosystem.as_str() {
        "cargo" => Some("cargo:"),
        "npm" => Some("npm:"),
        "python" => Some("pyproject:"),
        "java" => Some("gradle:"),
        _ => None,
    };
    // Whether ANY declared module of this ecosystem exists in the snapshot. When one does (django,
    // FRAKTAG, repo-graph — a root/workspace manifest was discovered as an ecosystem module), the
    // strict prefix gate is kept EXACTLY as before (byte-parity for repos that already worked). The
    // containment fallback below activates ONLY when the ecosystem has ZERO declared modules — the
    // DEMONSTRATED glamCRM nested-workspace shape (no root package.json / pnpm-workspace.yaml → repo-
    // index discovers no `npm:` module, so the manifest-governed source is owned by coarse `inferred:`
    // directory modules the old gate dropped). Gating on demonstrated variation, not imagined hybrids.
    let has_ecosystem_module =
        ecosystem_prefix.is_some_and(|p| modules.iter().any(|m| m.module_key.starts_with(p)));

    // 6. Reconcile each module.
    let mut summaries = Vec::new();

    for canonical_path in all_module_paths {
        // A module belongs to this ecosystem's view when its `module_key` carries the ecosystem prefix,
        // OR — only in the zero-ecosystem-module fallback — it is STRUCTURALLY covered by a PARSED
        // manifest of this ecosystem (file/manifest CONTAINMENT, not declared-dep presence — review-0
        // item 2). A module with no `module_key` keeps the prior "included" behaviour (no filter block
        // runs). See [`module_covered_by_parsed_manifest`] for why the earlier `module_has_manifest`
        // gate (NON-empty declared-dep rows) was the wrong predicate.
        let owns_ecosystem_manifest = !has_ecosystem_module
            && module_covered_by_parsed_manifest(
                canonical_path,
                &input.ecosystem,
                &input.manifest_provenance,
            );
        if let Some(module_key) = module_keys.get(canonical_path) {
            let prefix_match = ecosystem_prefix.is_none_or(|p| module_key.starts_with(p));
            if !(prefix_match || owns_ecosystem_manifest) {
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
        // Manifest provenance (DEPS-LIST-REWRITE-1 §2.2; operator ruling 2026-08-26): NEVER a
        // fabricated fixed-name path. The exact file comes from the persisted parsed-manifest
        // records; a Maven/Gradle repo can never report `package.json`, and `build.gradle.kts`
        // renders as itself because the record stores what was PARSED.
        let manifest_context = attach_manifest_context(
            has_manifest,
            canonical_path,
            &input.ecosystem,
            &input.manifest_provenance,
        );
        let manifest_scope_available = scope_available(has_manifest, &manifest_context);

        let reconcile_input = ReconcileInput {
            module: canonical_path.to_string(),
            manifest_context,
            declared_dependencies: declared,
            manifest_scope_available,
            observed_external_imports: imports,
            runtime_builtins: input.runtime_builtins.clone(),
            ecosystem: input.ecosystem.clone(),
            pre_rejected_non_specifier: module_rejected.get(canonical_path).copied().unwrap_or(0),
        };

        let summary = reconcile_module_dependencies(reconcile_input);
        summaries.push(summary);
    }

    // Sort summaries by module canonical path for deterministic output.
    summaries.sort_by(|a, b| a.module.cmp(&b.module));

    Ok(ComposeDependenciesResult {
        summaries,
        total_external_imports,
    })
}

/// The set of module canonical paths that need a reconciliation summary: every module with an
/// admitted import, a declared dependency, OR a rejected non-import reference (review-5 item 2).
/// A rejected-only module MUST be reconciled so its dropped-fragment count reaches a summary and is
/// accounted in `total_rejected` — otherwise those fragments leak into the unattributed headline.
/// Borrows the keys of all three maps (no allocation of the key strings).
fn reconcilable_module_paths<'a>(
    imports: &'a HashMap<String, Vec<String>>,
    declared: &'a HashMap<String, HashSet<String>>,
    rejected: &'a HashMap<String, usize>,
) -> HashSet<&'a str> {
    imports
        .keys()
        .map(String::as_str)
        .chain(declared.keys().map(String::as_str))
        .chain(rejected.keys().map(String::as_str))
        .collect()
}

/// Attach the manifest provenance for one module (§2.2; operator rulings 2026-08-26 + ruling 3).
///
/// The exact file is pinned from the persisted records FIRST, regardless of whether this module
/// carried declared deps — so a manifest that PARSED but declared ZERO dependencies still renders
/// its real path (ruling 3, item 3: parsed ≠ produced-rows), never "no manifest".
///
/// - A record whose `ecosystem` matches and whose `dir` is the LONGEST ancestor-or-equal of
///   `canonical_path` → [`ManifestContext::Parsed`] (the same nearest-manifest semantics the index
///   used, evaluated against the persisted records — NOT a filesystem rescan).
/// - Otherwise, if no declared deps were parsed for this module → [`ManifestContext::Absent`]
///   (genuinely no owning manifest).
/// - Otherwise (deps parsed but no exact file) → [`ManifestContext::ProvenanceUnavailable`] carrying
///   the SPECIFIC cause (ruling 3, item 2): predates tracking / read failure / corruption / tracked
///   but uncovered — never one collapsed label.
fn attach_manifest_context(
    has_manifest: bool,
    canonical_path: &str,
    ecosystem: &str,
    provenance: &ProvenanceRead,
) -> ManifestContext {
    // Pin the exact manifest from the records first (works even for a zero-dependency manifest).
    if let ProvenanceRead::Tracked(records) = provenance {
        let best = records
            .iter()
            .filter(|r| {
                r.ecosystem == ecosystem && dir_is_ancestor_or_equal(&r.dir, canonical_path)
            })
            .max_by_key(|r| r.dir.len());
        if let Some(r) = best {
            // review-4 item 1: a record that PARSED (`error == None`) pins its exact file; a record
            // that was PRESENT but unreadable/malformed (`error == Some`) renders unknown-with-reason
            // — never laundered into a `Parsed` zero-dep that would present read failure as
            // measured-empty. The reason is carried verbatim (ruling-3 item 2), tagged with the file.
            return match &r.error {
                None => ManifestContext::Parsed {
                    path: r.path.clone(),
                },
                Some(reason) => ManifestContext::ProvenanceUnavailable {
                    reason: format!("manifest {} present but not parsed: {}", r.path, reason),
                },
            };
        }

        // DEPS-ATTRIB-2: no ANCESTOR manifest, but a COARSE inferred module may CONTAIN several nested
        // manifests (glamCRM's `frontend` module spans `frontend/web` + `frontend/workspace` — neither
        // is at the module root). A single exact file can't be pinned, but the manifests DO cover this
        // module's subtree, so the honest cell names them rather than falsely claiming "no manifest
        // record covers this module". (Parsed records only — a present-but-unparsed nested manifest is
        // not asserted as covering.)
        let mut nested: Vec<&str> = records
            .iter()
            .filter(|r| {
                r.ecosystem == ecosystem
                    && r.error.is_none()
                    && dir_is_ancestor_or_equal(canonical_path, &r.dir)
            })
            .map(|r| r.path.as_str())
            .collect();
        if !nested.is_empty() {
            nested.sort_unstable();
            return ManifestContext::ProvenanceUnavailable {
                reason: format!(
                    "governed by {} nested {} manifest{} ({})",
                    nested.len(),
                    ecosystem,
                    if nested.len() == 1 { "" } else { "s" },
                    nested.join(", "),
                ),
            };
        }
    }

    if !has_manifest {
        // No declared deps AND no covering record → this module has no owning manifest.
        return ManifestContext::Absent;
    }

    // Declared deps exist but the exact file can't be pinned — carry the specific honest cause.
    let reason = match provenance {
        ProvenanceRead::Tracked(_) => {
            "declared deps parsed but no manifest record covers this module".to_string()
        }
        ProvenanceRead::Absent => "indexed before provenance tracking".to_string(),
        ProvenanceRead::Unavailable { reason } => reason.clone(),
    };
    ManifestContext::ProvenanceUnavailable { reason }
}

/// Whether a module has MANIFEST SCOPE — i.e. its owning manifest was read, so its imports can be
/// reconciled as `observed_but_undeclared` (declared-context known) rather than degraded to
/// `unknown_external_like`/unattributed (§2.5; ruling-3 item 3).
///
/// True when EITHER: the module carries declared-dep rows (`has_manifest`), OR its owning manifest
/// is in the parsed-provenance set (a [`ManifestContext::Parsed`]) — the latter is the item-3 fix:
/// a manifest that PARSED but declared ZERO deps has NO declared rows yet is genuine scope (parsed ≠
/// produced-rows). An old snapshot (`ProvenanceUnavailable`/`Absent`) has no records, so it falls
/// back to `has_manifest` — the honest best obtainable without provenance.
fn scope_available(has_manifest: bool, manifest_context: &ManifestContext) -> bool {
    has_manifest || matches!(manifest_context, ManifestContext::Parsed { .. })
}

/// Whether repo-relative directory `dir` is an ancestor of (or equal to) module path `path`.
/// The empty/`"."` dir is the repo root — ancestor of everything. Otherwise `path` must equal
/// `dir` or begin with `dir` + `/` (a true path-segment boundary, so `a/b` is not an ancestor
/// of `a/bc`).
fn dir_is_ancestor_or_equal(dir: &str, path: &str) -> bool {
    let dir = if dir == "." { "" } else { dir };
    let path = if path == "." { "" } else { path };
    if dir.is_empty() {
        return true;
    }
    path == dir
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Whether module `canonical_path` is STRUCTURALLY covered by a PARSED manifest of `ecosystem`
/// (DEPS-ATTRIB-2, review-0 item 2). The membership fact is FILE/MANIFEST CONTAINMENT, never
/// declared-dep presence. True when a provenance record of this ecosystem that PARSED
/// (`error == None`) is an ANCESTOR-or-equal of the module (the module sits UNDER the manifest) OR
/// is NESTED within the module (a coarse `inferred:` module whose subtree spans several leaf
/// manifests — glamCRM's `frontend` over `frontend/web` + `frontend/workspace`).
///
/// Why NOT the earlier `module_has_manifest` gate: that flag was set only from NON-empty
/// declared-dep rows (`compose` step 4), and the indexer stores no dep rows for a zero-dependency
/// manifest — so a nested manifest with indexed source but zero declared deps was wrongly excluded
/// from the ecosystem view. A parsed provenance record exists for a zero-dep manifest (ruling-3
/// item 3), so containment over the records is the correct structural predicate. When provenance is
/// NOT `Tracked` (old snapshot / unreadable) there is no structural evidence → `false` (honest
/// degradation: membership is never fabricated from a name or a dep-count without a covering record;
/// a fresh re-index restores the records and the module).
fn module_covered_by_parsed_manifest(
    canonical_path: &str,
    ecosystem: &str,
    provenance: &ProvenanceRead,
) -> bool {
    let ProvenanceRead::Tracked(records) = provenance else {
        return false;
    };
    records.iter().any(|r| {
        r.ecosystem == ecosystem
            && r.error.is_none()
            && (dir_is_ancestor_or_equal(&r.dir, canonical_path)
                || dir_is_ancestor_or_equal(canonical_path, &r.dir))
    })
}

/// The dependency ecosystem a source file belongs to, by extension (DEPS-LIST-REWRITE-1). Index-time
/// resolvers attach a file's declared deps keyed by ITS language, so this recovers which ecosystem a
/// stored declared-dep set belongs to — the query-time filter that keeps a Python-dominant view from
/// showing a stray `package.json`'s npm deps (django). `None` = a file whose language has no manifest
/// reader (config files, C/C++, Go, …) — such a file never carries declared deps anyway.
fn path_ecosystem(file_path: &str) -> Option<&'static str> {
    let ext = file_path.rsplit('.').next()?;
    Some(match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => "npm",
        "py" | "pyi" => "python",
        "rs" => "cargo",
        "java" => "java",
        _ => return None,
    })
}

/// The outcome of the §2.1 observed-bucket admission gate for one resolved reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    /// Import-declaration-backed (or a builtin) — goes to the observed bucket for reconciliation.
    Import,
    /// A non-import call target (method chain / member access / bare unbound word) — dropped and
    /// counted in `rejected_non_specifier` so the unattributed headline stays honest.
    Rejected,
    /// A local/relative reference — not external, neither attributed nor counted.
    Skip,
}

/// Decide whether one resolved observed reference enters the package namespace (§2.1; review-3
/// item 1). The rule is EVIDENCE, not shape:
///
/// - `Local` (relative/crate-path) → [`Admission::Skip`].
/// - `Builtin` → [`Admission::Import`] (a dotted builtin like `Math.sqrt`/`java.util.List` must
///   still reach the builtin bucket; the classifier already proved it a builtin).
/// - `NonSpecifier` (call-expression text with parens/whitespace/operators) → [`Admission::Rejected`].
/// - `Package` → admitted ONLY on import-declaration EVIDENCE: a resolved import binding (`bound`)
///   or the value being a literal import specifier of this file (`specifier_backed`). Review-4
///   item 1 removed the earlier `@`/`/`/`::` shape bypass — a package-boundary token is still just
///   SHAPE, and §2.1 requires evidence that the value "came from an import/require/use/include
///   declaration", not that it merely looks like a package. So an unbound, non-specifier-backed
///   `@scope/pkg` / `lodash/get` / `tokio::spawn` (a scoped-shaped string no import declares) is
///   [`Admission::Rejected`], exactly like the bare-dotted `file.toString` method chain.
fn admit_observed(
    resolved: &str,
    ecosystem: &str,
    builtins: &HashSet<String>,
    bound: bool,
    specifier_backed: bool,
) -> Admission {
    match classify_observed(resolved, ecosystem, builtins) {
        ObservedKind::Local => Admission::Skip,
        ObservedKind::Builtin { .. } => Admission::Import,
        ObservedKind::NonSpecifier => Admission::Rejected,
        ObservedKind::Package { .. } => {
            if bound || specifier_backed {
                Admission::Import
            } else {
                Admission::Rejected
            }
        }
    }
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
    use crate::deps::builtins::deps_runtime_builtins;
    use crate::deps::types::ManifestProvenance;

    fn prov(path: &str, dir: &str, eco: &str) -> ManifestProvenance {
        ManifestProvenance {
            path: path.to_string(),
            dir: dir.to_string(),
            ecosystem: eco.to_string(),
            error: None,
        }
    }

    fn prov_failed(path: &str, dir: &str, eco: &str, reason: &str) -> ManifestProvenance {
        ManifestProvenance {
            path: path.to_string(),
            dir: dir.to_string(),
            ecosystem: eco.to_string(),
            error: Some(reason.to_string()),
        }
    }

    #[test]
    fn reconcilable_modules_include_rejected_only_modules() {
        // review-5 item 2 REGRESSION: a module present ONLY in `module_rejected` (all its external
        // references were call-expression fragments — no admitted import, no declared dep) must
        // still be reconciled, so its dropped-fragment count is accounted (never re-labelled
        // unattributed). Before the fix the union skipped `module_rejected` and this module vanished.
        let mut imports: HashMap<String, Vec<String>> = HashMap::new();
        imports.insert("has_imports".to_string(), vec!["react".to_string()]);
        let mut declared: HashMap<String, HashSet<String>> = HashMap::new();
        declared.insert("has_declared".to_string(), HashSet::new());
        let mut rejected: HashMap<String, usize> = HashMap::new();
        rejected.insert("rejected_only".to_string(), 4);

        let set = reconcilable_module_paths(&imports, &declared, &rejected);
        assert!(set.contains("has_imports"));
        assert!(set.contains("has_declared"));
        assert!(
            set.contains("rejected_only"),
            "rejected-only module dropped from reconciliation: {set:?}"
        );
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn path_ecosystem_maps_extension_to_manifest_ecosystem() {
        // The django fix hinges on this: a Python source file's deps are python-ecosystem,
        // a JS tooling file's deps are npm — so a Python-dominant view drops the npm ones.
        assert_eq!(path_ecosystem("django/db/models.py"), Some("python"));
        assert_eq!(path_ecosystem("scripts/build.js"), Some("npm"));
        assert_eq!(path_ecosystem("app/ui.tsx"), Some("npm"));
        assert_eq!(path_ecosystem("src/lib.rs"), Some("cargo"));
        assert_eq!(path_ecosystem("src/Main.java"), Some("java"));
        // No manifest reader for these — no declared deps to attribute.
        assert_eq!(path_ecosystem("src/util.c"), None);
        assert_eq!(path_ecosystem("Makefile"), None);
        assert_eq!(path_ecosystem("go.mod"), None);
    }

    #[test]
    fn attach_no_manifest_and_no_record_is_absent() {
        assert_eq!(
            attach_manifest_context(false, "a/b", "npm", &ProvenanceRead::Tracked(vec![])),
            ManifestContext::Absent
        );
    }

    #[test]
    fn attach_zero_dep_manifest_still_renders_parsed_path() {
        // Ruling 3 item 3: a manifest that PARSED but produced no declared deps (`has_manifest`
        // false) must still render its exact file, never "no manifest".
        let records = vec![prov("pkg/package.json", "pkg", "npm")];
        assert_eq!(
            attach_manifest_context(false, "pkg", "npm", &ProvenanceRead::Tracked(records)),
            ManifestContext::Parsed {
                path: "pkg/package.json".to_string()
            }
        );
    }

    #[test]
    fn attach_absent_provenance_reason_is_predates_tracking() {
        // Old snapshot (provenance not tracked): deps parsed but exact file unknown.
        assert_eq!(
            attach_manifest_context(true, "a/b", "npm", &ProvenanceRead::Absent),
            ManifestContext::ProvenanceUnavailable {
                reason: "indexed before provenance tracking".to_string()
            }
        );
    }

    #[test]
    fn attach_read_failure_reason_is_not_predates_tracking() {
        // Ruling 3 item 2: a read/parse failure must carry ITS OWN reason, never the old-snapshot one.
        let ctx = attach_manifest_context(
            true,
            "a/b",
            "npm",
            &ProvenanceRead::Unavailable {
                reason: "extraction diagnostics unreadable: disk error".to_string(),
            },
        );
        assert_eq!(
            ctx,
            ManifestContext::ProvenanceUnavailable {
                reason: "extraction diagnostics unreadable: disk error".to_string()
            }
        );
    }

    #[test]
    fn attach_picks_longest_ancestor_dir_in_matching_ecosystem() {
        let records = vec![
            prov("package.json", "", "npm"),
            prov("a/package.json", "a", "npm"),
            prov("a/build.gradle.kts", "a", "java"), // wrong ecosystem — ignored
        ];
        // Module a/b resolves to the nearest npm manifest a/package.json (longest ancestor).
        assert_eq!(
            attach_manifest_context(
                true,
                "a/b",
                "npm",
                &ProvenanceRead::Tracked(records.clone())
            ),
            ManifestContext::Parsed {
                path: "a/package.json".to_string()
            }
        );
        // Module c (only the root manifest is an ancestor) → root package.json.
        assert_eq!(
            attach_manifest_context(true, "c", "npm", &ProvenanceRead::Tracked(records)),
            ManifestContext::Parsed {
                path: "package.json".to_string()
            }
        );
    }

    #[test]
    fn attach_gradle_kts_renders_as_itself() {
        let records = vec![prov("svc/build.gradle.kts", "svc", "java")];
        assert_eq!(
            attach_manifest_context(true, "svc", "java", &ProvenanceRead::Tracked(records)),
            ManifestContext::Parsed {
                path: "svc/build.gradle.kts".to_string()
            }
        );
    }

    #[test]
    fn attach_coarse_module_containing_nested_manifests_names_them_truthfully() {
        // DEPS-ATTRIB-2: glamCRM's `frontend` inferred module CONTAINS frontend/web + frontend/workspace
        // (no frontend/package.json at the module root). No ancestor manifest → the honest cell must
        // name the nested manifests, NOT claim "no manifest record covers this module".
        let records = vec![
            prov("frontend/web/package.json", "frontend/web", "npm"),
            prov(
                "frontend/workspace/package.json",
                "frontend/workspace",
                "npm",
            ),
        ];
        assert_eq!(
            attach_manifest_context(true, "frontend", "npm", &ProvenanceRead::Tracked(records)),
            ManifestContext::ProvenanceUnavailable {
                reason: "governed by 2 nested npm manifests (frontend/web/package.json, \
                         frontend/workspace/package.json)"
                    .to_string()
            }
        );
    }

    #[test]
    fn attach_tracked_but_no_match_is_unavailable_with_reason() {
        // Provenance tracked, but no record covers this module/ecosystem → unknown-with-reason.
        let records = vec![prov("other/package.json", "other", "npm")];
        assert_eq!(
            attach_manifest_context(true, "svc", "java", &ProvenanceRead::Tracked(records)),
            ManifestContext::ProvenanceUnavailable {
                reason: "declared deps parsed but no manifest record covers this module"
                    .to_string()
            }
        );
    }

    #[test]
    fn attach_present_but_unreadable_manifest_is_unavailable_not_parsed() {
        // review-4 item 1 REGRESSION: a manifest that was PRESENT but could not be parsed (io error
        // / malformed) must render unknown-with-reason, NEVER a `Parsed` zero-dep that would present
        // a read failure as measured-empty. The reason is carried verbatim, tagged with the file.
        let records = vec![prov_failed(
            "svc/build.gradle.kts",
            "svc",
            "java",
            "unreadable: permission denied",
        )];
        assert_eq!(
            attach_manifest_context(false, "svc", "java", &ProvenanceRead::Tracked(records)),
            ManifestContext::ProvenanceUnavailable {
                reason: "manifest svc/build.gradle.kts present but not parsed: unreadable: \
                         permission denied"
                    .to_string()
            }
        );
    }

    #[test]
    fn present_but_unreadable_manifest_has_no_reconcile_scope() {
        // The failed manifest yields no declared rows (`has_manifest == false`) and a
        // ProvenanceUnavailable context → NOT scope-available, so its imports render unattributed
        // WITH the reason rather than as a deceptive declared/observed reconciliation.
        assert!(!scope_available(
            false,
            &ManifestContext::ProvenanceUnavailable {
                reason: "manifest a/package.json present but not parsed: malformed JSON".into()
            }
        ));
    }

    #[test]
    fn module_covered_by_parsed_manifest_uses_containment_not_dep_presence() {
        // DEPS-ATTRIB-2 review-0 item 2: membership is FILE/MANIFEST CONTAINMENT, not declared-dep
        // presence. A coarse inferred module `frontend` is covered by its NESTED leaf manifests even
        // though no manifest sits at the module root, and a FINE module `serverless/packages/backend`
        // is covered by the manifest at its own dir. A zero-DEPENDENCY manifest still counts (it is a
        // parsed record) — the exact gap the old `module_has_manifest` gate created.
        let records = vec![
            prov("frontend/web/package.json", "frontend/web", "npm"),
            prov(
                "frontend/workspace/package.json",
                "frontend/workspace",
                "npm",
            ),
            // zero-dependency nested manifest — a parsed record even with no declared deps.
            prov(
                "serverless/packages/backend/package.json",
                "serverless/packages/backend",
                "npm",
            ),
            prov("backend/build.gradle", "backend", "java"),
        ];
        let tracked = ProvenanceRead::Tracked(records);
        // Coarse module NESTING several npm manifests → covered.
        assert!(module_covered_by_parsed_manifest(
            "frontend", "npm", &tracked
        ));
        // Fine module AT a (zero-dep) npm manifest dir → covered.
        assert!(module_covered_by_parsed_manifest(
            "serverless/packages/backend",
            "npm",
            &tracked
        ));
        // Wrong ecosystem: the java manifest does not cover an npm view of `backend`.
        assert!(!module_covered_by_parsed_manifest(
            "backend", "npm", &tracked
        ));
        // The java manifest DOES cover a java view of `backend`.
        assert!(module_covered_by_parsed_manifest(
            "backend", "java", &tracked
        ));
        // A module with no covering manifest in either direction → not covered.
        assert!(!module_covered_by_parsed_manifest(
            "unrelated/pkg",
            "npm",
            &tracked
        ));
        // No structural evidence (old snapshot) → never fabricated as covered.
        assert!(!module_covered_by_parsed_manifest(
            "frontend",
            "npm",
            &ProvenanceRead::Absent
        ));
    }

    #[test]
    fn dir_ancestor_respects_segment_boundary() {
        assert!(dir_is_ancestor_or_equal("a/b", "a/b"));
        assert!(dir_is_ancestor_or_equal("a/b", "a/b/c"));
        assert!(dir_is_ancestor_or_equal("", "anything/here"));
        assert!(dir_is_ancestor_or_equal(".", "anything"));
        // Not a segment boundary: a/b is NOT an ancestor of a/bc.
        assert!(!dir_is_ancestor_or_equal("a/b", "a/bc"));
        assert!(!dir_is_ancestor_or_equal("a/b", "a"));
    }

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

    // ── §2.1 admission gate (review-3 item 1): evidence, never bare dotted shape ──

    #[test]
    fn admit_rejects_unbound_method_chain() {
        // review-3 item 1 REGRESSION: petclinic rendered `file.toString` under `undeclared`. A bare
        // dotted, unbound value that no import declares is a method chain, not a package → Rejected.
        let b = deps_runtime_builtins("java");
        assert_eq!(
            admit_observed("file.toString", "java", &b, false, false),
            Admission::Rejected
        );
        // Same for a JS member-access chain.
        let bn = deps_runtime_builtins("npm");
        assert_eq!(
            admit_observed("obj.method", "npm", &bn, false, false),
            Admission::Rejected
        );
    }

    #[test]
    fn admit_keeps_dotted_import_backed_by_specifier() {
        // A real Java import used fully-qualified/unbound: NOT bound by identifier, but the file's
        // import-declaration specifiers contain it → admitted (evidence, not shape).
        let b = deps_runtime_builtins("java");
        assert_eq!(
            admit_observed(
                "org.springframework.boot.SpringApplication",
                "java",
                &b,
                false,
                true
            ),
            Admission::Import
        );
        // A Python dotted submodule, unbound but specifier-backed.
        let p = deps_runtime_builtins("python");
        assert_eq!(
            admit_observed("asgiref.sync", "python", &p, false, true),
            Admission::Import
        );
    }

    #[test]
    fn admit_keeps_bound_values_regardless_of_shape() {
        let b = deps_runtime_builtins("npm");
        // Bound → admitted regardless of shape.
        assert_eq!(
            admit_observed("react", "npm", &b, true, false),
            Admission::Import
        );
        // A scoped package that IS import-declaration-backed (specifier_backed) is admitted.
        assert_eq!(
            admit_observed("@fraktag/engine", "npm", &b, false, true),
            Admission::Import
        );
        // A bound Rust path is admitted.
        let c = cargo_runtime_builtins();
        assert_eq!(
            admit_observed("tokio::spawn", "cargo", &c, true, false),
            Admission::Import
        );
    }

    #[test]
    fn admit_rejects_unbound_unbacked_scoped_subpath_and_path() {
        // review-4 item 1 REGRESSION: a package-boundary token (`@`/`/`/`::`) is SHAPE, not
        // import-declaration evidence. An unbound, non-specifier-backed value that merely LOOKS like
        // a scoped package / subpath / Rust path is NOT admitted — §2.1 requires it to have come
        // from an import declaration, which `bound == false && specifier_backed == false` denies.
        let b = deps_runtime_builtins("npm");
        assert_eq!(
            admit_observed("@scope/pkg", "npm", &b, false, false),
            Admission::Rejected
        );
        assert_eq!(
            admit_observed("lodash/get", "npm", &b, false, false),
            Admission::Rejected
        );
        let c = cargo_runtime_builtins();
        assert_eq!(
            admit_observed("tokio::spawn", "cargo", &c, false, false),
            Admission::Rejected
        );
    }

    #[test]
    fn admit_dotted_builtin_survives_to_builtin_bucket() {
        // Dotted builtins (`Math.sqrt`, `java.util.List`) are Builtin-kind → admitted even unbound,
        // so they classify as builtins rather than being dropped.
        let bn = deps_runtime_builtins("npm");
        assert_eq!(
            admit_observed("Math.sqrt", "npm", &bn, false, false),
            Admission::Import
        );
        let bj = deps_runtime_builtins("java");
        assert_eq!(
            admit_observed("java.util.List", "java", &bj, false, false),
            Admission::Import
        );
    }

    #[test]
    fn admit_rejects_call_expression_text_and_skips_local() {
        let b = deps_runtime_builtins("npm");
        assert_eq!(
            admit_observed("Object.values(x).filter", "npm", &b, false, false),
            Admission::Rejected
        );
        assert_eq!(
            admit_observed("./utils", "npm", &b, false, true),
            Admission::Skip
        );
    }

    #[test]
    fn zero_dep_parsed_manifest_has_scope_but_old_snapshot_needs_declared_rows() {
        // Ruling-3 item 3: a parsed manifest with ZERO declared deps (`has_manifest == false`) is
        // STILL manifest scope via its `Parsed` context — so its imports reconcile as
        // observed_but_undeclared, not unattributed.
        assert!(scope_available(
            false,
            &ManifestContext::Parsed {
                path: "pkg/package.json".into()
            }
        ));
        // With declared rows, scope holds regardless of provenance.
        assert!(scope_available(true, &ManifestContext::Absent));
        // Old snapshot (no provenance record) + no declared rows → no scope (honest degradation:
        // parsed-zero-dep can't be recovered without a record).
        assert!(!scope_available(
            false,
            &ManifestContext::ProvenanceUnavailable {
                reason: "indexed before provenance tracking".into()
            }
        ));
        assert!(!scope_available(false, &ManifestContext::Absent));
    }
}
