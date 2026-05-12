//! package.json module extraction (rust-module-parity Phase 2).
//!
//! Extracts declared module candidates from package.json manifests and
//! pnpm-workspace.yaml workspace definitions. This enables the Rust indexer
//! to populate `module_candidates` and `module_candidate_evidence` tables
//! for TS/JS ecosystems, matching the Cargo.toml path from Phase 1.
//!
//! # Identity Contract
//!
//! Module key format: `npm:{repo_uid}:{package_root_path}`
//!
//! Examples:
//! - `npm:repo-123:packages/core` (workspace member)
//! - `npm:repo-123:.` (root package in single-package repo)
//!
//! Path-anchored identity, NOT package name identity. Same rule as Cargo.
//!
//! # Evidence Structure
//!
//! Each module candidate has associated evidence:
//! - `source_type` = "package_json" | "pnpm_workspace_yaml"
//! - `source_path` = path to the manifest file
//! - `evidence_kind` = "manifest_declaration"
//! - `payload_json` contains:
//!   - `package_name`: name from package.json
//!   - `package_root`: directory containing package.json
//!   - `workspace_member`: true if discovered via workspace patterns
//!   - `version`: optional package version
//!
//! # Module Kind
//!
//! All package.json-derived modules are `module_kind` = "declared".
//! This distinguishes them from heuristic-inferred modules.
//!
//! # Workspace Pattern Support
//!
//! Two workspace definition sources:
//! - `package.json` `workspaces` array (npm/yarn workspaces)
//! - `pnpm-workspace.yaml` `packages` array (pnpm workspaces)
//!
//! Glob patterns are expanded at preparation time (same as Cargo).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Extraction output types ──────────────────────────────────────────

/// A package discovered from package.json parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmModule {
    /// Package name from package.json "name" field
    pub package_name: String,
    /// Package root directory (relative to repo root)
    /// For workspace members, this is the member path.
    /// For root packages, this is "." or empty.
    pub package_root: String,
    /// Package version from package.json "version" field (if present)
    pub version: Option<String>,
    /// Path to the package.json that declared this package
    pub manifest_path: String,
    /// True if discovered via workspace patterns
    pub is_workspace_member: bool,
    /// Source of discovery: "package_json" or "pnpm_workspace_yaml"
    pub source_type: String,
}

/// Result of parsing a package.json manifest.
#[derive(Debug, Clone)]
pub struct PackageJsonParseResult {
    /// Discovered package module (if package.json has "name" field)
    pub module: Option<NpmModule>,
    /// Workspace member patterns (if this has "workspaces" array)
    pub workspace_patterns: Vec<String>,
    /// True if this manifest declares workspaces
    pub is_workspace_root: bool,
}

/// Result of parsing a pnpm-workspace.yaml file.
#[derive(Debug, Clone)]
pub struct PnpmWorkspaceParseResult {
    /// Workspace member patterns from "packages" array
    pub workspace_patterns: Vec<String>,
}

/// Evidence payload for package.json-derived modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpmEvidencePayload {
    /// Package name from package.json "name" field
    pub package_name: String,
    /// Package root directory (relative to repo root)
    pub package_root: String,
    /// True if discovered via workspace patterns
    pub workspace_member: bool,
    /// Package version (if present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// ── Parsing types (private) ──────────────────────────────────────────

/// Minimal package.json structure for package extraction.
#[derive(Debug, Deserialize)]
struct PackageJson {
    name: Option<String>,
    version: Option<String>,
    workspaces: Option<WorkspacesField>,
}

/// Workspaces can be an array or an object with "packages" field.
/// npm/yarn use array, some configs use object form.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WorkspacesField {
    Array(Vec<String>),
    Object { packages: Option<Vec<String>> },
}

impl WorkspacesField {
    fn into_patterns(self) -> Vec<String> {
        match self {
            WorkspacesField::Array(patterns) => patterns,
            WorkspacesField::Object { packages } => packages.unwrap_or_default(),
        }
    }
}

/// pnpm-workspace.yaml structure.
#[derive(Debug, Deserialize)]
struct PnpmWorkspace {
    packages: Option<Vec<String>>,
}

// ── Parsing functions ────────────────────────────────────────────────

