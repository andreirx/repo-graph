//! Quality-specific support utilities.
//!
//! Contains utilities specific to quality handlers (churn, hotspots, risk, coverage).
//! Common handler utilities live in `handlers::support`.

use std::path::{Path, PathBuf};

// Re-export shared utilities for quality handlers
pub use crate::handlers::support::{get_optional_string_param, resolve_and_load_repo};

/// Resolve a repo's root_path to an absolute path.
///
/// The `root_path` in the database is stored relative to the db_path.
/// This function resolves it to an absolute path by joining with the
/// db_path's parent directory.
///
/// BUG FIX: Without this resolution, git commands fail with "No such file
/// or directory" when the daemon runs with cwd=/ (as launchd services do).
pub fn resolve_root_path(db_path: &Path, relative_root_path: &str) -> PathBuf {
    let db_dir = db_path.parent().unwrap_or(Path::new("/"));
    let resolved = db_dir.join(relative_root_path);
    // Canonicalize to remove ../ components and resolve symlinks
    resolved.canonicalize().unwrap_or(resolved)
}

/// Vendored directory segments (exact match only).
///
/// DOCS-LIST-2 (2026-09-01): added `site-packages` / `dist-packages` — the pip/virtualenv install
/// target, the Python structural equivalent of the `node_modules` already listed here. The list was
/// authored TS-first and under-covered Python; FRAKTAG's `fraktag-env/lib/pythonX.Y/site-packages/**`
/// docs proved the gap (they are vendored dependency content, not the reader's code). Shared with
/// `hotspots --exclude-vendored` (strictly more correct there too: a site-packages hotspot IS
/// vendored). One-line revert if the reviewer wants the pre-Python list.
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
];

/// Check if path contains a vendored directory segment.
pub fn is_vendored_path(path: &str) -> bool {
    path.split('/').any(|segment| {
        let lower = segment.to_lowercase();
        VENDORED_SEGMENTS.contains(&lower.as_str())
    })
}
