//! Documentation relevance selector.
//!
//! Pure function that selects and ranks relevant documentation files
//! for agent orientation. This is the primary documentation surface.
//!
//! ## Design principles
//!
//! - Docs are primary; semantic facts are secondary derived hints
//! - Structural relevance dominates ranking (path match >> root doc)
//! - `generated` flag is a mild tie-breaker, not a strong penalty
//! - Every doc includes a relevance reason
//! - Works on repos with zero semantic hints
//! - Does NOT derive from semantic_facts — uses doc inventory directly
//!
//! ## Relevance rules (v1, structural only)
//!
//! 1. Repo root README/ARCHITECTURE always relevant at repo scope
//! 2. Docs under matching module/path prefix are relevant
//! 3. Generated MAP in matching path is relevant (mild rank penalty)
//! 4. Config docs relevant for environment/build orientation
//!
//! ## Ranking rationale
//!
//! A generated MAP.md for `src/core/service` may be more useful than
//! a repo-root authored README for that specific focus. Structural
//! relevance (path match = 200 points) dominates over authored vs
//! generated distinction (5 point penalty). Authored preference is
//! a tie-breaker, not a filter.
//!
//! No text search, embeddings, or fuzzy matching in v1.

use crate::dto::envelope::{DocRelevanceReason, RelevantDoc, ResolvedKind};

/// Focus context for doc relevance selection.
#[derive(Debug, Clone)]
pub struct DocFocusContext {
    /// What kind of focus (repo, module, file, symbol).
    pub kind: ResolvedKind,
    /// The resolved path for path/module/file focus. None for repo-level.
    pub path: Option<String>,
}

impl DocFocusContext {
    /// Repo-level focus (no specific path).
    pub fn repo() -> Self {
        Self {
            kind: ResolvedKind::Repo,
            path: None,
        }
    }

    /// Path/module-level focus.
    pub fn path(path: &str) -> Self {
        Self {
            kind: ResolvedKind::Module,
            path: Some(path.to_string()),
        }
    }

    /// File-level focus.
    pub fn file(path: &str) -> Self {
        Self {
            kind: ResolvedKind::File,
            path: Some(path.to_string()),
        }
    }

    /// Symbol-level focus (uses file path for doc relevance).
    pub fn symbol(file_path: Option<&str>) -> Self {
        Self {
            kind: ResolvedKind::Symbol,
            path: file_path.map(|p| p.to_string()),
        }
    }
}

/// A doc inventory entry (input to relevance selection).
#[derive(Debug, Clone)]
pub struct DocEntry {
    pub path: String,
    pub kind: String,
    pub generated: bool,
}

/// Select and rank relevant documentation files.
///
/// This is a pure function. It does not access storage or the
/// filesystem — it operates on the provided inventory.
///
/// ## Ranking
///
/// 1. Structural relevance dominates (path match >> root doc)
/// 2. Authored docs rank above generated as a mild tie-breaker
/// 3. Architecture/README rank above config docs
/// 4. Generated MAPs in matching path outrank root-level authored docs
///
/// A generated MAP.md exactly for the focus path is more useful than
/// a repo-root authored README for that specific focus.
///
/// ## Return
///
/// Ordered list of relevant docs with reasons. Empty if no docs
/// match the focus context.
pub fn select_relevant_docs(
    inventory: &[DocEntry],
    focus: &DocFocusContext,
) -> Vec<RelevantDoc> {
    let mut results: Vec<(RelevantDoc, i32)> = Vec::new();

    for entry in inventory {
        if let Some((doc, priority)) = evaluate_doc_relevance(entry, focus) {
            results.push((doc, priority));
        }
    }

    // Sort by priority descending, then by path for determinism
    results.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| a.0.path.cmp(&b.0.path))
    });

    results.into_iter().map(|(doc, _)| doc).collect()
}

