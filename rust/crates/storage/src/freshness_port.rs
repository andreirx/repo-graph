//! Freshness and provenance storage port (ACR-3).
//!
//! This module defines the storage port trait for per-row freshness state
//! and provenance tracking. These are core primitives for ACR-4 impact
//! propagation.
//!
//! # Architecture
//!
//! - `artifact-contracts` defines the type model (`FreshnessState`, `Provenance`)
//! - This port defines the storage operations
//! - `freshness_impl.rs` provides the `StorageConnection` implementation
//!
//! # Semantic Model
//!
//! Every Layer 2+ artifact row has:
//! - `freshness_state`: current | impacted | stale | unknown
//! - `freshness_updated_at`: ISO 8601 timestamp of last state change
//! - `provenance_json`: canonical JSON encoding Layer 0 dependencies
//!
//! Layer 0-1 rows have implicit freshness from source file currency.
//!
//! # Usage
//!
//! ```rust,ignore
//! use artifact_contracts::{FreshnessState, FreshnessFilter, Provenance, ProvenanceAnchor};
//! use repo_graph_storage::FreshnessStoragePort;
//!
//! // Mark rows impacted when their provenance anchors change
//! storage.mark_impacted_by_stable_keys(
//!     "snapshot-uid",
//!     "inferences",
//!     &["repo:file.ts#func:SYMBOL:FUNCTION"],
//! )?;
//!
//! // Query rows by freshness state
//! let impacted = storage.count_by_freshness(
//!     "snapshot-uid",
//!     "inferences",
//!     FreshnessState::Impacted,
//! )?;
//! ```
//!
//! # References
//!
//! - `docs/slices/acr-3-provenance-and-freshness-schema.md`
//! - `docs/architecture/artifact-contract-model.md`
//! - `rust/crates/artifact-contracts/src/freshness.rs`
//! - `rust/crates/artifact-contracts/src/provenance.rs`

use artifact_contracts::{FreshnessFilter, FreshnessState, Provenance};

use crate::error::StorageError;

// ═══════════════════════════════════════════════════════════════════════════
// Freshness-aware input types
// ═══════════════════════════════════════════════════════════════════════════

/// Row reference for freshness operations.
///
/// Identifies a row in a freshness-tracked table by its UID.
#[derive(Debug, Clone)]
pub struct RowRef {
    /// The table name (e.g., "inferences", "boundary_contracts").
    pub table: String,
    /// The row's primary key UID.
    pub row_uid: String,
}

/// Summary of freshness state counts for a table.
#[derive(Debug, Clone, Default)]
pub struct FreshnessSummary {
    /// Rows with freshness_state = 'current'.
    pub current: usize,
    /// Rows with freshness_state = 'impacted'.
    pub impacted: usize,
    /// Rows with freshness_state = 'stale'.
    pub stale: usize,
    /// Rows with freshness_state = 'unknown'.
    pub unknown: usize,
}

impl FreshnessSummary {
    /// Total row count across all states.
    pub fn total(&self) -> usize {
        self.current + self.impacted + self.stale + self.unknown
    }

