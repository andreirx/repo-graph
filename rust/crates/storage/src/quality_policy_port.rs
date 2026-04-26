//! Quality policy storage port.
//!
//! This module defines the storage port trait for quality policy
//! assessment orchestration. Unlike the typical Clean Architecture
//! pattern where the domain crate defines the port, this port lives
//! in storage because:
//!
//! 1. The quality-policy crate already depends on storage for DTOs
//! 2. The port's input/output types are storage-native
//! 3. Avoids crate proliferation for a narrow interface
//!
//! The runner crate depends on storage and uses this trait.

use crate::error::StorageError;
use crate::types::{QualityAssessmentInput, QualityPolicyPayload};

/// A measurement row enriched with scope metadata for policy evaluation.
///
/// This DTO carries the data needed for scope matching in the pure
/// assessment engine. The storage layer produces these by joining
/// measurements with nodes and files tables.
#[derive(Debug, Clone, PartialEq)]
pub struct EnrichedMeasurement {
    /// Stable key of the measurement target.
    pub target_stable_key: String,

    /// Measurement kind (e.g., "cyclomatic_complexity", "function_length").
    pub measurement_kind: String,

    /// Numeric measurement value.
    ///
    /// Extracted from the `value_json` field's `value` property.
    pub value: f64,

    /// Repo-relative file path, if the measurement target is file-scoped.
    ///
    /// For SYMBOL nodes: derived from the node's file_uid join.
    /// For FILE nodes: the file path directly.
    /// For MODULE or REPO nodes: `None` (not file-scoped).
    pub file_path: Option<String>,

    /// Symbol kind for SYMBOL-type nodes (e.g., "FUNCTION", "CLASS").
    ///
    /// `None` for non-symbol measurements (FILE, MODULE, REPO targets).
    pub symbol_kind: Option<String>,
}

/// Loaded policy with its storage identity.
#[derive(Debug, Clone)]
pub struct LoadedPolicy {
    /// Storage-assigned declaration UID.
    pub policy_uid: String,

    /// Parsed and validated policy payload.
    pub payload: QualityPolicyPayload,
}

/// Port trait for quality policy storage operations.
///
/// Implemented by `StorageConnection`. The runner uses this trait
/// to load policies, measurements, and persist assessments.
pub trait QualityPolicyStoragePort {
    /// Load all active quality policy declarations for a repository.
    ///
    /// Returns parsed `QualityPolicyPayload` values with their UIDs.
    /// Malformed JSON in storage produces a storage error.
    fn load_active_quality_policies(
        &self,
        repo_uid: &str,
    ) -> Result<Vec<LoadedPolicy>, StorageError>;

    /// Load enriched measurements for a snapshot, filtered by kinds.
    ///
    /// The `kinds` parameter lists which measurement kinds to load
    /// (e.g., `["cyclomatic_complexity", "function_length"]`).
    ///
    /// Returns measurements enriched with scope metadata (file_path,
    /// symbol_kind) from node/file joins.
    fn load_enriched_measurements(
        &self,
        snapshot_uid: &str,
        kinds: &[&str],
    ) -> Result<Vec<EnrichedMeasurement>, StorageError>;

    /// Atomically replace all assessments for a snapshot.
    ///
    /// Deletes existing assessments for the snapshot, then inserts
    /// the new set. Returns the count of assessments inserted.
    fn replace_assessments(
        &mut self,
        snapshot_uid: &str,
        assessments: &[QualityAssessmentInput],
    ) -> Result<usize, StorageError>;
}
