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

/// CHURN-SHALLOW-1 §2: diagnose the repo's history shape and serialize it as the
/// additive `history` block shared by the churn/hotspots/risk responses.
///
/// This is the daemon→CLI boundary DTO: a raw tagged-union JSON object (`kind` +
/// per-variant fields), NOT the `repo_graph_git::HistoryShape` domain enum (the git
/// crate carries no serde). Three concrete callers (churn/hotspots/risk handlers)
/// share it so the wire shape can never drift between the three surfaces.
///
/// Honesty rule #1: a FAILED git read is `kind: "unknown"` WITH its reason — never a
/// guessed shape. The four known cells map to their own tags; `head_commit_date`
/// additionally derives a concrete `suggested_since` (`--since <date>` inclusive of
/// the last commit).
pub fn diagnose_history_json(
    root_path: &Path,
    window: &repo_graph_git::ChurnWindow,
) -> serde_json::Value {
    match repo_graph_git::diagnose_history(root_path, window) {
        Ok(shape) => history_shape_json(&shape),
        Err(e) => serde_json::json!({
            "kind": "unknown",
            "reason": format!("history diagnosis failed: {e}"),
        }),
    }
}

/// Map a known [`repo_graph_git::HistoryShape`] to its wire DTO. Exhaustive — a new
/// cell must break this match (and every renderer's).
fn history_shape_json(shape: &repo_graph_git::HistoryShape) -> serde_json::Value {
    use repo_graph_git::HistoryShape;
    match shape {
        HistoryShape::NoHistory => serde_json::json!({ "kind": "no_history" }),
        HistoryShape::ShallowOrSingle {
            commits_available,
            is_shallow,
            head_commit_date,
            commits_in_window,
        } => serde_json::json!({
            "kind": "shallow_or_single",
            "commits_available": commits_available,
            "is_shallow": is_shallow,
            "head_commit_date": head_commit_date,
            "commits_in_window": commits_in_window,
        }),
        HistoryShape::ZeroInWindow { head_commit_date } => serde_json::json!({
            "kind": "zero_in_window",
            "head_commit_date": head_commit_date,
            // `--since <head-date>` is inclusive at day granularity, so it captures
            // the most recent commit — the concrete widening the reader should try.
            "suggested_since": head_commit_date,
        }),
        HistoryShape::Healthy => serde_json::json!({ "kind": "healthy" }),
    }
}
