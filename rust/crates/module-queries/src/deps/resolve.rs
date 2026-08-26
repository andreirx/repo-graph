//! Import specifier resolution for DEP-1.
//!
//! Resolves callee identifiers (from unresolved_edges.target_key) to
//! their actual import specifiers using import binding data.

use std::collections::{HashMap, HashSet};

use repo_graph_storage::crud::module_edges_support::ImportBindingFact;

/// Build an identifier→specifier resolution map from import bindings.
///
/// Returns a map keyed by (file_uid, identifier) with the import specifier as value.
pub fn build_identifier_resolution_map(
    bindings: &[ImportBindingFact],
) -> HashMap<(&str, &str), &str> {
    let mut map = HashMap::new();
    for binding in bindings {
        map.insert(
            (binding.file_uid.as_str(), binding.identifier.as_str()),
            binding.specifier.as_str(),
        );
    }
    map
}

/// Build a per-file set of import-declaration SPECIFIERS (the exact strings a source file imports:
/// `org.springframework.boot.SpringApplication`, `asgiref.sync`, `react`, `tokio`).
///
/// Used by the deps assembly (§2.1 review-3 item 1) as the deterministic import-declaration fact
/// that admits a dotted value which is genuinely imported while rejecting a like-shaped method
/// chain (`file.toString`) that no import declares. Keyed by `file_uid`; the specifier is the value
/// side of an [`ImportBindingFact`] (relative imports are already filtered out by the storage read).
pub fn build_file_specifier_sets(bindings: &[ImportBindingFact]) -> HashMap<&str, HashSet<&str>> {
    let mut map: HashMap<&str, HashSet<&str>> = HashMap::new();
    for binding in bindings {
        map.entry(binding.file_uid.as_str())
            .or_default()
            .insert(binding.specifier.as_str());
    }
    map
}

/// Resolve an external import target_key to its import specifier.
///
/// The unresolved_edges table stores callee identifiers (e.g., "useState") or
/// member access patterns (e.g., "React.createElement") as target_key. This
/// function resolves them to the actual import specifier (e.g., "react").
///
/// Resolution strategy:
/// 1. Direct lookup: target_key matches an import binding identifier
/// 2. Member access: for "Foo.bar", try resolving "Foo" as the receiver
/// 3. Fallback: return the original target_key (may be a direct package call like "express()")
pub fn resolve_import_specifier<'a>(
    target_key: &'a str,
    file_uid: &str,
    identifier_to_specifier: &HashMap<(&str, &str), &'a str>,
) -> String {
    // Strategy 1: Direct lookup — the target_key itself is an import binding.
    // This handles cases like `express()` where the identifier is "express".
    if let Some(&specifier) = identifier_to_specifier.get(&(file_uid, target_key)) {
        return specifier.to_string();
    }

    // Strategy 2: Member access pattern — "Foo.bar" where "Foo" is the import binding.
    // This handles cases like `React.createElement()` where the receiver is "React".
    if let Some(dot_pos) = target_key.find('.') {
        let receiver = &target_key[..dot_pos];
        if let Some(&specifier) = identifier_to_specifier.get(&(file_uid, receiver)) {
            return specifier.to_string();
        }
    }

    // Fallback: no binding found. Return the original target_key.
    // This may be a direct package name (e.g., "lodash") or an unresolved identifier.
    target_key.to_string()
}

/// Whether `target_key` is backed by an import binding in `file_uid` (DEPS-LIST-REWRITE-1 §2.1):
/// the identifier itself is a bound import, or its member-access receiver (`Foo` in `Foo.bar`) is.
/// Mirrors [`resolve_import_specifier`]'s strategies 1 and 2. Used by the deps assembly to keep the
/// observed bucket to actual import declarations, dropping unbound call targets (`chr`, `class`).
pub fn is_bound(
    target_key: &str,
    file_uid: &str,
    identifier_to_specifier: &HashMap<(&str, &str), &str>,
) -> bool {
    if identifier_to_specifier.contains_key(&(file_uid, target_key)) {
        return true;
    }
    if let Some(dot_pos) = target_key.find('.') {
        let receiver = &target_key[..dot_pos];
        if identifier_to_specifier.contains_key(&(file_uid, receiver)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_binding(file_uid: &str, identifier: &str, specifier: &str) -> ImportBindingFact {
        ImportBindingFact {
            file_uid: file_uid.to_string(),
            identifier: identifier.to_string(),
            specifier: specifier.to_string(),
            is_relative: false,
        }
    }

    #[test]
    fn direct_lookup_resolves_identifier() {
        let bindings = vec![make_binding("file1", "useState", "react")];
        let map = build_identifier_resolution_map(&bindings);

        let result = resolve_import_specifier("useState", "file1", &map);
        assert_eq!(result, "react");
    }

    #[test]
    fn member_access_resolves_receiver() {
        let bindings = vec![make_binding("file1", "React", "react")];
        let map = build_identifier_resolution_map(&bindings);

        let result = resolve_import_specifier("React.createElement", "file1", &map);
        assert_eq!(result, "react");
    }

    #[test]
    fn fallback_returns_original() {
        let bindings = vec![];
        let map = build_identifier_resolution_map(&bindings);

        let result = resolve_import_specifier("express", "file1", &map);
        assert_eq!(result, "express");
    }

    #[test]
    fn different_files_resolve_independently() {
        let bindings = vec![
            make_binding("file1", "useState", "react"),
            make_binding("file2", "useState", "preact"),
        ];
        let map = build_identifier_resolution_map(&bindings);

        assert_eq!(resolve_import_specifier("useState", "file1", &map), "react");
        assert_eq!(
            resolve_import_specifier("useState", "file2", &map),
            "preact"
        );
    }

    #[test]
    fn nested_member_access_uses_first_segment() {
        let bindings = vec![make_binding("file1", "React", "react")];
        let map = build_identifier_resolution_map(&bindings);

        let result = resolve_import_specifier("React.DOM.div", "file1", &map);
        assert_eq!(result, "react");
    }

    #[test]
    fn file_specifier_sets_group_specifiers_by_file() {
        // The import-declaration evidence set the §2.1 gate consults: exact imported specifiers,
        // per file. `file.toString` (never imported) is absent → the gate rejects it.
        let bindings = vec![
            make_binding(
                "f1",
                "SpringApplication",
                "org.springframework.boot.SpringApplication",
            ),
            make_binding("f1", "sync", "asgiref.sync"),
            make_binding("f2", "react", "react"),
        ];
        let sets = build_file_specifier_sets(&bindings);
        assert!(sets["f1"].contains("org.springframework.boot.SpringApplication"));
        assert!(sets["f1"].contains("asgiref.sync"));
        assert!(!sets["f1"].contains("file.toString"));
        assert!(sets["f2"].contains("react"));
        assert!(!sets["f2"].contains("asgiref.sync"));
    }

    #[test]
    fn is_bound_matches_identifier_and_receiver_only() {
        let bindings = vec![make_binding("f", "express", "express")];
        let map = build_identifier_resolution_map(&bindings);
        // Bound import identifier and its member-access receiver are bound.
        assert!(is_bound("express", "f", &map));
        assert!(is_bound("express.Router", "f", &map));
        // A bare unbound call target (a builtin/keyword misclassified as external) is NOT bound.
        assert!(!is_bound("chr", "f", &map));
        assert!(!is_bound("class", "f", &map));
        // Different file → not bound.
        assert!(!is_bound("express", "other", &map));
    }
}
