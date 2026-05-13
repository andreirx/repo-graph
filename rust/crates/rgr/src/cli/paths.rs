//! Platform-aware directory resolution for repo-graph.
//!
//! Implements DIST-1 D3 (Platform-Native Directory Layout):
//! - macOS: ~/Library/Application Support/repo-graph/, ~/Library/Logs/repo-graph/
//! - Linux: ~/.config/rmap/, ~/.local/share/rmap/
//!
//! These paths are used by HOOK-1 for session state, logs, and configuration.

use std::path::PathBuf;

/// Platform-specific application name.
///
/// macOS uses "repo-graph" (consistent with Application Support conventions).
/// Linux uses "rmap" (XDG conventions, shorter name).
#[cfg(target_os = "macos")]
const APP_NAME: &str = "repo-graph";

#[cfg(not(target_os = "macos"))]
const APP_NAME: &str = "rmap";

/// Returns the platform-native configuration directory.
///
/// - macOS: `~/Library/Application Support/repo-graph/`
/// - Linux: `~/.config/rmap/`
///
/// Returns `None` if the home directory cannot be determined.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir().map(|p| p.join(APP_NAME))
    }

    #[cfg(not(target_os = "macos"))]
    {
        dirs::config_dir().map(|p| p.join(APP_NAME))
    }
}

/// Returns the platform-native data directory.
///
/// - macOS: `~/Library/Application Support/repo-graph/`
/// - Linux: `~/.local/share/rmap/`
///
/// Returns `None` if the home directory cannot be determined.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir().map(|p| p.join(APP_NAME))
    }

    #[cfg(not(target_os = "macos"))]
    {
        dirs::data_local_dir().map(|p| p.join(APP_NAME))
    }
}

/// Returns the platform-native logs directory.
///
/// - macOS: `~/Library/Logs/repo-graph/`
/// - Linux: `~/.local/share/rmap/logs/`
///
/// Returns `None` if the home directory cannot be determined.
pub fn logs_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library").join("Logs").join(APP_NAME))
    }

    #[cfg(not(target_os = "macos"))]
    {
        data_dir().map(|d| d.join("logs"))
    }
}

/// Returns the platform-native sessions directory.
///
/// - macOS: `~/Library/Application Support/repo-graph/sessions/`
/// - Linux: `~/.local/share/rmap/sessions/`
///
/// Returns `None` if the home directory cannot be determined.
pub fn sessions_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("sessions"))
}

/// Returns the platform-native databases directory.
///
/// - macOS: `~/Library/Application Support/repo-graph/databases/`
/// - Linux: `~/.local/share/rmap/databases/`
///
/// Returns `None` if the home directory cannot be determined.
pub fn databases_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("databases"))
}

/// Ensure a directory exists, creating it and parents if necessary.
///
/// Returns the path if successful, or an error message if creation fails.
pub fn ensure_dir(path: &PathBuf) -> Result<PathBuf, String> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| format!("failed to create directory '{}': {}", path.display(), e))?;
    }
    Ok(path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_returns_some() {
        // Should return Some on any platform with a home directory
        let result = config_dir();
        assert!(result.is_some());

        let path = result.unwrap();
        #[cfg(target_os = "macos")]
        assert!(path.to_string_lossy().contains("Application Support/repo-graph"));

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
}
