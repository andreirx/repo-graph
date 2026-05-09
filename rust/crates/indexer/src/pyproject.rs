//! pyproject.toml module extraction (rust-module-parity Phase 2c).
//!
//! Extracts declared module candidates from pyproject.toml manifests.
//! Single-package support only (no workspace/monorepo patterns yet).
//!
//! # Identity Contract
//!
//! Module key format: `pyproject:{repo_uid}:{package_root_path}`
//!
//! Examples:
//! - `pyproject:django:.` (root package)
//! - `pyproject:monorepo:packages/core` (future workspace member)
//!
//! Path-anchored identity. Same rule as Cargo and npm.
//!
//! # Evidence Structure
//!
//! Each module candidate has associated evidence:
//! - `source_type` = "pyproject_toml"
//! - `source_path` = path to the pyproject.toml file
//! - `evidence_kind` = "manifest_declaration"
//! - `payload_json` contains:
//!   - `package_name`: name from [project].name
//!   - `package_root`: directory containing pyproject.toml
//!   - `version`: optional package version
//!
//! # Scope
//!
//! Phase 2c scope (single-package):
//! - [project].name and [project].version parsing
//! - Root package detection only
//!
//! Not in scope:
//! - Python workspace/monorepo patterns
//! - setup.py parsing
//! - Poetry-specific [tool.poetry] variants
//! - requirements.txt as module evidence

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Extraction output types ──────────────────────────────────────────

/// A package discovered from pyproject.toml parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyprojectModule {
    /// Package name from [project].name
    pub package_name: String,
    /// Package root directory (relative to repo root)
    pub package_root: String,
    /// Package version from [project].version (if present, not dynamic)
    pub version: Option<String>,
    /// Path to the pyproject.toml that declared this package
    pub manifest_path: String,
}

/// Result of parsing a pyproject.toml manifest.
#[derive(Debug, Clone)]
pub struct PyprojectParseResult {
    /// Discovered package module (if [project].name exists)
    pub module: Option<PyprojectModule>,
}

/// Evidence payload for pyproject.toml-derived modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyprojectEvidencePayload {
    /// Package name from [project].name
    pub package_name: String,
    /// Package root directory (relative to repo root)
    pub package_root: String,
    /// Package version (if present and not dynamic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// ── Parsing types (private) ──────────────────────────────────────────

/// Minimal pyproject.toml structure for package extraction.
#[derive(Debug, Deserialize)]
struct PyprojectToml {
    project: Option<ProjectSection>,
}

#[derive(Debug, Deserialize)]
struct ProjectSection {
    name: Option<String>,
    version: Option<String>,
    /// Dynamic fields — if "version" is in this list, we don't extract version
    dynamic: Option<Vec<String>>,
}

// ── Parsing functions ────────────────────────────────────────────────

/// Parse a pyproject.toml manifest and extract package metadata.
///
/// # Arguments
/// - `content`: raw pyproject.toml file content
/// - `manifest_path`: path to the pyproject.toml relative to repo root
///   (e.g., "pyproject.toml" for root, "packages/core/pyproject.toml" for nested)
///
/// # Returns
/// - `Ok(PyprojectParseResult)` with extracted module (if [project].name exists)
/// - `Err` if TOML parsing fails
///
/// # Notes
///
/// This function extracts from the `[project]` table per PEP 621.
/// Poetry-specific `[tool.poetry]` is not supported in Phase 2c.
///
/// If `version` is listed in `[project].dynamic`, it is not extracted
/// (the version is computed at build time, not declared in the manifest).
pub fn parse_pyproject_toml(
    content: &str,
    manifest_path: &str,
) -> Result<PyprojectParseResult, toml::de::Error> {
    let parsed: PyprojectToml = toml::from_str(content)?;

    // Derive package root from manifest path.
    // "pyproject.toml" -> "."
    // "packages/core/pyproject.toml" -> "packages/core"
    let package_root = manifest_path
        .strip_suffix("/pyproject.toml")
        .or_else(|| manifest_path.strip_suffix("pyproject.toml"))
        .map(|s| if s.is_empty() { "." } else { s.trim_end_matches('/') })
        .unwrap_or(".");

    // Extract module if [project].name exists.
    let module = parsed.project.and_then(|project| {
        project.name.map(|name| {
            // Check if version is dynamic
            let version = if project
                .dynamic
                .as_ref()
                .map(|d| d.contains(&"version".to_string()))
                .unwrap_or(false)
            {
                None // Version is dynamic, don't extract
            } else {
                project.version
            };

            PyprojectModule {
                package_name: name,
                package_root: package_root.to_string(),
                version,
                manifest_path: manifest_path.to_string(),
            }
        })
    });

    Ok(PyprojectParseResult { module })
}

