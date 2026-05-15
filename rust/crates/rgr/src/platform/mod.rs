//! Platform-specific operations for CLI commands.
//!
//! This module provides thin adapters for platform-specific mechanisms:
//! - Service management (launchd on macOS, systemd on Linux)
//! - Manifest reading
//! - Doctor probes
//!
//! **Architecture boundary:** This module contains mechanism only, not policy.
//! Policy decisions (what to uninstall, what is healthy) live in command handlers.
//!
//! **Path contract:** Platform paths must match `cli/paths.rs` and DIST-1 D3.

pub mod manifest;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

use std::path::PathBuf;

use crate::cli::paths;
use crate::daemon_client::is_daemon_reachable;

/// Service status as reported by the platform service manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    /// Service is running with given PID.
    Running { pid: Option<u32> },
    /// Service is loaded but not running.
    Stopped,
    /// Service is not loaded/registered.
    NotInstalled,
    /// Could not determine status.
    Unknown { reason: String },
}

impl ServiceStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, ServiceStatus::Running { .. })
    }

    pub fn is_installed(&self) -> bool {
        matches!(self, ServiceStatus::Running { .. } | ServiceStatus::Stopped)
    }
}

/// Result of a doctor probe.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub details: Option<String>,
}

impl ProbeResult {
    pub fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            message: message.into(),
            details: None,
        }
    }

    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Check daemon socket connectivity.
///
/// This is a cross-platform probe used by both macOS and Linux adapters.
/// It checks:
/// 1. Socket path can be determined
/// 2. Socket file exists
/// 3. Daemon accepts connections on the socket
///
/// The socket connectivity check uses the same code path as production
/// CLI-to-daemon communication (no special health endpoint).
pub fn check_daemon_socket() -> ProbeResult {
    let socket_path = match paths::daemon_socket_path() {
        Some(p) => p,
        None => {
            return ProbeResult::fail("daemon_socket", "could not determine socket path");
        }
    };

    if !socket_path.exists() {
        return ProbeResult::fail(
            "daemon_socket",
            format!("socket not found: {}", socket_path.display()),
        );
    }

    // Check if daemon is accepting connections
    if is_daemon_reachable(&socket_path) {
        ProbeResult::pass(
            "daemon_socket",
            format!("connected: {}", socket_path.display()),
        )
    } else {
        ProbeResult::fail(
            "daemon_socket",
            format!(
                "socket exists but not responding: {}",
                socket_path.display()
            ),
        )
        .with_details("daemon process may have crashed without cleanup")
    }
}

/// Install manifest as written by the installer.
/// Matches schema in DIST-1 D6.
#[derive(Debug, Clone, Default)]
pub struct InstallManifest {
    pub schema_version: String,
    pub installed_at: Option<String>,
    pub platform: String,
    pub arch: String,
    pub install_mode: String,
    pub components: ManifestComponents,
    pub directories: ManifestDirectories,
    pub service: Option<ManifestService>,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestComponents {
    pub rmap: Option<ManifestComponent>,
    pub rmapd: Option<ManifestComponent>,
    pub rgistr: Option<ManifestComponent>,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestComponent {
    pub path: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestDirectories {
    pub config: PathBuf,
    pub data: PathBuf,
    pub logs: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestService {
    pub service_type: String,
    pub path: PathBuf,
    pub status: String,
}

/// Platform adapter trait for service operations.
///
/// Implemented by platform-specific modules (macos.rs, linux.rs).
pub trait PlatformAdapter {
    /// Get the current status of the daemon service.
    fn service_status(&self) -> ServiceStatus;

    /// Stop the daemon service.
    fn stop_service(&self) -> Result<(), String>;

    /// Remove the service registration (plist/unit file).
    fn remove_service(&self) -> Result<(), String>;

    /// Read the install manifest.
    fn read_manifest(&self) -> Result<InstallManifest, String>;

    /// Run doctor probes and return results.
    fn doctor_probes(&self) -> Vec<ProbeResult>;
}

/// Get the platform adapter for the current platform.
#[cfg(target_os = "macos")]
pub fn get_adapter() -> impl PlatformAdapter {
    macos::MacOSAdapter::new()
}

#[cfg(target_os = "linux")]
pub fn get_adapter() -> impl PlatformAdapter {
    linux::LinuxAdapter::new()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn get_adapter() -> impl PlatformAdapter {
    UnsupportedAdapter
}

/// Stub adapter for unsupported platforms.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
struct UnsupportedAdapter;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
impl PlatformAdapter for UnsupportedAdapter {
    fn service_status(&self) -> ServiceStatus {
        ServiceStatus::Unknown {
            reason: "unsupported platform".to_string(),
        }
    }

    fn stop_service(&self) -> Result<(), String> {
        Err("unsupported platform".to_string())
    }

    fn remove_service(&self) -> Result<(), String> {
        Err("unsupported platform".to_string())
    }

    fn read_manifest(&self) -> Result<InstallManifest, String> {
        Err("unsupported platform".to_string())
    }

    fn doctor_probes(&self) -> Vec<ProbeResult> {
        vec![ProbeResult::fail(
            "platform",
            "unsupported platform for doctor",
        )]
    }
}
