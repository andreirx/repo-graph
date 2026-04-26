//! Document classification and generated detection.
//!
//! Classifies documentation files by kind and detects whether
//! content is authored or generated.

use crate::types::DocKind;

/// Classify a document by its relative path.
pub fn classify_doc_kind(relative_path: &str) -> DocKind {
    let lower = relative_path.to_lowercase();
    let file_name = relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .to_lowercase();

    // MAP.md files
    if file_name == "map.md" {
        return DocKind::Map;
    }

    // README variants
    if file_name == "readme.md" || file_name == "readme" {
        return DocKind::Readme;
    }

    // Architecture docs
    if file_name == "architecture.md"
        || file_name == "contributing.md"
        || file_name == "changelog.md"
        || lower.contains("docs/")
        || lower.contains("design/")
    {
        return DocKind::Architecture;
    }

    // Config files
    if file_name.starts_with("docker-compose")
        || file_name.starts_with("compose.")
        || file_name.starts_with(".env")
        || file_name.ends_with(".yaml")
        || file_name.ends_with(".yml")
    {
        return DocKind::Config;
    }

    // Default to architecture for unknown markdown
    if file_name.ends_with(".md") {
        return DocKind::Architecture;
    }

    // Default for unknown
    DocKind::Config
}

/// Check if a file is likely generated based on path alone.
///
/// This is a heuristic check. Full detection also uses frontmatter.
pub fn is_generated_by_path(relative_path: &str) -> bool {
    let file_name = relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .to_lowercase();

    // MAP.md is the primary generated doc pattern
    file_name == "map.md"
}

/// Frontmatter markers that indicate generated content.
const GENERATED_MARKERS: &[&str] = &[
    "generated_by",
    "generated: true",
    "kind: synthesized_summary",
    "auto_generated",
    "machine_generated",
];

/// Check if content frontmatter indicates generated document.
///
/// Parses YAML frontmatter and looks for generation markers.
pub fn is_generated_by_frontmatter(content: &str) -> bool {
    let frontmatter = match extract_frontmatter(content) {
        Some(fm) => fm,
        None => return false,
    };

    let lower = frontmatter.to_lowercase();

    for marker in GENERATED_MARKERS {
        if lower.contains(marker) {
            return true;
        }
    }

    false
}

/// Get explicit generated status from frontmatter.
///
/// Returns:
/// - `Some(true)` if frontmatter explicitly indicates generated content
/// - `Some(false)` if frontmatter explicitly indicates authored content (`generated: false`)
/// - `None` if frontmatter is absent or silent on generated status
///
/// This is authoritative: an explicit `generated: false` overrides path heuristics.
pub fn get_generated_from_frontmatter(content: &str) -> Option<bool> {
    if let Some(data) = parse_frontmatter(content) {
        // Explicit boolean field takes precedence
        if let Some(g) = data.generated {
            return Some(g);
        }
        // generated_by field implies generated = true
        if data.generated_by.is_some() {
            return Some(true);
        }
    }

    // Fall back to marker-based detection (returns Some(true) if markers found, None otherwise)
    let frontmatter = extract_frontmatter(content)?;
    let lower = frontmatter.to_lowercase();

    for marker in GENERATED_MARKERS {
        if lower.contains(marker) {
            return Some(true);
        }
    }

    None
}

/// Detect if document is generated using content analysis.
///
/// Path-based heuristics alone are NOT sufficient evidence.
/// Returns true only if content contains positive generation evidence
/// (explicit frontmatter markers like `generated: true` or `generated_by`).
///
/// When content is provided but silent on generation status, returns `false`.
/// Use `is_generated_by_path()` only when content is unavailable.
pub fn is_generated(_relative_path: &str, content: &str) -> bool {
    // Content analysis is authoritative. Path alone is insufficient.
    get_generated_from_frontmatter(content).unwrap_or(false)
}

/// Extract YAML frontmatter from markdown content.
///
/// Frontmatter is delimited by `---` at start and end.
pub fn extract_frontmatter(content: &str) -> Option<&str> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return None;
    }

    let after_start = &trimmed[3..];
    let end_pos = after_start.find("\n---")?;

    Some(&after_start[..end_pos])
}

