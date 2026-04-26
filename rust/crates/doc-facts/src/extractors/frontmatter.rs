//! YAML frontmatter extractor.
//!
//! Extracts semantic facts from YAML frontmatter fields:
//! - `replaces: <module>`
//! - `deprecated: true`
//! - `deprecated_by: <module>`
//! - `alternative_to: <module>`

use crate::classification::parse_frontmatter;
use crate::types::{
    compute_confidence, DocFile, ExtractedFact, ExtractionMethod, FactKind, RefKind,
};

/// Extract semantic facts from YAML frontmatter.
pub fn extract(doc: &DocFile, content: &str, content_hash: &str) -> Vec<ExtractedFact> {
    let mut facts = Vec::new();

    let data = match parse_frontmatter(content) {
        Some(d) => d,
        None => return facts,
    };

    let confidence = compute_confidence(ExtractionMethod::Frontmatter, doc.generated);
    let subject = infer_subject_from_doc(doc);

    // replaces: <module>
    if let Some(target) = &data.replaces {
        facts.push(ExtractedFact {
            fact_kind: FactKind::ReplacementFor,
            subject_ref: subject.clone(),
            subject_ref_kind: RefKind::Module,
            object_ref: Some(target.clone()),
            object_ref_kind: Some(RefKind::Module),
            source_file: doc.relative_path.clone(),
            line_start: Some(1), // Frontmatter is at top
            line_end: None,
            excerpt: Some(format!("replaces: {}", target)),
            content_hash: content_hash.to_string(),
            extraction_method: ExtractionMethod::Frontmatter,
            confidence,
            generated: doc.generated,
            doc_kind: doc.kind,
        });
    }

    // deprecated_by: <module>
    if let Some(target) = &data.deprecated_by {
        facts.push(ExtractedFact {
            fact_kind: FactKind::DeprecatedBy,
            subject_ref: subject.clone(),
            subject_ref_kind: RefKind::Module,
            object_ref: Some(target.clone()),
            object_ref_kind: Some(RefKind::Module),
            source_file: doc.relative_path.clone(),
            line_start: Some(1),
            line_end: None,
            excerpt: Some(format!("deprecated_by: {}", target)),
            content_hash: content_hash.to_string(),
            extraction_method: ExtractionMethod::Frontmatter,
            confidence,
            generated: doc.generated,
            doc_kind: doc.kind,
        });
    } else if data.deprecated == Some(true) {
        // deprecated: true (without specifying replacement)
        facts.push(ExtractedFact {
            fact_kind: FactKind::DeprecatedBy,
            subject_ref: subject.clone(),
            subject_ref_kind: RefKind::Module,
            object_ref: None,
            object_ref_kind: None,
            source_file: doc.relative_path.clone(),
            line_start: Some(1),
            line_end: None,
            excerpt: Some("deprecated: true".to_string()),
            content_hash: content_hash.to_string(),
            extraction_method: ExtractionMethod::Frontmatter,
            confidence,
            generated: doc.generated,
            doc_kind: doc.kind,
        });
    }

    // alternative_to: <module>
    if let Some(target) = &data.alternative_to {
        facts.push(ExtractedFact {
            fact_kind: FactKind::AlternativeTo,
            subject_ref: subject.clone(),
            subject_ref_kind: RefKind::Module,
            object_ref: Some(target.clone()),
            object_ref_kind: Some(RefKind::Module),
            source_file: doc.relative_path.clone(),
            line_start: Some(1),
            line_end: None,
            excerpt: Some(format!("alternative_to: {}", target)),
            content_hash: content_hash.to_string(),
            extraction_method: ExtractionMethod::Frontmatter,
            confidence,
            generated: doc.generated,
            doc_kind: doc.kind,
        });
    }

    facts
}

/// Infer subject reference from document path.
fn infer_subject_from_doc(doc: &DocFile) -> String {
    let path = &doc.relative_path;

    if let Some(pos) = path.rfind('/') {
        let dir = &path[..pos];
        if dir.is_empty() {
            ".".to_string()
        } else {
            dir.to_string()
        }
    } else {
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
    fn extract_replaces_from_frontmatter() {
        let doc = make_doc("src/new-auth/README.md");
        let content = "---\nreplaces: old-auth\n---\n# New Auth";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::ReplacementFor);
        assert_eq!(facts[0].subject_ref, "src/new-auth");
        assert_eq!(facts[0].object_ref, Some("old-auth".to_string()));
        assert_eq!(facts[0].extraction_method, ExtractionMethod::Frontmatter);
    }

    #[test]
    fn extract_deprecated_by() {
        let doc = make_doc("README.md");
        let content = "---\ndeprecated_by: NewService\n---\n# Old";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::DeprecatedBy);
        assert_eq!(facts[0].object_ref, Some("NewService".to_string()));
    }

    #[test]
    fn extract_deprecated_without_replacement() {
        let doc = make_doc("README.md");
        let content = "---\ndeprecated: true\n---\n# Legacy";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::DeprecatedBy);
        assert_eq!(facts[0].object_ref, None);
    }

    #[test]
    fn extract_alternative_to() {
        let doc = make_doc("README.md");
        let content = "---\nalternative_to: java-backend\n---\n# TS Backend";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::AlternativeTo);
        assert_eq!(facts[0].object_ref, Some("java-backend".to_string()));
    }

    #[test]
    fn no_frontmatter_returns_empty() {
        let doc = make_doc("README.md");
        let content = "# No frontmatter here";

        let facts = extract(&doc, content, "hash");
        assert!(facts.is_empty());
    }

    #[test]
    fn multiple_facts_from_frontmatter() {
        let doc = make_doc("README.md");
        let content = "---\nreplaces: old-mod\nalternative_to: other-mod\n---\n# Module";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 2);
    }
}
