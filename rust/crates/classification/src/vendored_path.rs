//! The ONE vendored-path predicate (RG-REQ-002-L02, RG-REQ-001-L08).
//!
//! A path is "vendored" when one of its whole `/`-delimited components (case-
//! insensitively) is a known third-party/dependency directory. This is the single
//! definition in the tree: the agent complexity aggregator, `hotspots
//! --exclude-vendored`, the docs vendored line, and the orient additive fields all
//! read it, so no two surfaces can disagree about what "vendored" means.
//!
//! It lives in this inner leaf crate (deps: serde + small helpers) so the outer
//! `agent` crate — which cannot import the `daemon-runtime` adapter where the
//! predicate used to live — can call the SAME function. The body moved verbatim
//! from `daemon-runtime/src/handlers/quality/support.rs`; the segment list gained
//! `dependencies` and `contrib` (COMPLEXITY-SCOPE-1 §2.1.2 — poco's `dependencies/`
//! and `contrib/` third-party trees are structurally vendored).

/// Vendored directory segments (exact whole-component match only).
///
/// DOCS-LIST-2 (2026-09-01): `site-packages` / `dist-packages` — the pip/virtualenv
/// install target, the Python structural equivalent of `node_modules`.
/// COMPLEXITY-SCOPE-1 (§2.1.2): `dependencies` and `contrib` — third-party trees
/// vendored into the repository (poco's `dependencies/pcre2/**`, `contrib/**`).
pub const VENDORED_SEGMENTS: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "external",
    "deps",
    "node_modules",
    "site-packages",
    "dist-packages",
    "dependencies",
    "contrib",
];

/// Check if a path contains a vendored directory component.
///
/// Matches on WHOLE `/`-delimited components, case-insensitively: `vendor/lib.js`
/// and `src/vendor/lib.js` are vendored; `src/vendors_list.js` (substring) and
/// `myvendor/lib.js` (prefix) are not.
pub fn is_vendored_path(path: &str) -> bool {
    path.split('/').any(|segment| {
        let lower = segment.to_lowercase();
        VENDORED_SEGMENTS.contains(&lower.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::{is_vendored_path, VENDORED_SEGMENTS};

    #[test]
    fn dependencies_and_contrib_segments_are_vendored() {
        // The two segments COMPLEXITY-SCOPE-1 added — poco's real vendored trees.
        assert!(is_vendored_path("dependencies/pcre2/src/pcre2_match.c"));
        assert!(is_vendored_path("contrib/x/y.c"));
    }

    #[test]
    fn existing_segments_remain_vendored() {
        for seg in [
            "vendor",
            "vendors",
            "third_party",
            "third-party",
            "external",
            "deps",
            "node_modules",
            "site-packages",
            "dist-packages",
        ] {
            assert!(
                is_vendored_path(&format!("a/{seg}/b.rs")),
                "segment should be vendored: {seg}"
            );
            assert!(
                VENDORED_SEGMENTS.contains(&seg),
                "segment should be listed: {seg}"
            );
        }
    }

    #[test]
    fn substring_and_prefix_matches_are_not_segments() {
        assert!(!is_vendored_path("src/vendors_list.js")); // substring, not a component
        assert!(!is_vendored_path("myvendor/lib.js")); // prefix, not a component
        assert!(!is_vendored_path("dependencies_map.rs")); // substring, not a component
        assert!(!is_vendored_path("src/lib.js"));
    }
}
