//! Domain types for documentation semantic fact extraction.
//!
//! These types are independent of storage schema. The outer layer
//! maps `ExtractedFact` to storage DTOs (`NewSemanticFact`).

use std::path::PathBuf;

/// A documentation or configuration file discovered for extraction.
#[derive(Debug, Clone)]
pub struct DocFile {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Path relative to repo root.
    pub relative_path: String,
    /// Classified document kind.
    pub kind: DocKind,
    /// Whether this is a generated document (e.g., MAP.md from rgistr).
    pub generated: bool,
    /// File content (populated after read).
    pub content: Option<String>,
    /// SHA-256 hash of content (populated after read).
    pub content_hash: Option<String>,
}

/// Classification of documentation source type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocKind {
    /// README.md, README
    Readme,
    /// ARCHITECTURE.md, CONTRIBUTING.md, design docs
    Architecture,
    /// docker-compose.yml, .env.*, deploy configs
    Config,
    /// Generated MAP.md files (lower confidence)
    Map,
}

impl DocKind {
    /// Storage-compatible string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            DocKind::Readme => "readme",
            DocKind::Architecture => "architecture",
            DocKind::Config => "config",
            DocKind::Map => "map",
        }
    }
}

/// Kind of semantic fact extracted from documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactKind {
    /// "X replaces Y"
    ReplacementFor,
    /// "X and Y are alternatives"
    AlternativeTo,
    /// "X is deprecated in favor of Y"
    DeprecatedBy,
    /// "migrate from X to Y"
    MigrationPath,
    /// "service runs in production/staging/dev"
    EnvironmentSurface,
    /// "must not be called in hot path"
    OperationalConstraint,
}

impl FactKind {
    /// Storage-compatible string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            FactKind::ReplacementFor => "replacement_for",
            FactKind::AlternativeTo => "alternative_to",
            FactKind::DeprecatedBy => "deprecated_by",
            FactKind::MigrationPath => "migration_path",
            FactKind::EnvironmentSurface => "environment_surface",
            FactKind::OperationalConstraint => "operational_constraint",
        }
    }
}

/// Kind of reference (subject or object) in a semantic fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    /// Module path (e.g., `src/core`, `packages/api`)
    Module,
    /// Symbol stable key or qualified name
    Symbol,
    /// File path relative to repo root
    File,
    /// Environment name (e.g., `production`, `dev`)
    Environment,
    /// Free-form text (e.g., constraint description)
    Text,
}

impl RefKind {
    /// Storage-compatible string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            RefKind::Module => "module",
            RefKind::Symbol => "symbol",
            RefKind::File => "file",
            RefKind::Environment => "environment",
            RefKind::Text => "text",
        }
    }
}

/// Method used to extract a semantic fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtractionMethod {
    /// Explicit `<!-- rg:* -->` marker
    ExplicitMarker,
    /// YAML frontmatter field
    Frontmatter,
    /// Structured config parsing (docker-compose, .env)
    ConfigParse,
    /// Keyword/regex pattern matching
    KeywordPattern,
}

impl ExtractionMethod {
    /// Storage-compatible string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ExtractionMethod::ExplicitMarker => "explicit_marker",
            ExtractionMethod::Frontmatter => "frontmatter",
            ExtractionMethod::ConfigParse => "config_parse",
            ExtractionMethod::KeywordPattern => "keyword_pattern",
        }
    }

    /// Base confidence for this extraction method.
    ///
    /// These are pre-generated-modifier values:
    /// - explicit_marker: 0.95 (highest — author intent is clear)
    /// - frontmatter: 0.90 (structured, author-provided)
    /// - config_parse: 0.90 (structured, machine-readable)
    /// - keyword_pattern: 0.70 (heuristic, may have false positives)
    pub fn base_confidence(&self) -> f64 {
        match self {
            ExtractionMethod::ExplicitMarker => 0.95,
            ExtractionMethod::Frontmatter => 0.90,
            ExtractionMethod::ConfigParse => 0.90,
            ExtractionMethod::KeywordPattern => 0.70,
        }
    }
}

