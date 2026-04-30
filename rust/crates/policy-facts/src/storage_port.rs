//! Storage port for policy-facts persistence.
//!
//! Defines the port traits for reading and writing policy-facts:
//! - STATUS_MAPPING (PF-1)
//! - BEHAVIORAL_MARKER (PF-2)
//! - RETURN_FATE (PF-3)
//!
//! The storage adapter crate implements these traits.
//!
//! This follows Clean Architecture: the domain crate (policy-facts)
//! defines the port, the adapter crate (storage) implements it.
//! Dependency direction: adapter → domain (outer → inner).

use crate::types::{BehavioralMarker, FateKind, MarkerKind, ReturnFate, StatusMapping};

/// Error type for policy-facts storage operations.
///
/// This is a domain-level error, not tied to any specific storage backend.
/// The storage adapter maps its internal errors to this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyFactsStorageError {
    /// Database operation failed.
    DatabaseError(String),

    /// JSON serialization/deserialization failed.
    JsonError(String),

    /// Snapshot not found.
    SnapshotNotFound(String),

    /// Constraint violation (e.g., duplicate symbol_key).
    ConstraintViolation(String),
}

impl std::fmt::Display for PolicyFactsStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyFactsStorageError::DatabaseError(msg) => {
                write!(f, "database error: {}", msg)
            }
            PolicyFactsStorageError::JsonError(msg) => {
                write!(f, "JSON error: {}", msg)
            }
            PolicyFactsStorageError::SnapshotNotFound(uid) => {
                write!(f, "snapshot not found: {}", uid)
            }
            PolicyFactsStorageError::ConstraintViolation(msg) => {
                write!(f, "constraint violation: {}", msg)
            }
        }
    }
}

impl std::error::Error for PolicyFactsStorageError {}

/// Port trait for writing policy-facts to storage.
///
/// Implemented by the storage adapter. The indexer calls this during
/// C file extraction to persist extracted facts.
pub trait PolicyFactsStorageWrite {
    /// Insert STATUS_MAPPING facts for a snapshot.
    ///
    /// Replaces any existing STATUS_MAPPING facts for the same snapshot.
    /// Returns the count of facts inserted.
    ///
    /// # Arguments
    /// * `snapshot_uid` - Snapshot identity
    /// * `mappings` - Extracted StatusMapping facts
    ///
    /// # Errors
    /// Returns error if the snapshot doesn't exist or database operation fails.
    fn insert_status_mappings(
        &mut self,
        snapshot_uid: &str,
        mappings: &[StatusMapping],
    ) -> Result<usize, PolicyFactsStorageError>;

    /// Insert BEHAVIORAL_MARKER facts for a snapshot.
    ///
    /// Replaces any existing BEHAVIORAL_MARKER facts for the same snapshot.
    /// Returns the count of facts inserted.
    ///
    /// # Arguments
    /// * `snapshot_uid` - Snapshot identity
    /// * `markers` - Extracted BehavioralMarker facts
    ///
    /// # Errors
    /// Returns error if the snapshot doesn't exist or database operation fails.
    fn insert_behavioral_markers(
        &mut self,
        snapshot_uid: &str,
        markers: &[BehavioralMarker],
    ) -> Result<usize, PolicyFactsStorageError>;

    /// Insert RETURN_FATE facts for a snapshot.
    ///
    /// Replaces any existing RETURN_FATE facts for the same snapshot.
    /// Returns the count of facts inserted.
    ///
    /// # Arguments
    /// * `snapshot_uid` - Snapshot identity
    /// * `fates` - Extracted ReturnFate facts
    ///
    /// # Errors
    /// Returns error if the snapshot doesn't exist or database operation fails.
    fn insert_return_fates(
        &mut self,
        snapshot_uid: &str,
        fates: &[ReturnFate],
    ) -> Result<usize, PolicyFactsStorageError>;
}

/// Port trait for reading policy-facts from storage.
///
/// Implemented by the storage adapter. The CLI and agent use this to
/// query persisted facts.
pub trait PolicyFactsStorageRead {
    /// Query STATUS_MAPPING facts for a snapshot.
    ///
    /// # Arguments
    /// * `snapshot_uid` - Snapshot identity
    /// * `file_filter` - Optional file path prefix filter
    ///
    /// # Returns
    /// Vector of StatusMapping facts, sorted by file_path then function_name.
    ///
    /// # Errors
    /// Returns error if database operation fails.
    fn query_status_mappings(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
    ) -> Result<Vec<StatusMapping>, PolicyFactsStorageError>;

    /// Count STATUS_MAPPING facts for a snapshot.
    ///
    /// More efficient than query + len when only count is needed.
    fn count_status_mappings(
        &self,
        snapshot_uid: &str,
    ) -> Result<usize, PolicyFactsStorageError>;

    /// Query BEHAVIORAL_MARKER facts for a snapshot.
    ///
    /// # Arguments
    /// * `snapshot_uid` - Snapshot identity
    /// * `file_filter` - Optional file path prefix filter
    /// * `kind_filter` - Optional marker kind filter
    ///
    /// # Returns
    /// Vector of BehavioralMarker facts, sorted by file_path then line_start.
    ///
    /// # Errors
    /// Returns error if database operation fails.
    fn query_behavioral_markers(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
        kind_filter: Option<MarkerKind>,
    ) -> Result<Vec<BehavioralMarker>, PolicyFactsStorageError>;

    /// Count BEHAVIORAL_MARKER facts for a snapshot.
    ///
    /// More efficient than query + len when only count is needed.
    fn count_behavioral_markers(
        &self,
        snapshot_uid: &str,
    ) -> Result<usize, PolicyFactsStorageError>;

    /// Query RETURN_FATE facts for a snapshot.
    ///
    /// # Arguments
    /// * `snapshot_uid` - Snapshot identity
    /// * `file_filter` - Optional file path prefix filter
    /// * `callee_filter` - Optional callee name filter
    /// * `fate_filter` - Optional fate kind filter
    ///
    /// # Returns
    /// Vector of ReturnFate facts, sorted by file_path then line.
    ///
    /// # Errors
    /// Returns error if database operation fails.
    fn query_return_fates(
        &self,
        snapshot_uid: &str,
        file_filter: Option<&str>,
        callee_filter: Option<&str>,
        fate_filter: Option<FateKind>,
    ) -> Result<Vec<ReturnFate>, PolicyFactsStorageError>;

    /// Count RETURN_FATE facts for a snapshot.
    ///
    /// More efficient than query + len when only count is needed.
    fn count_return_fates(
        &self,
        snapshot_uid: &str,
    ) -> Result<usize, PolicyFactsStorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = PolicyFactsStorageError::DatabaseError("connection failed".to_string());
        assert_eq!(format!("{}", err), "database error: connection failed");

        let err = PolicyFactsStorageError::SnapshotNotFound("snap-123".to_string());
        assert_eq!(format!("{}", err), "snapshot not found: snap-123");
    }
}
