//! Java project detection and file grouping.
//!
//! jdtls requires a workspace context to resolve types. Files must be
//! grouped by their nearest build file ancestor, and jdtls needs to
//! know the workspace root for proper initialization.
//!
//! # Two-Level Detection
//!
//! Java projects have a distinction between:
//!
//! 1. **Module root** — the nearest build file (Maven pom.xml, Gradle build.gradle*,
//!    Eclipse .project). This is where the file "belongs" for compilation.
//!
//! 2. **Workspace launch root** — where jdtls should be started. For Gradle
//!    multi-module projects, this may be promoted to an enclosing settings.gradle*.
//!
//! # Build System Priority
//!
//! 1. `pom.xml` — Maven project
//! 2. `build.gradle.kts` — Gradle Kotlin DSL
//! 3. `build.gradle` — Gradle Groovy DSL
//! 4. `settings.gradle.kts` — Gradle settings (Kotlin)
//! 5. `settings.gradle` — Gradle settings (Groovy)
//! 6. `.project` — Eclipse project
//! 7. Repo root — standalone fallback
//!
//! # Gradle Workspace Promotion
//!
//! For Gradle projects, the workspace root is promoted to an enclosing
//! `settings.gradle*` if one exists. This is because Gradle multi-module
//! projects have a root settings file that defines the workspace structure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use enrichment::EligibleEdge;

/// Build file types in priority order for module grouping.
const MODULE_BUILD_FILES: &[&str] = &["pom.xml", "build.gradle.kts", "build.gradle", ".project"];

/// Gradle settings files (for workspace promotion).
const GRADLE_SETTINGS_FILES: &[&str] = &["settings.gradle.kts", "settings.gradle"];

/// Detected build system type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildSystem {
    Maven,
    Gradle,
    Eclipse,
    None,
}

impl BuildSystem {
    /// Detect build system from a build file name.
    pub fn from_file(filename: &str) -> Self {
        match filename {
            "pom.xml" => Self::Maven,
            "build.gradle.kts" | "build.gradle" | "settings.gradle.kts" | "settings.gradle" => {
                Self::Gradle
            }
            ".project" => Self::Eclipse,
            _ => Self::None,
        }
    }
}

/// Result of Java project detection for a file.
#[derive(Debug, Clone)]
pub struct JavaProjectContext {
    /// Module root — nearest build file directory.
    /// This is where the file "belongs" for compilation.
    pub module_root: PathBuf,

    /// Workspace launch root — where jdtls should be started.
    /// For Gradle multi-module projects, this may be higher than module_root.
    pub workspace_root: PathBuf,

    /// Detected build system.
    pub build_system: BuildSystem,

    /// The build file that determined the module root.
    pub build_file: Option<String>,
}

/// Group eligible edges by their Java workspace context.
///
/// Returns a map from workspace root to (context, edges).
/// All edges in a group share the same workspace launch root.
pub fn group_by_workspace_root(
    repo_root: &Path,
    edges: &[EligibleEdge],
) -> HashMap<PathBuf, (JavaProjectContext, Vec<EligibleEdge>)> {
    let mut cache: HashMap<PathBuf, JavaProjectContext> = HashMap::new();
    let mut groups: HashMap<PathBuf, (JavaProjectContext, Vec<EligibleEdge>)> = HashMap::new();

    for edge in edges {
        let file_path = Path::new(&edge.source_file_path);
        let context = detect_project_context(repo_root, file_path, &mut cache);

        groups
            .entry(context.workspace_root.clone())
            .or_insert_with(|| (context.clone(), Vec::new()))
            .1
            .push(edge.clone());
    }

    groups
}

/// Detect the Java project context for a file.
///
/// Walks upward from the file's directory to repo_root, checking for
/// build files. For Gradle projects, also checks for workspace promotion.
fn detect_project_context(
    repo_root: &Path,
    file_rel_path: &Path,
    cache: &mut HashMap<PathBuf, JavaProjectContext>,
) -> JavaProjectContext {
    // Get the directory containing the file
    let file_dir = file_rel_path.parent().unwrap_or(Path::new(""));

    // Check cache first
    if let Some(cached) = cache.get(file_dir) {
        return cached.clone();
    }

    // Find nearest module root (build file)
    let (module_root, build_file, build_system) = find_module_root(repo_root, file_dir);

    // For Gradle, check for workspace promotion
    let workspace_root = if build_system == BuildSystem::Gradle {
        find_gradle_workspace_root(repo_root, &module_root).unwrap_or_else(|| module_root.clone())
    } else {
        module_root.clone()
    };

    let context = JavaProjectContext {
        module_root,
        workspace_root,
        build_system,
        build_file,
    };

    // Cache the result
    cache.insert(file_dir.to_path_buf(), context.clone());

    context
}

