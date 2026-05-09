//! settings.gradle module extraction (rust-module-parity Phase 2b).
//!
//! Extracts declared module candidates from Gradle settings files.
//! Multi-project Gradle builds declare subprojects in `settings.gradle`
//! (Groovy DSL) or `settings.gradle.kts` (Kotlin DSL).
//!
//! # Identity Contract
//!
//! Module key format: `gradle:{repo_uid}:{project_path}`
//!
//! Path uses filesystem separator `/`, NOT Gradle's `:` notation.
//!
//! Examples:
//! - `gradle:kafka:.` (root project)
//! - `gradle:kafka:connect/api` (subproject at `connect/api/`)
//! - `gradle:kafka:streams/test-utils` (subproject at `streams/test-utils/`)
//!
//! Path-anchored identity. Same rule as Cargo, npm, pyproject.
//!
//! # Evidence Structure
//!
//! Each module candidate has associated evidence:
//! - `source_type` = "settings_gradle"
//! - `source_path` = path to the settings.gradle file
//! - `evidence_kind` = "manifest_declaration"
//! - `payload_json` contains:
//!   - `gradle_path`: Gradle path notation (`:connect:api`)
//!   - `project_root`: filesystem path (`connect/api`)
//!   - `display_name`: renamed name if explicit, otherwise directory basename
//!
//! # Display Name Resolution
//!
//! 1. If `project(":x:y").name = "foo"` exists → use `"foo"`
//! 2. Otherwise → use directory basename (e.g., `api` for `connect/api`)
//!
//! # Scope
//!
//! Phase 2b scope:
//! - `settings.gradle` parsing (Groovy DSL)
//! - `settings.gradle.kts` parsing (Kotlin DSL) — best effort
//! - Root project detection
//! - Subproject detection from `include` statements
//! - Project rename detection
//!
//! Not in scope:
//! - `build.gradle` / `build.gradle.kts` parsing
//! - Composite builds (`includeBuild`)
//! - Plugin management blocks

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

// ── Extraction output types ──────────────────────────────────────────

/// A project discovered from settings.gradle parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradleModule {
    /// Gradle path notation (e.g., `:connect:api` or `:` for root)
    pub gradle_path: String,
    /// Filesystem path relative to repo root (e.g., `connect/api` or `.`)
    pub project_root: String,
    /// Display name (renamed name if explicit, otherwise directory basename)
    pub display_name: String,
    /// Path to the settings file that declared this project
    pub settings_path: String,
    /// Whether this is the root project
    pub is_root: bool,
}

/// Result of parsing a settings.gradle file.
#[derive(Debug, Clone)]
pub struct SettingsGradleParseResult {
    /// Root project (always present if settings file is valid)
    pub root_project: Option<GradleModule>,
    /// Subprojects declared via `include` statements
    pub subprojects: Vec<GradleModule>,
}

/// Evidence payload for settings.gradle-derived modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradleEvidencePayload {
    /// Gradle path notation (`:connect:api`)
    pub gradle_path: String,
    /// Filesystem path (`connect/api`)
    pub project_root: String,
    /// Display name (renamed or basename)
    pub display_name: String,
    /// Whether this is the root project
    pub is_root: bool,
}

// ── Parsing functions ────────────────────────────────────────────────

/// Parse a settings.gradle or settings.gradle.kts file.
///
/// # Arguments
/// - `content`: raw settings file content
/// - `settings_path`: path to the settings file relative to repo root
///   (e.g., "settings.gradle" or "settings.gradle.kts")
///
/// # Returns
/// - `SettingsGradleParseResult` with root project and subprojects
///
/// # Notes
///
/// This function uses regex-based extraction rather than a full Groovy/Kotlin
/// parser. It handles the common patterns found in real Gradle projects:
/// - `include 'a', 'b', 'c'` (comma-separated, possibly multi-line)
/// - `include('a', 'b', 'c')` (function call syntax)
/// - `rootProject.name = 'name'`
/// - `project(":path").name = "name"`
///
/// Edge cases like programmatic `include` calls are not supported.
pub fn parse_settings_gradle(content: &str, settings_path: &str) -> SettingsGradleParseResult {
    // Extract root project name (if declared).
    let root_name = extract_root_project_name(content);

    // Extract project renames.
    let renames = extract_project_renames(content);

    // Extract included subprojects.
    let included_paths = extract_include_paths(content);

    // Build subproject modules.
    let subprojects: Vec<GradleModule> = included_paths
        .into_iter()
        .map(|gradle_path| {
            let project_root = gradle_path_to_filesystem(&gradle_path);
            let display_name = renames
                .get(&gradle_path)
                .cloned()
                .unwrap_or_else(|| directory_basename(&project_root));

            GradleModule {
                gradle_path: format!(":{}", gradle_path),
                project_root,
                display_name,
                settings_path: settings_path.to_string(),
                is_root: false,
            }
        })
        .collect();

    // Build root project module.
    let root_display_name = root_name.unwrap_or_else(|| "root".to_string());
    let root_project = Some(GradleModule {
        gradle_path: ":".to_string(),
        project_root: ".".to_string(),
        display_name: root_display_name,
        settings_path: settings_path.to_string(),
        is_root: true,
    });

    SettingsGradleParseResult {
        root_project,
        subprojects,
    }
}

