//! The single Cargo package-name canonicalisation (IMPORT-RESOLUTION-RUST-1 §2.3).
//!
//! Cargo treats `-` and `_` as EQUIVALENT in a package name: a crate declared
//! `repo-graph-storage` is imported in Rust source as `repo_graph_storage`, and a
//! dependency key may be spelled with either separator. To compare two parsed-manifest
//! facts (an import specifier's first segment vs a declared package name) they must be
//! reduced to one canonical spelling first. This function is that canonicalisation:
//! `_` → `-` (the crates.io display form).
//!
//! # Why one function
//!
//! This exact `_`→`-` reduction previously existed as three private copies —
//! `storage::trust_impl::canonicalize_cargo_name`,
//! `module_queries::deps::reconcile` (the `"cargo"` arm), and
//! `unresolved_classifier::resolve_declared_dependency` — plus the new indexer
//! resolver Rust-crate import stage needs it. `classification` is the one crate all
//! four already depend on (indexer → classification, storage → classification,
//! module-queries → classification), so hosting it here removes the duplication
//! WITHOUT adding a dependency edge (the component graph stays a DAG).
//!
//! # What this is NOT
//!
//! `repo_index::config::extract_cargo_dependencies` performs the OPPOSITE reduction
//! (`-`→`_`) to build a `PackageDependencySet` whose underscore form is what
//! `has_package_dependency` compares against. That is a DIFFERENT canonical
//! representative with a DIFFERENT contract; it is deliberately NOT folded into this
//! function — inverting its direction would change the dependency-set form globally
//! and shift classification/trust counts. Only the `_`→`-` family lives here.

/// Canonicalise a Cargo package name to its `-`-separated form so that `-`/`_`
/// spellings of the SAME package compare equal.
///
/// Cargo-ONLY: npm / pyproject / Gradle package names are literal (there `foo_bar`
/// and `foo-bar` are distinct packages), so callers apply this only to cargo-evidenced
/// names.
pub fn canonicalize_cargo_package_name(name: &str) -> String {
    name.replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underscores_become_hyphens() {
        assert_eq!(
            canonicalize_cargo_package_name("repo_graph_storage"),
            "repo-graph-storage"
        );
    }

    #[test]
    fn hyphen_form_is_idempotent() {
        assert_eq!(
            canonicalize_cargo_package_name("repo-graph-storage"),
            "repo-graph-storage"
        );
    }

    #[test]
    fn single_segment_unchanged() {
        assert_eq!(canonicalize_cargo_package_name("b"), "b");
    }
}
