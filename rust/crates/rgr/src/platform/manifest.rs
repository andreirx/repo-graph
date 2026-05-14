//! Shared manifest parsing for install-manifest.json.
//!
//! This module provides platform-agnostic parsing of the install manifest.
//! Platform adapters (macos.rs, linux.rs) use these functions to read manifests.
//!
//! **Contract:** Parsing is purely structural. No platform-specific semantics
//! (e.g., interpreting service types) belong here.

use std::path::{Path, PathBuf};

use super::{
    InstallManifest, ManifestComponent, ManifestComponents, ManifestDirectories, ManifestService,
};

/// Parse install manifest from a file path.
///
/// Returns error if file does not exist, cannot be read, or contains invalid JSON.
pub fn parse_manifest_from_path(path: &Path) -> Result<InstallManifest, String> {
    if !path.exists() {
        return Err(format!("manifest not found: {}", path.display()));
    }

    let content =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read manifest: {}", e))?;

    parse_manifest_from_str(&content)
}

/// Parse install manifest from a JSON string.
///
/// Useful for testing and for cases where content is already in memory.
pub fn parse_manifest_from_str(content: &str) -> Result<InstallManifest, String> {
    let json: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("failed to parse manifest: {}", e))?;

    let manifest = InstallManifest {
        schema_version: json["schema_version"]
            .as_str()
            .unwrap_or("1")
            .to_string(),
        installed_at: json["installed_at"].as_str().map(|s| s.to_string()),
        platform: json["platform"].as_str().unwrap_or("").to_string(),
        arch: json["arch"].as_str().unwrap_or("").to_string(),
        install_mode: json["install_mode"].as_str().unwrap_or("user").to_string(),
        components: parse_components(&json["components"]),
        directories: parse_directories(&json["directories"]),
        service: parse_service(&json["service"]),
    };

    Ok(manifest)
}

/// Parse components section.
fn parse_components(json: &serde_json::Value) -> ManifestComponents {
    ManifestComponents {
        rmap: parse_component(&json["rmap"]),
        rmapd: parse_component(&json["rmapd"]),
        rgistr: parse_component(&json["rgistr"]),
    }
}

/// Parse a single component entry.
fn parse_component(json: &serde_json::Value) -> Option<ManifestComponent> {
    json["path"].as_str().map(|p| ManifestComponent {
        path: PathBuf::from(p),
        version: json["version"].as_str().unwrap_or("").to_string(),
    })
}

/// Parse directories section.
fn parse_directories(json: &serde_json::Value) -> ManifestDirectories {
    ManifestDirectories {
        config: PathBuf::from(json["config"].as_str().unwrap_or("")),
        data: PathBuf::from(json["data"].as_str().unwrap_or("")),
        logs: PathBuf::from(json["logs"].as_str().unwrap_or("")),
    }
}

/// Parse service section.
fn parse_service(json: &serde_json::Value) -> Option<ManifestService> {
    json["type"].as_str().map(|t| ManifestService {
        service_type: t.to_string(),
        path: PathBuf::from(json["path"].as_str().unwrap_or("")),
        status: json["status"].as_str().unwrap_or("").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let json = r#"{
            "schema_version": "1",
            "platform": "darwin",
            "arch": "aarch64"
        }"#;

        let manifest = parse_manifest_from_str(json).unwrap();
        assert_eq!(manifest.schema_version, "1");
        assert_eq!(manifest.platform, "darwin");
        assert_eq!(manifest.arch, "aarch64");
        assert_eq!(manifest.install_mode, "user"); // default
        assert!(manifest.service.is_none());
    }

    #[test]
    fn parse_full_manifest() {
        let json = r#"{
            "schema_version": "1",
            "installed_at": "2026-05-14T12:00:00Z",
            "platform": "linux",
            "arch": "x86_64",
            "install_mode": "user",
            "components": {
                "rmap": {"path": "/home/user/.local/bin/rmap", "version": "0.1.0"},
                "rmapd": {"path": "/home/user/.local/bin/rmapd", "version": "0.1.0"},
                "rgistr": {"path": "/home/user/.local/bin/rgistr", "version": "0.1.0"}
            },
            "directories": {
                "config": "/home/user/.config/rmap",
                "data": "/home/user/.local/share/rmap",
                "logs": "/home/user/.local/share/rmap/logs"
            },
            "service": {
                "type": "systemd",
                "path": "/home/user/.config/systemd/user/rmapd.service",
                "status": "installed"
            }
        }"#;

        let manifest = parse_manifest_from_str(json).unwrap();
        assert_eq!(manifest.platform, "linux");
        assert_eq!(manifest.installed_at, Some("2026-05-14T12:00:00Z".to_string()));

        // Components
        let rmap = manifest.components.rmap.as_ref().unwrap();
        assert_eq!(rmap.path.to_string_lossy(), "/home/user/.local/bin/rmap");
        assert_eq!(rmap.version, "0.1.0");

        // Directories
        assert_eq!(
            manifest.directories.config.to_string_lossy(),
            "/home/user/.config/rmap"
        );

        // Service
        let service = manifest.service.as_ref().unwrap();
        assert_eq!(service.service_type, "systemd");
        assert_eq!(
            service.path.to_string_lossy(),
            "/home/user/.config/systemd/user/rmapd.service"
        );
    }

    #[test]
    fn parse_manual_service_mode() {
        let json = r#"{
            "schema_version": "1",
            "platform": "linux",
            "arch": "x86_64",
            "service": {
                "type": "manual",
                "path": "/home/user/.local/share/rmap/daemon.pid",
                "status": "installed"
            }
        }"#;

        let manifest = parse_manifest_from_str(json).unwrap();
        let service = manifest.service.as_ref().unwrap();
        assert_eq!(service.service_type, "manual");
    }

    #[test]
    fn parse_launchd_service() {
        let json = r#"{
            "schema_version": "1",
            "platform": "darwin",
            "arch": "aarch64",
            "service": {
                "type": "launchd",
                "path": "/Users/user/Library/LaunchAgents/com.repo-graph.rmapd.plist",
                "status": "installed"
            }
        }"#;

        let manifest = parse_manifest_from_str(json).unwrap();
        let service = manifest.service.as_ref().unwrap();
        assert_eq!(service.service_type, "launchd");
    }

    #[test]
    fn parse_invalid_json_fails() {
        let result = parse_manifest_from_str("{not valid json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to parse"));
    }

    #[test]
    fn parse_missing_fields_uses_defaults() {
        let json = "{}";
        let manifest = parse_manifest_from_str(json).unwrap();

        assert_eq!(manifest.schema_version, "1"); // default
        assert_eq!(manifest.install_mode, "user"); // default
        assert_eq!(manifest.platform, ""); // empty string for missing
        assert!(manifest.components.rmap.is_none());
        assert!(manifest.service.is_none());
    }
}
