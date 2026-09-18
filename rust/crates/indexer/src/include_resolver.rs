//! C/C++ include resolution policy — pure deterministic resolution
//! of `#include` directives to indexed header files.
//!
//! Resolution order (per c-include-resolution-v1.2.md, roots derived by
//! CPP-INCLUDE-ROOTS-1):
//!
//! For **quoted** includes (`"foo.h"`):
//! 1. Same-directory (source file's directory)
//! 2. Configured roots (explicit, user-declared)
//! 3. Derived conventional roots (every `include`/`inc` directory in the tree)
//!
//! For **angle-bracket** includes (`<foo.h>`):
//! 1. Configured roots (skip same-directory)
//! 2. Derived conventional roots
//!
//! CPP-INCLUDE-ROOTS-1 change: the conventional roots are no longer the three
//! repo-root-anchored literals `include`/`inc`/`src/include`; they are DERIVED
//! from the indexed file list — every directory named `include`/`inc` at any
//! depth (see [`derive_include_roots`]). This resolves headers under per-module
//! roots such as poco's `Foundation/include/Poco/…`; the three former literals
//! are subsumed (they are just the root-level case).
//!
//! v1.2 change: angle-bracket includes are now resolved against local
//! headers. The `is_system_include` flag controls same-dir behavior,
//! not whether resolution is attempted. If resolution fails, the
//! include is classified as system/external by outcome.
//!
//! All functions are PURE. No I/O, no storage access.

use std::collections::{BTreeSet, HashSet};

// ── Types ────────────────────────────────────────────────────────

/// Result of resolving an include directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeResolution {
    pub status: ResolutionStatus,
    /// The resolved stable key (only set when status is Resolved).
    pub target_stable_key: Option<String>,
    /// Candidate paths when ambiguous (for debugging/trust reporting).
    pub candidates: Vec<String>,
}

/// Resolution outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionStatus {
    /// Exactly one indexed file matches.
    Resolved,
    /// No indexed file matches.
    Unresolved,
    /// Multiple indexed files match (exact normalized lookup).
    Ambiguous,
}

/// Configuration for include resolution.
#[derive(Debug, Clone, Default)]
pub struct IncludeResolverConfig {
    /// User-declared roots (from CLI/`--include-root`, config, or programmatic).
    /// Checked BEFORE the derived conventional roots, then POOLED with them —
    /// a header present under both is AMBIGUOUS, never a silent pick.
    pub configured_roots: Vec<String>,
    /// Conventional include roots DERIVED from the indexed file list: every
    /// directory named `include`/`inc` at any depth (see
    /// [`derive_include_roots`]). `build_include_resolution_map` populates this
    /// from `file_paths`; a directly-constructed resolver (tests) may set it
    /// explicitly. Empty ⇒ same-directory / configured-only resolution.
    pub derived_roots: Vec<String>,
}

/// Include resolver with configuration.
#[derive(Debug, Clone)]
pub struct IncludeResolver {
    config: IncludeResolverConfig,
}

impl IncludeResolver {
    pub fn new(config: IncludeResolverConfig) -> Self {
        Self { config }
    }

    /// Create resolver with explicit configured roots and no derived roots.
    /// The derived conventional roots come from the indexed file list via
    /// [`build_include_resolution_map`]; this constructor is for a caller that
    /// supplies only explicit `--include-root`-style roots.
    pub fn with_configured_roots(roots: Vec<String>) -> Self {
        Self::new(IncludeResolverConfig {
            configured_roots: roots,
            derived_roots: Vec::new(),
        })
    }

