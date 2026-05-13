//! macOS platform adapter.
//!
//! Implements launchd service management and doctor probes for macOS.
//!
//! **Path contract:** Must match `cli/paths.rs` and `scripts/lib/macos.sh`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::paths;

use super::{
    InstallManifest, ManifestComponent, ManifestComponents, ManifestDirectories, ManifestService,
    PlatformAdapter, ProbeResult, ServiceStatus,
};

/// Service label for launchd.
const SERVICE_LABEL: &str = "com.repo-graph.rmapd";

/// Plist filename.
const PLIST_NAME: &str = "com.repo-graph.rmapd.plist";

/// macOS platform adapter.
pub struct MacOSAdapter {
    /// User ID for gui domain.
    uid: u32,
}

impl MacOSAdapter {
    pub fn new() -> Self {
        Self {
            uid: unsafe { libc::getuid() },
        }
    }

    /// Get the path to the LaunchAgents directory.
    fn launch_agents_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join("Library").join("LaunchAgents"))
    }

    /// Get the path to the plist file.
    fn plist_path() -> Option<PathBuf> {
        Self::launch_agents_dir().map(|d| d.join(PLIST_NAME))
    }

    /// Get the gui domain target for launchctl.
    #[allow(dead_code)] // May be used for future launchctl operations
    fn gui_target(&self) -> String {
        format!("gui/{}", self.uid)
    }

    /// Get the service target for launchctl.
    fn service_target(&self) -> String {
        format!("gui/{}/{}", self.uid, SERVICE_LABEL)
    }

    /// Parse launchctl print output for service status.
    fn parse_launchctl_print(&self, output: &str) -> ServiceStatus {
        // Look for "state = running" or "state = waiting"
        if output.contains("state = running") {
            // Try to extract PID
            let pid = output
                .lines()
                .find(|line| line.contains("pid = "))
                .and_then(|line| {
                    line.split('=')
                        .nth(1)
                        .and_then(|s| s.trim().parse::<u32>().ok())
                });
            ServiceStatus::Running { pid }
        } else if output.contains("state = waiting") || output.contains("state = idle") {
            ServiceStatus::Stopped
        } else {
            ServiceStatus::Unknown {
                reason: "could not parse launchctl output".to_string(),
            }
        }
    }

    /// Check if a binary exists and is executable.
    fn check_binary(&self, path: &PathBuf) -> ProbeResult {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "binary".to_string());

        if !path.exists() {
            return ProbeResult::fail(&name, format!("not found: {}", path.display()));
        }

        // Try to get version
        match Command::new(path).arg("--version").output() {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("unknown")
                    .to_string();
                ProbeResult::pass(&name, version)
            }
            Ok(_) => ProbeResult::fail(&name, "failed to get version"),
            Err(e) => ProbeResult::fail(&name, format!("execution error: {}", e)),
        }
    }

    /// Check if a directory exists.
    fn check_directory(&self, path: &Path, name: &str) -> ProbeResult {
        if path.exists() && path.is_dir() {
            ProbeResult::pass(name, path.display().to_string())
        } else {
            ProbeResult::fail(name, format!("not found: {}", path.display()))
        }
    }
}

