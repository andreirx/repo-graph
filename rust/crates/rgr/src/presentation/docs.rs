//! Presentation layer for documentation commands.
//!
//! # CLI-OUT-5 Group 1
//!
//! Two commands, two response shapes:
//! - `docs list`: inventory of known documentation files
//! - `docs extract`: operation summary for semantic fact extraction
//!
//! Both share documentation vocabulary but have different payloads:
//! - list is inventory (array of entries)
//! - extract is operation summary (counts + warnings)
//!
//! # Output Contract
//!
//! - Deterministic ordering (by path for list)
//! - Full output, no truncation
//! - `--json` preserved for machine mode

use serde::Deserialize;
use std::collections::BTreeMap;

// ── docs list ────────────────────────────────────────────────────────────────

/// Response DTO for `docs list`.
#[derive(Debug, Deserialize)]
pub struct DocsListResponse {
    pub command: String,
    pub repo: String,
    pub repo_path: String,
    pub entries: Vec<DocEntry>,
    pub count: usize,
    pub counts_by_kind: BTreeMap<String, usize>,
    pub generated_count: usize,
}

/// Individual documentation entry.
#[derive(Debug, Deserialize, Clone)]
pub struct DocEntry {
    pub path: String,
    pub kind: String,
    pub generated: bool,
    #[allow(dead_code)]
    pub content_hash: String,
}

impl DocsListResponse {
    /// Render human-readable output.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Documentation\n\n");

        // Count line
        let doc_word = if self.count == 1 {
            "document"
        } else {
            "documents"
        };
        out.push_str(&format!("{} {}\n", self.count, doc_word));

        if self.count == 0 {
            out.push_str("\nhint: no documentation files detected in this repository.\n");
            return out;
        }

        // By kind breakdown (sorted by count desc, then kind asc)
        if !self.counts_by_kind.is_empty() {
            out.push_str("\nBy kind:\n");
            let mut by_kind: Vec<_> = self.counts_by_kind.iter().collect();
            by_kind.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (kind, count) in by_kind {
                out.push_str(&format!("  {}  {}\n", kind, count));
            }
        }

        // Generated count if any
        if self.generated_count > 0 {
            out.push_str(&format!("\n{} generated\n", self.generated_count));
        }

        // Entry list (sorted by path for determinism)
        out.push('\n');
        let mut entries = self.entries.clone();
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        for entry in &entries {
            let generated_marker = if entry.generated { "  [generated]" } else { "" };
            out.push_str(&format!(
                "  {}  {}{}\n",
                entry.path, entry.kind, generated_marker
            ));
        }

        // Hint
        out.push_str("\nhint: run 'rmap docs extract' to scan for explicit rg: markers and config patterns.\n");

        out
    }
}

// ── docs extract ─────────────────────────────────────────────────────────────

/// Response DTO for `docs extract`.
#[derive(Debug, Deserialize)]
pub struct DocsExtractResponse {
    pub command: String,
    pub repo: String,
    pub repo_path: String,
    pub files_scanned: usize,
    pub files_by_kind: BTreeMap<String, usize>,
    pub facts_extracted: usize,
    pub facts_inserted: usize,
    pub facts_deleted: usize,
    pub counts_by_kind: BTreeMap<String, usize>,
    pub generated_docs_count: usize,
    pub warnings: Vec<String>,
}

