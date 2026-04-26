//! Documentation file discovery.
//!
//! Finds candidate documentation and configuration files in a repository
//! for semantic fact extraction.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::classification::{classify_doc_kind, is_generated_by_path};
use crate::types::DocFile;

/// Maximum directory depth for recursive discovery.
const MAX_DEPTH: usize = 10;

/// Directories to skip during discovery.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    ".next",
    "__pycache__",
    ".venv",
    "venv",
    "vendor",
];

/// Discover documentation and configuration files in a repository.
///
/// Returns a list of `DocFile` entries with classification but without
/// content (content is loaded separately during extraction).
pub fn discover_doc_files(repo_path: &Path) -> Vec<DocFile> {
    let mut results = Vec::new();
    let mut seen = HashSet::new();

    discover_recursive(repo_path, repo_path, 0, &mut results, &mut seen);

    results
}

fn discover_recursive(
    repo_root: &Path,
    current: &Path,
    depth: usize,
    results: &mut Vec<DocFile>,
    seen: &mut HashSet<PathBuf>,
) {
    if depth > MAX_DEPTH {
        return;
    }

    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip if already seen (symlink loops)
        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !seen.insert(canonical) {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if path.is_dir() {
            // Skip excluded directories
            if SKIP_DIRS.contains(&file_name) {
                continue;
            }

            // Special case: docs/ directory gets deeper exploration
            if file_name == "docs" || file_name == "design" {
                discover_recursive(repo_root, &path, depth + 1, results, seen);
            } else {
                // For other dirs, only look for MAP.md and .env files
                discover_recursive(repo_root, &path, depth + 1, results, seen);
            }
        } else if path.is_file() {
            if is_doc_candidate(&path, file_name) {
                if let Some(doc_file) = make_doc_file(repo_root, &path) {
                    results.push(doc_file);
                }
            }
        }
    }
}

/// Check if a file is a documentation candidate.
fn is_doc_candidate(path: &Path, file_name: &str) -> bool {
    // Exact matches for known doc files
    let known_docs = [
        "README.md",
        "README",
        "ARCHITECTURE.md",
        "CONTRIBUTING.md",
        "CHANGELOG.md",
        "MAP.md",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ];

    if known_docs.contains(&file_name) {
        return true;
    }

    // .env files (including .env.production, .env.development, etc.)
    if file_name.starts_with(".env") {
        return true;
    }

    // Markdown files in docs/ directories
    if let Some(parent) = path.parent() {
        if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
            if (parent_name == "docs" || parent_name == "design")
                && file_name.ends_with(".md")
            {
                return true;
            }
        }
    }

    false
}

/// Create a DocFile from a discovered path.
fn make_doc_file(repo_root: &Path, path: &Path) -> Option<DocFile> {
    let relative_path = path
        .strip_prefix(repo_root)
        .ok()?
        .to_str()?
        .to_string();

    let kind = classify_doc_kind(&relative_path);
    let generated = is_generated_by_path(&relative_path);

    Some(DocFile {
        path: path.to_path_buf(),
        relative_path,
        kind,
        generated,
        content: None,
        content_hash: None,
    })
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
    fn discovers_readme_at_root() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "README.md", "# Test");

        let files = discover_doc_files(dir.path());

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "README.md");
    }

    #[test]
    fn discovers_docker_compose() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "docker-compose.yml", "version: '3'");

        let files = discover_doc_files(dir.path());

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "docker-compose.yml");
    }

    #[test]
    fn discovers_env_files() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), ".env", "FOO=bar");
        create_file(dir.path(), ".env.production", "FOO=prod");

        let files = discover_doc_files(dir.path());

        assert_eq!(files.len(), 2);
        let paths: Vec<_> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&".env"));
        assert!(paths.contains(&".env.production"));
    }

    #[test]
    fn discovers_map_md_in_subdirectory() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "src/core/MAP.md", "# Core module");

        let files = discover_doc_files(dir.path());

        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.ends_with("MAP.md"));
        assert!(files[0].generated); // MAP.md is marked generated by path
    }

    #[test]
    fn skips_node_modules() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "README.md", "# Root");
        create_file(dir.path(), "node_modules/pkg/README.md", "# Package");

        let files = discover_doc_files(dir.path());

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "README.md");
    }

    #[test]
    fn discovers_docs_directory_markdown() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "docs/design.md", "# Design");
        create_file(dir.path(), "docs/api.md", "# API");

        let files = discover_doc_files(dir.path());

        assert_eq!(files.len(), 2);
    }
}
