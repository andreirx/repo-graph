//! Retention types and constants.
//!
//! This module defines the core types for snapshot retention management:
//! - `CURRENT_CACHE_EPOCH`: Version marker for derived cache validity
//! - `RetentionClass`: Classification determining snapshot lifecycle
//! - `RetentionStats`: Aggregate statistics for retention reporting

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Current derived cache epoch.
///
/// Bump major on breaking extractor changes requiring full re-extraction.
/// Bump minor on compatible changes (new optional data).
pub const CURRENT_CACHE_EPOCH: &str = "1.0";

/// Retention class for a snapshot.
///
/// Determines whether a snapshot is protected from pruning.
/// The classification hierarchy is:
/// - `Current`: Active snapshot (always retained)
/// - `Parent`: Parent of current (retained for incremental refresh)
/// - `BaselineAuto`: Auto-selected comparison baseline
/// - `BaselineUser`: User-marked baseline, **row-retaining** (graph-family rows
///   pinned in full — the pre-M-7 behavior, now the explicit opt-in)
/// - `BaselineStamp`: User-marked baseline, **stamp-only** (EC-1 M-7 / D-EC-8-D
///   default): the `snapshots` row (comparability/toolchain/epoch identity),
///   the FC4 measurement/assessment rows, and the Tier-A `declarations` rows
///   are retained; graph-family rows are NOT promised and are removed by the
///   retention pass once the snapshot leaves the serving pair (see
///   `retention/narrow.rs`)
/// - `Prunable`: Eligible for deletion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionClass {
    /// Active snapshot for this repo (always retained)
    Current,
    /// Parent of current snapshot (retained for incremental refresh)
    Parent,
    /// Automatically selected comparison baseline
    BaselineAuto,
    /// Explicitly marked by user as a row-retaining baseline (graph rows pinned)
    BaselineUser,
    /// Explicitly marked by user as a provenance stamp (graph rows not retained)
    BaselineStamp,
    /// Eligible for pruning
    Prunable,
}

impl RetentionClass {
    /// Convert to string for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            RetentionClass::Current => "current",
            RetentionClass::Parent => "parent",
            RetentionClass::BaselineAuto => "baseline_auto",
            RetentionClass::BaselineUser => "baseline_user",
            RetentionClass::BaselineStamp => "baseline_stamp",
            RetentionClass::Prunable => "prunable",
        }
    }

    /// Parse from storage string (convenience wrapper).
    pub fn parse(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl FromStr for RetentionClass {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "current" => Ok(RetentionClass::Current),
            "parent" => Ok(RetentionClass::Parent),
            "baseline_auto" => Ok(RetentionClass::BaselineAuto),
            "baseline_user" => Ok(RetentionClass::BaselineUser),
            "baseline_stamp" => Ok(RetentionClass::BaselineStamp),
            "prunable" => Ok(RetentionClass::Prunable),
            _ => Err(()),
        }
    }
}

impl RetentionClass {
    /// Returns true if this snapshot is protected from pruning.
    pub fn is_protected(&self) -> bool {
        !matches!(self, RetentionClass::Prunable)
    }
}

/// Retention statistics for a repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionStats {
    /// Total snapshot count
    pub total: i64,
    /// Current snapshots
    pub current: i64,
    /// Parent snapshots
    pub parent: i64,
    /// Auto-baseline snapshots
    pub baseline_auto: i64,
    /// Row-retaining user-baseline snapshots (graph rows pinned)
    pub baseline_user: i64,
    /// Stamp-only user-baseline snapshots (EC-1 M-7: stamp + measurements retained)
    pub baseline_stamp: i64,
    /// Prunable snapshots
    pub prunable: i64,
    /// Snapshots with NULL retention_class (pre-classification)
    pub unclassified: i64,
    /// Snapshots with stale epoch
    pub stale_epoch: i64,
}
