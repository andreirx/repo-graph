//! Directory path resolution.
//!
//! Provides canonical paths for data, config, logs, sessions, and databases
//! directories. All paths are derived from canonical home (passwd entry),
//! not `$HOME`.

use std::path::PathBuf;

use crate::home::canonical_home;

/// Platform-specific application name.
#[cfg(target_os = "macos")]
pub const APP_NAME: &str = "repo-graph";

#[cfg(not(target_os = "macos"))]
pub const APP_NAME: &str = "rmap";

/// Returns the canonical data directory.
///
/// - macOS: `<canonical_home>/Library/Application Support/repo-graph/`
/// - Linux: `<canonical_home>/.local/share/rmap/`
pub fn data_dir() -> Option<PathBuf> {
    canonical_home().map(|home| {
        #[cfg(target_os = "macos")]
        {
            home.join("Library")
                .join("Application Support")
                .join(APP_NAME)
        }

        #[cfg(not(target_os = "macos"))]
        {
            home.join(".local").join("share").join(APP_NAME)
        }
    })
}

/// Returns the legacy data directory from `$HOME` / dirs crate.
///
/// Used for migration fallback probing.
pub fn legacy_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir().map(|p| p.join(APP_NAME))
    }

    #[cfg(not(target_os = "macos"))]
    {
        dirs::data_local_dir().map(|p| p.join(APP_NAME))
    }
}

/// Returns the canonical config directory.
///
/// - macOS: `<canonical_home>/Library/Application Support/repo-graph/`
/// - Linux: `<canonical_home>/.config/rmap/`
pub fn config_dir() -> Option<PathBuf> {
    canonical_home().map(|home| {
        #[cfg(target_os = "macos")]
        {
            home.join("Library")
                .join("Application Support")
                .join(APP_NAME)
        }

        #[cfg(not(target_os = "macos"))]
        {
            home.join(".config").join(APP_NAME)
        }
    })
}

/// Returns the canonical logs directory.
///
/// - macOS: `<canonical_home>/Library/Logs/repo-graph/`
/// - Linux: `<data_dir>/logs/`
pub fn logs_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        canonical_home().map(|home| home.join("Library").join("Logs").join(APP_NAME))
    }

    #[cfg(not(target_os = "macos"))]
    {
        data_dir().map(|d| d.join("logs"))
    }
}

/// Returns the canonical sessions directory.
///
/// - macOS: `<data_dir>/sessions/`
/// - Linux: `<data_dir>/sessions/`
pub fn sessions_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("sessions"))
}

/// Returns the canonical databases directory.
///
/// - macOS: `<data_dir>/databases/`
/// - Linux: `<data_dir>/databases/`
pub fn databases_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("databases"))
}

/// Ensure a directory exists, creating it and parents if necessary.
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
    fn data_dir_returns_some() {
        let dir = data_dir();
        assert!(dir.is_some());

        let path = dir.unwrap();
        #[cfg(target_os = "macos")]
        assert!(path.to_string_lossy().contains("Application Support"));

        #[cfg(not(target_os = "macos"))]
        assert!(path.to_string_lossy().contains(".local/share"));
    }

    #[test]
    fn config_dir_returns_some() {
        let dir = config_dir();
        assert!(dir.is_some());
    }

    #[test]
    fn logs_dir_returns_some() {
        let dir = logs_dir();
        assert!(dir.is_some());

        let path = dir.unwrap();
        #[cfg(target_os = "macos")]
        assert!(path.to_string_lossy().contains("Library/Logs"));

        #[cfg(not(target_os = "macos"))]
        assert!(path.to_string_lossy().contains("logs"));
    }

    #[test]
    fn sessions_dir_returns_some() {
        let dir = sessions_dir();
        assert!(dir.is_some());
        assert!(dir.unwrap().to_string_lossy().contains("sessions"));
    }

    #[test]
    fn databases_dir_returns_some() {
        let dir = databases_dir();
        assert!(dir.is_some());
        assert!(dir.unwrap().to_string_lossy().contains("databases"));
    }
}
