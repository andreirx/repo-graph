//! Documentation semantic fact extraction for repo-graph.
//!
//! This crate extracts semantic facts from documentation and
//! configuration files. It is independent of storage — the outer
//! layer maps `ExtractedFact` to storage DTOs.
//!
//! ## Architecture
//!
//! - `types`: Domain DTOs (ExtractedFact, DocKind, etc.)
//! - `discovery`: Find candidate doc/config files
//! - `classification`: Classify doc kind, detect generated content
//! - `extractors`: Per-format extraction (marker, frontmatter, keyword, config)
//!
//! ## Usage
//!
//! ```ignore
//! use repo_graph_doc_facts::{extract_semantic_facts, ExtractionResult};
//! use std::path::Path;
//!
//! let result = extract_semantic_facts(Path::new("/path/to/repo"))?;
//! println!("Extracted {} facts from {} files", result.facts.len(), result.files_scanned);
//! ```

pub mod classification;
pub mod discovery;
pub mod extractors;
pub mod types;

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub use types::{
    DocFile, DocKind, ExtractedFact, ExtractionMethod, ExtractionResult, ExtractionWarning,
    FactKind, RefKind,
};

/// Error type for documentation extraction operations.
#[derive(Debug, thiserror::Error)]
pub enum DocFactsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("path is not a directory: {0}")]
    NotADirectory(String),
}

/// Documentation inventory entry (primary documentation surface).
///
/// This is the shared DTO for `docs list`, `orient`, and future
/// persisted inventory. Docs are primary; semantic facts are secondary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocInventoryEntry {
    /// Path relative to repo root.
    pub path: String,
    /// Classified document kind.
    pub kind: String,
    /// Whether this is a generated document (e.g., MAP.md from rgistr).
    pub generated: bool,
    /// SHA-256 hash of content (optional, computed on demand).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// Result of documentation inventory discovery.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocInventoryResult {
    /// Discovered documentation files.
    pub entries: Vec<DocInventoryEntry>,
    /// Files matched by doc kind.
    pub counts_by_kind: HashMap<String, usize>,
    /// Generated docs encountered.
    pub generated_count: usize,
}

/// Discover documentation inventory without extracting semantic facts.
///
/// This is the primary documentation surface. Returns doc file paths,
/// kinds, and generated flags. Content hashing is optional.
///
/// ## Generated flag semantics
///
/// The `generated` flag is determined by **path convention only**:
/// - `MAP.md` files are marked generated (workflow convention)
/// - All other doc files are marked as authored
///
/// This is an advisory workflow hint, not semantic truth. Content-based
/// frontmatter detection (e.g., `generated: true` in YAML) belongs in
/// the semantic-fact extraction layer, not the inventory layer.
///
/// For semantic fact extraction, use `extract_semantic_facts` instead.
pub fn discover_doc_inventory(
    repo_path: &Path,
    compute_hashes: bool,
) -> Result<DocInventoryResult, DocFactsError> {
    if !repo_path.is_dir() {
        return Err(DocFactsError::NotADirectory(
            repo_path.display().to_string(),
        ));
    }

    let mut doc_files = discovery::discover_doc_files(repo_path);

    // Optionally read and hash content (for staleness detection)
    if compute_hashes {
        for doc in &mut doc_files {
            let _ = read_and_hash(doc);
        }
    }

    // NOTE: generated flag is path-based only (set in discover_doc_files).
    // Content-based frontmatter override was removed to unify behavior
    // between `docs list` and `orient`. See architectural lock in
    // docs/design/documentation-semantic-facts.md.

    // Build entries
    let entries: Vec<DocInventoryEntry> = doc_files
        .iter()
        .map(|f| DocInventoryEntry {
            path: f.relative_path.clone(),
            kind: f.kind.as_str().to_string(),
            generated: f.generated,
            content_hash: f.content_hash.clone(),
        })
        .collect();

    // Count by kind
    let mut counts_by_kind: HashMap<String, usize> = HashMap::new();
    for entry in &entries {
        *counts_by_kind.entry(entry.kind.clone()).or_insert(0) += 1;
    }

    let generated_count = entries.iter().filter(|e| e.generated).count();

    Ok(DocInventoryResult {
        entries,
        counts_by_kind,
        generated_count,
    })
}