/// Find the nearest module root (build file) for a file directory.
///
/// Returns (absolute module root path, build file name, build system).
fn find_module_root(repo_root: &Path, file_dir: &Path) -> (PathBuf, Option<String>, BuildSystem) {
    let mut current = file_dir.to_path_buf();

    loop {
        // Build absolute path for this directory
        let abs_dir = if current.as_os_str().is_empty() {
            repo_root.to_path_buf()
        } else {
            repo_root.join(&current)
        };

        // Check for build files in priority order
        for build_file in MODULE_BUILD_FILES {
            if abs_dir.join(build_file).exists() {
                let build_system = BuildSystem::from_file(build_file);
                return (abs_dir, Some(build_file.to_string()), build_system);
            }
        }

        // Also check for settings files (they indicate a Gradle workspace)
        for settings_file in GRADLE_SETTINGS_FILES {
            if abs_dir.join(settings_file).exists() {
                return (
                    abs_dir,
                    Some(settings_file.to_string()),
                    BuildSystem::Gradle,
                );
            }
        }

        // Move up one directory
        if current.as_os_str().is_empty() {
            break;
        }

        current = current.parent().unwrap_or(Path::new("")).to_path_buf();
    }

    // No build file found — fall back to repo root
    (repo_root.to_path_buf(), None, BuildSystem::None)
}

/// Find Gradle workspace root by looking for settings.gradle* above module root.
///
/// If the module root has a build.gradle* but there's a settings.gradle* higher up,
/// promote to the settings file location as the workspace root.
fn find_gradle_workspace_root(repo_root: &Path, module_root: &Path) -> Option<PathBuf> {
    // Only promote if module_root is below repo_root
    let rel_path = module_root.strip_prefix(repo_root).ok()?;
    if rel_path.as_os_str().is_empty() {
        return None; // Already at repo root
    }

    // Walk upward from module_root's parent
    let mut current = module_root.parent()?;

    while current.starts_with(repo_root) || current == repo_root {
        for settings_file in GRADLE_SETTINGS_FILES {
            if current.join(settings_file).exists() {
                return Some(current.to_path_buf());
            }
        }

        if current == repo_root {
            break;
        }

        current = current.parent()?;
    }

    None
}

