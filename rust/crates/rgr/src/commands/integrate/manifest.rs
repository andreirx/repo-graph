//! Install manifest recording for host integrations.
//!
//! This module handles:
//! - Read/update manifest host_integrations section
//! - No hook merge logic (that's in config.rs)
//!
//! The install manifest lives at (per DIST-1 contract):
//! - macOS: ~/Library/Application Support/repo-graph/install-manifest.json
//! - Linux: ~/.config/rmap/install-manifest.json

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A recorded host integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostIntegration {
    /// Host identifier (e.g., "claude-code", "codex")
    pub host: String,
    /// Scope: "global" or "project"
    pub scope: String,
    /// Path to the config file that was modified
    pub config_path: String,
    /// Path to the backup file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// When the integration was installed
    pub installed_at: DateTime<Utc>,
    /// Which hooks were installed
    pub hooks_installed: Vec<String>,
    /// Profile: "minimal" or "full"
    pub profile: String,
}

/// Install manifest structure.
///
/// This preserves all existing manifest fields (schema_version, installed_at, platform,
/// arch, components, directories, service, etc.) while adding/updating host_integrations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstallManifest {
    /// List of host integrations (added by CLAUDE-1)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_integrations: Vec<HostIntegration>,
    /// All other manifest fields preserved unchanged
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

impl InstallManifest {
    /// Find an existing integration by host and scope.
    pub fn find_integration(&self, host: &str, scope: &str) -> Option<&HostIntegration> {
        self.host_integrations
            .iter()
            .find(|i| i.host == host && i.scope == scope)
    }

    /// Add or update an integration record.
    pub fn upsert_integration(&mut self, integration: HostIntegration) {
        // Remove existing entry for same host/scope
        self.host_integrations
            .retain(|i| !(i.host == integration.host && i.scope == integration.scope));
        // Add new entry
        self.host_integrations.push(integration);
    }

    /// Remove an integration record.
    pub fn remove_integration(&mut self, host: &str, scope: &str) -> Option<HostIntegration> {
        let idx = self
            .host_integrations
            .iter()
            .position(|i| i.host == host && i.scope == scope);

        idx.map(|i| self.host_integrations.remove(i))
    }
}

/// Get the platform-specific manifest directory.
pub fn manifest_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Application Support/repo-graph"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::home_dir().map(|h| h.join(".config/rmap"))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        dirs::home_dir().map(|h| h.join(".rmap"))
    }
}

/// Get the full path to the manifest file.
pub fn manifest_path() -> Option<PathBuf> {
    manifest_dir().map(|d| d.join("install-manifest.json"))
}

/// Load the install manifest, creating default if not found.
pub fn load_manifest() -> Result<InstallManifest, String> {
    let Some(path) = manifest_path() else {
        return Ok(InstallManifest::default());
    };

    if !path.exists() {
        return Ok(InstallManifest::default());
    }

    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("failed to read manifest: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("failed to parse manifest: {}", e))
}

/// Save the install manifest.
pub fn save_manifest(manifest: &InstallManifest) -> Result<(), String> {
    let Some(path) = manifest_path() else {
        return Err("could not determine manifest path".to_string());
    };

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create manifest directory: {}", e))?;
    }

    let content = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("failed to serialize manifest: {}", e))?;

    std::fs::write(&path, content).map_err(|e| format!("failed to write manifest: {}", e))
}

/// Record a new host integration in the manifest.
pub fn record_integration(
    host: &str,
    scope: &str,
    config_path: &Path,
    backup_path: Option<&Path>,
    hooks_installed: Vec<String>,
    profile: &str,
) -> Result<(), String> {
    let mut manifest = load_manifest()?;

    let integration = HostIntegration {
        host: host.to_string(),
        scope: scope.to_string(),
        config_path: config_path.display().to_string(),
        backup_path: backup_path.map(|p| p.display().to_string()),
        installed_at: Utc::now(),
        hooks_installed,
        profile: profile.to_string(),
    };

    manifest.upsert_integration(integration);
    save_manifest(&manifest)
}

