//! Unresolved-edge classifier (pure core logic).
//!
//! Mirror of `src/core/classification/unresolved-classifier.ts`.
//!
//! Given an unresolved edge + its extractor-determined category +
//! snapshot-level and file-level signals, returns a classification
//! verdict: `{ classification, basisCode }`.
//!
//! Pure function. No I/O, no storage, no state. All inputs are
//! plain data. The function is deterministic with respect to its
//! inputs.
//!
//! Decision structure (first-match-wins):
//!
//!   A. Category shortcuts (dispatch BEFORE identifier extraction):
//!      - `this`-receiver categories → immediate INTERNAL verdict
//!      - `imports_file_not_found` → dispatches to a separate
//!        `classify_unresolved_import` sub-classifier with its own
//!        multi-rule chain (relative path, package dep, runtime
//!        module, project alias, TS stable-key, Rust crate-
//!        internal heuristic)
//!      - `other` → immediate UNKNOWN verdict
//!
//!   B. Identifier-based classification (remaining categories):
//!      1. Extract the target identifier from `targetKey` /
//!         `metadataJson` (category-aware: receiver vs callee)
//!      2. Same-file symbol match (subtype-aware: value symbols
//!         for CALLS, class symbols for INSTANTIATES, interface
//!         symbols for IMPLEMENTS)
//!      3. Import binding dispatch:
//!         a. Runtime/stdlib module match
//!         b. Package dependency match
//!         c. Relative import
//!         d. Project alias match
//!      4. Runtime global identifier match
//!      5. Otherwise → UNKNOWN
//!
//! The ordering within section B reflects evidence strength: a
//! lexical same-file binding is more certain than an imported
//! binding. Alias-matching is ranked last among internal
//! categories because first-slice alias data may be incomplete.

use crate::signals::{
    has_package_dependency, has_runtime_builtin_identifier, has_runtime_builtin_module,
    matches_any_alias,
};
use crate::types::{
    ClassifierEdgeInput, ClassifierVerdict, FileSignals, ImportBinding, PackageDependencySet,
    SnapshotSignals, UnresolvedEdgeBasisCode, UnresolvedEdgeCategory, UnresolvedEdgeClassification,
};

/// Classify a single unresolved edge.
///
/// Mirror of `classifyUnresolvedEdge` from
/// `unresolved-classifier.ts:68`. See module-level docs for the
/// full decision structure (category shortcuts + identifier-based
/// classification chain).
///
/// This function is re-exported as `pub` from the crate root.
pub fn classify_unresolved_edge(
    edge: &ClassifierEdgeInput,
    category: UnresolvedEdgeCategory,
    snapshot_signals: &SnapshotSignals,
    file_signals: &FileSignals,
) -> ClassifierVerdict {
    // Rule 1: `this`-receiver shortcut.
    if category == UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext
        || category == UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo
    {
        return internal(UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal);
    }

    // IMPORTS_FILE_NOT_FOUND: classify by import kind.
    if category == UnresolvedEdgeCategory::ImportsFileNotFound {
        return classify_unresolved_import(edge, snapshot_signals, file_signals);
    }

    // OTHER: no semantic rules apply.
    if category == UnresolvedEdgeCategory::Other {
        return unknown();
    }

    // Every remaining category classifies by an IDENTIFIER extracted
    // from the edge's targetKey (possibly from metadataJson).
    let identifier = match extract_target_identifier(edge, category) {
        Some(id) => id,
        None => return unknown(),
    };

    // Rule 2: same-file symbol match (strongest lexical evidence).
    if matches_same_file_by_role(&identifier, category, file_signals) {
        return internal(same_file_basis_for(category));
    }

    // Find the import binding (if any) that introduced this identifier.
    let binding = find_binding_for_identifier(&identifier, &file_signals.import_bindings);

    if let Some(binding) = binding {
        // Rule 5a: runtime/stdlib module.
        if !binding.is_relative
            && has_runtime_builtin_module(&snapshot_signals.runtime_builtins, &binding.specifier)
        {
            return external(UnresolvedEdgeBasisCode::SpecifierMatchesRuntimeModule);
        }
        // Rule 5b: declared package dependency.
        if !binding.is_relative
            && has_package_dependency(&file_signals.package_dependencies, &binding.specifier)
        {
            return external(external_basis_for(category));
        }
        // Rule 5c: relative import.
        if binding.is_relative {
            return internal(internal_import_basis_for(category));
        }
        // Rule 5d: project alias.
        if matches_any_alias(&binding.specifier, &file_signals.tsconfig_aliases) {
            return internal(UnresolvedEdgeBasisCode::SpecifierMatchesProjectAlias);
        }
        // Non-relative specifier that matched nothing. Fall through.
    }

    // Rule 6: runtime global identifier.
    if has_runtime_builtin_identifier(&snapshot_signals.runtime_builtins, &identifier) {
        return external(runtime_global_basis_for(category));
    }

    // Rule 7: unknown.
    unknown()
}

