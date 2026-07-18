//! CACHE-SEMANTICS-1: Snapshot retention management.
//!
//! This module provides types and storage methods for managing snapshot
//! retention according to the cache/authority semantic model.
//!
//! # Retention Classes
//!
//! Each snapshot is assigned a retention class that determines its lifecycle:
//!
//! - `current`: Active snapshot for this repo (always retained)
//! - `parent`: Parent of current snapshot (retained for incremental refresh)
//! - `baseline_auto`: Automatically selected comparison baseline
//! - `baseline_user`: Explicitly marked by user as baseline (graph rows retained)
//! - `baseline_stamp`: Explicitly marked by user as baseline, stamp-only (graph
//!   rows narrowed once the mark leaves the serving pair; measurements retained)
//! - `prunable`: Eligible for pruning
//!
//! # Derived Cache Epoch
//!
//! Snapshots carry a `derived_cache_epoch` that indicates the validity of
//! their Tier B (derived cache) data. When the extractor version changes,
//! snapshots with mismatched epochs are considered stale and can be marked
//! prunable.
//!
//! # Whole-Snapshot Invalidation
//!
//! The semantic contract is whole-snapshot invalidation: if the epoch
//! mismatches, the entire snapshot's derived cache is stale. There are
//! no half-valid snapshot states.
//!
//! # Module Organization
//!
//! - `types`: Core types (RetentionClass, RetentionStats, CURRENT_CACHE_EPOCH)
//! - `classify`: Classification logic (assign retention classes based on relationships)
//! - `prune`: Pruning logic (transactional deletion of prunable snapshots)
//! - `narrow`: EC-M7 baseline-stamp narrowing (delete a stamp mark's graph
//!   families, keep the `snapshots` row + FC4 measurements) + per-mark cost
//!   measurement
//!
//! # References
//!
//! - `docs/slices/cache-semantics-1.md`
//! - `docs/slices/retention-policy-1.md`
//! - `docs/slices/ec-m7-baseline-stamp-1.md`
//! - `agent_docs/storage-architecture-v2.md`

mod classify;
mod narrow;
mod prune;
mod types;

pub use narrow::{
    CascadeChild, FamilyRows, NarrowedBaseline, SnapshotFamilyCost, STAMP_KEEP_AUTHORITY_TABLES,
    STAMP_KEEP_MEASUREMENT_TABLES, STAMP_NARROW_CASCADE_CHILDREN, STAMP_NARROW_TABLES,
};
pub use types::{RetentionClass, RetentionStats, CURRENT_CACHE_EPOCH};

#[cfg(test)]
mod tests;