/// Parse a package.json manifest and extract package metadata.
///
/// # Arguments
/// - `content`: raw package.json file content
/// - `manifest_path`: path to the package.json relative to repo root
///   (e.g., "package.json" for root, "packages/core/package.json" for nested)
///
/// # Returns
/// - `Ok(PackageJsonParseResult)` with extracted module and workspace info
/// - `Err` if JSON parsing fails
///
/// # Notes
///
/// This function handles:
/// - Single-package repositories (just has "name")
/// - Workspace roots ("workspaces" array with optional "name")
/// - Workspace members ("name" only, discovered via parent workspace)
///
/// For workspace roots, this function returns workspace patterns
/// but does NOT resolve globs. Glob resolution requires filesystem access
/// and is the caller's responsibility.
pub fn parse_package_json(
    content: &str,
    manifest_path: &str,
) -> Result<PackageJsonParseResult, serde_json::Error> {
    let parsed: PackageJson = serde_json::from_str(content)?;

    // Derive package root from manifest path.
    // "package.json" -> "."
    // "packages/core/package.json" -> "packages/core"
    let package_root = manifest_path
        .strip_suffix("/package.json")
        .or_else(|| manifest_path.strip_suffix("package.json"))
        .map(|s| {
            if s.is_empty() {
                "."
            } else {
                s.trim_end_matches('/')
            }
        })
        .unwrap_or(".");

    let workspace_patterns = parsed
        .workspaces
        .map(|w| w.into_patterns())
        .unwrap_or_default();
    let is_workspace_root = !workspace_patterns.is_empty();

    // Extract module if package has a name.
    let module = parsed.name.map(|name| NpmModule {
        package_name: name,
        package_root: package_root.to_string(),
        version: parsed.version,
        manifest_path: manifest_path.to_string(),
        is_workspace_member: false, // Will be updated by caller if discovered via workspace
        source_type: "package_json".to_string(),
    });

    Ok(PackageJsonParseResult {
        module,
        workspace_patterns,
        is_workspace_root,
    })
}

/// Parse a pnpm-workspace.yaml file and extract workspace patterns.
///
/// # Arguments
/// - `content`: raw pnpm-workspace.yaml file content
///
/// # Returns
/// - `Ok(PnpmWorkspaceParseResult)` with workspace patterns
/// - `Err` if YAML parsing fails
pub fn parse_pnpm_workspace(content: &str) -> Result<PnpmWorkspaceParseResult, serde_yaml::Error> {
    let parsed: PnpmWorkspace = serde_yaml::from_str(content)?;

    Ok(PnpmWorkspaceParseResult {
        workspace_patterns: parsed.packages.unwrap_or_default(),
    })
}

// ── Identity generation ──────────────────────────────────────────────