// ── Unresolved import classifier ─────────────────────────────────

/// Classify an unresolved IMPORTS edge by import kind.
///
/// Mirrors `classifyUnresolvedImport` from
/// `unresolved-classifier.ts:185`.
fn classify_unresolved_import(
    edge: &ClassifierEdgeInput,
    snapshot_signals: &SnapshotSignals,
    file_signals: &FileSignals,
) -> ClassifierVerdict {
    // Read the specifier + relativity exactly as before (metadata rawPath/specifier,
    // target_key fallback) — extracted to `parse_import_specifier` so
    // `resolve_external_dependency_name` (ATTRIBUTION-1) reads the specifier the same way.
    let (specifier, is_relative) = parse_import_specifier(edge);

    // Relative → internal.
    if is_relative {
        return internal(UnresolvedEdgeBasisCode::RelativeImportTargetUnresolved);
    }

    // Check package dependency (with Rust hyphen normalization + Java prefix) — the shared
    // reduction (ATTRIBUTION-1: `resolve_declared_dependency` reuses this exact matching so a
    // named dependency renders as the DECLARED manifest name, not the raw specifier). The
    // verdict is unchanged: external iff a declared dependency matches.
    if resolve_declared_dependency(&specifier, &file_signals.package_dependencies).is_some() {
        return external(UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency);
    }

    // Check runtime modules (the specifier itself, and its scoped base — `std::collections`
    // → base `std`). `base_specifier` is the shared split reused by `resolve_declared_dependency`.
    if has_runtime_builtin_module(&snapshot_signals.runtime_builtins, &specifier)
        || has_runtime_builtin_module(
            &snapshot_signals.runtime_builtins,
            base_specifier(&specifier),
        )
    {
        return external(UnresolvedEdgeBasisCode::SpecifierMatchesRuntimeModule);
    }

    // Check project aliases.
    if matches_any_alias(&specifier, &file_signals.tsconfig_aliases) {
        return internal(UnresolvedEdgeBasisCode::SpecifierMatchesProjectAlias);
    }

    // TS stable-key form (contains ":FILE") → internal file import.
    if specifier.contains(":FILE") || edge.target_key.contains(":FILE") {
        return internal(UnresolvedEdgeBasisCode::RelativeImportTargetUnresolved);
    }

    // Rust crate-internal module import heuristic.
    if is_rust_crate_internal_import(edge, &specifier) {
        return internal(UnresolvedEdgeBasisCode::RustCrateInternalModuleHeuristic);
    }

    // Truly unknown.
    unknown()
}

/// Detect Rust crate-internal module imports.
///
/// Mirrors `isRustCrateInternalImport` from
/// `unresolved-classifier.ts:306`.
fn is_rust_crate_internal_import(edge: &ClassifierEdgeInput, specifier: &str) -> bool {
    let meta_str = match &edge.metadata_json {
        Some(s) => s,
        None => return false,
    };
    let meta = match serde_json::from_str::<serde_json::Value>(meta_str) {
        Ok(v) => v,
        Err(_) => return false,
    };
    // Rust extractor stores "specifier" key. TS stores "rawPath".
    // If "rawPath" exists, this is a TS import — not applicable.
    if meta.get("rawPath").and_then(|v| v.as_str()).is_some() {
        return false;
    }
    if meta.get("specifier").and_then(|v| v.as_str()).is_none() {
        return false;
    }

    // First path segment must be a valid Rust module name.
    let first_segment = if specifier.contains("::") {
        specifier.split("::").next().unwrap_or("")
    } else {
        specifier
    };
    if first_segment.is_empty() {
        return false;
    }
    is_rust_module_name(first_segment)
}