/// Remove a host integration from the manifest.
pub fn remove_integration_record(
    host: &str,
    scope: &str,
) -> Result<Option<HostIntegration>, String> {
    let mut manifest = load_manifest()?;
    let removed = manifest.remove_integration(host, scope);
    save_manifest(&manifest)?;
    Ok(removed)
}

/// Get an integration record from the manifest.
pub fn get_integration(host: &str, scope: &str) -> Result<Option<HostIntegration>, String> {
    let manifest = load_manifest()?;
    Ok(manifest.find_integration(host, scope).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_upsert() {
        let mut manifest = InstallManifest::default();

        let integration = HostIntegration {
            host: "claude-code".to_string(),
            scope: "global".to_string(),
            config_path: "~/.claude/settings.json".to_string(),
            backup_path: Some("~/.claude/settings.json.rmap-backup".to_string()),
            installed_at: Utc::now(),
            hooks_installed: vec!["SessionStart".to_string(), "Stop".to_string()],
            profile: "minimal".to_string(),
        };

        manifest.upsert_integration(integration.clone());
        assert_eq!(manifest.host_integrations.len(), 1);

        // Upsert same host/scope updates
        manifest.upsert_integration(integration);
        assert_eq!(manifest.host_integrations.len(), 1);

        // Different scope adds new entry
        let project_integration = HostIntegration {
            host: "claude-code".to_string(),
            scope: "project".to_string(),
            config_path: "./.claude/settings.json".to_string(),
            backup_path: None,
            installed_at: Utc::now(),
            hooks_installed: vec!["SessionStart".to_string()],
            profile: "minimal".to_string(),
        };
        manifest.upsert_integration(project_integration);
        assert_eq!(manifest.host_integrations.len(), 2);
    }

    #[test]
    fn test_manifest_find() {
        let mut manifest = InstallManifest::default();
        let integration = HostIntegration {
            host: "claude-code".to_string(),
            scope: "global".to_string(),
            config_path: "test".to_string(),
            backup_path: None,
            installed_at: Utc::now(),
            hooks_installed: vec![],
            profile: "minimal".to_string(),
        };
        manifest.upsert_integration(integration);

        assert!(manifest.find_integration("claude-code", "global").is_some());
        assert!(manifest
            .find_integration("claude-code", "project")
            .is_none());
        assert!(manifest.find_integration("codex", "global").is_none());
    }

    #[test]
    fn test_manifest_remove() {
        let mut manifest = InstallManifest::default();
        let integration = HostIntegration {
            host: "claude-code".to_string(),
            scope: "global".to_string(),
            config_path: "test".to_string(),
            backup_path: None,
            installed_at: Utc::now(),
            hooks_installed: vec![],
            profile: "minimal".to_string(),
        };
        manifest.upsert_integration(integration);

        let removed = manifest.remove_integration("claude-code", "global");
        assert!(removed.is_some());
        assert!(manifest.host_integrations.is_empty());

        // Removing again returns None
        let removed = manifest.remove_integration("claude-code", "global");
        assert!(removed.is_none());
    }

    #[test]
    fn test_manifest_serialization() {
        let mut manifest = InstallManifest::default();
        manifest.upsert_integration(HostIntegration {
            host: "claude-code".to_string(),
            scope: "global".to_string(),
            config_path: "/home/user/.claude/settings.json".to_string(),
            backup_path: Some("/home/user/.claude/settings.json.rmap-backup".to_string()),
            installed_at: Utc::now(),
            hooks_installed: vec!["SessionStart".to_string(), "Stop".to_string()],
            profile: "minimal".to_string(),
        });

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let parsed: InstallManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.host_integrations.len(), 1);
        assert_eq!(parsed.host_integrations[0].host, "claude-code");
    }
}
