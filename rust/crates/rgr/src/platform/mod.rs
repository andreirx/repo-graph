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
use crate::daemon_client::{
    check_socket_connectivity, DaemonClient, SocketConnectResult, StateRootMode, TransportMode,
};

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

/// Check daemon socket connectivity with full path resolution diagnostics.
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
    let diag = paths::daemon_socket_path_with_diagnostics();

    let socket_path = match diag.chosen_path {
        Some(ref p) => p.clone(),
        None => {
            return ProbeResult::fail("daemon_socket", "could not determine socket path")
                .with_details(format!(
                    "uid={}, HOME={}, canonical_home={}, reason={}",
                    diag.effective_uid,
                    diag.env_home.as_deref().unwrap_or("(not set)"),
                    diag.canonical_home
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "(failed)".to_string()),
                    diag.resolution_reason
                ));
        }
    };

    // Use granular connectivity check
    let connect_result = check_socket_connectivity(&socket_path);

    match connect_result {
        SocketConnectResult::SocketMissing => {
            let details = format!(
                "resolution: {}, canonical={}, legacy={}",
                diag.resolution_reason,
                diag.canonical_socket_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(none)".to_string()),
                diag.legacy_socket_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(none)".to_string()),
            );
            ProbeResult::fail(
                "daemon_socket",
                format!("socket not found: {}", socket_path.display()),
            )
            .with_details(details)
        }

        SocketConnectResult::ConnectFailed { error, code } => {
            let err_detail = if let Some(c) = code {
                format!("{} (errno {})", error, c)
            } else {
                error
            };
            ProbeResult::fail(
                "daemon_socket",
                format!(
                    "socket exists but not responding: {}",
                    socket_path.display()
                ),
            )
            .with_details(format!(
                "connect failed: {}; daemon process may have crashed without cleanup",
                err_detail
            ))
        }

        SocketConnectResult::Connected => ProbeResult::pass(
            "daemon_socket",
            format!("connected: {}", socket_path.display()),
        ),
    }
}

/// Run granular socket health probes.
///
/// DAEMON-SOCKET-HEALTH-1: Returns separate probes for each failure layer:
/// - `socket_file`: exists / missing
/// - `socket_connect`: succeeded / failed (with error detail)
/// - `socket_ping`: succeeded / failed / timeout
///
/// Used by `rmap doctor --json` for agent-parseable diagnostics.
pub fn granular_socket_probes() -> Vec<ProbeResult> {
    let diag = paths::daemon_socket_path_with_diagnostics();
    let mut probes = Vec::new();

    let socket_path = match diag.chosen_path {
        Some(ref p) => p.clone(),
        None => {
            probes.push(ProbeResult::fail(
                "socket_file",
                "could not determine socket path",
            ));
            probes.push(ProbeResult::fail("socket_connect", "skipped (no path)"));
            probes.push(ProbeResult::fail("socket_ping", "skipped (no path)"));
            return probes;
        }
    };

    // Probe 1: Socket file existence
    let socket_exists = socket_path.exists();
    if socket_exists {
        probes.push(ProbeResult::pass(
            "socket_file",
            socket_path.display().to_string(),
        ));
    } else {
        probes.push(ProbeResult::fail(
            "socket_file",
            format!("not found: {}", socket_path.display()),
        ));
        // Can't test connect or ping if file doesn't exist
        probes.push(ProbeResult::fail(
            "socket_connect",
            "skipped (socket missing)",
        ));
        probes.push(ProbeResult::fail("socket_ping", "skipped (socket missing)"));
        return probes;
    }

    // Probe 2: Socket connectivity (can we connect?)
    let connect_result = check_socket_connectivity(&socket_path);
    match &connect_result {
        SocketConnectResult::SocketMissing => {
            // Shouldn't happen given we checked exists, but handle it
            probes.push(ProbeResult::fail(
                "socket_connect",
                "socket disappeared during check",
            ));
            probes.push(ProbeResult::fail("socket_ping", "skipped (connect failed)"));
            return probes;
        }
        SocketConnectResult::ConnectFailed { error, code } => {
            let msg = if let Some(c) = code {
                format!("failed: {} (errno {})", error, c)
            } else {
                format!("failed: {}", error)
            };
            probes.push(ProbeResult::fail("socket_connect", msg));
            probes.push(ProbeResult::fail("socket_ping", "skipped (connect failed)"));
            return probes;
        }
        SocketConnectResult::Connected => {
            probes.push(ProbeResult::pass("socket_connect", "succeeded"));
        }
    }

    // Probe 3: Socket ping (does daemon respond to requests?)
    // This requires a full DaemonClient connection and ping request
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            probes.push(
                ProbeResult::fail("socket_ping", "client creation failed")
                    .with_details(e.to_string()),
            );
            return probes;
        }
    };

    match client.ping() {
        Ok(()) => {
            probes.push(ProbeResult::pass("socket_ping", "pong received"));
        }
        Err(e) => {
            let msg = match &e {
                crate::daemon_client::DaemonClientError::Timeout { timeout_secs } => {
                    format!("timeout after {}s", timeout_secs)
                }
                other => format!("failed: {}", other),
            };
            probes.push(ProbeResult::fail("socket_ping", msg));
        }
    }

    // Probe 4: Transport mode used
    // STDIO-TRANSPORT-1: Report which transport was selected
    let transport_mode = client.transport_mode();
    let active_transport = client.active_transport_name().unwrap_or("none");
    let transport_msg = format!("{} (active: {})", transport_mode, active_transport);

    if transport_mode == TransportMode::Stdio
        || (transport_mode == TransportMode::Auto && active_transport == "stdio")
    {
        // Stdio transport indicates sandbox fallback occurred
        probes.push(
            ProbeResult::pass("transport", &transport_msg)
                .with_details("using stdio subprocess (socket permission denied)"),
        );
    } else {
        probes.push(ProbeResult::pass("transport", transport_msg));
    }

    // Probe 5: State root mode
    // STDIO-STATE-ROOT-1: Report which state root is in use
    let state_root_mode = client.state_root_mode();
    let state_root_msg = match state_root_mode {
        StateRootMode::SandboxLocal => {
            let path = client
                .sandbox_state_root()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "/private/tmp/repo-graph-agent/<uid>".to_string());
            format!("{} ({})", state_root_mode, path)
        }
        StateRootMode::Override => {
            let path = std::env::var("RMAP_STATE_ROOT").unwrap_or_else(|_| "(unknown)".to_string());
            format!("{} ({})", state_root_mode, path)
        }
        StateRootMode::Global => state_root_mode.to_string(),
    };

    if state_root_mode == StateRootMode::SandboxLocal {
        probes.push(
            ProbeResult::pass("state_root", &state_root_msg)
                .with_details("using isolated sandbox state (not shared with launchd daemon)"),
        );
    } else {
        probes.push(ProbeResult::pass("state_root", state_root_msg));
    }

    probes
}

