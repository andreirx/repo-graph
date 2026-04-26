//! Semantic fact extractors for documentation files.
//!
//! Each extractor targets a specific pattern or format:
//! - `marker`: explicit `<!-- rg:* -->` HTML comments
//! - `frontmatter`: YAML frontmatter fields
//! - `keyword`: regex-based keyword pattern matching
//! - `config`: structured config file parsing

pub mod config;
pub mod frontmatter;
pub mod keyword;
pub mod marker;

use crate::types::{DocFile, ExtractedFact};

/// Extract all semantic facts from a document file.
///
/// Runs all applicable extractors based on document kind and
/// aggregates results.
pub fn extract_from_file(doc: &DocFile) -> Vec<ExtractedFact> {
    let content = match &doc.content {
        Some(c) => c,
        None => return Vec::new(),
    };

    let content_hash = match &doc.content_hash {
        Some(h) => h.clone(),
        None => return Vec::new(),
    };

    let mut facts = Vec::new();

    // Run extractors based on file type
    match doc.kind {
        crate::types::DocKind::Config => {
            // Config files: only config extractor
            facts.extend(config::extract(doc, content, &content_hash));
        }
        _ => {
            // Documentation files: markers, frontmatter, keywords
            facts.extend(marker::extract(doc, content, &content_hash));
            facts.extend(frontmatter::extract(doc, content, &content_hash));
            facts.extend(keyword::extract(doc, content, &content_hash));
        }
    }

    facts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DocKind;
    use std::path::PathBuf;

    fn make_doc(kind: DocKind, content: &str) -> DocFile {
        DocFile {
            path: PathBuf::from("/test/file"),
            relative_path: "test.md".to_string(),
            kind,
            generated: false,
            content: Some(content.to_string()),
            content_hash: Some("testhash".to_string()),
        }
    }

    #[test]
    fn extract_from_empty_content() {
        let mut doc = make_doc(DocKind::Readme, "");
        doc.content = None;

        let facts = extract_from_file(&doc);
        assert!(facts.is_empty());
    }

    #[test]
    fn extract_runs_multiple_extractors() {
        let content = r#"---
replaces: old-module
---

# New Module

<!-- rg:replaces old-service -->

This module replaces the legacy implementation.
"#;
        let doc = make_doc(DocKind::Readme, content);

        let facts = extract_from_file(&doc);

        // Should get facts from frontmatter, marker, and possibly keyword
        assert!(!facts.is_empty());
    }
}
