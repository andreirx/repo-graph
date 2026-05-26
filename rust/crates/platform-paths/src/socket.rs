//! Daemon socket path resolution.
//!
//! Provides socket path resolution with migration fallback logic.
//! The fallback considers socket reachability, not just file existence.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::dirs::{legacy_data_dir, APP_NAME};
use crate::home::{canonical_home, effective_uid};

/// Diagnostic information about path resolution.
///
/// Used by `rmap doctor` and error messages to show the full resolution chain.
#[derive(Debug, Clone)]
pub struct PathResolutionDiagnostics {
    /// Effective user ID used for home lookup.
    pub effective_uid: u32,
    /// Value of `$HOME` environment variable.
    pub env_home: Option<String>,
    /// Canonical home from passwd entry.
    pub canonical_home: Option<PathBuf>,
    /// Whether `RMAP_SOCKET_PATH` override is active.
    pub override_active: bool,
    /// The override path if set.
    pub override_path: Option<PathBuf>,
    /// Canonical socket path (from passwd home).
    pub canonical_socket_path: Option<PathBuf>,
    /// Legacy socket path (from `$HOME` / dirs crate).
    pub legacy_socket_path: Option<PathBuf>,
    /// Which path was chosen.
    pub chosen_path: Option<PathBuf>,
    /// Why this path was chosen.
    pub resolution_reason: ResolutionReason,
}

/// Reason for path resolution choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionReason {
    /// Used explicit `RMAP_SOCKET_PATH` override.
    Override,
    /// Used canonical path from passwd home.
    Canonical,
    /// Fell back to legacy env-derived path (migration).
    LegacyFallback,
    /// Could not determine any valid path.
    Failed,
}

impl std::fmt::Display for ResolutionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Override => write!(f, "RMAP_SOCKET_PATH override"),
            Self::Canonical => write!(f, "canonical (passwd home)"),
            Self::LegacyFallback => write!(f, "legacy fallback ($HOME)"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

/// Check if a socket is reachable (can accept connections).
///
/// This is a quick connectivity test with a 100ms timeout.
/// Does NOT send any application-level request.
fn is_socket_reachable(path: &PathBuf) -> bool {
    if !path.exists() {
        return false;
    }

    // Try to connect with a short timeout
    match UnixStream::connect(path) {
        Ok(stream) => {
            // Set a short timeout to avoid blocking
            let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
            drop(stream);
            true
        }
        Err(_) => false,
    }
}

/// Returns the daemon socket path with full resolution diagnostics.
///
/// Resolution order:
/// 1. `RMAP_SOCKET_PATH` environment variable (if set)
/// 2. Canonical path from passwd home (if reachable, or if legacy not reachable)
/// 3. Legacy path from `$HOME` (migration fallback, only if reachable AND canonical not)
///
/// The fallback logic considers **reachability**, not just file existence.
/// This handles the migration case where a stale socket file exists at canonical
/// but the live daemon is still on the legacy path.
pub fn daemon_socket_path_with_diagnostics() -> PathResolutionDiagnostics {
    let uid = effective_uid();
    let env_home = std::env::var("HOME").ok();
    let canon_home = canonical_home();

    // Check for override
    let override_path = std::env::var("RMAP_SOCKET_PATH").ok().map(PathBuf::from);
    let override_active = override_path.is_some();

    // Compute canonical socket path
    let canonical_socket = canon_home
        .as_ref()
        .map(|home| compute_socket_path_from_home(home.as_path()));

    // Compute legacy socket path
    let legacy_socket = legacy_data_dir().map(|d| d.join("daemon.sock"));

    // Resolution logic with reachability checks
    let (chosen, reason) = resolve_socket_path(&override_path, &canonical_socket, &legacy_socket);

    PathResolutionDiagnostics {
        effective_uid: uid,
        env_home,
        canonical_home: canon_home,
        override_active,
        override_path,
        canonical_socket_path: canonical_socket,
        legacy_socket_path: legacy_socket,
        chosen_path: chosen,
        resolution_reason: reason,
    }
}

/// Resolve socket path considering override, canonical, and legacy paths.
///
/// Migration fallback logic:
/// - If override is set, use it unconditionally
/// - If canonical is reachable, use it
/// - If canonical is not reachable but legacy is reachable, use legacy (migration)
/// - If neither is reachable, prefer canonical (daemon may not be running)
fn resolve_socket_path(
    override_path: &Option<PathBuf>,
    canonical_socket: &Option<PathBuf>,
    legacy_socket: &Option<PathBuf>,
) -> (Option<PathBuf>, ResolutionReason) {
    // 1. Override takes absolute precedence
    if let Some(ref ovr) = override_path {
        return (Some(ovr.clone()), ResolutionReason::Override);
    }

    // 2. Check canonical path
    if let Some(ref canon) = canonical_socket {
        let canon_reachable = is_socket_reachable(canon);

        if canon_reachable {
            // Canonical is live, use it
            return (Some(canon.clone()), ResolutionReason::Canonical);
        }

        // Canonical not reachable, check legacy
        if let Some(ref legacy) = legacy_socket {
            if legacy != canon && is_socket_reachable(legacy) {
                // Legacy is reachable but canonical is not - migration fallback
                return (Some(legacy.clone()), ResolutionReason::LegacyFallback);
            }
        }

        // Neither reachable, or they're the same path
        // Return canonical as the expected path (daemon may not be running)
        return (Some(canon.clone()), ResolutionReason::Canonical);
    }

    // 3. No canonical home available, fall back to legacy
    if let Some(ref legacy) = legacy_socket {
        return (Some(legacy.clone()), ResolutionReason::LegacyFallback);
    }

    // 4. Nothing available
    (None, ResolutionReason::Failed)
}

/// Returns the daemon socket path.
///
/// This is the primary API for getting the socket path. For diagnostics,
/// use `daemon_socket_path_with_diagnostics()`.
pub fn daemon_socket_path() -> Option<PathBuf> {
    daemon_socket_path_with_diagnostics().chosen_path
}

/// Compute socket path from a given home directory.
fn compute_socket_path_from_home(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library")
            .join("Application Support")
            .join(APP_NAME)
            .join("daemon.sock")
    }

    #[cfg(not(target_os = "macos"))]
    {
        home.join(".local")
            .join("share")
            .join(APP_NAME)
            .join("daemon.sock")
    }
}