/// Extract root project name from `rootProject.name = 'name'` or `rootProject.name = "name"`.
fn extract_root_project_name(content: &str) -> Option<String> {
    // Match: rootProject.name = 'name' or rootProject.name = "name"
    // Also handle: rootProject.name='name' (no spaces)
    let re = Regex::new(r#"rootProject\s*\.\s*name\s*=\s*['"]([^'"]+)['"]"#).ok()?;
    re.captures(content).map(|cap| cap[1].to_string())
}

/// Extract project renames from `project(":path").name = "name"` statements.
fn extract_project_renames(content: &str) -> HashMap<String, String> {
    let mut renames = HashMap::new();

    // Match: project(":path").name = "name" or project(':path').name = 'name'
    // The path may use : separator (e.g., ":storage:api")
    let re = Regex::new(r#"project\s*\(\s*['"]([^'"]+)['"]\s*\)\s*\.\s*name\s*=\s*['"]([^'"]+)['"]"#);

    if let Ok(re) = re {
        for cap in re.captures_iter(content) {
            let gradle_path = cap[1].to_string();
            let new_name = cap[2].to_string();
            // Store without leading colon for lookup consistency
            let normalized_path = gradle_path.trim_start_matches(':').to_string();
            renames.insert(normalized_path, new_name);
        }
    }

    renames
}

/// Extract included subproject paths from `include` statements.
fn extract_include_paths(content: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Strategy: find all quoted strings that appear in include contexts.
    // This handles:
    // - include 'a', 'b', 'c'
    // - include('a', 'b')
    // - include 'a'
    // - Multi-line includes

    // Match quoted strings (single or double quotes)
    let string_re = Regex::new(r#"['"]([^'"]+)['"]"#).unwrap();

    // Process line by line, tracking whether we're in an include block.
    // An include block starts with `include` and continues while lines
    // look like string continuations (contain only whitespace, commas, and quoted strings).
    let mut in_include_block = false;
    let continuation_re = Regex::new(r#"^[\s,]*['":]"#).unwrap();

    for line in content.lines() {
        let trimmed = line.trim();

        // Check if this line starts an include block
        if trimmed.starts_with("include") {
            in_include_block = true;
        } else if in_include_block {
            // Check if this looks like a continuation line
            // A continuation line starts with whitespace/comma and has a quoted string
            if !continuation_re.is_match(line) && !trimmed.is_empty() {
                in_include_block = false;
            }
        }

        if in_include_block {
            // Extract all quoted strings from this line
            for string_cap in string_re.captures_iter(line) {
                let path = string_cap[1].to_string();
                // Skip if it looks like a version string, URL, or plugin ID
                if !path.contains("://")
                    && !path.starts_with("com.")
                    && !path.starts_with("org.")
                    && !path.starts_with("id ")
                    && !is_version_string(&path)
                {
                    paths.push(path);
                }
            }
        }
    }

    paths
}

/// Check if a string looks like a version number (e.g., "3.19", "1.0.0").
fn is_version_string(s: &str) -> bool {
    // Version strings are typically digits and dots
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Convert Gradle path notation to filesystem path.
///
/// `connect:api` → `connect/api`
/// `streams:test-utils` → `streams/test-utils`
fn gradle_path_to_filesystem(gradle_path: &str) -> String {
    // Remove leading colon if present
    let path = gradle_path.trim_start_matches(':');
    // Replace colons with slashes
    path.replace(':', "/")
}

/// Extract directory basename from a path.
///
/// `connect/api` → `api`
/// `.` → `root`
fn directory_basename(path: &str) -> String {
    if path == "." {
        return "root".to_string();
    }
    path.rsplit('/').next().unwrap_or(path).to_string()
}

// ── Identity generation ──────────────────────────────────────────────

/// Generate a deterministic module_candidate_uid for Gradle modules.
///
/// Identity is derived from: repo_uid + project_root + "declared"
pub fn generate_module_uid(repo_uid: &str, project_root: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"gradle_module:");
    hasher.update(repo_uid.as_bytes());
    hasher.update(b":");
    hasher.update(project_root.as_bytes());
    hasher.update(b":declared");
    let hash = hasher.finalize();
    format!(
        "gradle-mod-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate a deterministic evidence_uid.
pub fn generate_evidence_uid(module_uid: &str, settings_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"gradle_evidence:");
    hasher.update(module_uid.as_bytes());
    hasher.update(b":");
    hasher.update(settings_path.as_bytes());
    let hash = hasher.finalize();
    format!(
        "gradle-ev-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate the canonical module_key.
///
/// Format: `gradle:{repo_uid}:{project_root}`
/// Uses filesystem path separator `/`, not Gradle's `:`.
pub fn generate_module_key(repo_uid: &str, project_root: &str) -> String {
    format!("gradle:{}:{}", repo_uid, project_root)
}

// ── Storage input conversion ─────────────────────────────────────────

use crate::cargo_manifest::{CargoModuleCandidateInput, CargoModuleEvidenceInput};

/// Convert a GradleModule to storage inputs.
///
/// Generates deterministic UIDs and evidence payload.
/// Returns the same input types as Cargo/npm/pyproject (they're generic).
pub fn to_storage_inputs(
    module: &GradleModule,
    repo_uid: &str,
    snapshot_uid: &str,
) -> (CargoModuleCandidateInput, CargoModuleEvidenceInput) {
    let module_uid = generate_module_uid(repo_uid, &module.project_root);
    let module_key = generate_module_key(repo_uid, &module.project_root);
    let evidence_uid = generate_evidence_uid(&module_uid, &module.settings_path);

    let payload = GradleEvidencePayload {
        gradle_path: module.gradle_path.clone(),
        project_root: module.project_root.clone(),
        display_name: module.display_name.clone(),
        is_root: module.is_root,
    };

    let candidate = CargoModuleCandidateInput {
        module_candidate_uid: module_uid.clone(),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        module_key,
        module_kind: "declared".to_string(),
        canonical_root_path: module.project_root.clone(),
        confidence: 1.0,
        display_name: module.display_name.clone(),
        metadata_json: None,
    };

    let evidence = CargoModuleEvidenceInput {
        evidence_uid,
        module_candidate_uid: module_uid,
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        source_type: "settings_gradle".to_string(),
        source_path: module.settings_path.clone(),
        evidence_kind: "manifest_declaration".to_string(),
        confidence: 1.0,
        payload_json: serde_json::to_string(&payload).unwrap_or_default(),
    };

    (candidate, evidence)
}

// ── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kafka_style_settings() {
        let content = r#"
plugins {
    id 'com.gradle.develocity' version '3.19'
}

include 'clients',
    'clients:clients-integration-tests',
    'connect:api',
    'connect:runtime',
    'core',
    'storage',
    'storage:api',
    'streams',
    'streams:test-utils'

project(":storage:api").name = "storage-api"
rootProject.name = 'kafka'
"#;
        let result = parse_settings_gradle(content, "settings.gradle");

        // Check root project
        assert!(result.root_project.is_some());
        let root = result.root_project.unwrap();
        assert_eq!(root.display_name, "kafka");
        assert_eq!(root.project_root, ".");
        assert!(root.is_root);

        // Check subprojects count
        assert_eq!(result.subprojects.len(), 9);

        // Check specific subprojects
        let clients = result.subprojects.iter().find(|p| p.project_root == "clients");
        assert!(clients.is_some());
        assert_eq!(clients.unwrap().display_name, "clients");
        assert_eq!(clients.unwrap().gradle_path, ":clients");

        // Check nested subproject
        let connect_api = result.subprojects.iter().find(|p| p.project_root == "connect/api");
        assert!(connect_api.is_some());
        assert_eq!(connect_api.unwrap().display_name, "api");
        assert_eq!(connect_api.unwrap().gradle_path, ":connect:api");

        // Check renamed project
        let storage_api = result.subprojects.iter().find(|p| p.project_root == "storage/api");
        assert!(storage_api.is_some());
        assert_eq!(storage_api.unwrap().display_name, "storage-api"); // Renamed!
    }

    #[test]
    fn parse_simple_single_project() {
        let content = r#"
rootProject.name = 'my-app'
"#;
        let result = parse_settings_gradle(content, "settings.gradle");

        assert!(result.root_project.is_some());
        let root = result.root_project.unwrap();
        assert_eq!(root.display_name, "my-app");
        assert!(result.subprojects.is_empty());
    }

    #[test]
    fn parse_include_function_syntax() {
        // Kotlin DSL often uses function call syntax
        let content = r#"
rootProject.name = "my-project"
include("core", "api", "impl")
"#;
        let result = parse_settings_gradle(content, "settings.gradle.kts");

        assert_eq!(result.subprojects.len(), 3);
        assert!(result.subprojects.iter().any(|p| p.project_root == "core"));
        assert!(result.subprojects.iter().any(|p| p.project_root == "api"));
        assert!(result.subprojects.iter().any(|p| p.project_root == "impl"));
    }

    #[test]
    fn parse_no_root_name() {
        let content = r#"
include 'app', 'lib'
"#;
        let result = parse_settings_gradle(content, "settings.gradle");

        assert!(result.root_project.is_some());
        let root = result.root_project.unwrap();
        assert_eq!(root.display_name, "root"); // Default when not specified
    }

    #[test]
    fn gradle_path_conversion() {
        assert_eq!(gradle_path_to_filesystem("clients"), "clients");
        assert_eq!(gradle_path_to_filesystem(":clients"), "clients");
        assert_eq!(gradle_path_to_filesystem("connect:api"), "connect/api");
        assert_eq!(gradle_path_to_filesystem(":connect:api"), "connect/api");
        assert_eq!(gradle_path_to_filesystem("streams:test-utils"), "streams/test-utils");
    }

    #[test]
    fn directory_basename_extraction() {
        assert_eq!(directory_basename("connect/api"), "api");
        assert_eq!(directory_basename("clients"), "clients");
        assert_eq!(directory_basename("."), "root");
        assert_eq!(directory_basename("a/b/c"), "c");
    }

    #[test]
    fn module_key_format() {
        let key = generate_module_key("kafka", ".");
        assert_eq!(key, "gradle:kafka:.");

        let key = generate_module_key("kafka", "connect/api");
        assert_eq!(key, "gradle:kafka:connect/api");
    }

    #[test]
    fn uid_determinism() {
        let uid1 = generate_module_uid("kafka", ".");
        let uid2 = generate_module_uid("kafka", ".");
        assert_eq!(uid1, uid2);

        let uid3 = generate_module_uid("kafka", "connect/api");
        assert_ne!(uid1, uid3);
    }

    #[test]
    fn uid_prefix_distinguishes_from_others() {
        let gradle_uid = generate_module_uid("repo", ".");
        let cargo_uid = crate::cargo_manifest::generate_module_uid("repo", ".");
        let npm_uid = crate::package_json::generate_module_uid("repo", ".");
        let pyproject_uid = crate::pyproject::generate_module_uid("repo", ".");

        assert!(gradle_uid.starts_with("gradle-mod-"));
        assert!(cargo_uid.starts_with("cargo-mod-"));
        assert!(npm_uid.starts_with("npm-mod-"));
        assert!(pyproject_uid.starts_with("pyproject-mod-"));

        // All different
        assert_ne!(gradle_uid, cargo_uid);
        assert_ne!(gradle_uid, npm_uid);
        assert_ne!(gradle_uid, pyproject_uid);
    }

    #[test]
    fn storage_input_conversion() {
        let module = GradleModule {
            gradle_path: ":connect:api".to_string(),
            project_root: "connect/api".to_string(),
            display_name: "api".to_string(),
            settings_path: "settings.gradle".to_string(),
            is_root: false,
        };

        let (candidate, evidence) = to_storage_inputs(&module, "kafka", "snap-1");

        assert_eq!(candidate.module_kind, "declared");
        assert_eq!(candidate.canonical_root_path, "connect/api");
        assert_eq!(candidate.display_name, "api");
        assert!((candidate.confidence - 1.0).abs() < f64::EPSILON);
        assert_eq!(candidate.module_key, "gradle:kafka:connect/api");

        assert_eq!(evidence.source_type, "settings_gradle");
        assert_eq!(evidence.source_path, "settings.gradle");
        assert_eq!(evidence.evidence_kind, "manifest_declaration");

        // Verify payload JSON
        let payload: GradleEvidencePayload =
            serde_json::from_str(&evidence.payload_json).unwrap();
        assert_eq!(payload.gradle_path, ":connect:api");
        assert_eq!(payload.project_root, "connect/api");
        assert!(!payload.is_root);
    }

    #[test]
    fn parse_multiple_rename_statements() {
        let content = r#"
include 'a:b', 'c:d', 'e:f'
project(":a:b").name = "renamed-ab"
project(':c:d').name = 'renamed-cd'
rootProject.name = 'test'
"#;
        let result = parse_settings_gradle(content, "settings.gradle");

        let ab = result.subprojects.iter().find(|p| p.project_root == "a/b");
        assert!(ab.is_some());
        assert_eq!(ab.unwrap().display_name, "renamed-ab");

        let cd = result.subprojects.iter().find(|p| p.project_root == "c/d");
        assert!(cd.is_some());
        assert_eq!(cd.unwrap().display_name, "renamed-cd");

        // e:f should use basename
        let ef = result.subprojects.iter().find(|p| p.project_root == "e/f");
        assert!(ef.is_some());
        assert_eq!(ef.unwrap().display_name, "f");
    }
}