/// Generate socket resolution diagnostic probes.
///
/// Returns detailed probes about path resolution for `rmap doctor --json`.
/// Human output uses the simpler `check_daemon_socket()` result.
pub fn socket_resolution_probes() -> Vec<ProbeResult> {
    let diag = paths::daemon_socket_path_with_diagnostics();
    let mut probes = Vec::new();

    // Effective UID probe
    probes.push(ProbeResult::pass(
        "effective_uid",
        diag.effective_uid.to_string(),
    ));

    // HOME env probe
    probes.push(ProbeResult::pass(
        "env_home",
        diag.env_home
            .clone()
            .unwrap_or_else(|| "(not set)".to_string()),
    ));

    // Canonical home probe
    if let Some(ref canon) = diag.canonical_home {
        probes.push(ProbeResult::pass(
            "canonical_home",
            canon.display().to_string(),
        ));
    } else {
        probes.push(ProbeResult::fail("canonical_home", "could not determine"));
    }

    // Socket path resolution probe
    if let Some(ref path) = diag.chosen_path {
        let exists = path.exists();
        let msg = format!(
            "{} ({})",
            path.display(),
            if exists { "exists" } else { "not found" }
        );
        if exists {
            probes.push(ProbeResult::pass("socket_path", msg));
        } else {
            probes.push(ProbeResult::fail("socket_path", msg));
        }
    } else {
        probes.push(ProbeResult::fail("socket_path", "could not determine"));
    }

    // Resolution reason probe
    let reason_msg = diag.resolution_reason.to_string();
    let is_fallback = diag.resolution_reason == paths::ResolutionReason::LegacyFallback;
    if is_fallback {
        probes.push(
            ProbeResult::fail("socket_resolution", reason_msg)
                .with_details("using legacy $HOME path; restart daemon to use canonical path"),
        );
    } else {
        probes.push(ProbeResult::pass("socket_resolution", reason_msg));
    }

    // Override active probe
    if diag.override_active {
        probes.push(ProbeResult::pass(
            "socket_override",
            diag.override_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "active".to_string()),
        ));
    }

    probes
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
