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

use crate::types::NpmDeclaredEntry;

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
    /// TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L04): every root target the manifest declares
    /// EXPLICITLY (`exports`, else `main`), as repo-relative paths in manifest order, without
    /// duplicates, each tagged with its origin — see [`declared_entries`]. Never narrowed to one
    /// target (the importer's mode and bundler conditions are not known to the index) and never
    /// Node's implicit `index.js`. Empty when the manifest declares no root entry or its declaration
    /// is undeterminable. Read only by the workspace import stage; never persisted (the evidence
    /// payload is unchanged).
    pub declared_entries: Vec<NpmDeclaredEntry>,
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
    /// TS-WORKSPACE-RESOLUTION-1: `main` and `exports` are read as raw JSON (`Some` whenever the
    /// key is present, `null` included), never as typed fields, so a manifest whose `main` is a
    /// number or whose `exports` is any JSON parses exactly as before.
    #[serde(default, deserialize_with = "present_json")]
    main: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "present_json")]
    exports: Option<serde_json::Value>,
}

/// A key that is present yields `Some(value)` — `Some(Value::Null)` for an explicit `null` — so
/// "present and null" stays distinct from "absent" (`exports: null` still declares the root).
fn present_json<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde_json::Value::deserialize(deserializer).map(Some)
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
    let entries = declared_entries(package_root, parsed.main.as_ref(), parsed.exports.as_ref());
    let module = parsed.name.map(|name| NpmModule {
        package_name: name,
        package_root: package_root.to_string(),
        version: parsed.version,
        manifest_path: manifest_path.to_string(),
        is_workspace_member: false, // Will be updated by caller if discovered via workspace
        source_type: "package_json".to_string(),
        declared_entries: entries,
    });

    Ok(PackageJsonParseResult {
        module,
        workspace_patterns,
        is_workspace_root,
    })
}

// ── Declared entries (TS-WORKSPACE-RESOLUTION-1) ─────────────────────

/// The manifest's declaration could not be read as a list of root targets (a mixed `exports`
/// object, a nested subpath map under a condition, a non-string target, a target without `./`, a
/// target that leaves the package, or a non-string `main`).
struct Undeterminable;

/// TS-WORKSPACE-RESOLUTION-1 (RG-REQ-006-L04): every root target a package manifest declares
/// EXPLICITLY, as repo-relative paths under `package_root`, in manifest order, deduplicated, each
/// tagged with its origin: [`NpmDeclaredEntry::Export`] for a target of `exports`,
/// [`NpmDeclaredEntry::Main`] for the target of `main` (D-TWR-ENTRY-ORIGIN).
///
/// - `exports`, when the key is present (`null` included), alone declares the root (Node ignores
///   `main` then): a string is the root target; an object whose keys all start with `.` is a
///   subpath map whose `"."` value is the root (no `"."` → no root); a conditions object
///   contributes the targets of EVERY condition in its key order, recursively; an array every
///   element; `null` nothing.
/// - else a non-empty string `main`, with or without `./`.
/// - else nothing: Node's implicit `index.js` is never a declared entry.
///
/// Every target must start with `./` (`main` excepted) and stay inside the package after
/// normalization. An undeterminable declaration yields no entries. PURE.
pub fn declared_entries(
    package_root: &str,
    main: Option<&serde_json::Value>,
    exports: Option<&serde_json::Value>,
) -> Vec<NpmDeclaredEntry> {
    let targets = match declared_targets(package_root, main, exports) {
        Ok(targets) => targets,
        Err(Undeterminable) => return Vec::new(),
    };
    let mut entries: Vec<NpmDeclaredEntry> = Vec::with_capacity(targets.len());
    for t in targets {
        if !entries.contains(&t) {
            entries.push(t);
        }
    }
    entries
}