/// A semantic fact extracted from documentation.
///
/// This is the extraction domain DTO. The outer layer maps this
/// to `NewSemanticFact` for storage.
#[derive(Debug, Clone)]
pub struct ExtractedFact {
    /// Kind of semantic relationship.
    pub fact_kind: FactKind,

    /// Subject reference value.
    pub subject_ref: String,
    /// Subject reference kind.
    pub subject_ref_kind: RefKind,

    /// Object reference value (None for some fact kinds).
    pub object_ref: Option<String>,
    /// Object reference kind.
    pub object_ref_kind: Option<RefKind>,

    /// Source file relative path.
    pub source_file: String,
    /// Start line in source file (1-indexed, if known).
    pub line_start: Option<u32>,
    /// End line in source file (1-indexed, if known).
    pub line_end: Option<u32>,
    /// Short excerpt of matched text (not full doc).
    pub excerpt: Option<String>,

    /// SHA-256 hash of source file content.
    pub content_hash: String,

    /// Extraction method used.
    pub extraction_method: ExtractionMethod,
    /// Confidence score (0.0-1.0), adjusted for source type.
    pub confidence: f64,
    /// Whether source is a generated document.
    pub generated: bool,
    /// Document kind.
    pub doc_kind: DocKind,
}

/// Result of extracting semantic facts from a repository.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// Extracted facts.
    pub facts: Vec<ExtractedFact>,
    /// Total files scanned.
    pub files_scanned: usize,
    /// Files matched by doc kind.
    pub files_by_kind: std::collections::HashMap<DocKind, usize>,
    /// Generated docs encountered.
    pub generated_docs_count: usize,
    /// Warnings encountered during extraction.
    pub warnings: Vec<ExtractionWarning>,
}

/// A warning encountered during extraction (non-fatal).
#[derive(Debug, Clone)]
pub struct ExtractionWarning {
    /// File that caused the warning.
    pub file: String,
    /// Warning message.
    pub message: String,
}

/// Confidence modifier for generated documents.
///
/// Facts extracted from generated docs (e.g., MAP.md) are
/// multiplied by this factor to reflect lower confidence
/// in synthesized content.
pub const GENERATED_DOC_CONFIDENCE_MODIFIER: f64 = 0.8;

/// Compute final confidence from extraction method and source type.
pub fn compute_confidence(method: ExtractionMethod, generated: bool) -> f64 {
    let base = method.base_confidence();
    if generated {
        base * GENERATED_DOC_CONFIDENCE_MODIFIER
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_reduced_for_generated_docs() {
        let authored = compute_confidence(ExtractionMethod::ExplicitMarker, false);
        let generated = compute_confidence(ExtractionMethod::ExplicitMarker, true);

        assert!(generated < authored);
        assert!((authored - 0.95).abs() < 0.001);
        assert!((generated - 0.76).abs() < 0.001); // 0.95 * 0.8
    }

    #[test]
    fn doc_kind_string_representation() {
        assert_eq!(DocKind::Readme.as_str(), "readme");
        assert_eq!(DocKind::Architecture.as_str(), "architecture");
        assert_eq!(DocKind::Config.as_str(), "config");
        assert_eq!(DocKind::Map.as_str(), "map");
    }

    #[test]
    fn fact_kind_string_representation() {
        assert_eq!(FactKind::ReplacementFor.as_str(), "replacement_for");
        assert_eq!(FactKind::EnvironmentSurface.as_str(), "environment_surface");
    }

    #[test]
    fn extraction_method_base_confidence() {
        assert!(ExtractionMethod::ExplicitMarker.base_confidence() > 0.9);
        assert!(ExtractionMethod::KeywordPattern.base_confidence() < 0.8);
    }
}
