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

/// The result of one discovery walk: the candidate files, and how many MARKUP files the walk
/// visited but the rule refused (RG-REQ-008-L06 — a widened scope counts its own refusals).
#[derive(Debug)]
pub struct DocDiscovery {
    /// Candidate documentation/configuration files (classified, content not loaded).
    pub files: Vec<DocFile>,
    /// Files visited by the walk whose lowercased name ends with one of
    /// [`UNSCANNED_MARKUP_EXTENSIONS`] and that [`is_doc_candidate`] refused — markup prose
    /// outside a docs tree. `.txt` is deliberately NOT counted (D-DD1-002: a `.txt` outside a
    /// docs tree is not known to be prose — `CMakeLists.txt`, `requirements.txt`). Files under
    /// skipped directories or beyond `MAX_DEPTH` are never visited and never counted.
    pub unscanned_markup_outside_docs_tree: usize,
}

/// Discover documentation and configuration files in a repository.
///
/// Returns the `DocFile` entries (classified, without content — content is loaded separately
/// during extraction) and the count of refused markup files.
pub fn discover_doc_files(repo_path: &Path) -> DocDiscovery {
    let mut results = DocDiscovery {
        files: Vec::new(),
        unscanned_markup_outside_docs_tree: 0,
    };
    let mut seen = HashSet::new();

    discover_recursive(repo_path, repo_path, 0, &mut results, &mut seen);

    results
}

fn discover_recursive(
    repo_root: &Path,
    current: &Path,
    depth: usize,
    results: &mut DocDiscovery,
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
                    results.files.push(doc_file);
                }
            } else if is_markup_name(file_name)
                && !crate::self_generated::is_tool_state_path(rel_path)
            {
                // A refused markup file is counted as the authors' unscanned prose — except
                // rmap's own `.rgr/` tool state, decided by the ONE exhaust predicate
                // (RG-REQ-011-L07), never a second definition.
                results.unscanned_markup_outside_docs_tree += 1;
            }
        }
    }
}

/// RG-REQ-008-L01 (ratified 2026-09-14): the documentation name STEMS. A file whose stem is one
/// of these is a document anywhere in the tree, with or without a documentation extension. The
/// ONE spelling of the stem set — classification (`readme` by stem) and the orientation
/// recommendation (daemon-runtime `modules_method`) read it through [`doc_name_stem`].
pub const DOC_NAME_STEMS: &[&str] = &[
    "readme",
    "contributing",
    "changelog",
    "architecture",
    "design",
    "overview",
    "install",
    "building",
    "authors",
    "news",
];

/// RG-REQ-008-L07: the subset of [`DOC_NAME_STEMS`] that names an orientation document
/// (a file the recommendation line may point at by name). Tested to be a subset.
pub const ORIENTATION_STEMS: &[&str] = &["architecture", "design", "overview", "contributing"];

/// RG-REQ-008-L01: documentation file extensions. A name ending with one of these
/// (case-insensitive, D-DD1-003) is admitted inside a docs tree, and one of them is stripped
/// to find a name's stem. Markdown, reStructuredText, plain text and AsciiDoc.
pub const DOC_EXTENSIONS: &[&str] = &[".md", ".markdown", ".txt", ".rst", ".adoc"];

/// RG-REQ-008-L06 / D-DD1-002: the MARKUP extensions whose refusal the walk counts. `.txt` is
/// deliberately absent — a `.txt` outside a docs tree is not known to be prose.
const UNSCANNED_MARKUP_EXTENSIONS: &[&str] = &[".md", ".markdown", ".rst", ".adoc"];

/// Directory names that open a "documentation tree": any file with a doc
/// extension *anywhere below* one of these (not just its immediate child) is a
/// candidate. `doc` (singular, e.g. leveldb `doc/impl.md`) is included alongside
/// `docs`/`design`. Matched as whole path components, case-sensitively (unchanged).
const DOC_TREE_DIRS: &[&str] = &["docs", "doc", "design"];

/// RG-REQ-008-L01: multi-component conventions that open a documentation tree — the Maven
/// site convention `src/site/**` (a component `site` whose IMMEDIATE parent component is
/// `src`; hadoop's `hadoop-*/src/site/markdown` guides). Matched as consecutive whole
/// ancestor components.
const DOC_TREE_CONVENTIONS: &[&str] = &["src/site"];

