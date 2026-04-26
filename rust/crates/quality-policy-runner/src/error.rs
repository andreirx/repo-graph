//! Error types for the quality policy runner.
//!
//! Errors are structured to provide clear diagnostics at the use-case
//! boundary. Invalid policies fail loudly rather than being silently
//! skipped.

use repo_graph_quality_policy::PolicyValidationError;
use repo_graph_storage::error::StorageError;

/// Errors that can occur during quality policy assessment orchestration.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    /// Storage operation failed.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    /// A policy payload failed semantic validation.
    ///
    /// This is a loud failure — invalid policies are not silently skipped.
    /// The `policy_uid` identifies which declaration is malformed.
    #[error("invalid policy {policy_uid}: {source}")]
    InvalidPolicy {
        policy_uid: String,
        #[source]
        source: PolicyValidationError,
    },

    /// Baseline snapshot required but not provided.
    ///
    /// At least one policy requires comparative evaluation but no
    /// baseline snapshot UID was supplied.
    #[error("baseline snapshot required: {0} policies require baseline")]
    BaselineRequired(usize),
}
