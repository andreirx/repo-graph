//! Hook configuration support (hooks.toml).
//!
//! Configuration file location (per DIST-1 D3):
//! - macOS: ~/Library/Application Support/repo-graph/hooks.toml
//! - Linux: ~/.config/rmap/hooks.toml

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cli::paths;

/// Hook configuration loaded from hooks.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookConfig {
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub post_edit: PostEditConfig,
    #[serde(default)]
    pub stop: StopConfig,
}

/// Session-related configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Auto-refresh on session-start if DB older than this (minutes).
    #[serde(default = "default_stale_threshold")]
    pub stale_threshold_minutes: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            stale_threshold_minutes: default_stale_threshold(),
        }
    }
}

fn default_stale_threshold() -> u32 {
    30
}

/// Post-edit configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostEditConfig {
    /// Batch edits within this window before refresh (seconds).
    #[serde(default = "default_batch_window")]
    pub batch_window_seconds: u32,

    /// Skip refresh for these patterns.
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

impl Default for PostEditConfig {
    fn default() -> Self {
        Self {
            batch_window_seconds: default_batch_window(),
            ignore_patterns: vec![
                "*.log".to_string(),
                "*.tmp".to_string(),
                "node_modules/**".to_string(),
            ],
        }
    }
}

fn default_batch_window() -> u32 {
    5
}

/// Stop configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopConfig {
    /// Require these validations before completion.
    #[serde(default = "default_required_validations")]
    pub required_validations: Vec<String>,

    /// Enforcement mode (future).
    #[serde(default)]
    pub enforcement: bool,
}

impl Default for StopConfig {
    fn default() -> Self {
        Self {
            required_validations: default_required_validations(),
            enforcement: false,
        }
    }
}

fn default_required_validations() -> Vec<String> {
    vec!["refresh".to_string(), "trust".to_string()]
}

impl HookConfig {
    /// Load configuration from the default location.
    ///
    /// Returns default configuration if file doesn't exist.
    /// Returns error only if file exists but is malformed.
    pub fn load() -> Result<Self, String> {
        let path = config_file_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;

        toml::from_str(&content).map_err(|e| format!("failed to parse {}: {}", path.display(), e))
    }

    /// Save configuration to the default location.
    #[allow(dead_code)] // Future use: config editing
    pub fn save(&self) -> Result<PathBuf, String> {
        let path = config_file_path()?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            paths::ensure_dir(&parent.to_path_buf())?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize config: {}", e))?;

        fs::write(&path, content)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;

        Ok(path)
    }

    /// Get the configuration file path.
    #[allow(dead_code)] // Future use: config management commands
    pub fn path() -> Option<PathBuf> {
        config_file_path().ok()
    }
}

/// Get the path to hooks.toml.
fn config_file_path() -> Result<PathBuf, String> {
    let config_dir =
        paths::config_dir().ok_or_else(|| "could not determine config directory".to_string())?;

    Ok(config_dir.join("hooks.toml"))
}

/// Configuration status for display.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigStatus {
    pub path: Option<String>,
    pub exists: bool,
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<HookConfig>,
}

impl ConfigStatus {
    /// Check configuration status.
    pub fn check() -> Self {
        let path = config_file_path().ok();
        let path_str = path.as_ref().map(|p| p.display().to_string());

        let exists = path.as_ref().map(|p| p.exists()).unwrap_or(false);

        if !exists {
            return Self {
                path: path_str,
                exists: false,
                valid: true, // Non-existent is valid (uses defaults)
                error: None,
                config: Some(HookConfig::default()),
            };
        }

        match HookConfig::load() {
            Ok(config) => Self {
                path: path_str,
                exists: true,
                valid: true,
                error: None,
                config: Some(config),
            },
            Err(e) => Self {
                path: path_str,
                exists: true,
                valid: false,
                error: Some(e),
                config: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = HookConfig::default();
        assert_eq!(config.session.stale_threshold_minutes, 30);
        assert_eq!(config.post_edit.batch_window_seconds, 5);
        assert!(!config.stop.enforcement);
    }

    #[test]
    fn config_roundtrips_through_toml() {
        let config = HookConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: HookConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            config.session.stale_threshold_minutes,
            parsed.session.stale_threshold_minutes
        );
    }
}