/// The discovery rule as data, so `docs list --json` can state what the header's
/// "not scanned (--json for the rule)" line refers to (RG-REQ-008-L06). Every field is the
/// constant the walk itself reads — the rule is stated, never re-spelled.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DiscoveryRule {
    /// Names whose stem (name minus one documentation extension) is one of these are documents anywhere.
    pub stems: &'static [&'static str],
    /// Documentation extensions (admitted inside a doc tree; stripped to find a stem).
    pub extensions: &'static [&'static str],
    /// Ancestor directory components that open a doc tree.
    pub doc_tree_dirs: &'static [&'static str],
    /// Consecutive ancestor component sequences that open a doc tree.
    pub doc_tree_conventions: &'static [&'static str],
    /// Extensions whose refused files are counted as unscanned markup.
    pub unscanned_extensions: &'static [&'static str],
}

/// The discovery rule this build applies (the constants [`discover_doc_files`] reads).
pub fn discovery_rule() -> DiscoveryRule {
    DiscoveryRule {
        stems: DOC_NAME_STEMS,
        extensions: DOC_EXTENSIONS,
        doc_tree_dirs: DOC_TREE_DIRS,
        doc_tree_conventions: DOC_TREE_CONVENTIONS,
        unscanned_extensions: UNSCANNED_MARKUP_EXTENSIONS,
    }
}

/// The [`DOC_NAME_STEMS`] entry `file_name` names, or `None`.
///
/// Case-insensitive on both parts (D-DD1-003): the name is lowercased, ONE trailing
/// [`DOC_EXTENSIONS`] entry is stripped if present, and the remainder must EQUAL a stem.
/// So `README`, `readme.txt`, `README.TXT`, `INSTALL`, `BUILDING.txt`, `ChangeLog` match;
/// `README.html`, `install.sh`, `readme.txt.bak` do not (their remainder is the whole name).
pub fn doc_name_stem(file_name: &str) -> Option<&'static str> {
    let lower = file_name.to_lowercase();
    let stem = DOC_EXTENSIONS
        .iter()
        .find_map(|e| lower.strip_suffix(e))
        .unwrap_or(&lower);
    DOC_NAME_STEMS.iter().copied().find(|s| *s == stem)
}

/// Does `file_name` end (case-insensitively) with a [`DOC_EXTENSIONS`] entry?
fn has_doc_extension(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    DOC_EXTENSIONS.iter().any(|e| lower.ends_with(e))
}

/// Does `file_name` end (case-insensitively) with an [`UNSCANNED_MARKUP_EXTENSIONS`] entry?
fn is_markup_name(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    UNSCANNED_MARKUP_EXTENSIONS
        .iter()
        .any(|e| lower.ends_with(e))
}

