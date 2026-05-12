//! TypeScript/JavaScript project detection and file grouping.
//!
//! tsserver requires a project context to resolve types. Files must be
//! grouped by their nearest config file ancestor, and one tsserver
//! session started per project context.
//!
//! # Detection Order (layered)
//!
//! 1. `tsconfig.json` — TypeScript project
//! 2. `jsconfig.json` — JavaScript project with TS tooling
//! 3. `package.json` — Node.js package boundary
//! 4. Repo root — standalone fallback
//!
//! The first match wins. This handles:
//! - Pure TypeScript projects
//! - JavaScript projects using TS tooling
//! - Mixed TS/JS workspaces
//! - Partial config coverage

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use enrichment::EligibleEdge;

/// Config file types in priority order.
const CONFIG_FILES: &[&str] = &["tsconfig.json", "jsconfig.json", "package.json"];

/// Group eligible edges by their nearest project config ancestor.
///
/// Returns a map from project root path to edges in that context.
/// Falls back to repo root if no config file is found.
pub fn group_by_project_root(
    repo_root: &Path,
    edges: &[EligibleEdge],
) -> HashMap<PathBuf, Vec<EligibleEdge>> {
    let mut cache: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut groups: HashMap<PathBuf, Vec<EligibleEdge>> = HashMap::new();

    for edge in edges {
        let file_path = Path::new(&edge.source_file_path);
        let project_root = find_project_root(repo_root, file_path, &mut cache);

        groups.entry(project_root).or_default().push(edge.clone());
    }

    groups
}

/// Find the nearest project config ancestor for a file.
///
/// Walks upward from the file's directory to repo_root, checking for
/// config files in priority order at each level.
/// Returns repo_root if no config file is found.
fn find_project_root(
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

        // Build absolute path for this directory
        let abs_dir = if current.as_os_str().is_empty() {
            repo_root.to_path_buf()
        } else {
            repo_root.join(&current)
        };

        // Check for config files in priority order
        for config_file in CONFIG_FILES {
            if abs_dir.join(config_file).exists() {
                // Found it — cache all checked paths
                for dir in &checked {
                    cache.insert(dir.clone(), abs_dir.clone());
                }
                return abs_dir;
            }
        }

        // Move up one directory
        if current.as_os_str().is_empty() {
            break;
        }

        current = current.parent().unwrap_or(Path::new("")).to_path_buf();
    }

    // No config file found — fall back to repo root
    for dir in &checked {
        cache.insert(dir.clone(), repo_root.to_path_buf());
    }
    repo_root.to_path_buf()
}

/// Detect the project config type for a directory.
///
/// Returns the config file name if found, None otherwise.
#[allow(dead_code)] // Kept for API completeness and future use
pub fn detect_config_type(project_root: &Path) -> Option<&'static str> {
    CONFIG_FILES
        .iter()
        .find(|&config_file| project_root.join(config_file).exists())
        .copied()
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
            language: EnrichmentLanguage::TypeScript,
        }
    }

    #[test]
    fn test_group_by_project_root_single_tsconfig() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create tsconfig.json at root
        fs::write(repo_root.join("tsconfig.json"), "{}").unwrap();
        fs::create_dir_all(repo_root.join("src")).unwrap();
        fs::create_dir_all(repo_root.join("src/nested")).unwrap();

        let edges = vec![
            make_edge("src/main.ts"),
            make_edge("src/lib.ts"),
            make_edge("src/nested/util.ts"),
        ];

        let groups = group_by_project_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key(repo_root));
        assert_eq!(groups[repo_root].len(), 3);
    }

    #[test]
    fn test_group_by_project_root_multiple_projects() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create two separate TS projects
        fs::create_dir_all(repo_root.join("packages/app/src")).unwrap();
        fs::create_dir_all(repo_root.join("packages/lib/src")).unwrap();
        fs::write(repo_root.join("packages/app/tsconfig.json"), "{}").unwrap();
        fs::write(repo_root.join("packages/lib/tsconfig.json"), "{}").unwrap();

        let edges = vec![
            make_edge("packages/app/src/main.ts"),
            make_edge("packages/lib/src/util.ts"),
        ];

        let groups = group_by_project_root(repo_root, &edges);

        assert_eq!(groups.len(), 2);
        assert!(groups.contains_key(&repo_root.join("packages/app")));
        assert!(groups.contains_key(&repo_root.join("packages/lib")));
    }

    #[test]
    fn test_group_by_project_root_jsconfig_fallback() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create jsconfig.json (no tsconfig)
        fs::write(repo_root.join("jsconfig.json"), "{}").unwrap();
        fs::create_dir_all(repo_root.join("src")).unwrap();

        let edges = vec![make_edge("src/main.js")];

        let groups = group_by_project_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key(repo_root));
    }

    #[test]
    fn test_group_by_project_root_package_json_fallback() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create only package.json (no tsconfig or jsconfig)
        fs::write(repo_root.join("package.json"), "{}").unwrap();
        fs::create_dir_all(repo_root.join("src")).unwrap();

        let edges = vec![make_edge("src/main.ts")];

        let groups = group_by_project_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key(repo_root));
    }

    #[test]
    fn test_group_by_project_root_no_config() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // No config files anywhere
        fs::create_dir_all(repo_root.join("src")).unwrap();

        let edges = vec![make_edge("src/main.ts")];

        let groups = group_by_project_root(repo_root, &edges);

        // Falls back to repo root
        assert_eq!(groups.len(), 1);
        assert!(groups.contains_key(repo_root));
    }

    #[test]
    fn test_tsconfig_takes_priority_over_jsconfig() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Both tsconfig and jsconfig exist
        fs::write(repo_root.join("tsconfig.json"), "{}").unwrap();
        fs::write(repo_root.join("jsconfig.json"), "{}").unwrap();
        fs::create_dir_all(repo_root.join("src")).unwrap();

        let edges = vec![make_edge("src/main.ts")];

        let groups = group_by_project_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        // Should match at root (tsconfig wins by priority)
        assert!(groups.contains_key(repo_root));
    }

    #[test]
    fn test_detect_config_type() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // No config
        assert_eq!(detect_config_type(repo_root), None);

        // Add tsconfig
        fs::write(repo_root.join("tsconfig.json"), "{}").unwrap();
        assert_eq!(detect_config_type(repo_root), Some("tsconfig.json"));

        // Add jsconfig (tsconfig should still win)
        fs::write(repo_root.join("jsconfig.json"), "{}").unwrap();
        assert_eq!(detect_config_type(repo_root), Some("tsconfig.json"));

        // Remove tsconfig
        fs::remove_file(repo_root.join("tsconfig.json")).unwrap();
        assert_eq!(detect_config_type(repo_root), Some("jsconfig.json"));

        // Remove jsconfig, add package.json
        fs::remove_file(repo_root.join("jsconfig.json")).unwrap();
        fs::write(repo_root.join("package.json"), "{}").unwrap();
        assert_eq!(detect_config_type(repo_root), Some("package.json"));
    }
}