impl Default for MacOSAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformAdapter for MacOSAdapter {
    fn service_status(&self) -> ServiceStatus {
        let output = Command::new("launchctl")
            .args(["print", &self.service_target()])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                self.parse_launchctl_print(&stdout)
            }
            Ok(_) => {
                // Non-zero exit usually means service not found
                ServiceStatus::NotInstalled
            }
            Err(e) => ServiceStatus::Unknown {
                reason: format!("launchctl error: {}", e),
            },
        }
    }

    fn stop_service(&self) -> Result<(), String> {
        let output = Command::new("launchctl")
            .args(["bootout", &self.service_target()])
            .output()
            .map_err(|e| format!("failed to run launchctl: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            // Exit code 3 means service not found, which is fine for uninstall
            if output.status.code() == Some(3) {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("launchctl bootout failed: {}", stderr.trim()))
            }
        }
    }

    fn remove_service(&self) -> Result<(), String> {
        // First stop the service
        self.stop_service()?;

        // Then remove the plist
        if let Some(plist) = Self::plist_path() {
            if plist.exists() {
                std::fs::remove_file(&plist)
                    .map_err(|e| format!("failed to remove plist: {}", e))?;
            }
        }

        Ok(())
    }

    fn read_manifest(&self) -> Result<InstallManifest, String> {
        let config_dir = paths::config_dir()
            .ok_or_else(|| "could not determine config directory".to_string())?;
        let manifest_path = config_dir.join("install-manifest.json");

        if !manifest_path.exists() {
            return Err(format!("manifest not found: {}", manifest_path.display()));
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("failed to read manifest: {}", e))?;

        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse manifest: {}", e))?;

        // Parse manifest fields
        let manifest = InstallManifest {
            schema_version: json["schema_version"].as_str().unwrap_or("1").to_string(),
            installed_at: json["installed_at"].as_str().map(|s| s.to_string()),
            platform: json["platform"].as_str().unwrap_or("").to_string(),
            arch: json["arch"].as_str().unwrap_or("").to_string(),
            install_mode: json["install_mode"].as_str().unwrap_or("user").to_string(),
            components: ManifestComponents {
                rmap: json["components"]["rmap"]["path"]
                    .as_str()
                    .map(|p| ManifestComponent {
                        path: PathBuf::from(p),
                        version: json["components"]["rmap"]["version"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                    }),
                rmapd: json["components"]["rmapd"]["path"]
                    .as_str()
                    .map(|p| ManifestComponent {
                        path: PathBuf::from(p),
                        version: json["components"]["rmapd"]["version"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                    }),
                rgistr: json["components"]["rgistr"]["path"]
                    .as_str()
                    .map(|p| ManifestComponent {
                        path: PathBuf::from(p),
                        version: json["components"]["rgistr"]["version"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                    }),
            },
            directories: ManifestDirectories {
                config: PathBuf::from(json["directories"]["config"].as_str().unwrap_or("")),
                data: PathBuf::from(json["directories"]["data"].as_str().unwrap_or("")),
                logs: PathBuf::from(json["directories"]["logs"].as_str().unwrap_or("")),
            },
            service: json["service"]["type"].as_str().map(|t| ManifestService {
                service_type: t.to_string(),
                path: PathBuf::from(json["service"]["path"].as_str().unwrap_or("")),
                status: json["service"]["status"].as_str().unwrap_or("").to_string(),
            }),
        };

        Ok(manifest)
    }

    fn doctor_probes(&self) -> Vec<ProbeResult> {
        let mut probes = Vec::new();

        // Binary checks
        let install_dir = dirs::home_dir()
            .map(|h| h.join(".local").join("bin"))
            .unwrap_or_default();

        probes.push(self.check_binary(&install_dir.join("rmap")));
        probes.push(self.check_binary(&install_dir.join("rmapd")));
        probes.push(self.check_binary(&install_dir.join("rgistr")));

        // Directory checks
        if let Some(config_dir) = paths::config_dir() {
            probes.push(self.check_directory(&config_dir, "config_dir"));
        }
        if let Some(logs_dir) = paths::logs_dir() {
            probes.push(self.check_directory(&logs_dir, "logs_dir"));
        }

        // Service check
        let status = self.service_status();
        let service_probe = match &status {
            ServiceStatus::Running { pid } => {
                let msg = match pid {
                    Some(p) => format!("running (pid: {})", p),
                    None => "running".to_string(),
                };
                ProbeResult::pass("daemon_service", msg)
            }
            ServiceStatus::Stopped => ProbeResult::fail("daemon_service", "loaded but not running"),
            ServiceStatus::NotInstalled => ProbeResult::fail("daemon_service", "not installed"),
            ServiceStatus::Unknown { reason } => {
                ProbeResult::fail("daemon_service", format!("unknown: {}", reason))
            }
        };
        probes.push(service_probe);

        // Plist check
        if let Some(plist) = Self::plist_path() {
            if plist.exists() {
                probes.push(ProbeResult::pass("plist", plist.display().to_string()));
            } else {
                probes.push(ProbeResult::fail(
                    "plist",
                    format!("not found: {}", plist.display()),
                ));
            }
        }

        probes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_running_state() {
        let adapter = MacOSAdapter::new();
        let output = r#"
com.repo-graph.rmapd = {
    active count = 1
    pid = 12345
    state = running
}
"#;
        let status = adapter.parse_launchctl_print(output);
        assert!(matches!(
            status,
            ServiceStatus::Running { pid: Some(12345) }
        ));
    }

    #[test]
    fn parse_waiting_state() {
        let adapter = MacOSAdapter::new();
        let output = r#"
com.repo-graph.rmapd = {
    active count = 0
    state = waiting
}
"#;
        let status = adapter.parse_launchctl_print(output);
        assert!(matches!(status, ServiceStatus::Stopped));
    }
}