impl DocsExtractResponse {
    /// Render human-readable output.
    pub fn render_human(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str("Documentation Extraction\n\n");

        // Files scanned
        let file_word = if self.files_scanned == 1 {
            "file"
        } else {
            "files"
        };
        out.push_str(&format!("{} {} scanned\n", self.files_scanned, file_word));

        // Files by kind (sorted by count desc, then kind asc)
        if !self.files_by_kind.is_empty() {
            out.push_str("\nBy kind:\n");
            let mut by_kind: Vec<_> = self.files_by_kind.iter().collect();
            by_kind.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (kind, count) in by_kind {
                out.push_str(&format!("  {}  {}\n", kind, count));
            }
        }

        // Extraction results
        out.push_str("\nExtraction results:\n");
        out.push_str(&format!("  {} facts extracted\n", self.facts_extracted));
        out.push_str(&format!("  {} facts inserted\n", self.facts_inserted));
        out.push_str(&format!("  {} facts deleted\n", self.facts_deleted));
        out.push_str(&format!("  {} generated docs\n", self.generated_docs_count));

        // Facts by kind if any
        if !self.counts_by_kind.is_empty() {
            out.push_str("\nFacts by kind:\n");
            let mut by_kind: Vec<_> = self.counts_by_kind.iter().collect();
            by_kind.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (kind, count) in by_kind {
                out.push_str(&format!("  {}  {}\n", kind, count));
            }
        }

        // Warnings
        if self.warnings.is_empty() {
            out.push_str("\nNo warnings.\n");
        } else {
            out.push_str(&format!("\n{} warnings:\n", self.warnings.len()));
            for warning in &self.warnings {
                out.push_str(&format!("  - {}\n", warning));
            }
        }

        out
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── docs list tests ──────────────────────────────────────────────────────

    fn sample_list_response() -> DocsListResponse {
        let mut counts_by_kind = BTreeMap::new();
        counts_by_kind.insert("readme".to_string(), 2);
        counts_by_kind.insert("changelog".to_string(), 1);

        DocsListResponse {
            command: "docs list".to_string(),
            repo: "repo_test".to_string(),
            repo_path: "/test/repo".to_string(),
            entries: vec![
                DocEntry {
                    path: "README.md".to_string(),
                    kind: "readme".to_string(),
                    generated: false,
                    content_hash: "abc123".to_string(),
                },
                DocEntry {
                    path: "docs/README.md".to_string(),
                    kind: "readme".to_string(),
                    generated: false,
                    content_hash: "def456".to_string(),
                },
                DocEntry {
                    path: "CHANGELOG.md".to_string(),
                    kind: "changelog".to_string(),
                    generated: true,
                    content_hash: "ghi789".to_string(),
                },
            ],
            count: 3,
            counts_by_kind,
            generated_count: 1,
        }
    }

    #[test]
    fn list_render_shows_header() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.starts_with("Documentation\n"));
    }

    #[test]
    fn list_render_shows_count() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("3 documents"));
    }

    #[test]
    fn list_render_shows_by_kind() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("By kind:"));
        assert!(out.contains("readme  2"));
        assert!(out.contains("changelog  1"));
    }

    #[test]
    fn list_render_shows_generated_count() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("1 generated"));
    }

    #[test]
    fn list_render_shows_entries_sorted_by_path() {
        let resp = sample_list_response();
        let out = resp.render_human();
        // Entries should be sorted: CHANGELOG.md, README.md, docs/README.md
        let changelog_pos = out.find("CHANGELOG.md").unwrap();
        let readme_pos = out.find("README.md").unwrap();
        let docs_readme_pos = out.find("docs/README.md").unwrap();
        assert!(changelog_pos < readme_pos);
        assert!(readme_pos < docs_readme_pos);
    }

    #[test]
    fn list_render_shows_generated_marker() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("CHANGELOG.md  changelog  [generated]"));
    }

    #[test]
    fn list_render_shows_hint() {
        let resp = sample_list_response();
        let out = resp.render_human();
        assert!(out.contains("hint: run 'rmap docs extract' to scan for explicit rg: markers"));
    }

    #[test]
    fn list_render_empty_shows_hint() {
        let resp = DocsListResponse {
            command: "docs list".to_string(),
            repo: "repo_test".to_string(),
            repo_path: "/test/repo".to_string(),
            entries: vec![],
            count: 0,
            counts_by_kind: BTreeMap::new(),
            generated_count: 0,
        };
        let out = resp.render_human();
        assert!(out.contains("0 documents"));
        assert!(out.contains("hint: no documentation files detected"));
    }

    #[test]
    fn list_render_singular_document() {
        let mut counts_by_kind = BTreeMap::new();
        counts_by_kind.insert("readme".to_string(), 1);

        let resp = DocsListResponse {
            command: "docs list".to_string(),
            repo: "repo_test".to_string(),
            repo_path: "/test/repo".to_string(),
            entries: vec![DocEntry {
                path: "README.md".to_string(),
                kind: "readme".to_string(),
                generated: false,
                content_hash: "abc".to_string(),
            }],
            count: 1,
            counts_by_kind,
            generated_count: 0,
        };
        let out = resp.render_human();
        assert!(out.contains("1 document\n")); // singular
    }

    // ── docs extract tests ───────────────────────────────────────────────────

    fn sample_extract_response() -> DocsExtractResponse {
        let mut files_by_kind = BTreeMap::new();
        files_by_kind.insert("readme".to_string(), 2);

        let mut counts_by_kind = BTreeMap::new();
        counts_by_kind.insert("api_endpoint".to_string(), 5);
        counts_by_kind.insert("config_key".to_string(), 3);

        DocsExtractResponse {
            command: "docs extract".to_string(),
            repo: "repo_test".to_string(),
            repo_path: "/test/repo".to_string(),
            files_scanned: 2,
            files_by_kind,
            facts_extracted: 8,
            facts_inserted: 6,
            facts_deleted: 2,
            counts_by_kind,
            generated_docs_count: 1,
            warnings: vec![],
        }
    }

    #[test]
    fn extract_render_shows_header() {
        let resp = sample_extract_response();
        let out = resp.render_human();
        assert!(out.starts_with("Documentation Extraction\n"));
    }

    #[test]
    fn extract_render_shows_files_scanned() {
        let resp = sample_extract_response();
        let out = resp.render_human();
        assert!(out.contains("2 files scanned"));
    }

    #[test]
    fn extract_render_shows_files_by_kind() {
        let resp = sample_extract_response();
        let out = resp.render_human();
        assert!(out.contains("By kind:"));
        assert!(out.contains("readme  2"));
    }

    #[test]
    fn extract_render_shows_extraction_results() {
        let resp = sample_extract_response();
        let out = resp.render_human();
        assert!(out.contains("Extraction results:"));
        assert!(out.contains("8 facts extracted"));
        assert!(out.contains("6 facts inserted"));
        assert!(out.contains("2 facts deleted"));
        assert!(out.contains("1 generated docs"));
    }

    #[test]
    fn extract_render_shows_facts_by_kind() {
        let resp = sample_extract_response();
        let out = resp.render_human();
        assert!(out.contains("Facts by kind:"));
        assert!(out.contains("api_endpoint  5"));
        assert!(out.contains("config_key  3"));
    }

    #[test]
    fn extract_render_shows_no_warnings() {
        let resp = sample_extract_response();
        let out = resp.render_human();
        assert!(out.contains("No warnings."));
    }

    #[test]
    fn extract_render_shows_warnings() {
        let mut resp = sample_extract_response();
        resp.warnings = vec![
            "Failed to parse docs/api.md".to_string(),
            "Unknown format in CHANGELOG.md".to_string(),
        ];
        let out = resp.render_human();
        assert!(out.contains("2 warnings:"));
        assert!(out.contains("- Failed to parse docs/api.md"));
        assert!(out.contains("- Unknown format in CHANGELOG.md"));
    }

    #[test]
    fn extract_render_singular_file() {
        let mut files_by_kind = BTreeMap::new();
        files_by_kind.insert("readme".to_string(), 1);

        let resp = DocsExtractResponse {
            command: "docs extract".to_string(),
            repo: "repo_test".to_string(),
            repo_path: "/test/repo".to_string(),
            files_scanned: 1,
            files_by_kind,
            facts_extracted: 0,
            facts_inserted: 0,
            facts_deleted: 0,
            counts_by_kind: BTreeMap::new(),
            generated_docs_count: 0,
            warnings: vec![],
        };
        let out = resp.render_human();
        assert!(out.contains("1 file scanned")); // singular
    }
}
