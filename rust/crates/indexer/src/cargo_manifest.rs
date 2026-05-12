//! Cargo.toml module extraction (rust-module-parity Phase 1).
//!
//! Extracts declared module candidates from Cargo.toml manifests.
//! This enables the Rust indexer to populate `module_candidates` and
//! `module_candidate_evidence` tables, unifying the module data model
//! between TS and Rust indexer paths.
//!
//! # Identity Contract
//!
//! Module key format: `cargo:{repo_uid}:{crate_root_path}`
//!
//! Examples:
//! - `cargo:repo-123:rust/crates/storage` (workspace member)
//! - `cargo:repo-123:.` (root crate in single-crate repo)
//!
//! # Evidence Structure
//!
//! Each module candidate has associated evidence:
//! - `source_type` = "cargo_toml"
//! - `source_path` = path to the Cargo.toml file
//! - `evidence_kind` = "manifest_declaration"
//! - `payload_json` contains:
//!   - `package_name`: crate name from [package].name
//!   - `crate_root`: directory containing Cargo.toml
//!   - `workspace_member`: true if discovered via [workspace].members
//!   - `version`: optional package version
//!
//! # Module Kind
//!
//! All Cargo.toml-derived modules are `module_kind` = "declared".
//! This distinguishes them from heuristic-inferred modules.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Extraction output types ──────────────────────────────────────────

/// A crate discovered from Cargo.toml parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoModule {
    /// Package name from [package].name
    pub package_name: String,
    /// Crate root directory (relative to repo root)
    /// For workspace members, this is the member path.
    /// For root crates, this is "." or empty.
    pub crate_root: String,
    /// Package version from [package].version (if present)
    pub version: Option<String>,
    /// Path to the Cargo.toml that declared this crate
    pub manifest_path: String,
    /// True if discovered via [workspace].members pattern
    pub is_workspace_member: bool,
}

/// Result of parsing a Cargo.toml manifest.
#[derive(Debug, Clone)]
pub struct CargoParseResult {
    /// Discovered crate modules
    pub modules: Vec<CargoModule>,
    /// Workspace member patterns (if this is a workspace root)
    pub workspace_members: Vec<String>,
    /// True if this manifest declares a [workspace]
    pub is_workspace_root: bool,
}

/// Evidence payload for Cargo.toml-derived modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoEvidencePayload {
    /// Package name from [package].name
    pub package_name: String,
    /// Crate root directory (relative to repo root)
    pub crate_root: String,
    /// True if discovered via [workspace].members
    pub workspace_member: bool,
    /// Package version (if present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// ── Parsing types (private) ──────────────────────────────────────────

/// Minimal Cargo.toml structure for package extraction.
#[derive(Debug, Deserialize)]
struct CargoToml {
    package: Option<PackageSection>,
    workspace: Option<WorkspaceSection>,
}