/// Generate a deterministic module_candidate_uid for npm modules.
///
/// Identity is derived from: repo_uid + package_root + "declared"
/// The "declared" discriminator ensures separation from future
/// inferred module UIDs.
pub fn generate_module_uid(repo_uid: &str, package_root: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"npm_module:");
    hasher.update(repo_uid.as_bytes());
    hasher.update(b":");
    hasher.update(package_root.as_bytes());
    hasher.update(b":declared");
    let hash = hasher.finalize();
    format!(
        "npm-mod-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate a deterministic evidence_uid.
pub fn generate_evidence_uid(module_uid: &str, manifest_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"npm_evidence:");
    hasher.update(module_uid.as_bytes());
    hasher.update(b":");
    hasher.update(manifest_path.as_bytes());
    let hash = hasher.finalize();
    format!(
        "npm-ev-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate the canonical module_key.
///
/// Format: `npm:{repo_uid}:{package_root_path}`
pub fn generate_module_key(repo_uid: &str, package_root: &str) -> String {
    format!("npm:{}:{}", repo_uid, package_root)
}

// ── Storage input conversion ─────────────────────────────────────────

// Reuse the generic input types from cargo_manifest module.
// The types are structurally identical; only the content differs.
use crate::cargo_manifest::{CargoModuleCandidateInput, CargoModuleEvidenceInput};

/// Convert an NpmModule to storage inputs.
///
/// Generates deterministic UIDs and evidence payload.
/// Returns the same input types as Cargo (they're generic).
pub fn to_storage_inputs(
    module: &NpmModule,
    repo_uid: &str,
    snapshot_uid: &str,
) -> (CargoModuleCandidateInput, CargoModuleEvidenceInput) {
    let module_uid = generate_module_uid(repo_uid, &module.package_root);
    let module_key = generate_module_key(repo_uid, &module.package_root);
    let evidence_uid = generate_evidence_uid(&module_uid, &module.manifest_path);

    let payload = NpmEvidencePayload {
        package_name: module.package_name.clone(),
        package_root: module.package_root.clone(),
        workspace_member: module.is_workspace_member,
        version: module.version.clone(),
    };

    let candidate = CargoModuleCandidateInput {
        module_candidate_uid: module_uid.clone(),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        module_key,
        module_kind: "declared".to_string(),
        canonical_root_path: module.package_root.clone(),
        confidence: 1.0,
        display_name: module.package_name.clone(),
        metadata_json: None,
    };

    let evidence = CargoModuleEvidenceInput {
        evidence_uid,
        module_candidate_uid: module_uid,
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        source_type: module.source_type.clone(),
        source_path: module.manifest_path.clone(),
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
    fn parse_simple_package() {
        let content = r#"{
            "name": "my-package",
            "version": "1.0.0"
        }"#;
        let result = parse_package_json(content, "package.json").unwrap();
        assert!(result.module.is_some());
        let module = result.module.unwrap();
        assert_eq!(module.package_name, "my-package");
        assert_eq!(module.package_root, ".");
        assert_eq!(module.version, Some("1.0.0".to_string()));
        assert!(!result.is_workspace_root);
        assert!(result.workspace_patterns.is_empty());
    }

    #[test]
    fn parse_nested_package() {
        let content = r#"{
            "name": "@scope/core",
            "version": "2.0.0"
        }"#;
        let result = parse_package_json(content, "packages/core/package.json").unwrap();
        assert!(result.module.is_some());
        let module = result.module.unwrap();
        assert_eq!(module.package_name, "@scope/core");
        assert_eq!(module.package_root, "packages/core");
        assert_eq!(module.manifest_path, "packages/core/package.json");
    }

    #[test]
    fn parse_workspace_root_array() {
        let content = r#"{
            "name": "monorepo",
            "version": "0.0.0",
            "workspaces": [
                "packages/*",
                "apps/*"
            ]
        }"#;
        let result = parse_package_json(content, "package.json").unwrap();
        assert!(result.is_workspace_root);
        assert_eq!(result.workspace_patterns, vec!["packages/*", "apps/*"]);
        // Root package still extracted
        assert!(result.module.is_some());
        assert_eq!(result.module.unwrap().package_name, "monorepo");
    }

    #[test]
    fn parse_workspace_root_object() {
        let content = r#"{
            "name": "monorepo",
            "workspaces": {
                "packages": ["packages/*"]
            }
        }"#;
        let result = parse_package_json(content, "package.json").unwrap();
        assert!(result.is_workspace_root);
        assert_eq!(result.workspace_patterns, vec!["packages/*"]);
    }

    #[test]
    fn parse_package_without_name() {
        let content = r#"{
            "version": "1.0.0",
            "private": true
        }"#;
        let result = parse_package_json(content, "package.json").unwrap();
        assert!(result.module.is_none());
    }

    #[test]
    fn parse_pnpm_workspace() {
        let content = r#"
packages:
  - 'packages/*'
  - 'apps/**'
  - '!**/test/**'
"#;
        let result = super::parse_pnpm_workspace(content).unwrap();
        assert_eq!(
            result.workspace_patterns,
            vec!["packages/*", "apps/**", "!**/test/**"]
        );
    }

    #[test]
    fn parse_pnpm_workspace_empty() {
        let content = "# empty workspace\n";
        let result = super::parse_pnpm_workspace(content).unwrap();
        assert!(result.workspace_patterns.is_empty());
    }

    #[test]
    fn module_key_format() {
        let key = generate_module_key("repo-123", "packages/core");
        assert_eq!(key, "npm:repo-123:packages/core");
    }

    #[test]
    fn module_key_root_package() {
        let key = generate_module_key("repo-456", ".");
        assert_eq!(key, "npm:repo-456:.");
    }

    #[test]
    fn uid_determinism() {
        let uid1 = generate_module_uid("repo", "packages/foo");
        let uid2 = generate_module_uid("repo", "packages/foo");
        assert_eq!(uid1, uid2);

        let uid3 = generate_module_uid("repo", "packages/bar");
        assert_ne!(uid1, uid3);
    }

    #[test]
    fn uid_prefix_distinguishes_npm_from_cargo() {
        let npm_uid = generate_module_uid("repo", "packages/foo");
        let cargo_uid = crate::cargo_manifest::generate_module_uid("repo", "packages/foo");

        assert!(npm_uid.starts_with("npm-mod-"));
        assert!(cargo_uid.starts_with("cargo-mod-"));
        assert_ne!(npm_uid, cargo_uid);
    }

    #[test]
    fn storage_input_conversion() {
        let module = NpmModule {
            package_name: "@scope/core".to_string(),
            package_root: "packages/core".to_string(),
            version: Some("1.0.0".to_string()),
            manifest_path: "packages/core/package.json".to_string(),
            is_workspace_member: true,
            source_type: "package_json".to_string(),
        };

        let (candidate, evidence) = to_storage_inputs(&module, "repo-1", "snap-1");

        assert_eq!(candidate.module_kind, "declared");
        assert_eq!(candidate.canonical_root_path, "packages/core");
        assert_eq!(candidate.display_name, "@scope/core");
        assert!((candidate.confidence - 1.0).abs() < f64::EPSILON);
        assert!(candidate.module_key.starts_with("npm:repo-1:"));

        assert_eq!(evidence.source_type, "package_json");
        assert_eq!(evidence.source_path, "packages/core/package.json");
        assert_eq!(evidence.evidence_kind, "manifest_declaration");

        // Verify payload JSON
        let payload: NpmEvidencePayload = serde_json::from_str(&evidence.payload_json).unwrap();
        assert_eq!(payload.package_name, "@scope/core");
        assert!(payload.workspace_member);
        assert_eq!(payload.version, Some("1.0.0".to_string()));
    }
}