/// Evaluate a single doc's relevance to the focus context.
///
/// Returns `Some((RelevantDoc, priority))` if relevant, `None` if not.
/// Higher priority = more relevant.
fn evaluate_doc_relevance(
    entry: &DocEntry,
    focus: &DocFocusContext,
) -> Option<(RelevantDoc, i32)> {
    let path = &entry.path;
    let kind = &entry.kind;
    let generated = entry.generated;

    // Mild tie-breaker: prefer authored over generated, but structural
    // relevance dominates. -5 penalty is small enough that a path-matching
    // generated doc (200+) still outranks a root-level authored doc (100+).
    let generated_penalty = if generated { -5 } else { 0 };

    match focus.kind {
        ResolvedKind::Repo => {
            // Repo-level: root docs are most relevant
            if is_repo_root_doc(path) {
                let reason = if kind == "architecture" {
                    DocRelevanceReason::ArchitectureDoc
                } else {
                    DocRelevanceReason::RepoRootDoc
                };
                let priority = base_priority_for_kind(kind) + generated_penalty;
                return Some((make_doc(entry, reason), priority));
            }
            // Config docs at root are relevant for environment context
            if kind == "config" && !path.contains('/') {
                let priority = 50 + generated_penalty;
                return Some((make_doc(entry, DocRelevanceReason::ConfigRelevance), priority));
            }
            // Skip non-root docs at repo level (too many)
            None
        }

        ResolvedKind::Module | ResolvedKind::File | ResolvedKind::Symbol => {
            let focus_path = focus.path.as_deref()?;

            // Exact path match or parent directory match
            if doc_matches_path(path, focus_path) {
                let reason = if generated && kind == "map" {
                    DocRelevanceReason::GeneratedMapForTarget
                } else {
                    DocRelevanceReason::ModulePathMatch
                };
                let priority = 200 + base_priority_for_kind(kind) + generated_penalty;
                return Some((make_doc(entry, reason), priority));
            }

            // Architecture docs in ancestor paths are still relevant
            if kind == "architecture" && is_ancestor_doc(path, focus_path) {
                let priority = 150 + generated_penalty;
                return Some((make_doc(entry, DocRelevanceReason::ArchitectureDoc), priority));
            }

            // Repo-root docs are always somewhat relevant
            if is_repo_root_doc(path) {
                let reason = if kind == "architecture" {
                    DocRelevanceReason::ArchitectureDoc
                } else {
                    DocRelevanceReason::RepoRootDoc
                };
                let priority = 100 + base_priority_for_kind(kind) + generated_penalty;
                return Some((make_doc(entry, reason), priority));
            }

            // Config docs may be relevant for environment context
            if kind == "config" && config_relevant_to_path(path, focus_path) {
                let priority = 80 + generated_penalty;
                return Some((make_doc(entry, DocRelevanceReason::ConfigRelevance), priority));
            }

            None
        }
    }
}

fn make_doc(entry: &DocEntry, reason: DocRelevanceReason) -> RelevantDoc {
    RelevantDoc {
        path: entry.path.clone(),
        kind: entry.kind.clone(),
        generated: entry.generated,
        reason,
    }
}

fn is_repo_root_doc(path: &str) -> bool {
    !path.contains('/') && (
        path == "README.md" ||
        path == "README" ||
        path == "ARCHITECTURE.md" ||
        path == "CONTRIBUTING.md"
    )
}

fn base_priority_for_kind(kind: &str) -> i32 {
    match kind {
        "architecture" => 30,
        "readme" => 20,
        "map" => 10,
        "config" => 5,
        _ => 0,
    }
}

/// Check if a doc path matches or is a parent of the focus path.
fn doc_matches_path(doc_path: &str, focus_path: &str) -> bool {
    // Extract doc's directory
    let doc_dir = if let Some(pos) = doc_path.rfind('/') {
        &doc_path[..pos]
    } else {
        "" // root-level doc
    };

    // Exact directory match
    if doc_dir == focus_path {
        return true;
    }

    // Doc is in the focus directory
    if focus_path.starts_with(doc_dir) && !doc_dir.is_empty() {
        // Check it's a proper prefix (not just substring)
        let remainder = &focus_path[doc_dir.len()..];
        if remainder.starts_with('/') || remainder.is_empty() {
            return true;
        }
    }

    // Focus is in the doc's directory
    if doc_dir.starts_with(focus_path) {
        let remainder = &doc_dir[focus_path.len()..];
        if remainder.starts_with('/') || remainder.is_empty() {
            return true;
        }
    }

    false
}

