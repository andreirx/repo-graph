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
//!
//! # Workspace field inheritance (CARGO-WORKSPACE-INHERITANCE-1)
//!
//! Modern Cargo workspaces (RFC 2906) let a member crate inherit `[package]`
//! fields from the root `[workspace.package]` table via `field.workspace = true`
//! (e.g. `version.workspace = true`). `name` is always explicit — Cargo forbids
//! inheriting it — so a crate is always identifiable from its own manifest.
//!
//! This reader consumes exactly one typed inheritable field, `version`. When it
//! is written as `version.workspace = true` the value is a TOML *table*, not a
//! string; before this slice that crashed `version` deserialization
//! (`invalid type: map, expected a string`) and, because the error failed the
//! WHOLE manifest, suppressed the candidate entirely. On repo-graph itself that
//! hid five crates (rgr, daemon-runtime, graph-algorithms, platform-paths,
//! rmapd). Every other inheritable field (`edition`, `rust-version`, `license`,
//! …) is not declared here, so serde ignores it and it never affected parsing.
//!
//! The fix keeps this reader a *pure single-manifest parser*: it tolerates the
//! inherited-table form and still emits the candidate (crate identity = name +
//! location is a deterministic fact from the member manifest alone), leaving the
//! inherited `version` value **unresolved** (`None` = not measured, never
//! fabricated). Resolving the literal against the enclosing root's
//! `[workspace.package]` table — and honest-skipping a garbled/missing root with
//! an extraction diagnostic — requires the root table and the diagnostics sink,
//! both of which live at the CALLER (`repo-index`'s `compose.rs`), exactly like
//! the existing caller-owned workspace-member glob expansion. That cross-file
//! resolution is intentionally NOT done here.

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
    /// The raw `[package].version` node. Deserialized as an untyped
    /// `toml::Value` so BOTH shapes parse without error:
    /// - an explicit literal `version = "0.1.0"` → `Value::String`
    /// - workspace inheritance `version.workspace = true` → `Value::Table`
    ///
    /// `version` is the only typed inheritable field this reader consumes, so
    /// its `.workspace = true` table form was the only one that crashed the
    /// deserialization (`invalid type: map, expected a string`) and suppressed
    /// the whole candidate. Other inheritable fields (`edition`, `rust-version`,
    /// `license`, …) are not declared here, so serde ignores them — they never
    /// affected parsing. Accepting `Value` also means a genuinely malformed
    /// `version` never fails the manifest and hides a real crate: the identity
    /// still emits, the version just stays unresolved.
    version: Option<toml::Value>,
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
        // Resolve the version to a literal where the manifest carries one.
        // Explicit `version = "x"` is recorded as-is. Workspace-inherited
        // `version.workspace = true` cannot be resolved here — the literal
        // lives in the enclosing root's [workspace.package] table, not this
        // manifest — so it stays None: honestly unresolved (not measured),
        // never fabricated. The candidate is STILL emitted, because the crate's
        // identity (name + location) is a deterministic fact from this manifest
        // alone. Cross-file resolution against the root table is a caller
        // concern (see the CARGO-WORKSPACE-INHERITANCE-1 residual), mirroring
        // how glob expansion of workspace members already lives in the caller.
        let version = match package.version {
            // Explicit literal `version = "0.1.0"`: record it as-is.
            Some(toml::Value::String(v)) => Some(v),
            // Workspace-inherited (`version.workspace = true` → a Table) or any
            // other non-string shape: unresolved here.
            Some(_) | None => None,
        };
        modules.push(CargoModule {
            package_name: package.name,
            crate_root: crate_root.to_string(),
            version,
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
    fn parse_inheriting_package_emits_candidate() {
        // A workspace MEMBER crate that inherits fields from the root
        // [workspace.package] table via `field.workspace = true`. This is the
        // exact shape of repo-graph's own rgr / daemon-runtime / graph-algorithms
        // / platform-paths / rmapd crates. `name` is always explicit (Cargo
        // forbids inheriting it); version/edition/rust-version/license inherit.
        //
        // Before CARGO-WORKSPACE-INHERITANCE-1 the `version.workspace = true`
        // TABLE crashed the `version: Option<String>` deserialization, which
        // failed the WHOLE manifest parse and suppressed the candidate — the
        // crate went invisible to every module-model consumer.
        let content = r#"
[package]
name                   = "repo-graph-rgr"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
description            = "Minimal CLI"
publish                = false
"#;
        let result = parse_cargo_toml(content, "rust/crates/rgr/Cargo.toml").unwrap();
        assert_eq!(
            result.modules.len(),
            1,
            "inheriting crate must emit exactly one candidate"
        );
        assert_eq!(result.modules[0].package_name, "repo-graph-rgr");
        assert_eq!(result.modules[0].crate_root, "rust/crates/rgr");
        // Version is workspace-inherited. A single-manifest pure parser cannot
        // resolve it against the enclosing root's [workspace.package] table
        // (that is the caller's job, mirroring glob resolution), so it is
        // honestly left unresolved: None = not measured, never fabricated.
        assert_eq!(result.modules[0].version, None);
        assert!(!result.is_workspace_root);
    }

    #[test]
    fn parse_explicit_version_with_inherited_siblings() {
        // Explicit `version` alongside OTHER inherited fields. This proves the
        // non-version inheritable fields (`edition`, `rust-version`, `license`)
        // never affected parsing: they are not declared on `PackageSection`, so
        // serde ignores them. Only `version.workspace` ever broke the parse.
        let content = r#"
[package]
name                   = "repo-graph-coverage"
version                = "0.1.0"
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
"#;
        let result = parse_cargo_toml(content, "rust/crates/coverage/Cargo.toml").unwrap();
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].package_name, "repo-graph-coverage");
        assert_eq!(result.modules[0].version, Some("0.1.0".to_string()));
    }

    #[test]
    fn inheriting_candidate_evidence_shape_matches_explicit() {
        // "Same evidence shape as an explicit manifest": the storage candidate
        // + evidence a fold consumes are byte-for-byte the same kind for an
        // inheriting crate as for an explicit one. The ONLY difference is the
        // optional `version` inside the evidence payload (absent = unresolved),
        // which no fold/renderer consumes for grouping.
        let inheriting = CargoModule {
            package_name: "repo-graph-rgr".to_string(),
            crate_root: "rust/crates/rgr".to_string(),
            version: None, // workspace-inherited, unresolved by the single-manifest parser
            manifest_path: "rust/crates/rgr/Cargo.toml".to_string(),
            is_workspace_member: true,
        };
        let (candidate, evidence) = to_storage_inputs(&inheriting, "repo-1", "snap-1");

        // Identity + shape fields the module model actually folds on.
        assert_eq!(candidate.module_kind, "declared");
        assert_eq!(candidate.canonical_root_path, "rust/crates/rgr");
        assert_eq!(candidate.display_name, "repo-graph-rgr");
        assert!((candidate.confidence - 1.0).abs() < f64::EPSILON);
        assert_eq!(candidate.module_key, "cargo:repo-1:rust/crates/rgr");

        assert_eq!(evidence.source_type, "cargo_toml");
        assert_eq!(evidence.source_path, "rust/crates/rgr/Cargo.toml");
        assert_eq!(evidence.evidence_kind, "manifest_declaration");

        // Payload carries identity; version is honestly omitted (unresolved),
        // never rendered as a fabricated value.
        let payload: CargoEvidencePayload = serde_json::from_str(&evidence.payload_json).unwrap();
        assert_eq!(payload.package_name, "repo-graph-rgr");
        assert_eq!(payload.crate_root, "rust/crates/rgr");
        assert_eq!(payload.version, None);
        assert!(!evidence.payload_json.contains("version"));
    }

    #[test]
    fn inheriting_package_emitted_not_skipped_in_single_manifest_parse() {
        // SCOPE BOUNDARY (documented deviation from slice §4 test 4):
        // Slice §2.1 also asks that a `.workspace = true` field with a
        // missing/garbled enclosing [workspace.package] table be HONEST-SKIPPED
        // + logged as an extraction diagnostic. That requires the root table AND
        // the diagnostics sink, both of which live at the CALLER (compose.rs in
        // repo-index), not in this single-manifest pure reader — mirroring the
        // existing caller-owned glob resolution + `merge_extraction_diagnostics`.
        //
        // In-scope, this reader can only decide from the member manifest alone.
        // It therefore EMITS the candidate (the crate is a real, deterministic
        // fact) with the version unresolved, rather than skipping. The honest-
        // skip variant is deferred to the caller — see the run's DECISION_REQUIRED.
        let content = r#"
[package]
name              = "orphan-crate"
version.workspace = true
"#;
        let result = parse_cargo_toml(content, "crates/orphan/Cargo.toml").unwrap();
        assert_eq!(result.modules.len(), 1);
        assert_eq!(result.modules[0].package_name, "orphan-crate");
        assert_eq!(result.modules[0].version, None);
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
