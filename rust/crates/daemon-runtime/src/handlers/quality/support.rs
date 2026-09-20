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

// COMPLEXITY-SCOPE-1 (§2.1.2): the vendored-path predicate now has ONE definition,
// in the inner `classification` crate, so the agent complexity aggregator can call
// the SAME function the daemon consumers do (RG-REQ-002-L02). This module forwards
// BOTH names — the predicate (`is_vendored_path`) and its segment list
// (`VENDORED_SEGMENTS`) — from that one definition so the allocated forwarding surface
// is complete; `classification::vendored_path` stays the single home.
//
// `#[allow(unused_imports)]`: `support` is a `pub(crate)` module (see quality/mod.rs),
// so this `pub use` is capped at crate visibility rather than being a crate-public
// re-export. No in-crate code references `VENDORED_SEGMENTS` today (every mention is a
// comment), so without this allow the `-D warnings` gate rejects the re-export as
// `unused_imports`. The allow keeps the intentional forwarding surface without adding a
// contrived in-crate consumer; the segment list has exactly one definition, upstream.
#[allow(unused_imports)]
pub use repo_graph_classification::{is_vendored_path, VENDORED_SEGMENTS};

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