/// Detect the build system for a directory.
///
/// Returns the build system type if found, None otherwise.
#[allow(dead_code)]
pub fn detect_build_system(project_root: &Path) -> BuildSystem {
    for build_file in MODULE_BUILD_FILES {
        if project_root.join(build_file).exists() {
            return BuildSystem::from_file(build_file);
        }
    }
    for settings_file in GRADLE_SETTINGS_FILES {
        if project_root.join(settings_file).exists() {
            return BuildSystem::Gradle;
        }
    }
    BuildSystem::None
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
            language: EnrichmentLanguage::Java,
        }
    }

    #[test]
    fn test_group_by_workspace_root_maven() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create Maven project
        fs::write(repo_root.join("pom.xml"), "<project/>").unwrap();
        fs::create_dir_all(repo_root.join("src/main/java")).unwrap();

        let edges = vec![
            make_edge("src/main/java/Main.java"),
            make_edge("src/main/java/util/Helper.java"),
        ];

        let groups = group_by_workspace_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        let (context, group_edges) = groups.values().next().unwrap();
        assert_eq!(context.workspace_root, repo_root);
        assert_eq!(context.module_root, repo_root);
        assert_eq!(context.build_system, BuildSystem::Maven);
        assert_eq!(context.build_file, Some("pom.xml".to_string()));
        assert_eq!(group_edges.len(), 2);
    }

    #[test]
    fn test_group_by_workspace_root_gradle_single() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create single-module Gradle project
        fs::write(repo_root.join("build.gradle"), "plugins {}").unwrap();
        fs::create_dir_all(repo_root.join("src/main/java")).unwrap();

        let edges = vec![make_edge("src/main/java/Main.java")];

        let groups = group_by_workspace_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        let (context, _) = groups.values().next().unwrap();
        assert_eq!(context.build_system, BuildSystem::Gradle);
        assert_eq!(context.build_file, Some("build.gradle".to_string()));
    }

    #[test]
    fn test_group_by_workspace_root_gradle_multimodule() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create multi-module Gradle project
        fs::write(repo_root.join("settings.gradle"), "include 'app', 'lib'").unwrap();
        fs::create_dir_all(repo_root.join("app/src/main/java")).unwrap();
        fs::create_dir_all(repo_root.join("lib/src/main/java")).unwrap();
        fs::write(repo_root.join("app/build.gradle"), "plugins {}").unwrap();
        fs::write(repo_root.join("lib/build.gradle"), "plugins {}").unwrap();

        let edges = vec![
            make_edge("app/src/main/java/App.java"),
            make_edge("lib/src/main/java/Lib.java"),
        ];

        let groups = group_by_workspace_root(repo_root, &edges);

        // Both files should be grouped under the workspace root (settings.gradle location)
        assert_eq!(groups.len(), 1);
        let (context, group_edges) = groups.values().next().unwrap();
        assert_eq!(context.workspace_root, repo_root);
        assert_eq!(context.build_system, BuildSystem::Gradle);
        assert_eq!(group_edges.len(), 2);
    }

    #[test]
    fn test_gradle_workspace_promotion() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create multi-module Gradle project with Kotlin DSL
        fs::write(repo_root.join("settings.gradle.kts"), "include(\":app\")").unwrap();
        fs::create_dir_all(repo_root.join("app/src/main/java")).unwrap();
        fs::write(repo_root.join("app/build.gradle.kts"), "plugins {}").unwrap();

        let edges = vec![make_edge("app/src/main/java/App.java")];

        let groups = group_by_workspace_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        let (context, _) = groups.values().next().unwrap();
        // Module root is app/, but workspace root is promoted to repo root
        assert_eq!(context.module_root, repo_root.join("app"));
        assert_eq!(context.workspace_root, repo_root);
    }

    #[test]
    fn test_group_by_workspace_root_eclipse() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Create Eclipse project
        fs::write(repo_root.join(".project"), "<projectDescription/>").unwrap();
        fs::create_dir_all(repo_root.join("src")).unwrap();

        let edges = vec![make_edge("src/Main.java")];

        let groups = group_by_workspace_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        let (context, _) = groups.values().next().unwrap();
        assert_eq!(context.build_system, BuildSystem::Eclipse);
        assert_eq!(context.build_file, Some(".project".to_string()));
    }

    #[test]
    fn test_group_by_workspace_root_no_build_file() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // No build files
        fs::create_dir_all(repo_root.join("src")).unwrap();

        let edges = vec![make_edge("src/Main.java")];

        let groups = group_by_workspace_root(repo_root, &edges);

        assert_eq!(groups.len(), 1);
        let (context, _) = groups.values().next().unwrap();
        assert_eq!(context.workspace_root, repo_root);
        assert_eq!(context.build_system, BuildSystem::None);
        assert_eq!(context.build_file, None);
    }

    #[test]
    fn test_maven_takes_priority_over_gradle() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // Both Maven and Gradle exist
        fs::write(repo_root.join("pom.xml"), "<project/>").unwrap();
        fs::write(repo_root.join("build.gradle"), "plugins {}").unwrap();
        fs::create_dir_all(repo_root.join("src")).unwrap();

        let edges = vec![make_edge("src/Main.java")];

        let groups = group_by_workspace_root(repo_root, &edges);

        let (context, _) = groups.values().next().unwrap();
        // Maven takes priority
        assert_eq!(context.build_system, BuildSystem::Maven);
        assert_eq!(context.build_file, Some("pom.xml".to_string()));
    }

    #[test]
    fn test_detect_build_system() {
        let tmp = TempDir::new().unwrap();
        let repo_root = tmp.path();

        // No build system
        assert_eq!(detect_build_system(repo_root), BuildSystem::None);

        // Add Maven
        fs::write(repo_root.join("pom.xml"), "<project/>").unwrap();
        assert_eq!(detect_build_system(repo_root), BuildSystem::Maven);

        // Remove Maven, add Gradle
        fs::remove_file(repo_root.join("pom.xml")).unwrap();
        fs::write(repo_root.join("build.gradle.kts"), "plugins {}").unwrap();
        assert_eq!(detect_build_system(repo_root), BuildSystem::Gradle);

        // Remove Gradle build, add settings
        fs::remove_file(repo_root.join("build.gradle.kts")).unwrap();
        fs::write(repo_root.join("settings.gradle"), "").unwrap();
        assert_eq!(detect_build_system(repo_root), BuildSystem::Gradle);

        // Remove settings, add Eclipse
        fs::remove_file(repo_root.join("settings.gradle")).unwrap();
        fs::write(repo_root.join(".project"), "").unwrap();
        assert_eq!(detect_build_system(repo_root), BuildSystem::Eclipse);
    }
}
