//! Linux platform adapter.
//!
//! Implements systemd user service management and manual PID file fallback.
//!
//! **Service modes:**
//! - `systemd`: Unit file at `~/.config/systemd/user/rmapd.service`
//! - `manual`: PID file at `~/.local/share/rmap/daemon.pid`
//!
//! The mode is determined by the install manifest. The adapter reads the manifest
//! to determine which mode to use for status/stop/remove operations.
//!
//! **Path contract:** Must match `cli/paths.rs` and `scripts/lib/linux.sh`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::paths;

use super::{manifest, InstallManifest, PlatformAdapter, ProbeResult, ServiceStatus};

/// systemd service name.
const SERVICE_NAME: &str = "rmapd.service";

/// Linux platform adapter.
pub struct LinuxAdapter;

impl LinuxAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Get the path to the systemd user unit directory.
    fn systemd_user_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config").join("systemd").join("user"))
    }

    /// Get the path to the systemd unit file.
    fn unit_path() -> Option<PathBuf> {
        Self::systemd_user_dir().map(|d| d.join(SERVICE_NAME))
    }

    /// Get the path to the manual mode PID file.
    fn pid_file_path() -> Option<PathBuf> {
        paths::data_dir().map(|d| d.join("daemon.pid"))
    }

    /// Determine service mode from manifest.
    /// Returns "systemd", "manual", or None if manifest cannot be read.
    fn service_mode_from_manifest(&self) -> Option<String> {
        match self.read_manifest() {
            Ok(manifest) => manifest.service.map(|s| s.service_type),
            Err(_) => None,
        }
    }

    /// Check systemd service status.
    fn systemd_status(&self) -> ServiceStatus {
        let output = Command::new("systemctl")
            .args(["--user", "show", SERVICE_NAME, "--property=ActiveState,MainPID"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                self.parse_systemctl_show(&stdout)
            }
            Ok(_) => {
                // Non-zero exit or service not found
                ServiceStatus::NotInstalled
            }
            Err(e) => ServiceStatus::Unknown {
                reason: format!("systemctl error: {}", e),
            },
        }
    }

    /// Parse `systemctl show` output for ActiveState and MainPID.
    fn parse_systemctl_show(&self, output: &str) -> ServiceStatus {
        let mut active_state = None;
        let mut main_pid = None;

        for line in output.lines() {
            if let Some(value) = line.strip_prefix("ActiveState=") {
                active_state = Some(value.trim());
            } else if let Some(value) = line.strip_prefix("MainPID=") {
                main_pid = value.trim().parse::<u32>().ok().filter(|&p| p > 0);
            }
        }

        match active_state {
            Some("active") | Some("activating") => ServiceStatus::Running { pid: main_pid },
            Some("inactive") | Some("deactivating") | Some("failed") => ServiceStatus::Stopped,
            None => ServiceStatus::NotInstalled,
            Some(state) => ServiceStatus::Unknown {
                reason: format!("unexpected state: {}", state),
            },
        }
    }

    /// Check manual mode daemon status via PID file.
    fn manual_status(&self) -> ServiceStatus {
        let pid_path = match Self::pid_file_path() {
            Some(p) => p,
            None => {
                return ServiceStatus::Unknown {
                    reason: "could not determine PID file path".to_string(),
                }
            }
        };

        if !pid_path.exists() {
            return ServiceStatus::NotInstalled;
        }

        // Read PID from file
        let pid_str = match std::fs::read_to_string(&pid_path) {
            Ok(s) => s,
            Err(_) => return ServiceStatus::NotInstalled,
        };

        let pid: u32 = match pid_str.trim().parse() {
            Ok(p) => p,
            Err(_) => {
                return ServiceStatus::Unknown {
                    reason: "invalid PID in file".to_string(),
                }
            }
        };

        // Check if process is running
        if Self::process_exists(pid) {
            ServiceStatus::Running { pid: Some(pid) }
        } else {
            // PID file exists but process is gone (stale)
            ServiceStatus::Stopped
        }
    }

    /// Check if a process with given PID exists.
    fn process_exists(pid: u32) -> bool {
        // Use kill -0 to check process existence without sending a signal
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Stop systemd service.
    fn stop_systemd(&self) -> Result<(), String> {
        let output = Command::new("systemctl")
            .args(["--user", "stop", SERVICE_NAME])
            .output()
            .map_err(|e| format!("failed to run systemctl: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            // Exit code 5 means service not loaded, which is fine for uninstall
            if output.status.code() == Some(5) {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("systemctl stop failed: {}", stderr.trim()))
            }
        }
    }

    /// Stop manual mode daemon by PID.
    fn stop_manual(&self) -> Result<(), String> {
        let pid_path = Self::pid_file_path()
            .ok_or_else(|| "could not determine PID file path".to_string())?;

        if !pid_path.exists() {
            return Ok(()); // No daemon to stop
        }

        let pid_str = std::fs::read_to_string(&pid_path)
            .map_err(|e| format!("failed to read PID file: {}", e))?;

        let pid: u32 = pid_str
            .trim()
            .parse()
            .map_err(|_| "invalid PID in file".to_string())?;

        if Self::process_exists(pid) {
            // Send SIGTERM
            let _ = Command::new("kill").args([&pid.to_string()]).output();

            // Wait briefly for graceful shutdown
            std::thread::sleep(std::time::Duration::from_secs(2));

            // Force kill if still running
            if Self::process_exists(pid) {
                let _ = Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
            }
        }

        // Remove PID file
        let _ = std::fs::remove_file(&pid_path);

        Ok(())
    }

    /// Remove systemd service registration.
    fn remove_systemd(&self) -> Result<(), String> {
        // Disable the service
        let _ = Command::new("systemctl")
            .args(["--user", "disable", SERVICE_NAME])
            .output();

        // Remove unit file
        if let Some(unit) = Self::unit_path() {
            if unit.exists() {
                std::fs::remove_file(&unit)
                    .map_err(|e| format!("failed to remove unit file: {}", e))?;
            }
        }

        // Reload systemd
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();

        Ok(())
    }

    /// Remove manual mode artifacts.
    fn remove_manual(&self) -> Result<(), String> {
        if let Some(pid_path) = Self::pid_file_path() {
            if pid_path.exists() {
                std::fs::remove_file(&pid_path)
                    .map_err(|e| format!("failed to remove PID file: {}", e))?;
            }
        }
        Ok(())
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

impl Default for LinuxAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformAdapter for LinuxAdapter {
    fn service_status(&self) -> ServiceStatus {
        // Determine mode from manifest, default to checking both
        match self.service_mode_from_manifest() {
            Some(mode) if mode == "systemd" => self.systemd_status(),
            Some(mode) if mode == "manual" => self.manual_status(),
            _ => {
                // No manifest or unknown mode: try systemd first, then manual
                let systemd = self.systemd_status();
                if matches!(systemd, ServiceStatus::Running { .. } | ServiceStatus::Stopped) {
                    return systemd;
                }
                self.manual_status()
            }
        }
    }

    fn stop_service(&self) -> Result<(), String> {
        // Stop based on manifest mode, or try both
        match self.service_mode_from_manifest() {
            Some(mode) if mode == "systemd" => self.stop_systemd(),
            Some(mode) if mode == "manual" => self.stop_manual(),
            _ => {
                // Try both to be safe
                let _ = self.stop_systemd();
                self.stop_manual()
            }
        }
    }

    fn remove_service(&self) -> Result<(), String> {
        // First stop the service
        self.stop_service()?;

        // Remove based on manifest mode, or try both
        match self.service_mode_from_manifest() {
            Some(mode) if mode == "systemd" => self.remove_systemd(),
            Some(mode) if mode == "manual" => self.remove_manual(),
            _ => {
                // Try both to be safe
                let _ = self.remove_systemd();
                self.remove_manual()
            }
        }
    }

    fn read_manifest(&self) -> Result<InstallManifest, String> {
        let config_dir = paths::config_dir()
            .ok_or_else(|| "could not determine config directory".to_string())?;
        let manifest_path = config_dir.join("install-manifest.json");

        manifest::parse_manifest_from_path(&manifest_path)
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
        if let Some(data_dir) = paths::data_dir() {
            probes.push(self.check_directory(&data_dir, "data_dir"));
        }
        if let Some(logs_dir) = paths::logs_dir() {
            probes.push(self.check_directory(&logs_dir, "logs_dir"));
        }

        // Service check with mode awareness
        let mode = self.service_mode_from_manifest();
        let mode_label = mode.as_deref().unwrap_or("unknown");
        let status = self.service_status();

        let service_probe = match &status {
            ServiceStatus::Running { pid } => {
                let msg = match pid {
                    Some(p) => format!("running (pid: {}, mode: {})", p, mode_label),
                    None => format!("running (mode: {})", mode_label),
                };
                ProbeResult::pass("daemon_service", msg)
            }
            ServiceStatus::Stopped => {
                ProbeResult::fail("daemon_service", format!("stopped (mode: {})", mode_label))
            }
            ServiceStatus::NotInstalled => {
                ProbeResult::fail("daemon_service", "not installed")
            }
            ServiceStatus::Unknown { reason } => {
                ProbeResult::fail("daemon_service", format!("unknown: {}", reason))
            }
        };
        probes.push(service_probe);

        // Service artifact check (unit file or PID file)
        match mode.as_deref() {
            Some("systemd") => {
                if let Some(unit) = Self::unit_path() {
                    if unit.exists() {
                        probes.push(ProbeResult::pass("unit_file", unit.display().to_string()));
                    } else {
                        probes.push(ProbeResult::fail(
                            "unit_file",
                            format!("not found: {}", unit.display()),
                        ));
                    }
                }
            }
            Some("manual") => {
                if let Some(pid_file) = Self::pid_file_path() {
                    if pid_file.exists() {
                        probes.push(ProbeResult::pass("pid_file", pid_file.display().to_string()));
                    } else {
                        // PID file not existing is fine if daemon is not running
                        probes.push(ProbeResult::pass("pid_file", "not present (daemon not running)"));
                    }
                }
            }
            _ => {
                // Unknown mode, check for both
                if let Some(unit) = Self::unit_path() {
                    if unit.exists() {
                        probes.push(ProbeResult::pass("unit_file", unit.display().to_string()));
                    }
                }
            }
        }

        probes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_systemctl_running() {
        let adapter = LinuxAdapter::new();
        let output = "ActiveState=active\nMainPID=12345\n";
        let status = adapter.parse_systemctl_show(output);
        assert!(matches!(
            status,
            ServiceStatus::Running { pid: Some(12345) }
        ));
    }

    #[test]
    fn parse_systemctl_inactive() {
        let adapter = LinuxAdapter::new();
        let output = "ActiveState=inactive\nMainPID=0\n";
        let status = adapter.parse_systemctl_show(output);
        assert!(matches!(status, ServiceStatus::Stopped));
    }

    #[test]
    fn parse_systemctl_failed() {
        let adapter = LinuxAdapter::new();
        let output = "ActiveState=failed\nMainPID=0\n";
        let status = adapter.parse_systemctl_show(output);
        assert!(matches!(status, ServiceStatus::Stopped));
    }

    #[test]
    fn parse_systemctl_no_state() {
        let adapter = LinuxAdapter::new();
        let output = "MainPID=0\n";
        let status = adapter.parse_systemctl_show(output);
        assert!(matches!(status, ServiceStatus::NotInstalled));
    }

    #[test]
    fn parse_systemctl_activating() {
        let adapter = LinuxAdapter::new();
        let output = "ActiveState=activating\nMainPID=12345\n";
        let status = adapter.parse_systemctl_show(output);
        assert!(matches!(
            status,
            ServiceStatus::Running { pid: Some(12345) }
        ));
    }
}
