//! Platform-aware directory resolution for repo-graph.
//!
//! This module re-exports path resolution from the `platform-paths` crate,
//! which is the single source of truth for path resolution across both
//! the CLI (`rgr`) and daemon (`rmapd`).
//!
//! See `platform-paths` crate documentation for design principles.

// Re-export all path resolution from platform-paths crate
pub use repo_graph_platform_paths::{
    canonical_home, config_dir, daemon_socket_path, daemon_socket_path_with_diagnostics, data_dir,
    databases_dir, effective_uid, ensure_dir, is_using_legacy_fallback, legacy_fallback_warning,
    legacy_home, logs_dir, sessions_dir, PathResolutionDiagnostics, ResolutionReason,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_returns_some() {
        let result = config_dir();
        assert!(result.is_some());

        let path = result.unwrap();
        #[cfg(target_os = "macos")]
        assert!(path
            .to_string_lossy()
            .contains("Application Support/repo-graph"));

        #[cfg(not(target_os = "macos"))]
        assert!(path.to_string_lossy().contains(".config/rmap"));
    }

    #[test]
    fn logs_dir_returns_some() {
        let result = logs_dir();
        assert!(result.is_some());

        let path = result.unwrap();
        #[cfg(target_os = "macos")]
        assert!(path.to_string_lossy().contains("Library/Logs/repo-graph"));

        #[cfg(not(target_os = "macos"))]
        assert!(path.to_string_lossy().contains("rmap/logs"));
    }

    #[test]
    fn sessions_dir_returns_some() {
        let result = sessions_dir();
        assert!(result.is_some());

        let path = result.unwrap();
        assert!(path.to_string_lossy().contains("sessions"));
    }

    #[test]
    fn daemon_socket_path_returns_some() {
        let result = daemon_socket_path();
        assert!(result.is_some());

        let path = result.unwrap();
        assert!(path.to_string_lossy().ends_with("daemon.sock"));

        #[cfg(target_os = "macos")]
        assert!(path
            .to_string_lossy()
            .contains("Application Support/repo-graph"));

        #[cfg(not(target_os = "macos"))]
        assert!(path.to_string_lossy().contains(".local/share/rmap"));
    }
}