/// Extract semantic facts from a repository's documentation.
///
/// This is the top-level facade that orchestrates:
/// 1. Discovery of doc/config files
/// 2. Classification of each file
/// 3. Reading and hashing content
/// 4. Running extractors
/// 5. Aggregating results
///
/// For finer control, use the individual modules directly.
pub fn extract_semantic_facts(repo_path: &Path) -> Result<ExtractionResult, DocFactsError> {
    if !repo_path.is_dir() {
        return Err(DocFactsError::NotADirectory(
            repo_path.display().to_string(),
        ));
    }

    // Step 1: Discover candidate files
    let mut doc_files = discovery::discover_doc_files(repo_path);

    // Step 2: Read content and compute hashes
    let mut warnings = Vec::new();
    for doc in &mut doc_files {
        match read_and_hash(doc) {
            Ok(()) => {}
            Err(e) => {
                warnings.push(ExtractionWarning {
                    file: doc.relative_path.clone(),
                    message: format!("failed to read: {}", e),
                });
            }
        }
    }

    // Step 3: Update generated flag from content analysis.
    //
    // Path-based detection is provisional and NOT strong enough evidence alone.
    // Content analysis is authoritative:
    //   - Explicit frontmatter (generated: true/false) → use that value
    //   - Readable content but silent on generated → NOT generated (path alone insufficient)
    //   - Unreadable content → keep path-based guess (no better signal available)
    for doc in &mut doc_files {
        if let Some(content) = &doc.content {
            match classification::get_generated_from_frontmatter(content) {
                Some(explicit) => {
                    // Frontmatter is explicit, use it
                    doc.generated = explicit;
                }
                None => {
                    // Content is readable but silent on generated status.
                    // Path alone is not strong enough provenance to mark as generated.
                    doc.generated = false;
                }
            }
        }
        // If content is None (unreadable), keep path-based provisional value
    }

    // Step 4: Run extractors on each file
    let mut all_facts = Vec::new();
    for doc in &doc_files {
        let facts = extractors::extract_from_file(doc);
        all_facts.extend(facts);
    }

    // Step 5: Compute summary statistics
    let files_scanned = doc_files.len();
    let mut files_by_kind: HashMap<DocKind, usize> = HashMap::new();
    let mut generated_docs_count = 0;

    for doc in &doc_files {
        *files_by_kind.entry(doc.kind).or_insert(0) += 1;
        if doc.generated {
            generated_docs_count += 1;
        }
    }

    Ok(ExtractionResult {
        facts: all_facts,
        files_scanned,
        files_by_kind,
        generated_docs_count,
        warnings,
    })
}

/// Read file content and compute SHA-256 hash.
fn read_and_hash(doc: &mut DocFile) -> Result<(), std::io::Error> {
    let content = fs::read_to_string(&doc.path)?;

    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    let hash_hex = hex::encode(hash);

    doc.content = Some(content);
    doc.content_hash = Some(hash_hex);

    Ok(())
}

