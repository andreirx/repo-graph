//! Documentation file discovery.
//!
//! Finds candidate documentation and configuration files in a repository
//! for semantic fact extraction.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::classification::classify_doc_kind;
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
            // Recurse into every non-skipped directory. Candidacy (including
            // whether a file sits inside a docs/doc/design tree) is decided
            // per-file from its repo-relative path, not from the recursion arm.
            discover_recursive(repo_root, &path, depth + 1, results, seen);
        } else if path.is_file() {
            let rel_path = match path.strip_prefix(repo_root).ok().and_then(|p| p.to_str()) {
                Some(r) => r,
                None => continue,
            };
            if is_doc_candidate(rel_path, file_name) {
                if let Some(doc_file) = make_doc_file(repo_root, &path) {
                    results.push(doc_file);
                }
            }
        }
    }
}

/// Documentation file extensions admitted inside a docs tree (SELF-POLLUTION-1
/// §3): Markdown plus reStructuredText / plain-text, so django's `docs/**.txt`
/// and Sphinx `.rst` render, not just `.md`.
const DOC_EXTENSIONS: &[&str] = &[".md", ".txt", ".rst"];

/// Directory names that open a "documentation tree": any file with a doc
/// extension *anywhere below* one of these (not just its immediate child) is a
/// candidate. `doc` (singular, e.g. leveldb `doc/impl.md`) is included alongside
/// `docs`/`design`.
const DOC_TREE_DIRS: &[&str] = &["docs", "doc", "design"];

/// Is any ANCESTOR directory of `rel_path` a documentation-tree root? Checks the
/// path components excluding the final basename, so `docs/ref/models/fields.txt`
/// (django) and `doc/impl.md` (leveldb) both qualify.
fn in_docs_tree(rel_path: &str) -> bool {
    let mut components: Vec<&str> = rel_path.split('/').collect();
    components.pop(); // drop the basename; only ancestors count
    components.iter().any(|c| DOC_TREE_DIRS.contains(c))
}

/// Check if a file is a documentation candidate, from its repo-relative path.
///
/// NOTE on `.env*`: it IS a candidate here because the semantic-fact extractor
/// (`docs extract`) still derives an `EnvironmentSurface` hint from it. It is NOT
/// a *document*, so `discover_doc_inventory` drops it from the inventory /
/// orient surface (SELF-POLLUTION-1 §3) — the exclusion the slice requires lives
/// at that layer, not here.
fn is_doc_candidate(rel_path: &str, file_name: &str) -> bool {
    // .env files (including .env.production, .env.development, etc.) — kept for
    // the extractor; filtered out of the inventory downstream.
    if file_name.starts_with(".env") {
        return true;
    }

    // Exact matches for known doc files.
    let known_docs = [
        "README.md",
        "README",
        "ARCHITECTURE.md",
        "CONTRIBUTING.md",
        "CHANGELOG.md",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ];
    if known_docs.contains(&file_name) {
        return true;
    }

    // `rmap map` sidecars anywhere. Discovered so they can be COUNTED and, on
    // opt-in, listed; whether they are self-generated (excluded by default) is
    // decided later from the first-line marker, not from the name.
    if crate::self_generated::has_map_sidecar_name(rel_path) {
        return true;
    }

    // Doc-extension files inside a docs/doc/design tree.
    if DOC_EXTENSIONS.iter().any(|e| file_name.ends_with(e)) && in_docs_tree(rel_path) {
        return true;
    }

    false
}

/// Create a DocFile from a discovered path.
///
/// `generated` starts `false` — discovery has NO generation evidence yet. A bare
/// filename (`MAP.md`) is not evidence (honesty rule): the authoritative generation
/// signal is set downstream, where content is available — the marker-gated
/// [`crate::self_generated::is_self_generated`] in `discover_doc_inventory`, or the
/// frontmatter analysis in `extract_semantic_facts`. This keeps the marker-gated
/// predicate the SOLE generation classifier (review-5 finding 2).
fn make_doc_file(repo_root: &Path, path: &Path) -> Option<DocFile> {
    let relative_path = path.strip_prefix(repo_root).ok()?.to_str()?.to_string();

    let kind = classify_doc_kind(&relative_path);

    Some(DocFile {
        path: path.to_path_buf(),
        relative_path,
        kind,
        generated: false,
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
    fn env_files_still_candidates_for_extraction() {
        // `.env*` remains a discovery candidate (the semantic-fact extractor uses
        // it); the inventory layer is what drops it as a "document"
        // (see `discover_doc_inventory` — env_files_excluded_from_inventory).
        let dir = tempdir().unwrap();
        create_file(dir.path(), ".env", "FOO=bar");
        create_file(dir.path(), ".env.production", "FOO=prod");

        let files = discover_doc_files(dir.path());
        let paths: Vec<_> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(paths.contains(&".env"));
        assert!(paths.contains(&".env.production"));
    }

    #[test]
    fn admits_txt_and_rst_under_docs_tree() {
        // django ships docs as deeply-nested `.txt`; leveldb uses `doc/` (singular).
        let dir = tempdir().unwrap();
        create_file(dir.path(), "docs/ref/models/fields.txt", "Model fields");
        create_file(dir.path(), "docs/index.rst", "Index");
        create_file(dir.path(), "doc/impl.md", "impl notes");
        // A .txt OUTSIDE any docs tree is not a doc.
        create_file(dir.path(), "src/notes.txt", "scratch");

        let files = discover_doc_files(dir.path());
        let paths: Vec<_> = files.iter().map(|f| f.relative_path.as_str()).collect();

        assert!(paths.contains(&"docs/ref/models/fields.txt"), "{paths:?}");
        assert!(paths.contains(&"docs/index.rst"), "{paths:?}");
        assert!(paths.contains(&"doc/impl.md"), "{paths:?}");
        assert!(!paths.contains(&"src/notes.txt"), "{paths:?}");
    }

    #[test]
    fn discovers_map_md_in_subdirectory() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "src/core/MAP.md", "# Core module");

        let files = discover_doc_files(dir.path());

        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.ends_with("MAP.md"));
        // review-5 finding 2: discovery no longer asserts generation from the bare
        // `MAP.md` name — there is no marker here, so it is NOT generated. The
        // marker-gated predicate (in `discover_doc_inventory`) is the sole classifier.
        assert!(!files[0].generated);
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
