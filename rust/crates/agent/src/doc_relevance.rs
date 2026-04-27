//! Documentation relevance selector.
//!
//! Pure function that selects and ranks relevant documentation files
//! for agent orientation. This is the primary documentation surface.
//!
//! ## Design principles
//!
//! - Docs are primary; semantic facts are secondary derived hints
//! - Structural relevance dominates ranking via explicit tiers
//! - `generated` flag is a mild tie-breaker, not a strong penalty
//! - Every doc includes a relevance reason
//! - Works on repos with zero semantic hints
//! - Does NOT derive from semantic_facts — uses doc inventory directly
//!
//! ## Relevance tiers (explicit, not alphabetical)
//!
//! Priority is assigned by structural tier, then kind, then provenance:
//!
//! | Tier | Priority | Description |
//! |------|----------|-------------|
//! | 1 | 300+ | Exact directory match (doc in same dir as focus) |
//! | 2 | 250+ | Descendant doc (doc inside focus subtree) |
//! | 3 | 200+ | Ancestor doc (doc in parent dir of focus) |
//! | 4 | 150+ | Architecture doc in ancestor path |
//! | 5 | 100+ | Repo-root fallback |
//! | 6 | 80+ | Config docs in relevant scope |
//!
//! Within each tier: +30 architecture, +20 readme, +10 map, +5 config.
//! Authored gets +0, generated gets -5 (mild tie-breaker).
//!
//! This ensures exact-match local docs always outrank ancestor docs,
//! regardless of alphabetical order.
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

/// Match type for doc-to-focus relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocMatchType {
    /// Doc is in the exact same directory as focus.
    ExactDirectory,
    /// Doc is inside the focus subtree (descendant).
    Descendant,
    /// Doc is in a parent directory of focus (ancestor).
    Ancestor,
    /// No structural match.
    None,
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
    // generated doc (300+) still outranks a root-level authored doc (100+).
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
            let raw_focus_path = focus.path.as_deref()?;

            // For file/symbol focus, extract the parent directory.
            // For module focus, use the path as-is.
            let focus_path = match focus.kind {
                ResolvedKind::File | ResolvedKind::Symbol => {
                    // Extract parent directory from file path
                    if let Some(pos) = raw_focus_path.rfind('/') {
                        &raw_focus_path[..pos]
                    } else {
                        "" // root-level file
                    }
                }
                _ => raw_focus_path,
            };

            // Determine structural match type
            let match_type = classify_doc_match(path, focus_path);

            match match_type {
                DocMatchType::ExactDirectory => {
                    // Tier 1: exact directory match (highest priority)
                    let reason = if generated && kind == "map" {
                        DocRelevanceReason::GeneratedMapForTarget
                    } else {
                        DocRelevanceReason::ModulePathMatch
                    };
                    let priority = 300 + base_priority_for_kind(kind) + generated_penalty;
                    return Some((make_doc(entry, reason), priority));
                }
                DocMatchType::Descendant => {
                    // Tier 2: doc inside focus subtree
                    let reason = if generated && kind == "map" {
                        DocRelevanceReason::GeneratedMapForTarget
                    } else {
                        DocRelevanceReason::ModulePathMatch
                    };
                    let priority = 250 + base_priority_for_kind(kind) + generated_penalty;
                    return Some((make_doc(entry, reason), priority));
                }
                DocMatchType::Ancestor => {
                    // Tier 3: doc in parent directory
                    let reason = if generated && kind == "map" {
                        DocRelevanceReason::GeneratedMapForTarget
                    } else {
                        DocRelevanceReason::ModulePathMatch
                    };
                    let priority = 200 + base_priority_for_kind(kind) + generated_penalty;
                    return Some((make_doc(entry, reason), priority));
                }
                DocMatchType::None => {
                    // Not a direct path match, check other relevance rules
                }
            }

            // Architecture docs in ancestor paths are still relevant (tier 4)
            if kind == "architecture" && is_ancestor_doc(path, focus_path) {
                let priority = 150 + generated_penalty;
                return Some((make_doc(entry, DocRelevanceReason::ArchitectureDoc), priority));
            }

            // Repo-root docs are always somewhat relevant (tier 5)
            if is_repo_root_doc(path) {
                let reason = if kind == "architecture" {
                    DocRelevanceReason::ArchitectureDoc
                } else {
                    DocRelevanceReason::RepoRootDoc
                };
                let priority = 100 + base_priority_for_kind(kind) + generated_penalty;
                return Some((make_doc(entry, reason), priority));
            }

            // Config docs may be relevant for environment context (tier 6)
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