/// Check if doc is in an ancestor directory of focus.
fn is_ancestor_doc(doc_path: &str, focus_path: &str) -> bool {
    let doc_dir = if let Some(pos) = doc_path.rfind('/') {
        &doc_path[..pos]
    } else {
        return true; // root-level is ancestor of everything
    };

    focus_path.starts_with(doc_dir) && {
        let remainder = &focus_path[doc_dir.len()..];
        remainder.starts_with('/') || remainder.is_empty()
    }
}

/// Check if config doc is relevant to focus path.
fn config_relevant_to_path(config_path: &str, focus_path: &str) -> bool {
    // Config in focus directory or ancestor
    let config_dir = if let Some(pos) = config_path.rfind('/') {
        &config_path[..pos]
    } else {
        return true; // root config relevant to all
    };

    focus_path.starts_with(config_dir) && {
        let remainder = &focus_path[config_dir.len()..];
        remainder.starts_with('/') || remainder.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(path: &str, kind: &str, generated: bool) -> DocEntry {
        DocEntry {
            path: path.to_string(),
            kind: kind.to_string(),
            generated,
        }
    }

    #[test]
    fn repo_level_returns_root_docs_only() {
        let inventory = vec![
            doc("README.md", "readme", false),
            doc("ARCHITECTURE.md", "architecture", false),
            doc("src/core/README.md", "readme", false),
            doc("src/core/MAP.md", "map", true),
        ];

        let focus = DocFocusContext::repo();
        let result = select_relevant_docs(&inventory, &focus);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "ARCHITECTURE.md"); // architecture ranks higher
        assert_eq!(result[1].path, "README.md");
    }

    #[test]
    fn path_focus_returns_matching_and_root_docs() {
        let inventory = vec![
            doc("README.md", "readme", false),
            doc("src/core/README.md", "readme", false),
            doc("src/core/MAP.md", "map", true),
            doc("src/other/README.md", "readme", false),
        ];

        let focus = DocFocusContext::path("src/core");
        let result = select_relevant_docs(&inventory, &focus);

        // Should include: src/core/README.md, src/core/MAP.md, README.md
        // Should NOT include: src/other/README.md
        // Structural relevance dominates: path-match (200+) beats root (100+)
        // even when path-match is generated
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].path, "src/core/README.md"); // path match, authored (220)
        assert_eq!(result[0].reason, DocRelevanceReason::ModulePathMatch);
        assert_eq!(result[1].path, "src/core/MAP.md"); // path match, generated (205)
        assert!(result[1].generated);
        assert_eq!(result[2].path, "README.md"); // root doc (120)
    }

    #[test]
    fn authored_docs_rank_above_generated() {
        let inventory = vec![
            doc("src/core/MAP.md", "map", true),
            doc("src/core/README.md", "readme", false),
        ];

        let focus = DocFocusContext::path("src/core");
        let result = select_relevant_docs(&inventory, &focus);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "src/core/README.md"); // authored first
        assert_eq!(result[1].path, "src/core/MAP.md"); // generated second
    }

    #[test]
    fn config_docs_included_with_reason() {
        let inventory = vec![
            doc(".env.production", "config", false),
            doc("README.md", "readme", false),
        ];

        let focus = DocFocusContext::repo();
        let result = select_relevant_docs(&inventory, &focus);

        assert_eq!(result.len(), 2);
        // Config should be included with ConfigRelevance reason
        let config = result.iter().find(|d| d.path == ".env.production").unwrap();
        assert_eq!(config.reason, DocRelevanceReason::ConfigRelevance);
    }

    #[test]
    fn empty_inventory_returns_empty() {
        let inventory: Vec<DocEntry> = vec![];
        let focus = DocFocusContext::repo();
        let result = select_relevant_docs(&inventory, &focus);
        assert!(result.is_empty());
    }

    #[test]
    fn symbol_focus_uses_file_path() {
        let inventory = vec![
            doc("README.md", "readme", false),
            doc("src/core/README.md", "readme", false),
        ];

        // Symbol in src/core/service.ts
        let focus = DocFocusContext::symbol(Some("src/core/service.ts"));
        let result = select_relevant_docs(&inventory, &focus);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "src/core/README.md"); // path match
    }
}