/// Check if a string looks like a valid Rust module name:
/// starts with lowercase, contains only lowercase + digits + underscores.
/// Mirrors the TS regex `/^[a-z][a-z0-9_]*$/`.
fn is_rust_module_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// ── Same-file matching ───────────────────────────────────────────

/// Subtype-aware same-file match.
///
/// Mirrors `matchesSameFileByRole` from
/// `unresolved-classifier.ts:347`.
fn matches_same_file_by_role(
    identifier: &str,
    category: UnresolvedEdgeCategory,
    file_signals: &FileSignals,
) -> bool {
    match category {
        UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing
        | UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo => file_signals
            .same_file_value_symbols
            .iter()
            .any(|s| s == identifier),
        UnresolvedEdgeCategory::InstantiatesClassNotFound => file_signals
            .same_file_class_symbols
            .iter()
            .any(|s| s == identifier),
        UnresolvedEdgeCategory::ImplementsInterfaceNotFound => file_signals
            .same_file_interface_symbols
            .iter()
            .any(|s| s == identifier),
        _ => false,
    }
}

// ── Identifier extraction ────────────────────────────────────────

/// Extract the target identifier to look up against file signals.
///
/// Mirrors `extractTargetIdentifier` from
/// `unresolved-classifier.ts:462`. Not exported outside this
/// module (not in the locked public API).
fn extract_target_identifier(
    edge: &ClassifierEdgeInput,
    category: UnresolvedEdgeCategory,
) -> Option<String> {
    // Read pre-rewrite targetKey from metadata when present.
    let mut key = edge.target_key.clone();
    if let Some(ref meta_str) = edge.metadata_json {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
            if let Some(raw_callee) = meta.get("rawCalleeName").and_then(|v| v.as_str()) {
                key = raw_callee.to_string();
            }
        }
    }

    match category {
        UnresolvedEdgeCategory::InstantiatesClassNotFound
        | UnresolvedEdgeCategory::ImplementsInterfaceNotFound
        | UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing => {
            if is_simple_identifier(&key) {
                Some(key)
            } else {
                None
            }
        }
        UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo => {
            // Expect "receiver.x.y.method". First dotted segment is
            // the receiver.
            let dot_idx = key.find('.')?;
            if dot_idx == 0 {
                return None;
            }
            let receiver = &key[..dot_idx];
            if is_simple_identifier(receiver) {
                Some(receiver.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Check if a string is a simple JS/TS/Java identifier.
/// Mirrors the TS regex `/^[A-Za-z_$][\w$]*$/`.
fn is_simple_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

// ── Verdict constructors ─────────────────────────────────────────

fn external(basis: UnresolvedEdgeBasisCode) -> ClassifierVerdict {
    ClassifierVerdict {
        classification: UnresolvedEdgeClassification::ExternalLibraryCandidate,
        basis_code: basis,
    }
}

fn internal(basis: UnresolvedEdgeBasisCode) -> ClassifierVerdict {
    ClassifierVerdict {
        classification: UnresolvedEdgeClassification::InternalCandidate,
        basis_code: basis,
    }
}

fn unknown() -> ClassifierVerdict {
    ClassifierVerdict {
        classification: UnresolvedEdgeClassification::Unknown,
        basis_code: UnresolvedEdgeBasisCode::NoSupportingSignal,
    }
}

// ── Category-aware basis code selectors ──────────────────────────

fn same_file_basis_for(category: UnresolvedEdgeCategory) -> UnresolvedEdgeBasisCode {
    if category == UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo {
        UnresolvedEdgeBasisCode::ReceiverMatchesSameFileSymbol
    } else {
        UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol
    }
}

fn external_basis_for(category: UnresolvedEdgeCategory) -> UnresolvedEdgeBasisCode {
    if category == UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo {
        UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport
    } else {
        UnresolvedEdgeBasisCode::CalleeMatchesExternalImport
    }
}

fn internal_import_basis_for(category: UnresolvedEdgeCategory) -> UnresolvedEdgeBasisCode {
    if category == UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo {
        UnresolvedEdgeBasisCode::ReceiverMatchesInternalImport
    } else {
        UnresolvedEdgeBasisCode::CalleeMatchesInternalImport
    }
}

fn runtime_global_basis_for(category: UnresolvedEdgeCategory) -> UnresolvedEdgeBasisCode {
    if category == UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo {
        UnresolvedEdgeBasisCode::ReceiverMatchesRuntimeGlobal
    } else {
        UnresolvedEdgeBasisCode::CalleeMatchesRuntimeGlobal
    }
}

fn find_binding_for_identifier<'a>(
    identifier: &str,
    bindings: &'a [crate::types::ImportBinding],
) -> Option<&'a crate::types::ImportBinding> {
    bindings.iter().find(|b| b.identifier == identifier)
}

// ── Named-dependency resolution (ATTRIBUTION-1) ──────────────────
//
// The name-returning form of the classifier's own external-import matching. Extracted
// from `classify_unresolved_edge` / `classify_unresolved_import` so the ATTRIBUTION-1
// attribution join REUSES the exact reduction (operator-directed, 2026-07-15) instead of
// duplicating it. `classify_*` call these helpers, so the classifier verdicts are
// byte-identical — only the returned *name* is new surface.

/// Read an unresolved IMPORTS edge's specifier + relativity exactly as
/// `classify_unresolved_import` did inline (metadata `rawPath` for TS / `specifier` for
/// Rust, `target_key` fallback; relative iff `.`/`crate::`/`super::`/`self::`). Extracted
/// verbatim so both the classifier and [`resolve_external_dependency_name`] read the
/// specifier identically.
fn parse_import_specifier(edge: &ClassifierEdgeInput) -> (String, bool) {
    let mut specifier = edge.target_key.clone();
    let mut is_relative = false;
    if let Some(ref meta_str) = edge.metadata_json {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(meta_str) {
            // TS extractor: rawPath is the original specifier.
            if let Some(raw_path) = meta.get("rawPath").and_then(|v| v.as_str()) {
                specifier = raw_path.to_string();
                is_relative = specifier.starts_with('.');
            }
            // Rust extractor: specifier is the crate/module path.
            if let Some(spec) = meta.get("specifier").and_then(|v| v.as_str()) {
                specifier = spec.to_string();
                is_relative = specifier.starts_with("crate::")
                    || specifier.starts_with("super::")
                    || specifier.starts_with("self::");
            }
        }
    }
    // Python relative imports: targetKey starts with ".".
    if !is_relative && specifier.starts_with('.') {
        is_relative = true;
    }
    (specifier, is_relative)
}

/// The base dependency/module segment of a scoped specifier — the part before the first
/// `"::"` (`serde::de` → `serde`), or the whole specifier when unscoped. Shared by the
/// runtime-module check and [`resolve_declared_dependency`] so the split is defined once.
fn base_specifier(specifier: &str) -> &str {
    match specifier.split_once("::") {
        Some((base, _)) => base,
        None => specifier,
    }
}

/// Reduce an import specifier to the DECLARED dependency name it matches, if any — the
/// package-dependency matching `classify_unresolved_import` performs, returning the matched
/// *declared* name instead of a bare verdict. Reduces a scoped path to its base
/// (`serde::de` → `serde`; `repo_graph_indexer::types` → `repo_graph_indexer`), applies Rust
/// hyphen normalization (`repo_graph_indexer` → `repo-graph-indexer`) and Java group-prefix
/// matching, and returns the manifest entry that matched (`repo-graph-indexer`), or `None`.
///
/// The returned name is the DECLARED dependency (`repo-graph-indexer`), never the raw
/// specifier (`repo_graph_indexer::types`) — the review-2 defect this fixes. `is_some()` is
/// true in exactly the cases the classifier's three package-dependency checks returned
/// external, so the classifier verdict is preserved.
pub(crate) fn resolve_declared_dependency(
    specifier: &str,
    declared: &PackageDependencySet,
) -> Option<String> {
    let base = base_specifier(specifier);
    if has_package_dependency(declared, base) {
        return Some(base.to_string());
    }
    // Rust hyphen normalization: my_crate → my-crate.
    let hyphenated = base.replace('_', "-");
    if hyphenated != base && has_package_dependency(declared, &hyphenated) {
        return Some(hyphenated);
    }
    // Java prefix: "org.springframework.web.bind" matches Maven group
    // "org.springframework.boot" when the specifier starts with the dep name.
    if specifier.contains('.') && !specifier.contains("::") && !specifier.contains('/') {
        for dep in &declared.names {
            if dep.contains('.') && specifier.starts_with(dep.as_str()) {
                return Some(dep.clone());
            }
        }
    }
    None
}

/// Resolve the DECLARED dependency an external-import unresolved reference maps to, in the
/// reader's terms — the name-returning form of the classifier's external-import matching
/// (ATTRIBUTION-1). Reuses the SAME identifier extraction + specifier reduction the
/// classifier used to assign the basis, so the name is the manifest dependency
/// (`repo-graph-indexer`, `serde`), never the raw import path (`repo_graph_indexer::types`)
/// or the call expression (`app.listen`). Returns `None` when no declared dependency can be
/// named (→ the honest "dependency not identified"), and for any non-external-import basis.
///
/// - [`UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency`]: read the specifier as
///   the classifier does and reduce it against `declared`.
/// - [`UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport`] /
///   [`UnresolvedEdgeBasisCode::CalleeMatchesExternalImport`]: extract the receiver/callee
///   identifier, find the import binding that introduced it, and reduce that binding's
///   specifier against `declared`. (The classifier matched `binding.specifier` directly; the
///   reduction here is a superset — it returns the same name for a bare specifier and also
///   handles a scoped one.)
pub fn resolve_external_dependency_name(
    edge: &ClassifierEdgeInput,
    category: UnresolvedEdgeCategory,
    basis_code: UnresolvedEdgeBasisCode,
    import_bindings: &[ImportBinding],
    declared: &PackageDependencySet,
) -> Option<String> {
    use UnresolvedEdgeBasisCode as B;
    match basis_code {
        B::SpecifierMatchesPackageDependency => {
            let (specifier, _is_relative) = parse_import_specifier(edge);
            resolve_declared_dependency(&specifier, declared)
        }
        B::ReceiverMatchesExternalImport | B::CalleeMatchesExternalImport => {
            let identifier = extract_target_identifier(edge, category)?;
            let binding = find_binding_for_identifier(&identifier, import_bindings)?;
            resolve_declared_dependency(&binding.specifier, declared)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ImportBinding, ImportKind, PackageDependencySet, RuntimeBuiltinsSet, TsconfigAliasEntry,
        TsconfigAliases,
    };

    fn empty_snapshot() -> SnapshotSignals {
        SnapshotSignals::empty()
    }

    fn empty_file() -> FileSignals {
        FileSignals::empty()
    }

    fn edge(target_key: &str) -> ClassifierEdgeInput {
        ClassifierEdgeInput {
            target_key: target_key.to_string(),
            metadata_json: None,
        }
    }

    fn edge_with_meta(target_key: &str, meta: &str) -> ClassifierEdgeInput {
        ClassifierEdgeInput {
            target_key: target_key.to_string(),
            metadata_json: Some(meta.to_string()),
        }
    }

    fn binding(identifier: &str, specifier: &str) -> ImportBinding {
        ImportBinding {
            identifier: identifier.into(),
            specifier: specifier.into(),
            is_relative: false,
            location: None,
            is_type_only: false,
            imported_name: None,
            kind: ImportKind::Named,
        }
    }

    fn deps(names: &[&str]) -> PackageDependencySet {
        PackageDependencySet {
            names: names.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ── Rule 1: this-receiver shortcut ────────────────────────

    #[test]
    fn this_method_is_internal() {
        let v = classify_unresolved_edge(
            &edge("this.doStuff"),
            UnresolvedEdgeCategory::CallsThisMethodNeedsClassContext,
            &empty_snapshot(),
            &empty_file(),
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::InternalCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal
        );
    }

    #[test]
    fn this_wildcard_method_is_internal() {
        let v = classify_unresolved_edge(
            &edge("this.x.doStuff"),
            UnresolvedEdgeCategory::CallsThisWildcardMethodNeedsTypeInfo,
            &empty_snapshot(),
            &empty_file(),
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::InternalCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::ThisReceiverImpliesInternal
        );
    }

    // ── IMPORTS_FILE_NOT_FOUND ────────────────────────────────

    #[test]
    fn ts_relative_import_is_internal() {
        let e = edge_with_meta("test-repo:src/types:FILE", r#"{"rawPath":"./types"}"#);
        let v = classify_unresolved_edge(
            &e,
            UnresolvedEdgeCategory::ImportsFileNotFound,
            &empty_snapshot(),
            &empty_file(),
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::InternalCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::RelativeImportTargetUnresolved
        );
    }

    #[test]
    fn rust_external_crate_import_is_external() {
        let e = edge_with_meta("serde", r#"{"specifier":"serde"}"#);
        let mut fs = empty_file();
        fs.package_dependencies = PackageDependencySet {
            names: vec!["serde".into()],
        };
        let v = classify_unresolved_edge(
            &e,
            UnresolvedEdgeCategory::ImportsFileNotFound,
            &empty_snapshot(),
            &fs,
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::ExternalLibraryCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency
        );
    }

    #[test]
    fn rust_hyphenated_cargo_dep_is_external() {
        // my_crate in use path matches my-crate in Cargo.toml
        let e = edge_with_meta("my_crate", r#"{"specifier":"my_crate"}"#);
        let mut fs = empty_file();
        fs.package_dependencies = PackageDependencySet {
            names: vec!["my-crate".into()],
        };
        let v = classify_unresolved_edge(
            &e,
            UnresolvedEdgeCategory::ImportsFileNotFound,
            &empty_snapshot(),
            &fs,
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::ExternalLibraryCandidate
        );
    }

    #[test]
    fn rust_crate_internal_module_is_internal() {
        let e = edge_with_meta("renderer", r#"{"specifier":"renderer"}"#);
        let v = classify_unresolved_edge(
            &e,
            UnresolvedEdgeCategory::ImportsFileNotFound,
            &empty_snapshot(),
            &empty_file(),
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::InternalCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::RustCrateInternalModuleHeuristic
        );
    }

    #[test]
    fn rust_crate_module_import_is_internal() {
        let e = edge_with_meta("crate::foo", r#"{"specifier":"crate::foo"}"#);
        let v = classify_unresolved_edge(
            &e,
            UnresolvedEdgeCategory::ImportsFileNotFound,
            &empty_snapshot(),
            &empty_file(),
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::InternalCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::RelativeImportTargetUnresolved
        );
    }

    #[test]
    fn node_stdlib_import_is_external() {
        let e = edge("path");
        let ss = SnapshotSignals {
            runtime_builtins: RuntimeBuiltinsSet {
                identifiers: vec![],
                module_specifiers: vec!["path".into(), "fs".into()],
            },
        };
        let v = classify_unresolved_edge(
            &e,
            UnresolvedEdgeCategory::ImportsFileNotFound,
            &ss,
            &empty_file(),
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::ExternalLibraryCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::SpecifierMatchesRuntimeModule
        );
    }

    #[test]
    fn rust_std_import_is_external() {
        let e = edge_with_meta("std::collections", r#"{"specifier":"std::collections"}"#);
        let ss = SnapshotSignals {
            runtime_builtins: RuntimeBuiltinsSet {
                identifiers: vec![],
                module_specifiers: vec!["std".into()],
            },
        };
        let v = classify_unresolved_edge(
            &e,
            UnresolvedEdgeCategory::ImportsFileNotFound,
            &ss,
            &empty_file(),
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::ExternalLibraryCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::SpecifierMatchesRuntimeModule
        );
    }

    // ── OTHER category ────────────────────────────────────────

    #[test]
    fn other_category_is_unknown() {
        let v = classify_unresolved_edge(
            &edge("anything"),
            UnresolvedEdgeCategory::Other,
            &empty_snapshot(),
            &empty_file(),
        );
        assert_eq!(v.classification, UnresolvedEdgeClassification::Unknown);
    }

    // ── Rule 2: same-file symbol match ────────────────────────

    #[test]
    fn callee_matches_same_file_value_symbol() {
        let mut fs = empty_file();
        fs.same_file_value_symbols = vec!["myFunc".into()];
        let v = classify_unresolved_edge(
            &edge("myFunc"),
            UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
            &empty_snapshot(),
            &fs,
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::InternalCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::CalleeMatchesSameFileSymbol
        );
    }

    // ── Rule 3–5: import binding dispatch ─────────────────────

    #[test]
    fn external_import_binding_is_external() {
        let mut fs = empty_file();
        fs.import_bindings = vec![ImportBinding {
            identifier: "lodash".into(),
            specifier: "lodash".into(),
            is_relative: false,
            location: None,
            is_type_only: false,
            imported_name: None,
            kind: ImportKind::Named,
        }];
        fs.package_dependencies = PackageDependencySet {
            names: vec!["lodash".into()],
        };
        let v = classify_unresolved_edge(
            &edge("lodash"),
            UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
            &empty_snapshot(),
            &fs,
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::ExternalLibraryCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::CalleeMatchesExternalImport
        );
    }

    #[test]
    fn relative_import_binding_is_internal() {
        let mut fs = empty_file();
        fs.import_bindings = vec![ImportBinding {
            identifier: "helper".into(),
            specifier: "./utils".into(),
            is_relative: true,
            location: None,
            is_type_only: false,
            imported_name: None,
            kind: ImportKind::Named,
        }];
        let v = classify_unresolved_edge(
            &edge("helper"),
            UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
            &empty_snapshot(),
            &fs,
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::InternalCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::CalleeMatchesInternalImport
        );
    }

    #[test]
    fn alias_import_binding_is_internal() {
        let mut fs = empty_file();
        fs.import_bindings = vec![ImportBinding {
            identifier: "helper".into(),
            specifier: "@/utils".into(),
            is_relative: false,
            location: None,
            is_type_only: false,
            imported_name: None,
            kind: ImportKind::Named,
        }];
        fs.tsconfig_aliases = TsconfigAliases {
            entries: vec![TsconfigAliasEntry {
                pattern: "@/*".into(),
                substitutions: vec!["./src/*".into()],
            }],
        };
        let v = classify_unresolved_edge(
            &edge("helper"),
            UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
            &empty_snapshot(),
            &fs,
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::InternalCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::SpecifierMatchesProjectAlias
        );
    }

    // ── Rule 6: runtime global ────────────────────────────────

    #[test]
    fn runtime_global_identifier_is_external() {
        let ss = SnapshotSignals {
            runtime_builtins: RuntimeBuiltinsSet {
                identifiers: vec!["Map".into(), "Date".into()],
                module_specifiers: vec![],
            },
        };
        let v = classify_unresolved_edge(
            &edge("Map"),
            UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
            &ss,
            &empty_file(),
        );
        assert_eq!(
            v.classification,
            UnresolvedEdgeClassification::ExternalLibraryCandidate
        );
        assert_eq!(
            v.basis_code,
            UnresolvedEdgeBasisCode::CalleeMatchesRuntimeGlobal
        );
    }

    // ── Rule 7: no signal → unknown ──────────────────────────

    #[test]
    fn no_signal_is_unknown() {
        let v = classify_unresolved_edge(
            &edge("mysteryFunc"),
            UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
            &empty_snapshot(),
            &empty_file(),
        );
        assert_eq!(v.classification, UnresolvedEdgeClassification::Unknown);
        assert_eq!(v.basis_code, UnresolvedEdgeBasisCode::NoSupportingSignal);
    }

    // ── Helpers ───────────────────────────────────────────────

    #[test]
    fn is_simple_identifier_accepts_valid_js_identifiers() {
        assert!(is_simple_identifier("foo"));
        assert!(is_simple_identifier("Foo"));
        assert!(is_simple_identifier("_foo"));
        assert!(is_simple_identifier("$foo"));
        assert!(is_simple_identifier("foo123"));
    }

    #[test]
    fn is_simple_identifier_rejects_non_identifiers() {
        assert!(!is_simple_identifier(""));
        assert!(!is_simple_identifier("123foo"));
        assert!(!is_simple_identifier("foo.bar"));
        assert!(!is_simple_identifier("foo bar"));
        assert!(!is_simple_identifier("foo("));
    }

    #[test]
    fn is_rust_module_name_accepts_valid_rust_modules() {
        assert!(is_rust_module_name("renderer"));
        assert!(is_rust_module_name("my_module"));
        assert!(is_rust_module_name("a"));
        assert!(is_rust_module_name("mod123"));
    }

    #[test]
    fn is_rust_module_name_rejects_non_rust_modules() {
        assert!(!is_rust_module_name(""));
        assert!(!is_rust_module_name("MyModule")); // uppercase
        assert!(!is_rust_module_name("1mod")); // starts with digit
        assert!(!is_rust_module_name("my-module")); // hyphen
    }

    // ── Named-dependency resolution (ATTRIBUTION-1) ───────────

    #[test]
    fn resolve_names_scoped_rust_specifier_as_declared_hyphenated_dependency() {
        // review-2 defect: `repo_graph_indexer::types` is an import PATH; the DECLARED dep is
        // `repo-graph-indexer`. The reduction bases + hyphen-normalizes to the manifest name.
        let d = deps(&["repo-graph-indexer", "serde"]);
        let e = edge_with_meta(
            "repo_graph_indexer::types",
            r#"{"specifier":"repo_graph_indexer::types"}"#,
        );
        assert_eq!(
            resolve_external_dependency_name(
                &e,
                UnresolvedEdgeCategory::ImportsFileNotFound,
                UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency,
                &[],
                &d,
            ),
            Some("repo-graph-indexer".to_string()),
            "scoped specifier must name the DECLARED (hyphenated) dependency, not the import path"
        );
        // Bare specifier: serde → serde.
        let e2 = edge_with_meta("serde", r#"{"specifier":"serde"}"#);
        assert_eq!(
            resolve_external_dependency_name(
                &e2,
                UnresolvedEdgeCategory::ImportsFileNotFound,
                UnresolvedEdgeBasisCode::SpecifierMatchesPackageDependency,
                &[],
                &d,
            ),
            Some("serde".to_string())
        );
    }

    #[test]
    fn resolve_names_receiver_and_callee_external_calls_via_import_binding() {
        // The two CALL bases the classifier resolves through import bindings — named via the
        // binding's specifier, NOT the call expression `app.listen` / bare callee `useState`.
        let d = deps(&["express", "react"]);
        let bindings = vec![binding("app", "express"), binding("useState", "react")];
        assert_eq!(
            resolve_external_dependency_name(
                &edge("app.listen"),
                UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
                UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport,
                &bindings,
                &d,
            ),
            Some("express".to_string())
        );
        assert_eq!(
            resolve_external_dependency_name(
                &edge("useState"),
                UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
                UnresolvedEdgeBasisCode::CalleeMatchesExternalImport,
                &bindings,
                &d,
            ),
            Some("react".to_string())
        );
    }

    #[test]
    fn resolve_returns_none_when_unnameable_or_non_external_basis() {
        let d = deps(&["serde"]);
        // Receiver whose binding specifier is not a declared dep → unnameable (→ "not identified").
        assert_eq!(
            resolve_external_dependency_name(
                &edge("app.listen"),
                UnresolvedEdgeCategory::CallsObjMethodNeedsTypeInfo,
                UnresolvedEdgeBasisCode::ReceiverMatchesExternalImport,
                &[binding("app", "koa")],
                &d,
            ),
            None
        );
        // Callee with no import binding at all → None.
        assert_eq!(
            resolve_external_dependency_name(
                &edge("mystery"),
                UnresolvedEdgeCategory::CallsFunctionAmbiguousOrMissing,
                UnresolvedEdgeBasisCode::CalleeMatchesExternalImport,
                &[],
                &d,
            ),
            None
        );
        // A non-external-import basis is never named (own-code / stdlib / dynamic dispatch).
        assert_eq!(
            resolve_external_dependency_name(
                &edge("serde"),
                UnresolvedEdgeCategory::ImportsFileNotFound,
                UnresolvedEdgeBasisCode::RelativeImportTargetUnresolved,
                &[],
                &d,
            ),
            None
        );
    }

    #[test]
    fn resolve_declared_dependency_matches_classifier_reduction() {
        // The reduction reused by classify_unresolved_import: base + hyphen + Java prefix.
        let d = deps(&["repo-graph-indexer", "serde", "org.springframework.boot"]);
        assert_eq!(
            resolve_declared_dependency("serde::de", &d),
            Some("serde".to_string())
        );
        assert_eq!(
            resolve_declared_dependency("repo_graph_indexer", &d),
            Some("repo-graph-indexer".to_string())
        );
        assert_eq!(
            resolve_declared_dependency("org.springframework.boot.autoconfigure", &d),
            Some("org.springframework.boot".to_string()),
            "Java group-prefix match returns the declared group"
        );
        assert_eq!(resolve_declared_dependency("tokio", &d), None);
    }
}
