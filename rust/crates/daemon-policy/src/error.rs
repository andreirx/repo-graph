//! Typed errors for daemon coordination.

use thiserror::Error;

/// Errors that can occur during coordinator operations.
///
/// Note: With FIFO writer queuing, write conflicts are handled by queuing
/// rather than returning an error. The coordinator waits for its turn
/// rather than failing immediately.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoordinatorError {
    /// The operation timed out waiting for access.
    #[error("timeout waiting for {operation}")]
    Timeout { operation: String },

    /// The operation was cancelled before completion.
    #[error("operation cancelled")]
    Cancelled,

    /// The coordinator is in an invalid state for the requested operation.
    #[error("invalid state transition: cannot {operation} while {current_state}")]
    InvalidTransition {
        operation: String,
        current_state: String,
    },
}

impl CoordinatorError {
    /// Create a timeout error for the given operation.
    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::Timeout {
            operation: operation.into(),
        }
    }

    /// Create an invalid transition error.
    pub fn invalid_transition(
        operation: impl Into<String>,
        current_state: impl Into<String>,
    ) -> Self {
        Self::InvalidTransition {
            operation: operation.into(),
            current_state: current_state.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_timeout() {
        let err = CoordinatorError::timeout("write lock");
        assert_eq!(err.to_string(), "timeout waiting for write lock");
    }

    #[test]
    fn error_display_cancelled() {
        let err = CoordinatorError::Cancelled;
        assert_eq!(err.to_string(), "operation cancelled");
    }

    #[test]
    fn error_display_invalid_transition() {
        let err = CoordinatorError::invalid_transition("write", "Reading(3)");
        assert_eq!(
            err.to_string(),
            "invalid state transition: cannot write while Reading(3)"
        );
    }

    #[test]
    fn errors_are_clone_and_eq() {
        let err1 = CoordinatorError::timeout("test");
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }
}
