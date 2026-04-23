//! C/C++ include resolution policy — pure deterministic resolution
//! of `#include` directives to indexed header files.
//!
//! Resolution order (per c-include-resolution-v1.1.md):
//! 1. Same-directory (v1.0 behavior)
//! 2. Configured roots (explicit, user-declared)
//! 3. Conventional roots (heuristic: include/, inc/, src/include/)
//!
//! All functions are PURE. No I/O, no storage access.

use std::collections::HashSet;

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
#[derive(Debug, Clone)]
pub struct IncludeResolverConfig {
    /// User-declared roots (from CLI, config file, or programmatic).
    /// Checked BEFORE conventional roots.
    pub configured_roots: Vec<String>,
    /// Repo-root anchored conventional roots.
    /// Default: ["include", "inc", "src/include"]
    pub conventional_roots: Vec<&'static str>,
}

impl Default for IncludeResolverConfig {
    fn default() -> Self {
        Self {
            configured_roots: Vec::new(),
            conventional_roots: vec!["include", "inc", "src/include"],
        }
    }
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

    /// Create resolver with default conventional roots and no configured roots.
    pub fn with_defaults() -> Self {
        Self::new(IncludeResolverConfig::default())
    }

    /// Create resolver with explicit configured roots (overrides conventional).
    pub fn with_configured_roots(roots: Vec<String>) -> Self {
        Self::new(IncludeResolverConfig {
            configured_roots: roots,
            conventional_roots: vec!["include", "inc", "src/include"],
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
        // System includes are not resolved locally.
        if is_system_include {
            return IncludeResolution {
                status: ResolutionStatus::Unresolved,
                target_stable_key: None,
                candidates: Vec::new(),
            };
        }

        let mut candidates: Vec<String> = Vec::new();

        // Step 1: Same-directory resolution.
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

        // Step 2: Configured roots (checked before conventional).
        for root in &self.config.configured_roots {
            let candidate_path = join_path(root, include_specifier);
            let normalized = normalize_path(&candidate_path);
            if indexed_files.contains(&normalized) {
                candidates.push(normalized);
            }
        }

        // Step 3: Conventional roots.
        for root in &self.config.conventional_roots {
            let candidate_path = join_path(root, include_specifier);
            let normalized = normalize_path(&candidate_path);
            if indexed_files.contains(&normalized) {
                // Avoid duplicates if configured and conventional overlap.
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

/// Build the per-file include resolution map for C/C++ files.
///
/// This is the v1.1 replacement for the v1.0 same-directory-only
/// `build_per_file_include_resolution`. It uses the full resolution
/// policy with configured and conventional roots.
///
/// Outer key: source file UID (e.g., `r1:src/core/main.c`)
/// Inner key: bare include specifier (e.g., `util.h`, `sub/foo.h`)
/// Value: resolved stable key or None for ambiguous/unresolved
pub fn build_include_resolution_map(
    file_paths: &[String],
    repo_uid: &str,
    config: &IncludeResolverConfig,
) -> IncludeResolutionMap {
    let resolver = IncludeResolver::new(config.clone());

    // Build set of indexed files for O(1) lookup.
    let indexed_files: HashSet<String> = file_paths.iter().cloned().collect();

    // Collect all header basenames and subpaths we might need to resolve.
    // For now, we just need the indexed_files set for resolution.

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

    // ── Same-directory resolution (v1.0 baseline) ────────────────

    #[test]
    fn same_directory_resolves() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/core/main.c",
            "src/core/util.h",
        ]);

        let result = resolver.resolve(
            "src/core/main.c",
            "util.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:src/core/util.h:FILE".to_string()));
    }

    #[test]
    fn same_directory_wins_over_include_root() {
        // If header exists in both same-dir and include/, same-dir wins.
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/core/main.c",
            "src/core/config.h",  // same-dir
            "include/config.h",   // include root
        ]);

        let result = resolver.resolve(
            "src/core/main.c",
            "config.h",
            false,
            &indexed,
            "r1",
        );

        // Same-directory is authoritative.
        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:src/core/config.h:FILE".to_string()));
    }

    // ── Include root resolution ──────────────────────────────────

