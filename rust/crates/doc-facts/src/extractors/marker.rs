//! Explicit marker extractor.
//!
//! Extracts semantic facts from explicit HTML comment markers:
//! - `<!-- rg:replaces <target> -->`
//! - `<!-- rg:deprecated-by <target> -->`
//! - `<!-- rg:alternative-to <target> -->`
//! - `<!-- rg:migration-path <from> <to> -->`
//! - `<!-- rg:constraint <text> -->`

use regex::Regex;
use std::sync::LazyLock;

use crate::types::{
    compute_confidence, DocFile, ExtractedFact, ExtractionMethod, FactKind, RefKind,
};

/// Marker pattern: `<!-- rg:<directive> <args> -->`
static MARKER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<!--\s*rg:(\S+)\s+(.+?)\s*-->").unwrap()
});

/// Extract semantic facts from explicit rg: markers.
pub fn extract(doc: &DocFile, content: &str, content_hash: &str) -> Vec<ExtractedFact> {
    let mut facts = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line_number = (line_num + 1) as u32;

        for cap in MARKER_PATTERN.captures_iter(line) {
            let directive = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let args = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let excerpt = cap.get(0).map(|m| m.as_str().to_string());

            if let Some(fact) = parse_marker(doc, directive, args, line_number, excerpt, content_hash) {
                facts.push(fact);
            }
        }
    }

    facts
}

fn parse_marker(
    doc: &DocFile,
    directive: &str,
    args: &str,
    line_number: u32,
    excerpt: Option<String>,
    content_hash: &str,
) -> Option<ExtractedFact> {
    let confidence = compute_confidence(ExtractionMethod::ExplicitMarker, doc.generated);

    match directive {
        "replaces" => {
            let target = args.trim();
            if target.is_empty() {
                return None;
            }
            Some(ExtractedFact {
                fact_kind: FactKind::ReplacementFor,
                subject_ref: infer_subject_from_doc(doc),
                subject_ref_kind: RefKind::Module,
                object_ref: Some(target.to_string()),
                object_ref_kind: Some(RefKind::Module),
                source_file: doc.relative_path.clone(),
                line_start: Some(line_number),
                line_end: Some(line_number),
                excerpt,
                content_hash: content_hash.to_string(),
                extraction_method: ExtractionMethod::ExplicitMarker,
                confidence,
                generated: doc.generated,
                doc_kind: doc.kind,
            })
        }

        "deprecated-by" | "deprecated_by" => {
            let target = args.trim();
            if target.is_empty() {
                return None;
            }
            Some(ExtractedFact {
                fact_kind: FactKind::DeprecatedBy,
                subject_ref: infer_subject_from_doc(doc),
                subject_ref_kind: RefKind::Module,
                object_ref: Some(target.to_string()),
                object_ref_kind: Some(RefKind::Module),
                source_file: doc.relative_path.clone(),
                line_start: Some(line_number),
                line_end: Some(line_number),
                excerpt,
                content_hash: content_hash.to_string(),
                extraction_method: ExtractionMethod::ExplicitMarker,
                confidence,
                generated: doc.generated,
                doc_kind: doc.kind,
            })
        }

        "alternative-to" | "alternative_to" => {
            let target = args.trim();
            if target.is_empty() {
                return None;
            }
            Some(ExtractedFact {
                fact_kind: FactKind::AlternativeTo,
                subject_ref: infer_subject_from_doc(doc),
                subject_ref_kind: RefKind::Module,
                object_ref: Some(target.to_string()),
                object_ref_kind: Some(RefKind::Module),
                source_file: doc.relative_path.clone(),
                line_start: Some(line_number),
                line_end: Some(line_number),
                excerpt,
                content_hash: content_hash.to_string(),
                extraction_method: ExtractionMethod::ExplicitMarker,
                confidence,
                generated: doc.generated,
                doc_kind: doc.kind,
            })
        }

        "migration-path" | "migration_path" => {
            // Format: <from> <to>
            let parts: Vec<&str> = args.split_whitespace().collect();
            if parts.len() < 2 {
                return None;
            }
            Some(ExtractedFact {
                fact_kind: FactKind::MigrationPath,
                subject_ref: parts[0].to_string(),
                subject_ref_kind: RefKind::Module,
                object_ref: Some(parts[1].to_string()),
                object_ref_kind: Some(RefKind::Module),
                source_file: doc.relative_path.clone(),
                line_start: Some(line_number),
                line_end: Some(line_number),
                excerpt,
                content_hash: content_hash.to_string(),
                extraction_method: ExtractionMethod::ExplicitMarker,
                confidence,
                generated: doc.generated,
                doc_kind: doc.kind,
            })
        }

        "constraint" => {
            let text = args.trim();
            if text.is_empty() {
                return None;
            }
            Some(ExtractedFact {
                fact_kind: FactKind::OperationalConstraint,
                subject_ref: infer_subject_from_doc(doc),
                subject_ref_kind: RefKind::Module,
                object_ref: Some(text.to_string()),
                object_ref_kind: Some(RefKind::Text),
                source_file: doc.relative_path.clone(),
                line_start: Some(line_number),
                line_end: Some(line_number),
                excerpt,
                content_hash: content_hash.to_string(),
                extraction_method: ExtractionMethod::ExplicitMarker,
                confidence,
                generated: doc.generated,
                doc_kind: doc.kind,
            })
        }

        _ => None, // Unknown directive
    }
}

