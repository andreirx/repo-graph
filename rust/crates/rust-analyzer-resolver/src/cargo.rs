//! Cargo.toml discovery and file grouping.
//!
//! rust-analyzer requires a Cargo context to resolve types. Files must be
//! grouped by their nearest Cargo.toml ancestor, and one rust-analyzer
//! session started per Cargo context.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use enrichment::EligibleEdge;

/// Group eligible edges by their nearest Cargo.toml ancestor directory.
///
/// Returns a map from Cargo root path to edges in that context.
/// Falls back to repo root if no Cargo.toml is found.
pub fn group_by_cargo_root(
    repo_root: &Path,
    edges: &[EligibleEdge],
) -> HashMap<PathBuf, Vec<EligibleEdge>> {
    let mut cache: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut groups: HashMap<PathBuf, Vec<EligibleEdge>> = HashMap::new();

    for edge in edges {
        let file_path = Path::new(&edge.source_file_path);
        let cargo_root = find_cargo_root(repo_root, file_path, &mut cache);

        groups
            .entry(cargo_root)
            .or_default()
            .push(edge.clone());
    }

    groups
}

/// Find the nearest Cargo.toml ancestor for a file.
///
/// Walks upward from the file's directory to repo_root.
/// Returns repo_root if no Cargo.toml is found.
fn find_cargo_root(
    repo_root: &Path,
    file_rel_path: &Path,
    cache: &mut HashMap<PathBuf, PathBuf>,
) -> PathBuf {
    // Get the directory containing the file
    let file_dir = file_rel_path.parent().unwrap_or(Path::new(""));

    // Check cache first
    if let Some(cached) = cache.get(file_dir) {
        return cached.clone();
    }

    // Walk upward from file directory
    let mut current = file_dir.to_path_buf();
    let mut checked: Vec<PathBuf> = Vec::new();

    loop {
        checked.push(current.clone());

        // Check for Cargo.toml at this level
        let abs_dir = if current.as_os_str().is_empty() {
            repo_root.to_path_buf()
        } else {
            repo_root.join(&current)
        };

        if abs_dir.join("Cargo.toml").exists() {
            // Found it — cache all checked paths
            for dir in &checked {
                cache.insert(dir.clone(), abs_dir.clone());
            }
            return abs_dir;
        }

        // Move up one directory
        if current.as_os_str().is_empty() {
            break;
        }

        current = current.parent().unwrap_or(Path::new("")).to_path_buf();
    }

    // No Cargo.toml found — fall back to repo root
    for dir in &checked {
        cache.insert(dir.clone(), repo_root.to_path_buf());
    }
    repo_root.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use enrichment::{EnrichmentLanguage, UnresolvedCategory};
    use std::fs;
    use tempfile::TempDir;

    fn make_edge(file_path: &str) -> EligibleEdge {
        EligibleEdge {
            edge_uid: format!("edge-{}", file_path.replace('/', "-")),
            snapshot_uid: "snap-1".to_string(),
            repo_uid: "repo-1".to_string(),
            source_node_uid: "node-1".to_string(),
            target_key: "obj.method".to_string(),
            source_file_path: file_path.to_string(),
            line_start: 10,
            col_start: 5,
            category: UnresolvedCategory::CallsObjMethodNeedsTypeInfo,
            language: EnrichmentLanguage::Rust,
        }
    }

    #[test]
    fn test_group_by_cargo_root_single_workspace() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create Cargo.toml at root
        fs::write(repo_root.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        fs::create_dir_all(repo_root.join("src")).unwrap();
        fs::create_dir_all(repo_root.join("src/nested")).unwrap();

        let edges = vec![
            make_edge("src/main.rs"),
            make_edge("src/lib.rs"),
            make_edge("src/nested/mod.rs"),
        ];

        let groups = group_by_cargo_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key(repo_root));
        assert_eq!(groups[repo_root].len(), 3);
    }

    #[test]
    fn test_group_by_cargo_root_multiple_crates() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create two separate crates
        fs::create_dir_all(repo_root.join("crate_a/src")).unwrap();
        fs::create_dir_all(repo_root.join("crate_b/src")).unwrap();
        fs::write(
            repo_root.join("crate_a/Cargo.toml"),
            "[package]\nname = \"a\"",
        )
        .unwrap();
        fs::write(
            repo_root.join("crate_b/Cargo.toml"),
            "[package]\nname = \"b\"",
        )
        .unwrap();

        let edges = vec![
            make_edge("crate_a/src/lib.rs"),
            make_edge("crate_b/src/lib.rs"),
        ];

        let groups = group_by_cargo_root(repo_root, &edges);

        assert_eq!(groups.len(), 2);
        assert!(groups.contains_key(&repo_root.join("crate_a")));
        assert!(groups.contains_key(&repo_root.join("crate_b")));
    }

    #[test]
    fn test_group_by_cargo_root_no_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // No Cargo.toml anywhere
        fs::create_dir_all(repo_root.join("src")).unwrap();

        let edges = vec![make_edge("src/main.rs")];

        let groups = group_by_cargo_root(repo_root, &edges);

        // Falls back to repo root
        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key(repo_root));
    }
}
