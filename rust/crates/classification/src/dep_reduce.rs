//! The single specifier → package-head reduction (DEPS-CLASSIFIER-1 §2.1).
//!
//! A declared dependency is "used" when ANY of its modules is imported. To decide that, an
//! import specifier must be reduced to the DECLARED package head before it is matched against a
//! manifest name: `asgiref.sync` → `asgiref` (Python), `storybook/test` → `storybook` and
//! `@scope/pkg/sub` → `@scope/pkg` (npm). Before this module those reductions existed ONLY at
//! query time (`module-queries::deps::normalize`), which the INDEX-time classifier
//! (`unresolved_classifier::resolve_declared_dependency`) never reached — so every
//! `asgiref.sync` edge classified `unknown` and `deps list` printed the false
//! `no static import: asgiref` (root cause `docs/audits/2026-09-06-root-causes-v0.17.0.md` §B).
//!
//! # Why here, and why one copy
//!
//! Abstraction one-liner — WHAT: the per-ecosystem specifier→package-head reduction primitives.
//! CURRENT USERS (both concrete, today): the index-time classifier
//! (`unresolved_classifier::resolve_declared_dependency`) AND the query-time normalizers
//! (`module_queries::deps::normalize::{normalize_npm_specifier, normalize_python_specifier}`, which
//! now delegate here). AXIS OF VARIATION: the ecosystem's package-name grammar (npm scope/subpath;
//! Python dotted module + PEP 503 folding) — a demonstrated, not imagined, divergence: the two
//! copies already diverged and produced the defect. SIMPLER ALTERNATIVE REJECTED: leaving the
//! reduction only in `normalize.rs` (the status quo) — the classifier cannot import `module-queries`
//! (that is the reverse of the existing DAG edge `module-queries → classification`), so a second copy
//! in `classification` would be the very divergence this slice removes. `classification` is the crate
//! `module-queries` already depends on, exactly as `cargo_name` is hosted here for the same reason,
//! so this adds NO dependency edge (the component graph stays a DAG).
//!
//! Rust `a::b` → `a` and Java package-segment matching are NOT here: Rust reduction is the trivial
//! `split("::")` already shared via `base_specifier`, and Java matching is a declared-set search
//! (longest dotted group), a different shape from head reduction. Only the two reductions the
//! classifier LACKED (npm subpath, Python head) are lifted, per the "smallest earned" rule.

/// Reduce an npm/JS/TS import specifier to its package name.
///
/// Rules (identical to the query-time normalizer this replaced):
/// 1. Scoped packages (`@scope/name`) keep scope + name; a subpath after them is dropped.
/// 2. Unscoped subpath imports (`pkg/subpath`) reduce to `pkg`.
/// 3. Plain specifiers stay as-is.
pub fn npm_package_head(specifier: &str) -> String {
    if let Some(rest) = specifier.strip_prefix('@') {
        // Scoped: @scope/name or @scope/name/subpath — keep through the SECOND slash.
        // `rest` is `scope/name[/subpath]`; the package is `@scope/name`.
        let mut slash = 0;
        let mut boundary = specifier.len();
        for (i, c) in specifier.char_indices() {
            if c == '/' {
                slash += 1;
                if slash == 2 {
                    boundary = i;
                    break;
                }
            }
        }
        let _ = rest; // `rest` only proves the leading `@`; boundary is computed over `specifier`.
        specifier[..boundary].to_string()
    } else {
        match specifier.find('/') {
            Some(idx) => specifier[..idx].to_string(),
            None => specifier.to_string(),
        }
    }
}

/// Reduce a Python import specifier to its top-level import module name — the first dotted
/// segment, raw case (`asgiref.sync` → `asgiref`, `Django` → `Django`). Case folding is NOT applied
/// here; callers that need distribution-name equivalence apply [`pep503_normalize`] (Python's own
/// package-name equality rule) on top. Keeping case here lets the query-time normalizer reproduce
/// its exact lowercased output by composing the two, without this primitive baking in a policy.
pub fn python_import_head(specifier: &str) -> &str {
    specifier.split('.').next().unwrap_or(specifier)
}

/// PEP 503 name normalization: lowercase, then collapse every run of `-`, `_`, `.` into a single
/// `-`. This is Python's canonical package-name equality (`re.sub(r"[-_.]+", "-", name).lower()`),
/// so the distribution name `Django-Extensions` and the import head `django_extensions` reduce to
/// the same `django-extensions`. Python-ONLY: npm and cargo names are literal in `.`/`_` (there
/// `foo_bar` and `foo-bar` are distinct packages), so this must not be applied to them.
pub fn pep503_normalize(name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_plain_and_subpath() {
        assert_eq!(npm_package_head("react"), "react");
        assert_eq!(npm_package_head("react/jsx-runtime"), "react");
        assert_eq!(npm_package_head("lodash/fp/map"), "lodash");
    }

    #[test]
    fn npm_scoped_and_scoped_subpath() {
        assert_eq!(
            npm_package_head("@tanstack/react-query"),
            "@tanstack/react-query"
        );
        assert_eq!(
            npm_package_head("@tanstack/react-query/devtools"),
            "@tanstack/react-query"
        );
        assert_eq!(npm_package_head("@scope/pkg/sub"), "@scope/pkg");
    }

    #[test]
    fn python_head_takes_first_segment_preserving_case() {
        assert_eq!(python_import_head("asgiref.sync"), "asgiref");
        assert_eq!(python_import_head("asgiref.local"), "asgiref");
        assert_eq!(python_import_head("Django"), "Django");
        assert_eq!(python_import_head("sqlparse"), "sqlparse");
    }

    #[test]
    fn pep503_folds_case_and_separator_runs() {
        assert_eq!(pep503_normalize("Django-Extensions"), "django-extensions");
        assert_eq!(pep503_normalize("django_extensions"), "django-extensions");
        assert_eq!(pep503_normalize("Zope.Interface"), "zope-interface");
        assert_eq!(pep503_normalize("a__b--c..d"), "a-b-c-d");
        assert_eq!(pep503_normalize("asgiref"), "asgiref");
    }
}