/// Classify the structural relationship between a doc and the focus path.
///
/// Returns the match type which determines the priority tier.
fn classify_doc_match(doc_path: &str, focus_path: &str) -> DocMatchType {
    // Extract doc's directory
    let doc_dir = if let Some(pos) = doc_path.rfind('/') {
        &doc_path[..pos]
    } else {
        "" // root-level doc
    };

    // Exact directory match: doc is in the same directory as focus
    if doc_dir == focus_path {
        return DocMatchType::ExactDirectory;
    }

    // Descendant: doc is inside the focus subtree
    // e.g., focus="ext/wasm", doc_dir="ext/wasm/api"
    if doc_dir.starts_with(focus_path) {
        let remainder = &doc_dir[focus_path.len()..];
        if remainder.starts_with('/') || remainder.is_empty() {
            return DocMatchType::Descendant;
        }
    }

    // Ancestor: doc is in a parent directory of focus
    // e.g., focus="ext/wasm", doc_dir="ext"
    if !doc_dir.is_empty() && focus_path.starts_with(doc_dir) {
        let remainder = &focus_path[doc_dir.len()..];
        if remainder.starts_with('/') || remainder.is_empty() {
            return DocMatchType::Ancestor;
        }
    }

    // Root-level docs are ancestors of everything
    if doc_dir.is_empty() {
        return DocMatchType::Ancestor;
    }

    DocMatchType::None
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
        // Tier 1 exact-match (300+) beats tier 5 root (100+)
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].path, "src/core/README.md"); // exact match, authored (320)
        assert_eq!(result[0].reason, DocRelevanceReason::ModulePathMatch);
        assert_eq!(result[1].path, "src/core/MAP.md"); // exact match, generated (305)
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

    #[test]
    fn exact_match_outranks_ancestor_match() {
        // This test verifies the fix for the sqlite ext/wasm ranking bug:
        // ext/wasm/README.md (exact) must rank above ext/README.md (ancestor)
        let inventory = vec![
            doc("ext/README.md", "readme", false),       // ancestor
            doc("ext/wasm/README.md", "readme", false),  // exact match
            doc("ext/wasm/api/README.md", "readme", false), // descendant
        ];

        let focus = DocFocusContext::path("ext/wasm");
        let result = select_relevant_docs(&inventory, &focus);

        assert_eq!(result.len(), 3);
        // Tier 1: exact match (300+)
        assert_eq!(result[0].path, "ext/wasm/README.md");
        // Tier 2: descendant (250+)
        assert_eq!(result[1].path, "ext/wasm/api/README.md");
        // Tier 3: ancestor (200+)
        assert_eq!(result[2].path, "ext/README.md");
    }

    #[test]
    fn descendant_docs_outrank_ancestor_docs() {
        let inventory = vec![
            doc("src/README.md", "readme", false),           // ancestor
            doc("src/core/README.md", "readme", false),      // exact
            doc("src/core/model/README.md", "readme", false), // descendant
        ];

        let focus = DocFocusContext::path("src/core");
        let result = select_relevant_docs(&inventory, &focus);

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].path, "src/core/README.md");      // exact (320)
        assert_eq!(result[1].path, "src/core/model/README.md"); // descendant (270)
        assert_eq!(result[2].path, "src/README.md");           // ancestor (220)
    }
}
