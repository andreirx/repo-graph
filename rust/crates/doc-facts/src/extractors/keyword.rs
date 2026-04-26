//! Keyword pattern extractor.
//!
//! Extracts semantic facts from natural language patterns:
//! - "X replaces Y" / "replacement for Y"
//! - "deprecated in favor of Y" / "use Y instead"
//! - "alternative to Y" / "X or Y can be used"
//! - "migrate from X to Y"

use regex::Regex;
use std::sync::LazyLock;

use crate::types::{
    compute_confidence, DocFile, ExtractedFact, ExtractionMethod, FactKind, RefKind,
};

/// Pattern definitions for keyword extraction.
struct PatternDef {
    regex: Regex,
    fact_kind: FactKind,
    /// Index of subject capture group (1-indexed), or 0 to infer from doc
    subject_group: usize,
    /// Index of object capture group (1-indexed)
    object_group: usize,
}

static PATTERNS: LazyLock<Vec<PatternDef>> = LazyLock::new(|| {
    vec![
        // "X replaces Y" / "X is a replacement for Y"
        PatternDef {
            regex: Regex::new(r"(?i)\b(\S+)\s+(?:replaces|is\s+a\s+replacement\s+for)\s+(\S+)").unwrap(),
            fact_kind: FactKind::ReplacementFor,
            subject_group: 1,
            object_group: 2,
        },
        // "replacement for Y" (subject inferred from doc)
        PatternDef {
            regex: Regex::new(r"(?i)\breplacement\s+for\s+(\S+)").unwrap(),
            fact_kind: FactKind::ReplacementFor,
            subject_group: 0, // infer from doc
            object_group: 1,
        },
        // "supersedes Y"
        PatternDef {
            regex: Regex::new(r"(?i)\b(?:this\s+)?(?:module\s+)?supersedes\s+(\S+)").unwrap(),
            fact_kind: FactKind::ReplacementFor,
            subject_group: 0,
            object_group: 1,
        },
        // "deprecated in favor of Y"
        PatternDef {
            regex: Regex::new(r"(?i)\bdeprecated\s+in\s+favor\s+of\s+(\S+)").unwrap(),
            fact_kind: FactKind::DeprecatedBy,
            subject_group: 0,
            object_group: 1,
        },
        // "use Y instead"
        PatternDef {
            regex: Regex::new(r"(?i)\buse\s+(\S+)\s+instead\b").unwrap(),
            fact_kind: FactKind::DeprecatedBy,
            subject_group: 0,
            object_group: 1,
        },
        // "X is deprecated" (no replacement specified)
        PatternDef {
            regex: Regex::new(r"(?i)\b(\S+)\s+is\s+deprecated\b").unwrap(),
            fact_kind: FactKind::DeprecatedBy,
            subject_group: 1,
            object_group: 0, // no object
        },
        // "alternative to Y"
        PatternDef {
            regex: Regex::new(r"(?i)\balternative\s+to\s+(\S+)").unwrap(),
            fact_kind: FactKind::AlternativeTo,
            subject_group: 0,
            object_group: 1,
        },
        // "X and Y are alternatives"
        PatternDef {
            regex: Regex::new(r"(?i)\b(\S+)\s+and\s+(\S+)\s+are\s+alternatives\b").unwrap(),
            fact_kind: FactKind::AlternativeTo,
            subject_group: 1,
            object_group: 2,
        },
        // "migrate from X to Y"
        PatternDef {
            regex: Regex::new(r"(?i)\bmigrate\s+from\s+(\S+)\s+to\s+(\S+)").unwrap(),
            fact_kind: FactKind::MigrationPath,
            subject_group: 1,
            object_group: 2,
        },
        // "migration from X to Y"
        PatternDef {
            regex: Regex::new(r"(?i)\bmigration\s+from\s+(\S+)\s+to\s+(\S+)").unwrap(),
            fact_kind: FactKind::MigrationPath,
            subject_group: 1,
            object_group: 2,
        },
    ]
});

