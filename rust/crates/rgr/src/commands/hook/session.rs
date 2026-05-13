//! Session state management for hook commands.
//!
//! Session state persists across hook invocations within a single agent session.
//! This allows hooks to:
//! - Track files edited during the session
//! - Record validation events
//! - Maintain baseline snapshot reference
//!
//! State files are stored in the platform-native sessions directory.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cli::paths;

/// Session state persisted across hook invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Unique session identifier.
    pub session_id: String,
    /// Session start timestamp.
    pub started_at: DateTime<Utc>,
    /// Path to the repository database.
    pub db_path: Option<PathBuf>,
    /// Path to the repository root.
    pub repo_path: Option<PathBuf>,
    /// Baseline snapshot UID (if captured at session start).
    pub baseline_snapshot: Option<String>,
    /// Files edited during this session.
    #[serde(default)]
    pub files_edited: Vec<PathBuf>,
    /// Refresh events during this session.
    #[serde(default)]
    pub refreshes: Vec<RefreshEvent>,
    /// Validation events during this session.
    #[serde(default)]
    pub validations: ValidationRecord,
}

/// Record of a refresh operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshEvent {
    pub at: DateTime<Utc>,
    pub files: Vec<PathBuf>,
}

/// Record of validations run during the session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationRecord {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_check: Option<ValidationEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh: Option<ValidationEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gate: Option<ValidationEvent>,
}

/// A single validation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationEvent {
    pub at: DateTime<Utc>,
    pub result: ValidationResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationResult {
    Passed,
    Failed,
    Incomplete,
}

impl SessionState {
    /// Create a new session state.
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            started_at: Utc::now(),
            db_path: None,
            repo_path: None,
            baseline_snapshot: None,
            files_edited: Vec::new(),
            refreshes: Vec::new(),
            validations: ValidationRecord::default(),
        }
    }

    /// Load session state from file.
    pub fn load(session_id: &str) -> Result<Option<Self>, String> {
        let path = session_file_path(session_id)?;

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read session file: {}", e))?;

        let state: Self = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse session file: {}", e))?;

        Ok(Some(state))
    }

    /// Save session state to file.
    pub fn save(&self) -> Result<PathBuf, String> {
        let path = session_file_path(&self.session_id)?;

        // Ensure sessions directory exists
        if let Some(parent) = path.parent() {
            paths::ensure_dir(&parent.to_path_buf())?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize session state: {}", e))?;

        fs::write(&path, content)
            .map_err(|e| format!("failed to write session file: {}", e))?;

        Ok(path)
    }

    /// Record a file as edited.
    pub fn record_edit(&mut self, file: PathBuf) {
        if !self.files_edited.contains(&file) {
            self.files_edited.push(file);
        }
    }

    /// Record multiple files as edited.
    pub fn record_edits(&mut self, files: &[PathBuf]) {
        for file in files {
            self.record_edit(file.clone());
        }
    }

    /// Record a refresh operation.
    #[allow(dead_code)] // Future use: post-edit refresh tracking
    pub fn record_refresh(&mut self, files: Vec<PathBuf>) {
        self.refreshes.push(RefreshEvent {
            at: Utc::now(),
            files,
        });
    }

    /// Record a trust check result.
    pub fn record_trust_check(&mut self, result: ValidationResult) {
        self.validations.trust_check = Some(ValidationEvent {
            at: Utc::now(),
            result,
        });
    }

    /// Record a refresh validation.
    #[allow(dead_code)] // Future use: refresh validation tracking
    pub fn record_refresh_validation(&mut self, result: ValidationResult) {
        self.validations.refresh = Some(ValidationEvent {
            at: Utc::now(),
            result,
        });
    }

    /// Record a gate check result.
    #[allow(dead_code)] // Future use: gate check tracking
    pub fn record_gate(&mut self, result: ValidationResult) {
        self.validations.gate = Some(ValidationEvent {
            at: Utc::now(),
            result,
        });
    }

    /// Check if required validations have been run.
    pub fn has_required_validations(&self) -> bool {
        self.validations.trust_check.is_some() && self.validations.refresh.is_some()
    }
}

/// Get the path to a session state file.
fn session_file_path(session_id: &str) -> Result<PathBuf, String> {
    let sessions_dir = paths::sessions_dir()
        .ok_or_else(|| "could not determine sessions directory".to_string())?;

    // Sanitize session_id to prevent path traversal
    let safe_id = session_id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();

    if safe_id.is_empty() {
        return Err("invalid session ID".to_string());
    }

    Ok(sessions_dir.join(format!("{}.json", safe_id)))
}

/// Load or create session state.
pub fn load_or_create_session(session_id: &str) -> Result<SessionState, String> {
    match SessionState::load(session_id)? {
        Some(state) => Ok(state),
        None => Ok(SessionState::new(session_id.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_new() {
        let state = SessionState::new("test-123".to_string());
        assert_eq!(state.session_id, "test-123");
        assert!(state.files_edited.is_empty());
    }

    #[test]
    fn session_state_record_edit() {
        let mut state = SessionState::new("test".to_string());
        state.record_edit(PathBuf::from("src/a.rs"));
        state.record_edit(PathBuf::from("src/b.rs"));
        state.record_edit(PathBuf::from("src/a.rs")); // Duplicate

        assert_eq!(state.files_edited.len(), 2);
    }

    #[test]
    fn session_file_path_sanitizes() {
        let path = session_file_path("test-123_abc").unwrap();
        assert!(path.to_string_lossy().contains("test-123_abc.json"));
    }

    #[test]
    fn session_file_path_rejects_traversal() {
        let path = session_file_path("../../../etc/passwd").unwrap();
        // Should strip path traversal characters
        assert!(!path.to_string_lossy().contains(".."));
    }
}