fn declared_targets(
    package_root: &str,
    main: Option<&serde_json::Value>,
    exports: Option<&serde_json::Value>,
) -> Result<Vec<NpmDeclaredEntry>, Undeterminable> {
    use serde_json::Value;
    if let Some(exports) = exports {
        let root_value = match exports {
            Value::Object(map) if map.keys().any(|k| k.starts_with('.')) => {
                if !map.keys().all(|k| k.starts_with('.')) {
                    return Err(Undeterminable);
                }
                match map.get(".") {
                    Some(root) => root,
                    None => return Ok(Vec::new()),
                }
            }
            other => other,
        };
        let mut out = Vec::new();
        collect_export_targets(package_root, root_value, &mut out)?;
        return Ok(out.into_iter().map(NpmDeclaredEntry::Export).collect());
    }
    match main {
        None => Ok(Vec::new()),
        Some(Value::String(m)) if m.is_empty() => Ok(Vec::new()),
        Some(Value::String(m)) => {
            let target = if m.starts_with("./") {
                m.clone()
            } else {
                format!("./{m}")
            };
            Ok(vec![NpmDeclaredEntry::Main(package_target(
                package_root,
                &target,
            )?)])
        }
        Some(_) => Err(Undeterminable),
    }
}

/// The targets of one root export value (string, conditions object, array or null), in order.
fn collect_export_targets(
    package_root: &str,
    value: &serde_json::Value,
    out: &mut Vec<String>,
) -> Result<(), Undeterminable> {
    use serde_json::Value;
    match value {
        Value::Null => Ok(()),
        Value::String(target) => {
            out.push(package_target(package_root, target)?);
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                collect_export_targets(package_root, item, out)?;
            }
            Ok(())
        }
        Value::Object(conditions) => {
            if conditions.keys().any(|k| k.starts_with('.')) {
                // A nested subpath map under a condition.
                return Err(Undeterminable);
            }
            for target in conditions.values() {
                collect_export_targets(package_root, target, out)?;
            }
            Ok(())
        }
        Value::Bool(_) | Value::Number(_) => Err(Undeterminable),
    }
}

/// A `./`-relative manifest target as a repo-relative path under `package_root` (`.` = the repo
/// root). `.` and empty segments are dropped, `..` pops; a target that leaves the package, or names
/// the package directory itself, is undeterminable.
fn package_target(package_root: &str, target: &str) -> Result<String, Undeterminable> {
    let rest = target.strip_prefix("./").ok_or(Undeterminable)?;
    let mut segments: Vec<&str> = if package_root == "." {
        Vec::new()
    } else {
        package_root.split('/').collect()
    };
    let base = segments.len();
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.len() <= base {
                    return Err(Undeterminable);
                }
                segments.pop();
            }
            name => segments.push(name),
        }
    }
    if segments.len() == base {
        return Err(Undeterminable);
    }
    Ok(segments.join("/"))
}