/// Infer subject reference from document path.
///
/// For module-level docs (e.g., `src/core/README.md`), the subject
/// is the containing directory. For repo-level docs, it's the repo root.
fn infer_subject_from_doc(doc: &DocFile) -> String {
    let path = &doc.relative_path;

    // Remove filename to get directory
    if let Some(pos) = path.rfind('/') {
        let dir = &path[..pos];
        if dir.is_empty() {
            ".".to_string()
        } else {
            dir.to_string()
        }
    } else {
        // Root-level file
        ".".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DocKind;
    use std::path::PathBuf;

    fn make_doc(relative_path: &str) -> DocFile {
        DocFile {
            path: PathBuf::from("/test").join(relative_path),
            relative_path: relative_path.to_string(),
            kind: DocKind::Readme,
            generated: false,
            content: None,
            content_hash: None,
        }
    }

    #[test]
    fn extract_replaces_marker() {
        let doc = make_doc("src/new-service/README.md");
        let content = "# New Service\n\n<!-- rg:replaces old-service -->\n";

        let facts = extract(&doc, content, "hash123");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::ReplacementFor);
        assert_eq!(facts[0].subject_ref, "src/new-service");
        assert_eq!(facts[0].object_ref, Some("old-service".to_string()));
        assert_eq!(facts[0].line_start, Some(3));
    }

    #[test]
    fn extract_deprecated_by_marker() {
        let doc = make_doc("README.md");
        let content = "<!-- rg:deprecated-by NewClient -->";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::DeprecatedBy);
        assert_eq!(facts[0].object_ref, Some("NewClient".to_string()));
    }

    #[test]
    fn extract_migration_path() {
        let doc = make_doc("docs/migration.md");
        let content = "<!-- rg:migration-path rest-api graphql-api -->";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::MigrationPath);
        assert_eq!(facts[0].subject_ref, "rest-api");
        assert_eq!(facts[0].object_ref, Some("graphql-api".to_string()));
    }

    #[test]
    fn extract_constraint() {
        let doc = make_doc("src/hot-path/README.md");
        let content = "<!-- rg:constraint must not allocate in hot path -->";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::OperationalConstraint);
        assert_eq!(
            facts[0].object_ref,
            Some("must not allocate in hot path".to_string())
        );
    }

    #[test]
    fn infer_subject_from_nested_path() {
        let doc = make_doc("src/core/domain/README.md");
        assert_eq!(infer_subject_from_doc(&doc), "src/core/domain");
    }

    #[test]
    fn infer_subject_from_root() {
        let doc = make_doc("README.md");
        assert_eq!(infer_subject_from_doc(&doc), ".");
    }

    #[test]
    fn ignore_unknown_directive() {
        let doc = make_doc("README.md");
        let content = "<!-- rg:unknown-thing foo -->";

        let facts = extract(&doc, content, "hash");
        assert!(facts.is_empty());
    }

    #[test]
    fn multiple_markers_on_different_lines() {
        let doc = make_doc("README.md");
        let content = "<!-- rg:replaces old-a -->\n<!-- rg:replaces old-b -->";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].line_start, Some(1));
        assert_eq!(facts[1].line_start, Some(2));
    }
}
