//! Freshness tracking enumeration and types.
//!
//! Defines how per-row freshness state is tracked.

use serde::{Deserialize, Serialize};

/// How freshness is tracked for an artifact family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessTracking {
    /// Per-row freshness state column.
    ///
    /// Each row has a `freshness_state` column that tracks whether
    /// the row is current, impacted, stale, or unknown.
    ///
    /// Applies to: Deterministic relationships, hints/inferences
    PerRow,

    /// Implicit from source file currency.
    ///
    /// Freshness is determined by whether the source file hash
    /// matches the current file. No explicit freshness column.
    ///
    /// Applies to: Layer 0 extracted facts
    ImplicitFromSource,

    /// No freshness tracking.
    ///
    /// The artifact is snapshot-independent or freshness is
    /// not applicable.
    ///
    /// Applies to: Governance overlays
    None,
}

impl FreshnessTracking {
    /// Returns true if this requires a freshness_state column.
    pub fn requires_column(&self) -> bool {
        matches!(self, Self::PerRow)
    }

    /// Returns true if freshness is implicit from source.
    pub fn is_implicit(&self) -> bool {
        matches!(self, Self::ImplicitFromSource)
    }
}

impl std::fmt::Display for FreshnessTracking {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PerRow => write!(f, "per_row"),
            Self::ImplicitFromSource => write!(f, "implicit_from_source"),
            Self::None => write!(f, "none"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Freshness state values
// ═══════════════════════════════════════════════════════════════════════════

/// Per-row freshness state.
///
/// Every artifact row that is not Layer 0 has a freshness state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    /// Row is computed from current Layer 0 state.
    ///
    /// The row's provenance anchors are all current, and the
    /// computation is up-to-date.
    Current,

    /// Upstream Layer 0 changed; row may be stale but is still useful.
    ///
    /// The row was computed from a previous state but has not been
    /// recomputed yet. It is still useful for orientation but may
    /// not reflect the current truth.
    Impacted,

    /// Row is known to be out of date; use with caution.
    ///
    /// Stronger than impacted: the row is definitively stale,
    /// not just potentially affected.
    Stale,

    /// Freshness cannot be determined.
    ///
    /// Used for legacy rows created before freshness tracking
    /// was added, or when provenance is missing.
    Unknown,
}

impl FreshnessState {
    /// Returns the database string value for this state.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Impacted => "impacted",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }

    /// Parse from database string value.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "current" => Some(Self::Current),
            "impacted" => Some(Self::Impacted),
            "stale" => Some(Self::Stale),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Returns true if this state indicates the row is usable.
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Current | Self::Impacted | Self::Unknown)
    }

    /// Returns true if this state indicates potential staleness.
    pub fn is_potentially_stale(&self) -> bool {
        matches!(self, Self::Impacted | Self::Stale | Self::Unknown)
    }
}

impl std::fmt::Display for FreshnessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Freshness filter for queries
// ═══════════════════════════════════════════════════════════════════════════

/// Filter for freshness-aware queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessFilter {
    /// Only rows with `freshness_state = 'current'`.
    CurrentOnly,

    /// Rows with `freshness_state IN ('current', 'impacted')`.
    ///
    /// This is the default for agent surfaces: impacted data
    /// is still useful for orientation.
    CurrentAndImpacted,

    /// All rows regardless of freshness.
    All,
}

impl FreshnessFilter {
    /// Returns the SQL WHERE clause for this filter.
    pub fn sql_clause(&self) -> &'static str {
        match self {
            Self::CurrentOnly => "freshness_state = 'current'",
            Self::CurrentAndImpacted => "freshness_state IN ('current', 'impacted')",
            Self::All => "1=1",
        }
    }

    /// Returns the states included by this filter.
    pub fn included_states(&self) -> &'static [FreshnessState] {
        match self {
            Self::CurrentOnly => &[FreshnessState::Current],
            Self::CurrentAndImpacted => &[FreshnessState::Current, FreshnessState::Impacted],
            Self::All => &[
                FreshnessState::Current,
                FreshnessState::Impacted,
                FreshnessState::Stale,
                FreshnessState::Unknown,
            ],
        }
    }
}

impl Default for FreshnessFilter {
    fn default() -> Self {
        // Agent surfaces default to including impacted data
        Self::CurrentAndImpacted
    }
}

impl std::fmt::Display for FreshnessFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentOnly => write!(f, "current_only"),
            Self::CurrentAndImpacted => write!(f, "current_and_impacted"),
            Self::All => write!(f, "all"),
        }
    }
}