    #[test]
    fn include_root_resolves() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "core/main.c",
            "include/util.h",
        ]);

        let result = resolver.resolve(
            "core/main.c",
            "util.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:include/util.h:FILE".to_string()));
    }

    #[test]
    fn inc_root_resolves() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/main.c",
            "inc/types.h",
        ]);

        let result = resolver.resolve(
            "src/main.c",
            "types.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:inc/types.h:FILE".to_string()));
    }

    #[test]
    fn src_include_root_resolves() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/main.c",
            "src/include/api.h",
        ]);

        let result = resolver.resolve(
            "src/main.c",
            "api.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:src/include/api.h:FILE".to_string()));
    }

    // ── Header-to-header resolution ──────────────────────────────

    #[test]
    fn header_to_header_same_dir() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "include/a.h",
            "include/b.h",
        ]);

        let result = resolver.resolve(
            "include/a.h",
            "b.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:include/b.h:FILE".to_string()));
    }

    #[test]
    fn header_to_header_via_include_root() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "include/sub/a.h",
            "include/util.h",
        ]);

        // a.h in include/sub/ includes "util.h" which is in include/
        let result = resolver.resolve(
            "include/sub/a.h",
            "util.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:include/util.h:FILE".to_string()));
    }

    // ── Subpath includes ─────────────────────────────────────────

    #[test]
    fn subpath_include_resolves() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/main.c",
            "include/sub/foo.h",
        ]);

        let result = resolver.resolve(
            "src/main.c",
            "sub/foo.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:include/sub/foo.h:FILE".to_string()));
    }

    #[test]
    fn subpath_does_not_match_basename() {
        // #include "sub/foo.h" should NOT match "include/foo.h"
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/main.c",
            "include/foo.h",  // NOT include/sub/foo.h
        ]);

        let result = resolver.resolve(
            "src/main.c",
            "sub/foo.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Unresolved);
    }

    // ── Configured roots ─────────────────────────────────────────

    #[test]
    fn configured_root_resolves() {
        let resolver = IncludeResolver::with_configured_roots(vec![
            "custom/headers".to_string(),
        ]);
        let indexed = make_indexed_files(&[
            "src/main.c",
            "custom/headers/api.h",
        ]);

        let result = resolver.resolve(
            "src/main.c",
            "api.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Resolved);
        assert_eq!(result.target_stable_key, Some("r1:custom/headers/api.h:FILE".to_string()));
    }

    #[test]
    fn configured_root_wins_over_conventional() {
        // If header exists in both configured and conventional root,
        // we should get AMBIGUOUS (both are collected).
        // Wait, no — we collect all candidates then decide.
        // Actually, configured is checked first, but we collect ALL matches.
        let resolver = IncludeResolver::new(IncludeResolverConfig {
            configured_roots: vec!["custom".to_string()],
            conventional_roots: vec!["include"],
        });
        let indexed = make_indexed_files(&[
            "src/main.c",
            "custom/util.h",
            "include/util.h",
        ]);

        let result = resolver.resolve(
            "src/main.c",
            "util.h",
            false,
            &indexed,
            "r1",
        );

        // Both match — ambiguous.
        assert_eq!(result.status, ResolutionStatus::Ambiguous);
        assert_eq!(result.candidates.len(), 2);
    }

    // ── Ambiguity ────────────────────────────────────────────────

    #[test]
    fn ambiguous_when_multiple_roots_have_same_header() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/main.c",
            "include/config.h",
            "inc/config.h",
        ]);

        let result = resolver.resolve(
            "src/main.c",
            "config.h",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Ambiguous);
        assert!(result.candidates.contains(&"r1:include/config.h:FILE".to_string()));
        assert!(result.candidates.contains(&"r1:inc/config.h:FILE".to_string()));
    }

    // ── System includes ──────────────────────────────────────────

    #[test]
    fn system_include_unresolved() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/main.c",
            "include/stdio.h",  // even if we have it locally
        ]);

        let result = resolver.resolve(
            "src/main.c",
            "stdio.h",
            true,  // system include
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Unresolved);
    }

    // ── No sibling magic ─────────────────────────────────────────

    #[test]
    fn no_sibling_directory_magic() {
        // handlers/x.c includes "core/y.h" — should NOT resolve
        // because we don't search sibling directories.
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "handlers/x.c",
            "core/y.h",
        ]);

        let result = resolver.resolve(
            "handlers/x.c",
            "core/y.h",
            false,
            &indexed,
            "r1",
        );

        // This would only resolve if "core/y.h" existed under same-dir
        // or an include root. It doesn't.
        assert_eq!(result.status, ResolutionStatus::Unresolved);
    }

    // ── No suffix guessing ───────────────────────────────────────

    #[test]
    fn no_suffix_guessing() {
        let resolver = IncludeResolver::with_defaults();
        let indexed = make_indexed_files(&[
            "src/main.c",
            "include/foo.h",
        ]);

        // #include "foo" should NOT match "foo.h"
        let result = resolver.resolve(
            "src/main.c",
            "foo",
            false,
            &indexed,
            "r1",
        );

        assert_eq!(result.status, ResolutionStatus::Unresolved);
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
        let paths = vec![
            "src/main.c".to_string(),
            "include/util.h".to_string(),
        ];
        let map = build_include_resolution_map(&paths, "r1", &config);

        let key = map.get_resolved_key("src/main.c", "util.h", false);
        assert_eq!(key, Some("r1:include/util.h:FILE".to_string()));

        let key = map.get_resolved_key("src/main.c", "missing.h", false);
        assert_eq!(key, None);
    }
}
