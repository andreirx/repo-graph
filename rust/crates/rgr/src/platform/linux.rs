//! Linux platform adapter (stub).
//!
//! To be implemented in LINUX-1 slice.
//! Will implement systemd --user service management.

use crate::cli::paths;

use super::{InstallManifest, PlatformAdapter, ProbeResult, ServiceStatus};

/// Linux platform adapter.
pub struct LinuxAdapter;

impl LinuxAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformAdapter for LinuxAdapter {
    fn service_status(&self) -> ServiceStatus {
        // TODO: Implement systemctl --user status rmapd
        ServiceStatus::Unknown {
            reason: "Linux service support not yet implemented (LINUX-1)".to_string(),
        }
    }

    fn stop_service(&self) -> Result<(), String> {
        // TODO: Implement systemctl --user stop rmapd
        Err("Linux service support not yet implemented (LINUX-1)".to_string())
    }

    fn remove_service(&self) -> Result<(), String> {
        // TODO: Implement systemctl --user disable rmapd + remove unit file
        Err("Linux service support not yet implemented (LINUX-1)".to_string())
    }

    fn read_manifest(&self) -> Result<InstallManifest, String> {
        let config_dir = paths::config_dir()
            .ok_or_else(|| "could not determine config directory".to_string())?;
        let manifest_path = config_dir.join("install-manifest.json");

        if !manifest_path.exists() {
            return Err(format!("manifest not found: {}", manifest_path.display()));
        }

        // TODO: Share manifest parsing with macos.rs
        Err("Linux manifest reading not yet fully implemented".to_string())
    }

    fn doctor_probes(&self) -> Vec<ProbeResult> {
        let mut probes = Vec::new();

        // Binary checks (same as macOS)
        let install_dir = dirs::home_dir()
            .map(|h| h.join(".local").join("bin"))
            .unwrap_or_default();

        for name in &["rmap", "rmapd", "rgistr"] {
            let path = install_dir.join(name);
            if path.exists() {
                probes.push(ProbeResult::pass(*name, path.display().to_string()));
            } else {
                probes.push(ProbeResult::fail(
                    *name,
                    format!("not found: {}", path.display()),
                ));
            }
        }

        // Directory checks
        if let Some(config_dir) = paths::config_dir() {
            if config_dir.exists() {
                probes.push(ProbeResult::pass(
                    "config_dir",
                    config_dir.display().to_string(),
                ));
            } else {
                probes.push(ProbeResult::fail(
                    "config_dir",
                    format!("not found: {}", config_dir.display()),
                ));
            }
        }

        // Service check (stub)
        probes.push(ProbeResult::fail(
            "daemon_service",
            "Linux service support not yet implemented (LINUX-1)",
        ));

        probes
    }
}