// ── Identity generation ──────────────────────────────────────────────

/// Generate a deterministic module_candidate_uid for pyproject modules.
///
/// Identity is derived from: repo_uid + package_root + "declared"
pub fn generate_module_uid(repo_uid: &str, package_root: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"pyproject_module:");
    hasher.update(repo_uid.as_bytes());
    hasher.update(b":");
    hasher.update(package_root.as_bytes());
    hasher.update(b":declared");
    let hash = hasher.finalize();
    format!(
        "pyproject-mod-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate a deterministic evidence_uid.
pub fn generate_evidence_uid(module_uid: &str, manifest_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"pyproject_evidence:");
    hasher.update(module_uid.as_bytes());
    hasher.update(b":");
    hasher.update(manifest_path.as_bytes());
    let hash = hasher.finalize();
    format!(
        "pyproject-ev-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate the canonical module_key.
///
/// Format: `pyproject:{repo_uid}:{package_root_path}`
pub fn generate_module_key(repo_uid: &str, package_root: &str) -> String {
    format!("pyproject:{}:{}", repo_uid, package_root)
}

// ── Storage input conversion ─────────────────────────────────────────

use crate::cargo_manifest::{CargoModuleCandidateInput, CargoModuleEvidenceInput};

/// Convert a PyprojectModule to storage inputs.
///
/// Generates deterministic UIDs and evidence payload.
/// Returns the same input types as Cargo/npm (they're generic).
pub fn to_storage_inputs(
    module: &PyprojectModule,
    repo_uid: &str,
    snapshot_uid: &str,
) -> (CargoModuleCandidateInput, CargoModuleEvidenceInput) {
    let module_uid = generate_module_uid(repo_uid, &module.package_root);
    let module_key = generate_module_key(repo_uid, &module.package_root);
    let evidence_uid = generate_evidence_uid(&module_uid, &module.manifest_path);

    let payload = PyprojectEvidencePayload {
        package_name: module.package_name.clone(),
        package_root: module.package_root.clone(),
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
        source_type: "pyproject_toml".to_string(),
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
        let content = r#"
[project]
name = "my-package"
version = "1.0.0"
"#;
        let result = parse_pyproject_toml(content, "pyproject.toml").unwrap();
        assert!(result.module.is_some());
        let module = result.module.unwrap();
        assert_eq!(module.package_name, "my-package");
        assert_eq!(module.package_root, ".");
        assert_eq!(module.version, Some("1.0.0".to_string()));
    }

    #[test]
    fn parse_nested_package() {
        let content = r#"
[project]
name = "core"
version = "2.0.0"
"#;
        let result = parse_pyproject_toml(content, "packages/core/pyproject.toml").unwrap();
        assert!(result.module.is_some());
        let module = result.module.unwrap();
        assert_eq!(module.package_name, "core");
        assert_eq!(module.package_root, "packages/core");
        assert_eq!(module.manifest_path, "packages/core/pyproject.toml");
    }

    #[test]
    fn parse_dynamic_version() {
        let content = r#"
[project]
name = "Django"
dynamic = ["version"]
"#;
        let result = parse_pyproject_toml(content, "pyproject.toml").unwrap();
        assert!(result.module.is_some());
        let module = result.module.unwrap();
        assert_eq!(module.package_name, "Django");
        assert_eq!(module.version, None); // Dynamic version not extracted
    }

    #[test]
    fn parse_no_project_section() {
        let content = r#"
[build-system]
requires = ["setuptools"]
"#;
        let result = parse_pyproject_toml(content, "pyproject.toml").unwrap();
        assert!(result.module.is_none());
    }

    #[test]
    fn parse_project_without_name() {
        let content = r#"
[project]
version = "1.0.0"
description = "A package"
"#;
        let result = parse_pyproject_toml(content, "pyproject.toml").unwrap();
        assert!(result.module.is_none());
    }

    #[test]
    fn parse_django_style() {
        // Django's actual pyproject.toml structure
        let content = r#"
[build-system]
requires = ["setuptools>=77.0.3"]
build-backend = "setuptools.build_meta"

[project]
name = "Django"
dynamic = ["version"]
requires-python = ">= 3.12"
dependencies = [
    "asgiref>=3.9.1",
    "sqlparse>=0.5.0",
]
"#;
        let result = parse_pyproject_toml(content, "pyproject.toml").unwrap();
        assert!(result.module.is_some());
        let module = result.module.unwrap();
        assert_eq!(module.package_name, "Django");
        assert_eq!(module.package_root, ".");
        assert_eq!(module.version, None); // Dynamic
    }

    #[test]
    fn module_key_format() {
        let key = generate_module_key("django", ".");
        assert_eq!(key, "pyproject:django:.");
    }

    #[test]
    fn module_key_nested() {
        let key = generate_module_key("monorepo", "packages/core");
        assert_eq!(key, "pyproject:monorepo:packages/core");
    }

    #[test]
    fn uid_determinism() {
        let uid1 = generate_module_uid("repo", ".");
        let uid2 = generate_module_uid("repo", ".");
        assert_eq!(uid1, uid2);

        let uid3 = generate_module_uid("repo", "packages/core");
        assert_ne!(uid1, uid3);
    }

    #[test]
    fn uid_prefix_distinguishes_from_others() {
        let pyproject_uid = generate_module_uid("repo", ".");
        let cargo_uid = crate::cargo_manifest::generate_module_uid("repo", ".");
        let npm_uid = crate::package_json::generate_module_uid("repo", ".");

        assert!(pyproject_uid.starts_with("pyproject-mod-"));
        assert!(cargo_uid.starts_with("cargo-mod-"));
        assert!(npm_uid.starts_with("npm-mod-"));

        // All different
        assert_ne!(pyproject_uid, cargo_uid);
        assert_ne!(pyproject_uid, npm_uid);
        assert_ne!(cargo_uid, npm_uid);
    }

    #[test]
    fn storage_input_conversion() {
        let module = PyprojectModule {
            package_name: "Django".to_string(),
            package_root: ".".to_string(),
            version: None, // Dynamic
            manifest_path: "pyproject.toml".to_string(),
        };

        let (candidate, evidence) = to_storage_inputs(&module, "django", "snap-1");

        assert_eq!(candidate.module_kind, "declared");
        assert_eq!(candidate.canonical_root_path, ".");
        assert_eq!(candidate.display_name, "Django");
        assert!((candidate.confidence - 1.0).abs() < f64::EPSILON);
        assert_eq!(candidate.module_key, "pyproject:django:.");

        assert_eq!(evidence.source_type, "pyproject_toml");
        assert_eq!(evidence.source_path, "pyproject.toml");
        assert_eq!(evidence.evidence_kind, "manifest_declaration");

        // Verify payload JSON
        let payload: PyprojectEvidencePayload =
            serde_json::from_str(&evidence.payload_json).unwrap();
        assert_eq!(payload.package_name, "Django");
        assert_eq!(payload.version, None);
    }
}