/// TS-WORKSPACE-RESOLUTION-1: project the parsed npm workspace MEMBERS into the raw declared-module
/// catalog carried across the compose→indexer boundary (`ecosystem: "npm"`, the package name, the
/// package root, the declared entries). The root package and non-members are not members and are
/// not projected; a member without declared entries is projected (the import stage declines it,
/// and it still counts when two members declare one name).
pub fn declared_modules_from_npm<'a>(
    modules: impl IntoIterator<Item = &'a NpmModule>,
) -> Vec<crate::types::DeclaredModule> {
    modules
        .into_iter()
        .filter(|m| m.is_workspace_member)
        .map(|m| crate::types::DeclaredModule {
            ecosystem: "npm".to_string(),
            name: m.package_name.clone(),
            canonical_root: m.package_root.clone(),
            npm_declared_entries: m.declared_entries.clone(),
        })
        .collect()
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
            declared_entries: Vec::new(),
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

    // ── Declared entries (TS-WORKSPACE-RESOLUTION-1, RG-REQ-006-L04) ──

    fn entries_of(manifest: &str, path: &str) -> Vec<NpmDeclaredEntry> {
        parse_package_json(manifest, path)
            .expect("the manifest parses")
            .module
            .expect("a named package")
            .declared_entries
    }

    /// Expected entries declared by `main` (completed by Node).
    fn mains(paths: &[&str]) -> Vec<NpmDeclaredEntry> {
        paths
            .iter()
            .map(|p| NpmDeclaredEntry::Main(p.to_string()))
            .collect()
    }

    /// Expected entries declared by the root `exports` (matched as written).
    fn exports(paths: &[&str]) -> Vec<NpmDeclaredEntry> {
        paths
            .iter()
            .map(|p| NpmDeclaredEntry::Export(p.to_string()))
            .collect()
    }

    #[test]
    fn declared_entries_are_main_when_there_are_no_exports() {
        // FRAKTAG packages/engine/package.json: `"main": "dist/index.js"` (no `./`).
        let m = r#"{"name":"@fraktag/engine","main":"dist/index.js","types":"dist/index.d.ts"}"#;
        assert_eq!(
            entries_of(m, "packages/engine/package.json"),
            mains(&["packages/engine/dist/index.js"])
        );
        // With `./`, and at the repository root.
        let m = r#"{"name":"root","main":"./lib/main.js"}"#;
        assert_eq!(entries_of(m, "package.json"), mains(&["lib/main.js"]));
    }

    #[test]
    fn declared_entries_are_every_leaf_of_the_root_export_in_manifest_order() {
        // amodx packages/effects: a conditional root export — BOTH targets, in key order, none
        // selected.
        let m = r#"{"name":"@amodx/effects","main":"dist/index.js",
            "exports":{".":{"types":"./dist/index.d.ts","default":"./dist/index.js"},
                       "./package.json":"./package.json"}}"#;
        assert_eq!(
            entries_of(m, "packages/effects/package.json"),
            exports(&[
                "packages/effects/dist/index.d.ts",
                "packages/effects/dist/index.js"
            ])
        );
        // A `require` placed before `import` changes only the recorded order.
        let m = r#"{"name":"p","exports":{"require":"./dist/a.cjs","import":"./dist/a.mjs"}}"#;
        assert_eq!(
            entries_of(m, "p/package.json"),
            exports(&["p/dist/a.cjs", "p/dist/a.mjs"])
        );
        let m = r#"{"name":"p","exports":{"import":"./dist/a.mjs","require":"./dist/a.cjs"}}"#;
        assert_eq!(
            entries_of(m, "p/package.json"),
            exports(&["p/dist/a.mjs", "p/dist/a.cjs"])
        );
        // A string export, nested conditions, an array, a null leaf and a duplicate.
        let m = r#"{"name":"p","exports":"./index.js"}"#;
        assert_eq!(entries_of(m, "p/package.json"), exports(&["p/index.js"]));
        let m = r#"{"name":"p","exports":{".":{"node":{"import":"./n.mjs","require":"./n.cjs"},
            "browser":["./b.js",null],"code":"./src/index.ts","default":"./n.mjs"}}}"#;
        assert_eq!(
            entries_of(m, "p/package.json"),
            exports(&["p/n.mjs", "p/n.cjs", "p/b.js", "p/src/index.ts"])
        );
        // `exports` alone declares the root: `main` is ignored when `exports` is present.
        let m = r#"{"name":"p","main":"./main.js","exports":{".":"./exported.js"}}"#;
        assert_eq!(entries_of(m, "p/package.json"), exports(&["p/exported.js"]));
    }

    #[test]
    fn undeterminable_entries_leave_the_member_without_declared_entries() {
        for m in [
            // `.`-keys mixed with condition keys.
            r#"{"name":"p","exports":{".":"./a.js","import":"./b.js"}}"#,
            // A nested subpath map under a condition.
            r#"{"name":"p","exports":{".":{"import":{"./x":"./x.js"}}}}"#,
            // A non-string target.
            r#"{"name":"p","exports":{".":{"import":7}}}"#,
            r#"{"name":"p","exports":true}"#,
            // A target without `./`.
            r#"{"name":"p","exports":{".":"dist/index.js"}}"#,
            // A target that leaves the package.
            r#"{"name":"p","exports":"./../q/index.js"}"#,
            r#"{"name":"p","main":"../q/index.js"}"#,
            // A target that names the package directory itself.
            r#"{"name":"p","exports":"./"}"#,
            // A non-string `main`.
            r#"{"name":"p","main":3}"#,
            r#"{"name":"p","main":["./a.js"]}"#,
        ] {
            assert!(entries_of(m, "p/package.json").is_empty(), "{m}");
        }
    }

    #[test]
    fn declared_entries_are_none_without_a_root_export() {
        for m in [
            // Subpaths only (amodx `@amodx/plugins` exports `./admin` and `./render`).
            r#"{"name":"p","main":"./main.js","exports":{"./admin":"./dist/admin.js","./render":"./dist/render.js"}}"#,
            // `exports: null` declares the root as nothing; `main` is not consulted.
            r#"{"name":"p","main":"./main.js","exports":null}"#,
            r#"{"name":"p","exports":{".":null}}"#,
        ] {
            assert!(entries_of(m, "p/package.json").is_empty(), "{m}");
        }
    }

    #[test]
    fn declared_entries_are_none_without_main_or_exports_never_an_implicit_index_js() {
        for m in [
            r#"{"name":"p"}"#,
            r#"{"name":"p","main":""}"#,
            r#"{"name":"p","main":null}"#,
            r#"{"name":"p","types":"./dist/index.d.ts","module":"./dist/index.mjs"}"#,
        ] {
            let entries = entries_of(m, "p/package.json");
            assert!(entries.is_empty(), "{m}: {entries:?}");
        }
    }

    #[test]
    fn declared_entries_are_normalized_under_the_package_root() {
        let m = r#"{"name":"p","main":"./dist/../lib//./index.js"}"#;
        assert_eq!(
            entries_of(m, "packages/p/package.json"),
            mains(&["packages/p/lib/index.js"])
        );
        let m = r#"{"name":"p","exports":{".":{"types":"./a/../types/index.d.ts"}}}"#;
        assert_eq!(
            entries_of(m, "package.json"),
            exports(&["types/index.d.ts"])
        );
    }

    #[test]
    fn non_string_main_or_exports_never_fails_the_manifest_parse() {
        for m in [
            r#"{"name":"p","main":7}"#,
            r#"{"name":"p","main":{"x":1}}"#,
            r#"{"name":"p","exports":12}"#,
            r#"{"name":"p","exports":[1,{"a":false}]}"#,
            r#"{"name":"p","exports":{".":{"import":[null,3]}},"main":false}"#,
        ] {
            let parsed = parse_package_json(m, "p/package.json").expect(m);
            let module = parsed.module.expect("the named package is still extracted");
            assert_eq!(module.package_name, "p");
            assert_eq!(module.package_root, "p");
            assert!(module.declared_entries.is_empty(), "{m}");
        }
    }

    #[test]
    fn declared_modules_carry_npm_workspace_members_with_their_declared_entries() {
        let member = |name: &str, root: &str, entries: &[&str], member: bool| NpmModule {
            package_name: name.to_string(),
            package_root: root.to_string(),
            version: None,
            manifest_path: format!("{root}/package.json"),
            is_workspace_member: member,
            source_type: "package_json".to_string(),
            declared_entries: mains(entries),
        };
        let modules = [
            member("fraktag", ".", &["dist/index.js"], false),
            member(
                "@fraktag/engine",
                "packages/engine",
                &["packages/engine/dist/index.js"],
                true,
            ),
            member("@fraktag/ui", "packages/ui", &[], true),
        ];
        let declared = declared_modules_from_npm(modules.iter());
        assert_eq!(
            declared,
            vec![
                crate::types::DeclaredModule {
                    ecosystem: "npm".to_string(),
                    name: "@fraktag/engine".to_string(),
                    canonical_root: "packages/engine".to_string(),
                    npm_declared_entries: mains(&["packages/engine/dist/index.js"]),
                },
                crate::types::DeclaredModule {
                    ecosystem: "npm".to_string(),
                    name: "@fraktag/ui".to_string(),
                    canonical_root: "packages/ui".to_string(),
                    npm_declared_entries: vec![],
                },
            ],
            "workspace members only — the root package is not a member"
        );
    }
}
