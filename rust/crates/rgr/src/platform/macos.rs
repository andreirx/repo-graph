//! macOS platform adapter.
//!
//! Implements launchd service management and doctor probes for macOS.
//!
//! **Path contract:** Must match `cli/paths.rs` and `scripts/lib/macos.sh`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::paths;

use super::{
    check_daemon_socket, doctor_config, manifest, service_note_applies, service_note_detail,
    DoctorConfig, InstallManifest, PlatformAdapter, ProbeResult, ServiceStatus,
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
    ///
    /// Uses canonical home because LaunchAgents are system infrastructure,
    /// not session-local preference. Must be findable regardless of $HOME.
    fn launch_agents_dir() -> Option<PathBuf> {
        paths::canonical_home().map(|h| h.join("Library").join("LaunchAgents"))
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
        // Look for "state = <status>" in the output
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
        } else if output.contains("state = waiting")
            || output.contains("state = idle")
            || output.contains("state = not running")
        {
            // Service is loaded but not currently running
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

        manifest::parse_manifest_from_path(&manifest_path)
    }

    fn doctor_probes(&self) -> Vec<ProbeResult> {
        let mut probes = Vec::new();

        // Binary checks - use canonical home to find actual installation
        let install_dir = paths::canonical_home()
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

        // Service check (launchd status of the GLOBAL root's service; DOCTOR-FALLBACK-STATE-ROOT-1)
        probes.push(service_probe(self.service_status(), &doctor_config()));

        // Socket connectivity check (actual daemon responsiveness)
        probes.push(check_daemon_socket());

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

/// The `daemon_service` probe: the launchd service of the GLOBAL state root (pure; the live
/// `launchctl` read stays in `doctor_probes`).
///
/// Under the ONE note predicate (`service_note_applies`: forced stdio, or a configured override
/// root) the probe passes whatever the service state — this run is not judged by the global
/// service — and carries the detail that states what the configuration proves. Otherwise today's
/// pass/fail per state, no detail.
fn service_probe(status: ServiceStatus, cfg: &DoctorConfig) -> ProbeResult {
    let (passed_today, state) = match &status {
        ServiceStatus::Running { pid } => {
            let msg = match pid {
                Some(p) => format!("running (pid: {})", p),
                None => "running".to_string(),
            };
            (true, msg)
        }
        ServiceStatus::Stopped => (false, "loaded but not running".to_string()),
        ServiceStatus::NotInstalled => (false, "not installed".to_string()),
        ServiceStatus::Unknown { reason } => (false, format!("unknown: {}", reason)),
    };
    let message = format!("launchd service (global state root): {}", state);
    if service_note_applies(cfg) {
        let probe = ProbeResult::pass("daemon_service", message);
        match service_note_detail(cfg) {
            Some(detail) => probe.with_details(detail),
            None => probe,
        }
    } else if passed_today {
        ProbeResult::pass("daemon_service", message)
    } else {
        ProbeResult::fail("daemon_service", message)
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

    #[test]
    fn parse_not_running_state() {
        let adapter = MacOSAdapter::new();
        let output = r#"
gui/501/com.repo-graph.rmapd = {
    active count = 0
    path = /Users/apple/Library/LaunchAgents/com.repo-graph.rmapd.plist
    type = LaunchAgent
    state = not running
    program = /Users/apple/.local/bin/rmapd
}
"#;
        let status = adapter.parse_launchctl_print(output);
        assert!(matches!(status, ServiceStatus::Stopped));
    }

    // RG-REQ-011-L12 (macOS, D-DFSR-LINUX-SCOPE-1): the service probe names what it measures; it is
    // a passing note under forced stdio (any root) or a configured override root, with a detail that
    // states only what the configuration proves; under auto/socket on the global root it keeps
    // today's pass/fail per state with the new label and no detail.
    #[test]
    fn daemon_service_probe_names_the_global_root_under_override() {
        use super::super::RootConfig;
        use crate::daemon_client::TransportMode;

        let iso = PathBuf::from("/private/tmp/iso-root/state");
        let states = [
            (
                ServiceStatus::Running { pid: Some(42) },
                "running (pid: 42)",
                true,
            ),
            (ServiceStatus::Running { pid: None }, "running", true),
            (ServiceStatus::Stopped, "loaded but not running", false),
            (ServiceStatus::NotInstalled, "not installed", false),
            (
                ServiceStatus::Unknown {
                    reason: "could not parse launchctl output".to_string(),
                },
                "unknown: could not parse launchctl output",
                false,
            ),
        ];
        let stdio_detail = "stdio transport configured; this service is not used by a stdio client";
        let override_detail = format!(
            "measures the global launchd service; this run's configured state root is {}",
            iso.display()
        );

        for (status, state, passed_today) in states {
            let label = format!("launchd service (global state root): {state}");

            // Forced stdio — global root, override root, non-UTF-8 root: a passing note.
            for root in [
                RootConfig::Global,
                RootConfig::Override(iso.clone()),
                RootConfig::InvalidUtf8,
            ] {
                let cfg = DoctorConfig {
                    transport: TransportMode::Stdio,
                    root: root.clone(),
                };
                let p = service_probe(status.clone(), &cfg);
                assert_eq!(p.name, "daemon_service");
                assert_eq!(p.message, label, "{root:?}");
                assert!(p.passed, "stdio {root:?} {state}");
                assert_eq!(p.details.as_deref(), Some(stdio_detail), "{root:?}");
            }

            for transport in [TransportMode::Auto, TransportMode::Socket] {
                // A configured non-global root under auto/socket: the neutral note.
                let cfg = DoctorConfig {
                    transport,
                    root: RootConfig::Override(iso.clone()),
                };
                let p = service_probe(status.clone(), &cfg);
                assert_eq!(p.message, label);
                assert!(p.passed, "{transport:?} override {state}");
                assert_eq!(p.details.as_deref(), Some(override_detail.as_str()));

                // The global root (and a non-UTF-8 value, which is not an override): today's
                // pass/fail per state, the new label, no detail.
                for root in [RootConfig::Global, RootConfig::InvalidUtf8] {
                    let cfg = DoctorConfig {
                        transport,
                        root: root.clone(),
                    };
                    let p = service_probe(status.clone(), &cfg);
                    assert_eq!(p.message, label);
                    assert_eq!(p.passed, passed_today, "{transport:?} {root:?} {state}");
                    assert!(p.details.is_none(), "{transport:?} {root:?}: {p:?}");
                }
            }
        }
    }
}