    /// Resolve an include directive to an indexed file.
    ///
    /// # Arguments
    /// * `source_file_path` - Path of the file containing the #include (.c, .h, .cpp, etc.)
    /// * `include_specifier` - The include specifier (e.g., "util.h", "sub/foo.h")
    /// * `is_system_include` - True for <...>, false for "..."
    /// * `indexed_files` - Set of all indexed file paths (repo-relative)
    /// * `repo_uid` - Repository UID for stable key construction
    pub fn resolve(
        &self,
        source_file_path: &str,
        include_specifier: &str,
        is_system_include: bool,
        indexed_files: &HashSet<String>,
        repo_uid: &str,
    ) -> IncludeResolution {
        // v1.2: Both quoted and angle-bracket includes attempt resolution.
        // Same-dir is skipped for angle-bracket (is_system_include=true).
        let mut candidates: Vec<String> = Vec::new();

        // Step 1: Same-directory resolution (quoted includes only).
        // Angle-bracket includes skip same-dir — they conventionally imply
        // "search paths, not source locality."
        if !is_system_include {
            let source_dir = get_directory(source_file_path);
            let same_dir_path = join_path(source_dir, include_specifier);
            let normalized = normalize_path(&same_dir_path);

            if indexed_files.contains(&normalized) {
                // Same-directory match is authoritative — return immediately.
                let stable_key = format!("{}:{}:FILE", repo_uid, normalized);
                return IncludeResolution {
                    status: ResolutionStatus::Resolved,
                    target_stable_key: Some(stable_key),
                    candidates: Vec::new(),
                };
            }
        }

        // Step 2: Configured roots (checked before conventional).
        for root in &self.config.configured_roots {
            let candidate_path = join_path(root, include_specifier);
            let normalized = normalize_path(&candidate_path);
            if indexed_files.contains(&normalized) {
                candidates.push(normalized);
            }
        }

        // Step 3: Derived conventional roots (every include/inc dir in the tree).
        for root in &self.config.derived_roots {
            let candidate_path = join_path(root, include_specifier);
            let normalized = normalize_path(&candidate_path);
            if indexed_files.contains(&normalized) {
                // Avoid duplicates if configured and derived overlap.
                if !candidates.contains(&normalized) {
                    candidates.push(normalized);
                }
            }
        }

        // Step 4: Decide resolution status.
        match candidates.len() {
            0 => IncludeResolution {
                status: ResolutionStatus::Unresolved,
                target_stable_key: None,
                candidates: Vec::new(),
            },
            1 => {
                let path = candidates.remove(0);
                let stable_key = format!("{}:{}:FILE", repo_uid, path);
                IncludeResolution {
                    status: ResolutionStatus::Resolved,
                    target_stable_key: Some(stable_key),
                    candidates: Vec::new(),
                }
            }
            _ => {
                // Multiple matches — ambiguous.
                let candidate_keys: Vec<String> = candidates
                    .iter()
                    .map(|p| format!("{}:{}:FILE", repo_uid, p))
                    .collect();
                IncludeResolution {
                    status: ResolutionStatus::Ambiguous,
                    target_stable_key: None,
                    candidates: candidate_keys,
                }
            }
        }
    }
}

// ── Path helpers ─────────────────────────────────────────────────

/// Get the directory portion of a file path.
/// Returns empty string for top-level files.
fn get_directory(path: &str) -> &str {
    match path.rfind('/') {
        Some(pos) => &path[..pos],
        None => "",
    }
}

/// Join a directory and a relative path.
fn join_path(dir: &str, relative: &str) -> String {
    if dir.is_empty() {
        relative.to_string()
    } else {
        format!("{}/{}", dir, relative)
    }
}

/// Normalize a path: remove ./, resolve .., ensure forward slashes.
fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }

    parts.join("/")
}

// ── Build include resolution map ─────────────────────────────────

/// Derive the conventional include roots from the indexed file list.
///
/// A directory named exactly `include` or `inc` (lower-case, case-sensitive)
/// at ANY depth is an include root: for every indexed FILE path, each of its
/// DIRECTORY components equal to `include`/`inc` contributes the prefix up to
/// and including that component. A FILE literally named `include`/`inc`
/// contributes nothing — only directory components (all but the last) are
/// considered.
///
/// Example: `Foundation/include/Poco/Exception.h` → `Foundation/include`;
/// `a/include/b/inc/x.h` → `a/include` and `a/include/b/inc`.
///
/// The result is sorted and deduplicated, so the candidate order is
/// deterministic regardless of the file list's order (RG-REQ-001-L09).
///
/// This SUBSUMES the former hard-coded literals `include`, `inc`,
/// `src/include`: each arises naturally as the prefix of a file under a
/// root-level `include/`, `inc/` or `src/include/` directory. Adding roots can
/// only turn an Unresolved include into Resolved or Ambiguous — a same-directory
/// hit still returns first, and a header that resolved through a root-level
/// literal still resolves through the same (now derived) root
/// (RG-REQ-006-L03 never-flip).
pub fn derive_include_roots(file_paths: &[String]) -> Vec<String> {
    let mut roots: BTreeSet<String> = BTreeSet::new();
    for path in file_paths {
        let components: Vec<&str> = path.split('/').collect();
        // The last component is the file name — never a directory root.
        let dir_component_count = components.len().saturating_sub(1);
        for (i, component) in components.iter().enumerate().take(dir_component_count) {
            if *component == "include" || *component == "inc" {
                roots.insert(components[..=i].join("/"));
            }
        }
    }
    roots.into_iter().collect()
}