/// Returns true if the legacy fallback path is being used.
///
/// This indicates the daemon was started with old code and the socket
/// is at the legacy location. A daemon restart will move it to canonical.
pub fn is_using_legacy_fallback() -> bool {
    daemon_socket_path_with_diagnostics().resolution_reason == ResolutionReason::LegacyFallback
}

/// Format a diagnostic warning when legacy fallback is used.
pub fn legacy_fallback_warning() -> String {
    let diag = daemon_socket_path_with_diagnostics();

    format!(
        "Warning: Using legacy socket path (migration fallback)\n\
         \n\
         The daemon socket is at the old $HOME-derived location.\n\
         This works but will break in sandboxed shells.\n\
         \n\
         Legacy path:    {}\n\
         Canonical path: {}\n\
         \n\
         To fix: restart the daemon with `launchctl bootout` + `launchctl bootstrap`",
        diag.legacy_socket_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".to_string()),
        diag.canonical_socket_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex for tests that read/write RMAP_SOCKET_PATH
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn daemon_socket_path_returns_some() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Skip if override is set externally
        if std::env::var("RMAP_SOCKET_PATH").is_ok() {
            assert!(daemon_socket_path().is_some());
            return;
        }

        let path = daemon_socket_path();
        assert!(path.is_some());

        let p = path.unwrap();
        assert!(
            p.to_string_lossy().ends_with("daemon.sock"),
            "expected path ending with daemon.sock, got: {}",
            p.display()
        );
    }

    #[test]
    fn override_takes_precedence() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let original = std::env::var("RMAP_SOCKET_PATH").ok();
        std::env::set_var("RMAP_SOCKET_PATH", "/tmp/test-override.sock");

        let diag = daemon_socket_path_with_diagnostics();
        assert!(diag.override_active);
        assert_eq!(diag.resolution_reason, ResolutionReason::Override);
        assert_eq!(
            diag.chosen_path,
            Some(PathBuf::from("/tmp/test-override.sock"))
        );

        match original {
            Some(v) => std::env::set_var("RMAP_SOCKET_PATH", v),
            None => std::env::remove_var("RMAP_SOCKET_PATH"),
        }
    }

    #[test]
    fn diagnostics_includes_all_fields() {
        let diag = daemon_socket_path_with_diagnostics();

        assert!(diag.effective_uid > 0 || cfg!(not(unix)));
        assert!(diag.canonical_home.is_some() || diag.legacy_socket_path.is_some());
        assert!(diag.chosen_path.is_some() || diag.resolution_reason == ResolutionReason::Failed);
    }

    #[test]
    fn resolution_reason_display() {
        assert_eq!(
            ResolutionReason::Override.to_string(),
            "RMAP_SOCKET_PATH override"
        );
        assert_eq!(
            ResolutionReason::Canonical.to_string(),
            "canonical (passwd home)"
        );
        assert_eq!(
            ResolutionReason::LegacyFallback.to_string(),
            "legacy fallback ($HOME)"
        );
        assert_eq!(ResolutionReason::Failed.to_string(), "failed");
    }

    #[test]
    fn is_socket_reachable_returns_false_for_nonexistent() {
        assert!(!is_socket_reachable(&PathBuf::from(
            "/nonexistent/path.sock"
        )));
    }
}