// Re-export hex for internal use
mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut result = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            result.push(HEX_CHARS[(b >> 4) as usize] as char);
            result.push(HEX_CHARS[(b & 0x0f) as usize] as char);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    fn create_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn extract_from_repo_with_readme() {
        let dir = tempdir().unwrap();
        create_file(
            dir.path(),
            "README.md",
            "# Test\n\n<!-- rg:replaces old-module -->\n",
        );

        let result = extract_semantic_facts(dir.path()).unwrap();

        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].fact_kind, FactKind::ReplacementFor);
    }

    #[test]
    fn extract_from_repo_with_docker_compose() {
        let dir = tempdir().unwrap();
        create_file(
            dir.path(),
            "docker-compose.yml",
            "services:\n  api:\n    image: api:latest\n",
        );

        let result = extract_semantic_facts(dir.path()).unwrap();

        assert_eq!(result.files_scanned, 1);
        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].fact_kind, FactKind::EnvironmentSurface);
    }

    #[test]
    fn extract_detects_generated_from_frontmatter() {
        let dir = tempdir().unwrap();
        create_file(
            dir.path(),
            "src/core/MAP.md",
            "---\ngenerated_by: rgistr\n---\n# Core\n\n<!-- rg:replaces legacy -->\n",
        );

        let result = extract_semantic_facts(dir.path()).unwrap();

        assert_eq!(result.generated_docs_count, 1);
        assert!(result.facts[0].generated);
    }

    #[test]
    fn extract_handles_unreadable_files() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "README.md", "# OK");
        // Create a directory with the same name as a file pattern
        // This simulates an unreadable situation
        fs::create_dir_all(dir.path().join("docs")).unwrap();
        create_file(dir.path(), "docs/guide.md", "# Guide");

        let result = extract_semantic_facts(dir.path()).unwrap();

        // Should succeed even if some paths are tricky
        assert!(result.files_scanned >= 1);
    }

    #[test]
    fn error_on_non_directory() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        File::create(&file_path).unwrap();

        let result = extract_semantic_facts(&file_path);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DocFactsError::NotADirectory(_)));
    }

    #[test]
    fn files_by_kind_counts() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "README.md", "# Readme");
        create_file(dir.path(), "ARCHITECTURE.md", "# Arch");
        create_file(dir.path(), ".env", "FOO=bar");
        create_file(dir.path(), ".env.production", "FOO=prod");

        let result = extract_semantic_facts(dir.path()).unwrap();

        assert_eq!(result.files_by_kind.get(&DocKind::Readme), Some(&1));
        assert_eq!(result.files_by_kind.get(&DocKind::Architecture), Some(&1));
        assert_eq!(result.files_by_kind.get(&DocKind::Config), Some(&2));
    }

    #[test]
    fn authored_map_md_explicit_false_not_generated() {
        // P2 fix: MAP.md with explicit `generated: false` is not marked as generated
        let dir = tempdir().unwrap();
        create_file(
            dir.path(),
            "src/core/MAP.md",
            "---\ngenerated: false\ntitle: Core Module\n---\n# Core\n\nAuthored documentation.\n",
        );

        let result = extract_semantic_facts(dir.path()).unwrap();

        assert_eq!(result.files_scanned, 1);
        // Frontmatter `generated: false` overrides path-based detection
        assert_eq!(result.generated_docs_count, 0);
    }

    #[test]
    fn authored_map_md_silent_frontmatter_not_generated() {
        // P2 fix: MAP.md with silent frontmatter (no generated field) is NOT generated.
        // Path alone is not strong enough provenance.
        let dir = tempdir().unwrap();
        create_file(
            dir.path(),
            "src/core/MAP.md",
            "---\ntitle: Core Module\nauthor: human\n---\n# Core\n\nAuthored documentation.\n",
        );

        let result = extract_semantic_facts(dir.path()).unwrap();

        assert_eq!(result.files_scanned, 1);
        // Silent frontmatter means no evidence of generation → not generated
        assert_eq!(result.generated_docs_count, 0);
    }

    #[test]
    fn authored_map_md_no_frontmatter_not_generated() {
        // P2 fix: MAP.md with no frontmatter at all is NOT generated.
        // Readable content without generation evidence → authored.
        let dir = tempdir().unwrap();
        create_file(
            dir.path(),
            "src/core/MAP.md",
            "# Core Module\n\nThis is human-written documentation.\n",
        );

        let result = extract_semantic_facts(dir.path()).unwrap();

        assert_eq!(result.files_scanned, 1);
        // No frontmatter, content readable → not generated
        assert_eq!(result.generated_docs_count, 0);
    }

    #[test]
    fn nested_env_file_has_module_scope() {
        // P1 fix: nested .env files use parent directory as subject, not repo root
        let dir = tempdir().unwrap();
        create_file(dir.path(), "frontend/web/.env.prod", "API_URL=https://prod.api.example.com");

        let result = extract_semantic_facts(dir.path()).unwrap();

        assert_eq!(result.facts.len(), 1);
        assert_eq!(result.facts[0].subject_ref, "frontend/web");
    }
}