/// Build the per-file include resolution map for C/C++ files.
///
/// This is the CPP-INCLUDE-ROOTS-1 replacement for the literal-root v1.1
/// policy. It DERIVES the conventional roots from the indexed file list
/// ([`derive_include_roots`]) and pools them with the config's configured
/// roots (e.g. an `--include-root`); any `derived_roots` already carried on
/// the config are kept and augmented (tests may preset them; the orchestrator
/// passes none, so the derived set is exactly what the file list yields).
/// Deriving HERE — not at the call site — is what lets the one construction
/// site in the orchestrator "pass only configured_roots".
///
/// Outer key: source file UID (e.g., `r1:src/core/main.c`)
/// Inner key: bare include specifier (e.g., `util.h`, `sub/foo.h`)
/// Value: resolved stable key or None for ambiguous/unresolved
pub fn build_include_resolution_map(
    file_paths: &[String],
    repo_uid: &str,
    config: &IncludeResolverConfig,
) -> IncludeResolutionMap {
    let mut derived_roots = config.derived_roots.clone();
    for root in derive_include_roots(file_paths) {
        if !derived_roots.contains(&root) {
            derived_roots.push(root);
        }
    }
    let effective_config = IncludeResolverConfig {
        configured_roots: config.configured_roots.clone(),
        derived_roots,
    };
    let resolver = IncludeResolver::new(effective_config);

    // Build set of indexed files for O(1) lookup.
    let indexed_files: HashSet<String> = file_paths.iter().cloned().collect();

    IncludeResolutionMap {
        resolver,
        indexed_files,
        repo_uid: repo_uid.to_string(),
    }
}

/// Stateful include resolution map that can resolve includes on demand.
pub struct IncludeResolutionMap {
    resolver: IncludeResolver,
    indexed_files: HashSet<String>,
    repo_uid: String,
}

impl IncludeResolutionMap {
    /// Resolve an include from a source file.
    pub fn resolve(
        &self,
        source_file_path: &str,
        include_specifier: &str,
        is_system_include: bool,
    ) -> IncludeResolution {
        self.resolver.resolve(
            source_file_path,
            include_specifier,
            is_system_include,
            &self.indexed_files,
            &self.repo_uid,
        )
    }

    /// Get a resolved stable key, or None if unresolved/ambiguous.
    /// This is the simple API for integration with existing code.
    pub fn get_resolved_key(
        &self,
        source_file_path: &str,
        include_specifier: &str,
        is_system_include: bool,
    ) -> Option<String> {
        let result = self.resolve(source_file_path, include_specifier, is_system_include);
        match result.status {
            ResolutionStatus::Resolved => result.target_stable_key,
            _ => None,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_indexed_files(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    /// Build a resolver whose conventional roots are DERIVED from the given
    /// file list — the CPP-INCLUDE-ROOTS-1 replacement for the removed
    /// `with_defaults()` (which carried the three literal roots). A unit test
    /// that calls `resolver.resolve(..)` directly must derive its roots from
    /// the same paths it indexes, exactly as `build_include_resolution_map`
    /// does at the one construction site. No configured roots.
    fn resolver_for(paths: &[&str]) -> IncludeResolver {
        let owned: Vec<String> = paths.iter().map(|s| s.to_string()).collect();
        IncludeResolver::new(IncludeResolverConfig {
            configured_roots: Vec::new(),
            derived_roots: derive_include_roots(&owned),
        })
    }

    // ── Same-directory resolution (v1.0 baseline) ────────────────

    #[test]
    fn same_directory_resolves() {
        let files = ["src/core/main.c", "src/core/util.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/core/main.c", "util.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:src/core/util.h:FILE".to_string())
        );
    }

    #[test]
    fn same_directory_wins_over_include_root() {
        // If header exists in both same-dir and include/, same-dir wins.
        let files = [
            "src/core/main.c",
            "src/core/config.h", // same-dir
            "include/config.h",  // include root
        ];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/core/main.c", "config.h", false, &indexed, "r1");

        // Same-directory is authoritative.
        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:src/core/config.h:FILE".to_string())
        );
    }

    // ── Include root resolution ──────────────────────────────────

    #[test]
    fn include_root_resolves() {
        let files = ["core/main.c", "include/util.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("core/main.c", "util.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:include/util.h:FILE".to_string())
        );
    }

    #[test]
    fn inc_root_resolves() {
        let files = ["src/main.c", "inc/types.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/main.c", "types.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:inc/types.h:FILE".to_string())
        );
    }

