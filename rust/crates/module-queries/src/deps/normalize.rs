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
    if specifier.starts_with("crate::")
        || specifier.starts_with("self::")
        || specifier.starts_with("super::")
    {
        return specifier.to_string();
    }

    match specifier.find("::") {
        Some(idx) => specifier[..idx].to_string(),
        None => specifier.to_string(),
    }
}

/// Normalize a Python import specifier to its top-level distribution-ish module name.
///
/// Python imports are dotted (`asgiref.sync`, `os.path`); the package identity is the first
/// segment (`asgiref`, `os`). Lower-cased so it lines up with `pyproject` distribution names,
/// which the reader also lower-cases. This is best-effort: PyPI distribution names and import
/// module names diverge for some packages (`beautifulsoup4` → `bs4`) — an inherent Python limit,
/// not fixable at this layer.
///
/// `pub(crate)`: the sole caller is `classify.rs` (crate-internal). Unlike the npm/cargo
/// normalizers (published on the crate's public API with doctests), the python/java normalizers
/// added by DEPS-LIST-REWRITE-1 have no external consumer, so they stay crate-private.
pub(crate) fn normalize_python_specifier(specifier: &str) -> String {
    let head = specifier.split('.').next().unwrap_or(specifier);
    head.to_ascii_lowercase()
}

/// Normalize a Java import specifier (a fully-qualified name) to its package path.
///
/// A Java import is `pkg.sub.ClassName` (optionally `.member`). Drop the trailing type/member
/// segments (those whose first char is uppercase — the class and any static member) to recover the
/// package (`org.springframework.boot.SpringApplication` → `org.springframework.boot`), which is
/// what Gradle declares as a dependency group id. Keeps `SpringApplication` from being counted as a
/// package of its own (the petclinic double-count the audit named). A wildcard tail (`.*`) is
/// dropped too.
///
/// `pub(crate)`: sole caller is `classify.rs` (crate-internal), same as `normalize_python_specifier`.
pub(crate) fn normalize_java_specifier(specifier: &str) -> String {
    let mut segments: Vec<&str> = specifier.split('.').collect();
    while let Some(last) = segments.last() {
        let is_type_or_member = last == &"*"
            || last
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false);
        if is_type_or_member && segments.len() > 1 {
            segments.pop();
        } else {
            break;
        }
    }
    segments.join(".")
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
        assert_eq!(
            normalize_npm_specifier("@tanstack/react-query"),
            "@tanstack/react-query"
        );
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
        assert_eq!(normalize_cargo_specifier("tokio::sync::Mutex"), "tokio");
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

    // ── Python normalization ──────────────────────────────────────

    #[test]
    fn python_top_segment() {
        assert_eq!(normalize_python_specifier("asgiref.sync"), "asgiref");
        assert_eq!(normalize_python_specifier("os.path"), "os");
        assert_eq!(normalize_python_specifier("Django"), "django");
        assert_eq!(normalize_python_specifier("sqlparse"), "sqlparse");
    }

    // ── Java normalization ────────────────────────────────────────

    #[test]
    fn java_drops_type_and_member_tail() {
        assert_eq!(
            normalize_java_specifier("org.springframework.boot.SpringApplication"),
            "org.springframework.boot"
        );
        assert_eq!(normalize_java_specifier("java.util.List"), "java.util");
        assert_eq!(
            normalize_java_specifier("com.example.pkg"),
            "com.example.pkg"
        );
        assert_eq!(
            normalize_java_specifier("com.google.common.collect.ImmutableList"),
            "com.google.common.collect"
        );
        assert_eq!(normalize_java_specifier("java.util.*"), "java.util");
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