#[derive(Debug, Deserialize)]
struct PackageSection {
    name: String,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceSection {
    members: Option<Vec<String>>,
}

// ── Parsing functions ────────────────────────────────────────────────

/// Parse a Cargo.toml manifest and extract crate metadata.
///
/// # Arguments
/// - `content`: raw Cargo.toml file content
/// - `manifest_path`: path to the Cargo.toml relative to repo root
///   (e.g., "Cargo.toml" for root, "rust/crates/storage/Cargo.toml" for nested)
///
/// # Returns
/// - `Ok(CargoParseResult)` with extracted modules and workspace info
/// - `Err` if TOML parsing fails
///
/// # Notes
///
/// This function handles both:
/// - Single-crate repositories (just [package])
/// - Workspace roots ([workspace] with optional [package])
/// - Workspace members ([package] only, discovered via parent workspace)
///
/// For workspace roots, this function returns workspace member patterns
/// but does NOT resolve globs. Glob resolution requires filesystem access
/// and is the caller's responsibility.
pub fn parse_cargo_toml(
    content: &str,
    manifest_path: &str,
) -> Result<CargoParseResult, toml::de::Error> {
    let parsed: CargoToml = toml::from_str(content)?;

    let mut modules = Vec::new();
    let mut workspace_members = Vec::new();
    let is_workspace_root = parsed.workspace.is_some();

    // Derive crate root from manifest path.
    // "Cargo.toml" -> "."
    // "rust/crates/storage/Cargo.toml" -> "rust/crates/storage"
    let crate_root = manifest_path
        .strip_suffix("/Cargo.toml")
        .or_else(|| manifest_path.strip_suffix("Cargo.toml"))
        .map(|s| {
            if s.is_empty() {
                "."
            } else {
                s.trim_end_matches('/')
            }
        })
        .unwrap_or(".");

    // Extract package if present.
    if let Some(package) = parsed.package {
        modules.push(CargoModule {
            package_name: package.name,
            crate_root: crate_root.to_string(),
            version: package.version,
            manifest_path: manifest_path.to_string(),
            is_workspace_member: false, // Will be updated by caller if discovered via workspace
        });
    }

    // Extract workspace member patterns.
    if let Some(workspace) = parsed.workspace {
        if let Some(members) = workspace.members {
            workspace_members = members;
        }
    }

    Ok(CargoParseResult {
        modules,
        workspace_members,
        is_workspace_root,
    })
}

// ── Identity generation ──────────────────────────────────────────────

/// Generate a deterministic module_candidate_uid.
///
/// Identity is derived from: repo_uid + crate_root + "declared"
/// The "declared" discriminator ensures separation from future
/// inferred module UIDs.
pub fn generate_module_uid(repo_uid: &str, crate_root: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"cargo_module:");
    hasher.update(repo_uid.as_bytes());
    hasher.update(b":");
    hasher.update(crate_root.as_bytes());
    hasher.update(b":declared");
    let hash = hasher.finalize();
    format!(
        "cargo-mod-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate a deterministic evidence_uid.
pub fn generate_evidence_uid(module_uid: &str, manifest_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"cargo_evidence:");
    hasher.update(module_uid.as_bytes());
    hasher.update(b":");
    hasher.update(manifest_path.as_bytes());
    let hash = hasher.finalize();
    format!(
        "cargo-ev-{:x}",
        hash[..8].iter().fold(0u64, |acc, &b| acc << 8 | b as u64)
    )
}

/// Generate the canonical module_key.
///
/// Format: `cargo:{repo_uid}:{crate_root_path}`
pub fn generate_module_key(repo_uid: &str, crate_root: &str) -> String {
    format!("cargo:{}:{}", repo_uid, crate_root)
}

// ── Storage port input types ─────────────────────────────────────────

/// Input for inserting a module candidate from Cargo.toml.
#[derive(Debug, Clone)]
pub struct CargoModuleCandidateInput {
    /// Unique module candidate identifier (deterministic)
    pub module_candidate_uid: String,
    /// Snapshot UID
    pub snapshot_uid: String,
    /// Repository UID
    pub repo_uid: String,
    /// Module key: `cargo:{repo_uid}:{crate_root}`
    pub module_key: String,
    /// Module kind: always "declared" for Cargo.toml
    pub module_kind: String,
    /// Canonical root path (crate directory relative to repo root)
    pub canonical_root_path: String,
    /// Confidence: 1.0 for manifest-declared modules
    pub confidence: f64,
    /// Display name: package name
    pub display_name: String,
    /// Metadata JSON: optional additional info
    pub metadata_json: Option<String>,
}

/// Input for inserting module candidate evidence.
#[derive(Debug, Clone)]
pub struct CargoModuleEvidenceInput {
    /// Unique evidence identifier (deterministic)
    pub evidence_uid: String,
    /// Module candidate UID (FK)
    pub module_candidate_uid: String,
    /// Snapshot UID
    pub snapshot_uid: String,
    /// Repository UID
    pub repo_uid: String,
    /// Source type: "cargo_toml"
    pub source_type: String,
    /// Source path: path to Cargo.toml
    pub source_path: String,
    /// Evidence kind: "manifest_declaration"
    pub evidence_kind: String,
    /// Confidence: 1.0 for direct manifest declaration
    pub confidence: f64,
    /// Payload JSON with CargoEvidencePayload
    pub payload_json: String,
}

/// Input for inserting file ownership assignment.
#[derive(Debug, Clone)]
pub struct FileOwnershipInput {
    /// Snapshot UID
    pub snapshot_uid: String,
    /// Repository UID
    pub repo_uid: String,
    /// File UID (format: `{repo_uid}:{file_path}`)
    pub file_uid: String,
    /// Module candidate UID (FK to module_candidates)
    pub module_candidate_uid: String,
    /// Assignment kind: "manifest_prefix" for Cargo longest-prefix-match
    pub assignment_kind: String,
    /// Confidence: 1.0 for deterministic prefix assignment
    pub confidence: f64,
    /// Basis JSON: optional explanation of assignment
    pub basis_json: Option<String>,
}

// ── Storage port trait ───────────────────────────────────────────────

/// Storage port for Cargo.toml module candidate persistence.
///
/// The indexer (policy) owns this trait. The storage crate (adapter)
/// implements it on `StorageConnection`.
pub trait CargoModuleStorePort {
    /// Error type for storage operations.
    type Error: std::fmt::Debug + std::fmt::Display;

    /// Insert module candidates from Cargo.toml extraction.
    ///
    /// Returns the number of candidates inserted.
    fn insert_cargo_module_candidates(
        &mut self,
        candidates: &[CargoModuleCandidateInput],
    ) -> Result<usize, Self::Error>;

    /// Insert module candidate evidence from Cargo.toml extraction.
    ///
    /// Returns the number of evidence rows inserted.
    fn insert_cargo_module_evidence(
        &mut self,
        evidence: &[CargoModuleEvidenceInput],
    ) -> Result<usize, Self::Error>;