/// Is any ANCESTOR directory of `rel_path` a documentation-tree root? Checks the
/// path components excluding the final basename, so `docs/ref/models/fields.txt`
/// (django), `doc/impl.md` (leveldb) and `x/src/site/markdown/a.md` (hadoop) qualify.
fn in_docs_tree(rel_path: &str) -> bool {
    let mut components: Vec<&str> = rel_path.split('/').collect();
    components.pop(); // drop the basename; only ancestors count
    if components.iter().any(|c| DOC_TREE_DIRS.contains(c)) {
        return true;
    }
    DOC_TREE_CONVENTIONS.iter().any(|conv| {
        let parts: Vec<&str> = conv.split('/').collect();
        components
            .windows(parts.len())
            .any(|w| w == parts.as_slice())
    })
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

    // Compose files by exact name (config for the extractor, not prose).
    let known_docs = [
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

    // RG-REQ-008-L01: a documentation stem anywhere, with or without a doc extension.
    if doc_name_stem(file_name).is_some() {
        return true;
    }

    // Doc-extension files inside a doc tree (docs/doc/design or src/site).
    if has_doc_extension(file_name) && in_docs_tree(rel_path) {
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

        let files = discover_doc_files(dir.path()).files;

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "README.md");
    }

    #[test]
    fn discovers_docker_compose() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "docker-compose.yml", "version: '3'");

        let files = discover_doc_files(dir.path()).files;

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

        let files = discover_doc_files(dir.path()).files;
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

        let files = discover_doc_files(dir.path()).files;
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

        let files = discover_doc_files(dir.path()).files;

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

        let files = discover_doc_files(dir.path()).files;

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "README.md");
    }

    #[test]
    fn discovers_docs_directory_markdown() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "docs/design.md", "# Design");
        create_file(dir.path(), "docs/api.md", "# API");

        let files = discover_doc_files(dir.path()).files;

        assert_eq!(files.len(), 2);
    }

    fn discovered_paths(dir: &Path) -> Vec<String> {
        let mut paths: Vec<String> = discover_doc_files(dir)
            .files
            .into_iter()
            .map(|f| f.relative_path)
            .collect();
        paths.sort();
        paths
    }

    // DOCS-DISCOVERY-1 (RG-REQ-008-L01): a documentation STEM is a document anywhere, with or
    // without a documentation extension — hadoop's root `README.txt`/`BUILDING.txt`, django's
    // `README.rst`/`CONTRIBUTING.rst`/`AUTHORS`/`INSTALL`, leveldb's `NEWS`, GNU `ChangeLog`.
    #[test]
    fn stem_rule_admits_documentation_stems_with_or_without_a_doc_extension() {
        let dir = tempdir().unwrap();
        let admitted = [
            "README.txt",
            "README.rst",
            "CONTRIBUTING.rst",
            "INSTALL",
            "AUTHORS",
            "NEWS",
            "BUILDING.txt",
            "ChangeLog",
            "changelog.md",
            "Architecture.md",
            "OVERVIEW.adoc",
            "design.markdown",
            "src/main/native/AUTHORS",
            "board/arm/foundation-v8/readme.txt",
        ];
        for p in admitted {
            create_file(dir.path(), p, "prose");
        }
        let paths = discovered_paths(dir.path());
        for p in admitted {
            assert!(
                paths.iter().any(|x| x == p),
                "{p} must be admitted: {paths:?}"
            );
        }
        assert_eq!(paths.len(), admitted.len(), "{paths:?}");
    }

    #[test]
    fn stem_with_a_non_doc_extension_is_refused() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "README.html", "<p>x</p>");
        create_file(dir.path(), "install.sh", "#!/bin/sh");
        create_file(dir.path(), "readme.txt.bak", "old");
        create_file(dir.path(), "authors.json", "[]");
        assert!(discovered_paths(dir.path()).is_empty());
        assert_eq!(doc_name_stem("README.html"), None);
        assert_eq!(doc_name_stem("install.sh"), None);
        assert_eq!(doc_name_stem("readme.txt.bak"), None);
    }

    // D-DD1-003 (ratified 2026-09-22): stem AND extension are compared case-insensitively.
    #[test]
    fn stem_and_extension_match_case_insensitively() {
        let dir = tempdir().unwrap();
        // Separate directories: a case-insensitive filesystem (macOS) would merge same-name files.
        for p in [
            "Readme.md",
            "a/README.TXT",
            "b/README.MD",
            "docs/NOTES.MD",
            "extras/README.TXT",
        ] {
            create_file(dir.path(), p, "prose");
        }
        assert_eq!(
            discovered_paths(dir.path()),
            vec![
                "Readme.md",
                "a/README.TXT",
                "b/README.MD",
                "docs/NOTES.MD",
                "extras/README.TXT"
            ]
        );
        assert_eq!(doc_name_stem("README.TXT"), Some("readme"));
        assert_eq!(doc_name_stem("Readme.md"), Some("readme"));
        assert_eq!(doc_name_stem("ChangeLog"), Some("changelog"));
        assert_eq!(doc_name_stem("BUILDING.txt"), Some("building"));
    }

    #[test]
    fn adoc_and_markdown_admitted_inside_a_docs_tree() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "docs/a.adoc", "= A");
        create_file(dir.path(), "doc/b.markdown", "# B");
        create_file(dir.path(), "docs/manual/manual.adoc", "= Manual");
        // Outside a docs tree the extension alone admits nothing.
        create_file(dir.path(), "src/c.adoc", "= C");
        assert_eq!(
            discovered_paths(dir.path()),
            vec!["doc/b.markdown", "docs/a.adoc", "docs/manual/manual.adoc"]
        );
    }

    // RG-REQ-008-L01: the `src/site/**` convention (component `site` whose IMMEDIATE parent
    // component is `src`) opens a doc tree — hadoop's 519 `src/site/markdown` guides.
    #[test]
    fn src_site_convention_opens_a_docs_tree() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "x/src/site/markdown/a.md", "# A");
        create_file(dir.path(), "src/site/b.md", "# B");
        create_file(dir.path(), "site/a.md", "# no");
        create_file(dir.path(), "src/a.md", "# no");
        create_file(dir.path(), "srcx/site/a.md", "# no");
        create_file(dir.path(), "src/sitex/a.md", "# no");
        create_file(dir.path(), "src/other/site/a.md", "# no");
        assert_eq!(
            discovered_paths(dir.path()),
            vec!["src/site/b.md", "x/src/site/markdown/a.md"]
        );
    }

    // RG-REQ-008-L06 / D-DD1-002: the walk counts every refused MARKUP file (`.md`, `.markdown`,
    // `.rst`, `.adoc`, case-insensitive); a `.txt` outside a docs tree is never counted (it is not
    // known to be prose — `CMakeLists.txt`, `requirements.txt`); an admitted file is never counted.
    #[test]
    fn unscanned_counts_markup_outside_a_docs_tree_never_txt() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "README.md", "# admitted");
        create_file(dir.path(), "notes.md", "# refused");
        create_file(dir.path(), "CMakeLists.txt", "cmake_minimum_required()");
        create_file(dir.path(), "requirements.txt", "x==1");
        create_file(dir.path(), "docs/a.md", "# admitted");
        create_file(dir.path(), "src/MAP.md", "# sidecar, admitted");
        let d = discover_doc_files(dir.path());
        assert_eq!(d.unscanned_markup_outside_docs_tree, 1);

        create_file(dir.path(), ".github/SECURITY.MD", "# refused");
        create_file(dir.path(), "guide.markdown", "# refused");
        create_file(dir.path(), "pkg/notes.rst", "refused");
        create_file(dir.path(), "pkg/manual.adoc", "= refused");
        create_file(
            dir.path(),
            "node_modules/pkg/notes.md",
            "# skipped dir, never visited",
        );
        let d = discover_doc_files(dir.path());
        assert_eq!(d.unscanned_markup_outside_docs_tree, 5);
    }

    // RG-REQ-011-L07: rmap's own `.rgr/` tool-state directory is exhaust, never the authors'
    // refused prose — the count reads the one exhaust predicate (`is_tool_state_path`).
    #[test]
    fn unscanned_count_skips_the_rmap_tool_state_dir() {
        let dir = tempdir().unwrap();
        create_file(dir.path(), "notes.md", "# refused, counted");
        create_file(
            dir.path(),
            ".rgr/notes.md",
            "# rmap tool state, not counted",
        );
        let d = discover_doc_files(dir.path());
        assert_eq!(d.unscanned_markup_outside_docs_tree, 1);
        assert!(d.files.is_empty(), "{:?}", d.files);
    }

    #[test]
    fn orientation_stems_are_doc_name_stems() {
        for s in ORIENTATION_STEMS {
            assert!(DOC_NAME_STEMS.contains(s), "{s} is not a doc-name stem");
        }
        assert_eq!(
            DOC_NAME_STEMS,
            &[
                "readme",
                "contributing",
                "changelog",
                "architecture",
                "design",
                "overview",
                "install",
                "building",
                "authors",
                "news"
            ]
        );
        assert_eq!(
            DOC_EXTENSIONS,
            &[".md", ".markdown", ".txt", ".rst", ".adoc"]
        );
    }

    #[test]
    fn compose_files_still_admitted_by_exact_name() {
        let dir = tempdir().unwrap();
        for p in [
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ] {
            create_file(dir.path(), p, "services: {}");
        }
        create_file(dir.path(), "other.yaml", "x: 1");
        assert_eq!(
            discovered_paths(dir.path()),
            vec![
                "compose.yaml",
                "compose.yml",
                "docker-compose.yaml",
                "docker-compose.yml"
            ]
        );
    }
}