/// Parse frontmatter as YAML and extract specific fields.
///
/// Returns None if frontmatter is missing or malformed.
pub fn parse_frontmatter(content: &str) -> Option<FrontmatterData> {
    let fm_str = extract_frontmatter(content)?;

    // Use serde_yaml to parse
    let value: serde_yaml::Value = serde_yaml::from_str(fm_str).ok()?;
    let mapping = value.as_mapping()?;

    let mut data = FrontmatterData::default();

    // Extract known fields
    if let Some(v) = mapping.get("generated_by") {
        data.generated_by = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("generated") {
        data.generated = v.as_bool();
    }
    if let Some(v) = mapping.get("replaces") {
        data.replaces = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("deprecated") {
        data.deprecated = v.as_bool();
    }
    if let Some(v) = mapping.get("deprecated_by") {
        data.deprecated_by = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("alternative_to") {
        data.alternative_to = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("kind") {
        data.kind = v.as_str().map(String::from);
    }
    if let Some(v) = mapping.get("scope") {
        data.scope = v.as_str().map(String::from);
    }

    Some(data)
}

/// Structured frontmatter data extracted from documents.
#[derive(Debug, Clone, Default)]
pub struct FrontmatterData {
    /// Generator tool (e.g., "rgistr")
    pub generated_by: Option<String>,
    /// Explicit generated flag
    pub generated: Option<bool>,
    /// Module/symbol this replaces
    pub replaces: Option<String>,
    /// Whether this is deprecated
    pub deprecated: Option<bool>,
    /// What this is deprecated by
    pub deprecated_by: Option<String>,
    /// Alternative module/symbol
    pub alternative_to: Option<String>,
    /// Document kind (e.g., "synthesized_summary")
    pub kind: Option<String>,
    /// Scope (e.g., "folder", "repo")
    pub scope: Option<String>,
}

impl FrontmatterData {
    /// Check if this frontmatter indicates generated content.
    pub fn is_generated(&self) -> bool {
        self.generated == Some(true)
            || self.generated_by.is_some()
            || self.kind.as_deref() == Some("synthesized_summary")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_readme() {
        assert_eq!(classify_doc_kind("README.md"), DocKind::Readme);
        assert_eq!(classify_doc_kind("readme.md"), DocKind::Readme);
        assert_eq!(classify_doc_kind("README"), DocKind::Readme);
    }

    #[test]
    fn classify_map() {
        assert_eq!(classify_doc_kind("MAP.md"), DocKind::Map);
        assert_eq!(classify_doc_kind("src/core/MAP.md"), DocKind::Map);
    }

    #[test]
    fn classify_architecture() {
        assert_eq!(classify_doc_kind("ARCHITECTURE.md"), DocKind::Architecture);
        assert_eq!(classify_doc_kind("docs/design.md"), DocKind::Architecture);
    }

    #[test]
    fn classify_config() {
        assert_eq!(classify_doc_kind("docker-compose.yml"), DocKind::Config);
        assert_eq!(classify_doc_kind(".env"), DocKind::Config);
        assert_eq!(classify_doc_kind(".env.production"), DocKind::Config);
    }

    #[test]
    fn generated_by_path() {
        assert!(is_generated_by_path("MAP.md"));
        assert!(is_generated_by_path("src/core/MAP.md"));
        assert!(!is_generated_by_path("README.md"));
    }

    #[test]
    fn extract_frontmatter_basic() {
        let content = "---\ntitle: Test\n---\n# Content";
        let fm = extract_frontmatter(content).unwrap();
        assert_eq!(fm.trim(), "title: Test");
    }

    #[test]
    fn extract_frontmatter_missing() {
        let content = "# No frontmatter";
        assert!(extract_frontmatter(content).is_none());
    }

    #[test]
    fn generated_by_frontmatter() {
        let content = "---\ngenerated_by: rgistr\n---\n# Map";
        assert!(is_generated_by_frontmatter(content));

        let content2 = "---\ngenerated: true\n---\n# Map";
        assert!(is_generated_by_frontmatter(content2));

        let content3 = "---\ntitle: Authored\n---\n# Doc";
        assert!(!is_generated_by_frontmatter(content3));
    }

    #[test]
    fn parse_frontmatter_replaces() {
        let content = "---\nreplaces: old-module\ndeprecated: true\n---\n# Doc";
        let data = parse_frontmatter(content).unwrap();

        assert_eq!(data.replaces, Some("old-module".to_string()));
        assert_eq!(data.deprecated, Some(true));
    }

    #[test]
    fn frontmatter_data_is_generated() {
        let mut data = FrontmatterData::default();
        assert!(!data.is_generated());

        data.generated = Some(true);
        assert!(data.is_generated());

        data.generated = None;
        data.generated_by = Some("rgistr".to_string());
        assert!(data.is_generated());
    }

    #[test]
    fn get_generated_from_frontmatter_explicit_true() {
        let content = "---\ngenerated: true\n---\n# Doc";
        assert_eq!(get_generated_from_frontmatter(content), Some(true));
    }

    #[test]
    fn get_generated_from_frontmatter_explicit_false() {
        let content = "---\ngenerated: false\n---\n# Doc";
        assert_eq!(get_generated_from_frontmatter(content), Some(false));
    }

    #[test]
    fn get_generated_from_frontmatter_generated_by_implies_true() {
        let content = "---\ngenerated_by: rgistr\n---\n# Doc";
        assert_eq!(get_generated_from_frontmatter(content), Some(true));
    }

    #[test]
    fn get_generated_from_frontmatter_silent() {
        let content = "---\ntitle: Authored\n---\n# Doc";
        assert_eq!(get_generated_from_frontmatter(content), None);
    }

    #[test]
    fn get_generated_from_frontmatter_no_frontmatter() {
        let content = "# Doc without frontmatter";
        assert_eq!(get_generated_from_frontmatter(content), None);
    }

    #[test]
    fn is_generated_frontmatter_overrides_path() {
        // MAP.md is generated by path, but explicit false in frontmatter overrides
        let content = "---\ngenerated: false\n---\n# Authored MAP";
        assert!(!is_generated("MAP.md", content));
        assert!(!is_generated("src/core/MAP.md", content));
    }

    #[test]
    fn is_generated_silent_frontmatter_not_generated() {
        // MAP.md with silent frontmatter is NOT generated.
        // Path alone is insufficient when content is available.
        let content = "---\ntitle: Map\n---\n# Core Module";
        assert!(!is_generated("MAP.md", content));
    }

    #[test]
    fn is_generated_no_frontmatter_not_generated() {
        // MAP.md with no frontmatter is NOT generated.
        // Readable content without evidence → authored.
        let content = "# Core Module\n\nHuman-written docs.";
        assert!(!is_generated("MAP.md", content));
    }

    #[test]
    fn is_generated_readme_with_generated_true() {
        // README.md not generated by path, but frontmatter says true
        let content = "---\ngenerated: true\n---\n# Auto-generated readme";
        assert!(is_generated("README.md", content));
    }
}