    /// Insert file ownership assignments.
    ///
    /// Returns the number of ownership rows inserted.
    fn insert_file_ownership(
        &mut self,
        ownership: &[FileOwnershipInput],
    ) -> Result<usize, Self::Error>;
}

// ── Conversion helpers ───────────────────────────────────────────────

/// Convert a CargoModule to storage inputs.
///
/// Generates deterministic UIDs and evidence payload.
pub fn to_storage_inputs(
    module: &CargoModule,
    repo_uid: &str,
    snapshot_uid: &str,
) -> (CargoModuleCandidateInput, CargoModuleEvidenceInput) {
    let module_uid = generate_module_uid(repo_uid, &module.crate_root);
    let module_key = generate_module_key(repo_uid, &module.crate_root);
    let evidence_uid = generate_evidence_uid(&module_uid, &module.manifest_path);

    let payload = CargoEvidencePayload {
        package_name: module.package_name.clone(),
        crate_root: module.crate_root.clone(),
        workspace_member: module.is_workspace_member,
        version: module.version.clone(),
    };

    let candidate = CargoModuleCandidateInput {
        module_candidate_uid: module_uid.clone(),
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        module_key,
        module_kind: "declared".to_string(),
        canonical_root_path: module.crate_root.clone(),
        confidence: 1.0,
        display_name: module.package_name.clone(),
        metadata_json: None,
    };

    let evidence = CargoModuleEvidenceInput {
        evidence_uid,
        module_candidate_uid: module_uid,
        snapshot_uid: snapshot_uid.to_string(),
        repo_uid: repo_uid.to_string(),
        source_type: "cargo_toml".to_string(),
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
[package]
name = "my-crate"
version = "0.1.0"
edition = "2021"
"#;
        let result = parse_cargo_toml(content, "Cargo.toml").unwrap();
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].package_name, "my-crate");
        assert_eq!(result.modules[0].crate_root, ".");
        assert_eq!(result.modules[0].version, Some("0.1.0".to_string()));
        assert!(!result.is_workspace_root);
        assert!(result.workspace_members.is_empty());
    }

    #[test]
    fn parse_nested_package() {
        let content = r#"
[package]
name = "storage"
version = "0.2.0"
"#;
        let result = parse_cargo_toml(content, "rust/crates/storage/Cargo.toml").unwrap();
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].package_name, "storage");
        assert_eq!(result.modules[0].crate_root, "rust/crates/storage");
        assert_eq!(
            result.modules[0].manifest_path,
            "rust/crates/storage/Cargo.toml"
        );
    }

    #[test]
    fn parse_workspace_root() {
        let content = r#"
[workspace]
members = [
    "crates/core",
    "crates/cli",
]

[package]
name = "workspace-root"
version = "0.1.0"
"#;
        let result = parse_cargo_toml(content, "Cargo.toml").unwrap();
        assert!(result.is_workspace_root);
        assert_eq!(result.workspace_members, vec!["crates/core", "crates/cli"]);
        // Root package still extracted
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].package_name, "workspace-root");
    }

    #[test]
    fn parse_virtual_workspace() {
        let content = r#"
[workspace]
members = ["packages/*"]
"#;
        let result = parse_cargo_toml(content, "Cargo.toml").unwrap();
        assert!(result.is_workspace_root);
        assert_eq!(result.workspace_members, vec!["packages/*"]);
        // No root package in virtual workspace
        assert!(result.modules.is_empty());
    }

    #[test]
    fn module_key_format() {
        let key = generate_module_key("repo-123", "rust/crates/storage");
        assert_eq!(key, "cargo:repo-123:rust/crates/storage");
    }

    #[test]
    fn module_key_root_crate() {
        let key = generate_module_key("repo-456", ".");
        assert_eq!(key, "cargo:repo-456:.");
    }

    #[test]
    fn uid_determinism() {
        let uid1 = generate_module_uid("repo", "crates/foo");
        let uid2 = generate_module_uid("repo", "crates/foo");
        assert_eq!(uid1, uid2);

        let uid3 = generate_module_uid("repo", "crates/bar");
        assert_ne!(uid1, uid3);
    }

    #[test]
    fn storage_input_conversion() {
        let module = CargoModule {
            package_name: "my-crate".to_string(),
            crate_root: "crates/my-crate".to_string(),
            version: Some("1.0.0".to_string()),
            manifest_path: "crates/my-crate/Cargo.toml".to_string(),
            is_workspace_member: true,
        };

        let (candidate, evidence) = to_storage_inputs(&module, "repo-1", "snap-1");

        assert_eq!(candidate.module_kind, "declared");
        assert_eq!(candidate.canonical_root_path, "crates/my-crate");
        assert_eq!(candidate.display_name, "my-crate");
        assert!((candidate.confidence - 1.0).abs() < f64::EPSILON);
        assert!(candidate.module_key.starts_with("cargo:repo-1:"));

        assert_eq!(evidence.source_type, "cargo_toml");
        assert_eq!(evidence.source_path, "crates/my-crate/Cargo.toml");
        assert_eq!(evidence.evidence_kind, "manifest_declaration");

        // Verify payload JSON
        let payload: CargoEvidencePayload = serde_json::from_str(&evidence.payload_json).unwrap();
        assert_eq!(payload.package_name, "my-crate");
        assert!(payload.workspace_member);
        assert_eq!(payload.version, Some("1.0.0".to_string()));
    }
}
