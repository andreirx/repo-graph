//! Git operation errors.
//!
//! RS-MS-1: Error types for git CLI wrapper.

use std::fmt;
use std::io;

/// Error from git operations.
#[derive(Debug)]
pub enum GitError {
    /// Path is not a git repository.
    NotARepository(String),

    /// Git command failed with non-zero exit code.
    CommandFailed {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },

    /// Failed to spawn git process.
    SpawnFailed(io::Error),

    /// Git output could not be parsed.
    ParseError(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::NotARepository(path) => {
                write!(f, "not a git repository: {}", path)
            }
            GitError::CommandFailed {
                command,
                exit_code,
                stderr,
            } => {
                let code = exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                write!(
                    f,
                    "git command failed: {} (exit {}): {}",
                    command, code, stderr
                )
            }
            GitError::SpawnFailed(e) => {
                write!(f, "failed to spawn git: {}", e)
            }
            GitError::ParseError(msg) => {
                write!(f, "failed to parse git output: {}", msg)
            }
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GitError::SpawnFailed(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for GitError {
    fn from(e: io::Error) -> Self {
        GitError::SpawnFailed(e)
    }
}
