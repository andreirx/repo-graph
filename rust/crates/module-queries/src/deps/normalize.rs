//! Specifier → package normalization.
//!
//! Normalizes import specifiers to their root package names:
//!
//! **npm/TS/JS:**
//! - `react` → `react`
//! - `react/jsx-runtime` → `react`
//! - `@tanstack/react-query` → `@tanstack/react-query`
//! - `@tanstack/react-query/devtools` → `@tanstack/react-query`
//! - `lodash/get` → `lodash`
//!
//! **Rust/Cargo:**
//! - `tokio::spawn` → `tokio`
//! - `reqwest::Client` → `reqwest`
//! - `serde_json::Value` → `serde_json`

/// Normalize an npm/JS/TS import specifier to its package name.
///
/// Rules:
/// 1. Scoped packages (`@scope/name`) keep the full scope+name
/// 2. Subpath imports (`pkg/subpath`) reduce to the package name
/// 3. Plain specifiers stay as-is
///
/// # Examples
///
/// ```
/// use repo_graph_module_queries::deps::normalize_npm_specifier;
///
/// assert_eq!(normalize_npm_specifier("react"), "react");
/// assert_eq!(normalize_npm_specifier("react/jsx-runtime"), "react");
/// assert_eq!(normalize_npm_specifier("@tanstack/react-query"), "@tanstack/react-query");
/// assert_eq!(normalize_npm_specifier("@tanstack/react-query/devtools"), "@tanstack/react-query");
/// assert_eq!(normalize_npm_specifier("lodash/get"), "lodash");
/// ```
pub fn normalize_npm_specifier(specifier: &str) -> String {
    if specifier.starts_with('@') {
        // Scoped package: @scope/name or @scope/name/subpath
        // Find the second slash (after @scope/name)
        let mut slash_count = 0;
        let mut boundary = specifier.len();
        for (i, c) in specifier.char_indices() {
            if c == '/' {
                slash_count += 1;
                if slash_count == 2 {
                    boundary = i;
                    break;
                }
            }
        }
        specifier[..boundary].to_string()
    } else {
        // Unscoped package: name or name/subpath
        match specifier.find('/') {
            Some(idx) => specifier[..idx].to_string(),
            None => specifier.to_string(),
        }
    }
}

/// Normalize a Rust/Cargo use path to its crate name.
///
/// Rules:
/// 1. Take everything before the first `::`
/// 2. Handle `crate::` and `self::` as special cases (return as-is)
///
/// # Examples
///
/// ```
/// use repo_graph_module_queries::deps::normalize_cargo_specifier;
///
/// assert_eq!(normalize_cargo_specifier("tokio::spawn"), "tokio");
/// assert_eq!(normalize_cargo_specifier("reqwest::Client"), "reqwest");
/// assert_eq!(normalize_cargo_specifier("serde_json::Value"), "serde_json");
/// assert_eq!(normalize_cargo_specifier("tokio"), "tokio");
/// ```
pub fn normalize_cargo_specifier(specifier: &str) -> String {
    // Special cases for relative paths
    if specifier.starts_with("crate::") || specifier.starts_with("self::") || specifier.starts_with("super::") {
        return specifier.to_string();
    }

    match specifier.find("::") {
        Some(idx) => specifier[..idx].to_string(),
        None => specifier.to_string(),
    }
}

/// Determine if a specifier looks like a relative/local import.
///
/// Returns `true` for:
/// - Paths starting with `.` (relative imports)
/// - Paths starting with `/` (absolute file paths)
/// - Rust crate-relative paths (`crate::`, `self::`, `super::`)
pub fn is_local_specifier(specifier: &str) -> bool {
    specifier.starts_with('.')
        || specifier.starts_with('/')
        || specifier.starts_with("crate::")
        || specifier.starts_with("self::")
        || specifier.starts_with("super::")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── npm normalization ─────────────────────────────────────────

    #[test]
    fn npm_plain_package() {
        assert_eq!(normalize_npm_specifier("react"), "react");
        assert_eq!(normalize_npm_specifier("lodash"), "lodash");
        assert_eq!(normalize_npm_specifier("express"), "express");
    }

    #[test]
    fn npm_subpath_import() {
        assert_eq!(normalize_npm_specifier("react/jsx-runtime"), "react");
        assert_eq!(normalize_npm_specifier("lodash/get"), "lodash");
        assert_eq!(normalize_npm_specifier("lodash/fp/map"), "lodash");
    }

    #[test]
    fn npm_scoped_package() {
        assert_eq!(normalize_npm_specifier("@tanstack/react-query"), "@tanstack/react-query");
        assert_eq!(normalize_npm_specifier("@types/node"), "@types/node");
        assert_eq!(normalize_npm_specifier("@babel/core"), "@babel/core");
    }

    #[test]
    fn npm_scoped_subpath() {
        assert_eq!(
            normalize_npm_specifier("@tanstack/react-query/devtools"),
            "@tanstack/react-query"
        );
        assert_eq!(
            normalize_npm_specifier("@babel/core/lib/transform"),
            "@babel/core"
        );
    }

    // ── Cargo normalization ───────────────────────────────────────

    #[test]
    fn cargo_crate_path() {
        assert_eq!(normalize_cargo_specifier("tokio::spawn"), "tokio");
        assert_eq!(normalize_cargo_specifier("reqwest::Client"), "reqwest");
        assert_eq!(normalize_cargo_specifier("serde_json::Value"), "serde_json");
    }

    #[test]
    fn cargo_plain_crate() {
        assert_eq!(normalize_cargo_specifier("tokio"), "tokio");
        assert_eq!(normalize_cargo_specifier("serde"), "serde");
    }

    #[test]
    fn cargo_nested_path() {
        assert_eq!(
            normalize_cargo_specifier("tokio::sync::Mutex"),
            "tokio"
        );
        assert_eq!(
            normalize_cargo_specifier("std::collections::HashMap"),
            "std"
        );
    }

    #[test]
    fn cargo_relative_paths_unchanged() {
        assert_eq!(normalize_cargo_specifier("crate::utils"), "crate::utils");
        assert_eq!(normalize_cargo_specifier("self::helper"), "self::helper");
        assert_eq!(normalize_cargo_specifier("super::parent"), "super::parent");
    }

    // ── Local detection ───────────────────────────────────────────

    #[test]
    fn local_specifier_detection() {
        assert!(is_local_specifier("./utils"));
        assert!(is_local_specifier("../parent"));
        assert!(is_local_specifier("/absolute/path"));
        assert!(is_local_specifier("crate::module"));
        assert!(is_local_specifier("self::sibling"));
        assert!(is_local_specifier("super::parent"));

        assert!(!is_local_specifier("react"));
        assert!(!is_local_specifier("@tanstack/react-query"));
        assert!(!is_local_specifier("tokio::spawn"));
    }
}