    /// Percentage of rows that are current.
    pub fn current_percentage(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            (self.current as f64 / total as f64) * 100.0
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Port trait
// ═══════════════════════════════════════════════════════════════════════════

/// Port trait for freshness and provenance storage operations.
///
/// Implemented by `StorageConnection`. Used by:
/// - Extractors/detectors to persist provenance with new rows
/// - Refresh pipeline to mark rows impacted
/// - Query surfaces to filter by freshness
pub trait FreshnessStoragePort {
    // ── Write operations ────────────────────────────────────────────────

    /// Update the freshness state of a single row.
    ///
    /// Sets `freshness_state` and `freshness_updated_at` to current timestamp.
    fn update_freshness_state(
        &mut self,
        table: &str,
        row_uid: &str,
        state: FreshnessState,
    ) -> Result<bool, StorageError>;

    /// Mark multiple rows as impacted.
    ///
    /// Sets `freshness_state = 'impacted'` and updates timestamp.
    /// Returns count of rows actually updated.
    fn mark_rows_impacted(&mut self, table: &str, row_uids: &[&str])
        -> Result<usize, StorageError>;

    /// Mark rows impacted by provenance dependency.
    ///
    /// Finds all rows in `table` whose `provenance_json` contains any of
    /// the given stable keys in their `depends_on` array, and marks them
    /// as impacted.
    ///
    /// This is the core ACR-4 impact propagation primitive.
    fn mark_impacted_by_stable_keys(
        &mut self,
        snapshot_uid: &str,
        table: &str,
        changed_stable_keys: &[&str],
    ) -> Result<usize, StorageError>;

    /// Set provenance for a row.
    ///
    /// Updates `provenance_json` column. Does not change freshness state.
    fn set_provenance(
        &mut self,
        table: &str,
        row_uid: &str,
        provenance: &Provenance,
    ) -> Result<bool, StorageError>;

    /// Mark all rows in a table for a snapshot as current.
    ///
    /// Used after successful refresh to reset freshness state.
    fn mark_all_current(&mut self, snapshot_uid: &str, table: &str) -> Result<usize, StorageError>;

    // ── Read operations ─────────────────────────────────────────────────

    /// Get the freshness state of a single row.
    fn get_freshness_state(
        &self,
        table: &str,
        row_uid: &str,
    ) -> Result<Option<FreshnessState>, StorageError>;

    /// Get provenance for a single row.
    fn get_provenance(
        &self,
        table: &str,
        row_uid: &str,
    ) -> Result<Option<Provenance>, StorageError>;

    /// Count rows by freshness state.
    fn count_by_freshness(
        &self,
        snapshot_uid: &str,
        table: &str,
        state: FreshnessState,
    ) -> Result<usize, StorageError>;

    /// Get freshness summary for a table.
    fn freshness_summary(
        &self,
        snapshot_uid: &str,
        table: &str,
    ) -> Result<FreshnessSummary, StorageError>;

    /// List row UIDs by freshness state.
    ///
    /// Returns UIDs of rows matching the given state, up to `limit`.
    fn list_rows_by_freshness(
        &self,
        snapshot_uid: &str,
        table: &str,
        state: FreshnessState,
        limit: usize,
    ) -> Result<Vec<String>, StorageError>;

    /// Check if a row's provenance depends on a stable key.
    fn provenance_depends_on(
        &self,
        table: &str,
        row_uid: &str,
        stable_key: &str,
    ) -> Result<bool, StorageError>;
}

// ═══════════════════════════════════════════════════════════════════════════
// Freshness-filtered query support
// ═══════════════════════════════════════════════════════════════════════════

/// Extension trait for freshness-aware queries.
///
/// Provides helper methods for building SQL queries that respect
/// freshness filters.
pub trait FreshnessQueryExt {
    /// Generate the SQL WHERE clause for a freshness filter.
    ///
    /// Returns a clause like `freshness_state = 'current'` or
    /// `freshness_state IN ('current', 'impacted')`.
    fn freshness_where_clause(filter: FreshnessFilter) -> &'static str {
        filter.sql_clause()
    }

    /// Check if a freshness state passes a filter.
    fn state_passes_filter(state: FreshnessState, filter: FreshnessFilter) -> bool {
        filter.included_states().contains(&state)
    }
}

// Blanket implementation for any type
impl<T> FreshnessQueryExt for T {}

// ═══════════════════════════════════════════════════════════════════════════
// Table constants
// ═══════════════════════════════════════════════════════════════════════════

/// Tables that support freshness tracking (have the freshness columns).
///
/// These are the Layer 2+ artifact tables per the artifact contract model.
pub const FRESHNESS_TRACKED_TABLES: &[&str] = &[
    // Layer 2: Deterministic Relationships
    "boundary_contracts",
    "boundary_interaction_links",
    // Layer 3: Hints/Inferences
    "inferences",
    "project_surfaces",
    "project_surface_evidence",
    "surface_entrypoints",
    "surface_config_roots",
    "surface_env_dependencies",
    "surface_env_evidence",
    "surface_fs_mutations",
    "surface_fs_mutation_evidence",
    "module_candidates",
];

/// Check if a table supports freshness tracking.
pub fn is_freshness_tracked(table: &str) -> bool {
    FRESHNESS_TRACKED_TABLES.contains(&table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_summary_total() {
        let summary = FreshnessSummary {
            current: 10,
            impacted: 5,
            stale: 2,
            unknown: 3,
        };
        assert_eq!(summary.total(), 20);
    }

    #[test]
    fn freshness_summary_percentage() {
        let summary = FreshnessSummary {
            current: 80,
            impacted: 10,
            stale: 5,
            unknown: 5,
        };
        assert!((summary.current_percentage() - 80.0).abs() < 0.01);
    }

    #[test]
    fn freshness_summary_percentage_empty() {
        let summary = FreshnessSummary::default();
        assert_eq!(summary.current_percentage(), 0.0);
    }

    #[test]
    fn is_freshness_tracked_true() {
        assert!(is_freshness_tracked("inferences"));
        assert!(is_freshness_tracked("boundary_contracts"));
        assert!(is_freshness_tracked("module_candidates"));
    }

    #[test]
    fn is_freshness_tracked_false() {
        assert!(!is_freshness_tracked("nodes"));
        assert!(!is_freshness_tracked("edges"));
        assert!(!is_freshness_tracked("file_versions"));
    }

    #[test]
    fn state_passes_filter_current_only() {
        use FreshnessFilter::CurrentOnly;
        assert!(FreshnessSummary::state_passes_filter(
            FreshnessState::Current,
            CurrentOnly
        ));
        assert!(!FreshnessSummary::state_passes_filter(
            FreshnessState::Impacted,
            CurrentOnly
        ));
        assert!(!FreshnessSummary::state_passes_filter(
            FreshnessState::Stale,
            CurrentOnly
        ));
        assert!(!FreshnessSummary::state_passes_filter(
            FreshnessState::Unknown,
            CurrentOnly
        ));
    }

    #[test]
    fn state_passes_filter_current_and_impacted() {
        use FreshnessFilter::CurrentAndImpacted;
        assert!(FreshnessSummary::state_passes_filter(
            FreshnessState::Current,
            CurrentAndImpacted
        ));
        assert!(FreshnessSummary::state_passes_filter(
            FreshnessState::Impacted,
            CurrentAndImpacted
        ));
        assert!(!FreshnessSummary::state_passes_filter(
            FreshnessState::Stale,
            CurrentAndImpacted
        ));
        assert!(!FreshnessSummary::state_passes_filter(
            FreshnessState::Unknown,
            CurrentAndImpacted
        ));
    }

    #[test]
    fn state_passes_filter_all() {
        use FreshnessFilter::All;
        assert!(FreshnessSummary::state_passes_filter(
            FreshnessState::Current,
            All
        ));
        assert!(FreshnessSummary::state_passes_filter(
            FreshnessState::Impacted,
            All
        ));
        assert!(FreshnessSummary::state_passes_filter(
            FreshnessState::Stale,
            All
        ));
        assert!(FreshnessSummary::state_passes_filter(
            FreshnessState::Unknown,
            All
        ));
    }
}