/// Extract semantic facts from keyword patterns.
pub fn extract(doc: &DocFile, content: &str, content_hash: &str) -> Vec<ExtractedFact> {
    let mut facts = Vec::new();
    let confidence = compute_confidence(ExtractionMethod::KeywordPattern, doc.generated);
    let doc_subject = infer_subject_from_doc(doc);

    for (line_num, line) in content.lines().enumerate() {
        let line_number = (line_num + 1) as u32;

        for pattern in PATTERNS.iter() {
            for cap in pattern.regex.captures_iter(line) {
                let full_match = cap.get(0).map(|m| m.as_str().to_string());

                // Determine subject
                let subject = if pattern.subject_group == 0 {
                    doc_subject.clone()
                } else {
                    match cap.get(pattern.subject_group) {
                        Some(m) => clean_reference(m.as_str()),
                        None => continue,
                    }
                };

                // Determine object
                let object = if pattern.object_group == 0 {
                    None
                } else {
                    cap.get(pattern.object_group).map(|m| clean_reference(m.as_str()))
                };

                // Skip if subject and object are the same
                if let Some(ref obj) = object {
                    if subject == *obj {
                        continue;
                    }
                }

                // Skip common false positives
                if is_false_positive(&subject, object.as_deref()) {
                    continue;
                }

                facts.push(ExtractedFact {
                    fact_kind: pattern.fact_kind,
                    subject_ref: subject,
                    subject_ref_kind: RefKind::Module,
                    object_ref: object,
                    object_ref_kind: if pattern.object_group == 0 {
                        None
                    } else {
                        Some(RefKind::Module)
                    },
                    source_file: doc.relative_path.clone(),
                    line_start: Some(line_number),
                    line_end: Some(line_number),
                    excerpt: full_match,
                    content_hash: content_hash.to_string(),
                    extraction_method: ExtractionMethod::KeywordPattern,
                    confidence,
                    generated: doc.generated,
                    doc_kind: doc.kind,
                });
            }
        }
    }

    facts
}

/// Clean a reference string (remove punctuation, quotes, etc.)
fn clean_reference(s: &str) -> String {
    s.trim_matches(|c: char| c.is_ascii_punctuation() && c != '-' && c != '_' && c != '/')
        .to_string()
}

/// Check for common false positive patterns.
fn is_false_positive(subject: &str, object: Option<&str>) -> bool {
    let lower_subject = subject.to_lowercase();

    // Skip common words that aren't module/symbol names
    let skip_words = [
        "this", "that", "it", "the", "a", "an", "is", "are", "was", "were",
        "be", "been", "being", "have", "has", "had", "do", "does", "did",
        "will", "would", "could", "should", "may", "might", "must",
        "can", "need", "want", "like", "use", "make", "get", "set",
    ];

    if skip_words.contains(&lower_subject.as_str()) {
        return true;
    }

    if let Some(obj) = object {
        let lower_obj = obj.to_lowercase();
        if skip_words.contains(&lower_obj.as_str()) {
            return true;
        }
    }

    // Skip very short references (likely false positives)
    // Exception: "." is a valid repo root reference
    if subject.len() < 2 && subject != "." {
        return true;
    }

    false
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
    fn extract_replaces_pattern() {
        let doc = make_doc("README.md");
        // Pattern: (\S+)\s+replaces\s+(\S+)
        // Matches "module replaces old-auth" where subject="module", object="old-auth"
        let content = "The new-auth replaces old-auth.";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::ReplacementFor);
        assert_eq!(facts[0].subject_ref, "new-auth");
        // clean_reference strips trailing punctuation
        assert_eq!(facts[0].object_ref, Some("old-auth".to_string()));
    }

    #[test]
    fn extract_replacement_for_pattern() {
        let doc = make_doc("src/new-service/README.md");
        let content = "This is a replacement for legacy-service.";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::ReplacementFor);
        assert_eq!(facts[0].subject_ref, "src/new-service"); // inferred from doc
    }

    #[test]
    fn extract_deprecated_in_favor() {
        let doc = make_doc("README.md");
        // Note: "API is deprecated" and "deprecated in favor of" both match
        let content = "This module is deprecated in favor of NewAPI.";

        let facts = extract(&doc, content, "hash");

        // Both patterns match: "module is deprecated" and "deprecated in favor of NewAPI"
        assert_eq!(facts.len(), 2);
        assert!(facts.iter().all(|f| f.fact_kind == FactKind::DeprecatedBy));
    }

    #[test]
    fn extract_use_instead() {
        let doc = make_doc("README.md");
        let content = "Please use NewClient instead of this one.";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::DeprecatedBy);
        assert_eq!(facts[0].object_ref, Some("NewClient".to_string()));
    }

    #[test]
    fn extract_migrate_from_to() {
        let doc = make_doc("docs/migration.md");
        let content = "To migrate from REST to GraphQL, follow these steps.";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::MigrationPath);
        assert_eq!(facts[0].subject_ref, "REST");
        // clean_reference strips trailing comma
        assert_eq!(facts[0].object_ref, Some("GraphQL".to_string()));
    }

    #[test]
    fn extract_alternatives() {
        let doc = make_doc("README.md");
        let content = "java-backend and ts-serverless are alternatives for the API layer.";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_kind, FactKind::AlternativeTo);
        assert_eq!(facts[0].subject_ref, "java-backend");
        assert_eq!(facts[0].object_ref, Some("ts-serverless".to_string()));
    }

    #[test]
    fn skip_common_words() {
        let doc = make_doc("README.md");
        // "This replaces that" should be filtered out
        let content = "This replaces that.";

        let facts = extract(&doc, content, "hash");
        assert!(facts.is_empty());
    }

    #[test]
    fn multiple_patterns_same_line() {
        let doc = make_doc("README.md");
        let content = "new-api replaces old-api; also use NewClient instead";

        let facts = extract(&doc, content, "hash");

        assert_eq!(facts.len(), 2);
    }
}