    #[test]
    fn src_include_root_resolves() {
        let files = ["src/main.c", "src/include/api.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/main.c", "api.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:src/include/api.h:FILE".to_string())
        );
    }

    // ── Header-to-header resolution ──────────────────────────────

    #[test]
    fn header_to_header_same_dir() {
        let files = ["include/a.h", "include/b.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("include/a.h", "b.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:include/b.h:FILE".to_string())
        );
    }

    #[test]
    fn header_to_header_via_include_root() {
        let files = ["include/sub/a.h", "include/util.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        // a.h in include/sub/ includes "util.h" which is in include/
        let result = resolver.resolve("include/sub/a.h", "util.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:include/util.h:FILE".to_string())
        );
    }

    // ── Subpath includes ─────────────────────────────────────────

    #[test]
    fn subpath_include_resolves() {
        let files = ["src/main.c", "include/sub/foo.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/main.c", "sub/foo.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:include/sub/foo.h:FILE".to_string())
        );
    }

    #[test]
    fn subpath_does_not_match_basename() {
        // #include "sub/foo.h" should NOT match "include/foo.h"
        let files = [
            "src/main.c",
            "include/foo.h", // NOT include/sub/foo.h
        ];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/main.c", "sub/foo.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Unresolved);
    }

    // ── Configured roots ─────────────────────────────────────────

    #[test]
    fn configured_root_resolves() {
        let resolver = IncludeResolver::with_configured_roots(vec!["custom/headers".to_string()]);
        let indexed = make_indexed_files(&["src/main.c", "custom/headers/api.h"]);

        let result = resolver.resolve("src/main.c", "api.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:custom/headers/api.h:FILE".to_string())
        );
    }

    #[test]
    fn configured_and_conventional_both_matching_is_ambiguous() {
        // A header present under BOTH a configured root and a derived
        // conventional root is AMBIGUOUS — both candidates are pooled and no
        // silent pick is made. (The former name "configured_root_wins_over_
        // conventional" contradicted this body, which has always asserted
        // Ambiguity; renamed per decision D-TME-TEST-NAME-1, body unchanged.)
        let files = ["src/main.c", "custom/util.h", "include/util.h"];
        let resolver = IncludeResolver::new(IncludeResolverConfig {
            configured_roots: vec!["custom".to_string()],
            derived_roots: derive_include_roots(
                &files.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            ),
        });
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/main.c", "util.h", false, &indexed, "r1");

        // Both match — ambiguous.
        assert_eq!(result.status, ResolutionStatus::Ambiguous);
        assert_eq!(result.candidates.len(), 2);
    }

    // ── Ambiguity ────────────────────────────────────────────────

    #[test]
    fn ambiguous_when_multiple_roots_have_same_header() {
        let files = ["src/main.c", "include/config.h", "inc/config.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/main.c", "config.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Ambiguous);
        assert!(result
            .candidates
            .contains(&"r1:include/config.h:FILE".to_string()));
        assert!(result
            .candidates
            .contains(&"r1:inc/config.h:FILE".to_string()));
    }

    // ── Angle-bracket includes (v1.2) ─────────────────────────────

    #[test]
    fn angle_bracket_resolves_via_conventional_root() {
        // v1.2: <ngx_core.h> should resolve if it exists in conventional root.
        let files = ["src/http/request.c", "include/ngx_core.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve(
            "src/http/request.c",
            "ngx_core.h",
            true, // angle-bracket include
            &indexed,
            "nginx",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("nginx:include/ngx_core.h:FILE".to_string())
        );
    }

    #[test]
    fn angle_bracket_skips_same_dir() {
        // v1.2: <foo.h> should NOT check same directory.
        // Only quoted "foo.h" checks same-dir.
        let files = [
            "src/main.c",
            "src/foo.h", // same dir as source
                         // no include/foo.h
        ];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve(
            "src/main.c",
            "foo.h",
            true, // angle-bracket skips same-dir
            &indexed,
            "r1",
        );

        // Should NOT resolve — same-dir not checked for angle-bracket.
        assert_eq!(result.status, ResolutionStatus::Unresolved);
    }

    #[test]
    fn angle_bracket_unresolved_when_no_local_match() {
        // v1.2: <stdio.h> stays unresolved if no local header matches.
        let files = [
            "src/main.c",
            // no stdio.h anywhere
        ];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve(
            "src/main.c",
            "stdio.h",
            true, // angle-bracket
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Unresolved);
    }

    #[test]
    fn angle_bracket_resolves_if_local_header_exists() {
        // v1.2: even <stdio.h> resolves if someone vendored it locally.
        let files = [
            "src/main.c",
            "include/stdio.h", // vendored locally
        ];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve(
            "src/main.c",
            "stdio.h",
            true, // angle-bracket
            &indexed,
            "r1",
        );

        // Resolves because local header exists in conventional root.
        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:include/stdio.h:FILE".to_string())
        );
    }

    #[test]
    fn angle_bracket_ambiguous_when_multiple_matches() {
        // v1.2: ambiguity detection applies to angle-bracket too.
        let files = ["src/main.c", "include/config.h", "inc/config.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve(
            "src/main.c",
            "config.h",
            true, // angle-bracket
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Ambiguous);
        assert!(result
            .candidates
            .contains(&"r1:include/config.h:FILE".to_string()));
        assert!(result
            .candidates
            .contains(&"r1:inc/config.h:FILE".to_string()));
    }

    #[test]
    fn quoted_still_checks_same_dir_first() {
        // v1.2: quoted includes still check same-dir first.
        let files = [
            "src/main.c",
            "src/foo.h",     // same dir
            "include/foo.h", // also in conventional root
        ];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve(
            "src/main.c",
            "foo.h",
            false, // quoted — checks same-dir
            &indexed,
            "r1",
        );

        // Same-dir wins (authoritative, returns immediately).
        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:src/foo.h:FILE".to_string())
        );
    }

    // ── No sibling magic ─────────────────────────────────────────

    #[test]
    fn no_sibling_directory_magic() {
        // handlers/x.c includes "core/y.h" — should NOT resolve
        // because we don't search sibling directories.
        let files = ["handlers/x.c", "core/y.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("handlers/x.c", "core/y.h", false, &indexed, "r1");

        // This would only resolve if "core/y.h" existed under same-dir
        // or an include root. It doesn't.
        assert_eq!(result.status, ResolutionStatus::Unresolved);
    }

    // ── No suffix guessing ───────────────────────────────────────

    #[test]
    fn no_suffix_guessing() {
        let files = ["src/main.c", "include/foo.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        // #include "foo" should NOT match "foo.h"
        let result = resolver.resolve("src/main.c", "foo", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Unresolved);
    }

    // ── Derived conventional roots (CPP-INCLUDE-ROOTS-1) ──────────
    //
    // One test per §2.2 evidence-taxonomy row. These prove the derivation
    // itself: a directory named include/inc AT ANY DEPTH is a candidate root,
    // roots are pooled with configured, same-dir still wins, a FILE named
    // include is ignored, matching is case-sensitive, angle brackets resolve,
    // and derivation is order-independent.

    #[test]
    fn derived_root_resolves_header_under_nested_include_dir() {
        // poco's shape: Net/src/*.cpp includes "Poco/Exception.h", the header
        // living under a per-module Foundation/include/ root (RC-10).
        let files = ["Net/src/a.cpp", "Foundation/include/Poco/Exception.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("Net/src/a.cpp", "Poco/Exception.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:Foundation/include/Poco/Exception.h:FILE".to_string())
        );
    }

    #[test]
    fn derived_root_resolves_header_under_nested_inc_dir() {
        // A nested `inc/` directory is derived exactly like `include/`.
        let files = ["src/main.c", "lib/inc/api.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/main.c", "api.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:lib/inc/api.h:FILE".to_string())
        );
    }

    #[test]
    fn derived_root_ambiguous_when_two_derived_roots_hold_the_header() {
        // The same header under two per-module include roots is AMBIGUOUS and
        // counted, never a silent pick (RG-REQ-006-L03).
        let files = [
            "Net/src/b.cpp",
            "Foundation/include/Poco/Exception.h",
            "Util/include/Poco/Exception.h",
        ];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("Net/src/b.cpp", "Poco/Exception.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Ambiguous);
        assert_eq!(result.candidates.len(), 2);
        assert!(result
            .candidates
            .contains(&"r1:Foundation/include/Poco/Exception.h:FILE".to_string()));
        assert!(result
            .candidates
            .contains(&"r1:Util/include/Poco/Exception.h:FILE".to_string()));
    }

    #[test]
    fn derived_root_pooled_with_configured_root_is_ambiguous() {
        // A header under a configured root AND a derived root is pooled →
        // ambiguous, exactly as two derived roots would be.
        let files = ["src/main.c", "custom/api.h", "include/api.h"];
        let resolver = IncludeResolver::new(IncludeResolverConfig {
            configured_roots: vec!["custom".to_string()],
            derived_roots: derive_include_roots(
                &files.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            ),
        });
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/main.c", "api.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Ambiguous);
        assert_eq!(result.candidates.len(), 2);
    }

    #[test]
    fn derived_root_same_directory_hit_still_wins() {
        // A same-directory (quoted) hit is authoritative and returns before
        // any derived root is consulted — never-flip (RG-REQ-006-L03).
        let files = ["src/core/main.c", "src/core/util.h", "include/util.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("src/core/main.c", "util.h", false, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:src/core/util.h:FILE".to_string())
        );
    }

    #[test]
    fn derived_root_ignores_a_file_named_include() {
        // Only DIRECTORY components equal to include/inc derive a root; a FILE
        // named include/inc (its last path component) contributes nothing.
        let paths: Vec<String> = ["docs/include", "src/inc", "src/a.c"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(
            derive_include_roots(&paths).is_empty(),
            "a file named include/inc must derive no root"
        );
    }

    #[test]
    fn derived_root_is_case_sensitive() {
        // Matching is case-sensitive, lower-case only — `Include`/`INC`/`Inc`
        // derive nothing (a stated, counted conservatism, not a guess).
        let paths: Vec<String> = ["Include/foo.h", "INC/bar.h", "Inc/baz.h", "src/a.c"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(
            derive_include_roots(&paths).is_empty(),
            "upper/mixed-case include dirs must derive no root"
        );
    }

    #[test]
    fn derived_root_angle_bracket_resolves() {
        // Angle-bracket includes resolve through a derived root (same-dir is
        // skipped, which does not matter here).
        let files = ["Net/src/a.cpp", "Foundation/include/Poco/Exception.h"];
        let resolver = resolver_for(&files);
        let indexed = make_indexed_files(&files);

        let result = resolver.resolve("Net/src/a.cpp", "Poco/Exception.h", true, &indexed, "r1");

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(
            result.target_stable_key,
            Some("r1:Foundation/include/Poco/Exception.h:FILE".to_string())
        );
    }

    #[test]
    fn derived_roots_are_deterministic_regardless_of_file_order() {
        // The derived roots are sorted+deduplicated, so any file ordering
        // yields the same roots in the same order (RG-REQ-001-L09).
        let order_a: Vec<String> = [
            "Net/src/a.cpp",
            "Foundation/include/Poco/Exception.h",
            "a/include/b/inc/x.h",
            "Util/include/Poco/Bar.h",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let order_b: Vec<String> = [
            "a/include/b/inc/x.h",
            "Util/include/Poco/Bar.h",
            "Foundation/include/Poco/Exception.h",
            "Net/src/a.cpp",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let roots_a = derive_include_roots(&order_a);
        let roots_b = derive_include_roots(&order_b);

        assert_eq!(roots_a, roots_b);
        // The nested include/inc case contributes both prefixes.
        assert_eq!(
            roots_a,
            vec![
                "Foundation/include".to_string(),
                "Util/include".to_string(),
                "a/include".to_string(),
                "a/include/b/inc".to_string(),
            ]
        );
    }

    // ── Path normalization ───────────────────────────────────────

    #[test]
    fn normalize_removes_dot() {
        assert_eq!(normalize_path("./src/main.c"), "src/main.c");
        assert_eq!(normalize_path("src/./core/main.c"), "src/core/main.c");
    }

    #[test]
    fn normalize_resolves_dotdot() {
        assert_eq!(normalize_path("src/core/../main.c"), "src/main.c");
        assert_eq!(normalize_path("a/b/c/../../d.h"), "a/d.h");
    }

    #[test]
    fn normalize_removes_empty_segments() {
        assert_eq!(normalize_path("src//core/main.c"), "src/core/main.c");
    }

    // ── IncludeResolutionMap API ─────────────────────────────────

    #[test]
    fn resolution_map_get_resolved_key() {
        let config = IncludeResolverConfig::default();
        let paths = vec!["src/main.c".to_string(), "include/util.h".to_string()];
        let map = build_include_resolution_map(&paths, "r1", &config);

        let key = map.get_resolved_key("src/main.c", "util.h", false);
        assert_eq!(key, Some("r1:include/util.h:FILE".to_string()));

        let key = map.get_resolved_key("src/main.c", "missing.h", false);
        assert_eq!(key, None);
    }
}
